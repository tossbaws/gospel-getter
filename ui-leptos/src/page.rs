//! The page's glue: the toast at the foot of
//! the window; ☰'s Export data… (its chooser, `export_choice`, and the
//! backup, its own command) and Import data…, and an import's reloads;
//! the Export notes dialog asked for (the chooser's My notes, the
//! palette's Export notes…); the welcome (from ☰, and the startup question
//! boot asks); the keyboard shortcuts sheet; opening and closing the search
//! box; what the palette's commands need, and the commands it runs here;
//! the translation changing; the Library's lists reloaded, and
//! its notes kept for the welcome, the chooser and Export notes; Compare's
//! partner; and the launch check for updates. Boot is `boot`, the keys
//! `keyboard` and the clicks `clicks`.
//!
//! Its listeners are the first this crate adds ([`mount`], before any piece
//! mounts), so they run first.
//!
//! The header's buttons and keys never say "… isn't available.": the
//! surfaces (`surfaces`) mount in the same call as this, before any click
//! or key can come. What can come before the rest of the pieces have
//! mounted (`lib::mount_the_rest`) still says so: the sheet, the welcome,
//! the import, the export chooser, Export notes, search and About. Each of
//! those pieces keeps a handle to itself from its mount, `None` until then,
//! and its call here answers whether it was there; a piece that wasn't is
//! asked again from `lib`'s ready list where boot asked it (the Library's
//! lists, the welcome's startup question, the picker's books, the launch
//! check), and said to be unavailable where the reader asked.

use std::cell::RefCell;

use serde::Deserialize;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{HtmlButtonElement, HtmlElement, HtmlOptionElement, HtmlSelectElement};

use crate::bridge;
use crate::card::{self, Ask};
use crate::card_text::Slot;
use crate::dom::{self, by_id};
use crate::dto::PositionPreview;
use crate::export_choice::{self, ExportFacts};
use crate::export_notes::{self, OpenRequest};
use crate::intent::{Claim, Generation};
use crate::ipc;
use crate::keyboard;
use crate::library_text::List;
use crate::page_text::{self as text, Saved};
use crate::palette::{Choice, PaletteState};
use crate::pane::{self, PaneId};
use crate::picker_text::Book;
use crate::reader::{self, Request};
use crate::reader_text::Which;
use crate::surfaces;
use crate::welcome::{self, StartRow, WelcomeRequest};

#[derive(Default)]
struct State {
    /// The toast's fading, if it's showing.
    toast_timer: Option<i32>,
    /// How many notes the Library loaded (`list_all_notes`), once it has.
    notes: Option<u32>,
    /// An export or import is running (Export data…'s backup, Import
    /// data…): neither starts again until it's done.
    data_busy: bool,
    /// The backend's places to start, from boot, for the welcome.
    start_here: Option<JsValue>,
    /// Boot's books for the picker (the Old Testament's, the New's), sent
    /// again once every piece is there.
    picker_books: Option<(Vec<Book>, Vec<Book>)>,
    /// Library lists asked for before the Library was there, in order.
    library_waiting: Vec<String>,
    /// Those lists, asked again: settles once they have their answers.
    library_resent: Option<js_sys::Promise>,
    /// Boot's startup question for the welcome (`get_home`'s answer), if it
    /// came before the welcome was there.
    welcome_waiting: Option<JsValue>,
    /// The launch check, asked before the updater was there.
    update_check_waiting: bool,
    /// The modals' intents: a modal that gets ready to open before it does
    /// (Export notes, fetching its choices) claims it, and any modal that
    /// opens meanwhile moves it on ([`modal_opening`]).
    modals: Generation,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
    /// What the toast's timer calls ([`fade_toast`]): one callback, made
    /// once (`dom::kept_callback`).
    static FADE_TOAST: js_sys::Function = dom::kept_callback(fade_toast);
}

/// The glue's state, never held while anything else runs.
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
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

/// ☰ → Export data…'s chooser is open.
pub fn export_choice_open() -> bool {
    overlay_open("export-choice-overlay")
}

/// Export notes' dialog is open.
pub fn export_notes_open() -> bool {
    overlay_open("export-notes-overlay")
}

/// The keyboard shortcuts sheet is open.
pub fn shortcuts_open() -> bool {
    overlay_open("shortcuts-overlay")
}

/// A dialog is open: the import preview, About, the note editor, Export
/// data…'s chooser, Export notes or the shortcuts sheet. The reader's own
/// keys (`keyboard`) and a mouse's back and forward buttons (`reader`)
/// wait meanwhile, so nothing moves behind it (C2-01).
pub fn modal_open() -> bool {
    import_open()
        || about_open()
        || note_open()
        || export_choice_open()
        || export_notes_open()
        || shortcuts_open()
}

/// The search box is open (`#search-panel` shown).
pub fn search_shown() -> bool {
    by_id("search-panel").is_some_and(|panel| !panel.hidden())
}

/// Compare is on (`html[data-compare]`), with a translation or the notes.
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

// ---- The toast: export and import say how it went at the foot of the
// window, and fade after a while (errors stay longer). It sits above the
// update banner when that's showing.

/// Says `message` in the toast (none: hides it); an error is red and stays
/// longer.
pub fn set_data_status(message: &str, is_error: bool) {
    show_data_status(message, is_error, if is_error { 12000 } else { 6000 });
}

/// Says `message` in the toast with a button after it, `label` (a deleted
/// note's Undo), for `duration` ms: the button, for its caller to answer.
/// It's in the toast's live region, so it's heard as the toast's other
/// messages are, and a tab stop while it shows; when the toast fades or
/// says something else, the button goes, and focus in it goes home.
pub fn set_data_status_with_action(
    message: &str,
    label: &str,
    duration: i32,
) -> Option<HtmlElement> {
    let status = show_data_status(message, false, duration)?;
    let button = bridge::document()?
        .create_element("button")
        .ok()?
        .dyn_into::<HtmlElement>()
        .ok()?;
    let _ = button.set_attribute("type", "button");
    button.set_class_name("note-link toast-action");
    button.set_text_content(Some(label));
    let _ = status.append_with_str_1(" ");
    let _ = status.append_with_node_1(&button);
    Some(button)
}

/// [`set_data_status`], fading after `duration` ms: the toast, unless it's
/// hidden.
fn show_data_status(message: &str, is_error: bool, duration: i32) -> Option<HtmlElement> {
    dom::clear_timeout(with(|s| s.toast_timer.take()));
    let status = by_id("data-status")?;
    // An action's button, about to go with the message it ended.
    let had_focus = dom::active_element().is_some_and(|active| status.contains(Some(&active)));
    status.set_text_content(Some(message));
    if had_focus {
        bridge::focus_home();
    }
    let _ = status.class_list().toggle_with_force("is-error", is_error);
    if message.is_empty() {
        let _ = status.class_list().add_1("is-hidden");
        return None;
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
    let timer = FADE_TOAST.with(|fade| dom::set_timeout(fade, duration));
    with(|s| s.toast_timer = timer);
    Some(status)
}

/// The toast's time is up: it fades, and its button (if it has one) goes,
/// focus in it going home.
fn fade_toast() {
    with(|s| s.toast_timer = None);
    if let Some(status) = by_id("data-status") {
        let _ = status.class_list().add_1("is-hidden");
        if let Ok(Some(action)) = status.query_selector(".toast-action") {
            let had_focus = dom::active_element().is_some_and(|a| a == action);
            action.remove();
            if had_focus {
                bridge::focus_home();
            }
        }
    }
}

/// An export or import is running, or done: ☰'s Import data… is disabled
/// meanwhile, and so is the chooser's Everything (☰'s Export data… still
/// opens the chooser, for My notes).
pub fn set_data_busy(busy: bool) {
    with(|s| s.data_busy = busy);
    if let Some(item) = button("import-data-btn") {
        item.set_disabled(busy);
    }
    send_export_facts();
}

/// What the export chooser needs now.
fn export_facts() -> ExportFacts {
    with(|s| ExportFacts {
        notes: s.notes.unwrap_or(0) > 0,
        busy: s.data_busy,
    })
}

/// Tells the chooser what changed.
fn send_export_facts() {
    export_choice::set_facts(export_facts());
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
    findings: u64,
    #[serde(default)]
    findings_included: bool,
    #[serde(default)]
    has_position: bool,
    #[serde(default)]
    preferences: u64,
}

/// The backup (the chooser's Everything, the palette's Export data…):
/// bookmarks, highlights, notes, the reading position and the display
/// settings, to a file the reader chooses. Not while one is running.
pub fn export_data() {
    if with(|s| s.data_busy) {
        return;
    }
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
                    findings: outcome.findings,
                    findings_included: outcome.findings_included,
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

/// ☰ → Export data…: the chooser, Everything (the backup) or My notes (the
/// Export notes dialog); focus goes back to `return_focus`.
fn open_export_choice(return_focus: Option<HtmlElement>) {
    if !export_choice::open(export_facts(), return_focus) {
        set_data_status(text::EXPORT_UNAVAILABLE, true);
    }
}

/// The chooser's My notes and the palette's Export notes… (while there are
/// notes): asks the Export notes dialog to open with what it needs; closing
/// it gives focus back to `return_focus`.
pub fn open_export_notes(return_focus: Option<HtmlElement>) {
    let notes = with(|s| s.notes).unwrap_or(0);
    if notes == 0 {
        return;
    }
    let code = bridge::translation_code();
    let request = OpenRequest {
        note_count: u64::from(notes),
        book_id: card::facts().chapter.map(|c| c.book_id),
        translation_name: reader::translation_name(&code),
        translation_code: code,
    };
    if !export_notes::open(request, return_focus) {
        set_data_status(text::EXPORT_NOTES_UNAVAILABLE, true);
    }
}

/// A modal of the pieces' is about to open: popovers and panels go (focus
/// left alone, the modal takes it), and the search box with `close_search`.
/// A modal still getting ready to open ([`modal_claim`]) won't now.
pub fn modal_opening(close_search: bool) {
    with(|s| s.modals.cancel());
    surfaces::close(false);
    if close_search && search_shown() {
        set_search_open(false);
    }
}

/// A modal is getting ready to open (its answer is on its way): its claim,
/// which any modal opening before it does supersedes ([`modal_holds`]).
pub fn modal_claim() -> Claim {
    with(|s| s.modals.claim())
}

/// Whether the modal that claimed `claim` may still open: no other modal
/// has opened, and none has claimed, since.
pub fn modal_holds(claim: Claim) -> bool {
    with(|s| s.modals.holds(claim))
}

/// ☰ → Import data…: the import preview runs the open dialog, the preview
/// and the import, and says how it's going.
fn import_data() {
    if !crate::import_preview::begin() {
        set_data_status(text::IMPORT_UNAVAILABLE, true);
    }
}

/// After an import: the file's reading position (if chosen), then the
/// bookmarks, highlights and notes, then the result.
pub fn imported(position: Option<PositionPreview>, message: String) {
    // What comes first is asked within the event: the position's chapter,
    // or else the bookmarks. The position is the first pane's, the place
    // the backend keeps (S15), whichever pane is active: the second, open
    // beside it, is this computer's, and no import touches it (S9).
    let first = match position {
        Some(position) => Err(reader::ask(
            PaneId::First,
            Request::Load {
                book_id: position.book_id,
                chapter: position.chapter,
                translation_code: Some(position.translation_code),
                top: false,
            },
        )),
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
        let notice = reader::ask(
            pane::active(),
            Request::Notice {
                message: message.clone(),
            },
        );
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
    if let Some(done) = crate::library::refresh(lists) {
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

/// A Library list has loaded (`items`, or `null` if it failed): every
/// note, kept as their count, for the welcome, the chooser and Export notes.
pub fn library_loaded(list: List, items: &JsValue) {
    if list == List::Notes && items.is_truthy() {
        let count = js_sys::Reflect::get(items, &JsValue::from_str("length"))
            .ok()
            .and_then(|n| n.as_f64())
            .map_or(0, |n| n as u32);
        with(|s| s.notes = Some(count));
        send_export_facts();
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

/// Boot's places to start, as the welcome lists them (none, if they can't
/// be read: that's logged).
fn start_here() -> Option<Vec<StartRow>> {
    let start_here = with(|s| s.start_here.clone()).unwrap_or_else(|| js_sys::Array::new().into());
    serde_wasm_bindgen::from_value(start_here)
        .map_err(|e| ipc::log_error("The welcome request couldn't be read", &JsValue::from(e)))
        .ok()
}

/// ☰ → Show welcome; focus goes back to `return_focus`.
fn open_welcome(return_focus: Option<HtmlElement>) {
    let Some(start_here) = start_here() else {
        return;
    };
    let request = WelcomeRequest {
        startup: false,
        install: None,
        start_here,
    };
    if !welcome::request(request, return_focus) {
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
    let install = crate::install::Install {
        has_saved_position,
        bookmarks_loaded: facts.bookmarks_loaded,
        bookmarks: facts.bookmarks as u64,
        highlights_loaded: facts.highlights_loaded,
        highlights: facts.highlights as u64,
        notes_loaded: notes.is_some(),
        notes: u64::from(notes.unwrap_or(0)),
    };
    let Some(start_here) = start_here() else {
        return;
    };
    let request = WelcomeRequest {
        startup: true,
        install: Some(install),
        start_here,
    };
    if !welcome::request(request, None) {
        with(|s| s.welcome_waiting = Some(home.clone()));
    }
}

// ---- The book and chapter picker's books: boot's, which may come before
// the picker has mounted, so they go again once every piece is there.

/// Boot's books for the picker (`{ otBooks, ntBooks }`, `get_home`'s, as
/// it gave them), kept and sent now: to Back, Forward and Recent places
/// (`nav`), then the picker.
pub fn set_picker_books(books: JsValue) {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Books {
        ot_books: Vec<Book>,
        nt_books: Vec<Book>,
    }
    match serde_wasm_bindgen::from_value::<Books>(books) {
        Ok(books) => with(|s| s.picker_books = Some((books.ot_books, books.nt_books))),
        Err(e) => ipc::log_error("The books couldn't be read", &JsValue::from(e)),
    }
    send_picker_books();
}

fn send_picker_books() {
    if let Some((old, new)) = with(|s| s.picker_books.clone()) {
        crate::nav::set_books(old.clone(), new.clone());
        crate::picker::draw_books(old, new);
    }
}

// ---- The keyboard shortcuts sheet: a request to open carries where focus
// goes back to (what had it, or the button it was opened from).

/// Opens the sheet (`?`, ☰ → Keyboard shortcuts, the palette's command),
/// unless it's open; focus goes back to `opener`, or to what had it. A
/// modal, as the others: one still getting ready to open won't now (D2-01).
pub fn open_shortcuts(opener: Option<HtmlElement>) {
    if shortcuts_open() {
        return;
    }
    let return_focus = opener.or_else(focused);
    modal_opening(true);
    if !crate::shortcuts::open(return_focus) {
        set_data_status(text::SHORTCUTS_UNAVAILABLE, true);
    }
}

// ---- Search, go to a reference, or run a command: the box is the search
// piece's, which opens and closes it (from its button, and when a surface
// opens); this asks it to (its keys, a click away, a dialog opening).

/// Opens or closes the search box.
pub fn set_search_open(open: bool) {
    if crate::search::set_open(open) || !open {
        return;
    }
    set_data_status(text::SEARCH_UNAVAILABLE, true);
}

// ---- The palette: what its commands need of the page, and the commands
// it runs here.

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

/// What the palette's commands need now: Aa's settings, comparing, the
/// shape, the translations and a busy export (the page's), the reader's
/// chapters and history, and whether the card has the chapter's text ready
/// to copy.
pub fn palette_state() -> PaletteState {
    let mut state = PaletteState {
        comparing: is_comparing(),
        partner: crate::aa::partner(),
        compare_notes: crate::prefs::on(&crate::preferences::COMPARE_NOTES),
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
        notes: export_facts().notes,
        export_busy: export_facts().busy,
        import_busy: disabled("import-data-btn"),
        checking: disabled("update-check-btn"),
        chapter_shown: dom::query(".chapter-copy-btn").is_some(),
        ..PaletteState::default()
    };
    reader::palette_facts(&mut state);
    card::palette_facts(&mut state);
    state
}

/// Runs `command`, one of the palette's commands that `page` runs (see
/// `palette::Run::Page`): `copy-chapter`, `copy-chapter-as`,
/// `next-chapter`, `previous-chapter`, `next-book`, `previous-book`,
/// `back`, `forward`, `shortcuts`, `export-notes` (the Export notes dialog)
/// and `export-data` (the backup; neither by way of ☰'s chooser), within
/// the click or key that chose it (copying needs that), after the box has
/// closed and focus has gone back.
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
        // Straight to the dialog and the backup, not by way of ☰'s
        // chooser. The dialog gives focus to the page's focus home when it
        // closes.
        "export-notes" => open_export_notes(None),
        "export-data" => export_data(),
        _ => {}
    }
}

// ---- The launch check for updates: asked when boot is done, which may
// come before the updater has mounted, so it's asked again once every
// piece is there.

/// Asks the updater for its quiet launch check.
pub fn ask_update_check() {
    let asked = crate::updates::launch_check();
    with(|s| s.update_check_waiting = !asked);
}

/// Every piece has mounted (`lib`'s ready list): boot's Library lists, its
/// welcome question, the picker's books and the launch check, asked again if
/// they came first.
pub fn on_ready() {
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

/// Listens to the ☰ items, the search and About buttons and the translation
/// changing; then the
/// click dispatcher (`clicks`) and the keyboard dispatcher (`keyboard`).
/// Called first of all, before any piece mounts, so these run first.
pub fn mount() {
    let on_click = |id: &str, handler: fn()| {
        if let Some(el) = by_id(id) {
            dom::on_click(&el, move |_| handler());
        }
    };
    on_click("export-data-btn", || {
        open_export_choice(by_id("menu-toggle"))
    });
    on_click("import-data-btn", import_data);
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
            let load = reader::ask(pane::active(), Request::Translation);
            spawn_local(async move {
                load.await;
            });
            let _ = refresh_library(&["bookmarks", "highlights"]);
        });
    }
    // Without the pieces there is no About to open.
    on_click("about-btn", || {
        if by_id("about-overlay").is_none() {
            set_data_status(text::ABOUT_UNAVAILABLE, true);
        }
    });
}
