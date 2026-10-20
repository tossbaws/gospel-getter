//! Gospel Getter's Leptos pieces: Rust compiled to WebAssembly, loaded by
//! `ui/index.html` beside its JavaScript (Trunk builds the page; see
//! `Trunk.toml` and `README.md`). The pieces so far are the page's modal
//! dialogs: Export notes (`export_notes`), the keyboard shortcuts sheet
//! (`shortcuts`), the welcome (`welcome`), the import preview
//! (`import_preview`) and the note editor (`note_editor`); and About
//! (`about`) with the updater (`updates`). The page talks to them only as
//! `bridge` says.
//!
//! If this module fails to load, the page works as before, except that the
//! five dialogs and About say they are unavailable when asked for (see
//! `bridge`), and no update is offered.

mod about;
mod bridge;
mod dom;
mod dto;
mod export_notes;
mod import_preview;
mod import_text;
mod install;
mod ipc;
mod note_editor;
mod note_text;
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
    // Everything is listening: the page may send again what it asked
    // before the module had loaded.
    bridge::ready();
}
