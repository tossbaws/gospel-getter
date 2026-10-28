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
//! screen, how it gets there, and the passage history; and the selection
//! card (`card`, with its markup and rules in `card_text`): the verse
//! selection, copying, and the card's bookmark, highlight and notes; and
//! the notes in the margin in reading mode (`margin`, with its rules and
//! markup in `margin_text`); a note's text as Markdown wherever it shows
//! (`note_markdown`), its links (`note_links`) and Copy as Markdown
//! (`note_copy`); and
//! what the page's main script was: the keyboard (`keyboard`, with its
//! rules in `keyboard_text`), the clicks (`clicks`), boot (`boot`) and the
//! page's glue (`page`, with its words and rules in `page_text`): the
//! toast, Export and Import data…, the welcome, the shortcuts sheet and
//! search opened, the palette's page commands, the launch check. What the
//! page's few remaining scripts share with this is in `bridge`.
//!
//! If this module fails to load, the chapter still shows, as plain text, in
//! the saved look (the page's small fallback reads it, and its pre-paint
//! script applies the look), with a line saying the rest of the reader
//! isn't available. That line is all a reader is told: nothing else on the
//! page answers a click or a key (there is no script left to say what isn't
//! available), nothing opens, no setting can be changed, and no update is
//! offered.

mod aa;
mod aa_text;
mod about;
mod boot;
// The books' abbreviations, shared with the backend, which compiles the same
// file.
#[path = "../../shared/book_aliases.rs"]
pub mod book_aliases;
mod bridge;
mod card;
mod card_text;
mod clicks;
mod dom;
mod dto;
mod export_notes;
mod import_preview;
mod import_text;
mod install;
mod ipc;
mod keyboard;
mod keyboard_text;
mod library;
mod library_text;
mod margin;
mod margin_text;
mod note_copy;
mod note_editor;
mod note_links;
mod note_markdown;
mod note_text;
mod page;
mod page_text;
mod palette;
// How a passage is named, shared with the backend, which compiles the same
// file.
#[path = "../../shared/passage_reference.rs"]
pub mod passage_reference;
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

/// Mounts every piece, the reader with the first (see `mount_the_rest`).
/// Called by wasm-bindgen once the module is loaded.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    // The page's own listeners first, where its main script's were: the ☰
    // items, the click dispatcher, the keyboard dispatcher. Each runs before
    // any piece's listener for the same event on the same element, as the
    // main script's did.
    page::mount();
    // Before anything under the document sees a click on a note's link.
    note_links::mount();
    // The popovers and panels, and the picker's shape: they were the page's
    // head script's, which ran before the other pieces' code.
    surfaces::mount();
    // Aa and the saved settings next: their records (the `<select>`s) are
    // filled before any piece reads them.
    aa::mount();
    // The selection card, which the reader tells about each chapter, and
    // the reader, before the rest; then boot: its chapter is on its way (or
    // in, and drawn now, within this call).
    card::mount();
    margin::mount();
    reader::mount();
    boot::start();
    // The table of shortcuts, where the page had it.
    bridge::export_shortcuts();
    // The rest in a task of its own, so the chapter is shown (painted, and
    // the window titled) before they mount, as it was before they loaded.
    let rest = Closure::once_into_js(mount_the_rest);
    if web_sys::window()
        .and_then(|w| w.set_timeout_with_callback(rest.unchecked_ref()).ok())
        .is_none()
    {
        mount_the_rest();
    }
}

/// Mounts the dialogs, About, the Library, search and the picker, then says
/// every piece is listening.
fn mount_the_rest() {
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
