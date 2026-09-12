use anyhow::Context;
use sqlx::SqlitePool;

/// Create the schema if it doesn't already exist.
pub async fn migrate(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS books (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            testament TEXT NOT NULL CHECK (testament IN ('OT', 'NT')),
            chapter_count INTEGER NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create books table")?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS translations (
            id INTEGER PRIMARY KEY,
            code TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create translations table")?;

    // Book structure (names, testament, chapter counts) is shared across
    // translations; only the verse text itself is per-translation.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS verses (
            translation_id INTEGER NOT NULL REFERENCES translations(id),
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL,
            verse INTEGER NOT NULL,
            text TEXT NOT NULL,
            PRIMARY KEY (translation_id, book_id, chapter, verse)
        )
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create verses table")?;

    // Single-row table (id is always 1) remembering the last chapter read,
    // so the app can reopen where the reader left off instead of always
    // starting at Genesis 1.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS reading_position (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            translation_id INTEGER NOT NULL REFERENCES translations(id),
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create reading_position table")?;

    Ok(())
}
