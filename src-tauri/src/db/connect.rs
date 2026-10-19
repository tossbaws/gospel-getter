use anyhow::Context;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Sqlite, SqlitePool, Transaction};
use std::str::FromStr;
use std::time::Duration;

/// How long a statement waits for another connection's lock before SQLite
/// gives up with "database is locked". The app's own writes never wait on
/// each other's locks (they take turns, see [`WritePool`]), so this only
/// matters if something outside the app (another copy of it, a database
/// tool) holds the lock: a backstop, not an expected wait.
pub const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a write waits for its turn on the [`WritePool`] before giving
/// up. Waiting in line is normal (behind a note being saved, or a book of
/// the search index being built), and every write is short, so this is
/// far longer than any wait should be.
pub const WRITE_QUEUE_TIMEOUT: Duration = Duration::from_secs(60);

/// How many connections the app reads with at once.
const READ_CONNECTIONS: u32 = 5;

/// The app's database: a pool of connections that only read, and the one
/// connection everything that writes goes through.
///
/// Every write takes its turn on the one writing connection, so the app's
/// own writes queue up behind each other instead of competing for
/// SQLite's write lock (where a loser waits at most [`BUSY_TIMEOUT`], then
/// fails with "database is locked"). Reads use their own connections and,
/// with write-ahead logging, never wait for a write. Cloning is cheap: the
/// clone shares the same connections.
#[derive(Clone, Debug)]
pub struct Db {
    read: SqlitePool,
    write: WritePool,
}

impl Db {
    /// The connections that read. They refuse to write (`PRAGMA
    /// query_only`), so a write sent here by mistake fails at once, every
    /// time, rather than racing the writer.
    pub fn read(&self) -> &SqlitePool {
        &self.read
    }

    /// The one connection that writes.
    pub fn write(&self) -> &WritePool {
        &self.write
    }

    /// Close every connection, waiting for any in use to be returned.
    pub async fn close(&self) {
        self.write.0.close().await;
        self.read.close().await;
    }
}

/// The one connection the app writes with, as a pool of one: callers
/// waiting for it are served in the order they asked (sqlx's pools are
/// fair), so the pool is the queue. Every function that writes takes this
/// type, which only [`create_pool`] makes, so a write can't be handed the
/// read pool by mistake.
#[derive(Clone, Debug)]
pub struct WritePool(SqlitePool);

impl WritePool {
    /// The writing connection's pool, for a write that's a single statement
    /// (SQLite runs it in a transaction of its own) and for reads that
    /// belong with a write. A transaction starts with [`begin_write`].
    pub fn pool(&self) -> &SqlitePool {
        &self.0
    }
}

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
pub async fn create_pool(database_url: &str) -> anyhow::Result<Db> {
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

    // The writer connects first: it creates the file and switches it to
    // WAL, which the read-only connections can't.
    let write = SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(WRITE_QUEUE_TIMEOUT)
        .connect_with(options.clone())
        .await
        .context(format!("Failed to connect to database at {}", database_url))?;
    let read = SqlitePoolOptions::new()
        .max_connections(READ_CONNECTIONS)
        .connect_with(options.pragma("query_only", "ON"))
        .await
        .context(format!("Failed to connect to database at {}", database_url))?;
    Ok(Db {
        read,
        write: WritePool(write),
    })
}

/// Starts a transaction that writes, on the writing connection once it's
/// this caller's turn, taking SQLite's write lock at once (`BEGIN
/// IMMEDIATE`, waiting up to [`BUSY_TIMEOUT`] for it if something outside
/// the app holds it).
///
/// A plain `BEGIN` takes no lock until the first statement, and a
/// transaction that reads before it writes (checking a passage exists
/// before adding a bookmark, say) then has to turn its read lock into the
/// write lock. If another connection is writing at that moment (another
/// copy of the app), SQLite refuses at once, without waiting: waiting
/// could deadlock. The same happens on a write into the search index,
/// whose full-text table reads its own tables before writing. So every
/// transaction that writes starts with the write lock.
pub async fn begin_write(pool: &WritePool) -> sqlx::Result<Transaction<'static, Sqlite>> {
    pool.0.begin_with("BEGIN IMMEDIATE").await
}
