use std::path::Path;

/// If the app's own database doesn't exist yet, but a database from the
/// pre-Tauri install (the systemd/browser-wrapper app, which kept its data
/// at `~/.local/share/gospel-getter/data/gospel_getter.db`) does, copy it
/// into place so the reader's translations and reading position survive
/// the move to the Tauri app data directory.
///
/// Only ever reads and copies the legacy file — never moves, modifies, or
/// deletes it — so a failed or repeated migration can't lose data, and the
/// old install stays intact until the reader chooses to remove it
/// themselves. Returns whether a copy actually happened.
pub fn migrate_legacy_db_if_needed(
    new_db_path: &Path,
    legacy_db_path: &Path,
) -> anyhow::Result<bool> {
    if new_db_path.exists() {
        return Ok(false);
    }
    if !legacy_db_path.is_file() {
        return Ok(false);
    }
    if let Some(parent) = new_db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(legacy_db_path, new_db_path)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A fresh, isolated scratch directory under the OS temp dir — never
    /// anywhere near a real install's data — cleaned up at the end of each
    /// test regardless of outcome.
    struct ScratchDir(std::path::PathBuf);

    impl ScratchDir {
        fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "gospel_getter_legacy_migration_test_{label}_{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("create scratch dir");
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn copies_legacy_db_when_new_db_is_absent() {
        let dir = ScratchDir::new("copy");
        let legacy = dir.path().join("legacy.db");
        let new_db = dir.path().join("app_data").join("gospel_getter.db");
        fs::write(&legacy, b"legacy bytes").unwrap();

        let copied = migrate_legacy_db_if_needed(&new_db, &legacy).unwrap();

        assert!(copied, "should report that a copy happened");
        assert_eq!(
            fs::read(&new_db).unwrap(),
            b"legacy bytes",
            "new db should contain exactly the legacy db's bytes"
        );
        assert_eq!(
            fs::read(&legacy).unwrap(),
            b"legacy bytes",
            "legacy db must be left completely untouched"
        );
    }

    #[test]
    fn does_not_overwrite_an_existing_new_db() {
        let dir = ScratchDir::new("no_overwrite");
        let legacy = dir.path().join("legacy.db");
        let new_db = dir.path().join("gospel_getter.db");
        fs::write(&legacy, b"legacy bytes").unwrap();
        fs::write(&new_db, b"already migrated").unwrap();

        let copied = migrate_legacy_db_if_needed(&new_db, &legacy).unwrap();

        assert!(
            !copied,
            "should not report a copy when the new db already exists"
        );
        assert_eq!(
            fs::read(&new_db).unwrap(),
            b"already migrated",
            "an existing new db must never be overwritten"
        );
    }

    #[test]
    fn does_nothing_when_neither_db_exists() {
        let dir = ScratchDir::new("neither");
        let legacy = dir.path().join("legacy.db");
        let new_db = dir.path().join("gospel_getter.db");

        let copied = migrate_legacy_db_if_needed(&new_db, &legacy).unwrap();

        assert!(!copied);
        assert!(!new_db.exists());
    }
}
