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
//! keep the saved settings (`prefs`, by the one table in `preferences`);
//! and the header's popovers and panels themselves, with the picker's two
//! shapes (`surfaces`), which open and close all of these; and the reader
//! (`reader`, with its markup and rules in `reader_text`): the chapter on
//! screen, how it gets there, and the passage history. The page talks to
//! them only as `bridge` says.
//!
//! If this module fails to load, the chapter still shows, as plain text, in
//! the saved look (the page's small fallback reads it, and its pre-paint
//! script applies the look), with a line saying the rest of the reader
//! isn't available: nothing in it can be selected, turned or changed. The
//! five dialogs, About, the Library, search, Aa, the translations, the ☰
//! menu and the picker say they are unavailable when asked for (see
//! `bridge`), nothing opens, no setting can be changed (reading mode can't
//! be left: its button and Escape say so), and no update is offered.

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
mod reader;
mod reader_text;
mod search;
mod search_text;
mod shortcut_table;
mod shortcuts;
mod surfaces;
mod surfaces_text;
mod text;
mod update_text;
mod updates;
// A verse's parts and footnotes, shared with the backend, which compiles the
// same file.
#[path = "../../shared/verse_parts.rs"]
pub mod verse_parts;
mod welcome;

use wasm_bindgen::prelude::*;

/// Mounts every piece. Called by wasm-bindgen once the module is loaded.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    // The popovers and panels, and the picker's shape, first: they were the
    // page's head script's, which ran before the other pieces' code.
    surfaces::mount();
    // Aa and the saved settings next: their records (the `<select>`s) are
    // filled before any piece reads them.
    aa::mount();
    // The reader, before the rest: boot's chapter is on its way, and needn't
    // wait for every other piece to mount (it's drawn once they have).
    reader::mount();
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
    // The Library's tabs and lists go inside the page's panel, and
    // the search box's contents inside the page's search panel.
    library::mount();
    search::mount();
    // The picker's books and chapters go inside the page's picker.
    picker::mount();
    // Everything is listening: the page may send again what it asked
    // before the module had loaded.
    bridge::ready();
}
