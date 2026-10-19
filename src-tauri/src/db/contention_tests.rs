//! The database under contention, as on a fresh install: the first-run
//! search index build runs in the background (as `spawn_search_index_build`
//! starts it) while the reader's first reads and writes happen: the page's
//! boot reading a chapter and saving the reading position, then a bookmark,
//! a highlight and a note. Neither side may fail on the other's locks.

use std::sync::Arc;
use std::sync::Mutex;

use sqlx::SqlitePool;

use super::bookmarks::add_bookmark;
use super::highlights::set_highlights;
use super::notes::{Passage, add_note};
use super::test_support::TempDb;
use super::{BUSY_TIMEOUT, HighlightColor, ensure_search_index, prepare, save_reading_position};

const KJV: i64 = 1;
const GENESIS: i64 = 1;

/// Whether an error is SQLite refusing for a lock ("database is locked",
/// SQLITE_BUSY), as opposed to any other failure.
fn is_lock_error(message: &str) -> bool {
    message.contains("database is locked") || message.contains("(code: 5)")
}

/// The pause between a reader's rounds. A person reads and clicks far more
/// slowly; this keeps the writes coming many times a second, as a busy
/// boot would, without one test loop hogging the write lock back to back
/// (which no reader does, and which would starve any other writer).
const BETWEEN_ROUNDS: std::time::Duration = std::time::Duration::from_millis(25);

/// What one reader does, round after round, until the build has finished:
/// read a chapter and remember it (the page's `get_reading`), then mark it.
async fn reader(
    pool: SqlitePool,
    lane: i64,
    done: Arc<Mutex<bool>>,
    failures: Arc<Mutex<Vec<String>>>,
) {
    let mut round: i64 = 0;
    loop {
        if done.lock().map(|d| *d).unwrap_or(true) {
            return;
        }
        round += 1;
        if round > 1 {
            tokio::time::sleep(BETWEEN_ROUNDS).await;
        }
        // Genesis has 50 chapters; each lane keeps to its own verses so
        // nothing is a duplicate of another lane's.
        let chapter = 1 + (round % 50);
        let verse = 1 + lane;
        let mut errors: Vec<String> = Vec::new();

        let read: Result<Vec<(i64, String)>, _> = sqlx::query_as(
            "SELECT verse, text FROM verses WHERE translation_id = $1 AND book_id = $2 AND chapter = $3",
        )
        .bind(KJV)
        .bind(GENESIS)
        .bind(chapter)
        .fetch_all(&pool)
        .await;
        if let Err(e) = read {
            errors.push(format!("reading Genesis {chapter}: {e}"));
        }
        if let Err(e) = save_reading_position(&pool, KJV, GENESIS, chapter).await {
            errors.push(format!("saving the reading position: {e:#}"));
        }
        if let Err(e) = add_bookmark(&pool, GENESIS, chapter, verse, verse).await {
            errors.push(format!("adding a bookmark: {e}"));
        }
        if let Err(e) = set_highlights(
            &pool,
            GENESIS,
            chapter,
            verse,
            verse,
            HighlightColor::Yellow,
        )
        .await
        {
            errors.push(format!("highlighting: {e}"));
        }
        let passage = Passage {
            book_id: GENESIS,
            chapter,
            verse_start: verse,
            verse_end: verse,
        };
        if let Err(e) = add_note(
            &pool,
            &passage,
            "A note written during the first index build.",
            None,
        )
        .await
        {
            errors.push(format!("adding a note: {e}"));
        }
        if !errors.is_empty()
            && let Ok(mut failures) = failures.lock()
        {
            failures.extend(
                errors
                    .into_iter()
                    .map(|e| format!("lane {lane}, round {round}: {e}")),
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_first_search_index_build_and_the_readers_writes_never_lock_each_other_out() {
    let db = TempDb::new("contention");
    let pool = db.connect().await;
    prepare(&pool).await.expect("prepare the database");

    let done = Arc::new(Mutex::new(false));
    let failures = Arc::new(Mutex::new(Vec::new()));
    let build = tokio::spawn({
        let pool = pool.clone();
        async move { ensure_search_index(&pool).await }
    });
    let readers: Vec<_> = (0..3)
        .map(|lane| tokio::spawn(reader(pool.clone(), lane, done.clone(), failures.clone())))
        .collect();

    let built = build.await.expect("the build task");
    if let Ok(mut d) = done.lock() {
        *d = true;
    }
    for reader in readers {
        reader.await.expect("a reader task");
    }

    let failures = failures.lock().map(|f| f.clone()).unwrap_or_default();
    let locked = failures.iter().filter(|f| is_lock_error(f)).count();
    assert!(
        built.is_ok(),
        "the search index build failed: {:#}",
        built.err().map(|e| format!("{e:#}")).unwrap_or_default()
    );
    assert!(
        failures.is_empty(),
        "{} of the reader's reads and writes failed ({locked} for a lock), e.g. {:?}",
        failures.len(),
        failures.iter().take(5).collect::<Vec<_>>()
    );
}

/// Every connection the app opens writes ahead to a log (readers and the
/// writer don't wait for each other), syncs as WAL allows, and waits for a
/// lock rather than failing at once.
#[tokio::test]
async fn every_connection_uses_wal_and_waits_for_locks() {
    let db = TempDb::new("pragmas");
    let pool = db.connect().await;
    // Held together, so each is a different connection.
    let mut connections = Vec::new();
    for _ in 0..3 {
        connections.push(pool.acquire().await.expect("a connection"));
    }
    for conn in &mut connections {
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&mut **conn)
            .await
            .expect("journal_mode");
        assert_eq!(mode, "wal");
        let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
            .fetch_one(&mut **conn)
            .await
            .expect("synchronous");
        assert_eq!(synchronous, 1, "NORMAL");
        let timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
            .fetch_one(&mut **conn)
            .await
            .expect("busy_timeout");
        assert_eq!(u128::try_from(timeout).ok(), Some(BUSY_TIMEOUT.as_millis()));
    }
}
