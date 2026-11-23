//! About Gospel Getter (☰ → About Gospel Getter, or the palette): the
//! app's icon, name and version, its Updates section (`updates`), the texts
//! it credits, a Release notes link to this version's release page, and a
//! Support Gospel Getter link to the Ko-fi page, both of which the backend
//! opens.
//!
//! About is one of the page's surfaces: `surfaces` opens and closes it,
//! keeps one surface open at a time, puts focus on its close button and
//! back on ☰ when it closes, and tells the updater (`updates`). This
//! renders it, fills it, and has the keyboard while it's open, as every
//! modal does (`overlays`): Escape closes it, Tab stays inside it, bringing
//! back focus that got outside it, and no other handler hears a key. Its
//! `hidden` is `surfaces`' alone.

use std::cell::Cell;

use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsValue;
use web_sys::{HtmlElement, KeyboardEvent, MouseEvent};

use crate::bridge;
use crate::constants::SUPPORT_PAGE;
use crate::dom;
use crate::ipc;
use crate::overlays::Overlay;
use crate::surfaces;
use crate::update_text;
use crate::updates::{GoBackConfirm, UpdateBanner, Updates, UpdatesSection};

/// The release page of `version`, as the backend opens it
/// (`updater::release_notes_page`), shown as the link's address.
fn release_notes_page(version: &str) -> String {
    format!("https://github.com/tossbaws/gospel-getter/releases/tag/v{version}")
}

/// The cross-references' credit, after the bundled texts'.
const CROSS_REFERENCES_CREDIT: &str =
    "Cross-references from OpenBible.info, selected and reformatted, CC BY 4.0";

/// While About is open, every key is its own: on `window`, capturing, so a
/// key from outside About reaches it first, and stopped there, so the
/// reader's shortcuts and Aa's text-size keys never hear it (one Escape
/// closes About and does nothing else). A key's default still happens
/// (Enter or Space on a link or button). Escape closes it. Tab and
/// Shift+Tab wrap between its first and last control, and focus that got
/// outside it (a click on its backdrop) comes back to the first (or, with
/// Shift, the last), as in the other modals (`dom::trap_tab`, D2-02).
fn on_keydown(panel: NodeRef<html::Div>, event: &KeyboardEvent) {
    // Go back's confirm dialog, over About, has the keyboard while it's
    // open (`updates`).
    if !Overlay::About.is_open() || Overlay::GoBack.is_open() {
        return;
    }
    event.stop_propagation();
    match event.key().as_str() {
        "Escape" => {
            event.prevent_default();
            surfaces::close(true);
        }
        "Tab" => {
            let Some(panel) = panel.get_untracked() else {
                return;
            };
            let stops = dom::focusable(&panel, "input, button, a[href]");
            dom::trap_tab(event, &panel, &stops);
        }
        _ => {}
    }
}

thread_local! {
    /// About's credits, once About has mounted (it takes the reader's then).
    static CREDITS: Cell<Option<RwSignal<Vec<String>>>> = const { Cell::new(None) };
}

/// The bundled texts' credits, each translation's in order (the reader's,
/// from boot): shown in About, if it has mounted.
pub fn show_credits(credits: Vec<String>) {
    if let Some(shown) = CREDITS.get() {
        shown.set(credits);
    }
}

/// About and the update banner, which share the offer: mounted once, then
/// each moved to its place ([`place`]).
#[component]
pub fn AboutAndUpdates() -> impl IntoView {
    // Informational only: if the version can't be read, the line stays
    // empty and the link hidden.
    let version = LocalResource::new(|| async {
        ipc::invoke::<String>("get_app_version", JsValue::UNDEFINED)
            .await
            .ok()
            .filter(|v| !v.is_empty())
    });
    let updates = Updates::new(version);
    updates.mounted();

    // The texts the page credits (each bundled translation's credit, from
    // boot): the reader's now, and whenever it has them again.
    let credits = RwSignal::new(crate::reader::credits());
    CREDITS.set(Some(credits));

    let notes_status = RwSignal::new(String::new());
    let support_status = RwSignal::new(String::new());
    let panel = NodeRef::<html::Div>::new();
    dom::on_window_keydown(move |event: KeyboardEvent| on_keydown(panel, &event));

    let known_version = move || version.get().flatten();
    let open_release_notes = move |event: MouseEvent| {
        // The page itself never opens a browser: the backend opens this
        // version's release page (the URL shown, built the same way).
        event.prevent_default();
        notes_status.set(String::new());
        spawn_local(async move {
            if let Err(e) = ipc::invoke_value("open_release_notes", JsValue::UNDEFINED).await {
                notes_status.set(ipc::error_text(&e));
            }
        });
    };
    let open_support_page = move |event: MouseEvent| {
        // As Release notes: the backend opens its own fixed URL, the one
        // shown; the page passes none.
        event.prevent_default();
        support_status.set(String::new());
        spawn_local(async move {
            if let Err(e) = ipc::invoke_value("open_support_page", JsValue::UNDEFINED).await {
                support_status.set(ipc::error_text(&e));
            }
        });
    };

    view! {
        <div
            id="about-overlay"
            class="import-overlay"
            hidden
        >
            <div
                id="about-panel"
                class="import-panel about-panel"
                role="dialog"
                aria-modal="true"
                aria-labelledby="about-title"
                aria-describedby="app-version"
                node_ref=panel
            >
                <button
                    type="button"
                    id="about-close"
                    class="panel-close about-close"
                    aria-label="Close"
                    title="Close (Esc)"
                    on:click=move |_| surfaces::close(true)
                >
                    "×"
                </button>
                <div class="about-head">
                    <img class="about-icon" src="favicon.png" alt="" width="56" height="56" />
                    <div>
                        <h2 id="about-title" class="import-title">"Gospel Getter"</h2>
                        <p id="app-version" class="app-version">
                            {move || known_version().map(|v| update_text::version_line(&v)).unwrap_or_default()}
                        </p>
                    </div>
                </div>
                <UpdatesSection updates=updates />
                <section class="about-credits" aria-labelledby="credits-label">
                    <h3 class="section-label" id="credits-label">"Texts"</h3>
                    <ul id="about-credits" class="about-credits-list">
                        {move || {
                            let list = credits.get();
                            (!list.is_empty())
                                .then(|| {
                                    list.into_iter()
                                        .chain(std::iter::once(CROSS_REFERENCES_CREDIT.to_string()))
                                        .map(|credit| view! { <li>{credit}</li> })
                                        .collect_view()
                                })
                        }}
                    </ul>
                </section>
                <p class="about-links">
                    <a
                        id="release-notes-link"
                        href=move || {
                            known_version()
                                .map_or_else(
                                    || "https://github.com/tossbaws/gospel-getter/releases".to_string(),
                                    |v| release_notes_page(&v),
                                )
                        }
                        hidden=move || known_version().is_none()
                        on:click=open_release_notes
                    >
                        "Release notes"
                    </a>
                    " "
                    <span
                        id="release-notes-status"
                        class="data-status"
                        role="status"
                        aria-live="polite"
                    >
                        {move || notes_status.get()}
                    </span>
                    <span aria-hidden="true" hidden=move || known_version().is_none()>
                        " · "
                    </span>
                    <a id="support-link" href=SUPPORT_PAGE on:click=open_support_page>
                        "Support Gospel Getter"
                    </a>
                    " "
                    <span id="support-status" class="data-status" role="status" aria-live="polite">
                        {move || support_status.get()}
                    </span>
                </p>
            </div>
            <GoBackConfirm updates=updates />
        </div>
        <UpdateBanner updates=updates />
    }
}

/// Puts About before the toast (`#data-status`) and the banner before the
/// page's `<main>`: that fixes focus order and stacking.
pub fn place(holder: &HtmlElement) {
    let Some(document) = bridge::document() else {
        return;
    };
    let body = document.body();
    for (id, before) in [
        ("about-overlay", document.get_element_by_id("data-status")),
        (
            "update-banner",
            document.query_selector("body > main").ok().flatten(),
        ),
    ] {
        let Ok(Some(el)) = holder.query_selector(&format!("#{id}")) else {
            continue;
        };
        match (
            before.as_ref().and_then(|b| b.parent_node()),
            before.as_ref(),
        ) {
            (Some(parent), Some(before)) => {
                let _ = parent.insert_before(&el, Some(before));
            }
            _ => {
                if let Some(body) = &body {
                    let _ = body.append_child(&el);
                }
            }
        }
    }
}
