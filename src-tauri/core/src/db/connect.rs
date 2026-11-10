//! Opening the database: a pool of connections that only read, and the one
//! connection every write takes its turn on, with the settings both share;
//! or, for a program beside the app (`open_read_only`), a connection that
//! can't write at all.

use anyhow::Context;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Sqlite, SqlitePool, Transaction};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The app's identifier, `identifier` in `tauri.conf.json`: its data folder
/// is the platform's data directory and this.
pub const APP_IDENTIFIER: &str = "com.tossbaws.gospel-getter";

/// The database's file name, in the app's data folder.
pub const DATABASE_FILE: &str = "gospel_getter.db";

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
/// is a SQLite snapshot, made before anything opens the new one.
/// With WAL, `synchronous = NORMAL` is safe: a power cut can lose the last
/// transactions, never the database. And every connection waits up to
/// [`BUSY_TIMEOUT`] for a lock rather than failing at once (see
/// [`begin_write`] for the one wait SQLite won't make).
///
/// `path` is the database file's path, never a URL: no character in it
/// (`?`, `#`, `%`) is read as anything but part of a name, so the folder
/// made and the file opened are the ones named. SQLite (through sqlx) takes
/// only UTF-8 file names, so a path that isn't is refused before anything
/// is made, rather than opening a file of a similar name.
pub async fn create_pool(path: &Path) -> anyhow::Result<Db> {
    anyhow::ensure!(
        path.to_str().is_some(),
        "The database's path, {}, isn't valid UTF-8, which SQLite needs.",
        path.display()
    );
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        create_dir_privately(parent)
            .with_context(|| format!("Failed to create database directory {}", parent.display()))?;
    }
    #[cfg(unix)]
    keep_database_private(path)?;

    let options = SqliteConnectOptions::new()
        .filename(path)
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
        .with_context(|| format!("Failed to connect to database at {}", path.display()))?;
    let read = SqlitePoolOptions::new()
        .max_connections(READ_CONNECTIONS)
        .connect_with(options.pragma("query_only", "ON"))
        .await
        .with_context(|| format!("Failed to connect to database at {}", path.display()))?;
    // Made by now (the writer switched the file to WAL), with the
    // database's own mode; any left from an earlier run were seen to above.
    #[cfg(unix)]
    for suffix in ["-wal", "-shm"] {
        keep_private(&beside(path, suffix));
    }
    Ok(Db {
        read,
        write: WritePool(write),
    })
}

/// Make the app's data folder, `dir`, and any folder above it that's
/// missing, and keep it the reader's alone: on Unix it's made with mode
/// 0700, and one made by an earlier version (0755, as the umask left it)
/// loses its group and other permissions, as the reader's exports already
/// have (their own folders may be open to others on the computer). Making
/// it can fail; narrowing an existing folder's permissions is only logged
/// if it fails (a folder this account doesn't own, say), since the app works
/// without it. On Windows the folder gets its parent's inherited
/// permissions, as any new folder there does.
pub fn create_private_dir(dir: &Path) -> anyhow::Result<()> {
    create_dir_privately(dir).with_context(|| format!("Failed to create {}", dir.display()))?;
    #[cfg(unix)]
    keep_private(dir);
    Ok(())
}

/// Make `dir` and any folder above it that's missing, each (on Unix) with
/// mode 0700; one that's already there is left as it is.
fn create_dir_privately(dir: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)
}

/// Keep the database at `path` and the files SQLite keeps beside it
/// readable and writable by the reader alone (mode 0600). A new database is
/// made here, empty, at that mode, before SQLite opens it: an empty file is
/// an empty database, and SQLite gives the `-wal` and `-shm` it makes the
/// database's own mode. One made by an earlier version (0644) loses its
/// group and other permissions, as does any `-wal`, `-shm` or `-journal`
/// left beside it.
#[cfg(unix)]
fn keep_database_private(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
    {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            for file in [
                path.to_path_buf(),
                beside(path, "-wal"),
                beside(path, "-shm"),
                beside(path, "-journal"),
            ] {
                keep_private(&file);
            }
        }
        Err(e) => {
            return Err(e).with_context(|| format!("Failed to create {}", path.display()));
        }
    }
    Ok(())
}

/// Take the group and other permissions away from `path`, a file or
/// folder, if it has any; the owner's stay as they are. Nothing there is
/// nothing to do, and a failure is logged, not returned: the app works
/// without this.
#[cfg(unix)]
fn keep_private(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mode = match std::fs::metadata(path) {
        Ok(metadata) => metadata.permissions().mode(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            tracing::warn!("Couldn't check who can read {}: {e}", path.display());
            return;
        }
    };
    if mode & 0o077 == 0 {
        return;
    }
    if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & !0o077)) {
        tracing::warn!(
            "Couldn't make {} readable by this account alone: {e}",
            path.display()
        );
    }
}

/// One of the files SQLite keeps beside a database: `path` and `suffix`
/// ("-wal", "-shm").
pub(crate) fn beside(path: &Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(path.as_os_str());
    name.push(suffix);
    PathBuf::from(name)
}

/// Open a database that a program beside the app only reads, whether or not
/// the app is running: one connection, which SQLite itself refuses to write
/// on (`SQLITE_OPEN_READONLY`). Nothing is ever created. It's no
/// [`WritePool`], so nothing that writes can be handed it.
///
/// The app leaves its database in write-ahead logging mode. While it runs,
/// its `-wal` and `-shm` files are there, and this reads through them, seeing
/// everything committed. Once it has closed they're gone, and SQLite would
/// create them again even for a read-only connection, so the file is opened as
/// immutable instead, which needs neither. That's safe for a short read if the
/// app starts meanwhile, since it writes to `-wal` first; so open a new
/// connection for each read rather than keeping this one.
///
/// A `-wal` holding data with no `-shm` (a database copied without it, say)
/// can't be read without creating one, and refuses.
pub async fn open_read_only(path: &Path) -> anyhow::Result<SqlitePool> {
    let wal = std::fs::metadata(beside(path, "-wal")).ok();
    let shm_exists = beside(path, "-shm").exists();
    let immutable = match wal {
        None => true,
        Some(_) if shm_exists => false,
        Some(wal) if wal.len() == 0 => true,
        Some(_) => anyhow::bail!(
            "{} has a write-ahead log but no index for it. Open Gospel Getter once first, \
             then try again.",
            path.display()
        ),
    };
    let options = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .immutable(immutable)
        .create_if_missing(false)
        .busy_timeout(BUSY_TIMEOUT)
        .pragma("query_only", "ON");
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .with_context(|| format!("Failed to open {} read-only", path.display()))
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
