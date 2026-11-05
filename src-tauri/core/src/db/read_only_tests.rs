//! `open_read_only`: what a program beside the app gets. It sees everything
//! the app has committed, whether or not the app is running, and can neither
//! write nor create a file.

use std::collections::BTreeMap;
use std::path::Path;

use super::test_support::{TempDb, fresh_database, prepared_database};
use super::{begin_write, open_read_only};
use crate::db::search::{search_index_is_current, search_verses, search_verses_in_book};
use crate::domain::query::SearchQuery;

const KJV: i64 = 1;
const ROMANS: i64 = 45;

/// Every file in the database's folder, with its bytes.
fn files_beside(db: &TempDb) -> BTreeMap<String, Vec<u8>> {
    let dir = db.path();
    let dir = dir.parent().expect("a folder");
    std::fs::read_dir(dir)
        .expect("list the folder")
        .map(|entry| {
            let entry = entry.expect("an entry");
            let bytes = std::fs::read(entry.path()).expect("read the file");
            (entry.file_name().to_string_lossy().into_owned(), bytes)
        })
        .collect()
}

async fn note_count(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM notes")
        .fetch_one(pool)
        .await
        .expect("count notes")
}

const ADD_NOTE: &str = "INSERT INTO notes (book_id, chapter, verse_start, verse_end, body) \
                        VALUES (43, 3, 16, 16, 'A note')";

async fn writing_fails(pool: &sqlx::SqlitePool) {
    let error = sqlx::query(ADD_NOTE)
        .execute(pool)
        .await
        .expect_err("a read-only connection refuses to write");
    assert!(error.to_string().contains("readonly"), "{error}");
}

/// The app has closed: no `-wal`, no `-shm`. Reading creates neither, changes
/// nothing, and can't write.
#[tokio::test]
async fn a_closed_database_is_read_without_creating_a_file() {
    let (db, pool) = fresh_database("read_only_closed").await;
    pool.close().await;
    let before = files_beside(&db);
    assert_eq!(before.len(), 1, "{:?}", before.keys());

    let reader = open_read_only(&db.path()).await.expect("open");
    assert_eq!(note_count(&reader).await, 0);
    let (total, _) = search_verses(&reader, KJV, &SearchQuery::of_words(&["grace"]), 1, 0)
        .await
        .expect("full-text search reads");
    assert!(total > 0);
    writing_fails(&reader).await;
    reader.close().await;

    assert_eq!(files_beside(&db), before);
}

/// The app is running, mid-write. The read doesn't wait, and sees what's
/// committed (even if only in `-wal`), not what isn't. The database's own file
/// and its `-wal` stay as they were.
#[tokio::test]
async fn a_running_apps_database_is_read_through_a_write_in_progress() {
    let (db, pool) = fresh_database("read_only_open").await;
    sqlx::query(ADD_NOTE)
        .execute(pool.write().pool())
        .await
        .expect("a committed note");
    let mut uncommitted = begin_write(pool.write()).await.expect("the write lock");
    sqlx::query(ADD_NOTE)
        .execute(&mut *uncommitted)
        .await
        .expect("an uncommitted note");

    let wal = std::path::PathBuf::from(format!("{}-wal", db.path().display()));
    let file_before = std::fs::read(db.path()).expect("the database");
    let wal_before = std::fs::read(&wal).expect("its log");

    let reader = open_read_only(&db.path()).await.expect("open");
    assert_eq!(note_count(&reader).await, 1);
    writing_fails(&reader).await;
    reader.close().await;

    assert_eq!(std::fs::read(db.path()).expect("the database"), file_before);
    assert_eq!(std::fs::read(&wal).expect("its log"), wal_before);
    uncommitted.rollback().await.expect("roll back");
    pool.close().await;
}

/// The app was killed: its `-wal` and `-shm` are left behind, with no one
/// using them. What was committed is read, and nothing is checkpointed.
#[tokio::test]
async fn a_crashed_apps_log_is_read_and_left_alone() {
    let (db, pool) = fresh_database("read_only_crash").await;
    sqlx::query(ADD_NOTE)
        .execute(pool.write().pool())
        .await
        .expect("a committed note");
    let copy = TempDb::new("read_only_crash_copy");
    for suffix in ["", "-wal", "-shm"] {
        std::fs::copy(
            format!("{}{suffix}", db.path().display()),
            format!("{}{suffix}", copy.path().display()),
        )
        .expect("copy the files as a crash leaves them");
    }
    pool.close().await;
    let wal = format!("{}-wal", copy.path().display());
    let file_before = std::fs::read(copy.path()).expect("the database");
    let wal_before = std::fs::read(&wal).expect("its log");

    let reader = open_read_only(&copy.path()).await.expect("open");
    assert_eq!(note_count(&reader).await, 1);
    reader.close().await;

    assert_eq!(
        std::fs::read(copy.path()).expect("the database"),
        file_before
    );
    assert_eq!(std::fs::read(&wal).expect("its log"), wal_before);
}

/// A `-wal` with data but no `-shm` can't be read without creating one, so
/// it's refused; an empty one has nothing to read.
#[tokio::test]
async fn a_log_without_its_index_is_refused_unless_it_is_empty() {
    let (db, pool) = fresh_database("read_only_no_shm").await;
    pool.close().await;
    let wal = format!("{}-wal", db.path().display());

    std::fs::write(&wal, b"").expect("an empty log");
    let reader = open_read_only(&db.path()).await.expect("open");
    assert_eq!(note_count(&reader).await, 0);
    reader.close().await;
    assert!(!Path::new(&format!("{}-shm", db.path().display())).exists());

    std::fs::write(&wal, b"not empty").expect("a log with data");
    let error = open_read_only(&db.path()).await.expect_err("refused");
    assert!(
        error.to_string().contains("Open Gospel Getter once"),
        "{error}"
    );
    assert!(!Path::new(&format!("{}-shm", db.path().display())).exists());
}

/// A database that isn't there isn't created.
#[tokio::test]
async fn a_missing_database_is_an_error_and_is_not_created() {
    let db = TempDb::new("read_only_missing");
    open_read_only(&db.path()).await.expect_err("nothing there");
    assert!(files_beside(&db).is_empty());
}

/// The index counts as built only when it covers the translation, in the
/// current kind of content.
#[tokio::test]
async fn the_search_index_is_current_only_once_built() {
    let (_db, pool) = prepared_database("read_only_unindexed").await;
    assert!(!search_index_is_current(pool.read(), KJV).await.unwrap());
    pool.close().await;

    let (_db, pool) = fresh_database("read_only_indexed").await;
    assert!(search_index_is_current(pool.read(), KJV).await.unwrap());
    sqlx::query("UPDATE search_index_version SET version = 0")
        .execute(pool.write().pool())
        .await
        .unwrap();
    assert!(!search_index_is_current(pool.read(), KJV).await.unwrap());
    pool.close().await;
}

/// A search in one book finds just that book's part of the whole search, the
/// verses and their count, in the same order.
#[tokio::test]
async fn a_search_in_one_book_is_that_books_part_of_the_whole() {
    let (_db, pool) = fresh_database("read_only_in_book").await;
    let grace = SearchQuery::of_words(&["grace"]);
    let (_, all) = search_verses(pool.read(), KJV, &grace, 10_000, 0)
        .await
        .unwrap();
    let expected: Vec<_> = all.into_iter().filter(|h| h.book_id == ROMANS).collect();
    assert!(!expected.is_empty());
    let (total, hits) = search_verses_in_book(pool.read(), KJV, ROMANS, &grace, 10_000, 0)
        .await
        .unwrap();
    assert_eq!(total, expected.len() as i64);
    assert_eq!(hits, expected);

    let (total, hits) = search_verses_in_book(pool.read(), KJV, ROMANS, &grace, 2, 1)
        .await
        .unwrap();
    assert_eq!(total, expected.len() as i64);
    assert_eq!(hits, expected[1..3]);
    pool.close().await;
}
