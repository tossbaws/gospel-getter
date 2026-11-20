//! Trying background work again, a few times, with a pause between tries:
//! for work that can lose a race (a lock) it would win a moment later, and
//! that nothing else will redo until the app restarts.

use std::future::Future;
use std::time::Duration;

/// SQLite's primary result codes for "the database is busy" (another
/// connection holds a lock it needs) and "a table is locked"; each
/// extended code (`SQLITE_BUSY_SNAPSHOT`, `SQLITE_LOCKED_SHAREDCACHE`...)
/// is one of these in its low byte.
const SQLITE_BUSY: i32 = 5;
const SQLITE_LOCKED: i32 = 6;

/// Whether `error` is one that trying again can get past: SQLite busy or
/// locked, anywhere in its chain (the error at this one seam is matched by
/// type, while the rest of the app keeps `anyhow`). Any other failure (a
/// constraint, a missing table, a mistake) fails the same way every time,
/// so trying again only delays the answer.
fn can_succeed_later(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| match cause.downcast_ref::<sqlx::Error>() {
            Some(sqlx::Error::Database(e)) => e
                .code()
                .and_then(|code| code.parse::<i32>().ok())
                .is_some_and(|code| matches!(code & 0xff, SQLITE_BUSY | SQLITE_LOCKED)),
            _ => false,
        })
}

/// Runs `attempt` until it succeeds, pausing `delays[i]` after its `i`-th
/// failure; when the pauses run out, the last try's error is the answer.
/// Only a failure that can succeed later (`can_succeed_later`) is tried
/// again, and logged as a warning, naming `what`; any other is the answer
/// at once.
pub async fn with_retries<T, F, Fut>(
    what: &str,
    delays: &[Duration],
    mut attempt: F,
) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let tries = delays.len() + 1;
    for (failed, delay) in delays.iter().enumerate() {
        match attempt().await {
            Ok(value) => return Ok(value),
            Err(e) if !can_succeed_later(&e) => return Err(e),
            Err(e) => {
                tracing::warn!(
                    "{what} failed (try {} of {tries}), trying again in {delay:?}: {e:#}",
                    failed + 1
                );
                tokio::time::sleep(*delay).await;
            }
        }
    }
    attempt().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::cell::Cell;

    const NO_PAUSES: [Duration; 3] = [Duration::ZERO; 3];

    /// A real "database is locked" (SQLITE_BUSY): a write lock asked for,
    /// without waiting, while another connection holds it.
    async fn busy() -> anyhow::Error {
        let db = crate::db::test_support::TempDb::new("retry_busy");
        let options = SqliteConnectOptions::new()
            .filename(db.path())
            .create_if_missing(true)
            .busy_timeout(Duration::ZERO);
        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(options)
            .await
            .unwrap();
        let mut holder = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        sqlx::query("CREATE TABLE t (x)")
            .execute(&mut *holder)
            .await
            .unwrap();
        let error = pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map(|_| ())
            .context("Failed to start a write")
            .unwrap_err();
        holder.rollback().await.unwrap();
        pool.close().await;
        error
    }

    /// A real error SQLite gives for `sql` on an empty database in memory
    /// with one table, `t`, holding one row.
    async fn fault(sql: &str) -> anyhow::Error {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for setup in ["CREATE TABLE t (x PRIMARY KEY)", "INSERT INTO t VALUES (1)"] {
            sqlx::query(setup).execute(&pool).await.unwrap();
        }
        let error = sqlx::query(sqlx::AssertSqlSafe(sql.to_string()))
            .execute(&pool)
            .await
            .map(|_| ())
            .context("Failed to write")
            .unwrap_err();
        pool.close().await;
        error
    }

    /// A database error with SQLite's extended result `code`, as sqlx gives
    /// one, for the codes a test can't easily make SQLite give.
    #[derive(Debug)]
    struct Coded(i32);

    impl std::fmt::Display for Coded {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "SQLite error {}", self.0)
        }
    }

    impl std::error::Error for Coded {}

    impl sqlx::error::DatabaseError for Coded {
        fn message(&self) -> &str {
            "coded"
        }
        fn code(&self) -> Option<std::borrow::Cow<'_, str>> {
            Some(self.0.to_string().into())
        }
        fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
            self
        }
        fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
            self
        }
        fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
            self
        }
        fn kind(&self) -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::Other
        }
    }

    fn coded(code: i32) -> anyhow::Error {
        anyhow::Error::from(sqlx::Error::Database(Box::new(Coded(code))))
            .context("Failed to build the search index")
    }

    /// How many tries `with_retries` makes of work that fails with `error()`
    /// every time.
    async fn tries_of<F, Fut>(error: F) -> usize
    where
        F: Fn() -> Fut,
        Fut: Future<Output = anyhow::Error>,
    {
        let tries = Cell::new(0);
        let answer: anyhow::Result<()> = with_retries("work", &NO_PAUSES, || {
            tries.set(tries.get() + 1);
            let error = error();
            async move { Err(error.await) }
        })
        .await;
        assert!(answer.is_err());
        tries.get()
    }

    #[tokio::test]
    async fn a_success_is_not_tried_again() {
        let tries = Cell::new(0);
        let answer = with_retries("work", &NO_PAUSES, || {
            tries.set(tries.get() + 1);
            async { Ok::<_, anyhow::Error>(7) }
        })
        .await;
        assert_eq!(answer.unwrap(), 7);
        assert_eq!(tries.get(), 1);
    }

    /// A busy database is tried again until it's free, under its context.
    #[tokio::test]
    async fn a_busy_database_is_tried_again_until_it_succeeds() {
        let tries = Cell::new(0);
        let answer = with_retries("work", &NO_PAUSES, || {
            tries.set(tries.get() + 1);
            let n = tries.get();
            async move {
                if n < 3 {
                    return Err(busy().await);
                }
                Ok(n)
            }
        })
        .await;
        assert_eq!(answer.unwrap(), 3);
        assert_eq!(tries.get(), 3);
    }

    #[tokio::test]
    async fn after_the_last_pause_the_last_error_is_the_answer() {
        let tries = Cell::new(0);
        let answer: anyhow::Result<()> = with_retries("work", &NO_PAUSES, || {
            tries.set(tries.get() + 1);
            let n = tries.get();
            async move { Err(busy().await.context(format!("failure {n}"))) }
        })
        .await;
        assert_eq!(answer.unwrap_err().to_string(), "failure 4");
        assert_eq!(tries.get(), 4);
    }

    /// Busy and locked, and each of their extended codes, are tried again;
    /// any other code isn't.
    #[tokio::test]
    async fn only_busy_and_locked_codes_are_tried_again() {
        // SQLITE_BUSY and _RECOVERY, _SNAPSHOT, _TIMEOUT; SQLITE_LOCKED and
        // _SHAREDCACHE, _VTAB.
        for code in [5, 261, 517, 773, 6, 262, 518] {
            assert_eq!(tries_of(|| async { coded(code) }).await, 4, "{code}");
        }
        // SQLITE_ERROR, _CONSTRAINT_UNIQUE, _READONLY, _CORRUPT, _FULL.
        for code in [1, 2067, 8, 11, 13] {
            assert_eq!(tries_of(|| async { coded(code) }).await, 1, "{code}");
        }
    }

    /// A constraint broken or a table that isn't there fails the same way
    /// however often it's tried, so it's tried once; so is any error that
    /// isn't the database's.
    #[tokio::test]
    async fn a_fault_is_tried_once() {
        assert_eq!(
            tries_of(|| fault("INSERT INTO t VALUES (1)")).await,
            1,
            "a constraint"
        );
        assert_eq!(
            tries_of(|| fault("INSERT INTO missing VALUES (1)")).await,
            1,
            "a missing table"
        );
        assert_eq!(
            tries_of(|| async { anyhow::anyhow!("database is locked") }).await,
            1,
            "words, not a database error"
        );
    }

    #[tokio::test]
    async fn it_pauses_between_tries() {
        let started = std::time::Instant::now();
        let _: anyhow::Result<()> = with_retries(
            "work",
            &[Duration::from_millis(30), Duration::from_millis(60)],
            || async { Err(coded(5)) },
        )
        .await;
        assert!(started.elapsed() >= Duration::from_millis(90));
    }
}
