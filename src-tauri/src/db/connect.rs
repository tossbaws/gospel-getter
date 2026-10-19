use anyhow::Context;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Sqlite, SqlitePool, Transaction};
use std::str::FromStr;
use std::time::Duration;

/// How long a statement waits for another connection's lock before SQLite
/// gives up with "database is locked". Every write transaction is short (a
/// bookmark, a note, one book of the search index), so this is a generous
/// bound, not an expected wait.
pub const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Connect to the SQLite database, creating the file (and its parent
/// directory) on first run so a fresh checkout starts up on its own instead
/// of requiring the database to already exist.
///
/// Every connection uses write-ahead logging, so the reader's reads never
/// wait for a write and a write never waits for readers: the first-run
/// search index build runs in the background while the page reads and
/// saves. WAL is a setting of the database file itself (an older database
/// is switched on its first connection), and SQLite keeps `-wal` and `-shm`
/// files beside it. The app never copies its database file; the one copy
/// it makes, of an old install's database (`migrate_legacy_db_if_needed`),
/// happens before anything opens either file.
/// With WAL, `synchronous = NORMAL` is safe: a power cut can lose the last
/// transactions, never the database. And every connection waits up to
/// [`BUSY_TIMEOUT`] for a lock rather than failing at once (see
/// [`begin_write`] for the one wait SQLite won't make).
pub async fn create_pool(database_url: &str) -> anyhow::Result<SqlitePool> {
    if let Some(path) = database_url.strip_prefix("sqlite:") {
        let path = path.split('?').next().unwrap_or(path);
        if let Some(parent) = std::path::Path::new(path).parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).context(format!(
                "Failed to create database directory {}",
                parent.display()
            ))?;
        }
    }

    let options = SqliteConnectOptions::from_str(database_url)
        .context(format!("Invalid database URL: {}", database_url))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(BUSY_TIMEOUT);

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .context(format!("Failed to connect to database at {}", database_url))
}

/// Starts a transaction that writes, taking the write lock at once
/// (`BEGIN IMMEDIATE`, waiting up to [`BUSY_TIMEOUT`] for it).
///
/// A plain `BEGIN` takes no lock until the first statement, and a
/// transaction that reads before it writes (checking a passage exists
/// before adding a bookmark, say) then has to turn its read lock into the
/// write lock. If another connection is writing at that moment (the search
/// index build), SQLite refuses at once, without waiting: waiting could
/// deadlock. The same happens on a write into the search index, whose
/// full-text table reads its own tables before writing. So every
/// transaction that writes starts with the write lock.
pub async fn begin_write(pool: &SqlitePool) -> sqlx::Result<Transaction<'static, Sqlite>> {
    pool.begin_with("BEGIN IMMEDIATE").await
}
