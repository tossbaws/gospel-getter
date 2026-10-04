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

    // Cross-references are translation-independent (they're citations
    // between passages, not text), so they're keyed only by book/chapter/
    // verse. `ref_end_verse` is set when the reference is to a verse range
    // rather than a single verse.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS cross_references (
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL,
            verse INTEGER NOT NULL,
            ref_book_id INTEGER NOT NULL REFERENCES books(id),
            ref_chapter INTEGER NOT NULL,
            ref_verse INTEGER NOT NULL,
            ref_end_verse INTEGER,
            score INTEGER NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create cross_references table")?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_cross_references_verse
            ON cross_references (book_id, chapter, verse)
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create cross_references index")?;

    // The reader's bookmarks. A bookmark is a passage's coordinates —
    // book, chapter and an inclusive verse range — with no translation, so
    // it opens in whichever translation is selected. The CHECKs and the
    // UNIQUE constraint back up the validation in `db::bookmarks`.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS bookmarks (
            id INTEGER PRIMARY KEY,
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL CHECK (chapter >= 1),
            verse_start INTEGER NOT NULL CHECK (verse_start >= 1),
            verse_end INTEGER NOT NULL CHECK (verse_end >= verse_start),
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            UNIQUE (book_id, chapter, verse_start, verse_end)
        )
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create bookmarks table")?;

    // Full-text search index over `verses`. It's derived data only: built
    // from `verses` by `db::search`, never written back, and contentless —
    // it stores no copy of the text, only the tokens and a rowid encoding
    // each verse's coordinates (see `db::search`). Results
    // are always read from `verses` itself.
    sqlx::query(
        r#"
        CREATE VIRTUAL TABLE IF NOT EXISTS verse_search USING fts5(
            text,
            content = '',
            contentless_delete = 1
        )
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create verse_search index")?;

    // Which translations `verse_search` covers, and how many verses each
    // had when indexed, so startup only (re)indexes a translation that's
    // missing or has changed instead of rebuilding everything every launch.
    // Deliberately no foreign key: it mustn't stop a translation's rows
    // being cleared out for re-seeding (see the README).
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS search_index_state (
            translation_id INTEGER PRIMARY KEY,
            verse_count INTEGER NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create search_index_state table")?;

    Ok(())
}
