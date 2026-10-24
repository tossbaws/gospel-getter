//! Disposable databases for tests: each one a fresh file under the OS temp
//! directory, never anywhere near a real install, deleted when dropped.
//! Includes a stand-in for a database left by v2.1.0 (before bookmarks and
//! search existed), built from that release's exact schema.
//!
//! Preparing a database (seeding three translations and the
//! cross-references, then building the search index) takes seconds, so it's
//! done once per test binary, into template files (see [`Templates`]), and
//! each test that wants a prepared database gets its own copy of one.

use serde::Deserialize;
use sqlx::SqlitePool;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::db::{Db, WritePool, begin_write, create_pool, ensure_search_index, prepare};

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

    /// Open (creating if needed) this database's connections, as the app
    /// does: the read pool and the one writer.
    pub async fn connect(&self) -> Db {
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
/// the search index built: a copy of the [`Template::Indexed`] template.
pub async fn fresh_database(label: &str) -> (TempDb, Db) {
    copy_of(Template::Indexed, label).await
}

/// A database prepared exactly as the app prepares one at startup, before
/// the search index is built: a copy of the [`Template::Prepared`]
/// template.
pub async fn prepared_database(label: &str) -> (TempDb, Db) {
    copy_of(Template::Prepared, label).await
}

/// The template databases, each a stage of what the app does to a new
/// database at startup. `db/tests.rs` checks a copy of each against a
/// database prepared then and there, so they can't drift from it.
#[derive(Clone, Copy, Debug)]
enum Template {
    /// After `prepare`: every table made, every bundled translation and the
    /// cross-references seeded.
    Prepared,
    /// After `prepare` and `ensure_search_index`.
    Indexed,
}

/// The template databases, built once per test binary.
struct Templates {
    prepared: PathBuf,
    indexed: PathBuf,
    /// Locked for as long as this process runs, telling a later run's
    /// sweep (see [`claim_templates_dir`]) that these files are in use.
    _in_use: File,
}

impl Templates {
    fn path(&self, template: Template) -> &Path {
        match template {
            Template::Prepared => &self.prepared,
            Template::Indexed => &self.indexed,
        }
    }
}

/// A copy of `template` in a [`TempDb`] of its own, connected as the app
/// connects.
async fn copy_of(template: Template, label: &str) -> (TempDb, Db) {
    let db = TempDb::new(label);
    let path = db.path();
    // Blocking: the first caller builds the templates, and every other
    // caller waits for that build before copying.
    tokio::task::spawn_blocking(move || std::fs::copy(templates().path(template), path))
        .await
        .expect("copy the template database")
        .expect("copy the template database");
    let pool = db.connect().await;
    (db, pool)
}

/// The template databases, built by the first test to want one.
///
/// Tests run on many threads, each with a runtime of its own that ends with
/// the test, and a connection pool only works on the runtime it was opened
/// on. So the build has its own thread and runtime, and whatever thread
/// asks first, every other caller blocks in `get_or_init` until it's done.
fn templates() -> &'static Templates {
    static TEMPLATES: OnceLock<Templates> = OnceLock::new();
    TEMPLATES.get_or_init(|| {
        std::thread::spawn(build_templates)
            .join()
            .expect("build the template databases")
    })
}

/// Prepare a database as startup does, keeping a copy after `prepare` and
/// another once the search index is built, in this run's own directory.
fn build_templates() -> Templates {
    let (dir, in_use) = claim_templates_dir(&templates_root());
    let prepared = dir.join("prepared.db");
    let indexed = dir.join("indexed.db");
    let work = dir.join("work.db");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime for the template build");
    runtime.block_on(async {
        let db = create_pool(&format!("sqlite:{}", work.display()))
            .await
            .expect("create the template database");
        prepare(db.write())
            .await
            .expect("prepare the template database");
        vacuum_into(&db, &prepared).await;
        ensure_search_index(&db)
            .await
            .expect("build the template's search index");
        vacuum_into(&db, &indexed).await;
        db.close().await;
    });
    for name in ["work.db", "work.db-wal", "work.db-shm"] {
        let _ = std::fs::remove_file(dir.join(name));
    }
    Templates {
        prepared,
        indexed,
        _in_use: in_use,
    }
}

/// Write a copy of the database to `path` with `VACUUM INTO`: one
/// self-contained file, everything committed in it, no write-ahead log or
/// shared memory file beside it, so a plain file copy of it is a whole
/// database.
async fn vacuum_into(db: &Db, path: &Path) {
    sqlx::query("VACUUM INTO $1")
        .bind(path.to_str().expect("a UTF-8 template path"))
        .execute(db.write().pool())
        .await
        .expect("write a template database");
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut beside = path.as_os_str().to_owned();
        beside.push(suffix);
        assert!(
            !Path::new(&beside).exists(),
            "a template has a {suffix} file beside it"
        );
    }
}

/// Where every run's templates directory goes: cargo's `target/tmp` when
/// the test binary runs from `target/<profile>/deps`, as under `cargo
/// test`, otherwise the OS temp directory (often held in memory, where
/// these files are better kept out of).
fn templates_root() -> PathBuf {
    let target_tmp = std::env::current_exe().ok().and_then(|exe| {
        let deps = exe.parent()?;
        let target = deps.parent()?.parent()?;
        (deps.file_name()? == "deps").then(|| target.join("tmp"))
    });
    target_tmp
        .unwrap_or_else(std::env::temp_dir)
        .join("gospel_getter_db_templates")
}

/// Make this process's templates directory under `root` (named for the
/// process id), and remove any that earlier runs left: a test binary has no
/// hook to run at exit, so each run clears up after the ones before it, and
/// at most the last run's templates are ever left behind.
///
/// A directory `<id>` is in use for as long as its process holds the lock
/// on `<id>.lock`, which the OS releases however the process ends. Taking
/// that lock and sweeping both happen under `sweep.lock`, so a sweep never
/// finds a lock file before its owner has locked it, and another test
/// binary running at the same time keeps its templates.
fn claim_templates_dir(root: &Path) -> (PathBuf, File) {
    std::fs::create_dir_all(root).expect("create the templates directory");
    let sweep = open_lock_file(&root.join("sweep.lock")).expect("open the sweep lock");
    sweep.lock().expect("lock the templates directory");

    let own = std::process::id().to_string();
    let in_use = open_lock_file(&root.join(format!("{own}.lock"))).expect("open this run's lock");
    in_use.lock().expect("lock this run's templates");

    for entry in std::fs::read_dir(root)
        .expect("list the templates directory")
        .flatten()
    {
        let lock_path = entry.path();
        if lock_path.extension().is_none_or(|e| e != "lock") {
            continue;
        }
        let Some(id) = lock_path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if id == "sweep" || id == own {
            continue;
        }
        let Ok(other) = open_lock_file(&lock_path) else {
            continue;
        };
        if other.try_lock().is_ok() {
            // Its process has ended.
            drop(other);
            let _ = std::fs::remove_dir_all(root.join(id));
            let _ = std::fs::remove_file(&lock_path);
        }
    }

    let dir = root.join(&own);
    // An ended process's, if this one has its id.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create this run's templates directory");
    drop(sweep);
    (dir, in_use)
}

/// Open (creating if needed) a file that's only ever locked, never
/// written. Not truncated: on Windows, truncating a file another process
/// has locked fails.
fn open_lock_file(path: &Path) -> std::io::Result<File> {
    File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
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

/// The bundled BSB's book: `RawBook`'s shape, with `None` for a verse it
/// doesn't number, and its footnotes.
#[derive(Deserialize)]
pub struct RawBsbBook {
    pub name: String,
    pub testament: String,
    pub chapters: Vec<Vec<Option<String>>>,
    pub footnotes: Vec<RawBsbFootnote>,
}

/// One of the bundled BSB's footnotes (see `domain::usfm_note`).
#[derive(Deserialize)]
pub struct RawBsbFootnote {
    pub chapter: i64,
    pub verse: i64,
    pub at: usize,
    pub usfm: String,
}

/// The bundled BSB, parsed: the exact strings the seed step stores.
pub fn bundled_bsb() -> Vec<RawBsbBook> {
    serde_json::from_str(include_str!("../../data/bsb.json")).expect("bundled BSB JSON")
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
            .execute(pool.write().pool())
            .await
            .expect("v2.1.0 schema");
    }

    let mut tx = begin_write(pool.write()).await.expect("begin");
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

/// Add `count` distinct notes of exactly `chars` characters each (at least
/// 7), spread over chapter 1, verse 1 of every book, in one statement —
/// for filling a database up to the notes' limits quickly.
pub async fn fill_notes(writer: &WritePool, count: usize, chars: usize) {
    assert!(chars >= 7, "each note starts with a 6-digit number");
    sqlx::query(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < $1) \
         INSERT INTO notes (book_id, chapter, verse_start, verse_end, body) \
         SELECT 1 + i % 66, 1, 1, 1, \
             printf('%06d', i) || substr(replace(hex(zeroblob($2)), '0', 'x'), 1, $2 - 6) \
         FROM n",
    )
    .bind(count as i64)
    .bind(chars as i64)
    .execute(writer.pool())
    .await
    .expect("fill notes");
}
