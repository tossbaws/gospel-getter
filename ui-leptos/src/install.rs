//! Whether the app is a fresh install, for the welcome: the rule the page's
//! `isFreshInstall` applied, kept free of the DOM so it is tested natively
//! (`cargo test`). The page knows the facts (what its lists loaded); this
//! decides.

use serde::Deserialize;

/// What the page knows when its boot has finished. A list that couldn't be
/// loaded counts as one that has something in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Install {
    /// The backend has a saved reading position (any chapter read saves
    /// one, so every upgrade has it).
    pub has_saved_position: bool,
    pub bookmarks_loaded: bool,
    pub bookmarks: u64,
    pub highlights_loaded: bool,
    pub highlights: u64,
    pub notes_loaded: bool,
    pub notes: u64,
}

/// The prefix of every key the page keeps in local storage: display
/// settings, Library disclosures, and the welcome's own.
pub const KEY_PREFIX: &str = "gospel-getter-";

/// A fresh install: nothing read, bookmarked, highlighted, noted or
/// customised yet, and the welcome never closed before (no
/// `gospel-getter-*` key in local storage). `stored_keys` is `None` when
/// local storage couldn't be read; anything that can't be checked counts as
/// existing data, so the welcome errs towards not appearing.
pub fn is_fresh(install: &Install, stored_keys: Option<&[String]>) -> bool {
    if install.has_saved_position || !install.bookmarks_loaded || install.bookmarks > 0 {
        return false;
    }
    if !install.highlights_loaded || install.highlights > 0 {
        return false;
    }
    if !install.notes_loaded || install.notes > 0 {
        return false;
    }
    stored_keys.is_some_and(|keys| !keys.iter().any(|key| key.starts_with(KEY_PREFIX)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> Install {
        Install {
            bookmarks_loaded: true,
            highlights_loaded: true,
            notes_loaded: true,
            ..Install::default()
        }
    }

    #[test]
    fn nothing_saved_anywhere_is_fresh() {
        assert!(is_fresh(&empty(), Some(&[])));
        assert!(is_fresh(&empty(), Some(&["other-app".to_string()])));
    }

    #[test]
    fn any_saved_data_is_not() {
        let cases = [
            Install { has_saved_position: true, ..empty() },
            Install { bookmarks: 1, ..empty() },
            Install { highlights: 2, ..empty() },
            Install { notes: 3, ..empty() },
        ];
        for install in cases {
            assert!(!is_fresh(&install, Some(&[])), "{install:?}");
        }
    }

    #[test]
    fn a_list_that_did_not_load_counts_as_data() {
        let cases = [
            Install { bookmarks_loaded: false, ..empty() },
            Install { highlights_loaded: false, ..empty() },
            Install { notes_loaded: false, ..empty() },
        ];
        for install in cases {
            assert!(!is_fresh(&install, Some(&[])), "{install:?}");
        }
    }

    #[test]
    fn a_stored_setting_or_the_welcome_seen_is_not_fresh() {
        for key in ["gospel-getter-welcome", "gospel-getter-theme", "gospel-getter-compare"] {
            assert!(!is_fresh(&empty(), Some(&[key.to_string()])), "{key}");
        }
    }

    #[test]
    fn unreadable_storage_is_not_fresh() {
        assert!(!is_fresh(&empty(), None));
    }
}
