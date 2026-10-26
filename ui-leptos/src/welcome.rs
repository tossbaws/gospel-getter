//! The welcome: one card over the open chapter. It opens by itself only on
//! a fresh install (boot asks at its end, with what it loaded; `install`
//! decides), and whenever ☰ → Show welcome is chosen. It
//! offers a look (Chi Rho or Vellum, picked through Aa's own control) and a
//! few places to start reading (the backend's `startHere` list, which boot
//! keeps). Closing it any way (Skip, ×, Escape, or a place to
//! start) records that it has been seen, in local storage beside the
//! display settings.
//!
//! A modal, as the page's others: while open it has the keyboard (a
//! capturing `keydown` on `window`, stopping every other handler), Escape
//! closes it, the arrow keys move between the looks (and choose, as radio
//! buttons do), and Tab stays inside. Opened at startup, closing gives the
//! keyboard back to the page; opened from ☰, focus goes back there.
//!
//! This is the page's welcome script (`ui/index.html` at fa5969d,
//! `renderStartHere` to its keydown trap) as a Leptos component: the same
//! ids, classes, ARIA, `hidden`, text and focus order.

use leptos::html;
use leptos::prelude::*;
use leptos::task::{spawn_local, tick};
use wasm_bindgen::JsCast;
use web_sys::{CustomEvent, Element, HtmlElement, KeyboardEvent, MouseEvent};

use crate::bridge::{self, StartRow, WelcomeRequest};
use crate::dom;
use crate::install;

/// The two looks, in order: the theme each applies and its name.
const LOOKS: [(&str, &str); 2] = [("chi-rho", "Chi Rho"), ("vellum", "Vellum")];

#[derive(Clone, Copy)]
struct Welcome {
    open: RwSignal<bool>,
    /// The theme in use, as Aa's control has it (neither look is checked
    /// for another theme).
    theme: RwSignal<String>,
    start_here: RwSignal<Vec<StartRow>>,
    return_to: StoredValue<Option<HtmlElement>, LocalStorage>,
    overlay: NodeRef<html::Div>,
    panel: NodeRef<html::Div>,
    body: NodeRef<html::Div>,
    looks: NodeRef<html::Div>,
}

impl Welcome {
    /// The page's request: at startup, open only for a fresh install.
    fn request(self, request: WelcomeRequest, return_to: Option<HtmlElement>) {
        if request.startup {
            let install = request.install.unwrap_or_default();
            let keys = bridge::stored_keys();
            if !install::is_fresh(&install, keys.as_deref()) {
                return;
            }
        }
        self.show(request.start_here, return_to);
    }

    fn show(self, start_here: Vec<StartRow>, return_to: Option<HtmlElement>) {
        bridge::modal_opening(true);
        self.return_to.set_value(return_to);
        self.start_here.set(start_here);
        self.theme.set(bridge::theme());
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
            if let Some(panel) = self.panel.get() {
                let _ = panel.focus();
            }
        });
    }

    fn close(self) {
        self.open.set(false);
        bridge::welcome_seen();
        let target = self.return_to.get_value();
        self.return_to.set_value(None);
        match target {
            Some(target) if dom::usable(&target) => {
                let _ = target.focus();
            }
            _ => {
                // Opened at startup: give the keyboard back to the page,
                // where the reader's shortcuts work.
                if let (Some(overlay), Some(active)) = (self.overlay.get(), dom::active_element())
                    && overlay.contains(Some(&active))
                    && let Some(active) = active.dyn_ref::<HtmlElement>()
                {
                    let _ = active.blur();
                }
            }
        }
    }

    /// Applies the look live, through Aa's own record of the theme, and
    /// moves focus to it.
    fn choose(self, index: usize) {
        let Some((value, _)) = LOOKS.get(index) else {
            return;
        };
        bridge::choose_theme(value);
        self.theme.set(bridge::theme());
        if let Some(radio) = self.radio(index) {
            let _ = radio.focus();
        }
    }

    /// The look radios, in order.
    fn radios(self) -> Vec<HtmlElement> {
        self.looks
            .get()
            .and_then(|looks| looks.query_selector_all("[role=\"radio\"]").ok())
            .map(|list| dom::elements(&list))
            .unwrap_or_default()
    }

    fn radio(self, index: usize) -> Option<HtmlElement> {
        self.radios().into_iter().nth(index)
    }

    /// Start here: closes the card, then opens the chapter from the top.
    fn start(self, row: &StartRow) {
        self.close();
        crate::reader::jump(row.book_id, row.chapter, true);
    }

    fn on_keydown(self, event: KeyboardEvent) {
        if !self.open.get_untracked() {
            return;
        }
        event.stop_propagation();
        let key = event.key();
        let on_radio = event
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .and_then(|t| t.closest("#welcome-looks [role=\"radio\"]").ok().flatten())
            .and_then(|radio| {
                self.radios()
                    .iter()
                    .position(|r| dom::same(Some(&radio), r))
            });
        if key == "Escape" {
            event.prevent_default();
            self.close();
        } else if let Some(index) = on_radio
            && matches!(
                key.as_str(),
                "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown"
            )
        {
            event.prevent_default();
            let back = key == "ArrowLeft" || key == "ArrowUp";
            let next = if back {
                (index + LOOKS.len() - 1) % LOOKS.len()
            } else {
                (index + 1) % LOOKS.len()
            };
            self.choose(next);
        } else if key == "Tab" {
            self.trap_tab(&event);
        }
    }

    /// The card's tab stops, in order: ×, the checked look, the places to
    /// start and Skip. From the card itself, Shift+Tab goes to the last.
    fn trap_tab(self, event: &KeyboardEvent) {
        let Some(panel) = self.panel.get() else {
            return;
        };
        let stops = dom::tab_stops(&panel, "button");
        let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
            return;
        };
        let active = dom::active_element();
        let on_panel = dom::same(active.as_ref(), &panel);
        let inside = active
            .as_ref()
            .is_some_and(|active| panel.contains(Some(active)));
        if !inside || (on_panel && event.shift_key()) {
            event.prevent_default();
            let _ = if event.shift_key() { last } else { first }.focus();
        } else if event.shift_key() && dom::same(active.as_ref(), first) {
            event.prevent_default();
            let _ = last.focus();
        } else if !event.shift_key() && dom::same(active.as_ref(), last) {
            event.prevent_default();
            let _ = first.focus();
        }
    }

    fn listen(self) {
        dom::on_document_event(bridge::WELCOME_EVENT, move |event: CustomEvent| {
            event.prevent_default();
            match bridge::welcome_request(&event) {
                Ok(request) => self.request(request, bridge::return_focus(&event)),
                Err(e) => crate::ipc::log_error("The welcome request couldn't be read", &e),
            }
        });
        dom::on_window_keydown(move |event: KeyboardEvent| self.on_keydown(event));
    }
}

/// The welcome card, mounted once, hidden until it's asked for.
#[component]
pub fn WelcomeCard() -> impl IntoView {
    let welcome = Welcome {
        open: RwSignal::new(false),
        theme: RwSignal::new(String::new()),
        start_here: RwSignal::new(Vec::new()),
        return_to: StoredValue::new_local(None),
        overlay: NodeRef::new(),
        panel: NodeRef::new(),
        body: NodeRef::new(),
        looks: NodeRef::new(),
    };
    welcome.listen();

    // The look in effect is checked (neither, for another theme), and is
    // the group's one tab stop (the first, if neither).
    let checked = move |index: usize| {
        welcome
            .theme
            .with(|theme| LOOKS.get(index).is_some_and(|(value, _)| value == theme))
    };
    let tab_stop = move |index: usize| {
        let any = (0..LOOKS.len()).any(checked);
        if checked(index) || (!any && index == 0) {
            0
        } else {
            -1
        }
    };
    let looks = LOOKS
        .iter()
        .enumerate()
        .map(|(index, (value, name))| {
            view! {
                <button
                    type="button"
                    class="welcome-look"
                    role="radio"
                    aria-checked=move || checked(index).to_string()
                    tabindex=move || tab_stop(index)
                    data-value=*value
                    on:click=move |_| welcome.choose(index)
                >
                    <span class="welcome-look-preview" data-theme-preview=*value aria-hidden="true">
                        <span class="welcome-look-sample">"Aa"</span>
                        <span class="welcome-look-lines">
                            <span></span>
                            <span></span>
                            <span></span>
                        </span>
                    </span>
                    <span class="welcome-look-name">{*name}</span>
                </button>
            }
        })
        .collect_view();
    let start_rows = move || {
        welcome
            .start_here
            .get()
            .into_iter()
            .map(|row| {
                let title = row.title.clone();
                let line = row.line.clone();
                view! {
                    <li>
                        <button
                            type="button"
                            class="welcome-start-row"
                            data-book=row.book_id.to_string()
                            data-chapter=row.chapter.to_string()
                            on:click=move |_| welcome.start(&row)
                        >
                            <span class="welcome-start-title">{title}</span>
                            " "
                            <span class="welcome-start-line">{line}</span>
                        </button>
                    </li>
                }
            })
            .collect_view()
    };
    let no_places = move || welcome.start_here.with(Vec::is_empty);

    view! {
        <div
            id="welcome-overlay"
            class="welcome-overlay modal-overlay"
            hidden=move || !welcome.open.get()
            node_ref=welcome.overlay
            on:mousedown=move |ev: MouseEvent| {
                // Clicking the dimmed page behind the card does nothing, and
                // leaves focus where it was.
                if let Some(overlay) = welcome.overlay.get()
                    && dom::targets(&ev, &overlay)
                {
                    ev.prevent_default();
                }
            }
        >
            <div
                id="welcome-panel"
                class="welcome-panel modal-panel"
                role="dialog"
                aria-modal="true"
                aria-labelledby="welcome-title"
                aria-describedby="welcome-intro"
                tabindex="-1"
                node_ref=welcome.panel
            >
                <div class="welcome-head">
                    <h2 id="welcome-title" class="welcome-title">"Welcome to Gospel Getter"</h2>
                    <button
                        type="button"
                        id="welcome-close"
                        class="panel-close"
                        aria-label="Close"
                        title="Close (Esc)"
                        on:click=move |_| welcome.close()
                    >
                        "×"
                    </button>
                </div>
                <div class="welcome-body modal-body" node_ref=welcome.body>
                    <p id="welcome-intro" class="welcome-intro">
                        "A quiet place to read the Bible on your computer, offline and with no account."
                    </p>
                    <h3 class="section-label" id="welcome-look-label">"Pick a look"</h3>
                    <div
                        id="welcome-looks"
                        class="welcome-looks"
                        role="radiogroup"
                        aria-labelledby="welcome-look-label"
                        node_ref=welcome.looks
                    >
                        {looks}
                    </div>
                    <p class="welcome-hint">"More themes in Aa."</p>
                    <h3
                        class="section-label"
                        id="welcome-start-label"
                        hidden=no_places
                    >
                        "Where to start"
                    </h3>
                    <ul
                        id="welcome-start"
                        class="welcome-start"
                        aria-labelledby="welcome-start-label"
                        hidden=no_places
                    >
                        {start_rows}
                    </ul>
                </div>
                <div class="welcome-buttons modal-buttons">
                    <button
                        type="button"
                        id="welcome-skip"
                        class="reader-mode-btn"
                        on:click=move |_| welcome.close()
                    >
                        "Skip"
                    </button>
                </div>
            </div>
        </div>
    }
}
