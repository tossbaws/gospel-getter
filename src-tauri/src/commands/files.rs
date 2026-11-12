//! What the exports and the import share: writing a file completely or not
//! at all, and running the native file dialog off the async runtime.

use anyhow::Context;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// How old a temporary file of ours must be before a later save removes it:
/// one left by a save that never finished (the app stopped part-way), since
/// a save takes moments.
const STALE_TEMP_FILE: Duration = Duration::from_secs(60 * 60);

/// Write `bytes` to `path` completely or not at all: into a temporary file
/// beside it, synced, renamed over `path`, then the folder synced, so the
/// new name survives a power cut too (on Unix, where a folder can be
/// synced). Temporary files of ours an earlier save left there, over an
/// hour old, are removed first.
pub(super) fn write_atomically(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    write_atomically_then(path, bytes, sync_dir)
}

/// [`write_atomically`], with `sync_dir` run on the folder once the file is
/// in place.
fn write_atomically_then(
    path: &Path,
    bytes: &[u8],
    sync_dir: impl FnOnce(&Path),
) -> anyhow::Result<()> {
    use std::io::Write;
    path.file_name()
        .with_context(|| format!("{} isn't a file name", path.display()))?;
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    remove_stale_temp_files(&dir, SystemTime::now());
    let write = || -> std::io::Result<()> {
        let mut temp = create_temp_file(&dir, temp_names())?;
        let file = temp.file.as_mut().expect("open until persisted");
        file.write_all(bytes)?;
        file.sync_all()?;
        temp.persist_as(path)
    };
    write().with_context(|| format!("Couldn't write {}", path.display()))?;
    sync_dir(&dir);
    Ok(())
}

/// Best-effort: sync `dir`, so a file just renamed into it is still there
/// after a power cut (on Unix; elsewhere a folder can't be synced). A
/// failure is logged: the file is in place either way.
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Err(e) = std::fs::File::open(dir).and_then(|dir| dir.sync_all()) {
        tracing::warn!("Failed to sync {}: {e}", dir.display());
    }
    #[cfg(not(unix))]
    let _ = dir;
}

/// Whether `name` is one [`temp_names`] makes: ".gg-", a process id, two
/// hexadecimal counts, ".tmp", and nothing else.
fn is_temp_name(name: &str) -> bool {
    let Some(middle) = name
        .strip_prefix(".gg-")
        .and_then(|rest| rest.strip_suffix(".tmp"))
    else {
        return false;
    };
    let parts: Vec<&str> = middle.split('-').collect();
    match parts.as_slice() {
        [pid, nanos, n] => {
            !pid.is_empty()
                && pid.bytes().all(|b| b.is_ascii_digit())
                && [nanos, n]
                    .iter()
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_hexdigit()))
        }
        _ => false,
    }
}

/// Remove the temporary files of ours in `dir` that a save left behind:
/// files (never a folder or link) named as [`temp_names`] names them, last
/// changed over [`STALE_TEMP_FILE`] before `now`. Nothing else there is
/// touched, and one that can't be read or removed is left.
fn remove_stale_temp_files(dir: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_str().is_some_and(is_temp_name) {
            continue;
        }
        let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        let stale = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > STALE_TEMP_FILE);
        if metadata.is_file()
            && stale
            && let Err(e) = std::fs::remove_file(entry.path())
        {
            tracing::warn!("Failed to remove {}: {e}", entry.path().display());
        }
    }
}

/// A newly created temporary file, deleted when dropped unless it was
/// renamed into place with `persist_as`.
pub(crate) struct TempFile {
    file: Option<std::fs::File>,
    path: Option<PathBuf>,
}

impl TempFile {
    /// Close it and rename it over `target` (replacing any file there, which
    /// then has this file's permissions, not its own).
    fn persist_as(mut self, target: &Path) -> std::io::Result<()> {
        drop(self.file.take());
        let path = self.path.as_ref().expect("not yet persisted");
        std::fs::rename(path, target)?;
        self.path = None;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn path(&self) -> &Path {
        self.path.as_deref().expect("not yet persisted")
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        // Closed first: Windows can't delete a file that's still open.
        drop(self.file.take());
        if let Some(path) = self.path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Hidden candidate names for a temporary file, unique to this process,
/// this moment and each attempt, so simultaneous saves never share one.
/// None has the target's name in it, so each is at most 68 bytes (".gg-",
/// a process id, two hexadecimal counts and ".tmp") however long that name
/// is: a name up to the file system's limit (255 bytes) can be saved.
fn temp_names() -> impl Iterator<Item = String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    (0..64).map(move |_| {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        format!(".gg-{pid}-{nanos:x}-{n:x}.tmp")
    })
}

/// Create a brand-new file in `dir` with the first of `names` not already
/// taken. An existing file is never opened, truncated or removed: a name
/// that exists is skipped.
///
/// On Unix it's created readable and writable by the reader alone (mode
/// 0600, which the umask can only narrow), before a byte is written: an
/// export holds their notes and highlights, and their own folders may be
/// open to others on the computer. Renamed into place, it keeps that mode,
/// so replacing an export never leaves it more readable than it was. On
/// Windows it gets the folder's inherited permissions, as any new file
/// there does (and the rename keeps them).
pub(crate) fn create_temp_file(
    dir: &Path,
    names: impl IntoIterator<Item = String>,
) -> std::io::Result<TempFile> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    for name in names {
        let path = dir.join(name);
        match options.open(&path) {
            Ok(file) => {
                return Ok(TempFile {
                    file: Some(file),
                    path: Some(path),
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no unused temporary file name was found",
    ))
}

/// `dialog` in front of the app's main window, as its parent: it stays above
/// the window, and, where the platform does this, the window waits for it,
/// so nothing is changed behind it while it's open.
pub(super) fn over_main_window<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    dialog: tauri_plugin_dialog::FileDialogBuilder<R>,
) -> tauri_plugin_dialog::FileDialogBuilder<R> {
    #[cfg(desktop)]
    {
        use tauri::Manager;
        if let Some(window) = app.get_webview_window(crate::window::MAIN) {
            return dialog.set_parent(&window);
        }
    }
    #[cfg(not(desktop))]
    let _ = app;
    dialog
}

/// Runs a blocking native file dialog off the async runtime's worker
/// threads, and turns its answer into a path (`None` if cancelled).
pub(super) async fn run_dialog(
    show: impl FnOnce() -> Option<tauri_plugin_dialog::FilePath> + Send + 'static,
) -> Result<Option<PathBuf>, String> {
    let picked = tokio::task::spawn_blocking(show)
        .await
        .map_err(|e| format!("The file dialog failed: {e}"))?;
    picked
        .map(|p| {
            p.into_path()
                .map_err(|e| format!("That location can't be used: {e}"))
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A name as long as most file systems allow (255 bytes) has room for
    /// nothing more, so the temporary file it's written through can't be
    /// named after it; a 250-byte name is saved, completely, and nothing is
    /// left beside it.
    #[test]
    fn a_long_legal_file_name_is_saved() {
        let dir = std::env::temp_dir().join(format!(
            "gospel_getter_long_name_{}_{:x}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&dir).expect("make a scratch folder");
        let name = format!("{}.gospel-getter.json", "n".repeat(250 - 19));
        assert_eq!(name.len(), 250);
        let target = dir.join(&name);
        let saved = write_atomically(&target, b"the whole file");
        let left: Vec<_> = std::fs::read_dir(&dir)
            .expect("list it")
            .map(|e| e.expect("an entry").file_name())
            .collect();
        let read = std::fs::read(&target);
        std::fs::remove_dir_all(&dir).expect("remove the scratch folder");
        saved.expect("a 250-byte name is saved");
        assert_eq!(read.expect("the saved file"), b"the whole file");
        assert_eq!(left, [std::ffi::OsString::from(name)], "nothing else");
    }

    /// A scratch folder, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "gospel_getter_files_{label}_{}_{:x}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos())
            ));
            std::fs::create_dir_all(&dir).expect("make a scratch folder");
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The folder is synced once the file is in place under its own name,
    /// whole, and the temporary file is gone: after the rename, not before.
    #[test]
    fn the_folder_is_synced_after_the_rename() {
        let scratch = Scratch::new("sync");
        let target = scratch.0.join("notes.md");
        let mut synced = None;
        write_atomically_then(&target, b"the whole file", |dir| {
            let names: Vec<_> = std::fs::read_dir(dir)
                .expect("list it")
                .map(|e| e.expect("an entry").file_name())
                .collect();
            synced = Some((dir.to_path_buf(), std::fs::read(&target).ok(), names));
            sync_dir(dir);
        })
        .expect("saved");
        let (dir, read, names) = synced.expect("the folder was synced");
        assert_eq!(dir, scratch.0);
        assert_eq!(read.as_deref(), Some(&b"the whole file"[..]));
        assert_eq!(names, [std::ffi::OsString::from("notes.md")]);
    }

    /// A save removes the temporary files of ours an earlier save left over
    /// an hour ago, and nothing else: not a newer one (a save under way), not
    /// a name that's only like ours, a folder or a link with our name.
    #[test]
    fn a_save_removes_our_old_temporary_files_and_nothing_else() {
        let scratch = Scratch::new("sweep");
        let dir = &scratch.0;
        let two_hours_ago = SystemTime::now() - Duration::from_secs(2 * 60 * 60);
        let make = |name: &str, old: bool| {
            let path = dir.join(name);
            let file = std::fs::File::create(&path).expect("make it");
            if old {
                file.set_modified(two_hours_ago).expect("age it");
            }
            path
        };
        let left = make(".gg-4242-18a2b3c4d5e6f-0.tmp", true);
        let under_way = make(".gg-4243-18a2b3c4d5e70-1.tmp", false);
        let alike: Vec<PathBuf> = [
            ".gg-notours.tmp",
            ".gg-12-ab.tmp",
            ".gg-12-ab-cd-ef.tmp",
            ".gg-12x-ab-cd.tmp",
            ".gg-12-ab-cd.tmp.bak",
            "gg-12-ab-cd.tmp",
            ".gg-12-ab-cd.temp",
            "report.tmp",
        ]
        .into_iter()
        .map(|name| make(name, true))
        .collect();
        let folder = dir.join(".gg-77-ab-cd.tmp");
        std::fs::create_dir(&folder).expect("a folder with our kind of name");
        #[cfg(unix)]
        let link = {
            let elsewhere = Scratch::new("sweep_elsewhere");
            let theirs = elsewhere.0.join("theirs.txt");
            std::fs::write(&theirs, "kept").expect("a file elsewhere");
            std::fs::File::options()
                .write(true)
                .open(&theirs)
                .and_then(|f| f.set_modified(two_hours_ago))
                .expect("age it");
            let link = dir.join(".gg-78-ab-cd.tmp");
            std::os::unix::fs::symlink(&theirs, &link).expect("a link with our kind of name");
            (elsewhere, theirs, link)
        };

        write_atomically(&dir.join("notes.md"), b"saved").expect("saved");
        assert!(!left.exists(), "an old temporary file of ours is removed");
        assert!(under_way.exists(), "a newer one is kept");
        for path in &alike {
            assert!(path.exists(), "{} is kept", path.display());
        }
        assert!(folder.is_dir());
        #[cfg(unix)]
        {
            let (_elsewhere, theirs, link) = link;
            assert!(std::fs::symlink_metadata(&link).is_ok(), "the link is kept");
            assert_eq!(std::fs::read_to_string(theirs).expect("theirs"), "kept");
        }
        assert_eq!(
            std::fs::read(dir.join("notes.md")).expect("the saved file"),
            b"saved"
        );
    }

    /// Every candidate name is short, hidden and never the same twice, and
    /// is one a later save would know as ours.
    #[test]
    fn temporary_names_are_bounded_hidden_and_distinct() {
        let names: Vec<String> = temp_names().chain(temp_names()).collect();
        for name in &names {
            assert!(name.starts_with(".gg-") && name.ends_with(".tmp"), "{name}");
            assert!(name.len() <= 68, "{name}");
            assert!(is_temp_name(name), "{name}");
        }
        let distinct: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(distinct.len(), names.len());
    }
}
