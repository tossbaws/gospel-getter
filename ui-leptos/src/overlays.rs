//! The one list of the page's overlays: what can sit over the reader, each
//! by its element's id, and whether it's modal. "Is a dialog open?" has
//! this one answer, [`modal_open`], which the keyboard dispatcher
//! (`keyboard`), the reader's mouse Back and Forward (`reader`) and Aa's
//! text-size keys (`aa`) ask, so nothing moves behind a dialog (C2-01);
//! the card's Shift ghost asks [`any_open`], and the click dispatcher
//! (`clicks`) [`holds`].
//!
//! One list, because three lists that each named the dialogs disagreed:
//! the welcome was missing from the one the mouse buttons asked, so Back
//! turned the chapter behind it. A new dialog is one entry in [`Overlay`]:
//! `tests/frontend/dialogs.test.mjs` fails if the page has an overlay this
//! doesn't list (it reads `window.gospelGetterOverlays`, [`export`]).
//!
//! Each modal has the keyboard while it's open: a capturing `keydown` on
//! `window` (`dom::on_window_keydown`) that stops every other handler, and
//! so runs before the keyboard dispatcher (on `document`). Each keeps its
//! overlay's `hidden` truthful, which is what this reads.

use serde::Serialize;
use wasm_bindgen::JsValue;
use web_sys::Element;

use crate::dom::{closest, overlay_open};

/// An overlay of the page's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    /// ☰ → Import data…'s preview.
    Import,
    /// About, which `surfaces` opens and closes as one of its surfaces.
    About,
    NoteEditor,
    /// ☰ → Export data…'s chooser.
    ExportChoice,
    ExportNotes,
    /// The keyboard shortcuts sheet.
    Shortcuts,
    Welcome,
    /// The search box (search, Go to and the palette): over the reader,
    /// but not modal, so the reader's keys and the mouse buttons still
    /// work while it's open.
    Search,
}

impl Overlay {
    /// Every overlay.
    pub const ALL: [Self; 8] = [
        Self::Import,
        Self::About,
        Self::NoteEditor,
        Self::ExportChoice,
        Self::ExportNotes,
        Self::Shortcuts,
        Self::Welcome,
        Self::Search,
    ];

    /// The overlay's element's id: the element whose `hidden` says whether
    /// it's open.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Import => "import-overlay",
            Self::About => "about-overlay",
            Self::NoteEditor => "note-overlay",
            Self::ExportChoice => "export-choice-overlay",
            Self::ExportNotes => "export-notes-overlay",
            Self::Shortcuts => "shortcuts-overlay",
            Self::Welcome => "welcome-overlay",
            Self::Search => "search-panel",
        }
    }

    /// Whether it's modal: while it's open, the reader behind it takes no
    /// key, mouse button or click.
    pub const fn modal(self) -> bool {
        !matches!(self, Self::Search)
    }

    /// Whether it's open (looked up by id when asked: most are made when
    /// their piece mounts).
    pub fn is_open(self) -> bool {
        overlay_open(self.id())
    }
}

/// A modal is open.
pub fn modal_open() -> bool {
    Overlay::ALL.iter().any(|o| o.modal() && o.is_open())
}

/// Any overlay is open, the search box included.
pub fn any_open() -> bool {
    Overlay::ALL.iter().any(|o| o.is_open())
}

/// Whether `el` is in an overlay (open or not).
pub fn holds(el: &Element) -> bool {
    Overlay::ALL
        .iter()
        .any(|o| closest(el, &format!("#{}", o.id())).is_some())
}

/// One row of `window.gospelGetterOverlays`.
#[derive(Serialize)]
struct Row {
    id: &'static str,
    modal: bool,
}

/// `window.gospelGetterOverlays`, for the frontend tests: every overlay,
/// as `{ id, modal }`, in order.
pub fn export() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let rows: Vec<Row> = Overlay::ALL
        .iter()
        .map(|&o| Row {
            id: o.id(),
            modal: o.modal(),
        })
        .collect();
    let serializer = serde_wasm_bindgen::Serializer::json_compatible();
    if let Ok(rows) = rows.serialize(&serializer) {
        let _ = js_sys::Reflect::set(&window, &JsValue::from_str("gospelGetterOverlays"), &rows);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_overlay_is_listed_once_with_its_own_id() {
        let mut ids: Vec<&str> = Overlay::ALL.iter().map(|o| o.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), Overlay::ALL.len());
    }

    #[test]
    fn only_the_search_box_is_not_modal() {
        let not_modal: Vec<Overlay> = Overlay::ALL
            .iter()
            .copied()
            .filter(|o| !o.modal())
            .collect();
        assert_eq!(not_modal, [Overlay::Search]);
    }
}
