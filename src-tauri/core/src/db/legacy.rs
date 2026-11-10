//! Bringing a database from the pre-Tauri install into the app's data
//! directory, the first time the app runs: a consistent snapshot of it,
//! checked, then put in place whole, or not at all.

use anyhow::{Context, bail};
use sqlx::Connection;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::db::BUSY_TIMEOUT;
use crate::db::connect::beside;

/// The tables every Gospel Getter database has had, from the pre-Tauri
/// app's very first: a snapshot without them isn't of one.
const REQUIRED_TABLES: [&str; 4] = ["books", "translations", "verses", "reading_position"];

/// If the app's own database doesn't exist yet, but a database from the
/// pre-Tauri install (the systemd/browser-wrapper app, which kept its data
/// at `~/.local/share/gospel-getter/data/gospel_getter.db`) does, bring it
/// into place so the reader's translations and reading position survive
/// the move to the Tauri app data directory. Returns whether this call put
/// it there.
///
/// The legacy database is only ever read, on a read-only connection that
/// takes SQLite's usual locks: never moved, modified or deleted, so the old
/// install stays intact until the reader chooses to remove it themselves.
/// What's read is a consistent snapshot (`VACUUM INTO`): everything
/// committed, including what's still only in its write-ahead log, even
/// while the old app is open and writing; nothing half-written.
///
/// The snapshot goes to a new temporary file beside `new_db_path`, which is
/// checked (`PRAGMA integrity_check`, and the `REQUIRED_TABLES`) and only
/// then published, by a hard link that fails if anything is already at
/// `new_db_path`, so nothing there is ever replaced: if another launch got
/// there first, its copy stands and this returns `false`. On any failure the
/// temporary file is removed and nothing is at `new_db_path`, so a later
/// launch tries again. The caller must not create a database there in the
/// meantime: that would be taken for the migrated one.
pub async fn migrate_legacy_db_if_needed(
    new_db_path: &Path,
    legacy_db_path: &Path,
) -> anyhow::Result<bool> {
    if new_db_path.exists() {
        return Ok(false);
    }
    if !legacy_db_path.is_file() {
        return Ok(false);
    }
    let dir = match new_db_path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => PathBuf::from("."),
    };
    std::fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;
    let snapshot = Snapshot::take(legacy_db_path, &dir).await?;
    snapshot.check().await?;
    snapshot.publish(new_db_path)
}

/// How to read the legacy database: read-only, so SQLite itself refuses to
/// write it, and without making any file beside it.
///
/// While the old app has it open, SQLite's locks and (in WAL mode) its
/// `-wal` and `-shm` files are there: this reads through them, as any
/// reader would, waiting up to [`BUSY_TIMEOUT`] for a commit under way. A
/// WAL-mode database with no `-wal` beside it is open nowhere (SQLite
/// removes the log when its last connection closes), and reading it
/// normally would make both files (a read-only connection can't remove
/// them after), so it's read as immutable, like
/// [`open_read_only`](crate::db::open_read_only) does. A rollback-journal
/// database (the pre-Tauri app's own mode) needs nothing beside it.
fn source_options(legacy: &Path) -> anyhow::Result<SqliteConnectOptions> {
    let options = SqliteConnectOptions::new()
        .filename(legacy)
        .read_only(true)
        .create_if_missing(false)
        .busy_timeout(BUSY_TIMEOUT);
    let wal = std::fs::metadata(beside(legacy, "-wal")).ok();
    match wal {
        Some(_) if beside(legacy, "-shm").exists() => Ok(options),
        Some(wal) if wal.len() > 0 => bail!(
            "{} has a write-ahead log but no index for it. Open the earlier Gospel Getter once \
             and close it, then try again.",
            legacy.display()
        ),
        _ if in_wal_mode(legacy)? => Ok(options.immutable(true)),
        _ => Ok(options),
    }
}

/// Whether the database file at `path` is in WAL mode, by its header (a
/// file that isn't a database isn't: reading it then says what it is).
fn in_wal_mode(path: &Path) -> anyhow::Result<bool> {
    use std::io::Read;
    let mut header = [0; 20];
    let mut file =
        std::fs::File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
    let read = file
        .read(&mut header)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    // The format's magic string, then the read and write versions at bytes
    // 18 and 19: 2 for WAL.
    Ok(read == header.len() && header.starts_with(b"SQLite format 3\0") && header[18..] == [2, 2])
}

/// A snapshot of the legacy database in a temporary file of its own,
/// removed when dropped unless it was published.
#[derive(Debug)]
struct Snapshot {
    path: Option<PathBuf>,
}

impl Snapshot {
    /// Snapshot `legacy` into a new file in `dir`.
    async fn take(legacy: &Path, dir: &Path) -> anyhow::Result<Self> {
        let snapshot = Self::reserve(dir)?;
        let target = snapshot.path()?;
        let target_str = target
            .to_str()
            .with_context(|| format!("{} isn't valid UTF-8", target.display()))?;
        let options = source_options(legacy)?;
        let mut source = SqliteConnection::connect_with(&options)
            .await
            .with_context(|| format!("Failed to open {} read-only", legacy.display()))?;
        let copied = sqlx::query("VACUUM INTO $1")
            .bind(target_str)
            .execute(&mut source)
            .await
            .with_context(|| format!("Failed to snapshot {}", legacy.display()));
        source
            .close()
            .await
            .with_context(|| format!("Failed to close {}", legacy.display()))?;
        copied?;
        // On disk before it's published: `VACUUM INTO` doesn't sync its
        // output. Opened for writing: Windows refuses to flush a file
        // opened read-only (FlushFileBuffers needs write access).
        std::fs::OpenOptions::new()
            .write(true)
            .open(target)
            .and_then(|file| file.sync_all())
            .with_context(|| format!("Failed to save {}", target.display()))?;
        Ok(snapshot)
    }

    /// A new, empty file in `dir` (which `VACUUM INTO` accepts as its
    /// target), with a name no other launch is using. On Unix it's made
    /// readable and writable by the reader alone (mode 0600), as the
    /// database it becomes is kept (`create_pool`): `VACUUM INTO` keeps the
    /// mode of the file it writes into.
    fn reserve(dir: &Path) -> anyhow::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        for _ in 0..64 {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = dir.join(format!(".{pid}-{nanos:x}-{n:x}.legacy-migration.tmp"));
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
            match options.open(&path) {
                Ok(_) => return Ok(Self { path: Some(path) }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => {
                    return Err(e).with_context(|| format!("Failed to create {}", path.display()));
                }
            }
        }
        bail!(
            "No unused temporary file name was found in {}",
            dir.display()
        )
    }

    fn path(&self) -> anyhow::Result<&Path> {
        self.path
            .as_deref()
            .context("The snapshot was already published")
    }

    /// Whether the snapshot is whole, and of a Gospel Getter database.
    async fn check(&self) -> anyhow::Result<()> {
        let path = self.path()?;
        let options = SqliteConnectOptions::new()
            .filename(path)
            .read_only(true)
            .create_if_missing(false);
        let mut snapshot = SqliteConnection::connect_with(&options)
            .await
            .with_context(|| format!("Failed to open the snapshot {}", path.display()))?;
        let checked = Self::check_on(&mut snapshot).await;
        snapshot
            .close()
            .await
            .context("Failed to close the snapshot")?;
        checked
    }

    async fn check_on(snapshot: &mut SqliteConnection) -> anyhow::Result<()> {
        let integrity: Vec<String> = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_all(&mut *snapshot)
            .await
            .context("Failed to check the snapshot's integrity")?;
        if integrity != ["ok"] {
            bail!(
                "The snapshot failed SQLite's integrity check: {}",
                integrity.join("; ")
            );
        }
        let tables: Vec<String> =
            sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table'")
                .fetch_all(&mut *snapshot)
                .await
                .context("Failed to list the snapshot's tables")?;
        let missing: Vec<&str> = REQUIRED_TABLES
            .into_iter()
            .filter(|table| !tables.iter().any(|t| t == table))
            .collect();
        if !missing.is_empty() {
            bail!(
                "It isn't a Gospel Getter database: it has no {} table",
                missing.join(", ")
            );
        }
        Ok(())
    }

    /// Put the snapshot at `target`, unless something is already there.
    /// `true` if this put it there.
    fn publish(mut self, target: &Path) -> anyhow::Result<bool> {
        let path = self.path()?.to_path_buf();
        match std::fs::hard_link(&path, target) {
            Ok(()) => {
                // `target` keeps the file; only the temporary name goes.
                self.path = None;
                if let Err(e) = std::fs::remove_file(&path) {
                    tracing::warn!("Failed to remove {}: {e}", path.display());
                }
                sync_dir(target);
                Ok(true)
            }
            // Another launch published its snapshot first (or something
            // else is there): that stands, and this one goes.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
            Err(e) => Err(e).with_context(|| {
                format!(
                    "Failed to put the snapshot in place at {}",
                    target.display()
                )
            }),
        }
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Best-effort: the new name survives a power cut (on Unix, where a
/// directory can be synced).
fn sync_dir(target: &Path) {
    #[cfg(unix)]
    if let Some(dir) = target.parent()
        && let Err(e) = std::fs::File::open(dir).and_then(|dir| dir.sync_all())
    {
        tracing::warn!("Failed to sync {}: {e}", dir.display());
    }
    #[cfg(not(unix))]
    let _ = target;
}

#[cfg(test)]
mod tests;
