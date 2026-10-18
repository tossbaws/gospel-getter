//! Gospel Getter's Leptos pieces: Rust compiled to WebAssembly, loaded by
//! `ui/index.html` beside its JavaScript (Trunk builds the page; see
//! `Trunk.toml` and `README.md`). The first piece is the Export notes
//! dialog (`export_notes`); the page talks to it only as `bridge` says.
//!
//! If this module fails to load, the page works as before, except that
//! Export notes… says it is unavailable (see `bridge`).

mod bridge;
mod dom;
mod dto;
mod export_notes;
mod import_preview;
mod import_text;
mod install;
mod ipc;
mod shortcuts;
mod text;
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
    // The dialog is a modal overlay (position: fixed, above the page's
    // toast and banner by z-index), so where it sits in the body does not
    // matter; it goes at the end. The handle is forgotten on purpose: the
    // dialog lives as long as the page.
    leptos::mount::mount_to(body.clone(), export_notes::ExportNotes).forget();
    leptos::mount::mount_to(body.clone(), shortcuts::Shortcuts).forget();
    leptos::mount::mount_to(body.clone(), welcome::WelcomeCard).forget();
    leptos::mount::mount_to(body, import_preview::ImportDialog).forget();
    // Everything is listening: the page may send again what it asked
    // before the module had loaded.
    bridge::ready();
}
