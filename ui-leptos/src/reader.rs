//! The reader: the chapter on screen, and how it gets there. It reads a
//! chapter from the backend (`get_reading`, with `get_compare` beside it
//! while comparing), draws it into the page's `#reading-pane` (the verses,
//! the faded neighbors or the Bible's edge cards, Compare's grid, the
//! chapter's end and its Footnotes and Cross references), says where the
//! reader is on the reference button and in the window's title, keeps the
//! reading position for the next launch, says when a chapter couldn't be
//! read (with Try again), and keeps the passage history (Back and Forward,
//! with the mouse's back and forward buttons and the bar's, `nav`, which
//! it tells where they go and which places to list in Recent places). It fills the translation
//! button's record, `#translation-select`, from boot's answer, and answers
//! its own controls in the pane: the chapter steps, a neighbor's heading,
//! the two disclosures, the chapter's cross-references and their credit
//! links.
//!
//! The selection card (`card`) hears each chapter loading and shown, and
//! each passage to select, from here; boot (`boot`), the keyboard
//! (`keyboard`), the click dispatcher (`clicks`) and the page's glue
//! (`page`) ask it for chapters ([`start`], [`ask`]). With a second pane
//! open beside the first (`pane`, [`open_beside`]), each pane has its own
//! chapter, translation, loads and history, and the header says the active
//! one's; only the first pane's place is the backend's (S15), and the
//! second's is kept on this computer (`pane::keep_place`). A stale answer never
//! overwrites a newer one: each load has its token, each step back or
//! forward its own, and a position is saved only for a chapter put on
//! screen, in the order they were shown. What outlives a load has its own
//! generation (`intent`): a note's Edit waiting for its jump opens the note
//! only if nothing newer has happened (another navigation, or any note
//! editor opening), and Compare's own fetch, when it's switched on or its
//! translations change, shows (or says it failed) only while it's still the
//! comparison of the chapter on screen asked for.
//!
//! The pane is rebuilt whole per chapter, from markup (`reader_text`): the
//! page moves its selection card, Copy as… and the cross-reference preview
//! in and out of it, which a view owning the pane's nodes would fight.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use serde::Serialize;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_futures::spawn_local;
use web_sys::{Element, HtmlElement, HtmlOptionElement, HtmlSelectElement, MouseEvent};

use crate::aa;
use crate::aa_text::Beside;
use crate::bridge;
use crate::card;
use crate::dom::{self, by_id, closest, data_number};
use crate::dto::{Compare, CompareArgs, Home, Position, Reading, ReadingArgs, Verse};
use crate::intent::Generation;
use crate::ipc;
use crate::nav::{self, Facts};
use crate::nav_text::RecentPlace;
use crate::palette::PaletteState;
use crate::pane::{self, PaneId, PerPane};
use crate::pane_text::SecondPlace;
use crate::reader_text::{self as text, Disclosure, History, Opened, Place, Which};
use crate::surfaces;

/// A load, a jump or a step: whether it put a chapter on screen.
pub type Load = Pin<Box<dyn Future<Output = bool>>>;

/// A passage opened: whether it put its chapter on screen, and the verses
/// the selection card then had selected (`card::select_passage`).
type OpenedPassage = Pin<Box<dyn Future<Output = (bool, Option<(i64, i64)>)>>>;

/// The chapter on screen.
#[derive(Debug, Clone)]
struct Here {
    book_id: i64,
    book_name: String,
    chapter: i64,
    translation_code: String,
}

/// A pane's chapter, how it got there and where the reader has been in it.
#[derive(Default)]
struct State {
    /// The last `get_reading` and `get_compare` answers, so Compare going
    /// on or off draws the chapter again without reading it.
    last_dto: Option<Rc<Reading>>,
    last_compare: Option<Rc<Compare>>,
    reading: Option<Here>,
    /// The verse numbers of each column on screen, by translation.
    columns: Vec<(String, Vec<i64>)>,
    /// Bumped by each load: an answer to an older one is dropped.
    load_token: u64,
    /// The chapter (book, chapter) the newest load asked for, until it's
    /// answered or fails: a translation switched meanwhile reads it in
    /// the new one, as the same load, so the jump or step it's for still
    /// lands, and is recorded (C2-02).
    asked: Option<(i64, i64)>,
    /// A note's Edit waiting for its jump ([`open`]): claimed by the Edit,
    /// cancelled by any newer navigation ([`load_sent`]) and by any note
    /// editor opening ([`note_editor_opening`]), so a late jump never opens
    /// a note over something newer.
    edit: Generation,
    /// Compare's own fetch ([`on_compare_change`]), apart from a load's:
    /// claimed by each change of Compare (on, off, another translation),
    /// so only the newest one's answer, or failure, shows.
    compare: Generation,
    /// A position is being saved; the newest one waiting goes next.
    saving: bool,
    waiting: Option<Position>,
    /// The last position asked to be saved (a place unchanged since isn't
    /// sent again), and the last the backend saved.
    last_sent: Option<Position>,
    saved: Option<Position>,
    /// The reader is scrolling: where they are is saved once they stop.
    place_timer: Option<i32>,
    /// The chapter Try again reads again.
    retry: Option<(i64, i64)>,
    /// The translation is being switched for a jump that follows at once
    /// ([`switch_for_jump`]): the chapter on screen isn't read again in it.
    jump_follows: bool,
    history: History,
}

thread_local! {
    static STATE: RefCell<PerPane<State>> = RefCell::new(PerPane::default());
    /// Each bundled translation's credit, from boot, for About.
    static CREDITS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// What each pane's scrolling timer calls ([`save_place_soon`]): one
    /// callback a pane, made once (`dom::kept_callback`).
    static SAVE_PLACE: PerPane<js_sys::Function> =
        PerPane::new(|pane| dom::kept_callback(move || save_place(pane)));
}

/// A pane's state, never held while the page is told anything (its
/// listeners may ask the reader again).
fn with<R>(pane: PaneId, f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(|panes| f(&mut panes[pane]))
}

// ---- The page's elements and settings.

fn translation_select() -> Option<HtmlSelectElement> {
    bridge::document()?
        .get_element_by_id("translation-select")?
        .dyn_into::<HtmlSelectElement>()
        .ok()
}

/// A translation's name, as the translation button's record has it (its
/// code in capitals if it isn't there).
pub fn translation_name(code: &str) -> String {
    let options = translation_select().map(|select| select.options());
    options
        .and_then(|options| {
            (0..options.length())
                .filter_map(|i| options.item(i))
                .filter_map(|o| o.dyn_into::<HtmlOptionElement>().ok())
                .find(|o| o.value() == code)
        })
        .and_then(|o| o.text_content())
        .unwrap_or_else(|| code.to_uppercase())
}

/// Comparing translations (Compare on, with a translation rather than the
/// notes): the comparison is fetched.
fn is_comparing() -> bool {
    aa::beside() == Beside::Translations
}

fn compare_codes() -> Vec<String> {
    text::compare_codes(&bridge::translation_code(), aa::partner().as_deref())
}

fn compare_layout() -> String {
    text::compare_layout(aa::beside(), &compare_codes())
}

/// The notice above the chapter: `message`, or none.
fn show_notice(pane: PaneId, message: &str) {
    if let Some(notice) = pane.notice() {
        notice.set_text_content(Some(message));
        notice.set_hidden(message.is_empty());
    }
}

/// Each disclosure as last opened or closed, for every chapter (closed
/// unless saved open, or if local storage can't be read).
fn opened() -> Opened {
    let storage = web_sys::window().and_then(|w| w.local_storage().ok().flatten());
    let open = |kind: Disclosure| {
        storage
            .as_ref()
            .and_then(|s| s.get_item(kind.key()).ok().flatten())
            .is_some_and(|value| value == "open")
    };
    Opened {
        footnotes: open(Disclosure::Footnotes),
        xrefs: open(Disclosure::Xrefs),
    }
}

/// Gives the Cross references list its rows if it has none: a list drawn
/// closed is drawn without them (`reader_text::xref_summary`). From the
/// chapter on screen, which the pane is always drawn from
/// (`render_current`).
fn fill_xref_rows(pane: PaneId, list: &Element) {
    if list.first_child().is_none()
        && let Some(dto) = with(pane, |s| s.last_dto.clone())
    {
        list.set_inner_html(&text::xref_rows(&dto));
    }
}

/// Opens or closes every disclosure of that kind on screen (both columns'
/// footnotes while comparing), and remembers it; a Cross references list
/// opened is filled ([`fill_xref_rows`]).
fn set_disclosure(kind: Disclosure, open: bool) {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(kind.key(), if open { "open" } else { "closed" });
    }
    let Some(document) = bridge::document() else {
        return;
    };
    let selector = format!(r#".disclosure-toggle[data-disclosure="{}"]"#, kind.name());
    let Ok(toggles) = document.query_selector_all(&selector) else {
        return;
    };
    for toggle in dom::elements(&toggles) {
        let _ = toggle.set_attribute("aria-expanded", &open.to_string());
        if let Some(body) = toggle
            .get_attribute("aria-controls")
            .and_then(|id| by_id(&id))
        {
            if open
                && kind == Disclosure::Xrefs
                && let Some(pane) = pane::of(&body)
            {
                fill_xref_rows(pane, &body);
            }
            body.set_hidden(!open);
        }
    }
}

// ---- What the header says.

/// The translation button: the translation being read by its code, named
/// in full (the active pane's); and each pane's bar's, its own.
fn show_translation() {
    let code = bridge::translation_code();
    let (label, name) = text::translation_button(&code, &translation_name(&code));
    if let Some(toggle) = by_id("translation-toggle") {
        toggle.set_text_content(Some(&label));
        let _ = toggle.set_attribute("aria-label", &name);
    }
    for pane in pane::shown().filter(|_| pane::split()) {
        let code = pane::code(pane);
        let (label, name) = text::translation_button(&code, &translation_name(&code));
        pane::show_translation(pane, &label, &name);
    }
}

/// The chapter on screen as the reference button says it: the chapter, in
/// each translation on screen.
fn reference_of(here: &Here, compare: Option<&Compare>) -> text::Reference {
    let codes: Vec<&str> = match compare {
        Some(compare) => compare.columns.iter().map(|c| c.code.as_str()).collect(),
        None => vec![here.translation_code.as_str()],
    };
    text::reference(&here.book_name, here.chapter, &codes, translation_name)
}

/// `pane`'s bar and tab say where the reader is in it, as the reference
/// button does (two panes open).
fn show_bar_reference(pane: PaneId, here: &Here, compare: Option<&Compare>) {
    pane::show_reference(pane, &reference_of(here, compare));
}

/// The reference button and the window title (which the backend copies to
/// the window) say where the reader is in the active pane (D13): the
/// chapter, in each translation on screen.
fn update_reference(here: &Here, compare: Option<&Compare>) {
    let reference = reference_of(here, compare);
    if let Some(ref_text) = by_id("ref-text") {
        // The translations apart, which a narrow window leaves out (the
        // translation button says it).
        ref_text.set_inner_html(&text::reference_markup(&reference));
    }
    if let Some(button) = by_id("ref-btn") {
        let _ = button.set_attribute("aria-label", &reference.label);
    }
    if let Some(document) = bridge::document() {
        document.set_title(&reference.title);
    }
}

/// Boot's translations: the translation button's record (the one being read
/// selected), the button, and About's credits.
fn show_translations(home: &Home) {
    if let Some(select) = translation_select() {
        select.set_inner_html(&text::translation_options(
            &home.translations,
            &home.current_translation_code,
        ));
    }
    show_translation();
    let credits: Vec<String> = home.translations.iter().map(|t| t.credit.clone()).collect();
    if !credits.is_empty() {
        crate::about::show_credits(credits.clone());
    }
    CREDITS.set(credits);
}

/// Each bundled translation's credit, in order, from boot (none before):
/// About takes them as it mounts.
pub fn credits() -> Vec<String> {
    CREDITS.with_borrow(Clone::clone)
}

// ---- Putting a chapter on screen.

/// Puts the last reading (and the comparison, while comparing and it's
/// this chapter's in the translations wanted) on screen, then tells the
/// selection card (`card::shown`).
fn render_current(pane: PaneId) {
    let Some(root) = pane.root() else {
        return;
    };
    let (dto, compare, here) = with(pane, |s| {
        (
            s.last_dto.clone(),
            s.last_compare.clone(),
            s.reading.clone(),
        )
    });
    let (Some(dto), Some(here)) = (dto, here) else {
        return;
    };
    let codes = compare_codes();
    let compare = compare.filter(|c| {
        is_comparing()
            && text::compare_is_current(c, dto.current_book_id, dto.current_chapter, &codes)
    });
    let compare_first = with(pane, |s| {
        s.last_compare
            .as_ref()
            .and_then(|c| c.columns.first())
            .map(|c| c.code.clone())
    });

    // The columns on screen: the translation being read, then (while
    // comparing) each column, which replaces it where it's the same one.
    let numbers = |verses: &[Verse]| verses.iter().map(|v| v.number).collect::<Vec<_>>();
    let mut columns = vec![(here.translation_code.clone(), numbers(&dto.current_verses))];
    if let Some(compare) = &compare {
        for column in &compare.columns {
            let entry = (column.code.clone(), numbers(&column.verses));
            match columns.iter_mut().find(|(code, _)| *code == column.code) {
                Some(found) => *found = entry,
                None => columns.push(entry),
            }
        }
    }

    let name = translation_name(&here.translation_code);
    let beside = match (&compare, aa::beside()) {
        (Some(compare), _) => text::Beside::Translations(compare),
        (None, Beside::Notes) => text::Beside::Notes { name: &name },
        (None, _) => text::Beside::Nothing,
    };
    // The faded neighbors give way while two panes share the window (D7).
    let neighbours = if pane::split() {
        text::Neighbours::Omit
    } else {
        text::Neighbours::Draw
    };
    root.set_inner_html(&text::reading_pane(
        &dto,
        beside,
        &here.translation_code,
        opened(),
        pane.ids(),
        neighbours,
    ));
    // The grid's column count goes through the CSSOM, not a style attribute
    // in the markup, which the Content Security Policy would block.
    if let Some(grid) = root
        .query_selector(".compare-grid")
        .ok()
        .flatten()
        .and_then(|el| el.dyn_into::<HtmlElement>().ok())
    {
        let count = grid.get_attribute("data-columns").unwrap_or_default();
        let _ = grid.style().set_property("--compare-columns", &count);
    }
    with(pane, |s| s.columns = columns);
    show_bar_reference(pane, &here, compare.as_deref());
    // The top bar and the window's title say the active pane's (D13).
    if pane == pane::active() {
        update_reference(&here, compare.as_deref());
    }
    crate::picker::show_current(pane, here.book_id, here.chapter);
    show_nav(pane);
    card::shown(pane, dto, compare, &here.translation_code, compare_first);
}

/// Tells the bar's Back, Forward, Recent places, ‹ and › (`nav`) where the
/// reader is and where each would go.
fn show_nav(pane: PaneId) {
    // The chapters beside, as the pane was drawn with them (and ← and →
    // turn to): named from the reading already in hand.
    let side = |side: Option<&crate::dto::ChapterSide>| {
        side.map(|s| format!("{} {}", s.book_name, s.chapter))
    };
    let facts = with(pane, |s| Facts {
        here: s.reading.as_ref().map(|r| (r.book_id, r.chapter)),
        back: s.history.toward(-1).cloned(),
        forward: s.history.toward(1).cloned(),
        prev: s.last_dto.as_ref().and_then(|d| side(d.prev.as_ref())),
        next: s.last_dto.as_ref().and_then(|d| side(d.next.as_ref())),
    });
    pane::show_steps(pane, facts.prev.as_deref(), facts.next.as_deref());
    // The bar's say the active pane's (D11).
    if pane == pane::active() {
        nav::show(facts);
    }
}

/// `place`, read in `translation_code`, as a recent place.
fn recent(place: &Place, translation_code: &str) -> RecentPlace {
    RecentPlace {
        book_id: place.book_id,
        chapter: place.chapter,
        start: place.start,
        end: place.end,
        translation: translation_code.to_owned(),
    }
}

/// The place on screen, as the reader leaves it (after
/// [`remember_leaving`]), as a recent place.
fn recent_here(pane: PaneId) -> Option<RecentPlace> {
    with(pane, |s| {
        let code = &s.reading.as_ref()?.translation_code;
        s.history.here().map(|place| recent(place, code))
    })
}

/// A jump or a step has landed on `place`: it and the place left are the
/// newest recent places, in that order.
fn record_recent(pane: PaneId, leaving: Option<RecentPlace>, place: &Place) {
    if let Some(leaving) = leaving {
        nav::record(leaving);
    }
    if let Some(code) = with(pane, |s| {
        s.reading.as_ref().map(|r| r.translation_code.clone())
    }) {
        nav::record(recent(place, &code));
    }
}

/// The comparison of the chapter in the translations Compare shows now, or
/// `null` once the failure is logged: a promise, its command sent now.
fn load_compare(book_id: i64, chapter: i64) -> js_sys::Promise {
    let codes = compare_codes();
    let sent = ipc::args(&CompareArgs {
        book_id,
        chapter,
        translation_codes: &codes,
    })
    .and_then(|args| ipc::send("get_compare", args));
    wasm_bindgen_futures::future_to_promise(async move {
        let answer = match sent {
            Ok(promise) => JsFuture::from(promise).await,
            Err(e) => Err(e),
        };
        Ok(answer.unwrap_or_else(|e| {
            ipc::log_error("Failed to load comparison", &e);
            JsValue::NULL
        }))
    })
}

/// Reads the chapter (in the translation being read, and the comparison
/// beside it while comparing) and puts it on screen: the single path every
/// chapter takes. The selection goes at once (the reader has asked for
/// something else), and the commands are sent before this returns. A
/// chapter the Bible doesn't have changes nothing; one that couldn't be
/// read says so, unless a newer load has begun since.
fn load_reading(pane: PaneId, book_id: i64, chapter: i64) -> Load {
    load_sent(pane, book_id, chapter, None)
}

/// Boot's `get_reading` for its chapter, sent before this module loaded:
/// on its way, or already answered (when the page has had the answer by the
/// time it asks the reader).
pub enum Sent {
    Asked(js_sys::Promise),
    Answered(JsValue),
}

/// What a load's answers came to: a chapter on screen or not, or (Compare
/// changed on the way) a reason to ask again.
enum Outcome {
    Done(bool),
    Again,
}

/// [`load_reading`], with its `get_reading` already sent, if `sent` is (boot
/// asks for its chapter before this module has loaded). An answer already
/// in is put on screen now, within the request.
fn load_sent(pane: PaneId, book_id: i64, chapter: i64, sent: Option<Sent>) -> Load {
    // A navigation: a note's Edit still waiting for its own jump is let go
    // ([`open`]).
    with(pane, |s| s.edit.cancel());
    send_load(pane, book_id, chapter, sent)
}

/// [`load_sent`]'s work: the commands sent, and the answers put on screen.
/// Compare changing on the way asks again through here, as the same load
/// (a note's Edit waiting for it still opens after).
fn send_load(pane: PaneId, book_id: i64, chapter: i64, sent: Option<Sent>) -> Load {
    // The reader has asked for something else in the pane: a verse put back
    // where they were at launch stays put no longer.
    surfaces::let_go_of_place(pane);
    let token = with(pane, |s| {
        s.load_token += 1;
        s.asked = Some((book_id, chapter));
        // A step's Try again is this load's now (see `retry`); a step's
        // own load begins after it's taken.
        s.history.forget_failed();
        s.load_token
    });
    card::loading(pane);
    let translation_code = pane::code(pane);
    let layout = compare_layout();
    let reading = match sent {
        Some(sent) => Ok(sent),
        None => ipc::args(&ReadingArgs {
            book_id,
            chapter,
            translation_code: &translation_code,
        })
        .and_then(|args| ipc::send("get_reading", args))
        .map(Sent::Asked),
    };
    let compare = is_comparing().then(|| load_compare(book_id, chapter));
    if let (Ok(Sent::Answered(dto_raw)), None) = (&reading, &compare) {
        let answer = answered(
            pane,
            token,
            (book_id, chapter),
            translation_code,
            &layout,
            dto_raw.clone(),
            JsValue::NULL,
        );
        return match answer {
            Outcome::Done(loaded) => Box::pin(async move { loaded }),
            Outcome::Again => send_load(pane, book_id, chapter, None),
        };
    }
    Box::pin(async move {
        let answer = match reading {
            Ok(Sent::Asked(promise)) => JsFuture::from(promise).await,
            Ok(Sent::Answered(dto)) => Ok(dto),
            Err(e) => Err(e),
        };
        let dto_raw = match answer {
            Ok(dto) => dto,
            Err(e) => {
                failed(pane, token, &e, book_id, chapter);
                return false;
            }
        };
        let compare_raw = match compare {
            Some(promise) => JsFuture::from(promise).await.unwrap_or(JsValue::NULL),
            None => JsValue::NULL,
        };
        let answer = answered(
            pane,
            token,
            (book_id, chapter),
            translation_code,
            &layout,
            dto_raw,
            compare_raw,
        );
        match answer {
            Outcome::Done(loaded) => loaded,
            Outcome::Again => send_load(pane, book_id, chapter, None).await,
        }
    })
}

/// A load's answers (the reading, and the comparison or `null`), sent for
/// `pane` with `token` in the translation `translation_code` for the layout
/// `layout`:
/// put on screen, and the position saved, unless a newer load has begun,
/// the chapter isn't there, or the translation or Compare changed on the
/// way.
fn answered(
    pane: PaneId,
    token: u64,
    (book_id, chapter): (i64, i64),
    translation_code: String,
    layout: &str,
    dto_raw: JsValue,
    compare_raw: JsValue,
) -> Outcome {
    // A newer load has begun: let it win.
    if with(pane, |s| s.load_token) != token {
        return Outcome::Done(false);
    }
    // The translation was switched (`Request::Translation`), or Compare
    // (or its translations), on the way: what came back is for the old
    // one, so it's asked for again.
    if pane::code(pane) != translation_code || compare_layout() != layout {
        return Outcome::Again;
    }
    with(pane, |s| s.asked = None);
    if dto_raw.is_null() || dto_raw.is_undefined() {
        return Outcome::Done(false);
    }
    let dto: Reading = match serde_wasm_bindgen::from_value(dto_raw.clone()) {
        Ok(dto) => dto,
        Err(e) => {
            failed(pane, token, &JsValue::from(e.to_string()), book_id, chapter);
            return Outcome::Done(false);
        }
    };
    let compare = if compare_raw.is_null() {
        None
    } else {
        match serde_wasm_bindgen::from_value::<Compare>(compare_raw.clone()) {
            Ok(compare) => Some(compare),
            Err(e) => {
                ipc::log_error("Failed to load comparison", &JsValue::from(e.to_string()));
                None
            }
        }
    };
    let here = Here {
        book_id: dto.current_book_id,
        book_name: dto.current_book_name.clone(),
        chapter: dto.current_chapter,
        translation_code: translation_code.clone(),
    };
    let compared = compare.is_some();
    with(pane, |s| {
        s.last_dto = Some(Rc::new(dto));
        if let Some(compare) = compare {
            s.last_compare = Some(Rc::new(compare));
        }
        s.reading = Some(here.clone());
    });
    show_notice(
        pane,
        if is_comparing() && !compared {
            text::COMPARE_FAILED
        } else {
            ""
        },
    );
    render_current(pane);
    save_position(
        pane,
        Position {
            book_id: here.book_id,
            chapter: here.chapter,
            translation_code,
            verse: None,
            offset: None,
        },
    );
    // Then where in it the reader is, once the page is still: a chapter
    // turned by ← or → is shown at the scroll position the last one had.
    save_place_soon(pane);
    Outcome::Done(true)
}

/// A load that failed: logged, and, unless a newer one has begun, said.
fn failed(pane: PaneId, token: u64, error: &JsValue, book_id: i64, chapter: i64) {
    ipc::log_error("Failed to load reading pane", error);
    if with(pane, |s| s.load_token) == token {
        with(pane, |s| s.asked = None);
        show_failure(pane, &ipc::error_text(error), book_id, chapter);
    }
}

/// A chapter that couldn't be read: the notice says so until a chapter is
/// put on screen, with a Try again that asks for it once more (and starts
/// the history, if it was the first chapter). The chapter on screen, if
/// any, stays, and a toast says it too, however far down the reader is.
fn show_failure(pane: PaneId, message: &str, book_id: i64, chapter: i64) {
    show_notice(pane, message);
    with(pane, |s| s.retry = Some((book_id, chapter)));
    if let (Some(document), Some(notice)) = (bridge::document(), pane.notice())
        && let Ok(retry) = document.create_element("button")
    {
        let _ = retry.set_attribute("type", "button");
        retry.set_class_name("note-link reading-retry");
        retry.set_text_content(Some("Try again"));
        let _ = notice.append_with_str_1(" ");
        let _ = notice.append_with_node_1(&retry);
    }
    if with(pane, |s| s.last_dto.is_some()) {
        crate::page::set_data_status(message, true);
    }
}

/// Try again: the chapter that couldn't be read.
fn retry(pane: PaneId) {
    let Some((book_id, chapter)) = with(pane, |s| s.retry) else {
        return;
    };
    // After a step back or forward that couldn't be read, the step again.
    if with(pane, |s| s.history.can_retry(book_id, chapter)) {
        remember_leaving(pane);
        let leaving = recent_here(pane);
        if let Some(going) = with(pane, |s| s.history.begin_retry(book_id, chapter)) {
            spawn(open_step(pane, going, leaving));
        }
        return;
    }
    let load = load_reading(pane, book_id, chapter);
    spawn_local(async move {
        if load.await && !with(pane, |s| s.history.started()) {
            start_history(pane);
        }
    });
}

/// Saves the chapter put on screen as the position the next launch opens
/// at, and where in it the reader is (see [`save_place`]). One save is on
/// its way at a time; a newer one waits, replacing any that waited before
/// it, so they reach the backend in the order they were made and the last
/// one wins. The first is sent now.
fn save_position(pane: PaneId, position: Position) {
    // The place the backend keeps is the first pane's, and the second's is
    // never sent (S15): this computer keeps it, for the next launch (S9).
    if pane == PaneId::Second {
        pane::keep_place(&SecondPlace {
            book_id: position.book_id,
            chapter: position.chapter,
            translation: position.translation_code.clone(),
            verse: position.verse,
            offset: position.offset,
        });
        with(pane, |s| s.last_sent = Some(position));
        return;
    }
    let first = with(pane, |s| {
        s.last_sent = Some(position.clone());
        s.waiting = Some(position);
        if s.saving {
            None
        } else {
            s.saving = true;
            s.waiting.take()
        }
    });
    let Some(first) = first else {
        return;
    };
    let mut sending = first;
    let mut sent = send_position(&sending);
    spawn_local(async move {
        loop {
            let answer = match sent {
                Ok(promise) => JsFuture::from(promise).await,
                Err(e) => Err(e),
            };
            match answer {
                Ok(_) => with(pane, |s| s.saved = Some(sending)),
                // The next chapter shown, or scroll, saves again.
                Err(e) => {
                    ipc::log_error("Failed to save the reading position", &e);
                    with(pane, |s| s.last_sent = None);
                }
            }
            let Some(next) = with(pane, |s| s.waiting.take()) else {
                break;
            };
            sending = next;
            sent = send_position(&sending);
        }
        with(pane, |s| s.saving = false);
    });
}

fn send_position(position: &Position) -> Result<js_sys::Promise, JsValue> {
    ipc::args(position).and_then(|args| ipc::send("save_reading_position", args))
}

/// How long the page is left still after a scroll before where the reader
/// is is saved: once they've stopped, not on every step of the way.
const PLACE_SAVE_MS: i32 = 400;

/// Saves where in the chapter on screen the reader is, the verse at the
/// top of the reading pane (`surfaces::top_verse`, the place the reading
/// place logic keeps), if it's changed since the last save: so the next
/// launch opens there. At the chapter's heading it's the chapter's top.
fn save_place(pane: PaneId) {
    with(pane, |s| {
        if let Some(timer) = s.place_timer.take()
            && let Some(window) = web_sys::window()
        {
            window.clear_timeout_with_handle(timer);
        }
    });
    // Nothing to say while the reader's own pane isn't there (the page's
    // plain-text fallback has taken its place).
    let Some(here) = with(pane, |s| s.reading.clone()).filter(|_| pane.inner().is_some()) else {
        return;
    };
    let top = surfaces::top_verse(pane, &here.translation_code);
    let position = Position {
        book_id: here.book_id,
        chapter: here.chapter,
        translation_code: here.translation_code,
        verse: top.map(|(verse, _)| verse),
        offset: top.map(|(_, offset)| text::rounded_offset(offset)),
    };
    if with(pane, |s| s.last_sent.as_ref() == Some(&position)) {
        return;
    }
    save_position(pane, position);
}

/// [`save_place`] for every pane on screen.
fn save_places() {
    for pane in pane::shown() {
        save_place(pane);
    }
}

/// A scroll: where the reader is is saved once the page has been still for
/// [`PLACE_SAVE_MS`].
fn save_place_soon(pane: PaneId) {
    let timer = SAVE_PLACE.with(|save| dom::set_timeout(&save[pane], PLACE_SAVE_MS));
    let replaced = with(pane, |s| std::mem::replace(&mut s.place_timer, timer));
    dom::clear_timeout(replaced);
}

/// Boot's chapter is on screen (the first pane's, or the second's reopened
/// beside it): the verse the reader was at goes back to the top of the
/// pane's view, `offset` px below the bar, and is kept there as the page
/// settles (`surfaces::hold_place`, the way the reading place logic puts
/// one back). A verse the translation doesn't number
/// leaves the chapter at its top.
fn reopen_at(pane: PaneId, verse: i64, offset: Option<f64>) {
    let Some(here) = with(pane, |s| s.reading.clone()) else {
        return;
    };
    if let Some(mark) = pane.query(&text::verse_selector(&here.translation_code, verse)) {
        surfaces::hold_place(pane, mark, offset.unwrap_or(0.0));
        save_place(pane);
    }
}

/// `window.gospelGetterPlace.saved()`: the last position the backend saved
/// (`save_reading_position`'s arguments), or `null`: the first pane's, the
/// one the backend keeps. For the GUI checks, which wait for it before
/// closing the app.
fn export_place() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let global = js_sys::Object::new();
    let saved = Closure::<dyn Fn() -> JsValue>::new(|| {
        with(PaneId::First, |s| s.saved.clone())
            .and_then(|saved| ipc::args(&saved).ok())
            .unwrap_or(JsValue::NULL)
    });
    let _ = js_sys::Reflect::set(&global, &JsValue::from_str("saved"), saved.as_ref());
    let _ = js_sys::Reflect::set(&window, &JsValue::from_str("gospelGetterPlace"), &global);
    saved.forget();
}

/// Compare went on or off, or what it compares with changed (or reading
/// mode, with Compare's Notes chosen; `aa`): every pane draws its chapter
/// in the new layout ([`on_compare_change`]).
pub fn compare_changed() {
    for pane in pane::shown() {
        on_compare_change(pane);
    }
}

/// A note editor is opening (`card`): a note's Edit still waiting for its
/// jump, in any pane, is let go ([`open`]).
pub fn note_editor_opening() {
    for pane in pane::shown() {
        with(pane, |s| s.edit.cancel());
    }
}

/// Compare went on or off, or what it compares with changed: the chapter on
/// screen in the new layout, fetching the comparison only if it isn't the
/// one there already. Its answer (or its failure's notice) shows only while
/// it's still the comparison asked for: no newer change of Compare, no load
/// begun since, and the same chapter, translations and layout on screen.
fn on_compare_change(pane: PaneId) {
    // Whatever Compare is now, a fetch for what it was is no longer wanted.
    let claim = with(pane, |s| s.compare.claim());
    let Some(dto) = with(pane, |s| s.last_dto.clone()) else {
        return;
    };
    if !is_comparing() {
        show_notice(pane, "");
        render_current(pane);
        return;
    }
    let codes = compare_codes();
    let current = with(pane, |s| {
        s.last_compare.as_ref().is_some_and(|c| {
            text::compare_is_current(c, dto.current_book_id, dto.current_chapter, &codes)
        })
    });
    if current {
        render_current(pane);
        return;
    }
    let token = with(pane, |s| s.load_token);
    let layout = compare_layout();
    let (book_id, chapter) = (dto.current_book_id, dto.current_chapter);
    let compare = load_compare(book_id, chapter);
    spawn_local(async move {
        let compare_raw = JsFuture::from(compare).await.unwrap_or(JsValue::NULL);
        let asked = with(pane, |s| {
            s.compare.holds(claim)
                && s.load_token == token
                && s.last_dto
                    .as_ref()
                    .is_some_and(|d| d.current_book_id == book_id && d.current_chapter == chapter)
        });
        if !asked || compare_codes() != codes || compare_layout() != layout {
            return;
        }
        let compare = (!compare_raw.is_null())
            .then(|| serde_wasm_bindgen::from_value::<Compare>(compare_raw.clone()).ok())
            .flatten();
        match compare {
            Some(compare) => with(pane, |s| s.last_compare = Some(Rc::new(compare))),
            None => show_notice(pane, text::COMPARE_FAILED),
        }
        render_current(pane);
    });
}

// ---- Passages and the history.

/// Opens a chapter and selects `start`–`end` in it, in the translation
/// being read (the selection card selects them, `card::select_passage`), for
/// every jump and for going back and forward to one. If that translation
/// doesn't number them, the chapter opens and the notice says so, rather
/// than selecting other verses. Whether it opened, and what the page then
/// had selected.
fn open_passage(
    pane: PaneId,
    book_id: i64,
    chapter: i64,
    start: Option<i64>,
    end: Option<i64>,
) -> OpenedPassage {
    let load = load_reading(pane, book_id, chapter);
    Box::pin(async move {
        let loaded = load.await;
        let (true, Some(start)) = (loaded, start) else {
            return (loaded, None);
        };
        let end = end.unwrap_or(start);
        let Some(here) = with(pane, |s| s.reading.clone()) else {
            return (loaded, None);
        };
        let numbers = with(pane, |s| {
            s.columns
                .iter()
                .find(|(code, _)| *code == here.translation_code)
                .map(|(_, numbers)| numbers.clone())
                .unwrap_or_default()
        });
        // The card is the active pane's: a pane left on the way keeps no
        // selection.
        if numbers.contains(&start) && numbers.contains(&end) {
            let selected = (pane == pane::active())
                .then(|| card::select_passage(&here.translation_code, start, end))
                .flatten();
            return (loaded, selected);
        }
        // Past the chapter's end in this translation, or a verse it leaves
        // out in mid-chapter (as the BSB does Matthew 17:21).
        let last = numbers.last().copied().unwrap_or(0);
        show_notice(
            pane,
            &text::not_numbered(
                &here.book_name,
                chapter,
                start,
                end,
                &translation_name(&here.translation_code),
                last,
            ),
        );
        (loaded, None)
    })
}

/// Starts the history over at the chapter being read.
fn start_history(pane: PaneId) {
    with(pane, |s| {
        if let Some(here) = &s.reading {
            let (book_id, chapter) = (here.book_id, here.chapter);
            s.history.start(book_id, chapter);
        }
    });
    show_nav(pane);
}

/// The place on screen, brought up to date as the reader leaves it.
fn remember_leaving(pane: PaneId) {
    let y = pane.scroller().y().unwrap_or(0.0);
    with(pane, |s| {
        let here = s.reading.as_ref().map(|r| (r.book_id, r.chapter));
        s.history.remember_leaving(here, y);
    });
}

/// A jump: opens the chapter (selecting `start`–`end`, if given and
/// numbered; from the top, with `top`) and records it in the history.
fn jump_to(
    pane: PaneId,
    book_id: i64,
    chapter: i64,
    start: Option<i64>,
    end: Option<i64>,
    top: bool,
) -> Load {
    remember_leaving(pane);
    let leaving = recent_here(pane);
    with(pane, |s| s.history.begin_jump());
    let open = open_passage(pane, book_id, chapter, start, end);
    Box::pin(async move {
        let (loaded, selected) = open.await;
        let place = if loaded {
            if top {
                pane.scroller().scroll_to(0.0);
            }
            let landed =
                selected.filter(|&verses| start.is_some() && Some(verses) == start.zip(end));
            with(pane, |s| {
                s.reading.as_ref().map(|here| Place {
                    start: landed.map(|v| v.0),
                    end: landed.map(|v| v.1),
                    ..Place::chapter(here.book_id, here.chapter)
                })
            })
        } else {
            None
        };
        if let Some(place) = &place {
            record_recent(pane, leaving, place);
        }
        with(pane, |s| s.history.end_jump(place));
        show_nav(pane);
        loaded
    })
}

/// Back (-1) or forward (1): the place, its verses selected, and the page
/// scrolled where the reader left it; at either end, a toast says there's
/// nothing there. A newer step or jump takes over from one on its way.
fn go_history(pane: PaneId, step: i32) -> Load {
    if !with(pane, |s| s.history.can_go(step)) {
        crate::page::set_data_status(text::history_end(step), false);
        return Box::pin(async { false });
    }
    remember_leaving(pane);
    let leaving = recent_here(pane);
    let Some(going) = with(pane, |s| s.history.begin_go(step)) else {
        return Box::pin(async { false });
    };
    show_nav(pane);
    open_step(pane, going, leaving)
}

/// Opens the place a step back or forward (with its token) is going to.
/// Only once it's read is it the place on screen; one that can't be read
/// leaves the history as it was (`History::end_go`). Read, it and the place
/// left (`leaving`) are the newest recent places.
fn open_step(pane: PaneId, (token, place): (u64, Place), leaving: Option<RecentPlace>) -> Load {
    let open = open_passage(pane, place.book_id, place.chapter, place.start, place.end);
    Box::pin(async move {
        let (loaded, _) = open.await;
        if !with(pane, |s| s.history.current(token)) {
            return false;
        }
        if loaded {
            let scroller = pane.scroller();
            if let Some(y) = place.scroll_y {
                scroller.scroll_to(y);
            } else if place.start.is_none() {
                scroller.scroll_to(0.0);
            }
            record_recent(pane, leaving, &place);
        }
        with(pane, |s| s.history.end_go(token, loaded));
        show_nav(pane);
        loaded
    })
}

/// Runs `load` to its end, its answer dropped.
fn spawn(load: Load) {
    spawn_local(async move {
        let _ = load.await;
    });
}

/// A passage to open: a chapter, and its verses if not all of it (a
/// Library row or a search result, opened through [`open`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenPassage {
    pub book_id: i64,
    pub chapter: i64,
    pub verse_start: Option<i64>,
    pub verse_end: Option<i64>,
}

/// Opens a chapter as a jump (recorded in the history): the welcome's
/// places to start, a chapter chosen in the picker or Random chapter's;
/// from the top, with `top`.
pub fn jump(pane: PaneId, book_id: i64, chapter: i64, top: bool) {
    spawn(jump_to(pane, book_id, chapter, None, None, top));
}

/// A Library row or a search result opens its passage (the Library or the
/// search box already closed), as a jump, its verses selected; with a
/// `note` (a note's Edit, as `list_all_notes` gave it), the selection card
/// then opens the note editor on it (`card::edit_note`), once the jump has
/// put the passage on screen, and only if nothing newer has happened on
/// the way: a jump that failed or was overtaken, another navigation, or
/// any note editor opening meanwhile (even one closed again) lets the Edit
/// go, so it never opens over what the reader is doing now.
pub fn open(pane: PaneId, passage: &OpenPassage, note: JsValue) {
    let load = jump_to(
        pane,
        passage.book_id,
        passage.chapter,
        passage.verse_start,
        passage.verse_end,
        false,
    );
    if note.is_null() || note.is_undefined() {
        spawn(load);
        return;
    }
    // After the jump's own load has begun (which cancels any Edit before
    // this one).
    let edit = with(pane, |s| s.edit.claim());
    spawn_local(async move {
        let loaded = load.await;
        if loaded && with(pane, |s| s.edit.holds(edit) && s.reading.is_some()) {
            card::edit_note(&note);
        }
    });
}

/// How many chapter loads have begun, in every pane: a number that moves
/// on with every navigation, so a late answer can tell whether the reader
/// has gone anywhere since it was asked for (search's Enter, waiting for
/// its search). Read only.
pub fn loads() -> u64 {
    pane::shown().map(|pane| with(pane, |s| s.load_token)).sum()
}

/// What the palette's commands need of the reader: the chapters (as the
/// active pane says, which ← and → go by) and books around the one on
/// screen there, and its history either way.
pub fn palette_facts(state: &mut PaletteState) {
    let pane = pane::active();
    let inner = pane.inner();
    let has = |name| {
        inner
            .as_ref()
            .is_some_and(|inner| inner.has_attribute(name))
    };
    state.next_chapter = has("data-next-book");
    state.previous_chapter = has("data-prev-book");
    with(pane, |s| {
        let dto = s.last_dto.as_ref();
        state.next_book = dto.is_some_and(|d| d.next_book.is_some());
        state.previous_book = dto.is_some_and(|d| d.prev_book.is_some());
        state.back = s.history.can_go(-1);
        state.forward = s.history.can_go(1);
    });
}

// ---- What boot, the keys, the clicks and the page's glue ask.

/// What the keyboard, the selection card's Go to, an import, the
/// translation changing and the palette ask of the reader ([`ask`]).
#[derive(Debug)]
pub enum Request {
    /// Boot's ([`start`]): the translations and the chapter the reader was
    /// on, then the history from there.
    Start { home: Home },
    /// A chapter, not a jump (← and →, an import's position): in
    /// `translation_code`, if given; from the top, with `top`.
    Load {
        book_id: i64,
        chapter: i64,
        translation_code: Option<String>,
        top: bool,
    },
    /// `[` and `]`: chapter 1 of the book before or after, from the top.
    Book { which: Which },
    /// The translation changed: the chapter on screen again, in it.
    Translation,
    /// A jump, recorded in the history (the selection card's Go to).
    Jump {
        book_id: i64,
        chapter: i64,
        start: Option<i64>,
        end: Option<i64>,
    },
    /// Back (-1) or forward (1).
    History { step: i32 },
    /// Something for the notice above the chapter (after an import).
    Notice { message: String },
}

/// Boot's chapter: the translations from `home` (`get_home`'s answer), then
/// the chapter the reader was on, whose `get_reading` boot may already have
/// sent (`sent`: on its way, or answered, which is put on screen at once,
/// within this call), then the history from there, in the first pane (the
/// place the backend keeps is its). Its end is the load's.
pub fn start(home: Home, sent: Option<Sent>) -> Load {
    run(PaneId::First, Request::Start { home }, sent)
}

/// Switches to translation `code` for a jump asked for straight after
/// (Recent places): everything that hears of a new translation hears of it
/// (the Library's previews, Aa, the search box), but the chapter on screen
/// isn't read again in it first, as `Request::Load` with a translation
/// doesn't; the jump reads its own (C-04).
pub fn switch_for_jump(pane: PaneId, code: &str) {
    with(pane, |s| s.jump_follows = true);
    aa::choose_translation(code);
    with(pane, |s| s.jump_follows = false);
}

/// Starts what `request` asks for, now (anything it sends is sent within
/// this call); its end is the load's: whether a chapter was put on screen.
pub fn ask(pane: PaneId, request: Request) -> Load {
    run(pane, request, None)
}

/// Starts what `request` asks for, now; its end is the load's. `sent` is
/// boot's `get_reading`, already on its way, or answered.
fn run(pane: PaneId, request: Request, sent: Option<Sent>) -> Load {
    match request {
        Request::Start { home } => {
            show_translations(&home);
            let at = home
                .current_verse
                .map(|verse| (verse, home.current_verse_offset));
            let load = load_sent(pane, home.current_book_id, home.current_chapter, sent);
            Box::pin(async move {
                let loaded = load.await;
                if with(pane, |s| s.reading.is_some()) {
                    start_history(pane);
                }
                if let (true, Some((verse, offset))) = (loaded, at) {
                    reopen_at(pane, verse, offset);
                }
                loaded
            })
        }
        Request::Load {
            book_id,
            chapter,
            translation_code,
            top,
        } => {
            if let Some(code) = translation_code {
                pane::set_code(pane, &code);
                show_translation();
            }
            from_top(pane, load_reading(pane, book_id, chapter), top)
        }
        Request::Book { which } => {
            let found = with(pane, |s| {
                let dto = s.last_dto.as_ref()?;
                let here = s.reading.as_ref()?;
                let book = match which {
                    Which::Prev => dto.prev_book.as_ref(),
                    Which::Next => dto.next_book.as_ref(),
                };
                Some((book.map(|b| (b.book_id, b.chapter)), here.book_name.clone()))
            });
            match found {
                Some((Some((book_id, chapter)), _)) => {
                    from_top(pane, load_reading(pane, book_id, chapter), true)
                }
                Some((None, name)) => {
                    crate::page::set_data_status(&text::no_book(&name, which), false);
                    Box::pin(async { false })
                }
                None => Box::pin(async { false }),
            }
        }
        Request::Translation => {
            show_translation();
            // A jump follows, and reads its own chapter in it. Or a load is
            // on its way, which reads the chapter it asked for in it once
            // its answer is in (`answered`): that one, not the chapter
            // still on screen, and as its own jump or step (C2-02).
            if with(pane, |s| s.jump_follows || s.asked.is_some()) {
                return Box::pin(async { false });
            }
            // A pane made active, whose translation the record now says
            // (`pane::set_active`): it reads it already.
            let code = pane::code(pane);
            if pane::split()
                && with(pane, |s| {
                    s.reading
                        .as_ref()
                        .is_some_and(|r| r.translation_code == code)
                })
            {
                return Box::pin(async { false });
            }
            let inner = pane.inner();
            let number = |name| {
                inner
                    .as_ref()
                    .map_or(Some(1), |inner| data_number(inner, name))
            };
            match (number("data-current-book"), number("data-current-chapter")) {
                (Some(book_id), Some(chapter)) => load_reading(pane, book_id, chapter),
                _ => Box::pin(async { false }),
            }
        }
        Request::Jump {
            book_id,
            chapter,
            start,
            end,
        } => jump_to(pane, book_id, chapter, start, end, false),
        Request::History { step } => go_history(pane, step),
        Request::Notice { message } => {
            show_notice(pane, &message);
            Box::pin(async { true })
        }
    }
}

/// `load`, then the page scrolled to the top if it put a chapter there and
/// `top` asks for that (the chapter's end and `[`/`]`).
fn from_top(pane: PaneId, load: Load, top: bool) -> Load {
    if !top {
        return load;
    }
    Box::pin(async move {
        let loaded = load.await;
        if loaded {
            pane.scroller().scroll_to(0.0);
        }
        loaded
    })
}

// ---- The pane's own controls.

/// A click in a reading pane on one of the reader's controls, for that
/// pane. The click dispatcher (`clicks`, on the body too, registered first)
/// has already done what any click does there (closing the search box and
/// Copy as…) and left these alone.
fn on_click(event: &MouseEvent) {
    let Some(target) = event.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
        return;
    };
    let Some(pane) = pane::of(&target) else {
        return;
    };
    // The cross references' credit links: the page never opens a browser
    // itself; the backend opens the link's fixed page (its href, shown so
    // it reads as a link). A key press on a link arrives as a click too.
    if let Some(link) = closest(&target, "a.credit-link") {
        event.prevent_default();
        let which = link.get_attribute("data-link").unwrap_or_default();
        let sent = ipc::args(&CreditArgs { which: &which })
            .and_then(|args| ipc::send("open_credit_link", args));
        spawn_local(async move {
            let answer = match sent {
                Ok(promise) => JsFuture::from(promise).await,
                Err(e) => Err(e),
            };
            if let Err(e) = answer {
                crate::page::set_data_status(&ipc::error_text(&e), true);
            }
        });
        return;
    }
    // The chapter's end: the chapter before or after, from its top.
    if let Some(step) = closest(&target, ".chapter-step") {
        if let (Some(book_id), Some(chapter)) = (
            data_number(&step, "data-book"),
            data_number(&step, "data-chapter"),
        ) {
            spawn(from_top(pane, load_reading(pane, book_id, chapter), true));
        }
        return;
    }
    if let Some(toggle) = closest(&target, ".disclosure-toggle") {
        if let Some(kind) = toggle
            .get_attribute("data-disclosure")
            .and_then(|name| Disclosure::named(&name))
        {
            let open = toggle.get_attribute("aria-expanded").as_deref() != Some("true");
            set_disclosure(kind, open);
        }
        return;
    }
    // A neighbor's heading reads it, as the arrow key would; the rest of
    // its text is plain, selectable text.
    if let Some(heading) = closest(&target, ".chapter-heading-link") {
        if let (Some(book_id), Some(chapter)) = (
            data_number(&heading, "data-book"),
            data_number(&heading, "data-chapter"),
        ) {
            spawn(load_reading(pane, book_id, chapter));
        }
        return;
    }
    // A citation in the chapter's Cross references list reads it, its
    // verses selected (Back returns).
    if let Some(chip) = closest(&target, ".xref-summary-chip")
        && let (Some(book_id), Some(chapter), Some(verse)) = (
            data_number(&chip, "data-book"),
            data_number(&chip, "data-chapter"),
            data_number(&chip, "data-verse"),
        )
    {
        let end = data_number(&chip, "data-end-verse").unwrap_or(verse);
        spawn(jump_to(
            pane,
            book_id,
            chapter,
            Some(verse),
            Some(end),
            false,
        ));
    }
}

/// Listens for Try again in `pane`'s notice (the second pane's, as it's
/// made).
pub fn listen_to_notice(pane: PaneId) {
    if let Some(notice) = pane.notice() {
        dom::on(&notice, "click", move |event| {
            let on_retry = event
                .target()
                .and_then(|t| t.dyn_into::<Element>().ok())
                .is_some_and(|t| closest(&t, ".reading-retry").is_some());
            if on_retry {
                retry(pane);
            }
        });
    }
}

/// `pane`'s own column scrolled (two panes open): where the reader is is
/// saved once it's still, as the window's scrolling does for one pane.
pub fn scrolled(pane: PaneId) {
    save_place_soon(pane);
}

// ---- Two panes.

/// Opens `passage` in the second pane (`pane`'s `gospelGetterPanes`), in
/// `code`, or in the active pane's translation: a pane opened beside reads
/// it, its history starting there; one already open jumps to it, as a jump
/// in its own history, so its Back returns (S12). The first pane is never
/// replaced. The second pane is then the active one (S13). Its end is the
/// load's.
pub fn open_beside(passage: &OpenPassage, code: Option<String>) -> Load {
    let code = code.unwrap_or_else(|| pane::code(pane::active()));
    let pane = PaneId::Second;
    pane::open_second();
    if !pane::split() {
        return Box::pin(async { false });
    }
    let (book_id, chapter) = (passage.book_id, passage.chapter);
    let (start, end) = (passage.verse_start, passage.verse_end);
    if with(pane, |s| s.reading.is_some()) {
        // Active first, so it's in view: a pane out of view (the tabs,
        // reading mode) has its place put back as it comes into view
        // (`surfaces`), and the jump then records where the reader left
        // it, not a hidden column's scroll position (0 in WebKitGTK and
        // WebView2), which would send its Back to the chapter's top.
        pane::set_active(pane);
        if code != pane::code(pane) {
            switch_for_jump(pane, &code);
        }
        return jump_to(pane, book_id, chapter, start, end, start.is_none());
    }
    pane::set_code(pane, &code);
    let open = open_passage(pane, book_id, chapter, start, end);
    // After its load has begun, so the translation the record then says is
    // the one it's reading (`Request::Translation` leaves it be).
    pane::set_active(pane);
    Box::pin(async move {
        let (loaded, _) = open.await;
        if loaded && start.is_none() {
            pane.scroller().scroll_to(0.0);
        }
        if with(pane, |s| s.reading.is_some()) && !with(pane, |s| s.history.started()) {
            start_history(pane);
        }
        loaded
    })
}

/// Boot's second pane, opened empty beside the first before the first's
/// chapter was drawn (`pane::reopen`), once that chapter is on screen: its
/// own chapter, in its own translation, then the verse the reader was at
/// put back at the top of its view (as [`start`] does the first's), its
/// history starting there. The first pane stays the active one (D8). Its
/// end is the load's.
pub fn reopen_beside(place: SecondPlace) -> Load {
    let pane = PaneId::Second;
    if !pane::split() {
        return Box::pin(async { false });
    }
    pane::set_code(pane, &place.translation);
    let load = load_reading(pane, place.book_id, place.chapter);
    Box::pin(async move {
        let loaded = load.await;
        if with(pane, |s| s.reading.is_some()) {
            start_history(pane);
        }
        if let (true, Some(verse)) = (loaded, place.verse) {
            reopen_at(pane, verse, place.offset);
        }
        loaded
    })
}

/// `pane` is the active one now (`pane::set_active`): the header, the
/// window's title, the picker, Back and Forward and the translation button
/// say its place.
pub fn activated(pane: PaneId) {
    show_translation();
    let (here, compare) = with(pane, |s| (s.reading.clone(), s.last_compare.clone()));
    if let Some(here) = &here {
        let compare = compare.filter(|c| {
            is_comparing()
                && text::compare_is_current(c, here.book_id, here.chapter, &compare_codes())
        });
        update_reference(here, compare.as_deref());
        crate::picker::show_current(pane, here.book_id, here.chapter);
    }
    show_nav(pane);
}

/// The panes changed (one opened beside `pane`, or closed): its chapter
/// drawn again for them, without its faded neighbors while two share the
/// window, and in Compare as it is now (paused while they do).
pub fn redraw(pane: PaneId) {
    if is_comparing() {
        on_compare_change(pane);
    } else {
        render_current(pane);
    }
}

/// The second pane closed: nothing of it is kept, and anything still on its
/// way for it is dropped as it comes.
pub fn forget(pane: PaneId) {
    with(pane, |s| {
        s.edit.cancel();
        s.compare.cancel();
        *s = State {
            load_token: s.load_token + 1,
            edit: s.edit,
            compare: s.compare,
            ..State::default()
        };
    });
}

/// Every piece has mounted (`lib`'s ready list): the picker, mounted after
/// this, hears the chapter on screen again.
pub fn on_ready() {
    let pane = pane::active();
    if let Some(here) = with(pane, |s| s.reading.clone()) {
        crate::picker::show_current(pane, here.book_id, here.chapter);
    }
}

#[derive(Serialize)]
struct CreditArgs<'a> {
    which: &'a str,
}

/// Listens for clicks on the pane's controls and on Try again, and the
/// mouse's back and forward buttons. Called right after Aa's
/// (whose records it fills) and the selection card's (which it tells about
/// each chapter), and before boot (`boot`), which asks it for the first
/// chapter at once, so it needn't wait for the other pieces to mount.
pub fn mount() {
    let Some(document) = bridge::document() else {
        return;
    };
    // After the click dispatcher on the body (`clicks`), which ignores
    // these.
    if let Some(body) = document.body() {
        dom::on(&body, "click", |event| {
            if let Some(event) = event.dyn_ref::<MouseEvent>() {
                on_click(event);
            }
        });
    }
    for pane in pane::shown() {
        listen_to_notice(pane);
    }
    // Where the reader is, saved once they stop scrolling, and at once as
    // the window is hidden or loses focus, or the pointer leaves the page
    // (for the window's close button, say: a window closing destroys its
    // webview without a `pagehide`).
    if let Some(window) = web_sys::window() {
        for pane in pane::shown() {
            let Some(target) = pane.scroller().target() else {
                continue;
            };
            let passive = web_sys::AddEventListenerOptions::new();
            passive.set_passive(true);
            let scrolled = Closure::<dyn Fn()>::new(move || save_place_soon(pane));
            let _ = target.add_event_listener_with_callback_and_add_event_listener_options(
                "scroll",
                scrolled.as_ref().unchecked_ref(),
                &passive,
            );
            scrolled.forget();
        }
        dom::on(&window, "pagehide", |_| save_places());
        dom::on(&window, "blur", |_| save_places());
        // The print stylesheet prints both lists open, whatever they show
        // on screen, so a closed Cross references list is filled first: the
        // active pane's, the one that prints (D14).
        dom::on(&window, "beforeprint", |_| {
            let pane = pane::active();
            if let Some(list) = pane.query(&format!("#{}", pane.ids().xrefs)) {
                fill_xref_rows(pane, &list);
            }
        });
    }
    dom::on(&document, "visibilitychange", |_| {
        if bridge::document().is_some_and(|d| d.hidden()) {
            save_places();
        }
    });
    if let Some(html) = document.document_element() {
        dom::on(&html, "mouseleave", |_| save_places());
    }
    export_place();
    // A mouse's back and forward buttons; while a dialog is open, nothing
    // (as Alt+← and Alt+→, `keyboard`), so the chapter behind it stays.
    if let Some(window) = web_sys::window() {
        dom::on(&window, "mouseup", |event| {
            let Some(event) = event.dyn_ref::<MouseEvent>() else {
                return;
            };
            let step = match event.button() {
                3 => -1,
                4 => 1,
                _ => return,
            };
            event.prevent_default();
            if crate::page::modal_open() {
                return;
            }
            spawn(go_history(pane::active(), step));
        });
    }
}
