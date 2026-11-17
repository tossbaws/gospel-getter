//! The reading panes: their elements, what scrolls each, and which is
//! active. Every read or write of a reading view's scroll position, and of
//! the band of it in view, goes through a pane's [`Scroller`], and every
//! reading-view query runs on the pane's root ([`PaneId::root`]), not on
//! the document. This is the one place that knows where each pane scrolls
//! and where its elements are. `src-tauri/tests/frontend_markup.rs` keeps
//! the scroll calls here (`reading_view_scroll_stays_in_pane`).
//!
//! The selection card, the picker, search, the dialogs and the card's own
//! scrolling aren't the reading view's: they scroll themselves.
//!
//! One pane is the page as it has always been: the window scrolls it, and
//! its elements are the page's own (`#reader-notice`, `#reading-pane`). A
//! second pane opens beside it ([`open_second`]; only `gospelGetterPanes`
//! opens one for now): each pane then has a column of its own, which
//! scrolls by itself below the top bar, with a bar at its top whose
//! controls are copies of the top bar's reference, arrows and translation
//! button. Each control makes its pane active, then does exactly what the
//! top bar's does ([`set_active`], then the same call). [`close_second`]
//! puts the page back as it was.
//!
//! What each pane shows is its own (its chapter and translation, its loads,
//! its history and place, the card's and the findings' facts about its
//! chapter): the pieces keep it in a [`PerPane`], reached by the pane's
//! [`PaneId`]. What acts on a pane without being in one (the keys, the
//! palette, the header, the selection card) acts on the [`active`] one; what
//! follows every pane goes to each [`shown`]. A pointer pressed or focus
//! moving into a pane makes it the active one (D8). The translation button's
//! record, `#translation-select`, says the active pane's translation (D12),
//! so the Library's previews, search and Export notes follow it; each
//! pane's own is kept here while it isn't active ([`code`]).

use std::cell::{Cell, RefCell};
use std::ops::{Index, IndexMut};
use std::rc::Rc;

use wasm_bindgen::prelude::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    AddEventListenerOptions, Element, EventTarget, HtmlElement, HtmlSelectElement, KeyboardEvent,
    Node, ResizeObserver,
};

use crate::bridge;
use crate::dom::{self, by_id};
use crate::keyboard;
use crate::nav_text;
use crate::pane_text::{self as text, ScrollMove};
use crate::reader::{self, OpenPassage};
use crate::reader_text::{PaneIds, Which};
use crate::surfaces::{self, Surface};

/// A reading pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaneId {
    /// The page's reading pane: the one the reader opens, whose place the
    /// backend saves.
    First,
    /// The pane opened beside it. Its place is never saved (S15).
    Second,
}

impl PaneId {
    /// Its name in `data-pane` and to the tests.
    fn name(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::Second => "second",
        }
    }

    /// The ids its chapter carries.
    pub fn ids(self) -> PaneIds {
        match self {
            Self::First => PaneIds::FIRST,
            Self::Second => PaneIds::SECOND,
        }
    }

    /// The element the reader draws the pane's chapter into.
    pub fn root(self) -> Option<HtmlElement> {
        match self {
            Self::First => by_id("reading-pane"),
            Self::Second => by_id("reading-pane-2"),
        }
    }

    /// The pane's `#reading-pane-inner` (its own id, [`PaneIds`]), which
    /// says which chapter is on screen and which are around it
    /// (`data-current-*`, `data-prev-*`, `data-next-*`).
    pub fn inner(self) -> Option<Element> {
        self.root()?
            .query_selector(&format!("#{}", self.ids().inner))
            .ok()
            .flatten()
    }

    /// The notice above the pane's chapter (a chapter that couldn't be read,
    /// a verse the translation doesn't number).
    pub fn notice(self) -> Option<HtmlElement> {
        match self {
            Self::First => by_id("reader-notice"),
            Self::Second => by_id("reader-notice-2"),
        }
    }

    /// The first element in the pane matching `selector`.
    pub fn query(self, selector: &str) -> Option<HtmlElement> {
        self.root()?
            .query_selector(selector)
            .ok()??
            .dyn_into::<HtmlElement>()
            .ok()
    }

    /// Every element in the pane matching `selector`, in page order.
    pub fn query_all(self, selector: &str) -> Vec<HtmlElement> {
        self.root()
            .and_then(|root| root.query_selector_all(selector).ok())
            .map(|found| dom::elements(&found))
            .unwrap_or_default()
    }

    /// Everything that's the pane's on the page: its column while split
    /// (its bar, notice and chapter), its root otherwise.
    fn column(self) -> Option<HtmlElement> {
        if split() {
            made().map(|made| made.columns[self].clone())
        } else if self == Self::First {
            self.root()
        } else {
            None
        }
    }

    /// The control `selector` finds in the pane's bar, while it has one.
    fn in_bar(self, selector: &str) -> Option<HtmlElement> {
        if !split() {
            return None;
        }
        made()?.bars[self]
            .query_selector(selector)
            .ok()??
            .dyn_into::<HtmlElement>()
            .ok()
    }

    /// What scrolls the pane: the window, or its own column while split.
    pub fn scroller(self) -> Scroller {
        match (split(), made()) {
            (true, Some(made)) => Scroller::Element(made.columns[self].clone()),
            _ => Scroller::Window,
        }
    }
}

/// One `T` for each pane, reached by its [`PaneId`].
#[derive(Debug, Default)]
pub struct PerPane<T> {
    first: T,
    second: T,
}

impl<T> PerPane<T> {
    /// Each pane's `T`, made by `make`.
    pub fn new(make: impl Fn(PaneId) -> T) -> Self {
        Self {
            first: make(PaneId::First),
            second: make(PaneId::Second),
        }
    }
}

impl<T> Index<PaneId> for PerPane<T> {
    type Output = T;

    fn index(&self, pane: PaneId) -> &T {
        match pane {
            PaneId::First => &self.first,
            PaneId::Second => &self.second,
        }
    }
}

impl<T> IndexMut<PaneId> for PerPane<T> {
    fn index_mut(&mut self, pane: PaneId) -> &mut T {
        match pane {
            PaneId::First => &mut self.first,
            PaneId::Second => &mut self.second,
        }
    }
}

/// What scrolls a reading pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scroller {
    /// The window: the page scrolls, under the fixed top bar.
    Window,
    /// The pane's own column, below the top bar (two panes open), with the
    /// pane's bar stuck to its top. It sets `overflow-anchor: none`: the
    /// browser never scrolls it to keep a place of its own, so both webviews
    /// keep a pane's place the same way (`surfaces`).
    Element(HtmlElement),
}

/// The band of a scroller in view, in client coordinates: from `top` (the
/// foot of the bar over it) down to `bottom`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct View {
    pub top: f64,
    pub bottom: f64,
}

impl Scroller {
    /// How far it's scrolled down.
    pub fn y(&self) -> Option<f64> {
        match self {
            Self::Window => web_sys::window()?.scroll_y().ok(),
            // `scrollTop` as the double it is: `Element::scroll_top` rounds.
            Self::Element(el) => js_sys::Reflect::get(el, &JsValue::from_str("scrollTop"))
                .ok()?
                .as_f64(),
        }
    }

    /// Scrolls it to `y`.
    pub fn scroll_to(&self, y: f64) {
        match self {
            Self::Window => {
                if let Some(window) = web_sys::window() {
                    window.scroll_to_with_x_and_y(0.0, y);
                }
            }
            // Through `scrollTop`, which the browser keeps in range (and
            // jsdom, which has no layout, takes as it is).
            Self::Element(el) => {
                let _ = js_sys::Reflect::set(
                    el,
                    &JsValue::from_str("scrollTop"),
                    &JsValue::from_f64(y),
                );
            }
        }
    }

    /// Scrolls it by `dy`.
    pub fn scroll_by(&self, dy: f64) {
        match self {
            Self::Window => {
                if let Some(window) = web_sys::window() {
                    window.scroll_by_with_x_and_y(0.0, dy);
                }
            }
            Self::Element(_) => self.scroll_to(self.y().unwrap_or(0.0) + dy),
        }
    }

    /// The band of it in view. The window's is below the top bar, which is
    /// fixed over the page's top; a column's, below the pane's bar stuck to
    /// its top, down to its foot.
    pub fn view(&self) -> Option<View> {
        match self {
            Self::Window => {
                let top = by_id("top-bar")?.get_bounding_client_rect().bottom();
                let bottom = web_sys::window()?.inner_height().ok()?.as_f64()?;
                Some(View { top, bottom })
            }
            Self::Element(el) => {
                let rect = el.get_bounding_client_rect();
                let bar = el
                    .query_selector(":scope > .pane-bar")
                    .ok()
                    .flatten()
                    .map_or(rect.top(), |bar| bar.get_bounding_client_rect().bottom());
                Some(View {
                    top: bar.max(rect.top()),
                    bottom: rect.top() + f64::from(el.client_height()),
                })
            }
        }
    }

    /// What its `scroll` and `scrollend` events are dispatched to.
    pub fn target(&self) -> Option<EventTarget> {
        match self {
            Self::Window => web_sys::window().map(Into::into),
            Self::Element(el) => Some(el.clone().into()),
        }
    }

    /// Whether the browser scrolls it to keep a place of its own when the
    /// window is resized: Chromium's scroll anchoring does the window's
    /// (WebView2, on Windows); WebKitGTK's doesn't. Both say they support
    /// `overflow-anchor`, so it's told by Chromium's own `window.chrome`.
    /// Its place needn't be the reader's: it can be a line of the faded
    /// chapter beside, which comes first in the page (`surfaces`'
    /// `on_shape_change` says what's done about it). A column never anchors
    /// (`overflow-anchor: none`).
    pub fn anchors_on_resize(&self) -> bool {
        match self {
            Self::Window => web_sys::window()
                .and_then(|window| js_sys::Reflect::get(&window, &JsValue::from_str("chrome")).ok())
                .is_some_and(|chrome| chrome.is_truthy()),
            Self::Element(_) => false,
        }
    }
}

// ---- Which panes there are, and which is active.

/// The columns and bars, made the first time a second pane opens and kept
/// (with their listeners) for the page's life, out of the page while one
/// pane shows.
struct Made {
    columns: PerPane<HtmlElement>,
    bars: PerPane<HtmlElement>,
    /// The second pane's notice and root, which stay in its column.
    notice: HtmlElement,
    root: HtmlElement,
}

thread_local! {
    static MADE: RefCell<Option<Rc<Made>>> = const { RefCell::new(None) };
    /// Two panes are open.
    static SPLIT: Cell<bool> = const { Cell::new(false) };
    static ACTIVE: Cell<PaneId> = const { Cell::new(PaneId::First) };
    /// Each pane's translation while it isn't the active one.
    static CODES: RefCell<PerPane<String>> = RefCell::new(PerPane::default());
    /// Where the first pane's notice and root go back to as the second
    /// pane closes: the nodes that followed each.
    static SLOTS: RefCell<Option<(Option<Node>, Option<Node>)>> = const { RefCell::new(None) };
}

fn made() -> Option<Rc<Made>> {
    MADE.with_borrow(Clone::clone)
}

/// Two panes are open.
pub fn split() -> bool {
    SPLIT.get()
}

/// The pane the keys, the palette, the header and the selection card act
/// on.
pub fn active() -> PaneId {
    ACTIVE.get()
}

/// Every pane on screen, in order.
pub fn shown() -> impl Iterator<Item = PaneId> {
    [PaneId::First]
        .into_iter()
        .chain(split().then_some(PaneId::Second))
}

/// The pane `el` is in (its bar included), if it's in one.
pub fn of(el: &Element) -> Option<PaneId> {
    shown().find(|pane| {
        pane.column()
            .is_some_and(|column| column.contains(Some(el)))
    })
}

/// The translation `pane` reads: the translation button's record's for the
/// active one, kept here for the other.
pub fn code(pane: PaneId) -> String {
    if pane == active() {
        bridge::translation_code()
    } else {
        CODES.with_borrow(|codes| codes[pane].clone())
    }
}

/// Makes `code` the translation `pane` reads, as the reader does for a
/// chapter asked for in a translation: on the record for the active pane
/// (as it always has been, no `change`), kept here for the other.
pub fn set_code(pane: PaneId, code: &str) {
    if pane == active() {
        if let Some(select) = translation_select() {
            select.set_value(code);
        }
    } else {
        CODES.with_borrow_mut(|codes| codes[pane] = code.to_owned());
    }
}

fn translation_select() -> Option<HtmlSelectElement> {
    by_id("translation-select")?.dyn_into().ok()
}

/// Makes `pane` the active one (D8), if it's on screen: a pointer pressed
/// in it, focus moving into it, one of its bar's controls, or a passage
/// opened beside. The selection, in the pane left, goes (one at a time,
/// D3); the translation button's record says the pane's translation (D12),
/// and, if that's another, the Library's previews and search follow it as
/// they would a translation chosen (its `change`, which the reader leaves
/// alone: the pane already reads it). The pane's heading takes the accent
/// rule; the header, the picker, Back and Forward and the window's title
/// (D13) say the pane's place; the margin follows it.
pub fn set_active(pane: PaneId) {
    let was = active();
    if was == pane || !shown().any(|shown| shown == pane) {
        return;
    }
    crate::card::pane_switching();
    let leaving = bridge::translation_code();
    let coming = CODES.with_borrow_mut(|codes| {
        codes[was] = leaving.clone();
        codes[pane].clone()
    });
    ACTIVE.set(pane);
    mark_active();
    if coming != leaving
        && let Some(select) = translation_select()
    {
        select.set_value(&coming);
        let init = web_sys::EventInit::new();
        init.set_bubbles(true);
        if let Ok(change) = web_sys::Event::new_with_event_init_dict("change", &init) {
            let _ = select.dispatch_event(&change);
        }
    }
    reader::activated(pane);
    crate::card::pane_switched();
    crate::findings::pane_switched();
}

/// Says on each column which pane is active, for its heading's rule.
fn mark_active() {
    let Some(made) = made() else {
        return;
    };
    for pane in [PaneId::First, PaneId::Second] {
        let _ =
            made.columns[pane].set_attribute("data-pane-active", &(pane == active()).to_string());
    }
}

/// Opens the second pane beside the first, empty (the reader then reads its
/// chapter into it, `reader::open_beside`). The page stops scrolling: each
/// pane's column scrolls below the top bar, and the first pane's place
/// moves into its column with it, the same verse at the same height
/// (`surfaces::move_place`). The faded neighbors give way (D7), and Compare
/// pauses (S14).
pub fn open_second() {
    if split() {
        return;
    }
    let Some(made) = made().or_else(make) else {
        return;
    };
    let (Some(notice), Some(root)) = (PaneId::First.notice(), PaneId::First.root()) else {
        return;
    };
    let Some(main) = root.parent_element() else {
        return;
    };
    surfaces::let_go_of_place();
    CODES.with_borrow_mut(|codes| codes[PaneId::Second] = String::new());
    let code = code(PaneId::First);
    surfaces::move_place(PaneId::First, &code, || {
        SLOTS.set(Some((notice.next_sibling(), root.next_sibling())));
        let first = &made.columns[PaneId::First];
        let _ = notice.before_with_node_1(first);
        let _ = first.append_with_node_2(&notice, &root);
        let _ = main.append_with_node_1(&made.columns[PaneId::Second]);
        SPLIT.set(true);
        if let Some(html) = bridge::document().and_then(|d| d.document_element()) {
            let _ = html.set_attribute("data-split", "on");
        }
        mark_active();
        reader::redraw(PaneId::First);
    });
    crate::aa::split_changed();
}

/// Closes the second pane: the page is as it was before it opened, the
/// first pane active, its place moved back to the window (the same verse
/// at the same height), and focus on its reference button. Nothing of the
/// second pane is kept.
pub fn close_second() {
    if !split() {
        return;
    }
    let Some(made) = made() else {
        return;
    };
    set_active(PaneId::First);
    // The translation popover, opened from a bar that's going.
    if surfaces::current() == Some(Surface::Translation) {
        surfaces::close(false);
    }
    let (Some(notice), Some(root)) = (PaneId::First.notice(), PaneId::First.root()) else {
        return;
    };
    let Some(main) = made.columns[PaneId::First].parent_element() else {
        return;
    };
    let code = code(PaneId::First);
    surfaces::move_place(PaneId::First, &code, || {
        let (notice_slot, root_slot) = SLOTS.take().unwrap_or_default();
        let _ = main.insert_before(&notice, notice_slot.as_ref());
        let _ = main.insert_before(&root, root_slot.as_ref());
        made.columns[PaneId::First].remove();
        made.columns[PaneId::Second].remove();
        made.root.set_inner_html("");
        made.notice.set_text_content(None);
        made.notice.set_hidden(true);
        SPLIT.set(false);
        if let Some(html) = bridge::document().and_then(|d| d.document_element()) {
            let _ = html.remove_attribute("data-split");
        }
        reader::forget(PaneId::Second);
        crate::card::forget(PaneId::Second);
        crate::findings::forget(PaneId::Second);
        reader::redraw(PaneId::First);
    });
    crate::aa::split_changed();
    if let Some(reference) = by_id("ref-btn") {
        dom::focus(&reference);
    }
}

// ---- The columns and their bars.

/// Makes the columns, the second pane's notice and root, and the bars, and
/// listens to them, once.
fn make() -> Option<Rc<Made>> {
    let document = bridge::document()?;
    let element = |tag: &str, class: &str| {
        let el = document
            .create_element(tag)
            .ok()?
            .dyn_into::<HtmlElement>()
            .ok()?;
        el.set_class_name(class);
        Some(el)
    };
    let mut columns = Vec::new();
    let mut bars = Vec::new();
    for pane in [PaneId::First, PaneId::Second] {
        let column = element("section", "pane")?;
        let _ = column.set_attribute("data-pane", pane.name());
        let bar = bar(&element, pane)?;
        let _ = column.append_with_node_1(&bar);
        listen_to_column(pane, &column);
        columns.push(column);
        bars.push(bar);
    }
    let notice = element("p", "reader-notice")?;
    notice.set_id("reader-notice-2");
    let _ = notice.set_attribute("role", "status");
    notice.set_hidden(true);
    let root = document
        .create_element("div")
        .ok()?
        .dyn_into::<HtmlElement>()
        .ok()?;
    root.set_id("reading-pane-2");
    let _ = columns[1].append_with_node_2(&notice, &root);
    observe_root(&root);
    let [first, second]: [HtmlElement; 2] = columns.try_into().ok()?;
    let [first_bar, second_bar]: [HtmlElement; 2] = bars.try_into().ok()?;
    let made = Rc::new(Made {
        columns: PerPane::new(|pane| match pane {
            PaneId::First => first.clone(),
            PaneId::Second => second.clone(),
        }),
        bars: PerPane::new(|pane| match pane {
            PaneId::First => first_bar.clone(),
            PaneId::Second => second_bar.clone(),
        }),
        notice,
        root,
    });
    MADE.set(Some(made.clone()));
    reader::listen_to_notice(PaneId::Second);
    Some(made)
}

/// A copy of the top bar's control `id`: the same markup and icon, with no
/// ids (the top bar's stay unique) and `data-pane` instead.
fn copy_of(id: &str, pane: PaneId) -> Option<HtmlElement> {
    let copy = by_id(id)?
        .clone_node_with_deep(true)
        .ok()?
        .dyn_into::<HtmlElement>()
        .ok()?;
    copy.remove_attribute("id").ok()?;
    let inner = copy.query_selector_all("[id]").ok()?;
    for el in dom::elements(&inner) {
        let _ = el.remove_attribute("id");
    }
    let _ = copy.set_attribute("data-pane", pane.name());
    Some(copy)
}

/// The pane's bar (Variant A, S5): ‹, the reference ▾, ›, and the
/// translation button, and on the second pane × too. Each control makes its
/// pane active, then does what the top bar's does.
fn bar(element: &impl Fn(&str, &str) -> Option<HtmlElement>, pane: PaneId) -> Option<HtmlElement> {
    let bar = element("div", "pane-bar")?;
    let _ = bar.set_attribute("role", "toolbar");
    let _ = bar.set_attribute("aria-label", text::bar_label(pane == PaneId::First));
    let _ = bar.set_attribute("data-pane", pane.name());
    let prev = copy_of("chapter-prev", pane)?;
    let reference = copy_of("ref-btn", pane)?;
    let next = copy_of("chapter-next", pane)?;
    let spacer = element("span", "pane-bar-spacer")?;
    let item = element("div", "bar-item")?;
    let translation = copy_of("translation-toggle", pane)?;
    let _ = item.append_with_node_1(&translation);
    let _ = bar.append_with_node_5(&prev, &reference, &next, &spacer, &item);
    for (button, which) in [(prev, Which::Prev), (next, Which::Next)] {
        let target = button.clone();
        dom::on_click(&button, move |_| {
            set_active(pane);
            if target.get_attribute("aria-disabled").as_deref() != Some("true") {
                keyboard::turn_chapter(which);
            }
        });
    }
    let opener = reference.clone();
    dom::on_click(&reference, move |_| {
        set_active(pane);
        surfaces::open_picker_from(true, Some(opener.clone()));
    });
    dom::on_click(&translation.clone(), move |_| {
        set_active(pane);
        toggle_translation_from(&item, &translation);
    });
    if pane == PaneId::Second {
        let close = element("button", "bar-btn pane-close")?;
        let _ = close.set_attribute("type", "button");
        let _ = close.set_attribute("aria-label", text::CLOSE_SECOND);
        let _ = close.set_attribute("title", text::CLOSE_SECOND);
        close.set_inner_html(
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true" focusable="false"><path d="M6 6l12 12M18 6 6 18"/></svg>"#,
        );
        dom::on_click(&close, |_| close_second());
        let _ = bar.append_with_node_1(&close);
    }
    Some(bar)
}

/// A column scrolled: the reader saves its place (the first pane's), and
/// the reading place logic notes the layout.
fn listen_to_column(pane: PaneId, column: &HtmlElement) {
    let scrolled = Closure::<dyn Fn()>::new(move || {
        reader::scrolled(pane);
        surfaces::note_painted_place();
    });
    let passive = AddEventListenerOptions::new();
    passive.set_passive(true);
    let _ = column.add_event_listener_with_callback_and_add_event_listener_options(
        "scroll",
        scrolled.as_ref().unchecked_ref(),
        &passive,
    );
    scrolled.forget();
}

/// The second pane's chapter reflowing (drawn, its fonts in) changes the
/// layout the reading place logic keeps (`surfaces` watches the first's).
fn observe_root(root: &HtmlElement) {
    let Some(window) = web_sys::window() else {
        return;
    };
    if !js_sys::Reflect::has(&window, &JsValue::from_str("ResizeObserver")).unwrap_or(false) {
        return;
    }
    let noted = Closure::<dyn Fn()>::new(surfaces::note_painted_place);
    if let Ok(observer) = ResizeObserver::new(noted.as_ref().unchecked_ref()) {
        observer.observe(root);
    }
    noted.forget();
}

/// The translation popover, opened from (or closed by) a pane's button:
/// hung under it while open (`item`, the button's own `.bar-item`), and
/// handed back to the top bar as it closes ([`surface_closed`]).
fn toggle_translation_from(item: &HtmlElement, button: &HtmlElement) {
    let Some(popover) = by_id("translation-popover") else {
        return;
    };
    let here = popover
        .parent_element()
        .is_some_and(|parent| dom::same(Some(&parent), item));
    if surfaces::current() == Some(Surface::Translation) && here {
        surfaces::close(true);
        return;
    }
    // Moved before it opens: moving it with focus inside would lose focus.
    if surfaces::current().is_some() {
        surfaces::close(false);
    }
    let _ = item.append_with_node_1(&popover);
    surfaces::open(Surface::Translation, Some(button.clone()));
    let _ = button.set_attribute("aria-expanded", "true");
}

/// T, and anything else that opens the translation popover without a
/// button: from the active pane's bar while it has the top bar's place (two
/// panes, at least [`text::BARS_QUERY`] wide), else from the top bar.
pub fn toggle_translation() {
    let pane = active();
    if bars_shown()
        && let (Some(item), Some(button)) =
            (pane.in_bar(".bar-item"), pane.in_bar(".translation-toggle"))
    {
        toggle_translation_from(&item, &button);
        return;
    }
    surfaces::toggle_surface(Surface::Translation, None);
}

/// G, and anything else that opens the picker without a button: from the
/// active pane's bar's reference button while it has the top bar's place
/// (as [`toggle_translation`]), so focus goes back to it.
pub fn open_picker() {
    let opener = bars_shown().then(|| active().in_bar(".ref-btn")).flatten();
    surfaces::open_picker_from(false, opener);
}

/// Whether the panes' bars have the top bar's place: two panes, at least
/// [`text::BARS_QUERY`] wide.
fn bars_shown() -> bool {
    split()
        && web_sys::window()
            .and_then(|w| w.match_media(text::BARS_QUERY).ok().flatten())
            .is_some_and(|query| query.matches())
}

/// A header surface has closed (`surfaces`): the translation popover goes
/// back under the top bar's button, and no bar's button says it's open.
pub fn surface_closed(surface: Surface) {
    if surface != Surface::Translation {
        return;
    }
    if let (Some(popover), Some(home)) = (
        by_id("translation-popover"),
        by_id("translation-toggle").and_then(|toggle| toggle.parent_element()),
    ) && !popover
        .parent_element()
        .is_some_and(|parent| dom::same(Some(&parent), &home))
    {
        let _ = home.append_with_node_1(&popover);
    }
    if let Some(made) = made() {
        for pane in [PaneId::First, PaneId::Second] {
            if let Ok(Some(button)) = made.bars[pane].query_selector(".translation-toggle") {
                let _ = button.set_attribute("aria-expanded", "false");
            }
        }
    }
}

/// The picker's shape changed (`surfaces::sync_picker`): each bar's
/// reference button says what the top bar's does (it opens the same
/// picker).
pub fn picker_synced(reference: &HtmlElement) {
    let Some(made) = made() else {
        return;
    };
    for pane in [PaneId::First, PaneId::Second] {
        if let Ok(Some(button)) = made.bars[pane].query_selector(".ref-btn") {
            for name in ["aria-expanded", "aria-haspopup"] {
                match reference.get_attribute(name) {
                    Some(value) => {
                        let _ = button.set_attribute(name, &value);
                    }
                    None => {
                        let _ = button.remove_attribute(name);
                    }
                }
            }
        }
    }
}

/// The pane's bar says its place: the reference (`markup`, named `label`).
pub fn show_reference(pane: PaneId, markup: &str, label: &str) {
    if let Some(text) = pane.in_bar(".ref-text") {
        text.set_inner_html(markup);
    }
    if let Some(button) = pane.in_bar(".ref-btn") {
        let _ = button.set_attribute("aria-label", label);
    }
}

/// The pane's bar says its translation: `label` on the button, named
/// `name`.
pub fn show_translation(pane: PaneId, label: &str, name: &str) {
    if let Some(button) = pane.in_bar(".translation-toggle") {
        button.set_text_content(Some(label));
        let _ = button.set_attribute("aria-label", name);
    }
}

/// The pane's bar's ‹ and › say where they go (`prev`, `next`: the
/// chapters' names), as the top bar's do.
pub fn show_steps(pane: PaneId, prev: Option<&str>, next: Option<&str>) {
    let Some(made) = made().filter(|_| split()) else {
        return;
    };
    let Ok(arrows) = made.bars[pane].query_selector_all(".chapter-arrow") else {
        return;
    };
    for (arrow, (which, to)) in dom::elements(&arrows)
        .iter()
        .zip([(Which::Prev, prev), (Which::Next, next)])
    {
        crate::nav::set_button_on(arrow, &nav_text::chapter_button(which, to));
    }
}

// ---- The keys that scroll a page.

/// What Space and the arrows belong to, when they're aimed at it.
const CONTROLS: &str = "button, a[href], input, select, textarea, [contenteditable], \
     [role=\"button\"], [role=\"menuitem\"], [role=\"tab\"], [role=\"switch\"]";

/// A key that scrolls the page (Page Down, Space, Home…), while two panes
/// are open: the active pane's column scrolls, here, rather than whatever
/// the webview would pick (UNVERIFIED 1 in the design: with focus on the
/// page itself, which of two columns either picks, if any). Only for a key
/// aimed at the page or inside a pane, not at a popover or the card in it,
/// nor anything else that scrolls itself. Whether it was one, its
/// `preventDefault` given. With one pane the browser scrolls, as ever.
pub fn on_scroll_key(event: &KeyboardEvent) -> bool {
    if !split() {
        return false;
    }
    let Some(target) = event.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
        return false;
    };
    let page = bridge::document()
        .and_then(|d| d.body())
        .is_some_and(|body| dom::same(Some(&target), &body))
        || bridge::document()
            .and_then(|d| d.document_element())
            .is_some_and(|html| dom::same(Some(&target), &html));
    let in_pane = of(&target).is_some()
        && target
            .closest(".popover, #selection-card")
            .ok()
            .flatten()
            .is_none();
    if !page && !in_pane {
        return false;
    }
    let on_control = target.closest(CONTROLS).ok().flatten().is_some();
    let Some(key) = text::scroll_key(
        &event.key(),
        event.shift_key(),
        event.ctrl_key() || event.meta_key() || event.alt_key(),
        on_control,
    ) else {
        return false;
    };
    event.prevent_default();
    let scroller = active().scroller();
    let height = scroller
        .view()
        .map_or(0.0, |view| (view.bottom - view.top).max(0.0));
    match text::scroll_move(key, height) {
        ScrollMove::By(dy) => scroller.scroll_by(dy),
        ScrollMove::Top => scroller.scroll_to(0.0),
        ScrollMove::Bottom => {
            if let Scroller::Element(el) = &scroller {
                scroller.scroll_to(f64::from(el.scroll_height()));
            }
        }
    }
    true
}

// ---- Mounting.

/// `window.gospelGetterPanes`, for the frontend tests and the GUI checks,
/// and until the reader has a way in of its own, the only way into two
/// panes: `openBeside({ bookId, chapter, verse, translation })` (a promise
/// of whether the chapter was put on screen; `verse` and `translation`
/// optional, the active pane's translation by default) opens the passage in
/// the second pane, opening it if it isn't, and makes it active (S12,
/// S13); `close()` closes it; `active()` and `shown()` name the active
/// pane and the panes on screen (`"first"`, `"second"`).
fn export_panes() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let global = js_sys::Object::new();
    let number = |args: &JsValue, key: &str| {
        js_sys::Reflect::get(args, &JsValue::from_str(key))
            .ok()
            .and_then(|v| v.as_f64())
            .map(|n| n as i64)
    };
    let open_beside = Closure::<dyn Fn(JsValue) -> js_sys::Promise>::new(move |args: JsValue| {
        let (Some(book_id), Some(chapter)) = (number(&args, "bookId"), number(&args, "chapter"))
        else {
            return js_sys::Promise::resolve(&JsValue::FALSE);
        };
        let verse = number(&args, "verse");
        let code = js_sys::Reflect::get(&args, &JsValue::from_str("translation"))
            .ok()
            .and_then(|v| v.as_string());
        let passage = OpenPassage {
            book_id,
            chapter,
            verse_start: verse,
            verse_end: verse,
        };
        let load = reader::open_beside(&passage, code);
        wasm_bindgen_futures::future_to_promise(async move { Ok(JsValue::from_bool(load.await)) })
    });
    let close = Closure::<dyn Fn()>::new(close_second);
    let active_name = Closure::<dyn Fn() -> JsValue>::new(|| JsValue::from_str(active().name()));
    let shown_names = Closure::<dyn Fn() -> JsValue>::new(|| {
        shown()
            .map(|pane| JsValue::from_str(pane.name()))
            .collect::<js_sys::Array>()
            .into()
    });
    for (name, f) in [
        ("openBeside", open_beside.as_ref()),
        ("close", close.as_ref()),
        ("active", active_name.as_ref()),
        ("shown", shown_names.as_ref()),
    ] {
        let _ = js_sys::Reflect::set(&global, &JsValue::from_str(name), f);
    }
    let _ = js_sys::Reflect::set(&window, &JsValue::from_str("gospelGetterPanes"), &global);
    open_beside.forget();
    close.forget();
    active_name.forget();
    shown_names.forget();
}

/// Makes a pane active as a pointer is pressed or focus moves into it
/// (D8), and puts `window.gospelGetterPanes` where the tests read it.
pub fn mount() {
    let Some(document) = bridge::document() else {
        return;
    };
    for name in ["pointerdown", "focusin"] {
        dom::on(&document, name, |event| {
            if let Some(pane) = event
                .target()
                .and_then(|t| t.dyn_into::<Element>().ok())
                .and_then(|el| of(&el))
            {
                set_active(pane);
            }
        });
    }
    export_panes();
}
