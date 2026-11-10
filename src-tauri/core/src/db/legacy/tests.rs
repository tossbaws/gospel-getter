//! The legacy migration against real SQLite databases: one the old app has
//! open and is writing to, one it closed, files that aren't Gospel Getter
//! databases, a destination that can't be written, an attempt cut short,
//! and two first launches at once. Every legacy database is checked to be
//! left as it was.

use super::*;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteJournalMode, SqlitePoolOptions};
use std::collections::BTreeMap;
use std::fs;

/// A fresh, isolated scratch directory under the OS temp dir — never
/// anywhere near a real install's data — cleaned up at the end of each
/// test regardless of outcome.
struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gospel_getter_legacy_migration_test_{label}_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("legacy")).expect("create scratch dir");
        Self(dir)
    }

    /// Where the old install kept its database.
    fn legacy(&self) -> PathBuf {
        self.0.join("legacy").join("gospel_getter.db")
    }

    /// The app's own data directory (not created: the migration does that).
    fn app_dir(&self) -> PathBuf {
        self.0.join("app_data")
    }

    fn new_db(&self) -> PathBuf {
        self.app_dir().join("gospel_getter.db")
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The pre-Tauri app's first schema, whose tables every later version
/// kept, with the reader left in John 3 (KJV).
const LEGACY_SCHEMA: &str = "
    CREATE TABLE books (id INTEGER PRIMARY KEY, name TEXT NOT NULL,
        testament TEXT NOT NULL CHECK (testament IN ('OT', 'NT')), chapter_count INTEGER NOT NULL);
    CREATE TABLE translations (id INTEGER PRIMARY KEY, code TEXT NOT NULL UNIQUE, name TEXT NOT NULL);
    CREATE TABLE verses (translation_id INTEGER NOT NULL REFERENCES translations(id),
        book_id INTEGER NOT NULL REFERENCES books(id), chapter INTEGER NOT NULL,
        verse INTEGER NOT NULL, text TEXT NOT NULL,
        PRIMARY KEY (translation_id, book_id, chapter, verse));
    CREATE TABLE reading_position (id INTEGER PRIMARY KEY CHECK (id = 1),
        translation_id INTEGER NOT NULL REFERENCES translations(id),
        book_id INTEGER NOT NULL REFERENCES books(id), chapter INTEGER NOT NULL);
    INSERT INTO translations VALUES (1, 'kjv', 'King James Version');
    INSERT INTO books VALUES (19, 'Psalms', 'OT', 150), (43, 'John', 'NT', 21);
    INSERT INTO reading_position VALUES (1, 1, 43, 3);
";

/// The old app's connection to its database, as it would have had it open:
/// one connection, in `mode`, never checkpointing on its own, so in WAL
/// mode what it commits stays in the write-ahead log.
async fn old_app(path: &Path, mode: SqliteJournalMode) -> SqlitePool {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(mode)
        .busy_timeout(BUSY_TIMEOUT)
        .pragma("wal_autocheckpoint", "0");
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("open the legacy database")
}

/// A legacy database with the reader's place, closed again.
async fn closed_legacy(path: &Path, mode: SqliteJournalMode) {
    let pool = old_app(path, mode).await;
    sqlx::raw_sql(LEGACY_SCHEMA)
        .execute(&pool)
        .await
        .expect("legacy schema");
    pool.close().await;
}

/// Where `db` has the reader: (translation, book, chapter).
async fn reading_position(db: &Path) -> Option<(i64, i64, i64)> {
    let options = SqliteConnectOptions::new().filename(db).read_only(true);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("open read-only");
    let position = sqlx::query_as(
        "SELECT translation_id, book_id, chapter FROM reading_position WHERE id = 1",
    )
    .fetch_optional(&mut connection)
    .await
    .expect("read the position");
    connection.close().await.expect("close");
    position
}

/// Every file in `dir`, by name, with its bytes.
fn files_in(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(dir)
        .expect("list the directory")
        .map(|entry| entry.expect("an entry"))
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (name, fs::read(entry.path()).expect("read a file"))
        })
        .collect()
}

/// The names of the files in `dir`.
fn names_in(dir: &Path) -> Vec<String> {
    files_in(dir).into_keys().collect()
}

/// The old app is open, in WAL mode, with everything it has committed still
/// only in its write-ahead log (copying the database file alone would bring
/// none of it), and a write of its own under way. What comes across is
/// everything committed, and not the write under way; the old database and
/// its log are exactly as they were, and the old app's write still goes
/// through after.
#[tokio::test]
async fn committed_data_only_in_the_wal_comes_across_while_the_old_app_writes() {
    let dir = ScratchDir::new("wal");
    let legacy = dir.legacy();
    let pool = old_app(&legacy, SqliteJournalMode::Wal).await;
    sqlx::raw_sql(LEGACY_SCHEMA)
        .execute(&pool)
        .await
        .expect("legacy schema");

    // The database file alone doesn't have the reader's place yet.
    let main_only = dir.0.join("main-only.db");
    fs::copy(&legacy, &main_only).expect("copy the file alone");
    let options = SqliteConnectOptions::new()
        .filename(&main_only)
        .read_only(true)
        .immutable(true);
    let mut alone = SqliteConnection::connect_with(&options)
        .await
        .expect("open it");
    let tables: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master")
        .fetch_one(&mut alone)
        .await
        .expect("count tables");
    alone.close().await.expect("close");
    assert_eq!(tables, 0, "the schema should be only in the WAL");

    // The old app starts a write and hasn't committed it.
    let mut writing = pool.begin_with("BEGIN IMMEDIATE").await.expect("begin");
    sqlx::query("UPDATE reading_position SET book_id = 19, chapter = 23")
        .execute(&mut *writing)
        .await
        .expect("update");

    // The -shm file is SQLite's shared-memory index of the log, which every
    // connection (a reader too) works in; it's not part of the database.
    let untouched = |files: BTreeMap<String, Vec<u8>>| {
        files
            .into_iter()
            .filter(|(name, _)| !name.ends_with("-shm"))
            .collect::<BTreeMap<_, _>>()
    };
    let before = untouched(files_in(legacy.parent().expect("a folder")));
    assert!(before.contains_key("gospel_getter.db-wal"));

    assert!(
        migrate_legacy_db_if_needed(&dir.new_db(), &legacy)
            .await
            .expect("migrate")
    );

    assert_eq!(
        untouched(files_in(legacy.parent().expect("a folder"))),
        before,
        "the legacy database and its log must be left exactly as they were"
    );
    assert_eq!(reading_position(&dir.new_db()).await, Some((1, 43, 3)));
    assert_eq!(names_in(&dir.app_dir()), ["gospel_getter.db"]);

    // The old app is none the worse.
    writing.commit().await.expect("the old app's write commits");
    let now: (i64, i64) = sqlx::query_as("SELECT book_id, chapter FROM reading_position")
        .fetch_one(&pool)
        .await
        .expect("read back");
    assert_eq!(now, (19, 23));
    pool.close().await;
    assert_eq!(reading_position(&dir.new_db()).await, Some((1, 43, 3)));
}

/// The pre-Tauri app's own journal mode (sqlx's default, a rollback
/// journal), with a write under way: what comes across is what was
/// committed.
#[tokio::test]
async fn an_old_app_writing_in_rollback_mode_gives_what_it_committed() {
    let dir = ScratchDir::new("rollback");
    let legacy = dir.legacy();
    let pool = old_app(&legacy, SqliteJournalMode::Delete).await;
    sqlx::raw_sql(LEGACY_SCHEMA)
        .execute(&pool)
        .await
        .expect("legacy schema");
    let mut writing = pool.begin_with("BEGIN IMMEDIATE").await.expect("begin");
    sqlx::query("UPDATE reading_position SET book_id = 19, chapter = 23")
        .execute(&mut *writing)
        .await
        .expect("update");
    let before = fs::read(&legacy).expect("read");

    assert!(
        migrate_legacy_db_if_needed(&dir.new_db(), &legacy)
            .await
            .expect("migrate")
    );

    assert_eq!(fs::read(&legacy).expect("read"), before);
    assert_eq!(reading_position(&dir.new_db()).await, Some((1, 43, 3)));
    writing.commit().await.expect("the old app's write commits");
    pool.close().await;
}

/// Closed, the old database is read and left exactly as it was: no file
/// beside it is made or left behind, and nothing is left beside the new one
/// but the database.
#[tokio::test]
async fn a_closed_legacy_database_is_left_exactly_as_it_was() {
    for (label, mode) in [
        ("closed_wal", SqliteJournalMode::Wal),
        ("closed_rollback", SqliteJournalMode::Delete),
    ] {
        let dir = ScratchDir::new(label);
        let legacy = dir.legacy();
        closed_legacy(&legacy, mode).await;
        let before = files_in(legacy.parent().expect("a folder"));
        assert_eq!(before.keys().collect::<Vec<_>>(), ["gospel_getter.db"]);

        assert!(
            migrate_legacy_db_if_needed(&dir.new_db(), &legacy)
                .await
                .expect("migrate")
        );

        assert_eq!(files_in(legacy.parent().expect("a folder")), before);
        assert_eq!(reading_position(&dir.new_db()).await, Some((1, 43, 3)));
        assert_eq!(names_in(&dir.app_dir()), ["gospel_getter.db"]);
    }
}

/// Whatever fails, nothing is at the new path (so the app doesn't open it
/// as the migrated database), nothing is left beside it, and the legacy
/// file is as it was.
async fn assert_failed_cleanly(dir: &ScratchDir, legacy_before: &[u8], why: &str) {
    let failed = migrate_legacy_db_if_needed(&dir.new_db(), &dir.legacy()).await;
    let e = failed.expect_err("the migration should fail");
    assert!(format!("{e:#}").contains(why), "{e:#}");
    assert!(!dir.new_db().exists(), "nothing may be published: {e:#}");
    assert_eq!(names_in(&dir.app_dir()), Vec::<String>::new());
    assert_eq!(fs::read(dir.legacy()).expect("read"), legacy_before);
}

/// A file that isn't a database, or a database that isn't Gospel Getter's,
/// is never put in place; once the real one is there, the next launch
/// brings it over.
#[tokio::test]
async fn a_file_that_is_not_a_gospel_getter_database_is_never_put_in_place() {
    let dir = ScratchDir::new("not_ours");
    let legacy = dir.legacy();

    fs::write(&legacy, b"legacy bytes, not a database").expect("write");
    assert_failed_cleanly(&dir, b"legacy bytes, not a database", "Failed to snapshot").await;

    fs::remove_file(&legacy).expect("remove");
    let pool = old_app(&legacy, SqliteJournalMode::Delete).await;
    sqlx::query("CREATE TABLE something_else (x)")
        .execute(&pool)
        .await
        .expect("create");
    pool.close().await;
    let other = fs::read(&legacy).expect("read");
    assert_failed_cleanly(
        &dir,
        &other,
        "It isn't a Gospel Getter database: it has no books, translations, verses, \
         reading_position table",
    )
    .await;

    fs::remove_file(&legacy).expect("remove");
    closed_legacy(&legacy, SqliteJournalMode::Delete).await;
    assert!(
        migrate_legacy_db_if_needed(&dir.new_db(), &legacy)
            .await
            .expect("the retry")
    );
    assert_eq!(reading_position(&dir.new_db()).await, Some((1, 43, 3)));
}

/// A write-ahead log with data in it but no index (`-shm`) beside it, as a
/// database copied without it would be, can't be read without making one:
/// it's refused, nothing beside it is made, and nothing is published.
#[tokio::test]
async fn a_log_without_its_index_is_refused_and_nothing_is_made_beside_it() {
    let dir = ScratchDir::new("no_index");
    let open = dir.0.join("open");
    fs::create_dir_all(&open).expect("create");
    let pool = old_app(&open.join("gospel_getter.db"), SqliteJournalMode::Wal).await;
    sqlx::raw_sql(LEGACY_SCHEMA)
        .execute(&pool)
        .await
        .expect("legacy schema");
    for name in ["gospel_getter.db", "gospel_getter.db-wal"] {
        fs::copy(open.join(name), dir.legacy().with_file_name(name)).expect("copy");
    }
    pool.close().await;
    let before = files_in(dir.legacy().parent().expect("a folder"));

    let e = migrate_legacy_db_if_needed(&dir.new_db(), &dir.legacy())
        .await
        .expect_err("refused");
    assert!(
        format!("{e:#}").contains("has a write-ahead log but no index for it"),
        "{e:#}"
    );
    assert_eq!(files_in(dir.legacy().parent().expect("a folder")), before);
    assert!(!dir.new_db().exists());
    assert_eq!(names_in(&dir.app_dir()), Vec::<String>::new());
}

/// The app's data folder can't be written: nothing is published or left
/// behind, and once it can be, the next launch brings the database over.
#[cfg(unix)]
#[tokio::test]
async fn a_destination_that_cannot_be_written_publishes_nothing_and_a_later_launch_retries() {
    use std::os::unix::fs::PermissionsExt;
    let dir = ScratchDir::new("read_only_destination");
    let legacy = dir.legacy();
    closed_legacy(&legacy, SqliteJournalMode::Delete).await;
    let before = fs::read(&legacy).expect("read");
    fs::create_dir_all(dir.app_dir()).expect("create");
    fs::set_permissions(dir.app_dir(), fs::Permissions::from_mode(0o555)).expect("chmod");

    // Root writes anyway, and then there's nothing to test.
    let probe = dir.app_dir().join("probe");
    if fs::write(&probe, b"").is_ok() {
        fs::remove_file(&probe).expect("remove the probe");
        eprintln!("skipped: this user can write a read-only folder");
    } else {
        assert_failed_cleanly(&dir, &before, "Failed to create").await;
    }

    fs::set_permissions(dir.app_dir(), fs::Permissions::from_mode(0o755)).expect("chmod");
    assert!(
        migrate_legacy_db_if_needed(&dir.new_db(), &legacy)
            .await
            .expect("the retry")
    );
    assert_eq!(reading_position(&dir.new_db()).await, Some((1, 43, 3)));
}

/// An attempt that stopped part-way (the app quit, or the computer did)
/// left only its temporary file, under a name of its own: never the new
/// database. The next launch brings the database over as if it weren't
/// there, and leaves another launch's file alone.
#[tokio::test]
async fn an_attempt_cut_short_leaves_nothing_the_next_launch_mistakes_for_the_database() {
    let dir = ScratchDir::new("cut_short");
    let legacy = dir.legacy();
    closed_legacy(&legacy, SqliteJournalMode::Delete).await;
    fs::create_dir_all(dir.app_dir()).expect("create");
    let left = ".1-0-0.legacy-migration.tmp";
    fs::write(dir.app_dir().join(left), b"half a snapshot").expect("write");

    assert!(
        migrate_legacy_db_if_needed(&dir.new_db(), &legacy)
            .await
            .expect("migrate")
    );
    assert_eq!(reading_position(&dir.new_db()).await, Some((1, 43, 3)));
    assert_eq!(names_in(&dir.app_dir()), [left, "gospel_getter.db"]);
}

/// Two first launches at once: each takes its own snapshot, the first to
/// publish wins, and the other's snapshot goes without touching it. Step by
/// step, then for real, at the same time.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_first_launches_at_once_put_one_snapshot_in_place() {
    let dir = ScratchDir::new("race");
    let legacy = dir.legacy();
    closed_legacy(&legacy, SqliteJournalMode::Wal).await;
    let before = fs::read(&legacy).expect("read");
    fs::create_dir_all(dir.app_dir()).expect("create");

    let first = Snapshot::take(&legacy, &dir.app_dir())
        .await
        .expect("first");
    let second = Snapshot::take(&legacy, &dir.app_dir())
        .await
        .expect("second");
    first.check().await.expect("first is whole");
    second.check().await.expect("second is whole");
    assert!(first.publish(&dir.new_db()).expect("first publishes"));
    let published = fs::read(dir.new_db()).expect("read");
    assert!(!second.publish(&dir.new_db()).expect("second stands down"));
    assert_eq!(fs::read(dir.new_db()).expect("read"), published);
    assert_eq!(names_in(&dir.app_dir()), ["gospel_getter.db"]);

    fs::remove_file(dir.new_db()).expect("remove");
    let new_db = dir.new_db();
    let (a, b) = tokio::join!(
        migrate_legacy_db_if_needed(&new_db, &legacy),
        migrate_legacy_db_if_needed(&new_db, &legacy),
    );
    let (a, b) = (a.expect("one launch"), b.expect("the other"));
    assert!(a != b, "exactly one launch puts it in place: {a}, {b}");
    assert_eq!(reading_position(&dir.new_db()).await, Some((1, 43, 3)));
    assert_eq!(names_in(&dir.app_dir()), ["gospel_getter.db"]);
    assert_eq!(fs::read(&legacy).expect("read"), before);
}

#[tokio::test]
async fn does_not_overwrite_an_existing_new_db() {
    let dir = ScratchDir::new("no_overwrite");
    closed_legacy(&dir.legacy(), SqliteJournalMode::Delete).await;
    fs::create_dir_all(dir.app_dir()).expect("create");
    fs::write(dir.new_db(), b"already migrated").expect("write");

    let copied = migrate_legacy_db_if_needed(&dir.new_db(), &dir.legacy())
        .await
        .expect("migrate");

    assert!(
        !copied,
        "should not report a copy when the new db already exists"
    );
    assert_eq!(
        fs::read(dir.new_db()).expect("read"),
        b"already migrated",
        "an existing new db must never be overwritten"
    );
    assert_eq!(names_in(&dir.app_dir()), ["gospel_getter.db"]);
}

#[tokio::test]
async fn does_nothing_when_neither_db_exists() {
    let dir = ScratchDir::new("neither");

    let copied = migrate_legacy_db_if_needed(&dir.new_db(), &dir.legacy())
        .await
        .expect("migrate");

    assert!(!copied);
    assert!(!dir.new_db().exists());
    assert!(!dir.app_dir().exists());
}
