//! The whole bridge between this crate and the page's JavaScript
//! (`ui/index.html`), in one place. Everything the two sides share is
//! listed here; nothing else crosses. The model is custom events on
//! `document`, with what the other side needs as a snapshot in their
//! `detail`, so neither reads the other's variables. The page's half is
//! the short blocks in `ui/index.html` that dispatch and listen for these
//! names, each pointing here.
//!
//! Page to Rust (requests to open; each is cancelable, and the piece
//! cancels it to say it has it; if nothing does, the WebAssembly didn't
//! load, and the page says that piece isn't available):
//! - [`OPEN_EVENT`] (`gospel-getter:export-notes`): Export notes…, with an
//!   [`OpenRequest`].
//! - [`SHORTCUTS_EVENT`] (`gospel-getter:shortcuts`): the keyboard
//!   shortcuts sheet, no snapshot (the table is this crate's,
//!   `shortcut_table`).
//! - [`WELCOME_EVENT`] (`gospel-getter:welcome`): the welcome, asked at the
//!   end of boot (with what the fresh-install rule needs) or opened from
//!   ☰, with the places to start ([`WelcomeRequest`]). A startup request
//!   nobody answered (boot beat the module) is sent again on
//!   [`READY_EVENT`].
//! - [`IMPORT_EVENT`] (`gospel-getter:import`): Import data…, no detail.
//! - [`NOTE_EVENT`] (`gospel-getter:note`): the note editor, with a
//!   [`NoteRequest`] (the passage quoted as markup the page rendered) and
//!   the note itself. [`NOTE_OTHERS_EVENT`] (`gospel-getter:note-others`)
//!   follows the selection's notes while it may be open.
//! - `detail.returnFocus` on the sheet's, welcome's and editor's requests:
//!   the element focus goes back to ([`return_focus`]).
//! - [`LAUNCH_CHECK_EVENT`] (`gospel-getter:update-check`): boot is done, so
//!   the updater runs its quiet launch check, unless `html[data-update-check]`
//!   is `off` ([`launch_check_on`]). Sent again on [`READY_EVENT`] if nobody
//!   answered (boot beat the module).
//! - [`LIBRARY_REFRESH_EVENT`] (`gospel-getter:library-refresh`): the one way
//!   the page has the Library's lists reloaded (a bookmark or highlight
//!   changed in the reader, a translation switch, a note saved, an import,
//!   boot), `{ lists }` naming them. The Library cancels it and puts in
//!   `detail.done` a promise that settles once each list has its answer;
//!   asked before the module loaded, the page asks again on [`READY_EVENT`].
//! - [`SEARCH_EVENT`] (`gospel-getter:search`): `{ open }`, the page opens
//!   the search box (`/`, Ctrl+K) or closes it (Escape outside it, a click
//!   away, a dialog opening); the search box cancels it to say it has it.
//! - [`PALETTE_STATE_EVENT`] (`gospel-getter:palette-state`): asked by this
//!   crate (see Rust to page), answered by the page within the event:
//!   `detail.state` is a [`PaletteState`], what the palette's commands need
//!   of the page (Aa's settings, comparing, the chapter shown, the passage
//!   history, a busy export), as it is at that moment.
//! - [`PICKER_BOOKS_EVENT`] (`gospel-getter:picker-books`): `{ otBooks,
//!   ntBooks }`, the books as `get_home` listed them, from boot, for the
//!   book and chapter picker's lists; sent again on [`READY_EVENT`].
//! - [`READER_EVENT`] (`gospel-getter:reader`): the one way the page asks
//!   the reader (`reader`) for anything, `{ action, ... }`: `start` (boot's,
//!   with `home`, `get_home`'s answer: the translations, then the chapter
//!   the reader was on, then the history from there; asked again on
//!   [`READY_EVENT`] if nobody answered), `load` (`{ bookId, chapter,
//!   translationCode, top }`: ← and →, by `#reading-pane-inner`'s
//!   `data-prev-*` and `data-next-*`, and an import's position), `book`
//!   (`{ which }`, `prev` or `next`: `[` and `]`), `translation`
//!   (the translation changed: the chapter on screen in it), `jump`
//!   (`{ bookId, chapter, start, end }`: the selection card's Go to),
//!   `history` (`{ step }`, -1 or 1: Alt+← and Alt+→) and `notice`
//!   (`{ message }`, for above the chapter). The reader cancels it to say
//!   it has it, and puts in `detail.done` a promise of whether a chapter
//!   was put on screen. Nothing answers without this module.
//! - [`VERSE_MARKUP_EVENT`] (`gospel-getter:verse-markup`): `{ verses }` or
//!   `{ footnotes }`, as the backend sent them: the selection card's verses
//!   and footnotes as markup, answered within the event in `detail.html`,
//!   one string each (verses without note markers).
//!
//! Rust to page:
//! - [`READY_EVENT`] (`gospel-getter:pieces-ready`): the module has loaded
//!   and every piece is listening.
//! - [`SURFACE_OPEN_EVENT`] (`gospel-getter:surface-open`) and
//!   [`SURFACE_CLOSE_EVENT`] (`gospel-getter:surface-close`): `{ name }`, a
//!   header surface opened or closed (`surfaces`: "translation", "aa",
//!   "library", "menu", "about" or "picker"). This crate hears them too:
//!   one opening closes the search box, and About closing part way through
//!   an install brings the banner back.
//! - [`PICKER_SHOWN_EVENT`] (`gospel-getter:picker-shown`): no `detail`,
//!   the picker has appeared (the sidebar or the popover, `surfaces`), so
//!   it scrolls to the book being read (`picker`).
//! - [`TOAST_EVENT`] (`gospel-getter:toast`): `{ message, isError }` for
//!   the page's toast (`setDataStatus`).
//! - [`MODAL_OPENING_EVENT`] (`gospel-getter:modal-opening`): a piece is
//!   about to open on its own (the welcome at startup, the import preview
//!   once a file is chosen): popovers and panels go, and the search box
//!   with `closeSearch`.
//! - [`CHAPTER_LOADING_EVENT`] (`gospel-getter:chapter-loading`): no
//!   `detail`, the reader has asked for a chapter: the page drops the verse
//!   selection at once, so nothing acts on it meanwhile.
//! - [`CHAPTER_SHOWN_EVENT`] (`gospel-getter:chapter-shown`): a
//!   [`ChapterShown`] snapshot each time the reader has drawn a chapter
//!   (`#reading-pane-inner` is new): what the page's selection card,
//!   copying, verse marks and keys need of it, the verses as the backend
//!   sent them. The page then closes Copy as…, fetches the chapter's copy
//!   text, drops the selection and marks the verses, as it did after
//!   drawing one itself.
//! - [`CHAPTER_SELECT_EVENT`] (`gospel-getter:chapter-select`):
//!   `{ translationCode, start, end }`, a passage opened (a jump, Back or
//!   Forward) whose verses that translation numbers: the page selects them,
//!   scrolls them to the middle and places the card, and answers within the
//!   event in `detail.selection` (`{ start, end }` as now selected).
//! - [`NOTE_EDIT_EVENT`] (`gospel-getter:edit-note`): `{ note }`, a note's
//!   Edit in the Library has opened its passage: the page opens the note
//!   editor on it (as `list_all_notes` gave it).
//! - [`PICKER_CURRENT_EVENT`] (`gospel-getter:picker-current`): `{ bookId,
//!   chapter }`, sent within this crate by the reader each time it puts a
//!   chapter on screen: the picker marks it and shows its book's chapters.
//! - [`ABOUT_CREDITS_EVENT`] (`gospel-getter:about-credits`): `{ credits }`,
//!   each bundled translation's credit for About, sent within this crate by
//!   the reader from boot's answer.
//! - [`DATA_BUSY_EVENT`] (`gospel-getter:data-busy`): `{ busy }`, the ☰
//!   Export and Import items disabled while an import runs.
//! - [`IMPORTED_EVENT`] (`gospel-getter:imported`): `{ position, message }`
//!   after an import: the page reloads the position, bookmarks, highlights
//!   and notes, then says `message`.
//! - [`NOTE_DONE_EVENT`] (`gospel-getter:note-done`): `{ action, note }`
//!   after the editor added, saved or deleted a note: the page's status
//!   line, and a deletion's Undo (`restoreNote`, which stays the page's
//!   under its shared `actionAttempt` guard).
//! - [`LIBRARY_LOADED_EVENT`] (`gospel-getter:library-loaded`): `{ list,
//!   items }` each time the Library's bookmarks or notes have loaded (or
//!   failed to: `items` is `null`), the answer as the backend gave it: the
//!   page's own copy, for the reader's bookmark markers and the selection
//!   card's Bookmark button, the welcome's fresh-install facts and Export
//!   notes. So the page never asks the backend for them itself.
//! - [`PALETTE_STATE_EVENT`]: `{}`, asked whenever the palette is drawn or
//!   a command is chosen; the page fills in `detail.state` (above).
//! - [`COMMAND_EVENT`] (`gospel-getter:command`): `{ command }`, a
//!   palette command whose code is the page's (copy the chapter, Copy
//!   as…, turn the chapter or the book, Back and Forward, the shortcuts
//!   sheet), run within the click or key that chose it (the clipboard needs
//!   that), after the box has closed and focus has gone back (the turns and
//!   Back and Forward come back to the reader as [`READER_EVENT`]s, as the
//!   keys' do). The commands
//!   whose controls are on the page are run by clicking them (see Page
//!   controls); text size, a translation and Compare with are Aa's own.
//! - [`aa::COMPARE_CHANGE_EVENT`](crate::aa::COMPARE_CHANGE_EVENT)
//!   (`gospel-getter:compare-change`): no `detail`, Compare went on or off,
//!   or what it compares with changed: the reader shows the chapter in the
//!   new layout (heard within this crate).
//! - [`aa::LAYOUT_CHANGE_EVENT`](crate::aa::LAYOUT_CHANGE_EVENT)
//!   (`gospel-getter:layout-change`): no `detail`, the layout changed: the
//!   page moves the selection's card to its verses' new place.
//!
//! Page globals this crate calls (small, named, existing or exposed for it):
//! - `window.gospelGetterComparePartner()` (the page's, its rule still the
//!   page's): the translation Compare shows beside the one being read, for
//!   Aa's "with" choices and the reader's comparisons.
//! - `window.gospelGetterRefreshNotes()` (the page's `refreshNotes`, a
//!   promise): the editor waits for it before closing ([`refresh_notes`]).
//! - `window.gospelGetterRefreshHighlights()` (the page's
//!   `refreshHighlights`, a promise): after the Library removes a
//!   highlight, the reader's highlights and then the list, waited for
//!   before focus moves on ([`refresh_highlights`]).
//!
//! Page globals this crate sets: `window.gospelGetterShortcuts`, the table
//! of shortcuts (`shortcut_table`), as the page had it, for its frontend
//! tests ([`export_shortcuts`]); `window.gospelGetterPreferences` (`aa`):
//! `current()`, the display settings an export carries, which the page's
//! Export data… reads, and `apply(values)`; and
//! `window.gospelGetterSurfaces` (`surfaces`), the header surfaces for the
//! page's reader: `current()` (the open one's name, or `null`),
//! `close(restoreFocus)`, `toggle(name)` and `openPicker()`, each answered
//! within the call. Without this module none of them is there, and the page
//! says what isn't available.
//!
//! Aa's settings' records, the hidden `<select>`s `#theme-select`,
//! `#text-size-select`, `#line-spacing-select`, `#width-select` and
//! `#layout-select`, are this crate's (`aa` fills them from the one table
//! and keeps them), where the page's other scripts read them as before;
//! choosing sends their `change` as before. Aa draws inside the page's
//! `#aa-popover` and `#translation-popover`, and answers their controls,
//! `#reader-exit` and the size keys.
//!
//! The header's popovers and panels (`#translation-popover`,
//! `#aa-popover`, `#library-panel`, `#app-menu`, About's `#about-overlay`,
//! and `#picker` as the popover) are the page's markup, shown and hidden
//! here (`surfaces`): their `hidden`, their buttons' `aria-expanded`
//! (`#translation-toggle`, `#aa-toggle`, `#library-toggle`,
//! `#menu-toggle`, `#ref-btn`, `#sidebar-toggle`), the picker's `role` and
//! `#ref-btn`'s `aria-haspopup`, and `html[data-wide]` and
//! `html[data-picker]` after the pre-paint script's first ones. This
//! answers those buttons, `#library-close`, `#about-btn`, the ☰ menu's
//! keys, a click outside a popover, Escape (after the page's keyboard
//! dispatcher, which marks an Escape it used as handled) and the window
//! crossing 1200px.
//!
//! The reading pane (`#reading-pane`'s contents: `#reading-pane-inner`
//! with its `data-current-*`, `data-prev-*` and `data-next-*`, the verses,
//! the chapter's end and its lists), the notice above it
//! (`#reader-notice`, with Try again), the reference button's text and
//! label (`#ref-text`, `#ref-btn`), the window title, the translation
//! button's text and label (`#translation-toggle`) and its record's options
//! (`#translation-select`) are the reader's, drawn and kept here. The page
//! reads the pane's markup as before (its selection card goes after the
//! last selected verse, Copy as… after `.chapter-copy`), and its click
//! handler on the body, which runs first, leaves the reader's controls in
//! the pane (the chapter steps, a neighbor's heading, the disclosures, the
//! cross-reference chips and credit links) to the reader's own, also on the
//! body. The disclosures' state is the reader's in local storage
//! (`gospel-getter-footnotes`, `gospel-getter-cross-references`: `open` or
//! `closed`).
//!
//! If this module fails to load, the page's loader says so with
//! `gospel-getter:pieces-failed` on `document`, no `detail`; the page's
//! small inline fallback then shows the chapter as plain text (see
//! `ui/index.html`). Nothing in this crate hears it.
//!
//! Page controls this crate uses as a reader would: `#theme-select`, read
//! and changed (with a `change` event, as Aa does) for the welcome's looks
//! ([`theme`], [`choose_theme`]); the page's focus home
//! (`#aa-toggle` in reading mode, else `#menu-toggle`, [`focus_home`]);
//! `#translation-select`, the translation button's record of the
//! translation, read for the Library's previews and the search
//! ([`translation_code`]), and listened to (`change`: an open search runs
//! again in the new translation); for the editor's focus going back, the
//! selection card's `.verse-notes .note-edit[data-note-id]`,
//! `.verse-actions .verse-note-btn` and `.chapter-main .chapter-heading`;
//! and, for the search box, `#search-panel` (the page's, whose `hidden`
//! this crate keeps; the page reads it to know the box is open) and
//! `#search-toggle` (its click, its `aria-expanded`, focus back to it),
//! and the controls the palette's commands click as a reader would: by id
//! (`#compare-btn`, `#reader-mode-btn`, `#reader-exit`,
//! `#random-chapter-btn`, `#sidebar-toggle`, `#export-notes-btn`,
//! `#export-data-btn`, `#import-data-btn`, `#welcome-btn`, `#about-btn`
//! then `#update-check-btn`, `#library-toggle` then a `#library-tab-*`),
//! or a `[data-value]` choice in `#layout-choices`, `#width-choices` or
//! `#theme-choices`.
//!
//! The pieces' overlays (`#export-notes-overlay`, `#shortcuts-overlay`,
//! `#welcome-overlay`, `#import-overlay`, `#note-overlay`) keep their
//! `hidden` truthful: the page's keyboard dispatcher and its Shift-ghost
//! check read them, looked up by id when asked, to know a modal is open.
//! Each piece has the keyboard while it's open: a capturing `keydown` on
//! `window` that stops every other handler.
//!
//! The Library's tabs and lists (`#library-panel`'s tablist and tab
//! panels, with `#export-notes-btn`, whose click stays the page's) are this
//! crate's elements, put inside the page's panel, which `surfaces` opens
//! and closes, focusing its chosen tab.
//!
//! The search box's contents (`#search-input`, `#search-status`,
//! `#search-results` and `#search-more`) are this crate's elements, put
//! inside the page's `#search-panel`. The page's keyboard dispatcher
//! ignores keys there, as before.
//!
//! The book and chapter picker's books and chapters (the `.book-item`s in
//! `#ot-book-list` and `#nt-book-list`, and `#picker-chapters`) are this
//! crate's elements, drawn inside the page's `#picker`, whose clicks and
//! keys this crate answers (Random chapter, `#random-chapter-btn`, is the
//! page's button). Its two shapes are `surfaces`': it shows and hides
//! `#picker` as the sidebar or the popover (one of the surfaces, `picker`),
//! keeps the reading place as the sidebar comes and goes, and focuses the
//! chapter being read (or its book, or Random chapter) by the marks drawn
//! here.
//!
//! About (`#about-overlay`) and the update banner (`#update-banner`) are
//! this crate's elements too, put where the page had them. About's `hidden`
//! is `surfaces`' (it shows and hides it as a surface, looking it up when
//! it does); its keys are on the overlay itself, as they were. The
//! page looks the banner up for its toast, which sits above it. The launch
//! check's setting, `#update-check-toggle`, is drawn here and kept here too
//! (`gospel-getter-update-check`, applied as `html[data-update-check]`),
//! by a listener for its `change` on the document (`aa`).
//!
//! Local storage: the saved settings' keys are this crate's to write
//! (`prefs`), as the one table says (`preferences`); the page's pre-paint
//! script only reads them. The welcome reads the `gospel-getter-*` keys
//! (the fresh-install rule) and writes [`WELCOME_KEY`].
//!
//! Shared with the backend, not the page: the commands the pieces invoke,
//! through the same `window.__TAURI__.core.invoke` the page uses (see
//! `ipc.rs`; their shapes are in `dto.rs`).

use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use web_sys::{CustomEvent, CustomEventInit, Document, HtmlElement};

use crate::ipc;
use crate::palette::PaletteState;

/// The page asks the dialog to open: `document.dispatchEvent(new
/// CustomEvent('gospel-getter:export-notes', { cancelable: true, detail }))`.
pub const OPEN_EVENT: &str = "gospel-getter:export-notes";

/// The dialog has something to say in the page's toast.
pub const TOAST_EVENT: &str = "gospel-getter:toast";

/// What the page knows and the dialog needs, taken when the button is
/// clicked: how many notes there are (none: nothing to export), the book
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

/// The page asks the shortcuts sheet to open (`?`, ☰ → Keyboard shortcuts,
/// the palette's command): a cancelable `CustomEvent` whose `detail` is
/// `{ returnFocus }` (see [`return_focus`]).
pub const SHORTCUTS_EVENT: &str = "gospel-getter:shortcuts";

/// Puts the table of shortcuts where the page had it,
/// `window.gospelGetterShortcuts`: an array of `{ id, group, keys, aria,
/// does, handles }`, as `SHORTCUTS` was.
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

/// Sends the page `name` as a `CustomEvent` on `document`, with `detail`.
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

/// This module has loaded and every piece is listening: a `CustomEvent`
/// on `document`, no `detail`. The page then sends again any request it
/// made before (the welcome's, at the end of a boot that beat the module).
pub const READY_EVENT: &str = "gospel-getter:pieces-ready";

/// Says [`READY_EVENT`].
pub fn ready() {
    if let (Some(document), Ok(event)) = (document(), CustomEvent::new(READY_EVENT)) {
        let _ = document.dispatch_event(&event);
    }
}

/// A modal of this crate is about to open: put the page's popovers and
/// panels away (`closeSurfaces(false)`, focus left alone) and, with
/// `closeSearch`, the search box. A `CustomEvent` on `document`; the page
/// does it at once, before the modal takes focus.
pub const MODAL_OPENING_EVENT: &str = "gospel-getter:modal-opening";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModalOpening {
    close_search: bool,
}

/// Says [`MODAL_OPENING_EVENT`].
pub fn modal_opening(close_search: bool) {
    dispatch(MODAL_OPENING_EVENT, &ModalOpening { close_search });
}

/// The page asks the welcome to open: a cancelable `CustomEvent` whose
/// `detail` is a [`WelcomeRequest`] plus `returnFocus`. At the end of boot
/// it is a question (`startup`, with the facts the fresh-install rule needs;
/// the welcome opens only for a fresh install); from ☰ → Show welcome it
/// opens. Either way the welcome cancels it to say it has it.
pub const WELCOME_EVENT: &str = "gospel-getter:welcome";

/// What the welcome needs from the page.
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

/// The page asks the import to begin (its ☰ → Import data…): a cancelable
/// `CustomEvent`, no `detail`. The import cancels it to say it has it.
pub const IMPORT_EVENT: &str = "gospel-getter:import";

/// Import or export is under way, or done: a `CustomEvent` whose `detail`
/// is `{ busy }`. The page disables (or enables) its two ☰ items for it
/// (`setDataBusy`), as it does for its own export.
pub const DATA_BUSY_EVENT: &str = "gospel-getter:data-busy";

#[derive(Debug, Serialize)]
struct DataBusy {
    busy: bool,
}

/// Says [`DATA_BUSY_EVENT`].
pub fn data_busy(busy: bool) {
    dispatch(DATA_BUSY_EVENT, &DataBusy { busy });
}

/// An import has changed the reader's data: a `CustomEvent` whose `detail`
/// is `{ position, message }`. The page reloads what it shows of it, in
/// order: the chapter at `position` (`{ translationCode, bookId, chapter }`,
/// or `null` to stay put), then the bookmarks, highlights and notes; then
/// it says `message` in its notice and its toast.
pub const IMPORTED_EVENT: &str = "gospel-getter:imported";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportedPosition<'a> {
    translation_code: &'a str,
    book_id: i64,
    chapter: i64,
}

#[derive(Debug, Serialize)]
struct Imported<'a> {
    position: Option<ImportedPosition<'a>>,
    message: &'a str,
}

/// Says [`IMPORTED_EVENT`].
pub fn imported(position: Option<&crate::dto::PositionPreview>, message: &str) {
    let position = position.map(|p| ImportedPosition {
        translation_code: &p.translation_code,
        book_id: p.book_id,
        chapter: p.chapter,
    });
    dispatch(IMPORTED_EVENT, &Imported { position, message });
}

/// The page opens the note editor (the selection card's + Add note and
/// Edit, N, the Library's Edit): a cancelable `CustomEvent` whose `detail`
/// is a [`NoteRequest`] plus `note` (the note being edited, as the page has
/// it, or `null`; handed back with [`NOTE_DONE_EVENT`]) and `returnFocus`.
/// The page puts its popovers and the search box away first.
pub const NOTE_EVENT: &str = "gospel-getter:note";

/// What the editor needs from the page to open.
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
    /// for the page ([`VERSE_MARKUP_EVENT`]: KJV supplied words in italics,
    /// no note markers, escaped), with what the translation doesn't number
    /// said at the end (escaped by the page). Shown as it is.
    pub passage_html: String,
    /// When adding, the other notes on these verses, read-only.
    pub others: Vec<OtherNote>,
}

/// The note being edited: its id and text (the rest stays the page's).
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
/// the date it was written (as the page shows dates) and its text.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OtherNote {
    pub reference: String,
    pub date: String,
    pub body: String,
}

pub fn note_request(event: &CustomEvent) -> Result<NoteRequest, JsValue> {
    detail(event)
}

/// The page's notes on the selection have changed while the editor may be
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

/// Refreshes the page's notes after the editor has saved or deleted one:
/// the markers, the selection card's Notes section and the Library, through
/// `window.gospelGetterRefreshNotes()` (the page's `refreshNotes`, which
/// reports its own failures), waiting for it, so the editor can close onto
/// the note's refreshed Edit.
pub async fn refresh_notes() {
    call_page_refresh("gospelGetterRefreshNotes").await;
}

/// Refreshes the reader's highlights after the Library has removed one:
/// `window.gospelGetterRefreshHighlights()` (the page's `refreshHighlights`,
/// which then has the list reloaded, through [`LIBRARY_REFRESH_EVENT`]),
/// waiting for it, so focus moves on in the refreshed list.
pub async fn refresh_highlights() {
    call_page_refresh("gospelGetterRefreshHighlights").await;
}

/// Calls the page's global `name()`, a promise, and waits for it.
async fn call_page_refresh(name: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let refresh = js_sys::Reflect::get(&window, &JsValue::from_str(name))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
    let Some(refresh) = refresh else {
        return;
    };
    if let Ok(promise) = refresh
        .call0(&JsValue::NULL)
        .and_then(|p| p.dyn_into::<js_sys::Promise>())
    {
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    }
}

/// The editor has added, saved or deleted a note (and the page's notes are
/// refreshed): a `CustomEvent` whose `detail` is `{ action, note }`,
/// `action` being `added`, `saved` or `deleted`, and `note` the deleted
/// note as the backend returned it (else `null`). The page says so in the
/// selection card's status line, a deletion with its Undo (the page's
/// `restoreNote`), or in its toast when the card is gone.
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

/// The page's boot is done: run the quiet launch check (unless it's turned
/// off). A cancelable `CustomEvent` on `document`, no `detail`; the updater
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

/// The page has the bundled texts' credits for About: a `CustomEvent`
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

/// The page asks the Library to reload lists: a cancelable `CustomEvent`
/// whose `detail` is `{ lists }`, each `bookmarks`, `highlights` or `notes`
/// (`library_text::List`). The Library cancels it to say it has it and sets
/// `detail.done` to a promise that settles once every list named has its
/// answer, shown (or dropped, when a newer reload of the same list has
/// already been asked for: only the latest of overlapping reloads shows).
/// The page waits for it where it waited for its own reloads.
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
/// page's `currentTranslation` reads it.
pub fn translation_code() -> String {
    document()
        .and_then(|d| d.get_element_by_id("translation-select"))
        .and_then(|el| el.dyn_into::<web_sys::HtmlSelectElement>().ok())
        .map(|select| select.value())
        .unwrap_or_default()
}

/// The page opens or closes the search box: a cancelable `CustomEvent`
/// whose `detail` is `{ open }`. The search box cancels it to say it has
/// it; when nothing does (the module didn't load), the page says search
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

/// This crate asks what the palette's commands need of the page: a
/// `CustomEvent` whose `detail` is `{}`; the page's listener sets
/// `detail.state` (a [`PaletteState`]) before the dispatch returns.
pub const PALETTE_STATE_EVENT: &str = "gospel-getter:palette-state";

/// The page as the palette's commands need it now (the default, all off and
/// empty, if the page didn't say, or said something unreadable, which is
/// logged).
pub fn palette_state() -> PaletteState {
    let Some(document) = document() else {
        return PaletteState::default();
    };
    let detail = js_sys::Object::new();
    let init = CustomEventInit::new();
    init.set_detail(&detail);
    let Ok(event) = CustomEvent::new_with_event_init_dict(PALETTE_STATE_EVENT, &init) else {
        return PaletteState::default();
    };
    let _ = document.dispatch_event(&event);
    let state = js_sys::Reflect::get(&detail, &JsValue::from_str("state")).unwrap_or_default();
    if state.is_undefined() {
        return PaletteState::default();
    }
    let mut state = serde_wasm_bindgen::from_value(state).unwrap_or_else(|e| {
        ipc::log_error("The page's palette state can't be read", &JsValue::from(e));
        PaletteState::default()
    });
    crate::reader::palette_facts(&mut state);
    state
}

/// A palette command the page runs: a `CustomEvent` whose `detail` is
/// `{ command }` (`palette::Run::Page`): `copy-chapter`,
/// `copy-chapter-as`, `next-chapter`, `previous-chapter`, `next-book`,
/// `previous-book`, `back`, `forward` and `shortcuts`.
pub const COMMAND_EVENT: &str = "gospel-getter:command";

#[derive(Debug, Serialize)]
struct PageCommand<'a> {
    command: &'a str,
}

/// Says [`COMMAND_EVENT`]: the page runs it before this returns.
pub fn page_command(command: &str) {
    let Ok(detail) = ipc::args(&PageCommand { command }) else {
        return;
    };
    dispatch_object(COMMAND_EVENT, &detail);
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

/// The page asks the reader: a cancelable `CustomEvent` whose `detail` is
/// `{ action, ... }` (`reader::Request`). The reader cancels it to say it
/// has it, and sets `detail.done` ([`reader_done`]).
pub const READER_EVENT: &str = "gospel-getter:reader";

/// Answers a [`READER_EVENT`]: `detail.done = done`, a promise of whether a
/// chapter was put on screen.
pub fn reader_done(event: &CustomEvent, done: &js_sys::Promise) {
    let _ = js_sys::Reflect::set(&event.detail(), &JsValue::from_str("done"), done);
}

/// The reader has asked for a chapter: a `CustomEvent`, no `detail`. The
/// page drops the selection at once.
pub const CHAPTER_LOADING_EVENT: &str = "gospel-getter:chapter-loading";

/// Says [`CHAPTER_LOADING_EVENT`]; the page has dropped the selection when
/// this returns.
pub fn chapter_loading() {
    if let (Some(document), Ok(event)) = (document(), CustomEvent::new(CHAPTER_LOADING_EVENT)) {
        let _ = document.dispatch_event(&event);
    }
}

/// The reader has drawn a chapter: a `CustomEvent` whose `detail` is a
/// [`ChapterShown`].
pub const CHAPTER_SHOWN_EVENT: &str = "gospel-getter:chapter-shown";

/// What the page's selection card, copying, verse marks and keys need of
/// the chapter on screen, as its `reading`, `lastDto` and `lastCompare`
/// gave it: `{ bookId, bookName, chapter, translationCode, columns,
/// currentVerses, compareFirst }`.
pub struct ChapterShown<'a> {
    pub book_id: i64,
    pub book_name: &'a str,
    pub chapter: i64,
    /// The translation being read (the left column while comparing).
    pub translation_code: &'a str,
    /// Each column on screen, by translation: `{ [code]: { verses } }`,
    /// the verses as the backend sent them (`text`, `parts`, `footnotes`;
    /// the only source of verse text for anything acting on a selection).
    pub columns: Vec<(&'a str, &'a JsValue)>,
    /// `get_reading`'s verses as it sent them, with their
    /// cross-references (the same in every column).
    pub current_verses: &'a JsValue,
    /// The first column of the last comparison fetched, if any: the
    /// translation a cross-reference preview reads while comparing.
    pub compare_first: Option<&'a str>,
}

/// Says [`CHAPTER_SHOWN_EVENT`]; the page has done its part when this
/// returns.
pub fn chapter_shown(shown: &ChapterShown<'_>) {
    let set = |target: &js_sys::Object, key: &str, value: &JsValue| {
        let _ = js_sys::Reflect::set(target, &JsValue::from_str(key), value);
    };
    let columns = js_sys::Object::new();
    for (code, verses) in &shown.columns {
        let column = js_sys::Object::new();
        set(&column, "verses", verses);
        set(&columns, code, &column);
    }
    let detail = js_sys::Object::new();
    set(&detail, "bookId", &JsValue::from_f64(shown.book_id as f64));
    set(&detail, "bookName", &JsValue::from_str(shown.book_name));
    set(&detail, "chapter", &JsValue::from_f64(shown.chapter as f64));
    set(
        &detail,
        "translationCode",
        &JsValue::from_str(shown.translation_code),
    );
    set(&detail, "columns", &columns);
    set(&detail, "currentVerses", shown.current_verses);
    set(
        &detail,
        "compareFirst",
        &shown.compare_first.map_or(JsValue::NULL, JsValue::from_str),
    );
    dispatch_object(CHAPTER_SHOWN_EVENT, &detail);
}

/// The reader has opened a passage whose verses are numbered: a
/// `CustomEvent` whose `detail` is `{ translationCode, start, end }`. The
/// page selects them and answers within the event in `detail.selection`.
pub const CHAPTER_SELECT_EVENT: &str = "gospel-getter:chapter-select";

/// Says [`CHAPTER_SELECT_EVENT`]: the page's selection once it has
/// selected them (its first and last verse), if it has one.
pub fn chapter_select(translation_code: &str, start: i64, end: i64) -> Option<(i64, i64)> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Select<'a> {
        translation_code: &'a str,
        start: i64,
        end: i64,
    }
    #[derive(Deserialize)]
    struct Selected {
        start: i64,
        end: i64,
    }
    let detail = ipc::args(&Select {
        translation_code,
        start,
        end,
    })
    .ok()?;
    dispatch_object(CHAPTER_SELECT_EVENT, &detail);
    let selection = js_sys::Reflect::get(&detail, &JsValue::from_str("selection")).ok()?;
    if selection.is_null() || selection.is_undefined() {
        return None;
    }
    serde_wasm_bindgen::from_value::<Selected>(selection)
        .ok()
        .map(|s| (s.start, s.end))
}

/// The page's selection card asks for markup: a `CustomEvent` whose
/// `detail` is `{ verses }` or `{ footnotes }`; answered within the event
/// ([`markup_answer`]).
pub const VERSE_MARKUP_EVENT: &str = "gospel-getter:verse-markup";

/// Answers a [`VERSE_MARKUP_EVENT`]: `detail.html = html`.
pub fn markup_answer(event: &CustomEvent, html: &js_sys::Array) {
    let _ = js_sys::Reflect::set(&event.detail(), &JsValue::from_str("html"), html);
}

/// A note's passage is open: a `CustomEvent` whose `detail` is `{ note }`;
/// the page opens the note editor on it.
pub const NOTE_EDIT_EVENT: &str = "gospel-getter:edit-note";

/// Says [`NOTE_EDIT_EVENT`].
pub fn edit_note(note: &JsValue) {
    let detail = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&detail, &JsValue::from_str("note"), note);
    dispatch_object(NOTE_EDIT_EVENT, &detail);
}
