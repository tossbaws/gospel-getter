//! The header's popovers and panels, and the book and chapter picker's two
//! shapes: what opens and closes them. The surfaces are the translation
//! button's popover, Aa, the Library, the ☰ menu, About, and the picker
//! when it's a popover (`surfaces_text::Surface`). Only one is open at a
//! time; Escape closes it, and so does a click outside a popover (the
//! translation's, Aa's, the menu or the picker). Opening one moves focus
//! into it, and closing it gives focus back to the button that opened it.
//!
//! The picker is a sidebar down the left on a window at least 1200px wide
//! (`preferences::WIDE_QUERY`), unless the reader hid it or is in reading
//! mode, and a popover from the reference button otherwise. At 1200px the
//! sidebar (`--sidebar-width`, 264px) and the page's gutters leave 888px
//! for the chapter and its two faded neighbors: a reading column of about
//! 51 characters at the default size (55 in the default 1280px window,
//! which gets the sidebar; 62 from 1440px up, as without it). Any narrower
//! and the column drops below 50. The page's pre-paint script sets
//! `html[data-wide]` and `html[data-picker]` before the first frame, so
//! the page is laid out for it from the start; this keeps them from then
//! on.
//!
//! What's inside them is the other pieces': Aa and the translations
//! (`aa`, drawn here as they open), the Library's tabs and lists
//! (`library`), the picker's books and chapters (`picker`), About
//! (`about`, whose own keys close it through here). The ☰ menu's items
//! are the page's, and this crate's for About.
//!
//! The page (its reader, still JavaScript) reaches them through
//! `window.gospelGetterSurfaces` ([`export_surfaces`]), and hears them open
//! and close (`bridge::SURFACE_OPEN_EVENT`, `bridge::SURFACE_CLOSE_EVENT`).
//! Without this module, nothing opens, and the page says so.
//!
//! This is the head script's surfaces controller (`ui/index.html` at
//! 49736df, from `isWide` to its Escape handler) in Rust: the same ids,
//! ARIA, focus order and keys. Everything happens within the click or key,
//! as it did. Since then, main's #116, #118 and #121 (D14): a resize across
//! 1200px keeps the reading place the reader last saw ([`on_shape_change`],
//! [`note_painted_place`]), even right after a scroll, and in Chromium,
//! whatever place its scroll anchoring kept ([`SCROLL_BEFORE_RESIZE`]).

use std::cell::{Cell, RefCell};

use wasm_bindgen::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    AddEventListenerOptions, CustomEvent, Element, HtmlElement, KeyboardEvent, MediaQueryList,
    Node, ResizeObserver,
};

use crate::aa;
use crate::bridge;
use crate::dom;
use crate::preferences;
use crate::prefs;
pub use crate::surfaces_text::Surface;
use crate::surfaces_text::{self as text, MenuKey};

thread_local! {
    /// The surface that's open.
    static CURRENT: Cell<Option<Surface>> = const { Cell::new(None) };
    /// Where focus goes back to as it closes.
    static RETURN_FOCUS: RefCell<Option<HtmlElement>> = const { RefCell::new(None) };
    /// The picker is open as the popover.
    static PICKER_POPOVER_OPEN: Cell<bool> = const { Cell::new(false) };
    /// `matchMedia(WIDE_QUERY)`: is there room for the sidebar?
    static WIDE: RefCell<Option<MediaQueryList>> = const { RefCell::new(None) };
    /// The page as last painted ([`note_painted_place`]).
    static PAINTED_LAYOUT: RefCell<Option<PaintedLayout>> = const { RefCell::new(None) };
    /// A frame is asked for, to note the painted layout.
    static LAYOUT_PENDING: Cell<bool> = const { Cell::new(false) };
    /// The scroll position the page's last `scroll` event found.
    static SCROLL_SEEN: Cell<f64> = const { Cell::new(0.0) };
    /// [`SCROLL_SEEN`] as the last resize's event came, before the scroll
    /// event of anything that scrolled for it ([`on_shape_change`]).
    static SCROLL_BEFORE_RESIZE: Cell<Option<f64>> = const { Cell::new(None) };
}

// ---- The page's elements.

fn by_id(id: &str) -> Option<HtmlElement> {
    bridge::document()?
        .get_element_by_id(id)?
        .dyn_into::<HtmlElement>()
        .ok()
}

/// The surface's element. About's is looked up whenever it's needed.
fn element(surface: Surface) -> Option<HtmlElement> {
    by_id(surface.element_id())
}

fn toggle_of(surface: Surface) -> Option<HtmlElement> {
    surface.toggle_id().and_then(by_id)
}

/// An attribute of `<html>`.
fn root_data(name: &str) -> Option<String> {
    bridge::document()?.document_element()?.get_attribute(name)
}

fn is_reader_mode() -> bool {
    root_data("data-reader-mode").as_deref() == Some("on")
}

fn is_wide() -> bool {
    WIDE.with_borrow(|wide| wide.as_ref().is_some_and(MediaQueryList::matches))
}

/// The sidebar shows on a wide window unless the reader hid it; reading
/// mode never shows it.
fn sidebar_shown() -> bool {
    text::sidebar_shown(
        is_wide(),
        root_data("data-sidebar").as_deref(),
        root_data("data-reader-mode").as_deref(),
    )
}

/// On the page and shown: somewhere focus can go back to.
fn shown(el: &HtmlElement) -> bool {
    dom::usable(el)
        && web_sys::window()
            .and_then(|w| w.get_computed_style(el).ok().flatten())
            .and_then(|style| style.get_property_value("display").ok())
            .is_none_or(|display| display != "none")
}

/// The element that has focus: what a focus that was inside a surface
/// would be.
fn active() -> Option<Element> {
    dom::active_element()
}

fn contains(el: &Element, node: Option<&Node>) -> bool {
    node.is_some() && el.contains(node)
}

// ---- The picker's two shapes.

/// On `<html>`, for the CSS: `data-wide` while there's room for the
/// sidebar, and `data-picker="sidebar"` while it shows (the page makes room
/// for it).
fn apply_picker_shape() {
    let Some(root) = bridge::document().and_then(|d| d.document_element()) else {
        return;
    };
    if is_wide() {
        let _ = root.set_attribute("data-wide", "");
    } else {
        let _ = root.remove_attribute("data-wide");
    }
    if sidebar_shown() {
        let _ = root.set_attribute("data-picker", "sidebar");
    } else {
        let _ = root.remove_attribute("data-picker");
    }
}

/// The picker takes the shape the window, reading mode and the sidebar
/// setting call for. As the sidebar it's a navigation landmark, shown
/// while [`sidebar_shown`]; as the popover it's a dialog, shown while open.
/// The reference button and the sidebar's own button say which. As it
/// appears, the picker (`picker`) scrolls to the book being read.
pub fn sync_picker() {
    apply_picker_shape();
    let (Some(picker), Some(ref_btn), Some(sidebar_toggle)) =
        (by_id("picker"), by_id("ref-btn"), by_id("sidebar-toggle"))
    else {
        return;
    };
    let sidebar = sidebar_shown();
    let was_hidden = picker.hidden();
    picker.set_hidden(!(sidebar || PICKER_POPOVER_OPEN.get()));
    if sidebar {
        let _ = picker.remove_attribute("role");
        let _ = ref_btn.remove_attribute("aria-haspopup");
    } else {
        let _ = picker.set_attribute("role", "dialog");
        let _ = ref_btn.set_attribute("aria-haspopup", "dialog");
    }
    let _ = ref_btn.set_attribute("aria-expanded", &(!picker.hidden()).to_string());
    let _ = sidebar_toggle.set_attribute("aria-expanded", &sidebar.to_string());
    if was_hidden
        && !picker.hidden()
        && let (Some(document), Ok(event)) = (
            bridge::document(),
            CustomEvent::new(bridge::PICKER_SHOWN_EVENT),
        )
    {
        let _ = document.dispatch_event(&event);
    }
}

/// What a reading place can be: a heading or verse of the chapter being
/// read, or a gap in compare's columns.
const PLACE_MARKS: &str =
    ".chapter-main .chapter-heading, .chapter-main .verse[data-verse], .chapter-main .compare-gap";

/// The marks a reading place can be, in order.
fn place_marks() -> Vec<HtmlElement> {
    bridge::document()
        .and_then(|document| document.query_selector_all(PLACE_MARKS).ok())
        .map(|found| dom::elements(&found))
        .unwrap_or_default()
}

/// The reader's place: the first heading or verse of the chapter being
/// read that reaches below the bar, and where it is on screen.
#[derive(Clone)]
pub struct Place {
    mark: HtmlElement,
    top: f64,
}

/// The page as last painted: where each mark [`reading_place`] looks at is
/// in the page (its top and bottom; scrolling doesn't move it there), and
/// where the bar ends on screen.
struct PaintedLayout {
    bar_bottom: f64,
    marks: Vec<HtmlElement>,
    /// Each mark's top and bottom in the page, in order.
    at: Vec<(f64, f64)>,
}

/// The reader's place as the page is laid out now, if anything of the
/// chapter reaches below the bar.
fn reading_place() -> Option<Place> {
    let bar = by_id("top-bar")?;
    let bar_bottom = bar.get_bounding_client_rect().bottom();
    let marks = place_marks();
    // Each mark's box is read once, up to the one kept.
    let mut top = 0.0;
    let index = text::first_below(
        bar_bottom,
        marks.iter().map(|mark| {
            let rect = mark.get_bounding_client_rect();
            top = rect.top();
            rect.bottom()
        }),
    )?;
    Some(Place {
        mark: marks[index].clone(),
        top,
    })
}

/// Notes the page as painted ([`PaintedLayout`]), in the animation frame
/// after anything that can move it (one frame asked for at most). By then
/// that frame's resize and media query listeners have run, so
/// [`on_shape_change`] still finds the layout from before the change.
fn note_painted_place() {
    if LAYOUT_PENDING.replace(true) {
        return;
    }
    let noted = Closure::once_into_js(|| {
        LAYOUT_PENDING.set(false);
        let (Some(window), Some(bar)) = (web_sys::window(), by_id("top-bar")) else {
            return;
        };
        let y = window.scroll_y().unwrap_or(0.0);
        let marks = place_marks();
        let at = marks
            .iter()
            .map(|mark| {
                let rect = mark.get_bounding_client_rect();
                (rect.top() + y, rect.bottom() + y)
            })
            .collect();
        PAINTED_LAYOUT.set(Some(PaintedLayout {
            bar_bottom: bar.get_bounding_client_rect().bottom(),
            marks,
            at,
        }));
    });
    let asked = web_sys::window().is_some_and(|window| {
        window
            .request_animation_frame(noted.unchecked_ref())
            .is_ok()
    });
    if !asked {
        LAYOUT_PENDING.set(false);
    }
}

/// The reader's place in the painted layout scrolled to `y`, as
/// [`reading_place`] would have measured it there; none with no layout
/// painted yet, or a place no longer on the page.
fn painted_place_at(y: f64) -> Option<Place> {
    PAINTED_LAYOUT.with_borrow(|layout| {
        let layout = layout.as_ref()?;
        let (index, top) = text::place_at(layout.bar_bottom, &layout.at, y)?;
        let mark = &layout.marks[index];
        mark.is_connected().then(|| Place {
            mark: mark.clone(),
            top,
        })
    })
}

/// Whether the browser scrolls to keep a place of its own when the window
/// is resized: Chromium's scroll anchoring does (WebView2, on Windows);
/// WebKitGTK's doesn't. Both say they support `overflow-anchor`, so it's
/// told by Chromium's own `window.chrome`. Its place needn't be the
/// reader's: it can be a line of the faded chapter beside, which comes
/// first in the page.
fn anchors_on_resize() -> bool {
    web_sys::window()
        .and_then(|window| js_sys::Reflect::get(&window, &JsValue::from_str("chrome")).ok())
        .is_some_and(|chrome| chrome.is_truthy())
}

/// Runs `change` (which reflows the page) keeping the reader's place: the
/// first heading or verse of the chapter being read that reaches below the
/// bar stays where it was on screen, or where `place` says it was.
pub fn keep_reading_place(change: impl FnOnce(), place: Option<Place>) {
    let place = place.or_else(reading_place);
    change();
    if let Some(place) = place
        && let Some(shift) =
            text::scroll_shift(place.top, place.mark.get_bounding_client_rect().top())
        && let Some(window) = web_sys::window()
    {
        window.scroll_by_with_x_and_y(0.0, shift);
    }
    note_painted_place();
}

/// Shows or hides the sidebar, and remembers it (the saved settings').
fn set_sidebar(open: bool) {
    keep_reading_place(
        || {
            prefs::set(&preferences::SIDEBAR, if open { "open" } else { "closed" });
            sync_picker();
        },
        None,
    );
}

/// The reference button, or G: on a wide window, into the sidebar (showing
/// it, if it was hidden); otherwise the popover, which the button also
/// closes (`toggle`). Focus goes to the chapter being read. The books are
/// `picker`'s: until boot has handed them over, it says they aren't there.
fn open_picker(toggle: bool) {
    if is_wide() && !is_reader_mode() {
        if CURRENT.get().is_some_and(Surface::popover) {
            close(false);
        }
        if !sidebar_shown() {
            set_sidebar(true);
        }
        focus_into(Surface::Picker);
    } else if toggle {
        toggle_surface(Surface::Picker, None);
    } else if CURRENT.get() != Some(Surface::Picker) {
        open(Surface::Picker, None);
    }
    if let Some(picker) = by_id("picker")
        && !picker.hidden()
        && picker.query_selector(".book-item").ok().flatten().is_none()
    {
        bridge::toast("The book list isn\u{2019}t available.", true);
    }
}

/// A window resized across the breakpoint changes shape: an open popover
/// closes, and focus in a sidebar that's gone goes to the reference button.
/// The reading place stays, as with the sidebar's own button. By now the
/// page is laid out at the new width in the old shape, which the reader
/// never saw (and which can have another verse first under the bar).
/// The place is the painted layout's ([`painted_place_at`]) at the scroll
/// position the reader had. WebKitGTK has left the scroll position alone,
/// so that's the one now: right even when the page was scrolled after the
/// last frame, in the same one as the resize, which a place noted in that
/// frame would miss. Chromium's anchoring has already scrolled to keep a
/// place of its own ([`anchors_on_resize`]), as it laid the page out at
/// the new width (in WebView2 before the resize's event), so it's the one
/// the page's last `scroll` event found as the resize's event came
/// ([`SCROLL_BEFORE_RESIZE`]): the anchoring's own scroll event comes after
/// it. Measured now instead, the first verse under the bar would be
/// wherever Chromium's place left it. With no layout painted yet, or a
/// place no longer on the page, it's the place found now. The close is
/// inside [`keep_reading_place`] too: it syncs the picker itself, so
/// outside it the new shape would reflow the page before the place was
/// kept.
fn on_shape_change() {
    let Some(picker) = by_id("picker") else {
        return;
    };
    let focus_was_inside = contains(&picker, active().as_deref());
    let before_resize = SCROLL_BEFORE_RESIZE.take();
    let place = web_sys::window()
        .and_then(|window| window.scroll_y().ok())
        .map(|now| text::scroll_read_at(anchors_on_resize(), before_resize, now))
        .and_then(painted_place_at);
    keep_reading_place(
        || {
            if CURRENT.get() == Some(Surface::Picker) {
                close(false);
            }
            sync_picker();
        },
        place,
    );
    if focus_was_inside
        && picker.hidden()
        && let Some(ref_btn) = by_id("ref-btn")
    {
        let _ = ref_btn.focus();
    }
}

// ---- Opening and closing.

/// The surface that's open, if any.
pub fn current() -> Option<Surface> {
    CURRENT.get()
}

fn set_shown(surface: Surface, open: bool) {
    if surface == Surface::Picker {
        PICKER_POPOVER_OPEN.set(open);
        sync_picker();
        return;
    }
    if let Some(el) = element(surface) {
        el.set_hidden(!open);
    }
    if let Some(toggle) = toggle_of(surface) {
        let _ = toggle.set_attribute("aria-expanded", &open.to_string());
    }
}

/// Says `name` (`gospel-getter:surface-open` or `-close`) with the
/// surface's name.
fn announce(name: &str, surface: Surface) {
    #[derive(serde::Serialize)]
    struct Named {
        name: &'static str,
    }
    bridge::dispatch(
        name,
        &Named {
            name: surface.name(),
        },
    );
}

/// Closes whatever is open. Focus goes back to the button that opened it,
/// unless it has already moved on to something else (a dialog the surface
/// opened, say), or `restore_focus` is false.
pub fn close(restore_focus: bool) {
    let Some(closing) = CURRENT.get() else {
        return;
    };
    let target = RETURN_FOCUS.take();
    let active = active();
    let body = bridge::document().and_then(|d| d.body());
    let focus_was_inside = match &active {
        None => true,
        Some(active) => {
            body.as_ref()
                .is_some_and(|body| dom::same(Some(active), body))
                || element(closing).is_some_and(|el| el.contains(Some(active)))
        }
    };
    set_shown(closing, false);
    CURRENT.set(None);
    if restore_focus
        && focus_was_inside
        && let Some(target) = target.filter(shown)
    {
        let _ = target.focus();
    }
    // The update banner comes back if About closes on an install
    // (`updates`).
    announce(bridge::SURFACE_CLOSE_EVENT, closing);
}

/// Moves focus into `surface`, by its first-focus rule.
fn focus_into(surface: Surface) {
    let Some(el) = element(surface) else {
        return;
    };
    let first = surface
        .first_focus()
        .iter()
        .find_map(|selector| el.query_selector(selector).ok().flatten())
        .and_then(|first| first.dyn_into::<HtmlElement>().ok());
    if let Some(first) = first {
        let _ = first.focus();
    }
}

/// Opens `surface`, closing what was open. `opener` is the control focus
/// returns to on closing: the surface's own button unless given (About,
/// opened from the ☰ menu, returns to ☰).
pub fn open(surface: Surface, opener: Option<HtmlElement>) {
    // The Library's button is hidden in reading mode.
    if surface == Surface::Library && is_reader_mode() {
        return;
    }
    if element(surface).is_none() {
        return;
    }
    if CURRENT.get().is_some() {
        close(false);
    }
    CURRENT.set(Some(surface));
    RETURN_FOCUS.set(opener.or_else(|| toggle_of(surface)));
    // What's in them is drawn as they open, before focus moves in.
    aa::draw(surface);
    set_shown(surface, true);
    // Search closes (`search`).
    announce(bridge::SURFACE_OPEN_EVENT, surface);
    focus_into(surface);
}

pub fn toggle_surface(surface: Surface, opener: Option<HtmlElement>) {
    if CURRENT.get() == Some(surface) {
        close(true);
    } else {
        open(surface, opener);
    }
}

// ---- The ☰ menu: a menu, so ↑ and ↓ (and Home and End) move between its
// items, Enter or Space chooses one, and Escape or Tab closes it. Choosing
// an item closes it too.

fn menu_items() -> Vec<HtmlElement> {
    element(Surface::Menu)
        .and_then(|menu| menu.query_selector_all(text::MENU_ITEMS).ok())
        .map(|found| dom::elements(&found))
        .unwrap_or_default()
}

fn on_menu_key(event: &KeyboardEvent) {
    let items = menu_items();
    let active = active();
    let index = items
        .iter()
        .position(|item| dom::same(active.as_ref(), item));
    match text::menu_key(&event.key(), index, items.len()) {
        MenuKey::Move(to) => {
            event.prevent_default();
            event.stop_propagation();
            let _ = items[to].focus();
        }
        MenuKey::Nowhere => {
            event.prevent_default();
            event.stop_propagation();
        }
        MenuKey::Close => {
            event.prevent_default();
            close(true);
        }
        // Not a chapter turn while in the menu.
        MenuKey::Keep => event.stop_propagation(),
        MenuKey::Pass => {}
    }
}

/// ↓ or ↑ on ☰ opens the menu at its first or last item.
fn on_menu_toggle_key(event: &KeyboardEvent) {
    let key = event.key();
    if key != "ArrowDown" && key != "ArrowUp" {
        return;
    }
    event.prevent_default();
    if CURRENT.get() != Some(Surface::Menu) {
        open(Surface::Menu, None);
    }
    let items = menu_items();
    let item = if key == "ArrowDown" {
        items.first()
    } else {
        items.last()
    };
    if let Some(item) = item {
        let _ = item.focus();
    }
}

// ---- Listening.

/// Escape backs out one step at a time: first the search panel, then
/// whichever popover or panel is open, then a verse selection, then reading
/// mode. The page's keyboard dispatcher runs first (it's registered before
/// this module loads) and marks an Escape that closed search or cleared a
/// verse selection as handled; About's own Escape is handled on it.
fn on_escape(event: &KeyboardEvent) {
    if event.key() != "Escape" || event.default_prevented() {
        return;
    }
    if CURRENT.get().is_some() {
        event.prevent_default();
        close(true);
    } else if is_reader_mode() {
        aa::set_reader_mode(false);
    }
}

/// A click outside the open popover (and its button) closes it. The
/// Library stays open while the reader reads, and About is modal.
fn on_document_click(event: &web_sys::Event) {
    let Some(surface) = CURRENT.get().filter(|s| s.popover()) else {
        return;
    };
    let target = event.target().and_then(|t| t.dyn_into::<Node>().ok());
    let inside = |el: Option<HtmlElement>| el.is_some_and(|el| contains(&el, target.as_ref()));
    if inside(element(surface)) || inside(toggle_of(surface)) {
        return;
    }
    close(true);
}

fn on_click_of(id: &str, handler: impl Fn(&web_sys::MouseEvent) + 'static) {
    if let Some(el) = by_id(id) {
        dom::on_click(&el, move |event| handler(&event));
    }
}

fn on_key_of(id: &str, handler: impl Fn(&KeyboardEvent) + 'static) {
    if let Some(el) = by_id(id) {
        dom::on(&el, "keydown", move |event| {
            if let Some(event) = event.dyn_ref::<KeyboardEvent>() {
                handler(event);
            }
        });
    }
}

/// `window.gospelGetterSurfaces`, for the page's reader: `current()`, the
/// open surface's name or `null`; `close(restoreFocus)` (focus goes back
/// unless `restoreFocus` is `false`); `toggle(name)`, as its button; and
/// `openPicker()`, as G. Answered within the call.
fn export_surfaces() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let global = js_sys::Object::new();
    let current = Closure::<dyn Fn() -> JsValue>::new(|| {
        CURRENT
            .get()
            .map_or(JsValue::NULL, |s| JsValue::from_str(s.name()))
    });
    let close = Closure::<dyn Fn(JsValue)>::new(|restore_focus: JsValue| {
        close(restore_focus.as_bool() != Some(false));
    });
    let toggle = Closure::<dyn Fn(JsValue)>::new(|name: JsValue| {
        if let Some(surface) = name.as_string().as_deref().and_then(Surface::by_name) {
            toggle_surface(surface, None);
        }
    });
    let open_picker = Closure::<dyn Fn()>::new(|| open_picker(false));
    for (name, function) in [
        ("current", current.as_ref()),
        ("close", close.as_ref()),
        ("toggle", toggle.as_ref()),
        ("openPicker", open_picker.as_ref()),
    ] {
        let _ = js_sys::Reflect::set(&global, &JsValue::from_str(name), function);
    }
    let _ = js_sys::Reflect::set(&window, &JsValue::from_str("gospelGetterSurfaces"), &global);
    current.forget();
    close.forget();
    toggle.forget();
    open_picker.forget();
}

/// Notes the painted layout after a scroll, a resize, or a reflow of the
/// reading pane (a chapter put on screen, or reflowed: text size, fonts),
/// and once now. Each scroll's position is noted at once ([`SCROLL_SEEN`]),
/// and a resize notes it as its event comes ([`SCROLL_BEFORE_RESIZE`]).
fn watch_painted_place() {
    if let Some(window) = web_sys::window() {
        SCROLL_SEEN.set(window.scroll_y().unwrap_or(0.0));
        let scrolled = Closure::<dyn Fn()>::new(|| {
            if let Some(y) = web_sys::window().and_then(|window| window.scroll_y().ok()) {
                SCROLL_SEEN.set(y);
            }
            note_painted_place();
        });
        let passive = AddEventListenerOptions::new();
        passive.set_passive(true);
        let _ = window.add_event_listener_with_callback_and_add_event_listener_options(
            "scroll",
            scrolled.as_ref().unchecked_ref(),
            &passive,
        );
        scrolled.forget();
        let resized = Closure::<dyn Fn()>::new(|| {
            SCROLL_BEFORE_RESIZE.set(Some(SCROLL_SEEN.get()));
            note_painted_place();
        });
        let _ = window.add_event_listener_with_callback("resize", resized.as_ref().unchecked_ref());
        resized.forget();
        let noted = Closure::<dyn Fn()>::new(note_painted_place);
        if js_sys::Reflect::has(&window, &JsValue::from_str("ResizeObserver")).unwrap_or(false)
            && let (Some(pane), Ok(observer)) = (
                by_id("reading-pane"),
                ResizeObserver::new(noted.as_ref().unchecked_ref()),
            )
        {
            observer.observe(&pane);
        }
        noted.forget();
    }
    note_painted_place();
}

/// Takes the picker's shape, listens to the header's buttons, the menu's
/// keys, the window's width and Escape, and puts
/// `window.gospelGetterSurfaces` where the page reads it. Called first of
/// all the pieces, as the head script ran before them.
pub fn mount() {
    let Some(document) = bridge::document() else {
        return;
    };
    let wide =
        web_sys::window().and_then(|w| w.match_media(preferences::WIDE_QUERY).ok().flatten());
    if let Some(wide) = &wide {
        if js_sys::Reflect::has(wide, &JsValue::from_str("addEventListener")).unwrap_or(false) {
            dom::on(wide, "change", |_| on_shape_change());
        } else {
            let listener = Closure::<dyn Fn()>::new(on_shape_change);
            let _ = wide.add_listener_with_opt_callback(Some(listener.as_ref().unchecked_ref()));
            listener.forget();
        }
    }
    WIDE.set(wide);
    watch_painted_place();

    sync_picker();
    on_click_of("ref-btn", |_| open_picker(true));
    on_click_of("sidebar-toggle", |_| set_sidebar(!sidebar_shown()));
    for surface in [
        Surface::Translation,
        Surface::Aa,
        Surface::Library,
        Surface::Menu,
    ] {
        if let Some(id) = surface.toggle_id() {
            on_click_of(id, move |_| toggle_surface(surface, None));
        }
    }
    on_click_of("library-close", |_| close(true));
    on_click_of("about-btn", |_| {
        open(Surface::About, by_id("menu-toggle"));
    });
    dom::on(&document, "click", |event| on_document_click(&event));

    on_key_of("app-menu", on_menu_key);
    on_key_of("menu-toggle", on_menu_toggle_key);
    // Runs after the item's own action (that listener is on the item
    // itself), so an item that opened something else has already closed
    // the menu.
    on_click_of("app-menu", |event| {
        let item = event
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .and_then(|t| t.closest(r#"[role="menuitem"]"#).ok().flatten());
        if item.is_some() && CURRENT.get() == Some(Surface::Menu) {
            close(true);
        }
    });

    dom::on(&document, "keydown", |event| {
        if let Some(event) = event.dyn_ref::<KeyboardEvent>() {
            on_escape(event);
        }
    });

    export_surfaces();
}
