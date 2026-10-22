//! Aa (text and display) and the translation button's popover: the
//! translations, Compare and what it compares with; text size (its stepper,
//! Ctrl+=, Ctrl+- and Ctrl+0, and the size shown for a moment), line
//! spacing, width, layout (kept but disabled while comparing) and theme;
//! reading mode; and the launch check's setting in About. Every choice
//! goes through the saved settings' store (`prefs`).
//!
//! The two popovers themselves (`#aa-popover`, `#translation-popover`, with
//! their headings, the stepper, Compare and Reading mode) are the page's;
//! opening and closing them is `surfaces`'. As one opens, it has it drawn
//! ([`draw`]): the translations' rows and the choices' marks are drawn
//! here, there and then, before focus moves in. This draws the
//! choices themselves, and fills the hidden `<select>`s that record Aa's
//! settings for the page's other scripts, from the one table
//! (`preferences`).
//!
//! The `<select>`s stay the record, as they were: choosing sets one and
//! sends its `change`, which applies and saves the setting here, and which
//! the page's own listeners hear as before (a translation is read again; a
//! new layout keeps the reader's place and moves the selection's card).
//! `#translation-select` is the page's (boot fills it from the backend); a
//! translation is chosen through it the same way.
//!
//! This is the head script's Aa (`ui/index.html` at 4b2d15e, from `choose`
//! to `gospelGetterPreferences`) in Rust: the same ids, classes, ARIA,
//! text and focus order. Everything happens within the click or key, as it
//! did; the choices are drawn as Leptos views, built and put in place at
//! once. See `bridge` for what crosses to the page.

use std::cell::{Cell, OnceCell};

use leptos::prelude::*;
use leptos::tachys::view::{Mountable, Render};
use wasm_bindgen::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    CustomEvent, Element, HtmlButtonElement, HtmlElement, HtmlOptionElement, HtmlSelectElement,
    KeyboardEvent,
};

use crate::aa_text::{self as text, Press};
use crate::bridge;
use crate::dom;
use crate::preferences::{self, Preference, Values};
use crate::prefs::{self, Raw};
use crate::surfaces::{self, Surface};

/// Compare was switched on or off, or what it compares with changed: a
/// `CustomEvent` on `document`, no `detail`. The page shows the chapter in
/// the new layout.
pub const COMPARE_CHANGE_EVENT: &str = "gospel-getter:compare-change";

/// The layout changed: a `CustomEvent` on `document`, no `detail`. The
/// page moves the selection's card to its verses' new place.
pub const LAYOUT_CHANGE_EVENT: &str = "gospel-getter:layout-change";

/// How long the text size shows after a size key, in milliseconds.
const SIZE_SHOWN_MS: i32 = 1500;

/// The settings with a choice group in Aa, and the group.
const GROUPS: [(&str, Preference); 3] = [
    ("line-spacing-choices", preferences::LINE_SPACING),
    ("width-choices", preferences::WIDTH),
    ("layout-choices", preferences::LAYOUT),
];

thread_local! {
    /// The text size's toast is up until this timer runs.
    static SIZE_TIMER: Cell<Option<i32>> = const { Cell::new(None) };
    /// Puts the text size's toast away (one closure, for the page's life).
    static HIDE_SIZE: OnceCell<Closure<dyn Fn()>> = const { OnceCell::new() };
}

// ---- The page's elements.

fn by_id(id: &str) -> Option<HtmlElement> {
    bridge::document()?
        .get_element_by_id(id)?
        .dyn_into::<HtmlElement>()
        .ok()
}

fn select(id: &str) -> Option<HtmlSelectElement> {
    bridge::document()?
        .get_element_by_id(id)?
        .dyn_into::<HtmlSelectElement>()
        .ok()
}

fn button(id: &str) -> Option<HtmlButtonElement> {
    bridge::document()?
        .get_element_by_id(id)?
        .dyn_into::<HtmlButtonElement>()
        .ok()
}

fn translation_select() -> Option<HtmlSelectElement> {
    select("translation-select")
}

/// A `<select>`'s options, in order.
fn options_of(select: &HtmlSelectElement) -> Vec<HtmlOptionElement> {
    let options = select.options();
    (0..options.length())
        .filter_map(|i| options.item(i))
        .filter_map(|el| el.dyn_into::<HtmlOptionElement>().ok())
        .collect()
}

/// The buttons `selector` finds in `root`, in order.
fn buttons(root: &Element, selector: &str) -> Vec<HtmlButtonElement> {
    root.query_selector_all(selector)
        .map(|found| {
            (0..found.length())
                .filter_map(|i| found.item(i))
                .filter_map(|node| node.dyn_into::<HtmlButtonElement>().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn data_value(el: &Element) -> Option<String> {
    el.get_attribute("data-value")
}

/// The button `event` was on (or in), within its group.
fn button_hit(event: &web_sys::Event) -> Option<Element> {
    event
        .target()?
        .dyn_into::<Element>()
        .ok()?
        .closest("button")
        .ok()?
}

/// Empties `el`.
fn clear(el: &Element) {
    el.set_text_content(Some(""));
}

/// Draws `view` at the end of `parent`, now, and leaves it there.
fn append(parent: &Element, view: AnyView) {
    let mut drawn = view.build();
    drawn.mount(parent, None);
}

/// Sends `name` on `document`, as the page's own `new CustomEvent(name)`.
fn announce(name: &str) {
    if let (Some(document), Ok(event)) = (bridge::document(), CustomEvent::new(name)) {
        let _ = document.dispatch_event(&event);
    }
}

// ---- Choosing.

/// Chooses `value` in a setting's `<select>`: the `change` it sends applies
/// and saves it (and the page's own listeners hear it), then Aa shows it.
fn choose(select: &HtmlSelectElement, value: &str) {
    if select.value() != value {
        select.set_value(value);
        let init = web_sys::EventInit::new();
        init.set_bubbles(true);
        if let Ok(change) = web_sys::Event::new_with_event_init_dict("change", &init) {
            let _ = select.dispatch_event(&change);
        }
    }
    sync_aa();
}

/// The translation compare shows beside the one being read (the page's
/// `window.gospelGetterComparePartner()`, whose rule is the reader's), if
/// there's one.
fn partner() -> Option<String> {
    let window = web_sys::window()?;
    let partner =
        js_sys::Reflect::get(&window, &JsValue::from_str("gospelGetterComparePartner")).ok()?;
    partner
        .dyn_into::<js_sys::Function>()
        .ok()?
        .call0(&JsValue::NULL)
        .ok()?
        .as_string()
}

/// Compare's "with": chooses `code`, unless it's already the one.
pub fn compare_with(code: &str) {
    let chosen = prefs::shown(&preferences::COMPARE_WITH);
    if partner().as_deref() == Some(code) && chosen.as_deref() == Some(code) {
        return;
    }
    prefs::set(&preferences::COMPARE_WITH, code);
    sync_aa();
    announce(COMPARE_CHANGE_EVENT);
}

/// Reads `code` instead: the translation button's popover's rows, and the
/// palette's commands.
pub fn choose_translation(code: &str) {
    if let Some(select) = translation_select() {
        choose(&select, code);
    }
}

/// The text size `by` steps larger (or smaller).
fn step_text_size(by: i32) {
    let Some(select) = select("text-size-select") else {
        return;
    };
    let options = options_of(&select);
    if let Some(next) = text::stepped(select.selected_index(), options.len() as u32, by)
        .and_then(|i| options.get(i as usize))
    {
        choose(&select, &next.value());
    }
}

/// The text size, for a moment: "Text size: Large".
fn show_text_size() {
    let (Some(window), Some(status), Some(select)) = (
        web_sys::window(),
        by_id("size-status"),
        select("text-size-select"),
    ) else {
        return;
    };
    if let Some(timer) = SIZE_TIMER.get() {
        window.clear_timeout_with_handle(timer);
    }
    let label = selected_label(&select);
    status.set_text_content(Some(&format!("Text size: {label}")));
    let _ = status.class_list().remove_1("is-hidden");
    let timer = HIDE_SIZE.with(|hide| {
        let hide = hide.get_or_init(|| {
            Closure::new(|| {
                SIZE_TIMER.set(None);
                if let Some(status) = by_id("size-status") {
                    let _ = status.class_list().add_1("is-hidden");
                }
            })
        });
        window
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                hide.as_ref().unchecked_ref(),
                SIZE_SHOWN_MS,
            )
            .ok()
    });
    SIZE_TIMER.set(timer);
}

/// A size key's step (or the palette's): a step larger (1) or smaller
/// (-1), or back to Medium (0), then the size shown for a moment.
pub fn text_size_shortcut(step: i8) {
    if step == 0 {
        if let Some(select) = select("text-size-select") {
            choose(&select, "medium");
        }
    } else {
        step_text_size(i32::from(step));
    }
    show_text_size();
}

/// Reading mode on or off (also Escape, last of all, leaving it). Focus
/// goes to whichever control undoes it, so a keyboard reader always lands
/// somewhere sensible: the exit button, or Aa, which holds its toggle. It
/// hides the sidebar, and leaving it brings the sidebar back.
pub fn set_reader_mode(on: bool) {
    prefs::set(&preferences::READER_MODE, if on { "on" } else { "off" });
    if let Some(toggle) = by_id("reader-mode-btn") {
        let _ = toggle.set_attribute("aria-pressed", &on.to_string());
    }
    surfaces::close(false);
    surfaces::sync_picker();
    if let Some(target) = by_id(if on { "reader-exit" } else { "aa-toggle" }) {
        let _ = target.focus();
    }
}

/// Compare, on or off.
fn toggle_compare() {
    let value = if prefs::on(&preferences::COMPARE) {
        "off"
    } else {
        "on"
    };
    prefs::set(&preferences::COMPARE, value);
    if let Some(compare) = by_id("compare-btn") {
        let _ = compare.set_attribute("aria-pressed", &(value == "on").to_string());
    }
    sync_aa();
    announce(COMPARE_CHANGE_EVENT);
}

// ---- Drawing.

fn selected_label(select: &HtmlSelectElement) -> String {
    let options = options_of(select);
    usize::try_from(select.selected_index())
        .ok()
        .and_then(|i| options.get(i))
        .and_then(|o| o.text_content())
        .unwrap_or_default()
}

/// One choice of a segmented row.
fn segment_view(value: String, label: String, title: Option<String>) -> AnyView {
    view! { <button type="button" class="segment" data-value=value title=title>{label}</button> }
        .into_any()
}

/// One theme's swatch, previewing it.
fn swatch_view(value: &'static str, label: &'static str) -> AnyView {
    view! {
        <button type="button" class="theme-swatch" data-value=value title=label aria-label=label>
            <span class="theme-swatch-preview" data-theme-preview=value aria-hidden="true">
                "A"
            </span>
        </button>
    }
    .into_any()
}

/// One translation's row: "KJV · King James Version · 1611/1769 · public
/// domain", a radio. The dots are only seen; the words are its name.
fn translation_view(option: &HtmlOptionElement) -> AnyView {
    let code = option.value();
    let parts: Vec<(&'static str, String)> = [
        ("translation-code", Some(code.to_uppercase())),
        ("translation-name", option.text_content()),
        ("translation-detail", option.get_attribute("data-date")),
        ("translation-detail", option.get_attribute("data-licence")),
    ]
    .into_iter()
    .filter_map(|(class, part)| part.filter(|p| !p.is_empty()).map(|p| (class, p)))
    .collect();
    let parts = parts
        .into_iter()
        .enumerate()
        .map(|(i, (class, part))| {
            let dot = (i > 0).then(
                || view! { <span class="translation-dot" aria-hidden="true">" \u{b7} "</span> },
            );
            view! {
                {dot}
                <span class=class>{part}</span>
            }
        })
        .collect_view();
    view! { <button type="button" class="translation-choice" role="radio" data-value=code>{parts}</button> }
        .into_any()
}

/// The translations, one row each, then what Compare can compare with:
/// drawn each time their popover opens, since the translations are the
/// backend's.
fn render_translations() {
    let (Some(choices), Some(select)) = (by_id("translation-choices"), translation_select()) else {
        return;
    };
    clear(&choices);
    for option in options_of(&select) {
        append(&choices, translation_view(&option));
    }
    render_compare_with();
}

/// Compare with: every translation but the one being read.
fn render_compare_with() {
    let (Some(choices), Some(select)) = (by_id("compare-with-choices"), translation_select())
    else {
        return;
    };
    clear(&choices);
    let reading = select.value();
    for option in options_of(&select) {
        let code = option.value();
        if code == reading {
            continue;
        }
        let name = option.text_content().unwrap_or_default();
        append(
            &choices,
            segment_view(
                code.clone(),
                code.to_uppercase(),
                Some(format!("Compare with the {name}")),
            ),
        );
    }
    sync_aa();
}

fn set_pressed(group: &str, value: Option<&str>) {
    let Some(group) = by_id(group) else {
        return;
    };
    for btn in buttons(&group, "button") {
        let pressed = value.is_some() && data_value(&btn).as_deref() == value;
        let _ = btn.set_attribute("aria-pressed", &pressed.to_string());
    }
}

/// Aa and the translations show what's chosen: each `<select>`'s choice
/// marked, Layout disabled while comparing (kept, but it has no effect,
/// and a note says why), the theme's and the size's names, and the size
/// stepper's ends. A stepper button that just reached its end can't keep
/// focus: it goes to the other one.
fn sync_aa() {
    let Some(document) = bridge::document() else {
        return;
    };
    let reading = translation_select().map(|s| s.value());
    if let Some(choices) = by_id("translation-choices") {
        for radio in buttons(&choices, "[role=\"radio\"]") {
            let checked = reading.is_some() && data_value(&radio) == reading;
            let _ = radio.set_attribute("aria-checked", &checked.to_string());
            radio.set_tab_index(if checked { 0 } else { -1 });
        }
    }
    set_pressed("compare-with-choices", partner().as_deref());
    for (group, pref) in GROUPS
        .iter()
        .chain(&[("theme-choices", preferences::THEME)])
    {
        let value = pref.select.and_then(select).map(|s| s.value());
        set_pressed(group, value.as_deref());
    }
    let comparing = prefs::on(&preferences::COMPARE);
    if let Some(layout) = by_id("layout-choices") {
        for btn in buttons(&layout, "button") {
            btn.set_disabled(comparing);
        }
    }
    if let Some(note) = by_id("layout-note") {
        note.set_hidden(!comparing);
    }
    if let (Some(name), Some(theme)) = (by_id("theme-name"), select("theme-select")) {
        name.set_text_content(Some(&selected_label(&theme)));
    }
    let Some(sizes) = select("text-size-select") else {
        return;
    };
    if let Some(shown) = by_id("text-size-value") {
        shown.set_text_content(Some(&selected_label(&sizes)));
    }
    let (Some(down), Some(up)) = (button("text-size-down"), button("text-size-up")) else {
        return;
    };
    let focused = document.active_element();
    let index = sizes.selected_index();
    down.set_disabled(index <= 0);
    up.set_disabled(index >= sizes.length() as i32 - 1);
    if dom::same(focused.as_ref(), &down) && down.disabled() {
        let _ = up.focus();
    }
    if dom::same(focused.as_ref(), &up) && up.disabled() {
        let _ = down.focus();
    }
}

/// Fills Aa's `<select>`s with their settings' choices (the one table's),
/// each set to what's saved, and draws the choices that don't change: the
/// segmented rows and the theme swatches.
fn draw_choices() {
    for pref in preferences::PREFERENCES {
        let (Some(id), Values::Choices(choices)) = (pref.select, pref.values) else {
            continue;
        };
        let Some(select) = select(id) else {
            continue;
        };
        clear(&select);
        for choice in choices {
            append(
                &select,
                view! { <option value=choice.value>{choice.label}</option> }.into_any(),
            );
        }
        select.set_value(&prefs::load(&pref));
    }
    for (group, pref) in GROUPS {
        let (Some(group), Values::Choices(choices)) = (by_id(group), pref.values) else {
            continue;
        };
        for choice in choices {
            append(
                &group,
                segment_view(choice.value.into(), choice.label.into(), None),
            );
        }
    }
    if let (Some(group), Values::Choices(choices)) =
        (by_id("theme-choices"), preferences::THEME.values)
    {
        for choice in choices {
            append(&group, swatch_view(choice.value, choice.label));
        }
    }
}

// ---- The page's globals.

/// Every display setting as an export file has it, from what's in effect
/// now: Compare's "with" only once chosen, and only a translation this
/// install has.
fn current() -> JsValue {
    let out = js_sys::Object::new();
    let known: Vec<String> = translation_select()
        .map(|s| {
            options_of(&s)
                .iter()
                .map(HtmlOptionElement::value)
                .collect()
        })
        .unwrap_or_default();
    for pref in preferences::PREFERENCES {
        let Some(file_name) = pref.file_name else {
            continue;
        };
        let value = prefs::value(&pref);
        let shown = if pref == preferences::COMPARE_WITH {
            if !known.contains(&value) {
                continue;
            }
            JsValue::from_str(&value)
        } else if pref.on_off() {
            JsValue::from_bool(value == "on")
        } else {
            JsValue::from_str(&value)
        };
        let _ = js_sys::Reflect::set(&out, &JsValue::from_str(file_name), &shown);
    }
    out.into()
}

#[wasm_bindgen]
extern "C" {
    /// JavaScript's `String(value)`.
    #[wasm_bindgen(js_name = String)]
    fn js_string(value: &JsValue) -> String;
}

/// `value` as the page received it from an export file.
fn raw(value: &JsValue) -> Raw {
    if let Some(on) = value.as_bool() {
        Raw::Bool(on)
    } else if let Some(text) = value.as_string() {
        Raw::Text(text)
    } else {
        Raw::Other(js_string(value))
    }
}

/// Applies and saves an export file's display settings (`values`, each by
/// its name in the file), after checking every one, so an unknown one
/// changes nothing: an import's, or `window.gospelGetterPreferences.apply`.
/// `Ok` says whether they were all saved too; if not (storage blocked or
/// full), they still apply until the app closes. An error is an `Error`
/// saying what was wrong.
pub fn apply_preferences(values: &JsValue) -> Result<bool, JsValue> {
    // Any object (an import's, or one from another realm, as the frontend
    // tests' are), as `Object.keys` takes it.
    if !values.is_object() {
        return Err(js_sys::TypeError::new("Cannot convert undefined or null to object").into());
    }
    let values: &js_sys::Object = values.unchecked_ref();
    let changes = js_sys::Object::keys(values)
        .iter()
        .map(|name| {
            let name = name.as_string().unwrap_or_default();
            let raw = raw(&js_sys::Reflect::get(values, &JsValue::from_str(&name))?);
            prefs::checked(&name, &raw).map_err(|e| JsValue::from(js_sys::Error::new(&e)))
        })
        .collect::<Result<Vec<_>, JsValue>>()?;
    let mut persisted = true;
    let mut compare_changed = false;
    for (pref, value) in changes {
        if *pref == preferences::COMPARE && prefs::on(pref) != (value == "on") {
            compare_changed = true;
        }
        if *pref == preferences::COMPARE_WITH && prefs::shown(pref).as_deref() != Some(&value) {
            compare_changed = true;
        }
        prefs::set(pref, &value);
        if prefs::stored(pref).as_deref() != Some(&value) {
            persisted = false;
        }
        if let Some(select) = pref.select.and_then(select) {
            select.set_value(&value);
        }
    }
    show_toggles();
    sync_aa();
    surfaces::sync_picker();
    if compare_changed {
        announce(COMPARE_CHANGE_EVENT);
    }
    Ok(persisted)
}

/// Reading mode's and Compare's buttons say whether they're on.
fn show_toggles() {
    for (id, pref) in [
        ("reader-mode-btn", preferences::READER_MODE),
        ("compare-btn", preferences::COMPARE),
    ] {
        if let Some(toggle) = by_id(id) {
            let _ = toggle.set_attribute("aria-pressed", &prefs::on(&pref).to_string());
        }
    }
}

/// `window.gospelGetterPreferences`: `current()`, the display settings in
/// effect as an export file has them (Export data… reads it), and
/// `apply(values)`, which applies and saves them and returns
/// `{ persisted }`, or throws what was wrong.
fn export_preferences() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let global = js_sys::Object::new();
    let current = Closure::<dyn Fn() -> JsValue>::new(current);
    let apply = Closure::<dyn Fn(JsValue) -> Result<JsValue, JsValue>>::new(|values: JsValue| {
        let persisted = apply_preferences(&values)?;
        let out = js_sys::Object::new();
        js_sys::Reflect::set(
            &out,
            &JsValue::from_str("persisted"),
            &JsValue::from_bool(persisted),
        )?;
        Ok(out.into())
    });
    let _ = js_sys::Reflect::set(&global, &JsValue::from_str("current"), current.as_ref());
    let _ = js_sys::Reflect::set(&global, &JsValue::from_str("apply"), apply.as_ref());
    let _ = js_sys::Reflect::set(
        &window,
        &JsValue::from_str("gospelGetterPreferences"),
        &global,
    );
    current.forget();
    apply.forget();
}

// ---- Listening.

/// The translations are radio buttons: one tab stop, and the arrow keys
/// (Home and End too) move and choose. They're not a chapter turn.
fn on_translation_key(event: &KeyboardEvent) {
    let (Some(choices), Some(target)) = (
        by_id("translation-choices"),
        event.target().and_then(|t| t.dyn_into::<Element>().ok()),
    ) else {
        return;
    };
    let radios = buttons(&choices, "[role=\"radio\"]");
    let Some(radio) = target.closest("[role=\"radio\"]").ok().flatten() else {
        return;
    };
    let Some(index) = radios.iter().position(|r| dom::same(Some(&radio), r)) else {
        return;
    };
    let count = radios.len();
    let next = match event.key().as_str() {
        "ArrowDown" | "ArrowRight" => Some((index + 1) % count),
        "ArrowUp" | "ArrowLeft" => Some((index + count - 1) % count),
        "Home" => Some(0),
        "End" => Some(count - 1),
        _ => None,
    };
    let Some(next) = next.and_then(|i| radios.get(i)) else {
        return;
    };
    event.prevent_default();
    event.stop_propagation();
    if let Some(code) = data_value(next) {
        choose_translation(&code);
    }
    let _ = next.focus();
}

/// Ctrl+= (Ctrl++, Ctrl+Shift+=, numpad +) and Ctrl+- (numpad -) step
/// through the text sizes, and Ctrl+0 goes back to Medium, through Aa's own
/// control, so Aa stays in step. They're never the webview's page zoom:
/// the app turns that off (`zoomHotkeysEnabled`), and these keys' default
/// is prevented besides. While typing in a field they do nothing.
fn on_size_key(event: &KeyboardEvent) {
    let key = event.key();
    let code = event.code();
    let Some(step) = text::size_key(&Press {
        key: &key,
        code: &code,
        ctrl: event.ctrl_key(),
        meta: event.meta_key(),
        alt: event.alt_key(),
        shift: event.shift_key(),
    }) else {
        return;
    };
    event.prevent_default();
    let typing = event
        .target()
        .and_then(|t| t.dyn_into::<HtmlElement>().ok())
        .is_some_and(|el| {
            matches!(el.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT")
                || el.is_content_editable()
        });
    if !typing {
        text_size_shortcut(step);
    }
}

/// A setting's `<select>` changed: apply and save it. A new layout reflows
/// the whole chapter, so the reader's place is kept
/// (`surfaces::keep_reading_place`), and the page moves the selection's
/// card.
fn on_select_change(pref: Preference) {
    let change = move || {
        if let Some(chosen) = pref.select.and_then(select) {
            prefs::set(&pref, &chosen.value());
        }
    };
    if pref == preferences::LAYOUT {
        surfaces::keep_reading_place(change, None);
        announce(LAYOUT_CHANGE_EVENT);
    } else {
        change();
    }
}

/// Draws what's in Aa or the translation button's popover as it opens
/// (`surfaces`); the other surfaces are drawn by their own pieces.
pub fn draw(surface: Surface) {
    match surface {
        Surface::Translation => render_translations(),
        Surface::Aa => sync_aa(),
        _ => {}
    }
}

/// Draws the choices, keeps the `<select>`s, and listens to Aa's and the
/// translations' controls, the size keys and About's launch check setting;
/// and puts `window.gospelGetterPreferences` where the page reads it.
/// Called before the other pieces (after `surfaces`, which reads none of
/// them), so the settings' records are filled before anything reads them.
pub fn mount() {
    let Some(document) = bridge::document() else {
        return;
    };
    draw_choices();

    // A click on a choice chooses it.
    for (group, pref) in GROUPS
        .iter()
        .chain(&[("theme-choices", preferences::THEME)])
    {
        let Some(el) = by_id(group) else {
            continue;
        };
        let id = pref.select.unwrap_or_default();
        dom::on_click(&el, move |event| {
            if let (Some(btn), Some(select)) = (button_hit(&event), select(id))
                && let Some(value) = data_value(&btn)
            {
                choose(&select, &value);
            }
        });
    }
    if let Some(el) = by_id("translation-choices") {
        dom::on_click(&el, |event| {
            if let Some(code) = button_hit(&event).and_then(|btn| data_value(&btn)) {
                choose_translation(&code);
            }
        });
        dom::on(&el, "keydown", |event| {
            if let Some(event) = event.dyn_ref::<KeyboardEvent>() {
                on_translation_key(event);
            }
        });
    }
    if let Some(el) = by_id("compare-with-choices") {
        dom::on_click(&el, |event| {
            if let Some(code) = button_hit(&event).and_then(|btn| data_value(&btn)) {
                compare_with(&code);
            }
        });
    }
    if let Some(down) = by_id("text-size-down") {
        dom::on_click(&down, |_| step_text_size(-1));
    }
    if let Some(up) = by_id("text-size-up") {
        dom::on_click(&up, |_| step_text_size(1));
    }
    if let Some(select) = translation_select() {
        dom::on(&select, "change", |_| render_compare_with());
    }
    dom::on(&document, "keydown", |event| {
        if let Some(event) = event.dyn_ref::<KeyboardEvent>() {
            on_size_key(event);
        }
    });
    for pref in preferences::PREFERENCES {
        if let Some(select) = pref.select.and_then(select) {
            dom::on(&select, "change", move |_| on_select_change(pref));
        }
    }

    show_toggles();
    if let Some(toggle) = by_id("reader-mode-btn") {
        dom::on_click(&toggle, |_| {
            set_reader_mode(!prefs::on(&preferences::READER_MODE));
        });
    }
    if let Some(exit) = by_id("reader-exit") {
        dom::on_click(&exit, |_| set_reader_mode(false));
    }
    if let Some(compare) = by_id("compare-btn") {
        dom::on_click(&compare, |_| toggle_compare());
    }
    // The launch check's setting: its checkbox is About's (`updates`).
    dom::on(&document, "change", |event| {
        let toggle = event
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            .filter(|t| t.id() == "update-check-toggle");
        if let Some(toggle) = toggle {
            let value = if toggle.checked() { "on" } else { "off" };
            prefs::set(&preferences::UPDATE_CHECK, value);
        }
    });

    export_preferences();
}
