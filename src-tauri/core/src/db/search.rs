//! Full-text search over verse text, using the `verse_search` FTS5 index
//! (see `migrate.rs`). The index is derived from `verses` and only ever
//! read from it: building it never writes to `verses`, and search results
//! carry each verse's text as stored in `verses`, not anything from the
//! index.
//!
//! What's indexed is each verse's plain text as the app shows it (see
//! `domain::kjv_markup::VerseText`): in the KJV, supplied words without
//! their braces and no margin notes, so a note's words don't find the
//! verse. Every other translation's text is indexed as stored.
//!
//! Each indexed row's rowid encodes the verse's coordinates —
//! `translation * 10^8 + book * 10^6 + chapter * 10^3 + verse` — so one
//! translation is a contiguous rowid range, and rowid order is canonical
//! Bible order.

use anyhow::{Context, bail};
use sqlx::{FromRow, SqlitePool};

use crate::db::{Db, WritePool};
use crate::domain::kjv_markup::{KJV, VerseText};
use crate::domain::query::SearchQuery;

/// What `verse_search` holds, recorded in `search_index_version`. A
/// database with any other version (or none: 2.9.1 and older indexed the
/// KJV's stored text, braces and notes included) has its whole index
/// rebuilt once. Change it whenever what's indexed changes.
pub const SEARCH_CONTENT_VERSION: i64 = 1;

const TRANSLATION_STRIDE: i64 = 100_000_000;
const BOOK_STRIDE: i64 = 1_000_000;
const CHAPTER_STRIDE: i64 = 1_000;

fn translation_rowids(translation_id: i64) -> (i64, i64) {
    (
        translation_id * TRANSLATION_STRIDE,
        (translation_id + 1) * TRANSLATION_STRIDE,
    )
}

/// A matching verse, with its text as stored in `verses`.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct SearchHit {
    pub book_id: i64,
    pub chapter: i64,
    pub verse: i64,
    pub text: String,
}

/// Index every translation that isn't indexed yet, or whose verse count no
/// longer matches what was indexed (`seed_missing` also clears the record
/// for anything it seeds), or all of them if the index holds an older
/// kind of content (see `SEARCH_CONTENT_VERSION`). Each translation is
/// indexed in its own transaction, so the database stays usable in
/// between. Returns the ids of the translations it (re)indexed — none at
/// all on a normal launch.
pub async fn ensure_search_index(db: &Db) -> anyhow::Result<Vec<i64>> {
    forget_outdated_index(db).await?;

    let translations: Vec<i64> = sqlx::query_scalar("SELECT id FROM translations ORDER BY id")
        .fetch_all(db.read())
        .await
        .context("Failed to list translations to index")?;

    let mut indexed = Vec::new();
    for translation_id in translations {
        let verse_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM verses WHERE translation_id = $1")
                .bind(translation_id)
                .fetch_one(db.read())
                .await
                .context("Failed to count verses to index")?;
        let indexed_count: Option<i64> = sqlx::query_scalar(
            "SELECT verse_count FROM search_index_state WHERE translation_id = $1",
        )
        .bind(translation_id)
        .fetch_optional(db.read())
        .await
        .context("Failed to read search index state")?;
        if indexed_count == Some(verse_count) {
            continue;
        }
        index_translation(db, translation_id, verse_count).await?;
        indexed.push(translation_id);
    }
    Ok(indexed)
}

/// If the index holds an older kind of content, forget what it covers, so
/// every translation is indexed again, and record the current version.
/// Both happen in one transaction: if the app quits part-way through the
/// rebuild, the translations already done stay done, and the rest are
/// indexed on the next launch.
async fn forget_outdated_index(db: &Db) -> anyhow::Result<()> {
    let version: Option<i64> =
        sqlx::query_scalar("SELECT version FROM search_index_version WHERE id = 1")
            .fetch_optional(db.read())
            .await
            .context("Failed to read search index version")?;
    if version == Some(SEARCH_CONTENT_VERSION) {
        return Ok(());
    }
    let mut tx = crate::db::begin_write(db.write())
        .await
        .context("Failed to start search index version transaction")?;
    sqlx::query("DELETE FROM search_index_state")
        .execute(&mut *tx)
        .await
        .context("Failed to reset search index state")?;
    sqlx::query(
        "INSERT INTO search_index_version (id, version) VALUES (1, $1) \
         ON CONFLICT (id) DO UPDATE SET version = excluded.version",
    )
    .bind(SEARCH_CONTENT_VERSION)
    .execute(&mut *tx)
    .await
    .context("Failed to record search index version")?;
    tx.commit()
        .await
        .context("Failed to commit search index version")?;
    tracing::info!(
        "Search index content version {version:?} is outdated; rebuilding it as version \
         {SEARCH_CONTENT_VERSION}"
    );
    Ok(())
}

/// (Re)index one translation. It's done a book at a time, each in its own
/// short transaction, so a write of the reader's own waits its turn behind
/// one book at most (reads don't wait at all); what's indexed is read on
/// the read pool, between the writes. The translation only counts as
/// indexed once the last book is in; if the app quits part-way, the next
/// launch starts that translation over.
async fn index_translation(db: &Db, translation_id: i64, verse_count: i64) -> anyhow::Result<()> {
    // The rowid encoding needs every coordinate to fit its field.
    let (max_book, max_chapter, max_verse): (Option<i64>, Option<i64>, Option<i64>) =
        sqlx::query_as(
            "SELECT MAX(book_id), MAX(chapter), MAX(verse) FROM verses WHERE translation_id = $1",
        )
        .bind(translation_id)
        .fetch_one(db.read())
        .await
        .context("Failed to check verse coordinates")?;
    if translation_id < 1
        || max_book.unwrap_or(0) >= TRANSLATION_STRIDE / BOOK_STRIDE
        || max_chapter.unwrap_or(0) >= BOOK_STRIDE / CHAPTER_STRIDE
        || max_verse.unwrap_or(0) >= CHAPTER_STRIDE
    {
        bail!("translation {translation_id} has coordinates too large to index for search");
    }

    let mut tx = crate::db::begin_write(db.write())
        .await
        .context("Failed to start search index transaction")?;
    sqlx::query("DELETE FROM search_index_state WHERE translation_id = $1")
        .bind(translation_id)
        .execute(&mut *tx)
        .await
        .context("Failed to reset search index state")?;
    let (lo, hi) = translation_rowids(translation_id);
    sqlx::query("DELETE FROM verse_search WHERE rowid >= $1 AND rowid < $2")
        .bind(lo)
        .bind(hi)
        .execute(&mut *tx)
        .await
        .context("Failed to clear stale search index rows")?;
    tx.commit()
        .await
        .context("Failed to commit search index reset")?;

    let code: String = sqlx::query_scalar("SELECT code FROM translations WHERE id = $1")
        .bind(translation_id)
        .fetch_one(db.read())
        .await
        .context("Failed to read the translation to index")?;
    let books: Vec<i64> = sqlx::query_scalar(
        "SELECT DISTINCT book_id FROM verses WHERE translation_id = $1 ORDER BY book_id",
    )
    .bind(translation_id)
    .fetch_all(db.read())
    .await
    .context("Failed to list books to index")?;
    for book_id in books {
        if code == KJV {
            index_book_as_shown(db, translation_id, book_id).await
        } else {
            index_book_as_stored(db.write(), translation_id, book_id).await
        }
        .with_context(|| format!("Failed to index book {book_id} for search"))?;
    }

    sqlx::query(
        "INSERT INTO search_index_state (translation_id, verse_count) VALUES ($1, $2) \
         ON CONFLICT (translation_id) DO UPDATE SET verse_count = excluded.verse_count",
    )
    .bind(translation_id)
    .bind(verse_count)
    .execute(db.write().pool())
    .await
    .context("Failed to record search index state")?;

    tracing::info!("Indexed translation {translation_id} for search ({verse_count} verses)");
    Ok(())
}

/// Index one book's stored text, as is, in one transaction that starts
/// with the write lock (see `begin_write`: a write into the full-text table
/// reads before it writes).
async fn index_book_as_stored(
    writer: &WritePool,
    translation_id: i64,
    book_id: i64,
) -> anyhow::Result<()> {
    let mut tx = crate::db::begin_write(writer).await?;
    sqlx::query(
        "INSERT INTO verse_search (rowid, text) \
         SELECT translation_id * $3 + book_id * $4 + chapter * $5 + verse, text \
         FROM verses WHERE translation_id = $1 AND book_id = $2",
    )
    .bind(translation_id)
    .bind(book_id)
    .bind(TRANSLATION_STRIDE)
    .bind(BOOK_STRIDE)
    .bind(CHAPTER_STRIDE)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Index one KJV book's plain text as the app shows it (see `VerseText`),
/// in one transaction that starts with the write lock.
async fn index_book_as_shown(db: &Db, translation_id: i64, book_id: i64) -> anyhow::Result<()> {
    let verses: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT chapter, verse, text FROM verses \
         WHERE translation_id = $1 AND book_id = $2 ORDER BY chapter, verse",
    )
    .bind(translation_id)
    .bind(book_id)
    .fetch_all(db.read())
    .await?;
    let mut tx = crate::db::begin_write(db.write()).await?;
    for (chapter, verse, stored) in verses {
        sqlx::query("INSERT INTO verse_search (rowid, text) VALUES ($1, $2)")
            .bind(
                translation_id * TRANSLATION_STRIDE
                    + book_id * BOOK_STRIDE
                    + chapter * CHAPTER_STRIDE
                    + verse,
            )
            .bind(VerseText::new(KJV, &stored).text)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// The FTS5 query for `query`: every word and every phrase must appear
/// (implicit AND). Each is one FTS5 string, in double quotes (any inside
/// doubled), so nothing typed is read as FTS syntax: a word's string
/// matches it anywhere; a phrase's, its words separated by spaces, matches
/// them in order, side by side (FTS5's phrase), the tokenizer folding case
/// and accents for both as it did for the text.
fn match_expression(query: &SearchQuery) -> String {
    let string = |text: &str| format!("\"{}\"", text.replace('"', "\"\""));
    query
        .phrases
        .iter()
        .map(|phrase| string(&phrase.join(" ")))
        .chain(query.words.iter().map(|word| string(word)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Verses in one translation with every word and phrase of `query`, in
/// Bible order: `limit` of them starting at `offset`, plus how many match
/// in total.
pub async fn search_verses(
    pool: &SqlitePool,
    translation_id: i64,
    query: &SearchQuery,
    limit: i64,
    offset: i64,
) -> anyhow::Result<(i64, Vec<SearchHit>)> {
    if query.is_empty() {
        return Ok((0, Vec::new()));
    }
    let expression = match_expression(query);
    let (lo, hi) = translation_rowids(translation_id);

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM verse_search \
         WHERE verse_search MATCH $1 AND rowid >= $2 AND rowid < $3",
    )
    .bind(&expression)
    .bind(lo)
    .bind(hi)
    .fetch_one(pool)
    .await
    .context("Failed to count search results")?;

    let hits = sqlx::query_as::<_, SearchHit>(
        "WITH page AS ( \
             SELECT rowid AS id FROM verse_search \
             WHERE verse_search MATCH $1 AND rowid >= $2 AND rowid < $3 \
             ORDER BY rowid LIMIT $4 OFFSET $5 \
         ) \
         SELECT v.book_id, v.chapter, v.verse, v.text FROM page \
         JOIN verses v ON v.translation_id = $6 \
             AND v.book_id = page.id / $7 % 100 \
             AND v.chapter = page.id / $8 % 1000 \
             AND v.verse = page.id % $8 \
         ORDER BY page.id",
    )
    .bind(&expression)
    .bind(lo)
    .bind(hi)
    .bind(limit)
    .bind(offset)
    .bind(translation_id)
    .bind(BOOK_STRIDE)
    .bind(CHAPTER_STRIDE)
    .fetch_all(pool)
    .await
    .context("Failed to fetch search results")?;

    Ok((total, hits))
}
