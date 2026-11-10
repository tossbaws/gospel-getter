//! Where the database is and who can read it, against real files: it's
//! opened at exactly the path given, whatever characters its folders' names
//! have (a URL would read `%`, `?` and `#` as something else), and on Unix
//! its folder and files are the reader's alone, as their exports are.

use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use super::test_support::TempDb;
use super::{DATABASE_FILE, create_pool, create_private_dir, open_read_only};

/// A folder of its own to make things in (`TempDb`'s, which removes it).
fn scratch(label: &str) -> (TempDb, PathBuf) {
    let db = TempDb::new(label);
    let root = db.path().parent().expect("a folder").to_path_buf();
    (db, root)
}

/// The names of what's in `dir`.
fn names_in(dir: &Path) -> Vec<std::ffi::OsString> {
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .expect("list the folder")
        .map(|entry| entry.expect("an entry").file_name())
        .collect();
    names.sort();
    names
}

/// The file SQLite has open as the main database on `pool`.
async fn main_file(pool: &SqlitePool) -> String {
    let rows: Vec<(i64, String, String)> = sqlx::query_as("PRAGMA database_list")
        .fetch_all(pool)
        .await
        .expect("list the databases");
    rows.into_iter()
        .find(|(_, name, _)| name == "main")
        .map(|(_, _, file)| file)
        .expect("the main database")
}

async fn marker(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT name FROM marker")
        .fetch_one(pool)
        .await
        .expect("read the marker")
}

/// A folder named with `%25`, `?` or `#` (Windows allows no `?`) holds the
/// database: SQLite opens that file, nothing else is made, and a read-only
/// connection by the same path, while the app has it open and after, reads
/// what was written.
#[tokio::test]
async fn a_database_opens_at_exactly_the_path_named() {
    let names: &[&str] = if cfg!(windows) {
        &["a%25b", "a#b", "100%25 done", "a%b"]
    } else {
        &[
            "a%25b",
            "a?b",
            "a#b",
            "100%25 done",
            "what?now",
            "a%b",
            "x?mode=ro",
        ]
    };
    for name in names {
        let (_scratch, root) = scratch("exact_path");
        let path = root.join(name).join(DATABASE_FILE);
        let db = create_pool(&path).await.expect("open the database");
        sqlx::query("CREATE TABLE marker (name TEXT NOT NULL)")
            .execute(db.write().pool())
            .await
            .expect("make a table");
        sqlx::query("INSERT INTO marker (name) VALUES ($1)")
            .bind(*name)
            .execute(db.write().pool())
            .await
            .expect("write to it");
        let expected = path.to_str().expect("a UTF-8 path");
        assert_eq!(main_file(db.write().pool()).await, expected, "{name}");
        assert_eq!(main_file(db.read()).await, expected, "{name}");
        assert!(path.is_file(), "{name}");
        assert_eq!(names_in(&root), [std::ffi::OsString::from(name)], "{name}");

        // While the app has it open (through its -wal), and once it's closed
        // (immutable, as a URI).
        let open = open_read_only(&path).await.expect("open it read-only");
        assert_eq!(marker(&open).await, *name);
        open.close().await;
        db.close().await;
        let closed = open_read_only(&path)
            .await
            .expect("open it read-only, closed");
        assert_eq!(marker(&closed).await, *name);
        assert_eq!(main_file(&closed).await, expected, "{name}");
        closed.close().await;
    }
}

/// A path that isn't UTF-8 (which SQLite can't be given) is refused, and
/// nothing is made: no folder by that name, and none of a similar name
/// made by replacing the bytes that aren't.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_path_that_is_not_utf8_is_refused_and_nothing_is_made() {
    use std::os::unix::ffi::OsStrExt;
    let (_scratch, root) = scratch("not_utf8");
    let path = root
        .join(std::ffi::OsStr::from_bytes(b"a\xffb"))
        .join(DATABASE_FILE);
    let refused = create_pool(&path)
        .await
        .map(|_| ())
        .expect_err("a path that isn't UTF-8");
    assert!(format!("{refused:#}").contains("UTF-8"), "{refused:#}");
    assert_eq!(names_in(&root), Vec::<std::ffi::OsString>::new());
}

/// The process's umask, set for a test and put back when it's dropped.
#[cfg(unix)]
struct Umask(libc::mode_t);

#[cfg(unix)]
impl Umask {
    fn set(mask: libc::mode_t) -> Self {
        // SAFETY: umask only swaps the process's file mode mask; it can't
        // fail and touches no memory.
        Self(unsafe { libc::umask(mask) })
    }
}

#[cfg(unix)]
impl Drop for Umask {
    fn drop(&mut self) {
        // SAFETY: as in `set`.
        unsafe { libc::umask(self.0) };
    }
}

/// Under the usual umask (022, which leaves a new file readable by
/// everyone), the data folder is the reader's alone (0700), and the
/// database and the -wal and -shm beside it (0600), whether startup makes
/// them or finds them made, open to others, by an earlier version.
#[cfg(unix)]
#[tokio::test]
async fn the_data_folder_and_database_are_the_readers_alone() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |path: &Path| {
        std::fs::metadata(path)
            .expect("its metadata")
            .permissions()
            .mode()
            & 0o777
    };
    let set_mode = |path: &Path, mode: u32| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
    };
    let beside = |path: &Path, suffix: &str| {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        PathBuf::from(name)
    };
    let (_scratch, root) = scratch("private_files");
    let _umask = Umask::set(0o022);

    // A first launch: the folder (and the one above it) and the database
    // are made.
    let dir = root.join("data").join(super::APP_IDENTIFIER);
    create_private_dir(&dir).expect("make the data folder");
    assert_eq!(mode(&dir), 0o700, "a new data folder");
    assert_eq!(mode(&root.join("data")), 0o700, "a folder made above it");
    let path = dir.join(DATABASE_FILE);
    let db = create_pool(&path).await.expect("open the database");
    sqlx::query("CREATE TABLE marker (name TEXT NOT NULL)")
        .execute(db.write().pool())
        .await
        .expect("write to it");
    assert_eq!(mode(&path), 0o600, "a new database");
    for suffix in ["-wal", "-shm"] {
        assert_eq!(mode(&beside(&path, suffix)), 0o600, "its {suffix}");
    }
    db.close().await;

    // A database folder made by `create_pool` alone (a path elsewhere) is
    // private too.
    let elsewhere = root.join("elsewhere").join(DATABASE_FILE);
    let other = create_pool(&elsewhere).await.expect("open another");
    assert_eq!(mode(&root.join("elsewhere")), 0o700, "its new folder");
    assert_eq!(mode(&elsewhere), 0o600, "another new database");
    other.close().await;

    // Made by an earlier version, open to everyone: startup narrows them.
    set_mode(&dir, 0o755);
    set_mode(&path, 0o644);
    create_private_dir(&dir).expect("find the data folder");
    assert_eq!(mode(&dir), 0o700, "an existing data folder");
    let db = create_pool(&path).await.expect("open it again");
    sqlx::query("INSERT INTO marker (name) VALUES ('private')")
        .execute(db.write().pool())
        .await
        .expect("write to it");
    assert_eq!(mode(&path), 0o600, "an existing database");
    for suffix in ["-wal", "-shm"] {
        assert_eq!(mode(&beside(&path, suffix)), 0o600, "its {suffix}");
    }
    db.close().await;

    // The owner's own permissions are left as they are.
    set_mode(&dir, 0o750);
    create_private_dir(&dir).expect("find it again");
    assert_eq!(mode(&dir), 0o700);
}
