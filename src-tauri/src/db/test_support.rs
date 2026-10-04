//! Disposable databases for tests: each one a fresh file under the OS temp
//! directory, never anywhere near a real install, deleted when dropped.
//! Includes a stand-in for a database left by v2.1.0 (before bookmarks and
//! search existed), built from that release's exact schema.

use serde::Deserialize;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::db::{create_pool, ensure_search_index, prepare};

/// A database file that's deleted (with any journal) on drop.
pub struct TempDb {
    dir: PathBuf,
}

impl TempDb {
    pub fn new(label: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "gospel_getter_db_test_{label}_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Self { dir }
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join("gospel_getter.db")
    }

    /// Open (creating if needed) a pool on this database, as the app does.
    pub async fn connect(&self) -> SqlitePool {
        create_pool(&format!("sqlite:{}", self.path().display()))
            .await
            .expect("connect to test database")
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A database prepared exactly as the app prepares one at startup, with
/// the search index built.
pub async fn fresh_database(label: &str) -> (TempDb, SqlitePool) {
    let db = TempDb::new(label);
    let pool = db.connect().await;
    prepare(&pool).await.expect("prepare database");
    ensure_search_index(&pool)
        .await
        .expect("build search index");
    (db, pool)
}

/// The schema as v2.1.0 created it, verbatim — frozen here rather than
/// shared with `migrate.rs`, which is exactly what's under test.
const V2_1_0_SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS books (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        testament TEXT NOT NULL CHECK (testament IN ('OT', 'NT')),
        chapter_count INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS translations (
        id INTEGER PRIMARY KEY,
        code TEXT NOT NULL UNIQUE,
        name TEXT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS verses (
        translation_id INTEGER NOT NULL REFERENCES translations(id),
        book_id INTEGER NOT NULL REFERENCES books(id),
        chapter INTEGER NOT NULL,
        verse INTEGER NOT NULL,
        text TEXT NOT NULL,
        PRIMARY KEY (translation_id, book_id, chapter, verse)
    )",
    "CREATE TABLE IF NOT EXISTS reading_position (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        translation_id INTEGER NOT NULL REFERENCES translations(id),
        book_id INTEGER NOT NULL REFERENCES books(id),
        chapter INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS cross_references (
        book_id INTEGER NOT NULL REFERENCES books(id),
        chapter INTEGER NOT NULL,
        verse INTEGER NOT NULL,
        ref_book_id INTEGER NOT NULL REFERENCES books(id),
        ref_chapter INTEGER NOT NULL,
        ref_verse INTEGER NOT NULL,
        ref_end_verse INTEGER,
        score INTEGER NOT NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_cross_references_verse
        ON cross_references (book_id, chapter, verse)",
];

#[derive(Deserialize)]
pub struct RawBook {
    pub name: String,
    pub testament: String,
    pub chapters: Vec<Vec<String>>,
}

/// A bundled translation file, parsed — the exact strings the seed step
/// stores.
pub fn bundled(code: &str) -> Vec<RawBook> {
    let json = match code {
        "kjv" => include_str!("../../data/kjv.json"),
        "web" => include_str!("../../data/web.json"),
        _ => panic!("no bundled translation {code}"),
    };
    serde_json::from_str(json).expect("bundled translation JSON")
}

/// Write a database as v2.1.0 left it after some reading: v2.1.0's schema,
/// both translations (seeded the way v2.1.0 seeded them), a saved reading
/// position (WEB, Romans 8) and a few cross-references. Cross-references
/// are only a sample — enough that the current seed step must leave them
/// alone, as it would a real install's.
pub async fn write_v2_1_0_database(path: &Path) {
    let pool = create_pool(&format!("sqlite:{}", path.display()))
        .await
        .expect("create v2.1.0 database");
    for statement in V2_1_0_SCHEMA {
        sqlx::query(*statement)
            .execute(&pool)
            .await
            .expect("v2.1.0 schema");
    }

    let mut tx = pool.begin().await.expect("begin");
    for (translation_id, code, name) in [
        (1_i64, "kjv", "King James Version"),
        (2, "web", "World English Bible"),
    ] {
        sqlx::query("INSERT INTO translations (id, code, name) VALUES ($1, $2, $3)")
            .bind(translation_id)
            .bind(code)
            .bind(name)
            .execute(&mut *tx)
            .await
            .expect("insert translation");
        for (book_index, book) in bundled(code).into_iter().enumerate() {
            let book_id = book_index as i64 + 1;
            if translation_id == 1 {
                sqlx::query(
                    "INSERT INTO books (id, name, testament, chapter_count) VALUES ($1, $2, $3, $4)",
                )
                .bind(book_id)
                .bind(&book.name)
                .bind(&book.testament)
                .bind(book.chapters.len() as i64)
                .execute(&mut *tx)
                .await
                .expect("insert book");
            }
            for (chapter_index, verses) in book.chapters.iter().enumerate() {
                for (verse_index, text) in verses.iter().enumerate() {
                    sqlx::query(
                        "INSERT INTO verses (translation_id, book_id, chapter, verse, text) \
                         VALUES ($1, $2, $3, $4, $5)",
                    )
                    .bind(translation_id)
                    .bind(book_id)
                    .bind(chapter_index as i64 + 1)
                    .bind(verse_index as i64 + 1)
                    .bind(text)
                    .execute(&mut *tx)
                    .await
                    .expect("insert verse");
                }
            }
        }
    }
    sqlx::query(
        "INSERT INTO reading_position (id, translation_id, book_id, chapter) VALUES (1, 2, 45, 8)",
    )
    .execute(&mut *tx)
    .await
    .expect("insert reading position");
    for (verse, ref_book, ref_chapter, ref_verse, score) in [
        (16, 45, 5, 8, 120),
        (16, 62, 4, 9, 98),
        (17, 43, 12, 47, 40),
    ] {
        sqlx::query(
            "INSERT INTO cross_references \
             (book_id, chapter, verse, ref_book_id, ref_chapter, ref_verse, ref_end_verse, score) \
             VALUES (43, 3, $1, $2, $3, $4, NULL, $5)",
        )
        .bind(verse)
        .bind(ref_book)
        .bind(ref_chapter)
        .bind(ref_verse)
        .bind(score)
        .execute(&mut *tx)
        .await
        .expect("insert cross reference");
    }
    tx.commit().await.expect("commit v2.1.0 data");
    pool.close().await;
}

/// Every row of `verses`, with the text as raw bytes, in key order.
pub async fn verses_snapshot(pool: &SqlitePool) -> Vec<(i64, i64, i64, i64, Vec<u8>)> {
    sqlx::query_as(
        "SELECT translation_id, book_id, chapter, verse, CAST(text AS BLOB) FROM verses \
         ORDER BY translation_id, book_id, chapter, verse",
    )
    .fetch_all(pool)
    .await
    .expect("snapshot verses")
}

/// Every row of the other tables v2.1.0 had, as text, for before/after
/// comparisons.
pub async fn other_tables_snapshot(pool: &SqlitePool) -> Vec<String> {
    let mut rows = Vec::new();
    for (table, row) in [
        (
            "books",
            "id || '|' || name || '|' || testament || '|' || chapter_count",
        ),
        ("translations", "id || '|' || code || '|' || name"),
        (
            "reading_position",
            "id || '|' || translation_id || '|' || book_id || '|' || chapter",
        ),
        (
            "cross_references",
            "book_id || '|' || chapter || '|' || verse || '|' || ref_book_id || '|' || \
             ref_chapter || '|' || ref_verse || '|' || COALESCE(ref_end_verse, '') || '|' || score",
        ),
    ] {
        // Table and column names are the constants just above.
        let found: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT '{table}: ' || {row} FROM {table} ORDER BY rowid"
        )))
        .fetch_all(pool)
        .await
        .expect("snapshot table");
        rows.extend(found.into_iter().map(|(r,)| r));
    }
    rows
}

/// The names of every table and virtual table in the database.
pub async fn table_names(pool: &SqlitePool) -> Vec<String> {
    sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .fetch_all(pool)
        .await
        .expect("list tables")
}
