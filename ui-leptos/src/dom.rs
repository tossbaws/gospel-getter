//! Small DOM helpers the pieces share: focus that never scrolls the page,
//! "is this element usable", the Tab trap's arithmetic, listening for the
//! page's events and the keyboard, and timers. As the page's scripts do
//! them.

use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::Closure;
use web_sys::{CustomEvent, Element, FocusOptions, HtmlElement, KeyboardEvent, NodeList};

use crate::bridge;

/// `el.focus({ preventScroll: true })`: focus never scrolls the page.
pub fn focus(el: &HtmlElement) {
    let options = FocusOptions::new();
    options.set_prevent_scroll(true);
    let _ = el.focus_with_options(&options);
}

/// Inside something `hidden` (or `hidden` itself)?
pub fn inside_hidden(el: &Element) -> bool {
    matches!(el.closest("[hidden]"), Ok(Some(_)))
}

/// On the page and shown: somewhere focus can go back to.
pub fn usable(el: &Element) -> bool {
    el.is_connected() && !inside_hidden(el)
}

/// The same element?
pub fn same(a: Option<&Element>, b: &Element) -> bool {
    a.is_some_and(|a| {
        let a: &JsValue = a.as_ref();
        let b: &JsValue = b.as_ref();
        a == b
    })
}

/// The element that has focus, if any.
pub fn active_element() -> Option<Element> {
    web_sys::window()?.document()?.active_element()
}

/// `document.querySelector(selector)` as an `HtmlElement`.
pub fn query(selector: &str) -> Option<HtmlElement> {
    web_sys::window()?
        .document()?
        .query_selector(selector)
        .ok()??
        .dyn_into::<HtmlElement>()
        .ok()
}

/// The page's element `id`.
pub(crate) fn by_id(id: &str) -> Option<HtmlElement> {
    bridge::document()?
        .get_element_by_id(id)?
        .dyn_into::<HtmlElement>()
        .ok()
}

/// `el.closest(selector)`, if any.
pub(crate) fn closest(el: &Element, selector: &str) -> Option<Element> {
    el.closest(selector).ok().flatten()
}

/// A `data-*` number of `el`.
pub(crate) fn data_number(el: &Element, name: &str) -> Option<i64> {
    el.get_attribute(name)?.parse().ok()
}

/// The page's `localStorage`, if it has one.
pub(crate) fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

/// `window.innerHeight`, or 0 without a window.
pub(crate) fn window_height() -> f64 {
    web_sys::window()
        .and_then(|w| w.inner_height().ok())
        .and_then(|h| h.as_f64())
        .unwrap_or(0.0)
}

/// The HTML elements of a `NodeList`, in order.
pub fn elements(list: &NodeList) -> Vec<HtmlElement> {
    (0..list.length())
        .filter_map(|i| list.item(i))
        .filter_map(|node| node.dyn_into::<HtmlElement>().ok())
        .collect()
}

/// The elements `selector` finds in `root` that are enabled and shown.
pub fn focusable(root: &Element, selector: &str) -> Vec<HtmlElement> {
    let Ok(found) = root.query_selector_all(selector) else {
        return Vec::new();
    };
    elements(&found)
        .into_iter()
        .filter(|el| !el.has_attribute("disabled") && !inside_hidden(el))
        .collect()
}

/// As [`focusable`], leaving out what is taken out of the tab order
/// (`tabindex="-1"`).
pub fn tab_stops(root: &Element, selector: &str) -> Vec<HtmlElement> {
    focusable(root, selector)
        .into_iter()
        .filter(|el| el.tab_index() >= 0)
        .collect()
}

/// The modals' Tab trap: Tab and Shift+Tab wrap from the last stop to the
/// first and back, and focus that got outside `panel` comes back to the
/// first (or, with Shift, the last). In between, Tab does what it does.
pub fn trap_tab(event: &KeyboardEvent, panel: &Element, stops: &[HtmlElement]) {
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
        return;
    };
    let active = active_element();
    let inside = active
        .as_ref()
        .is_some_and(|active| panel.contains(Some(active)));
    if !inside {
        event.prevent_default();
        focus(if event.shift_key() { last } else { first });
    } else if event.shift_key() && same(active.as_ref(), first) {
        event.prevent_default();
        focus(last);
    } else if !event.shift_key() && same(active.as_ref(), last) {
        event.prevent_default();
        focus(first);
    }
}

/// Listens for the page's `name` event on `document`, for as long as the
/// page lives.
pub fn on_document_event(name: &str, handler: impl FnMut(CustomEvent) + 'static) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let handler = Closure::<dyn FnMut(CustomEvent)>::new(handler);
    let _ = document.add_event_listener_with_callback(name, handler.as_ref().unchecked_ref());
    handler.forget();
}

/// Listens for `click` on `el` itself, for as long as the page lives. The
/// handler may run again inside itself (it clicks something within `el`,
/// whose click comes back up to it), which a Leptos `on:click` can't.
pub fn on_click(el: &Element, handler: impl Fn(web_sys::MouseEvent) + 'static) {
    let handler = Closure::<dyn Fn(web_sys::MouseEvent)>::new(handler);
    let _ = el.add_event_listener_with_callback("click", handler.as_ref().unchecked_ref());
    handler.forget();
}

/// Listens for `name` on `target`, for as long as the page lives: a plain
/// listener (bubbling), which, as [`on_click`]'s, may run again inside
/// itself.
pub fn on(target: &web_sys::EventTarget, name: &str, handler: impl Fn(web_sys::Event) + 'static) {
    let handler = Closure::<dyn Fn(web_sys::Event)>::new(handler);
    let _ = target.add_event_listener_with_callback(name, handler.as_ref().unchecked_ref());
    handler.forget();
}

/// A callback for `setTimeout` ([`set_timeout`]) made once and kept for as
/// long as the page lives, as the listeners above are: one per timer that's
/// started again and again (a debounce, a fade), never one per start. A
/// timer cancelled or replaced before it fires then leaves nothing behind,
/// where a `Closure::once_into_js` handed to `setTimeout` is freed only
/// once it's called (AUD-06). What it does when it fires it reads then.
pub fn kept_callback(f: impl Fn() + 'static) -> js_sys::Function {
    let callback = Closure::<dyn Fn()>::new(f);
    let function = callback
        .as_ref()
        .unchecked_ref::<js_sys::Function>()
        .clone();
    callback.forget();
    function
}

/// `setTimeout(callback, ms)`: its handle, for [`clear_timeout`], or `None`
/// if it couldn't be set.
pub fn set_timeout(callback: &js_sys::Function, ms: i32) -> Option<i32> {
    web_sys::window()?
        .set_timeout_with_callback_and_timeout_and_arguments_0(callback, ms)
        .ok()
}

/// `clearTimeout(handle)`, if there's one.
pub fn clear_timeout(handle: Option<i32>) {
    if let (Some(handle), Some(window)) = (handle, web_sys::window()) {
        window.clear_timeout_with_handle(handle);
    }
}

/// Listens for `keydown` on `window`, capturing: before any of the page's
/// own handlers, as its modals do. For as long as the page lives.
pub fn on_window_keydown(handler: impl FnMut(KeyboardEvent) + 'static) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let handler = Closure::<dyn FnMut(KeyboardEvent)>::new(handler);
    let _ = window.add_event_listener_with_callback_and_bool(
        "keydown",
        handler.as_ref().unchecked_ref(),
        true,
    );
    handler.forget();
}

/// Whether `event` was aimed at `el` itself (not something inside it): a
/// click on a modal's dimmed backdrop.
pub fn targets(event: &web_sys::Event, el: &Element) -> bool {
    event.target().is_some_and(|target| {
        let target: &JsValue = target.as_ref();
        let el: &JsValue = el.as_ref();
        target == el
    })
}
