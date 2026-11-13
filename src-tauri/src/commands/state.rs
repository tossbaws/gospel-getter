//! The state every command shares (`AppState`), and how far the search
//! index's build has got.

use anyhow::Context;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::{BridgeScript, PendingImport, PendingNotesFolder};
use crate::db::{self, Book, Translation};
use crate::domain::Store;

/// Application state, `.manage()`d by Tauri and injected into every command
/// via `tauri::State`.
pub struct AppState {
    pub store: Store,
    /// All 66 books, loaded once at startup. Small and immutable, so it's
    /// kept in memory rather than re-queried on every command — both for
    /// rendering the book list and for computing chapter navigation.
    pub books: Vec<Book>,
    /// All bundled translations, loaded once at startup, in the order Aa
    /// lists them (`seed::TRANSLATIONS`). `translations[0]` (the KJV) is
    /// the default used when a command doesn't specify one.
    pub translations: Vec<Translation>,
    /// Progress of the search index build, which runs in the background
    /// after startup (see `main.rs`) rather than holding up the window.
    pub search_index: Arc<SearchIndexStatus>,
    /// The import file the reader is previewing, validated and waiting for
    /// them to confirm (or cancel). Only one at a time; choosing another
    /// file replaces it.
    pub pending_import: Mutex<Option<PendingImport>>,
    /// The export as one file per book waiting for the reader to say
    /// whether to replace the files already in their folder.
    pub pending_notes_folder: Mutex<Option<PendingNotesFolder>>,
    /// The next token for what waits on the reader: an import's preview,
    /// or a folder export's question.
    pub(super) next_import_token: AtomicU64,
    /// What the frontend tests' bridge answers in place of the system
    /// (`BridgeScript`). Never set in the app.
    bridge_script: Option<Box<dyn BridgeScript>>,
}

impl AppState {
    /// Load the books and translations that stay in memory for the life of
    /// the app. The database must already be migrated and seeded.
    pub async fn load(database: db::Db) -> anyhow::Result<Self> {
        let books = db::get_all_books(database.read())
            .await
            .context("Failed to load books")?;
        let translations = db::get_listed_translations(database.read())
            .await
            .context("Failed to load translations")?;
        Ok(Self {
            store: Store::new(database),
            books,
            translations,
            search_index: Arc::default(),
            pending_import: Mutex::new(None),
            pending_notes_folder: Mutex::new(None),
            next_import_token: AtomicU64::new(1),
            bridge_script: None,
        })
    }

    /// `self`, with `script` answering in place of the file dialogs, the
    /// browser, the clock and the updater: for the frontend tests' bridge
    /// only (see [`BridgeScript`]).
    #[doc(hidden)]
    #[must_use]
    pub fn with_bridge_script(mut self, script: impl BridgeScript + 'static) -> Self {
        self.bridge_script = Some(Box::new(script));
        self
    }

    /// The frontend tests' bridge's script, if it set one; never in the app.
    pub(crate) fn bridge_script(&self) -> Option<&dyn BridgeScript> {
        self.bridge_script.as_deref()
    }

    /// Open `url` in the system browser (`open::that_detached`), or, under
    /// the frontend tests' bridge, what its script does instead.
    #[cfg(desktop)]
    pub(crate) fn open_in_browser(&self, url: &str) -> std::io::Result<()> {
        match self.bridge_script() {
            Some(script) => script.open_in_browser(url),
            None => open::that_detached(url),
        }
    }
}

/// Whether the search index is ready to use.
#[derive(Debug, Default)]
pub struct SearchIndexStatus(AtomicU8);

impl SearchIndexStatus {
    const BUILDING: u8 = 0;
    const READY: u8 = 1;
    const FAILED: u8 = 2;

    pub fn set_ready(&self) {
        self.0.store(Self::READY, Ordering::Release);
    }

    pub fn set_failed(&self) {
        self.0.store(Self::FAILED, Ordering::Release);
    }

    /// `None` when ready, otherwise what to tell the reader.
    pub(super) fn unavailable_reason(&self) -> Option<&'static str> {
        match self.0.load(Ordering::Acquire) {
            Self::READY => None,
            Self::BUILDING => Some(
                "Search is still being prepared \u{2014} this only happens once. Try again in a moment.",
            ),
            _ => Some(
                "Search isn't available: its index couldn't be built. Restart the app to try again.",
            ),
        }
    }
}
