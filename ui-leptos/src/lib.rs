//! Gospel Getter's Leptos pieces: Rust compiled to WebAssembly, loaded by
//! `ui/index.html` beside its JavaScript (Trunk builds the page; see
//! `Trunk.toml` and `README.md`). The pieces so far are the page's modal
//! dialogs: Export notes (`export_notes`), the keyboard shortcuts sheet
//! (`shortcuts`), the welcome (`welcome`), the import preview
//! (`import_preview`) and the note editor (`note_editor`); About
//! (`about`) with the updater (`updates`); the Library's tabs and lists
//! (`library`); and search with the command palette (`search`), with the
//! table of keyboard shortcuts (`shortcut_table`) and the palette's
//! commands (`palette`); the book and chapter picker's books and chapters
//! (`picker`); and Aa with the translation button's popover (`aa`), which
//! keep the saved settings (`prefs`, by the one table in `preferences`).
//! The page talks to them only as `bridge` says.
//!
//! If this module fails to load, the Bible text still shows, in the saved
//! look (the page's pre-paint script applies it), and the page reads on;
//! but the five dialogs, About, the Library, search, Aa, the translations
//! and the picker's books say they are unavailable when asked for (see
//! `bridge`), no setting can be changed (reading mode can't be left), no
//! update is offered, and no bookmark is marked in the reader (the Library
//! loads them).

mod aa;
mod aa_text;
mod about;
mod bridge;
mod dom;
mod dto;
mod export_notes;
mod import_preview;
mod import_text;
mod install;
mod ipc;
mod library;
mod library_text;
mod note_editor;
mod note_text;
mod palette;
mod picker;
mod picker_text;
// The saved settings' one table, shared with the backend (src-tauri), which
// compiles the same file.
#[path = "../../shared/preferences.rs"]
pub mod preferences;
mod prefs;
mod search;
mod search_text;
mod shortcut_table;
mod shortcuts;
mod text;
mod update_text;
mod updates;
mod welcome;

use wasm_bindgen::prelude::*;

/// Mounts every piece. Called by wasm-bindgen once the module is loaded.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    // Aa and the saved settings first: their records (the `<select>`s) are
    // filled before any piece reads them.
    aa::mount();
    // The table of shortcuts, where the page had it.
    bridge::export_shortcuts();
    let Some(body) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.body())
    else {
        return;
    };
    // Each dialog is a modal overlay (position: fixed, above the page's
    // toast and banner by z-index), so where it sits in the body does not
    // matter; they go at the end. The handles are forgotten on purpose:
    // the dialogs live as long as the page.
    leptos::mount::mount_to(body.clone(), export_notes::ExportNotes).forget();
    leptos::mount::mount_to(body.clone(), shortcuts::Shortcuts).forget();
    leptos::mount::mount_to(body.clone(), welcome::WelcomeCard).forget();
    leptos::mount::mount_to(body.clone(), import_preview::ImportDialog).forget();
    leptos::mount::mount_to(body, note_editor::NoteEditor).forget();
    // About and the update banner are not modals stacked anywhere: each
    // goes where the page had it (focus order), so they are drawn in a
    // holder first and moved there.
    if let Some(holder) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.create_element("div").ok())
        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
    {
        leptos::mount::mount_to(holder.clone(), about::AboutAndUpdates).forget();
        about::place(&holder);
    }
    // The Library's tabs and lists go inside the head script's panel, and
    // the search box's contents inside the page's search panel.
    library::mount();
    search::mount();
    // The picker's books and chapters go inside the page's picker.
    picker::mount();
    // Everything is listening: the page may send again what it asked
    // before the module had loaded.
    bridge::ready();
}
