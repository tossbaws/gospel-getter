//! About Gospel Getter (☰ → About Gospel Getter, or the palette): the
//! app's icon, name and version, its Updates section (`updates`), the texts
//! it credits, and a Release notes link to this version's release page,
//! which the backend opens.
//!
//! About is one of the page's surfaces: the head script opens and closes it
//! (`window.gospelGetterSurfaces`), keeps one surface open at a time, puts
//! focus on its close button and back on ☰ when it closes, and says
//! `gospel-getter:surface-close`. This renders it, fills it, and has its
//! keys: Escape closes it and Tab stays inside it, on the overlay as the
//! head script had them; the reader's shortcuts wait meanwhile (the page's
//! keyboard dispatcher looks the overlay up). Its `hidden` is the head
//! script's alone.
//!
//! This is the page's About markup and its version, Release notes and
//! credits scripts (`ui/index.html` at 41e04f3) as a Leptos component: the
//! same ids, classes, ARIA, text and focus order.

use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsValue;
use web_sys::{CustomEvent, HtmlElement, KeyboardEvent, MouseEvent};

use crate::bridge;
use crate::dom;
use crate::ipc;
use crate::update_text;
use crate::updates::{UpdateBanner, Updates, UpdatesSection};

/// The release page of `version`, as the backend opens it
/// (`updater::release_notes_page`), shown as the link's address.
fn release_notes_page(version: &str) -> String {
    format!("https://github.com/tossbaws/gospel-getter/releases/tag/v{version}")
}

/// The cross-references' credit, after the bundled texts'.
const CROSS_REFERENCES_CREDIT: &str = "Cross-references from OpenBible.info, CC BY 4.0";

/// Escape closes About; Tab and Shift+Tab wrap between its first and last
/// control. Everything else passes, as when the head script had them.
fn on_keydown(panel: NodeRef<html::Div>, event: &KeyboardEvent) {
    match event.key().as_str() {
        "Escape" => {
            event.prevent_default();
            bridge::close_surfaces();
        }
        "Tab" => {
            let Some(panel) = panel.get_untracked() else {
                return;
            };
            let stops = dom::focusable(&panel, "input, button, a[href]");
            let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
                return;
            };
            let active = dom::active_element();
            if event.shift_key() && dom::same(active.as_ref(), first) {
                event.prevent_default();
                let _ = last.focus();
            } else if !event.shift_key() && dom::same(active.as_ref(), last) {
                event.prevent_default();
                let _ = first.focus();
            }
        }
        _ => {}
    }
}

/// About and the update banner, which share the offer: mounted once, then
/// each moved to where the page had it (see `lib.rs`).
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
    updates.listen();

    // The texts the page credits (each bundled translation's credit, from
    // boot), sent whenever it has them.
    let credits = RwSignal::new(Vec::<String>::new());
    dom::on_document_event(bridge::ABOUT_CREDITS_EVENT, move |event: CustomEvent| {
        match bridge::about_credits(&event) {
            Ok(list) => credits.set(list),
            Err(e) => ipc::log_error("About's credits couldn't be read", &e),
        }
    });

    let notes_status = RwSignal::new(String::new());
    let panel = NodeRef::<html::Div>::new();

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

    view! {
        <div
            id="about-overlay"
            class="import-overlay"
            hidden
            on:keydown=move |event: KeyboardEvent| on_keydown(panel, &event)
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
                    on:click=move |_| bridge::close_surfaces()
                >
                    "×"
                </button>
                <div class="about-head">
                    <img class="about-icon" src="favicon.svg" alt="" width="56" height="56" />
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
                </p>
            </div>
        </div>
        <UpdateBanner updates=updates />
    }
}

/// Puts About and the banner where the page had them: About before the
/// toast (`#data-status`), the banner before the page's `<main>`, so focus
/// order and stacking are as before.
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
