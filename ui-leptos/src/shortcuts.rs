//! The keyboard shortcuts sheet (`?`, ☰ → Keyboard shortcuts, or the
//! palette's command): the page's one table of shortcuts, in its groups,
//! each row its key caps and what it does.
//!
//! The table stays the page's (`SHORTCUTS` in `ui/index.html`): the
//! palette's key hints and `aria-keyshortcuts`, and the key handlers it
//! documents, are the page's JavaScript and read it at once, whether or not
//! this module has loaded. The page hands it over in each request to open
//! (see `bridge`), so there is one table.
//!
//! A modal, as the page's others: while open it has the keyboard (a
//! capturing `keydown` on `window`, stopping every other handler), Escape
//! closes it, Tab moves between the close button and the scrolling list,
//! a click on the dimmed page closes it, and focus goes back to where it
//! was (or ☰, when opened from there).
//!
//! This is the page's sheet script (`ui/index.html` at fa5969d,
//! `renderShortcuts` to its keydown trap) as a Leptos component: the same
//! ids, classes, ARIA, `hidden`, text and focus order.

use leptos::html;
use leptos::prelude::*;
use leptos::task::{spawn_local, tick};
use web_sys::{CustomEvent, HtmlElement, KeyboardEvent, MouseEvent};

use crate::bridge::{self, ShortcutRow, ShortcutsRequest};
use crate::dom::{self, focus};

/// The sheet's state and the elements it reaches for.
#[derive(Clone, Copy)]
struct Sheet {
    open: RwSignal<bool>,
    /// The table, as the page last handed it over.
    table: RwSignal<ShortcutsRequest>,
    /// Where focus goes back to on closing.
    return_to: StoredValue<Option<HtmlElement>, LocalStorage>,
    overlay: NodeRef<html::Div>,
    close_button: NodeRef<html::Button>,
    body: NodeRef<html::Div>,
}

impl Sheet {
    fn open(self, request: ShortcutsRequest, return_to: Option<HtmlElement>) {
        if self.open.get_untracked() {
            return;
        }
        self.return_to.set_value(return_to);
        self.table.set(request);
        self.open.set(true);
        spawn_local(async move {
            tick().await;
            // Closed again before it was on screen: focus stays put.
            if !self.open.get_untracked() {
                return;
            }
            if let Some(body) = self.body.get() {
                body.set_scroll_top(0);
            }
            if let Some(close) = self.close_button.get() {
                let _ = close.focus();
            }
        });
    }

    fn close(self) {
        if !self.open.get_untracked() {
            return;
        }
        self.open.set(false);
        if let Some(back) = self.return_to.get_value()
            && dom::usable(&back)
        {
            focus(&back);
        }
        self.return_to.set_value(None);
    }

    /// While open it has the keyboard: none of the reader's shortcuts run.
    fn on_keydown(self, event: KeyboardEvent) {
        if !self.open.get_untracked() {
            return;
        }
        event.stop_propagation();
        match event.key().as_str() {
            "Escape" => {
                event.prevent_default();
                self.close();
            }
            "Tab" => {
                event.prevent_default();
                let stops: Vec<HtmlElement> = [
                    self.close_button.get().map(Into::into),
                    self.body.get().map(Into::into),
                ]
                .into_iter()
                .flatten()
                .collect();
                let active = dom::active_element();
                let at = stops.iter().position(|el| dom::same(active.as_ref(), el));
                let next = match at {
                    None => 0,
                    Some(i) if event.shift_key() => (i + stops.len() - 1) % stops.len(),
                    Some(i) => (i + 1) % stops.len(),
                };
                if let Some(next) = stops.get(next) {
                    let _ = next.focus();
                }
            }
            _ => {}
        }
    }

    fn listen(self) {
        dom::on_document_event(bridge::SHORTCUTS_EVENT, move |event: CustomEvent| {
            event.prevent_default();
            match bridge::shortcuts_request(&event) {
                Ok(request) => self.open(request, bridge::return_focus(&event)),
                Err(e) => crate::ipc::log_error("The shortcuts request couldn't be read", &e),
            }
        });
        dom::on_window_keydown(move |event: KeyboardEvent| self.on_keydown(event));
    }
}

/// A key cap the sheet draws (the arrows, as the page's chapter-end hint
/// does: the UI sans's are tiny), with the name screen readers hear.
fn drawn_arrow(cap: &str) -> Option<(&'static str, &'static str)> {
    match cap {
        "←" => Some(("Left arrow", "M13 8H3M7 4 3 8l4 4")),
        "→" => Some(("Right arrow", "M3 8h10M9 4l4 4-4 4")),
        "↑" => Some(("Up arrow", "M8 13V3M4 7l4-4 4 4")),
        "↓" => Some(("Down arrow", "M8 3v10M4 9l4 4 4-4")),
        _ => None,
    }
}

/// One key cap: `<kbd>`, with a drawn arrow named for screen readers.
fn cap_view(cap: String) -> AnyView {
    match drawn_arrow(&cap) {
        Some((name, path)) => view! {
            <kbd>
                <span class="visually-hidden">{name}</span>
                <svg viewBox="0 0 16 16" aria-hidden="true" focusable="false">
                    <path d=path></path>
                </svg>
            </kbd>
        }
        .into_any(),
        None => view! { <kbd>{cap}</kbd> }.into_any(),
    }
}

/// The ways to press a shortcut: each a combination of caps joined by
/// "+", the ways joined by "or".
fn caps_view(keys: Vec<Vec<String>>) -> impl IntoView {
    keys.into_iter()
        .enumerate()
        .map(|(i, combo)| {
            let combo = combo
                .into_iter()
                .enumerate()
                .map(|(j, cap)| {
                    let plus = (j > 0).then(|| {
                        view! { <span class="key-plus" aria-hidden="true">"+"</span> }
                    });
                    view! { {plus} {cap_view(cap)} }
                })
                .collect_view();
            let or = (i > 0).then(|| view! { " " <span class="key-or">"or"</span> " " });
            view! { {or} {combo} }
        })
        .collect_view()
}

fn row_view(row: ShortcutRow) -> impl IntoView {
    view! {
        <div class="shortcut-row" data-shortcut=row.id>
            <dt class="shortcut-keys">{caps_view(row.keys)}</dt>
            " "
            <dd class="shortcut-does">{row.does}</dd>
        </div>
    }
}

/// The keyboard shortcuts sheet, mounted once, hidden until the page asks.
#[component]
pub fn Shortcuts() -> impl IntoView {
    let sheet = Sheet {
        open: RwSignal::new(false),
        table: RwSignal::new(ShortcutsRequest::default()),
        return_to: StoredValue::new_local(None),
        overlay: NodeRef::new(),
        close_button: NodeRef::new(),
        body: NodeRef::new(),
    };
    sheet.listen();

    let groups = move || {
        let table = sheet.table.get();
        table
            .groups
            .iter()
            .enumerate()
            .map(|(i, group)| {
                let id = format!("shortcut-group-{i}");
                let labelled_by = id.clone();
                let rows = table
                    .shortcuts
                    .iter()
                    .filter(|s| &s.group == group)
                    .cloned()
                    .map(row_view)
                    .collect_view();
                view! {
                    <section class="shortcut-group" aria-labelledby=labelled_by>
                        <h3 class="shortcut-group-title" id=id>{group.clone()}</h3>
                        <dl class="shortcut-list">{rows}</dl>
                    </section>
                }
            })
            .collect_view()
    };

    view! {
        <div
            id="shortcuts-overlay"
            class="modal-overlay"
            hidden=move || !sheet.open.get()
            node_ref=sheet.overlay
            on:click=move |ev: MouseEvent| {
                // A click on the dimmed page behind it closes it.
                if let Some(overlay) = sheet.overlay.get()
                    && dom::targets(&ev, &overlay)
                {
                    sheet.close();
                }
            }
        >
            <div
                id="shortcuts-panel"
                class="modal-panel shortcuts-panel"
                role="dialog"
                aria-modal="true"
                aria-labelledby="shortcuts-title"
            >
                <div class="shortcuts-head">
                    <h2 id="shortcuts-title" class="shortcuts-title">"Keyboard shortcuts"</h2>
                    <button
                        type="button"
                        id="shortcuts-close"
                        class="panel-close"
                        aria-label="Close keyboard shortcuts"
                        title="Close (Esc)"
                        node_ref=sheet.close_button
                        on:click=move |_| sheet.close()
                    >
                        "×"
                    </button>
                </div>
                <div
                    id="shortcuts-body"
                    class="modal-body shortcuts-body"
                    tabindex="0"
                    role="region"
                    aria-labelledby="shortcuts-title"
                    node_ref=sheet.body
                >
                    {groups}
                </div>
            </div>
        </div>
    }
}
