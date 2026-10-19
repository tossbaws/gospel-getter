use anyhow::Context;
use serde::Deserialize;
use sqlx::SqlitePool;

/// One bundled translation's raw text, embedded into the binary so the app
/// is fully self-contained and works offline, and how the app lists it.
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
    json: &'static str,
}

/// The list of bundled translations, in the order Aa lists them. **To add
/// a new one**: drop a JSON file into `data/` shaped like the existing ones
/// (an array of `{"name", "testament", "chapters"}` objects, one per book,
/// in canonical Genesis-to-Revelation order — see `data/kjv.json`; a verse
/// the translation doesn't number is `null`, and footnotes kept apart from
/// the text go in each book's `footnotes`, as in `data/bsb.json`), then add
/// one entry here. Only include translations you have the right to
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
        json: include_str!("../../data/kjv.json"),
    },
    TranslationSource {
        code: "bsb",
        name: "Berean Standard Bible",
        date: Some("2023"),
        licence: "public domain",
        credit: "Berean Standard Bible (BSB), public domain, berean.bible",
        json: include_str!("../../data/bsb.json"),
    },
    TranslationSource {
        code: "web",
        name: "World English Bible",
        date: None,
        licence: "public domain",
        credit: "World English Bible (WEB), public domain, from TehShrike/world-english-bible (via ebible.org)",
        json: include_str!("../../data/web.json"),
    },
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
pub async fn seed_missing(pool: &SqlitePool) -> anyhow::Result<()> {
    let mut tx = crate::db::begin_write(pool)
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

        let books: Vec<RawBook> = serde_json::from_str(source.json)
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

const CROSS_REFERENCES_JSON: &str = include_str!("../../data/cross_references.json");

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
/// currently empty.
pub async fn seed_cross_references_if_empty(pool: &SqlitePool) -> anyhow::Result<()> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM cross_references")
        .fetch_one(pool)
        .await
        .context("Failed to check cross-reference count")?;
    if count > 0 {
        return Ok(());
    }

    let books: Vec<RawCrossRefBook> = serde_json::from_str(CROSS_REFERENCES_JSON)
        .context("Failed to parse bundled cross-reference data")?;

    let mut tx = crate::db::begin_write(pool)
        .await
        .context("Failed to start cross-reference seed transaction")?;

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
    Ok(())
}
