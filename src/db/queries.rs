use anyhow::Context;
use sqlx::SqlitePool;

use crate::db::models::{Book, Translation, Verse};

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
