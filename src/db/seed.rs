use anyhow::Context;
use serde::Deserialize;
use sqlx::SqlitePool;

/// One bundled translation's raw text, embedded into the binary so the app
/// is fully self-contained and works offline.
struct TranslationSource {
    code: &'static str,
    name: &'static str,
    json: &'static str,
}

/// The list of bundled translations. **To add a new one**: drop a JSON file
/// into `data/` shaped like the existing ones (an array of
/// `{"name", "testament", "chapters"}` objects, one per book, in canonical
/// Genesis-to-Revelation order — see `data/kjv.json`), then add one entry
/// here. Only include translations you have the right to redistribute
/// (public domain, or under a license that permits bulk/offline copies) —
/// see the README for why NIV and ESV aren't bundled this way.
const TRANSLATIONS: &[TranslationSource] = &[
    TranslationSource {
        code: "kjv",
        name: "King James Version",
        json: include_str!("../../data/kjv.json"),
    },
    TranslationSource {
        code: "web",
        name: "World English Bible",
        json: include_str!("../../data/web.json"),
    },
];

#[derive(Deserialize)]
struct RawBook {
    name: String,
    testament: String,
    chapters: Vec<Vec<String>>,
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
    let mut tx = pool
        .begin()
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

    for source in TRANSLATIONS {
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

        books_seeded = true;
        tracing::info!("Seeded {} ({} books)", source.name, books.len());
    }

    tx.commit()
        .await
        .context("Failed to commit seed transaction")?;

    Ok(())
}
