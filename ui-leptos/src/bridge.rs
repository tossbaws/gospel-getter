//! The whole bridge between this crate and the page's JavaScript
//! (`ui/index.html`), in one place. Everything the two sides share is
//! listed here; nothing else crosses.
//!
//! JavaScript to Rust:
//! - [`OPEN_EVENT`], a cancelable `CustomEvent` on `document`. The page's
//!   click handler on `#export-notes-btn` (the Library's Notes tab; the
//!   command palette clicks the same button) dispatches it with an
//!   [`OpenRequest`] as its `detail`: a snapshot of what the dialog needs
//!   to know, so it reads none of the page's variables. The dialog cancels
//!   the event to say it handled it; if nobody does (the WebAssembly did
//!   not load), the page tells the reader Export notes is unavailable.
//!
//! Rust to JavaScript:
//! - [`TOAST_EVENT`], a `CustomEvent` on `document` with
//!   `{ message, isError }` as its `detail`; the page shows it in its toast
//!   (`setDataStatus`), where the export reported before.
//! - `#export-notes-overlay`'s `hidden` attribute, which this crate keeps
//!   truthful: the page's keyboard dispatcher and its Shift-ghost check
//!   read it to know a modal is open.
//! - Focus: closing hands focus back to `#export-notes-btn` while it is on
//!   the page, shown and enabled; otherwise to the page's focus home, Aa
//!   (`#aa-toggle`) in reading mode (`html[data-reader-mode="on"]`) or the
//!   menu (`#menu-toggle`), as the page's own `focusHome` does.
//!
//! Shared with the backend, not the page: the three commands this dialog
//! invokes, through the same `window.__TAURI__.core.invoke` the page uses
//! (see `ipc.rs`).

use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use web_sys::{CustomEvent, CustomEventInit, Document, HtmlElement};

/// The page asks the dialog to open: `document.dispatchEvent(new
/// CustomEvent('gospel-getter:export-notes', { cancelable: true, detail }))`.
pub const OPEN_EVENT: &str = "gospel-getter:export-notes";

/// The dialog has something to say in the page's toast.
pub const TOAST_EVENT: &str = "gospel-getter:toast";

/// What the page knows and the dialog needs, taken when the button is
/// clicked: how many notes there are (none: nothing to export), the book
/// being read (ticked first, if it has notes), and the translation whose
/// text the file quotes (the one chosen in Aa).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRequest {
    pub note_count: u64,
    pub book_id: Option<i64>,
    pub translation_code: String,
    pub translation_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Toast<'a> {
    message: &'a str,
    is_error: bool,
}

/// The page's document, when there is one (there always is in the webview;
/// this crate never assumes it).
pub fn document() -> Option<Document> {
    web_sys::window().and_then(|w| w.document())
}

/// Says `message` in the page's toast; an error stays longer and is red.
pub fn toast(message: &str, is_error: bool) {
    let Some(document) = document() else {
        return;
    };
    let Ok(detail) = serde_wasm_bindgen::to_value(&Toast { message, is_error }) else {
        return;
    };
    let init = CustomEventInit::new();
    init.set_detail(&detail);
    if let Ok(event) = CustomEvent::new_with_event_init_dict(TOAST_EVENT, &init) {
        let _ = document.dispatch_event(&event);
    }
}

/// The Library's Export notes… button, if the page has it.
pub fn export_button() -> Option<HtmlElement> {
    document()?
        .get_element_by_id("export-notes-btn")?
        .dyn_into::<HtmlElement>()
        .ok()
}

/// Where focus goes when what had it is gone, as the page's `focusHome`:
/// Aa in reading mode (which hides the menu), else the menu button. A
/// plain focus, as there.
pub fn focus_home() {
    let Some(document) = document() else {
        return;
    };
    let reader_mode = document
        .document_element()
        .is_some_and(|html| html.get_attribute("data-reader-mode").as_deref() == Some("on"));
    let id = if reader_mode {
        "aa-toggle"
    } else {
        "menu-toggle"
    };
    if let Some(target) = document
        .get_element_by_id(id)
        .and_then(|el| el.dyn_into::<HtmlElement>().ok())
    {
        let _ = target.focus();
    }
}

/// `document.createElement` et al. are not needed; this reads an event's
/// `detail` as an [`OpenRequest`], or says what was wrong with it.
pub fn open_request(event: &CustomEvent) -> Result<OpenRequest, JsValue> {
    serde_wasm_bindgen::from_value(event.detail()).map_err(|e| JsValue::from(e.to_string()))
}
