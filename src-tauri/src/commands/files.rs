//! What the exports and the import share: writing a file completely or not
//! at all, and running the native file dialog off the async runtime.

use anyhow::Context;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) fn write_atomically(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;
    path.file_name()
        .with_context(|| format!("{} isn't a file name", path.display()))?;
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let write = || -> std::io::Result<()> {
        let mut temp = create_temp_file(&dir, temp_names())?;
        let file = temp.file.as_mut().expect("open until persisted");
        file.write_all(bytes)?;
        file.sync_all()?;
        temp.persist_as(path)
    };
    write().with_context(|| format!("Couldn't write {}", path.display()))
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

    /// Every candidate name is short, hidden and never the same twice.
    #[test]
    fn temporary_names_are_bounded_hidden_and_distinct() {
        let names: Vec<String> = temp_names().chain(temp_names()).collect();
        for name in &names {
            assert!(name.starts_with(".gg-") && name.ends_with(".tmp"), "{name}");
            assert!(name.len() <= 68, "{name}");
        }
        let distinct: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(distinct.len(), names.len());
    }
}
