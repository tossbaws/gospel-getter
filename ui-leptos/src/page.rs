//! The page's glue, which its main script was: the toast at the foot of
//! the window; ☰'s Export data… (its own command) and Import data…, and an
//! import's reloads; Export notes…'s button; the welcome (from ☰, and the
//! startup question boot asks); the keyboard shortcuts sheet; opening and
//! closing the search box; what the palette's commands need, and the
//! commands whose code was the page's; the translation changing; the
//! Library's lists reloaded, and its notes kept for the welcome and Export
//! notes; Compare's partner; and the launch check for updates. Boot is
//! `boot`, the keys `keyboard` and the clicks `clicks`.
//!
//! This is the main script of `ui/index.html` at 2a9d85c (`errorText` to
//! `aboutOpen`, less what `boot`, `keyboard` and `clicks` took) in Rust: the
//! same commands with the same arguments, in the same order, the same words,
//! ids, classes and focus. Its listeners are the first this module adds
//! ([`mount`], before any piece mounts), where the main script's were, so
//! they run first, as theirs did.
//!
//! The main script's "… isn't available." for the header's buttons and keys
//! (when `window.gospelGetterSurfaces` wasn't there) has no place here: the
//! surfaces (`surfaces`) mount in the same call as this, before any click or
//! key can come. What can come before the rest of the pieces have mounted
//! (`lib::mount_the_rest`) still says so: the sheet, the welcome, the
//! import, Export notes, search and About.

use std::cell::RefCell;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{Element, HtmlButtonElement, HtmlElement, HtmlOptionElement, HtmlSelectElement};

use crate::bridge;
use crate::card::{self, Ask};
use crate::card_text::Slot;
use crate::dom;
use crate::dto::PositionPreview;
use crate::ipc;
use crate::keyboard;
use crate::page_text::{self as text, Saved};
use crate::palette::{Choice, PaletteState};
use crate::reader::{self, Request};
use crate::reader_text::Which;
use crate::surfaces;

#[derive(Default)]
struct State {
    /// The toast's fading, if it's showing.
    toast_timer: Option<i32>,
    /// How many notes the Library loaded (`list_all_notes`), once it has.
    notes: Option<u32>,
    /// The backend's places to start, from boot, for the welcome.
    start_here: Option<JsValue>,
    /// Boot's books for the picker, sent again once every piece is there.
    picker_books: Option<JsValue>,
    /// Library lists asked for before the Library was there, in order.
    library_waiting: Vec<String>,
    /// Those lists, asked again: settles once they have their answers.
    library_resent: Option<js_sys::Promise>,
    /// Boot's startup question for the welcome (`get_home`'s answer), if it
    /// came before the welcome was there.
    welcome_waiting: Option<JsValue>,
    /// The launch check, asked before the updater was there.
    update_check_waiting: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// The glue's state, never held while anything else runs.
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
}

fn by_id(id: &str) -> Option<HtmlElement> {
    bridge::document()?
        .get_element_by_id(id)?
        .dyn_into::<HtmlElement>()
        .ok()
}

fn button(id: &str) -> Option<HtmlButtonElement> {
    by_id(id)?.dyn_into::<HtmlButtonElement>().ok()
}

fn select(id: &str) -> Option<HtmlSelectElement> {
    by_id(id)?.dyn_into::<HtmlSelectElement>().ok()
}

fn html_data(name: &str) -> Option<String> {
    bridge::document()?.document_element()?.get_attribute(name)
}

/// Whether an overlay of the pieces' is open (looked up by id when asked:
/// it's made when its piece mounts).
fn overlay_open(id: &str) -> bool {
    by_id(id).is_some_and(|overlay| !overlay.hidden())
}

/// The import preview is open.
pub fn import_open() -> bool {
    overlay_open("import-overlay")
}

/// About is open.
pub fn about_open() -> bool {
    overlay_open("about-overlay")
}

/// The note editor is open.
pub fn note_open() -> bool {
    overlay_open("note-overlay")
}

/// Export notes' dialog is open.
pub fn export_notes_open() -> bool {
    overlay_open("export-notes-overlay")
}

/// The keyboard shortcuts sheet is open.
pub fn shortcuts_open() -> bool {
    overlay_open("shortcuts-overlay")
}

/// The search box is open (`#search-panel` shown).
pub fn search_shown() -> bool {
    by_id("search-panel").is_some_and(|panel| !panel.hidden())
}

/// Comparing two translations (`html[data-compare]`).
fn is_comparing() -> bool {
    html_data("data-compare").as_deref() == Some("on")
}

/// The element that has focus, unless that's the body (or nothing): where
/// focus goes back to from a sheet or editor opened by a key.
pub fn focused() -> Option<HtmlElement> {
    let document = bridge::document()?;
    let active = document.active_element()?;
    let body = document.body();
    if body.is_some_and(|body| dom::same(Some(&active), &body)) {
        return None;
    }
    active.dyn_into::<HtmlElement>().ok()
}

/// A cancelable request with a `detail` object built from `fields`.
fn request(name: &str, fields: &[(&str, &JsValue)]) -> bool {
    let detail = js_sys::Object::new();
    for (key, value) in fields {
        let _ = js_sys::Reflect::set(&detail, &JsValue::from_str(key), value);
    }
    bridge::request(name, &detail)
}

fn element_or_null(el: Option<HtmlElement>) -> JsValue {
    el.map_or(JsValue::NULL, JsValue::from)
}

// ---- The toast: export and import say how it went at the foot of the
// window, and fade after a while (errors stay longer). It sits above the
// update banner when that's showing.

/// Says `message` in the toast (none: hides it); an error is red and stays
/// longer.
pub fn set_data_status(message: &str, is_error: bool) {
    if let (Some(timer), Some(window)) = (with(|s| s.toast_timer.take()), web_sys::window()) {
        window.clear_timeout_with_handle(timer);
    }
    let Some(status) = by_id("data-status") else {
        return;
    };
    status.set_text_content(Some(message));
    let _ = status.class_list().toggle_with_force("is-error", is_error);
    if message.is_empty() {
        let _ = status.class_list().add_1("is-hidden");
        return;
    }
    // Above the update banner, if it's up: the same assignment the page
    // made, `style.bottom`, through the CSSOM.
    let bottom = match by_id("update-banner").filter(|banner| !banner.hidden()) {
        Some(banner) => format!("calc({}px + 1.5rem)", banner.offset_height()),
        None => String::new(),
    };
    let _ = js_sys::Reflect::set(
        &status.style(),
        &JsValue::from_str("bottom"),
        &JsValue::from_str(&bottom),
    );
    let _ = status.class_list().remove_1("is-hidden");
    let fade = Closure::once_into_js(move || {
        if let Some(status) = by_id("data-status") {
            let _ = status.class_list().add_1("is-hidden");
        }
    });
    let timer = web_sys::window().and_then(|w| {
        w.set_timeout_with_callback_and_timeout_and_arguments_0(
            fade.unchecked_ref(),
            if is_error { 12000 } else { 6000 },
        )
        .ok()
    });
    with(|s| s.toast_timer = timer);
}

/// ☰'s Export data… and Import data…, disabled while either runs.
pub fn set_data_busy(busy: bool) {
    for id in ["export-data-btn", "import-data-btn"] {
        if let Some(item) = button(id) {
            item.set_disabled(busy);
        }
    }
}

// ---- Back up or move your data. The native file dialogs, file access and
// validation are all in the backend; this only shows what's happening.

/// `export_reader_data`'s answer.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Exported {
    status: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    bookmarks: u64,
    #[serde(default)]
    highlights: u64,
    #[serde(default)]
    notes: u64,
    #[serde(default)]
    has_position: bool,
    #[serde(default)]
    preferences: u64,
}

/// ☰ → Export data…: bookmarks, highlights, notes, the reading position and
/// the display settings, to a file the reader chooses.
fn export_data() {
    let args = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &args,
        &JsValue::from_str("preferences"),
        &crate::aa::current(),
    );
    set_data_busy(true);
    set_data_status(text::EXPORT_CHOOSING, false);
    let sent = ipc::send("export_reader_data", args.into());
    spawn_local(async move {
        let answer = match sent {
            Ok(promise) => JsFuture::from(promise).await,
            Err(e) => Err(e),
        };
        let outcome = answer.and_then(|value| {
            serde_wasm_bindgen::from_value::<Exported>(value)
                .map_err(|e| JsValue::from(js_sys::Error::new(&e.to_string())))
        });
        match outcome {
            Ok(outcome) if outcome.status == "cancelled" => {
                set_data_status(text::EXPORT_CANCELLED, false);
            }
            Ok(outcome) => set_data_status(
                &text::export_saved(&Saved {
                    path: &outcome.path,
                    bookmarks: outcome.bookmarks,
                    highlights: outcome.highlights,
                    notes: outcome.notes,
                    has_position: outcome.has_position,
                    preferences: outcome.preferences,
                }),
                false,
            ),
            Err(e) => {
                ipc::log_error("Export failed", &e);
                set_data_status(&format!("Export failed: {}", ipc::error_text(&e)), true);
            }
        }
        set_data_busy(false);
    });
}

/// Export notes…, at the top of the Library's Notes tab (the Library's
/// button, enabled while there are notes; the palette clicks it too): asks
/// the dialog to open with what it needs.
fn on_library_click(event: &web_sys::Event) {
    let Some(target) = event.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
        return;
    };
    let Some(export) = target
        .closest("#export-notes-btn")
        .ok()
        .flatten()
        .and_then(|el| el.dyn_into::<HtmlButtonElement>().ok())
    else {
        return;
    };
    let notes = with(|s| s.notes).unwrap_or(0);
    if export.disabled() || notes == 0 {
        return;
    }
    let code = bridge::translation_code();
    let book_id = card::facts().chapter.map(|c| c.book_id);
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Open<'a> {
        note_count: u32,
        book_id: Option<i64>,
        translation_code: &'a str,
        translation_name: &'a str,
    }
    let Ok(detail) = ipc::args(&Open {
        note_count: notes,
        book_id,
        translation_code: &code,
        translation_name: &reader::translation_name(&code),
    }) else {
        return;
    };
    if !bridge::request(bridge::OPEN_EVENT, &detail) {
        set_data_status(text::EXPORT_NOTES_UNAVAILABLE, true);
    }
}

/// A modal of the pieces' is about to open: popovers and panels go (focus
/// left alone, the modal takes it), and the search box with `close_search`.
pub fn modal_opening(close_search: bool) {
    surfaces::close(false);
    if close_search && search_shown() {
        set_search_open(false);
    }
}

/// ☰ → Import data…: the import preview runs the open dialog, the preview
/// and the import, and says how it's going.
fn import_data() {
    if !bridge::request(bridge::IMPORT_EVENT, &JsValue::NULL) {
        set_data_status(text::IMPORT_UNAVAILABLE, true);
    }
}

/// After an import: the file's reading position (if chosen), then the
/// bookmarks, highlights and notes, then the result.
pub fn imported(position: Option<PositionPreview>, message: String) {
    // What comes first is asked within the event, as the page's handler
    // asked it: the position's chapter, or else the bookmarks.
    let first = match position {
        Some(position) => Err(reader::ask(Request::Load {
            book_id: position.book_id,
            chapter: position.chapter,
            translation_code: Some(position.translation_code),
            top: false,
        })),
        None => Ok(refresh_library(&["bookmarks"])),
    };
    spawn_local(async move {
        let bookmarks = match first {
            Ok(bookmarks) => bookmarks,
            Err(load) => {
                load.await;
                refresh_library(&["bookmarks"])
            }
        };
        if JsFuture::from(bookmarks).await.is_err() {
            return;
        }
        card::refresh_highlights().await;
        if JsFuture::from(refresh_library(&["highlights"]))
            .await
            .is_err()
        {
            return;
        }
        card::refresh_notes().await;
        // Above the chapter (the reader's notice, shown within the ask).
        let notice = reader::ask(Request::Notice {
            message: message.clone(),
        });
        spawn_local(async move {
            notice.await;
        });
        set_data_status(&message, false);
    });
}

// ---- The Library's lists, reloaded whenever what they show changes (one
// request naming the lists), waited for where the page waited. A request
// made before the Library has mounted is made again once every piece has.

/// Has the Library reload `lists`: a promise that settles once each has its
/// answer (at once, if the Library isn't there yet).
pub fn refresh_library(lists: &[&str]) -> js_sys::Promise {
    if let Some(done) = bridge::refresh_library(lists) {
        return js_sys::Promise::resolve(&done);
    }
    with(|s| {
        for list in lists {
            if !s.library_waiting.iter().any(|l| l == list) {
                s.library_waiting.push((*list).to_owned());
            }
        }
    });
    js_sys::Promise::resolve(&JsValue::UNDEFINED)
}

/// Boot's lists asked for before the Library was there, asked again: the
/// promise to wait for (settled, if none were).
pub fn library_resent() -> js_sys::Promise {
    with(|s| s.library_resent.clone())
        .unwrap_or_else(|| js_sys::Promise::resolve(&JsValue::UNDEFINED))
}

/// Every note, as the Library loaded them: kept as their count, for the
/// welcome and Export notes.
fn on_library_loaded(event: &web_sys::CustomEvent) {
    let detail = event.detail();
    let get = |key: &str| js_sys::Reflect::get(&detail, &JsValue::from_str(key)).ok();
    let list = get("list").and_then(|l| l.as_string());
    let items = get("items").filter(JsValue::is_truthy);
    if let (Some("notes"), Some(items)) = (list.as_deref(), items) {
        let count = js_sys::Reflect::get(&items, &JsValue::from_str("length"))
            .ok()
            .and_then(|n| n.as_f64())
            .map_or(0, |n| n as u32);
        with(|s| s.notes = Some(count));
    }
}

// ---- The welcome: it opens by itself only on a fresh install, which it
// decides from what boot loaded (asked at the end of boot), and whenever
// Show welcome is chosen in the ☰ menu. Its places to start are the
// backend's `startHere`, kept from boot.

/// Keeps boot's places to start (the backend's `startHere`, an array, or
/// none).
pub fn set_start_here(start_here: JsValue) {
    let start_here = if js_sys::Array::is_array(&start_here) {
        start_here
    } else {
        js_sys::Array::new().into()
    };
    with(|s| s.start_here = Some(start_here));
}

fn start_here() -> JsValue {
    with(|s| s.start_here.clone()).unwrap_or_else(|| js_sys::Array::new().into())
}

fn ask_welcome(fields: &[(&str, &JsValue)]) -> bool {
    request(bridge::WELCOME_EVENT, fields)
}

/// ☰ → Show welcome; focus goes back to `return_focus`.
fn open_welcome(return_focus: Option<HtmlElement>) {
    let asked = ask_welcome(&[
        ("startup", &JsValue::FALSE),
        ("startHere", &start_here()),
        ("returnFocus", &element_or_null(return_focus)),
    ]);
    if !asked {
        set_data_status(text::WELCOME_UNAVAILABLE, true);
    }
}

/// What the welcome's fresh-install rule needs, from boot (`home`,
/// `get_home`'s answer, and the bookmarks and highlights the card has): the
/// welcome opens only for a fresh install. Asked again once every piece is
/// there, if the welcome wasn't.
pub fn ask_welcome_at_startup(home: &JsValue) {
    let facts = card::facts();
    let has_saved_position = js_sys::Reflect::get(home, &JsValue::from_str("hasSavedPosition"))
        .is_ok_and(|v| v.is_truthy());
    let notes = with(|s| s.notes);
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Install {
        has_saved_position: bool,
        bookmarks_loaded: bool,
        bookmarks: usize,
        highlights_loaded: bool,
        highlights: usize,
        notes_loaded: bool,
        notes: u32,
    }
    let Ok(install) = ipc::args(&Install {
        has_saved_position,
        bookmarks_loaded: facts.bookmarks_loaded,
        bookmarks: facts.bookmarks,
        highlights_loaded: facts.highlights_loaded,
        highlights: facts.highlights,
        notes_loaded: notes.is_some(),
        notes: notes.unwrap_or(0),
    }) else {
        return;
    };
    let asked = ask_welcome(&[
        ("startup", &JsValue::TRUE),
        ("install", &install),
        ("startHere", &start_here()),
        ("returnFocus", &JsValue::NULL),
    ]);
    if !asked {
        with(|s| s.welcome_waiting = Some(home.clone()));
    }
}

// ---- The book and chapter picker's books: boot's, which may come before
// the picker has mounted, so they go again once every piece is there.

/// Boot's books for the picker (`{ otBooks, ntBooks }`), sent now and kept.
pub fn set_picker_books(books: JsValue) {
    with(|s| s.picker_books = Some(books));
    send_picker_books();
}

fn send_picker_books() {
    if let Some(books) = with(|s| s.picker_books.clone()) {
        bridge::dispatch_object(bridge::PICKER_BOOKS_EVENT, &books);
    }
}

// ---- The keyboard shortcuts sheet: a request to open carries where focus
// goes back to (what had it, or the button it was opened from).

/// Opens the sheet (`?`, ☰ → Keyboard shortcuts, the palette's command),
/// unless it's open; focus goes back to `opener`, or to what had it.
pub fn open_shortcuts(opener: Option<HtmlElement>) {
    if shortcuts_open() {
        return;
    }
    let return_focus = opener.or_else(focused);
    if search_shown() {
        set_search_open(false);
    }
    surfaces::close(false);
    if !request(
        bridge::SHORTCUTS_EVENT,
        &[("returnFocus", &element_or_null(return_focus))],
    ) {
        set_data_status(text::SHORTCUTS_UNAVAILABLE, true);
    }
}

// ---- Search, go to a reference, or run a command: the box is the search
// piece's, which opens and closes it (from its button, and when a surface
// opens); this asks it to (its keys, a click away, a dialog opening).

/// Opens or closes the search box.
pub fn set_search_open(open: bool) {
    if request(bridge::SEARCH_EVENT, &[("open", &JsValue::from_bool(open))]) || !open {
        return;
    }
    set_data_status(text::SEARCH_UNAVAILABLE, true);
}

// ---- The palette: what its commands need of the page, and the commands
// whose code was the page's.

fn choices(id: &str) -> Vec<Choice> {
    select(id)
        .map(|select| {
            let options = select.options();
            (0..options.length())
                .filter_map(|i| options.item(i))
                .filter_map(|o| o.dyn_into::<HtmlOptionElement>().ok())
                .map(|o| Choice {
                    value: o.value(),
                    label: o.text_content().unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn value_of(id: &str) -> String {
    select(id).map(|s| s.value()).unwrap_or_default()
}

fn disabled(id: &str) -> bool {
    button(id).is_some_and(|b| b.disabled())
}

/// Aa's settings, comparing, the shape, the translations, a busy export:
/// what the palette's commands need of the page now (the reader's and the
/// card's facts are added by `bridge::palette_state`).
pub fn palette_state() -> PaletteState {
    PaletteState {
        comparing: is_comparing(),
        partner: crate::aa::partner(),
        reader_mode: html_data("data-reader-mode").as_deref() == Some("on"),
        sidebar: html_data("data-picker").as_deref() == Some("sidebar"),
        wide: html_data("data-wide").is_some(),
        layouts: choices("layout-select"),
        layout: value_of("layout-select"),
        widths: choices("width-select"),
        width: value_of("width-select"),
        text_sizes: choices("text-size-select"),
        text_size: value_of("text-size-select"),
        themes: choices("theme-select"),
        theme: value_of("theme-select"),
        translations: choices("translation-select"),
        translation: bridge::translation_code(),
        notes: button("export-notes-btn").is_some_and(|b| !b.disabled()),
        export_busy: disabled("export-data-btn"),
        import_busy: disabled("import-data-btn"),
        checking: disabled("update-check-btn"),
        chapter_shown: dom::query(".chapter-copy-btn").is_some(),
        ..PaletteState::default()
    }
}

/// Runs the palette's `command` whose code was the page's, within the
/// click or key that chose it (copying needs that).
pub fn run_command(command: &str) {
    match command {
        "copy-chapter" => {
            card::ask(Ask::CopyChapter);
        }
        "copy-chapter-as" => {
            card::ask(Ask::CopyAsOpen(Slot::Chapter));
        }
        "next-chapter" => keyboard::turn_chapter(Which::Next),
        "previous-chapter" => keyboard::turn_chapter(Which::Prev),
        "next-book" => keyboard::step_book(Which::Next),
        "previous-book" => keyboard::step_book(Which::Prev),
        "back" => keyboard::go_history(-1),
        "forward" => keyboard::go_history(1),
        "shortcuts" => open_shortcuts(None),
        _ => {}
    }
}

// ---- The launch check for updates: asked when boot is done, which may
// come before the updater has mounted, so it's asked again once every
// piece is there.

/// Asks the updater for its quiet launch check.
pub fn ask_update_check() {
    let asked = bridge::request(bridge::LAUNCH_CHECK_EVENT, &JsValue::NULL);
    with(|s| s.update_check_waiting = !asked);
}

/// Every piece has mounted: boot's Library lists, its welcome question,
/// the picker's books and the launch check, asked again if they came first.
fn on_ready() {
    let lists = with(|s| std::mem::take(&mut s.library_waiting));
    if !lists.is_empty() {
        let lists: Vec<&str> = lists.iter().map(String::as_str).collect();
        let resent = refresh_library(&lists);
        with(|s| s.library_resent = Some(resent));
    }
    if let Some(home) = with(|s| s.welcome_waiting.take()) {
        let resent = library_resent();
        spawn_local(async move {
            if JsFuture::from(resent).await.is_ok() {
                ask_welcome_at_startup(&home);
            }
        });
    }
    send_picker_books();
    if with(|s| s.update_check_waiting) {
        ask_update_check();
    }
}

/// Listens where the main script listened: the ☰ items, the Library's
/// Export notes…, the search and About buttons, the translation changing,
/// the Library's notes and every piece being there; then the click
/// dispatcher (`clicks`) and the keyboard dispatcher (`keyboard`). Called
/// first of all, before any piece mounts, so these run first, as the main
/// script's did.
pub fn mount() {
    let on_click = |id: &str, handler: fn()| {
        if let Some(el) = by_id(id) {
            dom::on_click(&el, move |_| handler());
        }
    };
    on_click("export-data-btn", export_data);
    if let Some(panel) = by_id("library-panel") {
        dom::on(&panel, "click", |event| on_library_click(&event));
    }
    on_click("import-data-btn", import_data);
    dom::on_document_event(bridge::LIBRARY_LOADED_EVENT, |event| {
        on_library_loaded(&event);
    });
    on_click("welcome-btn", || open_welcome(by_id("menu-toggle")));
    on_click("shortcuts-btn", || open_shortcuts(by_id("menu-toggle")));
    on_click("search-toggle", || {
        if by_id("search-input").is_none() {
            set_data_status(text::SEARCH_UNAVAILABLE, true);
        }
    });
    crate::clicks::mount();
    keyboard::mount();
    // Switching translations: the chapter on screen again, in it (the
    // reader's), and the Library's previews; the search box hears it after
    // this and searches again.
    if let Some(translations) = by_id("translation-select") {
        dom::on(&translations, "change", |_| {
            let load = reader::ask(Request::Translation);
            spawn_local(async move {
                load.await;
            });
            let _ = refresh_library(&["bookmarks", "highlights"]);
        });
    }
    dom::on_document_event(bridge::READY_EVENT, |_| on_ready());
    // Without the pieces there is no About to open.
    on_click("about-btn", || {
        if by_id("about-overlay").is_none() {
            set_data_status(text::ABOUT_UNAVAILABLE, true);
        }
    });
}
