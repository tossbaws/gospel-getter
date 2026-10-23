//! The database under contention, as on a fresh install: the first-run
//! search index build runs in the background (as `spawn_search_index_build`
//! starts it) while the reader's first reads and writes happen: the page's
//! boot reading a chapter and saving the reading position, then a bookmark,
//! a highlight and a note. Neither side may fail on the other's locks.
//! And the layout that guarantees it: every write waits its turn on the one
//! writing connection, however long the write ahead of it takes, while
//! reads carry on beside it.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::bookmarks::add_bookmark;
use super::highlights::set_highlights;
use super::notes::{Passage, add_note};
use super::test_support::TempDb;
use super::{
    BUSY_TIMEOUT, Db, HighlightColor, begin_write, ensure_search_index, get_reading_position,
    prepare, save_reading_position,
};

const KJV: i64 = 1;
const GENESIS: i64 = 1;

/// A write held open longer than SQLite waits for its lock
/// ([`BUSY_TIMEOUT`]): a write queued behind it on another connection
/// would fail with "database is locked".
const LONG_WRITE: Duration = Duration::from_secs(BUSY_TIMEOUT.as_secs() + 2);

/// How long a test gives a read that shouldn't wait at all, before calling
/// it stuck: far longer than a read takes, even in a slow debug build on a
/// busy machine.
const STUCK: Duration = Duration::from_secs(30);

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
/// read a chapter and remember it (the page's `get_reading`, then its
/// `save_reading_position`), then mark it.
async fn reader(pool: Db, lane: i64, done: Arc<Mutex<bool>>, failures: Arc<Mutex<Vec<String>>>) {
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
        .fetch_all(pool.read())
        .await;
        if let Err(e) = read {
            errors.push(format!("reading Genesis {chapter}: {e}"));
        }
        if let Err(e) = save_reading_position(pool.write(), KJV, GENESIS, chapter).await {
            errors.push(format!("saving the reading position: {e:#}"));
        }
        if let Err(e) = add_bookmark(pool.write(), GENESIS, chapter, verse, verse).await {
            errors.push(format!("adding a bookmark: {e}"));
        }
        if let Err(e) = set_highlights(
            pool.write(),
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
            pool.write(),
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
    prepare(pool.write()).await.expect("prepare the database");

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
        connections.push(pool.read().acquire().await.expect("a connection"));
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

/// Moves Genesis to `chapter` as the reading position, inside a write
/// that's still open.
async fn move_reading_position(tx: &mut sqlx::Transaction<'static, sqlx::Sqlite>, chapter: i64) {
    sqlx::query(
        "INSERT INTO reading_position (id, translation_id, book_id, chapter) \
         VALUES (1, $1, $2, $3) \
         ON CONFLICT(id) DO UPDATE SET chapter = excluded.chapter",
    )
    .bind(KJV)
    .bind(GENESIS)
    .bind(chapter)
    .execute(&mut **tx)
    .await
    .expect("write inside the open transaction");
}

/// A write that arrives while another is held open for longer than SQLite
/// would wait for the lock ([`LONG_WRITE`]) waits its turn on the writing
/// connection, then succeeds. (On two connections it would have failed
/// with "database is locked" after [`BUSY_TIMEOUT`].)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_write_waits_its_turn_behind_a_write_held_past_the_busy_timeout() {
    let db = TempDb::new("long_write");
    let pool = db.connect().await;
    prepare(pool.write()).await.expect("prepare the database");

    let mut tx = begin_write(pool.write())
        .await
        .expect("begin the long write");
    move_reading_position(&mut tx, 1).await;

    let started = Arc::new(AtomicBool::new(false));
    let second = tokio::spawn({
        let pool = pool.clone();
        let started = started.clone();
        async move {
            started.store(true, Ordering::Release);
            let asked = Instant::now();
            let saved = save_reading_position(pool.write(), KJV, GENESIS, 2).await;
            (saved, asked.elapsed())
        }
    });
    // Hold the write for the whole LONG_WRITE after the second has asked.
    while !started.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::sleep(LONG_WRITE).await;
    tx.commit().await.expect("commit the long write");

    let (saved, waited) = second.await.expect("the second write's task");
    assert!(
        saved.is_ok(),
        "the second write failed after {waited:?}: {:#}",
        saved.err().map(|e| format!("{e:#}")).unwrap_or_default()
    );
    assert!(
        waited > BUSY_TIMEOUT,
        "the second write took {waited:?}, so it didn't wait behind the long one"
    );
    let position = get_reading_position(pool.read())
        .await
        .expect("read the reading position");
    assert_eq!(
        position,
        Some((KJV, GENESIS, 2)),
        "the second write came last"
    );
}

/// Reads don't wait for a write that's held open (write-ahead logging):
/// they finish while it's still open, and see the database as it was
/// before it, until it commits.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reads_carry_on_while_a_write_is_held_open() {
    let db = TempDb::new("read_during_write");
    let pool = db.connect().await;
    prepare(pool.write()).await.expect("prepare the database");
    save_reading_position(pool.write(), KJV, GENESIS, 1)
        .await
        .expect("save the reading position");

    let mut tx = begin_write(pool.write()).await.expect("begin the write");
    move_reading_position(&mut tx, 2).await;

    // The write is never committed until these reads finish, so a read that
    // waited for it would be stuck here until STUCK.
    let during = tokio::time::timeout(STUCK, async {
        let position = get_reading_position(pool.read()).await;
        let verses: Result<Vec<(i64,)>, _> = sqlx::query_as(
            "SELECT verse FROM verses WHERE translation_id = $1 AND book_id = $2 AND chapter = 1",
        )
        .bind(KJV)
        .bind(GENESIS)
        .fetch_all(pool.read())
        .await;
        (position, verses)
    })
    .await;
    tx.commit().await.expect("commit the write");

    let (position, verses) = during.expect("the reads finished while the write was open");
    assert_eq!(
        position.expect("read the reading position"),
        Some((KJV, GENESIS, 1)),
        "a read sees the database as it was before the open write"
    );
    assert_eq!(verses.expect("read Genesis 1").len(), 31);
    let after = get_reading_position(pool.read())
        .await
        .expect("read the reading position");
    assert_eq!(
        after,
        Some((KJV, GENESIS, 2)),
        "and the write once it commits"
    );
}

/// The app writes through exactly one connection, set up like every other
/// (write-ahead log, NORMAL sync, the busy timeout as a backstop), and its
/// reading connections refuse to write.
#[tokio::test]
async fn one_connection_writes_and_the_reading_connections_cannot() {
    let db = TempDb::new("layout");
    let pool = db.connect().await;
    prepare(pool.write()).await.expect("prepare the database");

    assert_eq!(pool.write().pool().options().get_max_connections(), 1);
    let mut writer = pool.write().pool().acquire().await.expect("the writer");
    let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&mut *writer)
        .await
        .expect("journal_mode");
    assert_eq!(mode, "wal");
    let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(&mut *writer)
        .await
        .expect("synchronous");
    assert_eq!(synchronous, 1, "NORMAL");
    let timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(&mut *writer)
        .await
        .expect("busy_timeout");
    assert_eq!(u128::try_from(timeout).ok(), Some(BUSY_TIMEOUT.as_millis()));
    let query_only: i64 = sqlx::query_scalar("PRAGMA query_only")
        .fetch_one(&mut *writer)
        .await
        .expect("query_only");
    assert_eq!(query_only, 0, "the writer writes");
    drop(writer);

    let refused = save_reading_position_on_the_read_pool(&pool).await;
    assert!(
        refused
            .as_ref()
            .is_err_and(|e| e.to_string().contains("readonly")),
        "a write on a reading connection was not refused: {refused:?}"
    );
    let position = get_reading_position(pool.read())
        .await
        .expect("read the reading position");
    assert_eq!(position, None, "and nothing was written");
}

/// What `save_reading_position` does, sent to the read pool by mistake.
async fn save_reading_position_on_the_read_pool(pool: &Db) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO reading_position (id, translation_id, book_id, chapter) VALUES (1, $1, $2, 1)",
    )
    .bind(KJV)
    .bind(GENESIS)
    .execute(pool.read())
    .await
    .map(|_| ())
}
