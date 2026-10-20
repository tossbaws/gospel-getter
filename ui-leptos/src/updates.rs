//! Updates: Check now and its status line in About, the check at launch,
//! the "new version" banner with Later, the update shown inside About when
//! it's found there, installing with download progress (from either), and
//! Download page for builds that can't update themselves. The backend
//! decides what this copy can do (`updater.rs`): a release installer
//! downloads, verifies and installs the update (then restarts); any other
//! build links to the releases page. Nothing is installed until the reader
//! chooses to. A failed check at launch says nothing (the app works the
//! same offline); Check now says what went wrong. The check's time limit is
//! the backend's (`check_for_update` gives up after 20 s, as offline).
//!
//! Where an update shows: found while About is open (by Check now, or by
//! the launch check finishing), About shows it in place, with its notes,
//! Update now and Close, and the banner stays away for the rest of the
//! session, as after Later. Found by the launch check with About closed,
//! the banner shows it. Installing from either shows its progress in both,
//! and closing About part way through brings the banner up, so the download
//! never runs out of sight.
//!
//! This is the page's updater script (`ui/index.html` at 41e04f3, from
//! `withinUpdateCheckTime` to the launch check) in Rust: the same ids,
//! classes, ARIA, `hidden`, text and focus. As there, each event writes
//! the DOM within itself (the views, the status lines, focus), then any
//! command it starts runs on; the frontend tests look at the DOM right
//! after a click, while a check is still on its way.

use std::cell::RefCell;
use std::rc::Rc;

use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::Closure;
use web_sys::{CustomEvent, Element, HtmlElement};

use crate::bridge;
use crate::dom;
use crate::dto::{DownloadEvent, NoteBlock, NoteSpan, UpdateInfo};
use crate::ipc;
use crate::update_text::{self, Download};

/// The banner or About: the two places an update shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Banner,
    About,
}

const BOTH: [View; 2] = [View::Banner, View::About];

/// What the download's channel calls with each message.
type OnMessage = Closure<dyn FnMut(JsValue)>;

/// The updater's state and the elements it writes, shared by the banner and
/// About's Updates section.
#[derive(Clone, Copy)]
pub struct Updates {
    /// An install is under way.
    installing: StoredValue<bool>,
    /// The update the last check found.
    offered: StoredValue<Option<UpdateInfo>>,
    /// About has shown it (after which the banner stays away).
    shown_in_about: StoredValue<bool>,
    /// The app's version, as `get_app_version` read it (none if it
    /// couldn't be), for Check now's "latest version" line.
    pub version: LocalResource<Option<String>>,
    /// The channel the download reports on, kept while it runs.
    progress: StoredValue<Option<OnMessage>, LocalStorage>,

    // The banner.
    pub banner: NodeRef<html::Section>,
    pub banner_title: NodeRef<html::P>,
    pub banner_notes: NodeRef<html::Details>,
    pub banner_notes_text: NodeRef<html::Div>,
    pub banner_progress: NodeRef<html::Progress>,
    pub banner_status: NodeRef<html::P>,
    pub banner_install: NodeRef<html::Button>,
    pub banner_download: NodeRef<html::Button>,
    pub banner_later: NodeRef<html::Button>,

    // About's Updates section.
    pub check_button: NodeRef<html::Button>,
    pub check_status: NodeRef<html::P>,
    pub about_update: NodeRef<html::Div>,
    pub about_title: NodeRef<html::H4>,
    pub about_notes: NodeRef<html::Div>,
    pub about_progress: NodeRef<html::Progress>,
    pub about_status: NodeRef<html::P>,
    pub about_install: NodeRef<html::Button>,
    pub about_download: NodeRef<html::Button>,
    pub about_close: NodeRef<html::Button>,
}

fn hide(el: Option<impl AsRef<HtmlElement>>, hidden: bool) {
    if let Some(el) = el {
        el.as_ref().set_hidden(hidden);
    }
}

fn set_text(el: Option<impl AsRef<web_sys::Node>>, text: &str) {
    if let Some(el) = el {
        el.as_ref().set_text_content(Some(text));
    }
}

fn set_error(el: Option<impl AsRef<Element>>, is_error: bool) {
    if let Some(el) = el {
        let _ = el
            .as_ref()
            .class_list()
            .toggle_with_force("is-error", is_error);
    }
}

impl Updates {
    pub fn new(version: LocalResource<Option<String>>) -> Self {
        Self {
            installing: StoredValue::new(false),
            offered: StoredValue::new(None),
            shown_in_about: StoredValue::new(false),
            version,
            progress: StoredValue::new_local(None),
            banner: NodeRef::new(),
            banner_title: NodeRef::new(),
            banner_notes: NodeRef::new(),
            banner_notes_text: NodeRef::new(),
            banner_progress: NodeRef::new(),
            banner_status: NodeRef::new(),
            banner_install: NodeRef::new(),
            banner_download: NodeRef::new(),
            banner_later: NodeRef::new(),
            check_button: NodeRef::new(),
            check_status: NodeRef::new(),
            about_update: NodeRef::new(),
            about_title: NodeRef::new(),
            about_notes: NodeRef::new(),
            about_progress: NodeRef::new(),
            about_status: NodeRef::new(),
            about_install: NodeRef::new(),
            about_download: NodeRef::new(),
            about_close: NodeRef::new(),
        }
    }

    fn root(self, view: View) -> Option<HtmlElement> {
        match view {
            View::Banner => self.banner.get_untracked(),
            View::About => self.about_update.get_untracked().map(Into::into),
        }
    }

    fn progress_bar(self, view: View) -> Option<web_sys::HtmlProgressElement> {
        match view {
            View::Banner => self.banner_progress.get_untracked(),
            View::About => self.about_progress.get_untracked(),
        }
    }

    fn status(self, view: View) -> Option<web_sys::HtmlParagraphElement> {
        match view {
            View::Banner => self.banner_status.get_untracked(),
            View::About => self.about_status.get_untracked(),
        }
    }

    fn install_button(self, view: View) -> Option<web_sys::HtmlButtonElement> {
        match view {
            View::Banner => self.banner_install.get_untracked(),
            View::About => self.about_install.get_untracked(),
        }
    }

    fn download_button(self, view: View) -> Option<web_sys::HtmlButtonElement> {
        match view {
            View::Banner => self.banner_download.get_untracked(),
            View::About => self.about_download.get_untracked(),
        }
    }

    /// Later (the banner) or Close (About).
    fn dismiss_button(self, view: View) -> Option<web_sys::HtmlButtonElement> {
        match view {
            View::Banner => self.banner_later.get_untracked(),
            View::About => self.about_close.get_untracked(),
        }
    }

    /// Whether About is open (its overlay is the head script's to show and
    /// hide, as one of the page's surfaces).
    fn about_open() -> bool {
        bridge::document()
            .and_then(|d| d.get_element_by_id("about-overlay"))
            .and_then(|el| el.dyn_into::<HtmlElement>().ok())
            .is_some_and(|el| !el.hidden())
    }

    /// The status line in both views.
    fn set_update_status(self, message: &str, is_error: bool) {
        for view in BOTH {
            let status = self.status(view);
            set_text(status.clone(), message);
            set_error(status, is_error);
        }
    }

    /// Check now's status line.
    fn set_check_status(self, message: &str, is_error: bool) {
        let status = self.check_status.get_untracked();
        set_text(status.clone(), message);
        set_error(status, is_error);
    }

    fn reset_view(self, view: View, update: &UpdateInfo) {
        if let Some(install) = self.install_button(view) {
            install.set_hidden(!update.can_self_update);
            install.set_disabled(false);
        }
        hide(self.download_button(view), update.can_self_update);
        hide(self.dismiss_button(view), false);
        hide(self.progress_bar(view), true);
    }

    /// The banner's title and notes, for `update`.
    fn fill_banner(self, update: &UpdateInfo) {
        set_text(
            self.banner_title.get_untracked(),
            &update_text::available(&update.version),
        );
        hide(self.banner_notes.get_untracked(), update.notes.is_none());
        set_text(
            self.banner_notes_text.get_untracked(),
            update.notes.as_deref().unwrap_or(""),
        );
    }

    /// The banner, when About isn't showing the update.
    fn show_banner(self, update: &UpdateInfo) {
        if self.installing.get_value() {
            return;
        }
        self.fill_banner(update);
        if let Some(notes) = self.banner_notes.get_untracked() {
            notes.set_open(false);
        }
        self.reset_view(View::Banner, update);
        self.set_update_status("", false);
        hide(self.banner.get_untracked(), false);
    }

    /// About, showing the update in place of the banner. Not while an
    /// install is under way: wherever it's showing its progress (the banner,
    /// if it started there) keeps showing it, and Check now says so.
    fn show_in_about(self, update: &UpdateInfo) {
        if self.installing.get_value() {
            let version = self
                .offered
                .with_value(|o| o.as_ref().map(|o| o.version.clone()))
                .unwrap_or_else(|| update.version.clone());
            self.set_check_status(&update_text::already_downloading(&version), false);
            return;
        }
        self.shown_in_about.set_value(true);
        self.hide_banner();
        set_text(
            self.about_title.get_untracked(),
            &update_text::available(&update.version),
        );
        if let Some(notes) = self.about_notes.get_untracked() {
            render_note_blocks(&notes, &update.note_blocks);
            notes.set_hidden(notes.child_element_count() == 0);
            let _ = notes.set_attribute("aria-label", &update_text::whats_new_in(&update.version));
        }
        self.reset_view(View::About, update);
        self.set_update_status("", false);
        hide(self.about_update.get_untracked(), false);
        // Said once, by the check's own status line; the heading shows it.
        self.set_check_status(&update_text::available_sentence(&update.version), false);
        if let Some(status) = self.check_status.get_untracked() {
            let _ = status.class_list().add_1("visually-hidden");
        }
    }

    fn hide_banner(self) {
        let Some(banner) = self.banner.get_untracked() else {
            return;
        };
        let had_focus = dom::active_element().is_some_and(|active| banner.contains(Some(&active)));
        banner.set_hidden(true);
        if had_focus {
            bridge::focus_home();
        }
    }

    /// Where an offered update shows: About if it's open, else the banner,
    /// unless About has already shown it. The update being installed stays
    /// the one offered.
    fn offer(self, update: UpdateInfo) {
        if self.installing.get_value() {
            if Self::about_open()
                && let Some(offered) = self.offered.get_value()
            {
                self.show_in_about(&offered);
            }
            return;
        }
        self.offered.set_value(Some(update.clone()));
        if Self::about_open() {
            self.show_in_about(&update);
        } else if !self.shown_in_about.get_value() {
            self.show_banner(&update);
        }
    }

    /// Checks for an update: quietly at launch (nothing is said, whatever
    /// happens), or for Check now, which says how it went. What the reader
    /// sees changes at once; the check runs on.
    pub fn check(self, quiet: bool) {
        let button = self.check_button.get_untracked();
        if !quiet {
            if let Some(button) = &button {
                button.set_disabled(true);
            }
            if let Some(status) = self.check_status.get_untracked() {
                let _ = status.class_list().remove_1("visually-hidden");
            }
            self.set_check_status(update_text::CHECKING, false);
        }
        spawn_local(async move {
            match ipc::invoke::<Option<UpdateInfo>>("check_for_update", JsValue::UNDEFINED).await {
                Ok(Some(update)) => self.offer(update),
                Ok(None) => {
                    if !quiet {
                        let version = self.version.await;
                        self.set_check_status(&update_text::latest(version.as_deref()), false);
                    }
                }
                Err(e) => {
                    if !quiet {
                        self.set_check_status(&ipc::error_text(&e), true);
                    }
                }
            }
            if let Some(button) = button {
                button.set_disabled(false);
            }
        });
    }

    /// Update now (About) or Update and restart (the banner). Both views
    /// show the download at once; it reports its progress on a Tauri
    /// `Channel`, and only answers if something went wrong (on success the
    /// app restarts into the new version).
    pub fn install(self, from: View) {
        if self.installing.get_value() {
            return;
        }
        let Some(offered) = self.offered.get_value() else {
            return;
        };
        self.installing.set_value(true);
        // The reader can go on reading while it downloads. The button goes,
        // so focus moves to its panel rather than being lost.
        let had_focus = self.install_button(from).is_some_and(|install| {
            let install: &Element = install.as_ref();
            dom::same(dom::active_element().as_ref(), install)
        });
        for view in BOTH {
            hide(self.install_button(view), true);
            hide(self.download_button(view), true);
            hide(self.dismiss_button(view), true);
            if let Some(bar) = self.progress_bar(view) {
                bar.set_hidden(false);
                let _ = bar.remove_attribute("value");
            }
        }
        self.set_update_status(update_text::DOWNLOADING, false);
        if had_focus && let Some(root) = self.root(from) {
            let _ = root.focus();
        }

        let channel = match self.channel() {
            Ok(channel) => channel,
            Err(e) => {
                self.install_failed(from, &offered, &e);
                return;
            }
        };
        spawn_local(async move {
            let args = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&args, &JsValue::from_str("onEvent"), &channel);
            if let Err(e) = ipc::invoke_value("install_update", args.into()).await {
                self.install_failed(from, &offered, &e);
            }
        });
    }

    /// A Tauri `Channel` (`window.__TAURI__.core.Channel`, as the page made
    /// it) whose messages move both views' progress.
    fn channel(self) -> Result<JsValue, JsValue> {
        let window: JsValue = web_sys::window()
            .ok_or_else(|| JsValue::from_str("no window"))?
            .into();
        let core = ["__TAURI__", "core"].iter().try_fold(window, |at, key| {
            js_sys::Reflect::get(&at, &JsValue::from_str(key))
        })?;
        let class = js_sys::Reflect::get(&core, &JsValue::from_str("Channel"))?
            .dyn_into::<js_sys::Function>()?;
        let channel = js_sys::Reflect::construct(&class, &js_sys::Array::new())?;

        let download = Rc::new(RefCell::new(Download::default()));
        let on_message = OnMessage::new(move |message: JsValue| {
            let Ok(event) = serde_wasm_bindgen::from_value::<DownloadEvent>(message) else {
                return;
            };
            let step = download.borrow_mut().on_event(&event);
            self.show_step(&step);
        });
        js_sys::Reflect::set(
            &channel,
            &JsValue::from_str("onmessage"),
            on_message.as_ref().unchecked_ref(),
        )?;
        self.progress.set_value(Some(on_message));
        Ok(channel)
    }

    fn show_step(self, step: &update_text::Step) {
        for view in BOTH {
            if let Some(bar) = self.progress_bar(view) {
                if let Some(max) = step.max {
                    #[allow(clippy::cast_precision_loss)]
                    bar.set_max(max as f64);
                }
                if let Some(value) = step.value {
                    #[allow(clippy::cast_precision_loss)]
                    bar.set_value(value as f64);
                }
            }
        }
        if let Some(status) = &step.status {
            self.set_update_status(status, false);
        }
    }

    fn install_failed(self, from: View, offered: &UpdateInfo, error: &JsValue) {
        self.installing.set_value(false);
        self.progress.set_value(None);
        for view in BOTH {
            self.reset_view(view, offered);
        }
        self.set_update_status(&ipc::error_text(error), true);
        let active = dom::active_element();
        let focus_was_here = match (&active, self.root(from)) {
            (Some(active), Some(root)) => root.contains(Some(active)),
            _ => false,
        };
        let on_body = match (&active, bridge::document().and_then(|d| d.body())) {
            (Some(active), Some(body)) => dom::same(Some(active), &body),
            _ => false,
        };
        if (focus_was_here || on_body)
            && let Some(install) = self.install_button(from)
        {
            let _ = install.focus();
        }
    }

    /// Download page: the backend opens the one releases page.
    pub fn open_download_page(self) {
        spawn_local(async move {
            match ipc::invoke_value("open_download_page", JsValue::UNDEFINED).await {
                Ok(_) => self.set_update_status(update_text::RELEASES_OPEN, false),
                Err(e) => self.set_update_status(&ipc::error_text(&e), true),
            }
        });
    }

    /// Later: out of the way until the next launch (or Check now).
    pub fn later(self) {
        self.hide_banner();
    }

    /// About closed part way through an install: the banner carries on
    /// showing it.
    fn about_closed(self) {
        let banner_hidden = self.banner.get_untracked().is_none_or(|b| b.hidden());
        if !self.installing.get_value() || !banner_hidden {
            return;
        }
        let Some(offered) = self.offered.get_value() else {
            return;
        };
        self.fill_banner(&offered);
        hide(self.banner.get_untracked(), false);
    }

    /// Listens for the page: About closing (the head script's
    /// `gospel-getter:surface-close`), and the end of boot, when the launch
    /// check runs unless the reader turned it off (`html[data-update-check]`
    /// is `off`; the preference is the head script's).
    pub fn listen(self) {
        dom::on_document_event(bridge::SURFACE_CLOSE_EVENT, move |event: CustomEvent| {
            if bridge::surface_closed(&event).as_deref() == Some("about") {
                self.about_closed();
            }
        });
        dom::on_document_event(bridge::LAUNCH_CHECK_EVENT, move |event: CustomEvent| {
            event.prevent_default();
            if bridge::launch_check_on() {
                self.check(true);
            }
        });
    }
}

/// The notes as the backend read them (headings, paragraphs, lists, bold
/// and code), built as elements holding text: never markup.
fn render_note_blocks(container: &HtmlElement, blocks: &[NoteBlock]) {
    let Some(document) = bridge::document() else {
        return;
    };
    container.replace_children_with_node_0();
    let make = |tag: &str, spans: &[NoteSpan]| -> Option<Element> {
        let el = document.create_element(tag).ok()?;
        for span in spans {
            let mut node: web_sys::Node = document.create_text_node(&span.text).into();
            for (wrap, tag) in [(span.code, "code"), (span.strong, "strong")] {
                if wrap {
                    let outer = document.create_element(tag).ok()?;
                    outer.append_with_node_1(&node).ok()?;
                    node = outer.into();
                }
            }
            el.append_with_node_1(&node).ok()?;
        }
        Some(el)
    };
    // The lists open at each depth, outermost first.
    let mut lists: Vec<Element> = Vec::new();
    for block in blocks {
        let (depth, el) = match block {
            NoteBlock::Heading { spans } => (None, make("h5", spans)),
            NoteBlock::Paragraph { spans } => (None, make("p", spans)),
            NoteBlock::Item { depth, spans } => (Some(usize::from(*depth)), make("li", spans)),
        };
        let Some(el) = el else {
            return;
        };
        let Some(depth) = depth else {
            lists.clear();
            let _ = container.append_with_node_1(&el);
            continue;
        };
        // A list as deep as the item, inside the last item of the list
        // above it.
        lists.truncate(depth + 1);
        while lists.len() < depth + 1 {
            let Ok(list) = document.create_element("ul") else {
                return;
            };
            let parent: Element = match lists.last() {
                Some(above) => above.last_element_child().unwrap_or_else(|| above.clone()),
                None => container.clone().into(),
            };
            let _ = parent.append_with_node_1(&list);
            lists.push(list);
        }
        if let Some(list) = lists.get(depth) {
            let _ = list.append_with_node_1(&el);
        }
    }
}

/// The banner: the update the launch check found while About was closed,
/// or the download About started when About closes part way through.
#[component]
pub fn UpdateBanner(updates: Updates) -> impl IntoView {
    view! {
        <section
            id="update-banner"
            class="update-banner"
            aria-labelledby="update-title"
            tabindex="-1"
            hidden
            node_ref=updates.banner
        >
            <p
                id="update-title"
                class="update-title"
                role="status"
                aria-live="polite"
                node_ref=updates.banner_title
            ></p>
            <details id="update-notes" hidden node_ref=updates.banner_notes>
                <summary>"What’s new"</summary>
                <div
                    id="update-notes-text"
                    class="update-notes-text"
                    node_ref=updates.banner_notes_text
                ></div>
            </details>
            <progress
                id="update-progress"
                class="update-progress"
                aria-labelledby="update-status"
                hidden
                node_ref=updates.banner_progress
            ></progress>
            <p
                id="update-status"
                class="update-status"
                role="status"
                aria-live="polite"
                node_ref=updates.banner_status
            ></p>
            <div class="update-actions">
                <button
                    type="button"
                    id="update-install"
                    class="import-confirm"
                    node_ref=updates.banner_install
                    on:click=move |_| updates.install(View::Banner)
                >
                    "Update and restart"
                </button>
                <button
                    type="button"
                    id="update-download"
                    class="import-confirm"
                    hidden
                    node_ref=updates.banner_download
                    on:click=move |_| updates.open_download_page()
                >
                    "Download page"
                </button>
                <button
                    type="button"
                    id="update-later"
                    class="reader-mode-btn"
                    node_ref=updates.banner_later
                    on:click=move |_| updates.later()
                >
                    "Later"
                </button>
            </div>
        </section>
    }
}

/// About's Updates section: the launch check's setting (the head script's
/// preference; this only shows it), what checking sends, Check now with its
/// status line, and an update found while About is open.
#[component]
pub fn UpdatesSection(updates: Updates) -> impl IntoView {
    // On unless the reader turned it off: `html[data-update-check]`, which
    // the head script set from the stored preference before the page drew.
    let launch_check = bridge::launch_check_on();
    view! {
        <section class="about-updates" aria-labelledby="updates-label">
            <h3 class="section-label" id="updates-label">"Updates"</h3>
            <label class="update-check-option">
                <input type="checkbox" id="update-check-toggle" prop:checked=launch_check />
                " "
                <span>"Check for updates when Gospel Getter starts"</span>
            </label>
            <p class="section-note" id="update-note">
                "Checking downloads a small file from GitHub that names the latest version, so GitHub sees your IP address, as with any download. Nothing about your reading or your data is sent."
            </p>
            <button
                type="button"
                id="update-check-btn"
                class="reader-mode-btn"
                aria-describedby="update-note"
                node_ref=updates.check_button
                on:click=move |_| updates.check(false)
            >
                "Check now"
            </button>
            <p
                id="update-check-status"
                class="data-status"
                role="status"
                aria-live="polite"
                node_ref=updates.check_status
            ></p>
            <div
                id="about-update"
                class="about-update"
                role="group"
                aria-labelledby="about-update-title"
                tabindex="-1"
                hidden
                node_ref=updates.about_update
            >
                <h4
                    id="about-update-title"
                    class="about-update-title"
                    node_ref=updates.about_title
                ></h4>
                <div
                    id="about-update-notes"
                    class="about-update-notes"
                    role="region"
                    tabindex="0"
                    hidden
                    node_ref=updates.about_notes
                ></div>
                <progress
                    id="about-update-progress"
                    class="update-progress"
                    aria-labelledby="about-update-status"
                    hidden
                    node_ref=updates.about_progress
                ></progress>
                <p
                    id="about-update-status"
                    class="update-status"
                    role="status"
                    aria-live="polite"
                    node_ref=updates.about_status
                ></p>
                <div class="update-actions">
                    <button
                        type="button"
                        id="about-update-install"
                        class="import-confirm"
                        node_ref=updates.about_install
                        on:click=move |_| updates.install(View::About)
                    >
                        "Update now"
                    </button>
                    <button
                        type="button"
                        id="about-update-download"
                        class="import-confirm"
                        hidden
                        node_ref=updates.about_download
                        on:click=move |_| updates.open_download_page()
                    >
                        "Download page"
                    </button>
                    <button
                        type="button"
                        id="about-update-close"
                        class="reader-mode-btn"
                        node_ref=updates.about_close
                        on:click=move |_| bridge::close_surfaces()
                    >
                        "Close"
                    </button>
                </div>
            </div>
        </section>
    }
}
