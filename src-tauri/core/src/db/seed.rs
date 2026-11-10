//! The bundled translations and cross-references, embedded in the binary,
//! and seeding the database with whichever of them it doesn't have yet.

use anyhow::Context;
use serde::Deserialize;

use crate::db::WritePool;

/// How the app lists one bundled translation (its text is in `TEXTS`).
pub struct TranslationSource {
    pub code: &'static str,
    pub name: &'static str,
    /// When it was made, as the chooser shows it ("1611/1769"); `None` for
    /// one still being revised (the WEB).
    pub date: Option<&'static str>,
    /// Its licence, as the chooser shows it.
    pub licence: &'static str,
    /// Where it comes from, as About credits it.
    pub credit: &'static str,
}

/// The list of bundled translations, in the order Aa lists them. **To add
/// a new one**: drop a JSON file into `data/` shaped like the existing ones
/// (an array of `{"name", "testament", "chapters"}` objects, one per book,
/// in canonical Genesis-to-Revelation order — see `data/kjv.json`; a verse
/// the translation doesn't number is `null`, and footnotes kept apart from
/// the text go in each book's `footnotes`, as in `data/bsb.json`), then add
/// one entry here and one in `TEXTS`. Only include translations you have the right to
/// redistribute (public domain, or under a license that permits
/// bulk/offline copies) — see the README for why NIV and ESV aren't bundled
/// this way.
///
/// They're seeded in `SEED_ORDER`, which never changes, so a translation's
/// database id is the same on every install, new or upgraded.
pub const TRANSLATIONS: &[TranslationSource] = &[
    TranslationSource {
        code: "kjv",
        name: "King James Version",
        date: Some("1611/1769"),
        licence: "public domain",
        credit: "King James Version (KJV), public domain, from the thiagobodruk/bible dataset",
    },
    TranslationSource {
        code: "bsb",
        name: "Berean Standard Bible",
        date: Some("2023"),
        licence: "public domain",
        credit: "Berean Standard Bible (BSB), public domain, berean.bible",
    },
    TranslationSource {
        code: "web",
        name: "World English Bible",
        date: None,
        licence: "public domain",
        credit: "World English Bible (WEB), public domain, from TehShrike/world-english-bible (via ebible.org)",
    },
];

/// Each bundled translation's raw text, embedded into the binary so the app
/// is fully self-contained and works offline. Apart from [`TRANSLATIONS`], so
/// that a program that only lists them (`gospel-getter-cli`) doesn't carry
/// their text: the linker keeps only what's used. A `static`, not a `const`: a
/// const's data is copied into every crate that instantiates what uses it, and
/// `seed_missing` is instantiated in the app too, which doubled its size.
static TEXTS: [(&str, &str); 3] = [
    ("kjv", include_str!("../../../data/kjv.json")),
    ("bsb", include_str!("../../../data/bsb.json")),
    ("web", include_str!("../../../data/web.json")),
];

/// The order translations are seeded in: each new one at the end.
const SEED_ORDER: [&str; 3] = ["kjv", "web", "bsb"];

/// Where `code` comes in [`TRANSLATIONS`] (unknown codes last).
pub fn rank(code: &str) -> usize {
    TRANSLATIONS
        .iter()
        .position(|t| t.code == code)
        .unwrap_or(TRANSLATIONS.len())
}

/// The bundled translation `code`.
pub fn source(code: &str) -> Option<&'static TranslationSource> {
    TRANSLATIONS.iter().find(|t| t.code == code)
}

#[derive(Deserialize)]
struct RawBook {
    name: String,
    testament: String,
    /// `None` for a verse the translation doesn't number.
    chapters: Vec<Vec<Option<String>>>,
    #[serde(default)]
    footnotes: Vec<RawFootnote>,
}

/// A footnote kept apart from its verse's text (see `domain::usfm_note`).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFootnote {
    chapter: i64,
    verse: i64,
    at: i64,
    usfm: String,
}

/// Populate `translations`, `books`, and `verses` from the bundled
/// translations — but only whatever is actually missing. This is what
/// makes the app self-initializing on a fresh checkout *and* lets an
/// already-running install pick up a newly-added translation after a
/// rebuild: each translation in `TRANSLATIONS` is seeded independently, on
/// its own presence check (`SELECT id FROM translations WHERE code = ...`),
/// not gated by a single "is the database empty" flag. Editing the text of
/// a translation that's already seeded still requires clearing its rows
/// (or the whole database) first — this only fills in what's absent.
pub async fn seed_missing(writer: &WritePool) -> anyhow::Result<()> {
    let mut tx = crate::db::begin_write(writer)
        .await
        .context("Failed to start seed transaction")?;

    // Book structure (names, testament, chapter counts) is shared across
    // translations, so it's only seeded once, from whichever translation
    // seeds first.
    let (book_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM books")
        .fetch_one(&mut *tx)
        .await
        .context("Failed to check book count")?;
    let mut books_seeded = book_count > 0;

    let (max_translation_id,): (Option<i64>,) = sqlx::query_as("SELECT MAX(id) FROM translations")
        .fetch_one(&mut *tx)
        .await
        .context("Failed to check existing translations")?;
    let mut next_translation_id = max_translation_id.unwrap_or(0) + 1;

    for code in SEED_ORDER {
        let source = source(code).expect("every seeded translation is listed");
        let already_seeded: Option<(i64,)> =
            sqlx::query_as("SELECT id FROM translations WHERE code = $1")
                .bind(source.code)
                .fetch_optional(&mut *tx)
                .await
                .context(format!("Failed to check translation {}", source.code))?;

        if already_seeded.is_some() {
            continue;
        }

        let translation_id = next_translation_id;
        next_translation_id += 1;

        sqlx::query("INSERT INTO translations (id, code, name) VALUES ($1, $2, $3)")
            .bind(translation_id)
            .bind(source.code)
            .bind(source.name)
            .execute(&mut *tx)
            .await
            .context(format!("Failed to insert translation {}", source.code))?;

        // Whatever the search index held for this id (from a translation
        // since cleared out) is stale now; `ensure_search_index` rebuilds it.
        sqlx::query("DELETE FROM search_index_state WHERE translation_id = $1")
            .bind(translation_id)
            .execute(&mut *tx)
            .await
            .context(format!(
                "Failed to reset search index state for {}",
                source.code
            ))?;

        let (_, text) = TEXTS
            .iter()
            .find(|(code, _)| *code == source.code)
            .expect("every seeded translation has its text");
        let books: Vec<RawBook> = serde_json::from_str(text)
            .context(format!("Failed to parse bundled {} data", source.code))?;

        for (book_index, book) in books.iter().enumerate() {
            let book_id = book_index as i64 + 1;

            if !books_seeded {
                sqlx::query(
                    "INSERT INTO books (id, name, testament, chapter_count) VALUES ($1, $2, $3, $4)",
                )
                .bind(book_id)
                .bind(&book.name)
                .bind(&book.testament)
                .bind(book.chapters.len() as i64)
                .execute(&mut *tx)
                .await
                .context(format!("Failed to insert book {}", book.name))?;
            }

            for (chapter_index, verses) in book.chapters.iter().enumerate() {
                let chapter = chapter_index as i64 + 1;
                for (verse_index, text) in verses.iter().enumerate() {
                    let verse = verse_index as i64 + 1;
                    let Some(text) = text else { continue };
                    sqlx::query(
                        "INSERT INTO verses (translation_id, book_id, chapter, verse, text) \
                         VALUES ($1, $2, $3, $4, $5)",
                    )
                    .bind(translation_id)
                    .bind(book_id)
                    .bind(chapter)
                    .bind(verse)
                    .bind(text)
                    .execute(&mut *tx)
                    .await
                    .context(format!(
                        "Failed to insert {} {} {}:{}",
                        source.code, book.name, chapter, verse
                    ))?;
                }
            }
        }

        // Its footnotes, numbered in order within each verse; any left by a
        // translation since cleared out are gone first.
        sqlx::query("DELETE FROM footnotes WHERE translation_id = $1")
            .bind(translation_id)
            .execute(&mut *tx)
            .await
            .context(format!("Failed to clear footnotes for {}", source.code))?;
        for (book_index, book) in books.iter().enumerate() {
            let mut ordinal = 0;
            let mut last = (0, 0);
            for note in &book.footnotes {
                ordinal = if (note.chapter, note.verse) == last {
                    ordinal + 1
                } else {
                    0
                };
                last = (note.chapter, note.verse);
                sqlx::query(
                    "INSERT INTO footnotes \
                     (translation_id, book_id, chapter, verse, ordinal, position, usfm) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7)",
                )
                .bind(translation_id)
                .bind(book_index as i64 + 1)
                .bind(note.chapter)
                .bind(note.verse)
                .bind(ordinal)
                .bind(note.at)
                .bind(&note.usfm)
                .execute(&mut *tx)
                .await
                .context(format!(
                    "Failed to insert a footnote on {} {} {}:{}",
                    source.code, book.name, note.chapter, note.verse
                ))?;
            }
        }

        books_seeded = true;
        tracing::info!("Seeded {} ({} books)", source.name, books.len());
    }

    tx.commit()
        .await
        .context("Failed to commit seed transaction")?;

    Ok(())
}

const CROSS_REFERENCES_JSON: &str = include_str!("../../../data/cross_references.json");

#[derive(Deserialize)]
struct RawCrossRefBook {
    #[allow(dead_code)] // provenance only, not used at runtime
    code: String,
    chapters: Vec<Vec<Vec<RawCrossRef>>>,
}

#[derive(Deserialize)]
struct RawCrossRef {
    book: i64,
    chapter: i64,
    verse: i64,
    #[serde(rename = "endVerse")]
    end_verse: Option<i64>,
    score: i64,
}

/// Populate `cross_references` from the bundled dataset, but only if it's
/// currently empty. A quick look first, so a normal launch doesn't parse
/// the dataset; then `seed_cross_references`, which looks again with the
/// write lock held, so two copies of the app starting at once (both finding
/// it empty here) seed it once between them.
pub async fn seed_cross_references_if_empty(writer: &WritePool) -> anyhow::Result<()> {
    if cross_references_present(writer.pool()).await? {
        return Ok(());
    }
    let books = bundled_cross_references()?;
    seed_cross_references(writer, &books).await?;
    Ok(())
}

/// Whether `cross_references` has any rows.
async fn cross_references_present(
    conn: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
) -> anyhow::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM cross_references)")
        .fetch_one(conn)
        .await
        .context("Failed to check for cross-references")
}

/// The bundled cross-reference dataset, parsed.
fn bundled_cross_references() -> anyhow::Result<Vec<RawCrossRefBook>> {
    serde_json::from_str(CROSS_REFERENCES_JSON)
        .context("Failed to parse bundled cross-reference data")
}

/// Insert `books`' cross-references, in one transaction that starts with the
/// write lock, only if the table is still empty once that's held: another
/// copy of the app may have seeded it since this one looked. Whether this
/// seeded it.
async fn seed_cross_references(
    writer: &WritePool,
    books: &[RawCrossRefBook],
) -> anyhow::Result<bool> {
    let mut tx = crate::db::begin_write(writer)
        .await
        .context("Failed to start cross-reference seed transaction")?;
    if cross_references_present(&mut *tx).await? {
        return Ok(false);
    }

    for (book_index, book) in books.iter().enumerate() {
        let book_id = book_index as i64 + 1;
        for (chapter_index, verses) in book.chapters.iter().enumerate() {
            let chapter = chapter_index as i64 + 1;
            for (verse_index, refs) in verses.iter().enumerate() {
                let verse = verse_index as i64 + 1;
                for r in refs {
                    sqlx::query(
                        "INSERT INTO cross_references \
                         (book_id, chapter, verse, ref_book_id, ref_chapter, ref_verse, ref_end_verse, score) \
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                    )
                    .bind(book_id)
                    .bind(chapter)
                    .bind(verse)
                    .bind(r.book)
                    .bind(r.chapter)
                    .bind(r.verse)
                    .bind(r.end_verse)
                    .bind(r.score)
                    .execute(&mut *tx)
                    .await
                    .context("Failed to insert cross reference")?;
                }
            }
        }
    }

    tx.commit()
        .await
        .context("Failed to commit cross-reference seed transaction")?;
    tracing::info!("Seeded cross-reference data");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::prepared_database;

    async fn count(writer: &WritePool) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM cross_references")
            .fetch_one(writer.pool())
            .await
            .expect("count")
    }

    /// Two copies of the app start at once on a database with no
    /// cross-references: each looks and finds none, then each seeds as the
    /// write lock lets it, one after the other. One dataset results, as
    /// one seed makes it.
    #[tokio::test]
    async fn two_seeders_both_past_their_first_look_seed_the_cross_references_once() {
        let (db, one) = prepared_database("xref_two_seeders").await;
        let one_dataset = count(one.write()).await;
        assert!(one_dataset > 0);
        sqlx::query("DELETE FROM cross_references")
            .execute(one.write().pool())
            .await
            .expect("empty it, as on a first launch");
        let two = db.connect().await;

        assert!(!cross_references_present(one.write().pool()).await.unwrap());
        assert!(!cross_references_present(two.write().pool()).await.unwrap());
        let books = bundled_cross_references().unwrap();

        assert!(seed_cross_references(one.write(), &books).await.unwrap());
        assert!(!seed_cross_references(two.write(), &books).await.unwrap());
        assert_eq!(count(one.write()).await, one_dataset);
        assert_eq!(count(two.write()).await, one_dataset);

        // And a later launch's whole step does nothing.
        seed_cross_references_if_empty(two.write()).await.unwrap();
        assert_eq!(count(one.write()).await, one_dataset);
        two.close().await;
        one.close().await;
    }
}
