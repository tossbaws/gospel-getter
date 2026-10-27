//! What this crate shares with the page's JavaScript, and the events its
//! pieces send each other, in one place.
//!
//! The page's JavaScript is now four small inline scripts, none of which
//! reads this crate's variables or this crate theirs:
//! - the pre-paint script (`<head>`): the saved look, as `<html>`'s data
//!   attributes, before the first frame (see `preferences`);
//! - boot's prefetch (`<head>`): boot's first two commands, sent at once so
//!   they're on their way while the module loads: `get_home`, then, with its
//!   answer, the chapter the reader was on (`get_reading`, unless
//!   comparing). This crate takes them over with [`BOOT_EVENT`]
//!   ([`boot_prefetch`], `boot`);
//! - the loader (Trunk inlines `ui/leptos-loader.js` in `<head>`), which
//!   fetches, compiles and starts this module, and says
//!   `gospel-getter:pieces-failed` on `document` (no `detail`) if it can't;
//! - the plain-text fallback (the end of `<body>`), which hears that and
//!   shows the chapter as plain text. Nothing in this crate hears it.
//!
//! So these cross, and only these: [`BOOT_EVENT`] (page to Rust: a
//! cancelable `CustomEvent` on `document` whose `detail` is `{}`; the
//! prefetch cancels it and puts in `detail.boot` its state, [`Prefetch`])
//! and `gospel-getter:pieces-failed` (loader to fallback). Without this
//! module nothing else answers anything: the reader sees the chapter as
//! plain text with the fallback's line saying the rest isn't available, and
//! every control does nothing.
//!
//! Globals this crate sets for the frontend tests and the GUI checks (the
//! page reads none of them): `window.gospelGetterShortcuts`, the table of
//! shortcuts (`shortcut_table`, [`export_shortcuts`]);
//! `window.gospelGetterPreferences` (`aa`): `current()`, the display
//! settings an export carries, and `apply(values)`; and
//! `window.gospelGetterSurfaces` (`surfaces`): `current()`, the open
//! header surface's name or `null`.
//!
//! Within this crate, the pieces send each other custom events on
//! `document` (a `detail` snapshot each, as below), so a piece mounted
//! later hears what it missed when [`READY_EVENT`] comes:
//! - Requests to open, each cancelable; the piece cancels it to say it has
//!   it, and if nothing does (it hasn't mounted yet), the asker says it
//!   isn't available (`page`): [`OPEN_EVENT`] (`gospel-getter:export-notes`,
//!   Export notes…, an [`OpenRequest`]); [`SHORTCUTS_EVENT`]
//!   (`gospel-getter:shortcuts`, the sheet); [`WELCOME_EVENT`]
//!   (`gospel-getter:welcome`, at the end of boot with what the
//!   fresh-install rule needs, or from ☰, with the places to start, a
//!   [`WelcomeRequest`]; boot's is asked again on [`READY_EVENT`] if nobody
//!   answered); [`IMPORT_EVENT`] (`gospel-getter:import`, no detail);
//!   [`SEARCH_EVENT`] (`gospel-getter:search`, `{ open }`); and
//!   `detail.returnFocus` on the sheet's and welcome's ([`return_focus`]).
//! - [`LAUNCH_CHECK_EVENT`] (`gospel-getter:update-check`): boot is done,
//!   so the updater runs its quiet launch check, unless
//!   `html[data-update-check]` is `off` ([`launch_check_on`]); asked again
//!   on [`READY_EVENT`] if nobody answered.
//! - [`LIBRARY_REFRESH_EVENT`] (`gospel-getter:library-refresh`): the one
//!   way to have the Library's lists reloaded (boot, an import, a
//!   translation switch, the card's bookmark, highlight and restored note),
//!   `{ lists }`; the Library cancels it and puts in `detail.done` a promise
//!   that settles once each list has its answer. Asked before the Library
//!   has mounted, it's asked again on [`READY_EVENT`] ([`refresh_library`]).
//!   [`LIBRARY_LOADED_EVENT`] (`gospel-getter:library-loaded`, `{ list,
//!   items }`) answers with the bookmarks (the card's markers) and the notes
//!   (`page`'s copy, for the welcome and Export notes).
//! - [`PICKER_BOOKS_EVENT`] (`gospel-getter:picker-books`, `{ otBooks,
//!   ntBooks }`, boot's, sent again on [`READY_EVENT`]),
//!   [`PICKER_CURRENT_EVENT`] (`gospel-getter:picker-current`, the
//!   reader's chapter on screen) and [`PICKER_SHOWN_EVENT`]
//!   (`gospel-getter:picker-shown`, `surfaces` showed the picker): the
//!   picker's.
//! - [`ABOUT_CREDITS_EVENT`] (`gospel-getter:about-credits`, `{ credits }`,
//!   the reader's from boot, again on [`READY_EVENT`]): About's.
//! - [`SURFACE_OPEN_EVENT`] and [`SURFACE_CLOSE_EVENT`]
//!   (`gospel-getter:surface-open`, `-close`, `{ name }`): a header surface
//!   opened or closed (`surfaces`); one opening closes the search box, and
//!   About closing part way through an install brings the banner back.
//! - [`NOTE_EVENT`] (`gospel-getter:note`, a [`NoteRequest`], cancelable):
//!   the card opens the note editor; [`NOTE_OTHERS_EVENT`]
//!   (`gospel-getter:note-others`) follows the selection's notes while it
//!   may be open; [`NOTE_DONE_EVENT`] (`gospel-getter:note-done`, `{ action,
//!   note }`) says what the editor did.
//! - `aa::COMPARE_CHANGE_EVENT` (`gospel-getter:compare-change`) and
//!   `aa::LAYOUT_CHANGE_EVENT` (`gospel-getter:layout-change`): Compare or
//!   the layout changed (the reader and the card).
//! - [`READY_EVENT`] (`gospel-getter:pieces-ready`): every piece has
//!   mounted.
//!
//! Direct calls, where the page's script used to be in between: the toast
//! ([`toast`], `page`), a modal opening ([`modal_opening`]), the ☰ items
//! kept disabled ([`data_busy`]), an import's reloads ([`imported`]), what
//! the palette's commands need ([`palette_state`]) and the commands whose
//! code was the page's ([`page_command`]); and the reader's and the card's
//! own `reader::ask` and `card::ask`.
//!
//! Page markup this crate uses as a reader would: `#theme-select`, read and
//! changed (with a `change` event, as Aa does) for the welcome's looks
//! ([`theme`], [`choose_theme`]); the page's focus home (`#aa-toggle` in
//! reading mode, else `#menu-toggle`, [`focus_home`]); `#translation-select`,
//! the translation button's record ([`translation_code`]), whose `change`
//! the reader and the Library hear first (`page`), then the search box; the
//! controls the palette's commands click as a reader would, by id
//! (`#compare-btn`, `#reader-mode-btn`, `#reader-exit`,
//! `#random-chapter-btn`, `#sidebar-toggle`, `#export-notes-btn`,
//! `#export-data-btn`, `#import-data-btn`, `#welcome-btn`, `#about-btn`
//! then `#update-check-btn`, `#library-toggle` then a `#library-tab-*`),
//! or a `[data-value]` choice in `#layout-choices`, `#width-choices` or
//! `#theme-choices`.
//!
//! The pieces' overlays (`#export-notes-overlay`, `#shortcuts-overlay`,
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
//!
//! Shared with the backend, not the page: the commands the pieces invoke,
//! through the same `window.__TAURI__.core.invoke` the page's scripts use
//! (see `ipc.rs`; their shapes are in `dto.rs`).

use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use web_sys::{CustomEvent, CustomEventInit, Document, HtmlElement};

use crate::palette::PaletteState;

/// Export notes… asks its dialog to open (`page`): a cancelable
/// `CustomEvent` whose `detail` is an [`OpenRequest`].
pub const OPEN_EVENT: &str = "gospel-getter:export-notes";

/// What the dialog needs, taken when the button is clicked: how many notes there are (none: nothing to export), the book
/// being read (ticked first, if it has notes), and the translation whose
/// text the file quotes (the one chosen with the translation button).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRequest {
    pub note_count: u64,
    pub book_id: Option<i64>,
    pub translation_code: String,
    pub translation_name: String,
}

/// The page's document, when there is one (there always is in the webview;
/// this crate never assumes it).
pub fn document() -> Option<Document> {
    web_sys::window().and_then(|w| w.document())
}

/// Says `message` in the page's toast (`#data-status`, `page`); an error
/// stays longer and is red.
pub fn toast(message: &str, is_error: bool) {
    crate::page::set_data_status(message, is_error);
}

/// [`toast`] with a button after the message, `label`, for `duration` ms
/// (`page::set_data_status_with_action`): the button, for the caller to
/// answer.
pub fn toast_with_action(message: &str, label: &str, duration: i32) -> Option<HtmlElement> {
    crate::page::set_data_status_with_action(message, label, duration)
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

/// The shortcuts sheet is asked to open (`?`, ☰ → Keyboard shortcuts, the
/// palette's command; `page`): a cancelable `CustomEvent` whose `detail` is
/// `{ returnFocus }` (see [`return_focus`]).
pub const SHORTCUTS_EVENT: &str = "gospel-getter:shortcuts";

/// Puts the table of shortcuts where the page had it, for the frontend
/// tests and the GUI checks: `window.gospelGetterShortcuts`, an array of
/// `{ id, group, keys, aria, does, handles }`, as `SHORTCUTS` was.
pub fn export_shortcuts() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let serializer = serde_wasm_bindgen::Serializer::json_compatible();
    if let Ok(table) = crate::shortcut_table::SHORTCUTS.serialize(&serializer) {
        let _ = js_sys::Reflect::set(&window, &JsValue::from_str("gospelGetterShortcuts"), &table);
    }
}

/// Reads an event's `detail` as `T`, or says what was wrong with it.
pub fn detail<T: serde::de::DeserializeOwned>(event: &CustomEvent) -> Result<T, JsValue> {
    serde_wasm_bindgen::from_value(event.detail()).map_err(|e| JsValue::from(e.to_string()))
}

/// `detail.returnFocus`: the element focus goes back to when a modal the
/// page opened closes (what had focus, or the button it was opened from),
/// or none. An element, so it travels as itself, not in the snapshot.
pub fn return_focus(event: &CustomEvent) -> Option<HtmlElement> {
    js_sys::Reflect::get(&event.detail(), &JsValue::from_str("returnFocus"))
        .ok()?
        .dyn_into::<HtmlElement>()
        .ok()
}

/// Sends `name` as a `CustomEvent` on `document`, with `detail`.
pub fn dispatch(name: &str, detail: &impl Serialize) {
    let Some(document) = document() else {
        return;
    };
    let Ok(detail) = serde_wasm_bindgen::to_value(detail) else {
        return;
    };
    let init = CustomEventInit::new();
    init.set_detail(&detail);
    if let Ok(event) = CustomEvent::new_with_event_init_dict(name, &init) {
        let _ = document.dispatch_event(&event);
    }
}

/// Every piece has mounted and is listening: a `CustomEvent` on `document`,
/// no `detail`. What was asked before a piece mounted is asked again (boot's
/// Library lists, welcome, picker books and launch check; the reader's
/// credits and chapter; the card's Library lists).
pub const READY_EVENT: &str = "gospel-getter:pieces-ready";

/// Says [`READY_EVENT`].
pub fn ready() {
    if let (Some(document), Ok(event)) = (document(), CustomEvent::new(READY_EVENT)) {
        let _ = document.dispatch_event(&event);
    }
}

/// A modal of this crate is about to open: the header's popovers and panels
/// go (focus left alone) and, with `close_search`, the search box; at once,
/// before the modal takes focus (`page`).
pub fn modal_opening(close_search: bool) {
    crate::page::modal_opening(close_search);
}

/// The welcome is asked to open (`page`): a cancelable `CustomEvent` whose
/// `detail` is a [`WelcomeRequest`] plus `returnFocus`. At the end of boot
/// it is a question (`startup`, with the facts the fresh-install rule needs;
/// the welcome opens only for a fresh install); from ☰ → Show welcome it
/// opens. Either way the welcome cancels it to say it has it.
pub const WELCOME_EVENT: &str = "gospel-getter:welcome";

/// What the welcome is told.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WelcomeRequest {
    /// Asked at the end of boot: open only for a fresh install.
    pub startup: bool,
    /// What boot loaded, for the rule (with `startup`).
    pub install: Option<crate::install::Install>,
    /// The places to start, the backend's `startHere` list.
    pub start_here: Vec<StartRow>,
}

/// One place to start: a book (or a chapter) and a line about it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRow {
    pub book_id: i64,
    pub chapter: i64,
    pub title: String,
    pub line: String,
}

pub fn welcome_request(event: &CustomEvent) -> Result<WelcomeRequest, JsValue> {
    detail(event)
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
/// throw in the webview, so every use is guarded, as the page's are).
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

/// The import is asked to begin (☰ → Import data…, `page`): a cancelable
/// `CustomEvent`, no `detail`. The import cancels it to say it has it.
pub const IMPORT_EVENT: &str = "gospel-getter:import";

/// Import is under way, or done: ☰'s Export data… and Import data… are
/// disabled (or enabled) for it, as for an export (`page`).
pub fn data_busy(busy: bool) {
    crate::page::set_data_busy(busy);
}

/// An import has changed the reader's data: what's shown of it is reloaded,
/// in order: the chapter at `position` (or none, to stay put), then the
/// bookmarks, highlights and notes; then `message` is said above the
/// chapter and in the toast (`page`).
pub fn imported(position: Option<&crate::dto::PositionPreview>, message: &str) {
    crate::page::imported(position.cloned(), message.to_owned());
}

/// The card opens the note editor (its + Add note and Edit, N, the
/// Library's Edit): a cancelable `CustomEvent` whose `detail` is a
/// [`NoteRequest`] plus `note` (the note being edited, as the backend gave
/// it, or `null`; handed back with [`NOTE_DONE_EVENT`]) and `returnFocus`.
/// The page's popovers and the search box are put away first
/// ([`modal_opening`]).
pub const NOTE_EVENT: &str = "gospel-getter:note";

/// What the editor needs from the card to open.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteRequest {
    /// `add` or `edit`.
    pub mode: String,
    /// The note being edited.
    pub note: Option<NoteRef>,
    /// The verses the note is on.
    pub passage: Passage,
    /// The passage's reference, e.g. "John 3:16–18" (the title).
    pub reference: String,
    /// The translation whose text is quoted.
    pub code: String,
    /// The passage quoted, as markup: its verses as the reader draws them
    /// (`reader_text::verse_html`: KJV supplied words in italics, no note
    /// markers, escaped), with what the translation doesn't number said at
    /// the end (`card_text::note_passage_html`). Shown as it is.
    pub passage_html: String,
    /// When adding, the other notes on these verses, read-only.
    pub others: Vec<OtherNote>,
}

/// The note being edited: its id and text (the rest stays the card's).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct NoteRef {
    pub id: i64,
    pub body: String,
}

/// The verses a note is on.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Passage {
    pub book_id: i64,
    pub chapter: i64,
    pub start: i64,
    pub end: i64,
}

/// Another note on the passage, as the editor lists it: its reference,
/// the date it was written (as the backend writes dates) and its text.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OtherNote {
    pub reference: String,
    pub date: String,
    pub body: String,
}

pub fn note_request(event: &CustomEvent) -> Result<NoteRequest, JsValue> {
    detail(event)
}

/// The card's notes on the selection have changed while the editor may be
/// open (adding): a `CustomEvent` whose `detail` is `{ others }`, the
/// [`OtherNote`]s now. The editor shows them if it's adding.
pub const NOTE_OTHERS_EVENT: &str = "gospel-getter:note-others";

#[derive(Debug, Deserialize)]
struct Others {
    others: Vec<OtherNote>,
}

pub fn note_others(event: &CustomEvent) -> Result<Vec<OtherNote>, JsValue> {
    detail::<Others>(event).map(|o| o.others)
}

/// The editor has added, saved or deleted a note (and the card's notes are
/// refreshed): a `CustomEvent` whose `detail` is `{ action, note }`,
/// `action` being `added`, `saved` or `deleted`, and `note` the deleted
/// note as the backend returned it (else `null`). The card says so in its
/// status line, a deletion with its Undo, or in the page's toast when the
/// card is gone.
pub const NOTE_DONE_EVENT: &str = "gospel-getter:note-done";

/// Says [`NOTE_DONE_EVENT`].
pub fn note_done(action: &str, note: &JsValue) {
    let Some(document) = document() else {
        return;
    };
    let detail = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &detail,
        &JsValue::from_str("action"),
        &JsValue::from_str(action),
    );
    let _ = js_sys::Reflect::set(&detail, &JsValue::from_str("note"), note);
    let init = CustomEventInit::new();
    init.set_detail(&detail);
    if let Ok(event) = CustomEvent::new_with_event_init_dict(NOTE_DONE_EVENT, &init) {
        let _ = document.dispatch_event(&event);
    }
}

/// Boot is done: run the quiet launch check (unless it's turned off). A cancelable `CustomEvent` on `document`, no `detail`; the updater
/// cancels it to say it has it.
pub const LAUNCH_CHECK_EVENT: &str = "gospel-getter:update-check";

/// Whether the launch check is on: `html[data-update-check]` isn't `off`
/// (the pre-paint script applies the stored setting there before the page
/// draws, and `aa` keeps it).
pub fn launch_check_on() -> bool {
    document()
        .and_then(|d| d.document_element())
        .is_none_or(|html| html.get_attribute("data-update-check").as_deref() != Some("off"))
}

/// The reader has the bundled texts' credits for About: a `CustomEvent`
/// whose `detail` is `{ credits }`, each translation's credit in order.
pub const ABOUT_CREDITS_EVENT: &str = "gospel-getter:about-credits";

#[derive(Debug, Deserialize)]
struct Credits {
    credits: Vec<String>,
}

pub fn about_credits(event: &CustomEvent) -> Result<Vec<String>, JsValue> {
    detail::<Credits>(event).map(|c| c.credits)
}

/// A header surface closed (`surfaces`): `{ name }` ("about", "menu",
/// ...).
pub const SURFACE_CLOSE_EVENT: &str = "gospel-getter:surface-close";

#[derive(Debug, Deserialize)]
struct SurfaceClosed {
    name: String,
}

/// The surface `event` says closed, if it can be read.
pub fn surface_closed(event: &CustomEvent) -> Option<String> {
    detail::<SurfaceClosed>(event).ok().map(|s| s.name)
}

/// The Library is asked to reload lists: a cancelable `CustomEvent`
/// whose `detail` is `{ lists }`, each `bookmarks`, `highlights` or `notes`
/// (`library_text::List`). The Library cancels it to say it has it and sets
/// `detail.done` to a promise that settles once every list named has its
/// answer, shown (or dropped, when a newer reload of the same list has
/// already been asked for: only the latest of overlapping reloads shows).
/// Whoever asked waits for it where the page waited for its reloads.
pub const LIBRARY_REFRESH_EVENT: &str = "gospel-getter:library-refresh";

#[derive(Debug, Deserialize)]
struct LibraryRefresh {
    lists: Vec<String>,
}

/// The lists a [`LIBRARY_REFRESH_EVENT`] names, as it named them.
pub fn library_refresh(event: &CustomEvent) -> Result<Vec<String>, JsValue> {
    detail::<LibraryRefresh>(event).map(|r| r.lists)
}

/// Answers a [`LIBRARY_REFRESH_EVENT`]: `detail.done = done`.
pub fn library_refresh_done(event: &CustomEvent, done: &js_sys::Promise) {
    let _ = js_sys::Reflect::set(&event.detail(), &JsValue::from_str("done"), done);
}

/// The Library's bookmarks or notes have loaded: a `CustomEvent` whose
/// `detail` is `{ list, items }`, `list` being `bookmarks` or `notes` and
/// `items` the backend's answer as it came (`null` if it failed). Not sent
/// for an answer a newer reload has overtaken.
pub const LIBRARY_LOADED_EVENT: &str = "gospel-getter:library-loaded";

/// Says [`LIBRARY_LOADED_EVENT`].
pub fn library_loaded(list: &str, items: &JsValue) {
    let detail = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &detail,
        &JsValue::from_str("list"),
        &JsValue::from_str(list),
    );
    let _ = js_sys::Reflect::set(&detail, &JsValue::from_str("items"), items);
    dispatch_object(LIBRARY_LOADED_EVENT, &detail);
}

/// A passage to open: a chapter, and its verses if not all of it (a
/// Library row or a search result, opened through `reader::open`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenPassage {
    pub book_id: i64,
    pub chapter: i64,
    pub verse_start: Option<i64>,
    pub verse_end: Option<i64>,
}

/// Has the Library reload `lists` (`bookmarks`, `highlights`, `notes`): a
/// cancelable [`LIBRARY_REFRESH_EVENT`] sent now; the
/// promise the Library answers with, or `None` if nothing answered (the
/// Library isn't there yet).
pub fn refresh_library(lists: &[&str]) -> Option<js_sys::Promise> {
    #[derive(Serialize)]
    struct Refresh<'a> {
        lists: &'a [&'a str],
    }
    let document = document()?;
    let detail = serde_wasm_bindgen::to_value(&Refresh { lists }).ok()?;
    let init = CustomEventInit::new();
    init.set_cancelable(true);
    init.set_detail(&detail);
    let event = CustomEvent::new_with_event_init_dict(LIBRARY_REFRESH_EVENT, &init).ok()?;
    if document.dispatch_event(&event).unwrap_or(true) {
        return None;
    }
    js_sys::Reflect::get(&detail, &JsValue::from_str("done"))
        .ok()?
        .dyn_into::<js_sys::Promise>()
        .ok()
}

/// Sends `name` with `detail` as it is (an object built here).
pub fn dispatch_object(name: &str, detail: &JsValue) {
    let Some(document) = document() else {
        return;
    };
    let init = CustomEventInit::new();
    init.set_detail(detail);
    if let Ok(event) = CustomEvent::new_with_event_init_dict(name, &init) {
        let _ = document.dispatch_event(&event);
    }
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

/// The search box is asked to open (`/`, Ctrl+K) or close (Escape outside
/// it, a click away, a dialog opening; `page`): a cancelable `CustomEvent`
/// whose `detail` is `{ open }`. The search box cancels it to say it has
/// it; when nothing does (it hasn't mounted yet), opening says search
/// isn't available.
pub const SEARCH_EVENT: &str = "gospel-getter:search";

#[derive(Debug, Deserialize)]
struct SearchRequest {
    open: bool,
}

/// Whether a [`SEARCH_EVENT`] asks to open (else to close).
pub fn search_request(event: &CustomEvent) -> Result<bool, JsValue> {
    detail::<SearchRequest>(event).map(|r| r.open)
}

/// A header surface opened (`surfaces`): `{ name }`.
pub const SURFACE_OPEN_EVENT: &str = "gospel-getter:surface-open";

/// What the palette's commands need now: the page's settings and controls
/// (`page`), the reader's chapters and history, and whether the card has the
/// chapter's text ready to copy.
pub fn palette_state() -> PaletteState {
    let mut state = crate::page::palette_state();
    crate::reader::palette_facts(&mut state);
    crate::card::palette_facts(&mut state);
    state
}

/// Runs a palette command whose code was the page's (`palette::Run::Page`):
/// `copy-chapter`, `copy-chapter-as`, `next-chapter`, `previous-chapter`,
/// `next-book`, `previous-book`, `back`, `forward` and `shortcuts`, within
/// the click or key that chose it (the clipboard needs that), after the box
/// has closed and focus has gone back (`page`).
pub fn page_command(command: &str) {
    crate::page::run_command(command);
}

/// Boot's books for the picker: a `CustomEvent` whose `detail` is
/// `{ otBooks, ntBooks }`, each a list of `{ id, name, chapterCount }`
/// (`get_home`'s, as it gave them).
pub const PICKER_BOOKS_EVENT: &str = "gospel-getter:picker-books";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PickerBooks {
    ot_books: Vec<crate::picker_text::Book>,
    nt_books: Vec<crate::picker_text::Book>,
}

/// The Old and the New Testament's books a [`PICKER_BOOKS_EVENT`] carries.
pub fn picker_books(
    event: &CustomEvent,
) -> Result<(Vec<crate::picker_text::Book>, Vec<crate::picker_text::Book>), JsValue> {
    detail::<PickerBooks>(event).map(|b| (b.ot_books, b.nt_books))
}

/// The reader has put a chapter on screen: a `CustomEvent` whose `detail`
/// is `{ bookId, chapter }`.
pub const PICKER_CURRENT_EVENT: &str = "gospel-getter:picker-current";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PickerCurrent {
    book_id: i64,
    chapter: i64,
}

/// Says [`PICKER_CURRENT_EVENT`] (the reader, of the chapter it has put on
/// screen).
pub fn show_picker_current(book_id: i64, chapter: i64) {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Current {
        book_id: i64,
        chapter: i64,
    }
    dispatch(PICKER_CURRENT_EVENT, &Current { book_id, chapter });
}

/// The book id and chapter a [`PICKER_CURRENT_EVENT`] carries.
pub fn picker_current(event: &CustomEvent) -> Result<(i64, i64), JsValue> {
    detail::<PickerCurrent>(event).map(|c| (c.book_id, c.chapter))
}

/// `surfaces` has shown the picker (it was hidden): a `CustomEvent`,
/// no `detail`.
pub const PICKER_SHOWN_EVENT: &str = "gospel-getter:picker-shown";

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
pub fn request(name: &str, detail: &JsValue) -> bool {
    let Some(document) = document() else {
        return false;
    };
    let init = CustomEventInit::new();
    init.set_cancelable(true);
    init.set_detail(detail);
    CustomEvent::new_with_event_init_dict(name, &init)
        .is_ok_and(|event| !document.dispatch_event(&event).unwrap_or(true))
}
