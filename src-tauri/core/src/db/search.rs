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
use sqlx::{FromRow, SqliteConnection, SqlitePool};

use crate::db::Db;
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

/// The rowids of one book of a translation, in the same form.
fn book_rowids(translation_id: i64, book_id: i64) -> (i64, i64) {
    let first = translation_id * TRANSLATION_STRIDE + book_id * BOOK_STRIDE;
    (first, first + BOOK_STRIDE)
}

/// The rowid of one verse.
fn verse_rowid(translation_id: i64, (book_id, chapter, verse): (i64, i64, i64)) -> i64 {
    translation_id * TRANSLATION_STRIDE + book_id * BOOK_STRIDE + chapter * CHAPTER_STRIDE + verse
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
/// for anything it seeds), or whose index is missing verses all the same,
/// or all of them if the index holds an older kind of content (see
/// `SEARCH_CONTENT_VERSION`). Each translation is indexed a book at a time,
/// each book in its own transaction, so the database stays usable in
/// between (see `index_translation`). Returns the ids of the translations
/// it (re)indexed — none at all on a normal launch.
pub async fn ensure_search_index(db: &Db) -> anyhow::Result<Vec<i64>> {
    forget_outdated_index(db).await?;

    let translations: Vec<i64> = sqlx::query_scalar("SELECT id FROM translations ORDER BY id")
        .fetch_all(db.read())
        .await
        .context("Failed to list translations to index")?;

    let mut indexed = Vec::new();
    for translation_id in translations {
        let (verse_count, indexed_count) =
            verse_and_indexed_counts(db.read(), translation_id).await?;
        if indexed_count == Some(verse_count) {
            let mut reader = db
                .read()
                .acquire()
                .await
                .context("Failed to check the search index")?;
            let coverage = Coverage::of(&mut reader, translation_id).await?;
            if coverage.is_complete() {
                continue;
            }
            // An index left by an older version, which could mark one
            // complete that another copy of the app had cleared part of.
            tracing::warn!(
                "Translation {translation_id} was marked indexed for search, but isn't \
                 ({coverage:?}); indexing it again"
            );
        }
        index_translation(db, translation_id).await?;
        indexed.push(translation_id);
    }
    Ok(indexed)
}

/// How many verses a translation has, and how many the index recorded for it
/// (`None`: it isn't indexed).
async fn verse_and_indexed_counts(
    pool: &SqlitePool,
    translation_id: i64,
) -> anyhow::Result<(i64, Option<i64>)> {
    let verse_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM verses WHERE translation_id = $1")
            .bind(translation_id)
            .fetch_one(pool)
            .await
            .context("Failed to count verses to index")?;
    let indexed_count: Option<i64> =
        sqlx::query_scalar("SELECT verse_count FROM search_index_state WHERE translation_id = $1")
            .bind(translation_id)
            .fetch_optional(pool)
            .await
            .context("Failed to read search index state")?;
    Ok((verse_count, indexed_count))
}

/// Whether a translation's index is as `ensure_search_index` leaves it: of the
/// current kind of content, and covering every verse. Only reads, for a program
/// that can't build the index itself.
pub async fn search_index_is_current(
    pool: &SqlitePool,
    translation_id: i64,
) -> anyhow::Result<bool> {
    let version: Option<i64> =
        sqlx::query_scalar("SELECT version FROM search_index_version WHERE id = 1")
            .fetch_optional(pool)
            .await
            .context("Failed to read search index version")?;
    if version != Some(SEARCH_CONTENT_VERSION) {
        return Ok(false);
    }
    let (verse_count, indexed_count) = verse_and_indexed_counts(pool, translation_id).await?;
    Ok(indexed_count == Some(verse_count))
}

/// Take some of a translation's verses out of the index, in the transaction
/// (`conn`) that then takes them out of `verses`, and out of the verse count
/// it was recorded as indexed with, if it was. Both checks
/// `ensure_search_index` makes come out as before: a recorded count that
/// matched the verses still does (and one that didn't still doesn't), and
/// an index that held exactly the translation's verses still does, so a
/// complete index stays complete and isn't built again. Nothing else is
/// rebuilt.
pub(super) async fn forget_verses(
    conn: &mut SqliteConnection,
    translation_id: i64,
    verses: &[(i64, i64, i64)],
) -> anyhow::Result<()> {
    for &verse in verses {
        sqlx::query("DELETE FROM verse_search WHERE rowid = $1")
            .bind(verse_rowid(translation_id, verse))
            .execute(&mut *conn)
            .await
            .context("Failed to take a verse out of the search index")?;
    }
    sqlx::query(
        "UPDATE search_index_state SET verse_count = verse_count - $2 WHERE translation_id = $1",
    )
    .bind(translation_id)
    .bind(i64::try_from(verses.len()).context("Too many verses")?)
    .execute(&mut *conn)
    .await
    .context("Failed to update the search index state")?;
    Ok(())
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

/// How many times a translation's books are indexed before it's given up
/// on: once, and again for any that turn out to be missing verses when it's
/// published (something else cleared them meanwhile).
const INDEX_ROUNDS: usize = 3;

/// (Re)index one translation (see [`TranslationBuild`]): every book, then
/// the record that it's indexed, which is only written for an index that
/// holds every verse. Books found missing verses then are indexed again.
async fn index_translation(db: &Db, translation_id: i64) -> anyhow::Result<()> {
    let build = TranslationBuild::plan(db, translation_id).await?;
    let mut books = build.books.clone();
    for _ in 0..INDEX_ROUNDS {
        for &book_id in &books {
            build
                .index_book(db, book_id)
                .await
                .with_context(|| format!("Failed to index book {book_id} for search"))?;
        }
        match build.publish(db).await? {
            Published::Complete { verses } => {
                tracing::info!("Indexed translation {translation_id} for search ({verses} verses)");
                return Ok(());
            }
            Published::Incomplete(coverage) => {
                tracing::warn!(
                    "Translation {translation_id}'s search index was missing verses as it was \
                     finished ({coverage:?}), so it isn't marked indexed; indexing those books \
                     again"
                );
                books = coverage.books_to_index(&build.books);
            }
        }
    }
    bail!(
        "Translation {translation_id}'s search index was still missing verses after \
         {INDEX_ROUNDS} tries"
    )
}

/// One translation's (re)indexing, a step at a time, so that two copies of
/// the app (or the app and an older version) building it at once can never
/// leave it marked indexed while it's missing verses:
///
/// - [`index_book`](Self::index_book) replaces one book's rows with its
///   verses' text in one short transaction, so the index is never missing
///   part of a book, and a reader's own write waits behind one book at most
///   (reads don't wait at all). Two builds writing the same book just
///   replace it with the same text, one after the other.
/// - [`publish`](Self::publish) records the translation as indexed only if,
///   in that same transaction, the index holds exactly its verses: every
///   one, and nothing else (see [`Coverage`]). It's a few milliseconds'
///   check, not a count copied from `verses`.
/// - Both only write while the index holds this version's kind of content
///   ([`SEARCH_CONTENT_VERSION`]): one that a newer version has taken over
///   (to rebuild with its own) isn't this build's to write to.
///
/// Nothing is cleared up front, so a build that's stopped part-way (the
/// app quits) takes nothing away from the index that another build, or an
/// earlier launch, put there; and a translation that was indexed stays
/// searchable while it's indexed again.
#[derive(Debug)]
pub(super) struct TranslationBuild {
    translation_id: i64,
    code: String,
    /// Its books, in order.
    pub(super) books: Vec<i64>,
}

/// What [`TranslationBuild::publish`] found.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Published {
    /// The index holds every verse, and is recorded as indexed.
    Complete { verses: i64 },
    /// It doesn't, and isn't recorded.
    Incomplete(Coverage),
}

impl TranslationBuild {
    /// What indexing `translation_id` involves: its books.
    pub(super) async fn plan(db: &Db, translation_id: i64) -> anyhow::Result<Self> {
        // The rowid encoding needs every coordinate to fit its field.
        let (max_book, max_chapter, max_verse): (Option<i64>, Option<i64>, Option<i64>) =
            sqlx::query_as(
                "SELECT MAX(book_id), MAX(chapter), MAX(verse) FROM verses \
                 WHERE translation_id = $1",
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
        Ok(Self {
            translation_id,
            code,
            books,
        })
    }

    /// Replace one book's rows in the index with its verses' text, in one
    /// transaction that starts with the write lock (see `begin_write`: a
    /// write into the full-text table reads before it writes). The KJV's
    /// text as the app shows it (see `VerseText`) is made first, on the read
    /// pool, so the writer isn't held for it.
    pub(super) async fn index_book(&self, db: &Db, book_id: i64) -> anyhow::Result<()> {
        let shown = if self.code == KJV {
            let verses: Vec<(i64, i64, String)> = sqlx::query_as(
                "SELECT chapter, verse, text FROM verses \
                 WHERE translation_id = $1 AND book_id = $2 ORDER BY chapter, verse",
            )
            .bind(self.translation_id)
            .bind(book_id)
            .fetch_all(db.read())
            .await?;
            Some(
                verses
                    .into_iter()
                    .map(|(chapter, verse, stored)| {
                        let rowid = verse_rowid(self.translation_id, (book_id, chapter, verse));
                        (rowid, VerseText::new(KJV, &stored).text)
                    })
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        };

        let mut tx = crate::db::begin_write(db.write()).await?;
        ensure_current_content(&mut tx).await?;
        let (lo, hi) = book_rowids(self.translation_id, book_id);
        sqlx::query("DELETE FROM verse_search WHERE rowid >= $1 AND rowid < $2")
            .bind(lo)
            .bind(hi)
            .execute(&mut *tx)
            .await?;
        match shown {
            Some(verses) => {
                for (rowid, text) in verses {
                    sqlx::query("INSERT INTO verse_search (rowid, text) VALUES ($1, $2)")
                        .bind(rowid)
                        .bind(text)
                        .execute(&mut *tx)
                        .await?;
                }
            }
            None => {
                sqlx::query(
                    "INSERT INTO verse_search (rowid, text) \
                     SELECT translation_id * $3 + book_id * $4 + chapter * $5 + verse, text \
                     FROM verses WHERE translation_id = $1 AND book_id = $2",
                )
                .bind(self.translation_id)
                .bind(book_id)
                .bind(TRANSLATION_STRIDE)
                .bind(BOOK_STRIDE)
                .bind(CHAPTER_STRIDE)
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    /// Record the translation as indexed, if the index holds exactly its
    /// verses, checked in the same transaction (with the write lock, so
    /// nothing changes the index in between). Rows for books it no longer
    /// has (a translation since cleared out and seeded again) go first.
    pub(super) async fn publish(&self, db: &Db) -> anyhow::Result<Published> {
        let mut tx = crate::db::begin_write(db.write())
            .await
            .context("Failed to start the search index's last transaction")?;
        ensure_current_content(&mut tx).await?;
        let (lo, hi) = translation_rowids(self.translation_id);
        sqlx::query(
            "DELETE FROM verse_search WHERE rowid >= $1 AND rowid < $2 \
             AND rowid / $3 % 100 NOT IN \
                 (SELECT DISTINCT book_id FROM verses WHERE translation_id = $4)",
        )
        .bind(lo)
        .bind(hi)
        .bind(BOOK_STRIDE)
        .bind(self.translation_id)
        .execute(&mut *tx)
        .await
        .context("Failed to clear the index of books no longer there")?;
        let coverage = Coverage::of(&mut tx, self.translation_id).await?;
        if !coverage.is_complete() {
            // Rolled back as it's dropped: nothing is recorded.
            return Ok(Published::Incomplete(coverage));
        }
        sqlx::query(
            "INSERT INTO search_index_state (translation_id, verse_count) VALUES ($1, $2) \
             ON CONFLICT (translation_id) DO UPDATE SET verse_count = excluded.verse_count",
        )
        .bind(self.translation_id)
        .bind(coverage.verses)
        .execute(&mut *tx)
        .await
        .context("Failed to record search index state")?;
        tx.commit()
            .await
            .context("Failed to commit search index state")?;
        Ok(Published::Complete {
            verses: coverage.verses,
        })
    }
}

/// Refuse to write to an index that holds another version's kind of content
/// (`search_index_version`): a newer version of the app is rebuilding it
/// with its own. Within a write transaction, so it can't change before the
/// write.
async fn ensure_current_content(tx: &mut SqliteConnection) -> anyhow::Result<()> {
    let version: Option<i64> =
        sqlx::query_scalar("SELECT version FROM search_index_version WHERE id = 1")
            .fetch_optional(&mut *tx)
            .await
            .context("Failed to read search index version")?;
    if version != Some(SEARCH_CONTENT_VERSION) {
        bail!(
            "The search index now holds content version {version:?}, not this version's \
             {SEARCH_CONTENT_VERSION}: another version of the app is building it"
        );
    }
    Ok(())
}

/// How much of one translation the index holds: how many verses it has,
/// how many rows the index has in its range, how many of its verses have
/// none, and the books they're in. Complete when no verse is missing and
/// there's no row besides (each verse has one rowid, so then the rows are
/// exactly the verses).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Coverage {
    pub(super) verses: i64,
    pub(super) rows: i64,
    pub(super) missing: i64,
    pub(super) books_missing_verses: Vec<i64>,
}

impl Coverage {
    /// Read on `conn` (a reader, or a write transaction).
    pub(super) async fn of(
        conn: &mut SqliteConnection,
        translation_id: i64,
    ) -> anyhow::Result<Self> {
        let verses: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM verses WHERE translation_id = $1")
                .bind(translation_id)
                .fetch_one(&mut *conn)
                .await
                .context("Failed to count verses")?;
        let (lo, hi) = translation_rowids(translation_id);
        let rows: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM verse_search WHERE rowid >= $1 AND rowid < $2",
        )
        .bind(lo)
        .bind(hi)
        .fetch_one(&mut *conn)
        .await
        .context("Failed to count the search index's rows")?;
        // Each verse's book, if the index has no row for it.
        let missing_books: Vec<i64> = sqlx::query_scalar(
            "SELECT v.book_id FROM verses v WHERE v.translation_id = $1 \
             AND NOT EXISTS (SELECT 1 FROM verse_search s WHERE s.rowid = \
                 v.translation_id * $2 + v.book_id * $3 + v.chapter * $4 + v.verse) \
             ORDER BY v.book_id",
        )
        .bind(translation_id)
        .bind(TRANSLATION_STRIDE)
        .bind(BOOK_STRIDE)
        .bind(CHAPTER_STRIDE)
        .fetch_all(&mut *conn)
        .await
        .context("Failed to check the search index's verses")?;
        let missing = i64::try_from(missing_books.len()).context("Too many verses")?;
        let mut books_missing_verses = missing_books;
        books_missing_verses.dedup();
        Ok(Self {
            verses,
            rows,
            missing,
            books_missing_verses,
        })
    }

    pub(super) fn is_complete(&self) -> bool {
        self.missing == 0 && self.rows == self.verses
    }

    /// The books to index again to complete it: those missing verses, or
    /// every one of `books` if it has rows besides its verses' (which only
    /// indexing a book again clears).
    fn books_to_index(&self, books: &[i64]) -> Vec<i64> {
        if self.rows > self.verses - self.missing {
            books.to_vec()
        } else {
            self.books_missing_verses.clone()
        }
    }
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
    let rowids = translation_rowids(translation_id);
    search_rowids(pool, translation_id, rowids, query, limit, offset).await
}

/// As [`search_verses`], in one book of the translation.
pub async fn search_verses_in_book(
    pool: &SqlitePool,
    translation_id: i64,
    book_id: i64,
    query: &SearchQuery,
    limit: i64,
    offset: i64,
) -> anyhow::Result<(i64, Vec<SearchHit>)> {
    let rowids = book_rowids(translation_id, book_id);
    search_rowids(pool, translation_id, rowids, query, limit, offset).await
}

/// A page of the verses with every word and phrase of `query`, among the
/// rowids from `lo` up to `hi`, all of one translation, and how many match.
async fn search_rowids(
    pool: &SqlitePool,
    translation_id: i64,
    (lo, hi): (i64, i64),
    query: &SearchQuery,
    limit: i64,
    offset: i64,
) -> anyhow::Result<(i64, Vec<SearchHit>)> {
    if query.is_empty() {
        return Ok((0, Vec::new()));
    }
    let expression = match_expression(query);

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
