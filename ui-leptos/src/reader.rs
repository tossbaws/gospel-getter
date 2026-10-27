//! The reader: the chapter on screen, and how it gets there. It reads a
//! chapter from the backend (`get_reading`, with `get_compare` beside it
//! while comparing), draws it into the page's `#reading-pane` (the verses,
//! the faded neighbors or the Bible's edge cards, Compare's grid, the
//! chapter's end and its Footnotes and Cross references), says where the
//! reader is on the reference button and in the window's title, keeps the
//! reading position for the next launch, says when a chapter couldn't be
//! read (with Try again), and keeps the passage history (Back and Forward,
//! with the mouse's back and forward buttons). It fills the translation
//! button's record, `#translation-select`, from boot's answer, and answers
//! its own controls in the pane: the chapter steps, a neighbor's heading,
//! the two disclosures, the chapter's cross-references and their credit
//! links.
//!
//! The selection card (`card`) hears each chapter loading and shown, and
//! each passage to select, from here; boot (`boot`), the keyboard
//! (`keyboard`), the click dispatcher (`clicks`) and the page's glue
//! (`page`) ask it for chapters ([`start`], [`ask`]). A stale answer
//! never overwrites a newer one: each load has its token, as the page's
//! `loadToken`, each step back or forward its own, and a position is saved
//! only for a chapter put on screen, in the order they were shown.
//!
//! This is the page's reader (`ui/index.html` at 4a7f6f5: `updateReference`
//! to the `compare-change` listener, `openPassage`, `savePosition` and the
//! passage history) in Rust: the same commands with the same arguments, in
//! the same order, the same markup (`reader_text`), ids, classes, ARIA,
//! text and focus. The pane is rebuilt whole per chapter, from markup, as it
//! was: the page moves its selection card, Copy as… and the cross-reference
//! preview in and out of it, which a view owning the pane's nodes would
//! fight.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use serde::Serialize;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_futures::spawn_local;
use web_sys::{Element, HtmlElement, HtmlOptionElement, HtmlSelectElement, MouseEvent};

use crate::aa;
use crate::bridge;
use crate::card;
use crate::dom;
use crate::dto::{Compare, CompareArgs, Home, Position, Reading, ReadingArgs, Verse};
use crate::ipc;
use crate::palette::PaletteState;
use crate::reader_text::{self as text, Disclosure, History, Opened, Place, Which};

/// A load, a jump or a step: whether it put a chapter on screen.
pub type Load = Pin<Box<dyn Future<Output = bool>>>;

/// A passage opened: whether it put its chapter on screen, and the verses
/// the selection card then had selected (`card::select_passage`).
type OpenedPassage = Pin<Box<dyn Future<Output = (bool, Option<(i64, i64)>)>>>;

/// The chapter on screen, as the page's `reading` named it.
#[derive(Debug, Clone)]
struct Here {
    book_id: i64,
    book_name: String,
    chapter: i64,
    translation_code: String,
}

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
    /// A position is being saved; the newest one waiting goes next.
    saving: bool,
    waiting: Option<Position>,
    /// The chapter Try again reads again.
    retry: Option<(i64, i64)>,
    /// Each bundled translation's credit, from boot, for About.
    credits: Vec<String>,
    history: History,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// The reader's state, never held while the page is told anything (its
/// listeners may ask the reader again).
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
}

// ---- The page's elements and settings.

fn by_id(id: &str) -> Option<HtmlElement> {
    bridge::document()?
        .get_element_by_id(id)?
        .dyn_into::<HtmlElement>()
        .ok()
}

fn closest(el: &Element, selector: &str) -> Option<Element> {
    el.closest(selector).ok().flatten()
}

fn data_number(el: &Element, name: &str) -> Option<i64> {
    el.get_attribute(name)?.parse().ok()
}

/// The pane's `#reading-pane-inner`, which says which chapter is on screen
/// and which are around it (`data-current-*`, `data-prev-*`,
/// `data-next-*`), as the keys read it (`keyboard`).
fn pane_inner() -> Option<Element> {
    bridge::document()?.get_element_by_id("reading-pane-inner")
}

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

fn is_comparing() -> bool {
    bridge::document()
        .and_then(|d| d.document_element())
        .is_some_and(|html| html.get_attribute("data-compare").as_deref() == Some("on"))
}

fn compare_codes() -> Vec<String> {
    text::compare_codes(&bridge::translation_code(), aa::partner().as_deref())
}

fn compare_layout() -> String {
    text::compare_layout(is_comparing(), &compare_codes())
}

fn scroll_y() -> f64 {
    web_sys::window()
        .and_then(|w| w.scroll_y().ok())
        .unwrap_or(0.0)
}

fn scroll_to(y: f64) {
    if let Some(window) = web_sys::window() {
        window.scroll_to_with_x_and_y(0.0, y);
    }
}

/// The notice above the chapter: `message`, or none.
fn show_notice(message: &str) {
    if let Some(notice) = by_id("reader-notice") {
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

/// Opens or closes every disclosure of that kind on screen (both columns'
/// footnotes while comparing), and remembers it.
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
            body.set_hidden(!open);
        }
    }
}

// ---- What the header says.

/// The translation button: the translation being read by its code, named
/// in full.
fn show_translation() {
    let code = bridge::translation_code();
    let (label, name) = text::translation_button(&code, &translation_name(&code));
    if let Some(toggle) = by_id("translation-toggle") {
        toggle.set_text_content(Some(&label));
        let _ = toggle.set_attribute("aria-label", &name);
    }
}

/// The reference button and the window title (which the backend copies to
/// the window) say where the reader is: the chapter, in each translation on
/// screen.
fn update_reference(here: &Here, compare: Option<&Compare>) {
    let codes: Vec<&str> = match compare {
        Some(compare) => compare.columns.iter().map(|c| c.code.as_str()).collect(),
        None => vec![here.translation_code.as_str()],
    };
    let reference = text::reference(&here.book_name, here.chapter, &codes, translation_name);
    if let Some(ref_text) = by_id("ref-text") {
        ref_text.set_text_content(Some(&reference.text));
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
    send_credits(&credits);
    with(|s| s.credits = credits);
}

/// About's credits, to About (which may not be there yet: they go again
/// once every piece is, `bridge::READY_EVENT`).
fn send_credits(credits: &[String]) {
    if !credits.is_empty() {
        bridge::dispatch(bridge::ABOUT_CREDITS_EVENT, &Credits { credits });
    }
}

/// About's credits, each bundled text's, in order.
#[derive(Serialize)]
struct Credits<'a> {
    credits: &'a [String],
}

// ---- Putting a chapter on screen.

/// Puts the last reading (and the comparison, while comparing and it's
/// this chapter's in the translations wanted) on screen, then tells the
/// selection card (`card::shown`).
fn render_current() {
    let Some(pane) = by_id("reading-pane") else {
        return;
    };
    let (dto, compare, here) = with(|s| {
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
    let compare_first = with(|s| {
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

    pane.set_inner_html(&text::reading_pane(
        &dto,
        compare.as_deref(),
        &here.translation_code,
        opened(),
    ));
    // The grid's column count goes through the CSSOM, not a style attribute
    // in the markup, which the Content Security Policy would block.
    if let Some(grid) = pane
        .query_selector(".compare-grid")
        .ok()
        .flatten()
        .and_then(|el| el.dyn_into::<HtmlElement>().ok())
    {
        let count = grid.get_attribute("data-columns").unwrap_or_default();
        let _ = grid.style().set_property("--compare-columns", &count);
    }
    with(|s| s.columns = columns);
    update_reference(&here, compare.as_deref());
    bridge::show_picker_current(here.book_id, here.chapter);
    card::shown(dto, compare, &here.translation_code, compare_first);
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
fn load_reading(book_id: i64, chapter: i64) -> Load {
    load_sent(book_id, chapter, None)
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
fn load_sent(book_id: i64, chapter: i64, sent: Option<Sent>) -> Load {
    let token = with(|s| {
        s.load_token += 1;
        // A step's Try again is this load's now (see `retry`); a step's
        // own load begins after it's taken.
        s.history.forget_failed();
        s.load_token
    });
    card::loading();
    let translation_code = bridge::translation_code();
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
            token,
            book_id,
            chapter,
            translation_code,
            &layout,
            dto_raw.clone(),
            JsValue::NULL,
        );
        return match answer {
            Outcome::Done(loaded) => Box::pin(async move { loaded }),
            Outcome::Again => load_reading(book_id, chapter),
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
                failed(token, &e, book_id, chapter);
                return false;
            }
        };
        let compare_raw = match compare {
            Some(promise) => JsFuture::from(promise).await.unwrap_or(JsValue::NULL),
            None => JsValue::NULL,
        };
        let answer = answered(
            token,
            book_id,
            chapter,
            translation_code,
            &layout,
            dto_raw,
            compare_raw,
        );
        match answer {
            Outcome::Done(loaded) => loaded,
            Outcome::Again => load_reading(book_id, chapter).await,
        }
    })
}

/// A load's answers (the reading, and the comparison or `null`), sent with
/// `token` in the translation `translation_code` for the layout `layout`:
/// put on screen, and the position saved, unless a newer load has begun,
/// the chapter isn't there, or Compare changed on the way.
fn answered(
    token: u64,
    book_id: i64,
    chapter: i64,
    translation_code: String,
    layout: &str,
    dto_raw: JsValue,
    compare_raw: JsValue,
) -> Outcome {
    // A newer load has begun: let it win.
    if with(|s| s.load_token) != token || dto_raw.is_null() || dto_raw.is_undefined() {
        return Outcome::Done(false);
    }
    // Compare was switched (or its translations changed) on the way: what
    // came back is for the old layout, so it's asked for again.
    if compare_layout() != layout {
        return Outcome::Again;
    }
    let dto: Reading = match serde_wasm_bindgen::from_value(dto_raw.clone()) {
        Ok(dto) => dto,
        Err(e) => {
            failed(token, &JsValue::from(e.to_string()), book_id, chapter);
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
    with(|s| {
        s.last_dto = Some(Rc::new(dto));
        if let Some(compare) = compare {
            s.last_compare = Some(Rc::new(compare));
        }
        s.reading = Some(here.clone());
    });
    show_notice(if is_comparing() && !compared {
        text::COMPARE_FAILED
    } else {
        ""
    });
    render_current();
    save_position(Position {
        book_id: here.book_id,
        chapter: here.chapter,
        translation_code,
    });
    Outcome::Done(true)
}

/// A load that failed: logged, and, unless a newer one has begun, said.
fn failed(token: u64, error: &JsValue, book_id: i64, chapter: i64) {
    ipc::log_error("Failed to load reading pane", error);
    if with(|s| s.load_token) == token {
        show_failure(&ipc::error_text(error), book_id, chapter);
    }
}

/// A chapter that couldn't be read: the notice says so until a chapter is
/// put on screen, with a Try again that asks for it once more (and starts
/// the history, if it was the first chapter). The chapter on screen, if
/// any, stays, and a toast says it too, however far down the reader is.
fn show_failure(message: &str, book_id: i64, chapter: i64) {
    show_notice(message);
    with(|s| s.retry = Some((book_id, chapter)));
    if let (Some(document), Some(notice)) = (bridge::document(), by_id("reader-notice"))
        && let Ok(retry) = document.create_element("button")
    {
        let _ = retry.set_attribute("type", "button");
        retry.set_class_name("note-link reading-retry");
        retry.set_text_content(Some("Try again"));
        let _ = notice.append_with_str_1(" ");
        let _ = notice.append_with_node_1(&retry);
    }
    if with(|s| s.last_dto.is_some()) {
        bridge::toast(message, true);
    }
}

/// Try again: the chapter that couldn't be read.
fn retry() {
    let Some((book_id, chapter)) = with(|s| s.retry) else {
        return;
    };
    // After a step back or forward that couldn't be read, the step again.
    if with(|s| s.history.can_retry(book_id, chapter)) {
        remember_leaving();
        if let Some(going) = with(|s| s.history.begin_retry(book_id, chapter)) {
            spawn(open_step(going));
        }
        return;
    }
    let load = load_reading(book_id, chapter);
    spawn_local(async move {
        if load.await && !with(|s| s.history.started()) {
            start_history();
        }
    });
}

/// Saves the chapter put on screen as the position the next launch opens
/// at. One save is on its way at a time; a newer one waits, replacing any
/// that waited before it, so they reach the backend in the order the
/// chapters were shown and the last one wins. The first is sent now.
fn save_position(position: Position) {
    let first = with(|s| {
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
    let mut sent = send_position(&first);
    spawn_local(async move {
        loop {
            let answer = match sent {
                Ok(promise) => JsFuture::from(promise).await,
                Err(e) => Err(e),
            };
            if let Err(e) = answer {
                // The next chapter shown saves again.
                ipc::log_error("Failed to save the reading position", &e);
            }
            let Some(next) = with(|s| s.waiting.take()) else {
                break;
            };
            sent = send_position(&next);
        }
        with(|s| s.saving = false);
    });
}

fn send_position(position: &Position) -> Result<js_sys::Promise, JsValue> {
    ipc::args(position).and_then(|args| ipc::send("save_reading_position", args))
}

/// Compare went on or off, or what it compares with changed: the chapter on
/// screen in the new layout, fetching the comparison only if it isn't the
/// one there already (and dropping it if a load has begun meanwhile).
fn on_compare_change() {
    let Some(dto) = with(|s| s.last_dto.clone()) else {
        return;
    };
    if !is_comparing() {
        show_notice("");
        render_current();
        return;
    }
    let codes = compare_codes();
    let current = with(|s| {
        s.last_compare.as_ref().is_some_and(|c| {
            text::compare_is_current(c, dto.current_book_id, dto.current_chapter, &codes)
        })
    });
    if current {
        render_current();
        return;
    }
    let token = with(|s| s.load_token);
    let compare = load_compare(dto.current_book_id, dto.current_chapter);
    spawn_local(async move {
        let compare_raw = JsFuture::from(compare).await.unwrap_or(JsValue::NULL);
        if with(|s| s.load_token) != token {
            return;
        }
        let compare = (!compare_raw.is_null())
            .then(|| serde_wasm_bindgen::from_value::<Compare>(compare_raw.clone()).ok())
            .flatten();
        match compare {
            Some(compare) => with(|s| s.last_compare = Some(Rc::new(compare))),
            None => show_notice(text::COMPARE_FAILED),
        }
        render_current();
    });
}

// ---- Passages and the history.

/// Opens a chapter and selects `start`–`end` in it, in the translation
/// being read (the selection card selects them, `card::select_passage`), for
/// every jump and for going back and forward to one. If that translation
/// doesn't number them, the chapter opens and the notice says so, rather
/// than selecting other verses. Whether it opened, and what the page then
/// had selected.
fn open_passage(book_id: i64, chapter: i64, start: Option<i64>, end: Option<i64>) -> OpenedPassage {
    let load = load_reading(book_id, chapter);
    Box::pin(async move {
        let loaded = load.await;
        let (true, Some(start)) = (loaded, start) else {
            return (loaded, None);
        };
        let end = end.unwrap_or(start);
        let Some(here) = with(|s| s.reading.clone()) else {
            return (loaded, None);
        };
        let numbers = with(|s| {
            s.columns
                .iter()
                .find(|(code, _)| *code == here.translation_code)
                .map(|(_, numbers)| numbers.clone())
                .unwrap_or_default()
        });
        if numbers.contains(&start) && numbers.contains(&end) {
            let selected = card::select_passage(&here.translation_code, start, end);
            return (loaded, selected);
        }
        // Past the chapter's end in this translation, or a verse it leaves
        // out in mid-chapter (as the BSB does Matthew 17:21).
        let last = numbers.last().copied().unwrap_or(0);
        show_notice(&text::not_numbered(
            &here.book_name,
            chapter,
            start,
            end,
            &translation_name(&here.translation_code),
            last,
        ));
        (loaded, None)
    })
}

/// Starts the history over at the chapter being read.
fn start_history() {
    with(|s| {
        if let Some(here) = &s.reading {
            let (book_id, chapter) = (here.book_id, here.chapter);
            s.history.start(book_id, chapter);
        }
    });
}

/// The place on screen, brought up to date as the reader leaves it.
fn remember_leaving() {
    let y = scroll_y();
    with(|s| {
        let here = s.reading.as_ref().map(|r| (r.book_id, r.chapter));
        s.history.remember_leaving(here, y);
    });
}

/// A jump: opens the chapter (selecting `start`–`end`, if given and
/// numbered; from the top, with `top`) and records it in the history.
fn jump_to(book_id: i64, chapter: i64, start: Option<i64>, end: Option<i64>, top: bool) -> Load {
    remember_leaving();
    with(|s| s.history.begin_jump());
    let open = open_passage(book_id, chapter, start, end);
    Box::pin(async move {
        let (loaded, selected) = open.await;
        let place = if loaded {
            if top {
                scroll_to(0.0);
            }
            let landed =
                selected.filter(|&verses| start.is_some() && Some(verses) == start.zip(end));
            with(|s| {
                s.reading.as_ref().map(|here| Place {
                    start: landed.map(|v| v.0),
                    end: landed.map(|v| v.1),
                    ..Place::chapter(here.book_id, here.chapter)
                })
            })
        } else {
            None
        };
        with(|s| s.history.end_jump(place));
        loaded
    })
}

/// Back (-1) or forward (1): the place, its verses selected, and the page
/// scrolled where the reader left it; at either end, a toast says there's
/// nothing there. A newer step or jump takes over from one on its way.
fn go_history(step: i32) -> Load {
    if !with(|s| s.history.can_go(step)) {
        bridge::toast(text::history_end(step), false);
        return Box::pin(async { false });
    }
    remember_leaving();
    let Some(going) = with(|s| s.history.begin_go(step)) else {
        return Box::pin(async { false });
    };
    open_step(going)
}

/// Opens the place a step back or forward (with its token) is going to.
/// Only once it's read is it the place on screen; one that can't be read
/// leaves the history as it was (`History::end_go`).
fn open_step((token, place): (u64, Place)) -> Load {
    let open = open_passage(place.book_id, place.chapter, place.start, place.end);
    Box::pin(async move {
        let (loaded, _) = open.await;
        if !with(|s| s.history.current(token)) {
            return false;
        }
        if loaded {
            if let Some(y) = place.scroll_y {
                scroll_to(y);
            } else if place.start.is_none() {
                scroll_to(0.0);
            }
        }
        with(|s| s.history.end_go(token, loaded));
        loaded
    })
}

/// Runs `load` to its end, its answer dropped.
fn spawn(load: Load) {
    spawn_local(async move {
        let _ = load.await;
    });
}

/// Opens a chapter as a jump (recorded in the history): the welcome's
/// places to start, a chapter chosen in the picker or Random chapter's;
/// from the top, with `top`.
pub fn jump(book_id: i64, chapter: i64, top: bool) {
    spawn(jump_to(book_id, chapter, None, None, top));
}

/// A Library row or a search result opens its passage (the Library or the
/// search box already closed), as a jump, its verses selected; with a
/// `note` (a note's Edit, as `list_all_notes` gave it), the selection card
/// then opens the note editor on it (`card::edit_note`).
pub fn open(passage: &bridge::OpenPassage, note: JsValue) {
    let load = jump_to(
        passage.book_id,
        passage.chapter,
        passage.verse_start,
        passage.verse_end,
        false,
    );
    spawn_local(async move {
        let _ = load.await;
        if !note.is_null() && !note.is_undefined() && with(|s| s.reading.is_some()) {
            card::edit_note(&note);
        }
    });
}

/// What the palette's commands need of the reader: the chapters (as the
/// pane says, which ← and → go by) and books around the one on screen, and
/// the history either way.
pub fn palette_facts(state: &mut PaletteState) {
    let inner = pane_inner();
    let has = |name| {
        inner
            .as_ref()
            .is_some_and(|inner| inner.has_attribute(name))
    };
    state.next_chapter = has("data-next-book");
    state.previous_chapter = has("data-prev-book");
    with(|s| {
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
/// within this call), then the history from there. Its end is the load's.
pub fn start(home: Home, sent: Option<Sent>) -> Load {
    run(Request::Start { home }, sent)
}

/// Starts what `request` asks for, now (anything it sends is sent within
/// this call); its end is the load's: whether a chapter was put on screen.
pub fn ask(request: Request) -> Load {
    run(request, None)
}

/// Starts what `request` asks for, now; its end is the load's. `sent` is
/// boot's `get_reading`, already on its way, or answered.
fn run(request: Request, sent: Option<Sent>) -> Load {
    match request {
        Request::Start { home } => {
            show_translations(&home);
            let load = load_sent(home.current_book_id, home.current_chapter, sent);
            Box::pin(async move {
                let loaded = load.await;
                if with(|s| s.reading.is_some()) {
                    start_history();
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
            if let (Some(code), Some(select)) = (translation_code, translation_select()) {
                select.set_value(&code);
                show_translation();
            }
            from_top(load_reading(book_id, chapter), top)
        }
        Request::Book { which } => {
            let found = with(|s| {
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
                    from_top(load_reading(book_id, chapter), true)
                }
                Some((None, name)) => {
                    bridge::toast(&text::no_book(&name, which), false);
                    Box::pin(async { false })
                }
                None => Box::pin(async { false }),
            }
        }
        Request::Translation => {
            show_translation();
            let inner = pane_inner();
            let number = |name| {
                inner
                    .as_ref()
                    .map_or(Some(1), |inner| data_number(inner, name))
            };
            match (number("data-current-book"), number("data-current-chapter")) {
                (Some(book_id), Some(chapter)) => load_reading(book_id, chapter),
                _ => Box::pin(async { false }),
            }
        }
        Request::Jump {
            book_id,
            chapter,
            start,
            end,
        } => jump_to(book_id, chapter, start, end, false),
        Request::History { step } => go_history(step),
        Request::Notice { message } => {
            show_notice(&message);
            Box::pin(async { true })
        }
    }
}

/// `load`, then the page scrolled to the top if it put a chapter there and
/// `top` asks for that (the chapter's end and `[`/`]`, as before).
fn from_top(load: Load, top: bool) -> Load {
    if !top {
        return load;
    }
    Box::pin(async move {
        let loaded = load.await;
        if loaded {
            scroll_to(0.0);
        }
        loaded
    })
}

// ---- The pane's own controls.

/// A click in the reading pane on one of the reader's controls. The click
/// dispatcher (`clicks`, on the body too, registered first) has already
/// done what any click does there (closing the search box and Copy as…)
/// and left these alone.
fn on_click(event: &MouseEvent) {
    let Some(target) = event
        .target()
        .and_then(|t| t.dyn_into::<Element>().ok())
        .filter(|t| closest(t, "#reading-pane").is_some())
    else {
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
                bridge::toast(&ipc::error_text(&e), true);
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
            spawn(from_top(load_reading(book_id, chapter), true));
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
            spawn(load_reading(book_id, chapter));
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
        spawn(jump_to(book_id, chapter, Some(verse), Some(end), false));
    }
}

#[derive(Serialize)]
struct CreditArgs<'a> {
    which: &'a str,
}

/// Listens for Compare's changes, clicks on the pane's controls and on Try
/// again, and the mouse's back and forward buttons. Called right after Aa's
/// (whose records it fills) and the selection card's (which it tells about
/// each chapter), and before boot (`boot`), which asks it for the first
/// chapter at once, so it needn't wait for the other pieces to mount.
pub fn mount() {
    let Some(document) = bridge::document() else {
        return;
    };
    // About and the picker are mounted after this: they hear its credits
    // and the chapter on screen again once every piece is.
    dom::on_document_event(bridge::READY_EVENT, |_| {
        let (credits, here) = with(|s| (s.credits.clone(), s.reading.clone()));
        send_credits(&credits);
        if let Some(here) = here {
            bridge::show_picker_current(here.book_id, here.chapter);
        }
    });
    dom::on_document_event(aa::COMPARE_CHANGE_EVENT, |_| on_compare_change());
    // After the click dispatcher on the body (`clicks`), which ignores
    // these.
    if let Some(body) = document.body() {
        dom::on(&body, "click", |event| {
            if let Some(event) = event.dyn_ref::<MouseEvent>() {
                on_click(event);
            }
        });
    }
    if let Some(notice) = by_id("reader-notice") {
        dom::on(&notice, "click", |event| {
            let on_retry = event
                .target()
                .and_then(|t| t.dyn_into::<Element>().ok())
                .is_some_and(|t| closest(&t, ".reading-retry").is_some());
            if on_retry {
                retry();
            }
        });
    }
    // A mouse's back and forward buttons.
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
            spawn(go_history(step));
        });
    }
}
