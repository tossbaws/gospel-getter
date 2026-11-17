//! What crosses between this crate and the page's own scripts, the globals
//! the pieces set for the tests, and the page's markup and local storage
//! they use as a reader would, in one place.
//!
//! The page's own scripts (the pre-paint script, boot's prefetch, the
//! loader and the plain-text fallback) read none of this crate's
//! variables, nor it theirs; what each does is in this crate's `README.md`
//! ("Building and running"). Only two events cross: [`BOOT_EVENT`] (page to
//! Rust, [`boot_prefetch`]) and `gospel-getter:pieces-failed` (loader to
//! fallback, on `document`, no `detail`; nothing in this crate hears it).
//! The pieces call each other directly; one that mounts after the first
//! chapter keeps a handle to itself from its mount (see `page`).
//!
//! Globals this crate sets for the frontend tests and the GUI checks (the
//! page reads none of them): `window.gospelGetterShortcuts`, the table of
//! shortcuts (`shortcut_table`, [`export_shortcuts`]);
//! `window.gospelGetterPreferences` (`aa`): `current()`, the display
//! settings an export carries, and `apply(values)`;
//! `window.gospelGetterSurfaces` (`surfaces`): `current()`, the open
//! header surface's name or `null`; `window.gospelGetterLibrary`
//! (`library`): `refresh(lists)`, the Library's lists reloaded; and
//! `window.gospelGetterPanes` (`pane`): a second pane opened beside and
//! closed, and which panes show and which is active.
//!
//! Page markup this crate uses as a reader would: `#theme-select`, read and
//! changed (with a `change` event, as Aa does) for the welcome's looks
//! ([`theme`], [`choose_theme`]); the page's focus home (`#aa-toggle` in
//! reading mode, else `#menu-toggle`, [`focus_home`]); `#translation-select`,
//! the translation button's record ([`translation_code`]), whose `change`
//! the reader and the Library hear first (`page`), then the search box; the
//! controls the palette's commands click as a reader would, by id
//! (`#compare-btn`, `#reader-mode-btn`, `#reader-exit`,
//! `#random-chapter-btn`, `#sidebar-toggle`, `#import-data-btn`,
//! `#welcome-btn`, `#about-btn` then `#update-check-btn`, `#library-toggle`
//! then a `#library-tab-*`),
//! or a `[data-value]` choice in `#layout-choices`, `#width-choices` or
//! `#theme-choices`.
//!
//! The pieces' overlays (`#export-choice-overlay`, `#export-notes-overlay`,
//! `#shortcuts-overlay`,
//! `#welcome-overlay`, `#import-overlay`, `#note-overlay`, `#about-overlay`)
//! keep their `hidden` truthful: the keyboard dispatcher (`keyboard`) and
//! the card's Shift ghost read them, looked up by id when asked, to know a
//! modal is open. Each piece has the keyboard while it's open: a capturing
//! `keydown` on `window` that stops every other handler, and so runs
//! before the keyboard dispatcher (on `document`).
//!
//! Local storage: the saved settings' keys are this crate's to write
//! (`prefs`), as the one table says (`preferences`); the pre-paint script
//! only reads them. The welcome reads the `gospel-getter-*` keys (the
//! fresh-install rule) and writes [`WELCOME_KEY`]; the reader keeps the
//! disclosures' (`gospel-getter-footnotes`, `gospel-getter-cross-references`).

use serde::Serialize;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use web_sys::{CustomEvent, CustomEventInit, Document, HtmlElement};

/// The page's document, when there is one (there always is in the webview;
/// this crate never assumes it).
pub fn document() -> Option<Document> {
    web_sys::window().and_then(|w| w.document())
}

/// Where focus goes when what had it is gone: Aa in reading mode (which
/// hides the menu), else the menu button. A plain focus.
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

/// Puts the table of shortcuts at `window.gospelGetterShortcuts`, for the
/// frontend tests and the GUI checks: an array of
/// `{ id, group, keys, aria, does, handles }`.
pub fn export_shortcuts() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let serializer = serde_wasm_bindgen::Serializer::json_compatible();
    if let Ok(table) = crate::shortcut_table::SHORTCUTS.serialize(&serializer) {
        let _ = js_sys::Reflect::set(&window, &JsValue::from_str("gospelGetterShortcuts"), &table);
    }
}

/// Aa's record of the theme: `#theme-select`, a hidden `<select>`. The
/// welcome reads it to know which look is in use, and picks a look the way
/// Aa does: setting its value and sending it a `change` event, which Aa
/// applies and saves (`aa`).
const THEME_SELECT: &str = "theme-select";

/// The theme in use, as `#theme-select` has it.
pub fn theme() -> String {
    document()
        .and_then(|d| d.get_element_by_id(THEME_SELECT))
        .and_then(|el| el.dyn_into::<web_sys::HtmlSelectElement>().ok())
        .map(|select| select.value())
        .unwrap_or_default()
}

/// Uses the theme `value`, through Aa's own control (see [`theme`]).
pub fn choose_theme(value: &str) {
    let Some(select) = document()
        .and_then(|d| d.get_element_by_id(THEME_SELECT))
        .and_then(|el| el.dyn_into::<web_sys::HtmlSelectElement>().ok())
    else {
        return;
    };
    if select.value() == value {
        return;
    }
    select.set_value(value);
    let init = web_sys::EventInit::new();
    init.set_bubbles(true);
    if let Ok(change) = web_sys::Event::new_with_event_init_dict("change", &init) {
        let _ = select.dispatch_event(&change);
    }
}

/// The page's local storage keys, or `None` if it can't be read (it can
/// throw in the webview, so every use is guarded).
pub fn stored_keys() -> Option<Vec<String>> {
    let storage = web_sys::window()?.local_storage().ok()??;
    let length = storage.length().ok()?;
    (0..length)
        .map(|i| storage.key(i).ok())
        .collect::<Option<Vec<_>>>()
        .map(|keys| keys.into_iter().flatten().collect())
}

/// The welcome's key in local storage, beside the display settings: set to
/// `seen` when it closes. The earlier four-page tour used the same key, so
/// anyone who saw that never gets the welcome on upgrade.
pub const WELCOME_KEY: &str = "gospel-getter-welcome";

/// Records that the welcome has been seen; quietly not, if local storage
/// can't be written.
pub fn welcome_seen() {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(WELCOME_KEY, "seen");
    }
}

/// Whether the launch check is on: `html[data-update-check]` isn't `off`
/// (the pre-paint script applies the stored setting there before the page
/// draws, and `aa` keeps it).
pub fn launch_check_on() -> bool {
    document()
        .and_then(|d| d.document_element())
        .is_none_or(|html| html.get_attribute("data-update-check").as_deref() != Some("off"))
}

/// The translation chosen with the translation button
/// (`#translation-select`'s value), whose text the Library previews, as the
/// page's `currentTranslation` read it.
pub fn translation_code() -> String {
    document()
        .and_then(|d| d.get_element_by_id("translation-select"))
        .and_then(|el| el.dyn_into::<web_sys::HtmlSelectElement>().ok())
        .map(|select| select.value())
        .unwrap_or_default()
}

/// Boot's prefetch is asked for what it has sent (`boot`): a cancelable
/// `CustomEvent` on `document`, its `detail` `{}`. The page's prefetch
/// script cancels it and puts in `detail.boot` its state ([`Prefetch`]).
pub const BOOT_EVENT: &str = "gospel-getter:boot";

/// What boot's prefetch (the page's script in `<head>`) has sent, and what
/// has come back: `{ asked, home, failed, error, reading, answered }`, as it
/// is now (it fills in as the answers come).
pub struct Prefetch(JsValue);

impl Prefetch {
    fn field(&self, name: &str) -> Option<JsValue> {
        js_sys::Reflect::get(&self.0, &JsValue::from_str(name))
            .ok()
            .filter(|value| !value.is_null() && !value.is_undefined())
    }

    /// Settles once `get_home` has answered (or failed) and, with its
    /// answer, `get_reading` has been sent; never rejects. Adopted, so any
    /// promise will do (one from another realm too, as the frontend tests'
    /// `invoke` answers).
    pub fn asked(&self) -> Option<js_sys::Promise> {
        self.field("asked")
            .map(|asked| js_sys::Promise::resolve(&asked))
    }

    /// `get_home`'s answer, once it's in.
    pub fn home(&self) -> Option<JsValue> {
        self.field("home")
    }

    /// `get_home`'s failure (or the prefetch's own, reading its answer),
    /// once it's in: what it was thrown as.
    pub fn failure(&self) -> Option<JsValue> {
        self.field("failed")
            .filter(JsValue::is_truthy)
            .map(|_| self.field("error").unwrap_or(JsValue::UNDEFINED))
    }

    /// The chapter's `get_reading`, as boot's reader takes it: answered, or
    /// on its way, or neither (comparing: the reader asks for it beside the
    /// comparison).
    pub fn reading(&self) -> Option<crate::reader::Sent> {
        if let Some(answered) = self.field("answered") {
            let dto =
                js_sys::Reflect::get(&answered, &JsValue::from_str("dto")).unwrap_or(JsValue::NULL);
            return Some(crate::reader::Sent::Answered(dto));
        }
        self.field("reading")
            .map(|reading| crate::reader::Sent::Asked(js_sys::Promise::resolve(&reading)))
    }
}

/// Boot's prefetch, if the page has it ([`BOOT_EVENT`]).
pub fn boot_prefetch() -> Option<Prefetch> {
    let detail = js_sys::Object::new();
    if !request(BOOT_EVENT, &detail) {
        return None;
    }
    js_sys::Reflect::get(&detail, &JsValue::from_str("boot"))
        .ok()
        .filter(JsValue::is_object)
        .map(Prefetch)
}

/// Sends `name` on `document` as a cancelable `CustomEvent` with `detail`
/// as it is: whether something cancelled it (has it).
fn request(name: &str, detail: &JsValue) -> bool {
    let Some(document) = document() else {
        return false;
    };
    let init = CustomEventInit::new();
    init.set_cancelable(true);
    init.set_detail(detail);
    CustomEvent::new_with_event_init_dict(name, &init)
        .is_ok_and(|event| !document.dispatch_event(&event).unwrap_or(true))
}
