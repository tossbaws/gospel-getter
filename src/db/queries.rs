use anyhow::Context;
use sqlx::SqlitePool;

use crate::db::models::{Book, CrossReference, Translation, Verse};

/// Get every book, in canonical order.
pub async fn get_all_books(pool: &SqlitePool) -> anyhow::Result<Vec<Book>> {
    sqlx::query_as::<_, Book>("SELECT id, name, testament, chapter_count FROM books ORDER BY id")
        .fetch_all(pool)
        .await
        .context("Failed to fetch books")
}

/// Get every bundled translation, in the order they were seeded.
pub async fn get_all_translations(pool: &SqlitePool) -> anyhow::Result<Vec<Translation>> {
    sqlx::query_as::<_, Translation>("SELECT id, code, name FROM translations ORDER BY id")
        .fetch_all(pool)
        .await
        .context("Failed to fetch translations")
}

/// Get every verse in one chapter of one translation, in verse order.
pub async fn get_chapter_verses(
    pool: &SqlitePool,
    translation_id: i64,
    book_id: i64,
    chapter: i64,
) -> anyhow::Result<Vec<Verse>> {
    sqlx::query_as::<_, Verse>(
        "SELECT verse, text FROM verses \
         WHERE translation_id = $1 AND book_id = $2 AND chapter = $3 \
         ORDER BY verse",
    )
    .bind(translation_id)
    .bind(book_id)
    .bind(chapter)
    .fetch_all(pool)
    .await
    .context("Failed to fetch verses")
}

/// The last chapter read (translation, book, chapter), if the reader has
/// ever actually read one.
pub async fn get_reading_position(pool: &SqlitePool) -> anyhow::Result<Option<(i64, i64, i64)>> {
    sqlx::query_as("SELECT translation_id, book_id, chapter FROM reading_position WHERE id = 1")
        .fetch_optional(pool)
        .await
        .context("Failed to fetch reading position")
}

/// Remember the last chapter read, overwriting whatever was there before.
pub async fn save_reading_position(
    pool: &SqlitePool,
    translation_id: i64,
    book_id: i64,
    chapter: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO reading_position (id, translation_id, book_id, chapter) \
         VALUES (1, $1, $2, $3) \
         ON CONFLICT(id) DO UPDATE SET \
             translation_id = excluded.translation_id, \
             book_id = excluded.book_id, \
             chapter = excluded.chapter",
    )
    .bind(translation_id)
    .bind(book_id)
    .bind(chapter)
    .execute(pool)
    .await
    .context("Failed to save reading position")?;
    Ok(())
}

/// Cross-references for every verse in one chapter, ordered by verse then
/// descending relevance score.
pub async fn get_chapter_cross_references(
    pool: &SqlitePool,
    book_id: i64,
    chapter: i64,
) -> anyhow::Result<Vec<CrossReference>> {
    sqlx::query_as::<_, CrossReference>(
        "SELECT verse, ref_book_id, ref_chapter, ref_verse, ref_end_verse, score \
         FROM cross_references WHERE book_id = $1 AND chapter = $2 \
         ORDER BY verse, score DESC",
    )
    .bind(book_id)
    .bind(chapter)
    .fetch_all(pool)
    .await
    .context("Failed to fetch cross references")
}

/// The text of a verse or verse range, for expanding a cross-reference.
pub async fn get_verse_range(
    pool: &SqlitePool,
    translation_id: i64,
    book_id: i64,
    chapter: i64,
    start_verse: i64,
    end_verse: i64,
) -> anyhow::Result<Vec<Verse>> {
    sqlx::query_as::<_, Verse>(
        "SELECT verse, text FROM verses \
         WHERE translation_id = $1 AND book_id = $2 AND chapter = $3 \
         AND verse BETWEEN $4 AND $5 ORDER BY verse",
    )
    .bind(translation_id)
    .bind(book_id)
    .bind(chapter)
    .bind(start_verse)
    .bind(end_verse)
    .fetch_all(pool)
    .await
    .context("Failed to fetch verse range")
}
