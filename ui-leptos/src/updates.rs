//! Updates: Check now and its status line in About, the check at launch,
//! the "new version" banner with Skip this version and Later, the update
//! shown inside About when it's found there, installing with download
//! progress (from either), Download page for builds that can't update
//! themselves, and About's Go back to the release before this one. The backend
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
//! Skip this version remembers that exact version on this computer
//! (`update_text::SKIPPED_KEY`): the launch check then stays quiet for it,
//! a newer one shows the banner again, and Check now still shows it in
//! About, noting it was skipped, with Update now (`update_text::launch_offer`).
//! Later is for this session only.
//!
//! Go back: beside each check, the backend is asked for the release this
//! one's `previous.json` names (`check_previous_version`, quietly: none
//! means no button). A release installer whose previous release can use
//! the data gets "Go back to <version>"; one that can't gets a line saying
//! why. Pressing it asks first (its confirm dialog, an overlay over About,
//! `Overlay::GoBack`); then the backend saves a whole backup beside the
//! database, the version left behind is skipped, and the release is
//! downloaded, verified and installed as Update now does, with the same
//! progress (in About, and the banner if About closes). A failed backup
//! stops it there: nothing is installed. Both steps name the version the
//! dialog named, and the backend refuses them if it offers another by
//! then.
//!
//! Each event writes the DOM within itself (the views, the status lines,
//! focus), then any command it starts runs on; the frontend tests look at
//! the DOM right after a click, while a check is still on its way.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::Closure;
use web_sys::{Element, HtmlElement};

use crate::bridge;
use crate::dom;
use crate::dto::{BackupSaved, DownloadEvent, NoteBlock, NoteSpan, PreviousInfo, UpdateInfo};
use crate::ipc;
use crate::overlays::Overlay;
use crate::surfaces::{self, Surface};
use crate::update_text::{self, Download, Offer};

/// The banner or About: the two places an update shows; and About's Go
/// back, where going back shows its progress (and the banner, if About
/// closes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Banner,
    About,
    GoBack,
}

const BOTH: [View; 2] = [View::Banner, View::About];

/// The version the reader skipped, if any.
fn skipped() -> Option<String> {
    dom::storage()?.get_item(update_text::SKIPPED_KEY).ok()?
}

/// Skip `version` from now on; quietly not, if local storage can't be
/// written.
fn skip_version(version: &str) {
    if let Some(storage) = dom::storage() {
        let _ = storage.set_item(update_text::SKIPPED_KEY, version);
    }
}

/// What the download's channel calls with each message.
type OnMessage = Closure<dyn FnMut(JsValue)>;

thread_local! {
    /// The updater, once it has mounted (`None` before: see `page`).
    static UPDATES: Cell<Option<Updates>> = const { Cell::new(None) };
}

/// Boot is done: the quiet launch check runs, unless the reader turned it
/// off (`html[data-update-check]` is `off`; the preference is `aa`'s).
/// Whether the updater is there to be asked.
pub fn launch_check() -> bool {
    let Some(updates) = UPDATES.get() else {
        return false;
    };
    if bridge::launch_check_on() {
        updates.check(true);
    }
    true
}

/// A header surface closed (`surfaces`): About closing part way through an
/// install brings the banner back.
pub fn surface_closed(surface: Surface) {
    if surface == Surface::About
        && let Some(updates) = UPDATES.get()
    {
        updates.about_closed();
    }
}

/// The updater's state and the elements it writes, shared by the banner and
/// About's Updates section.
#[derive(Clone, Copy)]
pub struct Updates {
    /// An install is under way.
    installing: StoredValue<bool>,
    /// How many checks are on their way (the launch check's, Check now's):
    /// Check now is pressed again only once none is (D2-03).
    checks: StoredValue<u32>,
    /// The update the last check found.
    offered: StoredValue<Option<UpdateInfo>>,
    /// The release Go back offers (or says it can't), as the last check
    /// found it.
    previous: StoredValue<Option<PreviousInfo>>,
    /// The release being gone back to, while it is.
    going_back: StoredValue<Option<String>>,
    /// The release the confirm dialog names, while it's open: the one
    /// going back installs, though a check may offer another meanwhile.
    confirming: StoredValue<Option<String>>,
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
    pub banner_skip: NodeRef<html::Button>,
    pub banner_later: NodeRef<html::Button>,

    // About's Updates section.
    pub check_button: NodeRef<html::Button>,
    pub check_status: NodeRef<html::P>,
    pub about_update: NodeRef<html::Div>,
    pub about_title: NodeRef<html::H4>,
    pub about_skipped: NodeRef<html::P>,
    pub about_notes: NodeRef<html::Div>,
    pub about_progress: NodeRef<html::Progress>,
    pub about_status: NodeRef<html::P>,
    pub about_install: NodeRef<html::Button>,
    pub about_download: NodeRef<html::Button>,
    pub about_close: NodeRef<html::Button>,

    // About's Go back, and its confirm dialog.
    pub go_back_box: NodeRef<html::Div>,
    pub go_back_button: NodeRef<html::Button>,
    pub go_back_unavailable: NodeRef<html::P>,
    pub go_back_saved: NodeRef<html::P>,
    pub go_back_progress: NodeRef<html::Progress>,
    pub go_back_status: NodeRef<html::P>,
    pub confirm: NodeRef<html::Div>,
    pub confirm_panel: NodeRef<html::Div>,
    pub confirm_title: NodeRef<html::H2>,
    pub confirm_body: NodeRef<html::P>,
    pub confirm_cancel: NodeRef<html::Button>,
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
            checks: StoredValue::new(0),
            offered: StoredValue::new(None),
            previous: StoredValue::new(None),
            going_back: StoredValue::new(None),
            confirming: StoredValue::new(None),
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
            banner_skip: NodeRef::new(),
            banner_later: NodeRef::new(),
            check_button: NodeRef::new(),
            check_status: NodeRef::new(),
            about_update: NodeRef::new(),
            about_title: NodeRef::new(),
            about_skipped: NodeRef::new(),
            about_notes: NodeRef::new(),
            about_progress: NodeRef::new(),
            about_status: NodeRef::new(),
            about_install: NodeRef::new(),
            about_download: NodeRef::new(),
            about_close: NodeRef::new(),
            go_back_box: NodeRef::new(),
            go_back_button: NodeRef::new(),
            go_back_unavailable: NodeRef::new(),
            go_back_saved: NodeRef::new(),
            go_back_progress: NodeRef::new(),
            go_back_status: NodeRef::new(),
            confirm: NodeRef::new(),
            confirm_panel: NodeRef::new(),
            confirm_title: NodeRef::new(),
            confirm_body: NodeRef::new(),
            confirm_cancel: NodeRef::new(),
        }
    }

    /// Where the download under way shows: the banner and About's update,
    /// or, going back, the banner and About's Go back.
    fn views(self) -> [View; 2] {
        if self.going_back.with_value(Option::is_some) {
            [View::Banner, View::GoBack]
        } else {
            BOTH
        }
    }

    fn root(self, view: View) -> Option<HtmlElement> {
        match view {
            View::Banner => self.banner.get_untracked(),
            View::About => self.about_update.get_untracked().map(Into::into),
            View::GoBack => self.go_back_box.get_untracked().map(Into::into),
        }
    }

    fn progress_bar(self, view: View) -> Option<web_sys::HtmlProgressElement> {
        match view {
            View::Banner => self.banner_progress.get_untracked(),
            View::About => self.about_progress.get_untracked(),
            View::GoBack => self.go_back_progress.get_untracked(),
        }
    }

    fn status(self, view: View) -> Option<web_sys::HtmlParagraphElement> {
        match view {
            View::Banner => self.banner_status.get_untracked(),
            View::About => self.about_status.get_untracked(),
            View::GoBack => self.go_back_status.get_untracked(),
        }
    }

    fn install_button(self, view: View) -> Option<web_sys::HtmlButtonElement> {
        match view {
            View::Banner => self.banner_install.get_untracked(),
            View::About => self.about_install.get_untracked(),
            View::GoBack => None,
        }
    }

    fn download_button(self, view: View) -> Option<web_sys::HtmlButtonElement> {
        match view {
            View::Banner => self.banner_download.get_untracked(),
            View::About => self.about_download.get_untracked(),
            View::GoBack => None,
        }
    }

    /// Later (the banner) or Close (About).
    fn dismiss_button(self, view: View) -> Option<web_sys::HtmlButtonElement> {
        match view {
            View::Banner => self.banner_later.get_untracked(),
            View::About => self.about_close.get_untracked(),
            View::GoBack => None,
        }
    }

    /// Skip this version (the banner's).
    fn skip_button(self, view: View) -> Option<web_sys::HtmlButtonElement> {
        match view {
            View::Banner => self.banner_skip.get_untracked(),
            View::About | View::GoBack => None,
        }
    }

    /// The status line where the download under way shows (both views).
    fn set_update_status(self, message: &str, is_error: bool) {
        for view in self.views() {
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
        hide(self.skip_button(view), false);
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
        hide(
            self.about_skipped.get_untracked(),
            skipped().as_deref() != Some(&*update.version),
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
    /// unless About has already shown it; nowhere if the launch check
    /// (`quiet`) found the version the reader skipped. The update being
    /// installed stays the one offered.
    fn offer(self, update: UpdateInfo, quiet: bool) {
        if self.installing.get_value() {
            if Overlay::About.is_open()
                && self.going_back.with_value(Option::is_none)
                && let Some(offered) = self.offered.get_value()
            {
                self.show_in_about(&offered);
            }
            return;
        }
        let offer = update_text::launch_offer(&update.version, skipped().as_deref(), quiet);
        if offer == Offer::Quiet {
            return;
        }
        self.offered.set_value(Some(update.clone()));
        if Overlay::About.is_open() || offer == Offer::Skipped {
            self.show_in_about(&update);
        } else if !self.shown_in_about.get_value() {
            self.show_banner(&update);
        }
    }

    /// Checks for an update: quietly at launch (nothing is said, whatever
    /// happens), or for Check now, which says how it went. What the reader
    /// sees changes at once; the check runs on. Check now stays disabled
    /// until no check is on its way, the quiet one included, and a check is
    /// on its way until Go back's release is in too.
    pub fn check(self, quiet: bool) {
        if self.going_back.with_value(Option::is_some) {
            return;
        }
        let button = self.check_button.get_untracked();
        self.checks.update_value(|n| *n += 1);
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
                Ok(Some(update)) => self.offer(update, quiet),
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
            // Go back's release, quietly, once the update's answer is in.
            let previous =
                ipc::invoke::<Option<PreviousInfo>>("check_previous_version", JsValue::UNDEFINED)
                    .await;
            self.show_previous(previous.ok().flatten());
            let running = self.checks.try_update_value(|n| {
                *n = n.saturating_sub(1);
                *n
            });
            if running == Some(0)
                && self.going_back.with_value(Option::is_none)
                && let Some(button) = button
            {
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
            hide(self.skip_button(view), true);
            hide(self.dismiss_button(view), true);
            if let Some(bar) = self.progress_bar(view) {
                bar.set_hidden(false);
                let _ = bar.remove_attribute("value");
            }
        }
        hide(self.go_back_button.get_untracked(), true);
        self.set_update_status(update_text::DOWNLOADING, false);
        if had_focus && let Some(root) = self.root(from) {
            let _ = root.focus();
        }

        let channel = match self.channel(Download::default()) {
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

    /// A Tauri `Channel` (`window.__TAURI__.core.Channel`) whose messages
    /// move `download`'s progress, where it shows.
    fn channel(self, download: Download) -> Result<JsValue, JsValue> {
        let window: JsValue = web_sys::window()
            .ok_or_else(|| JsValue::from_str("no window"))?
            .into();
        let core = ["__TAURI__", "core"].iter().try_fold(window, |at, key| {
            js_sys::Reflect::get(&at, &JsValue::from_str(key))
        })?;
        let class = js_sys::Reflect::get(&core, &JsValue::from_str("Channel"))?
            .dyn_into::<js_sys::Function>()?;
        let channel = js_sys::Reflect::construct(&class, &js_sys::Array::new())?;

        let download = Rc::new(RefCell::new(download));
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
        for view in self.views() {
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
        self.draw_previous();
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

    /// Skip this version: that exact version is out of the way at every
    /// launch from now on (a newer one isn't), and out of the way now.
    pub fn skip(self) {
        if let Some(offered) = self.offered.get_value() {
            skip_version(&offered.version);
        }
        self.hide_banner();
    }

    /// About closed part way through an install, or through going back:
    /// the banner carries on showing it.
    fn about_closed(self) {
        let banner_hidden = self.banner.get_untracked().is_none_or(|b| b.hidden());
        if !self.installing.get_value() || !banner_hidden {
            return;
        }
        if let Some(version) = self.going_back.get_value() {
            set_text(
                self.banner_title.get_untracked(),
                &update_text::going_back(&version),
            );
            hide(self.banner_notes.get_untracked(), true);
            for button in [
                self.install_button(View::Banner),
                self.download_button(View::Banner),
                self.skip_button(View::Banner),
                self.dismiss_button(View::Banner),
            ] {
                hide(button, true);
            }
            hide(self.banner_progress.get_untracked(), false);
            hide(self.banner.get_untracked(), false);
            return;
        }
        let Some(offered) = self.offered.get_value() else {
            return;
        };
        self.fill_banner(&offered);
        hide(self.banner.get_untracked(), false);
    }

    /// The release Go back offers, as a check found it (`None`: no
    /// `previous.json`, offline, or not a release this build may go back
    /// to). Not while an install or going back is under way: what's
    /// showing stays.
    fn show_previous(self, previous: Option<PreviousInfo>) {
        if self.installing.get_value() {
            return;
        }
        self.previous.set_value(previous);
        // What the last Go back said is over: this check's answer stands.
        set_text(self.go_back_saved.get_untracked(), "");
        hide(self.go_back_saved.get_untracked(), true);
        let status = self.go_back_status.get_untracked();
        set_text(status.clone(), "");
        set_error(status, false);
        self.draw_previous();
    }

    /// About's Go back as the last check found it: the button, if that
    /// release can use the data; the line saying why not, if it can't;
    /// neither, if there's none.
    fn draw_previous(self) {
        let previous = self.previous.get_value();
        let offered = previous.as_ref().filter(|p| p.reads_this_data);
        if let Some(button) = self.go_back_button.get_untracked() {
            if let Some(offered) = offered {
                button.set_text_content(Some(&update_text::go_back(&offered.version)));
            }
            button.set_hidden(offered.is_none());
            button.set_disabled(false);
        }
        let unavailable = previous.as_ref().filter(|p| !p.reads_this_data);
        set_text(
            self.go_back_unavailable.get_untracked(),
            &unavailable
                .map(|p| update_text::go_back_unavailable(&p.version))
                .unwrap_or_default(),
        );
        hide(
            self.go_back_unavailable.get_untracked(),
            unavailable.is_none(),
        );
        hide(self.go_back_progress.get_untracked(), true);
        hide(self.go_back_box.get_untracked(), previous.is_none());
    }

    /// The release Go back would install now, if it may.
    fn previous_to_offer(self) -> Option<String> {
        self.previous.with_value(|p| {
            p.as_ref()
                .filter(|p| p.reads_this_data)
                .map(|p| p.version.clone())
        })
    }

    /// Go back to <version>: asks first, in its confirm dialog over About.
    pub fn ask_to_go_back(self) {
        if self.installing.get_value() {
            return;
        }
        let Some(version) = self.previous_to_offer() else {
            return;
        };
        self.confirming.set_value(Some(version.clone()));
        set_text(
            self.confirm_title.get_untracked(),
            &update_text::go_back_title(&version),
        );
        set_text(
            self.confirm_body.get_untracked(),
            &update_text::go_back_body(&version),
        );
        hide(self.confirm.get_untracked(), false);
        if let Some(cancel) = self.confirm_cancel.get_untracked() {
            dom::focus(&cancel);
        }
    }

    /// The confirm dialog closes (Cancel, Escape, a click on the dimmed
    /// page): focus goes back to Go back, and About stays open.
    fn close_confirm(self) {
        let Some(confirm) = self.confirm.get_untracked() else {
            return;
        };
        if confirm.hidden() {
            return;
        }
        confirm.set_hidden(true);
        if let Some(button) = self.go_back_button.get_untracked()
            && dom::usable(&button)
        {
            dom::focus(&button);
        }
    }

    /// While the confirm dialog is open it has the keyboard: Escape closes
    /// it, Tab stays inside it, and nothing else hears a key, About's own
    /// handler included (which stands aside too, `about`).
    fn on_confirm_keydown(self, event: &web_sys::KeyboardEvent) {
        if !Overlay::GoBack.is_open() {
            return;
        }
        // About's own handler is on `window` too, so it would hear this key
        // next, with the dialog already closed: none of the window's other
        // handlers do.
        event.stop_immediate_propagation();
        match event.key().as_str() {
            "Escape" => {
                event.prevent_default();
                self.close_confirm();
            }
            "Tab" => {
                if let Some(panel) = self.confirm_panel.get_untracked() {
                    let stops = dom::focusable(&panel, "button");
                    dom::trap_tab(event, &panel, &stops);
                }
            }
            _ => {}
        }
    }

    /// Go back, confirmed: the backup first, beside the database; if it
    /// can't be saved, that's said and nothing is installed. Then the
    /// version left behind is skipped, and the release is downloaded,
    /// verified and installed, its progress shown as an update's.
    fn go_back(self) {
        self.close_confirm();
        if self.installing.get_value() {
            return;
        }
        if self.previous_to_offer().is_none() {
            return;
        }
        let Some(version) = self.confirming.get_value() else {
            return;
        };
        self.installing.set_value(true);
        self.going_back.set_value(Some(version.clone()));
        for view in BOTH {
            hide(self.install_button(view), true);
            hide(self.download_button(view), true);
            hide(self.skip_button(view), true);
        }
        if let Some(button) = self.check_button.get_untracked() {
            button.set_disabled(true);
        }
        hide(self.go_back_button.get_untracked(), true);
        set_text(self.go_back_saved.get_untracked(), "");
        hide(self.go_back_saved.get_untracked(), true);
        self.set_update_status(update_text::BACKING_UP, false);
        if let Some(root) = self.root(View::GoBack) {
            dom::focus(&root);
        }

        spawn_local(async move {
            let args = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &args,
                &JsValue::from_str("preferences"),
                &crate::aa::current(),
            );
            let _ = js_sys::Reflect::set(
                &args,
                &JsValue::from_str("version"),
                &JsValue::from_str(&version),
            );
            match ipc::invoke::<BackupSaved>("save_backup_before_going_back", args.into()).await {
                Ok(saved) => {
                    set_text(
                        self.go_back_saved.get_untracked(),
                        &update_text::backup_saved(&saved.path),
                    );
                    hide(self.go_back_saved.get_untracked(), false);
                }
                Err(e) => {
                    self.go_back_failed(&e);
                    return;
                }
            }
            // The version left behind, so it isn't offered straight back
            // (by a release that knows Skip).
            if let Some(this_version) = self.version.await {
                skip_version(&this_version);
            }
            let download = Download::going_back_to(&version);
            for view in self.views() {
                if let Some(bar) = self.progress_bar(view) {
                    let _ = bar.remove_attribute("value");
                }
            }
            hide(self.go_back_progress.get_untracked(), false);
            self.set_update_status(&download.downloading(), false);
            let channel = match self.channel(download) {
                Ok(channel) => channel,
                Err(e) => {
                    self.go_back_failed(&e);
                    return;
                }
            };
            let args = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&args, &JsValue::from_str("onEvent"), &channel);
            let _ = js_sys::Reflect::set(
                &args,
                &JsValue::from_str("version"),
                &JsValue::from_str(&version),
            );
            if let Err(e) = ipc::invoke_value("install_previous_version", args.into()).await {
                self.go_back_failed(&e);
            }
        });
    }

    /// Going back stopped (the backup, the download, the install): said
    /// where it showed, and everything is as it was before, Go back
    /// offered again.
    fn go_back_failed(self, error: &JsValue) {
        self.set_update_status(&ipc::error_text(error), true);
        self.installing.set_value(false);
        self.going_back.set_value(None);
        self.progress.set_value(None);
        hide(self.banner_progress.get_untracked(), true);
        // The banner, if About closed part way: Later puts it away.
        hide(self.dismiss_button(View::Banner), false);
        if let Some(offered) = self.offered.get_value() {
            self.reset_view(View::About, &offered);
        }
        if let Some(button) = self.check_button.get_untracked() {
            button.set_disabled(false);
        }
        self.draw_previous();
        if let Some(button) = self.go_back_button.get_untracked()
            && dom::usable(&button)
            && self
                .root(View::GoBack)
                .is_some_and(|root| dom::same(dom::active_element().as_ref(), &root))
        {
            dom::focus(&button);
        }
    }

    /// Is there for the launch check ([`launch_check`]) and About closing
    /// ([`surface_closed`]), and the confirm dialog has the keyboard while
    /// it's open.
    pub fn mounted(self) {
        UPDATES.set(Some(self));
        dom::on_window_keydown(move |event| self.on_confirm_keydown(&event));
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
                    id="update-skip"
                    class="reader-mode-btn"
                    node_ref=updates.banner_skip
                    on:click=move |_| updates.skip()
                >
                    "Skip this version"
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

/// About's Updates section: the launch check's setting (`aa`'s preference;
/// this only shows it), what checking sends, Check now with its
/// status line, and an update found while About is open.
#[component]
pub fn UpdatesSection(updates: Updates) -> impl IntoView {
    // On unless the reader turned it off: `html[data-update-check]`, which
    // the pre-paint script set from the stored preference before the page
    // drew.
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
                "Checking downloads a small file from GitHub that names the latest version, and, on an install that updates itself, one that names the version before yours, so GitHub sees your IP address, as with any download. Nothing about your reading or your data is sent."
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
                <p
                    id="about-update-skipped"
                    class="section-note"
                    hidden
                    node_ref=updates.about_skipped
                >
                    {update_text::SKIPPED_NOTE}
                </p>
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
                        on:click=move |_| surfaces::close(true)
                    >
                        "Close"
                    </button>
                </div>
            </div>
            <div
                id="go-back"
                class="go-back"
                role="group"
                aria-label="Go back to the previous version"
                tabindex="-1"
                hidden
                node_ref=updates.go_back_box
            >
                <button
                    type="button"
                    id="go-back-btn"
                    class="reader-mode-btn"
                    hidden
                    node_ref=updates.go_back_button
                    on:click=move |_| updates.ask_to_go_back()
                ></button>
                <p
                    id="go-back-unavailable"
                    class="section-note"
                    hidden
                    node_ref=updates.go_back_unavailable
                ></p>
                <p id="go-back-saved" class="update-status" hidden node_ref=updates.go_back_saved></p>
                <progress
                    id="go-back-progress"
                    class="update-progress"
                    aria-labelledby="go-back-status"
                    hidden
                    node_ref=updates.go_back_progress
                ></progress>
                <p
                    id="go-back-status"
                    class="update-status"
                    role="status"
                    aria-live="polite"
                    node_ref=updates.go_back_status
                ></p>
            </div>
        </section>
    }
}

/// Go back's confirm dialog: over About, which stays open behind it
/// (`Overlay::GoBack`). Cancel, Escape or a click on the dimmed page close
/// it, focus going back to Go back.
#[component]
pub fn GoBackConfirm(updates: Updates) -> impl IntoView {
    view! {
        <div
            id="go-back-overlay"
            class="modal-overlay"
            hidden
            node_ref=updates.confirm
            on:click=move |ev: web_sys::MouseEvent| {
                if let Some(overlay) = updates.confirm.get_untracked()
                    && dom::targets(&ev, &overlay)
                {
                    updates.close_confirm();
                }
            }
        >
            <div
                id="go-back-panel"
                class="modal-panel go-back-panel"
                role="dialog"
                aria-modal="true"
                aria-labelledby="go-back-title"
                aria-describedby="go-back-body"
                node_ref=updates.confirm_panel
            >
                <div class="modal-body">
                    <h2 id="go-back-title" class="note-title" node_ref=updates.confirm_title></h2>
                    <p id="go-back-body" node_ref=updates.confirm_body></p>
                </div>
                <div class="modal-buttons">
                    <button
                        type="button"
                        id="go-back-confirm"
                        class="import-confirm"
                        on:click=move |_| updates.go_back()
                    >
                        "Go back"
                    </button>
                    <button
                        type="button"
                        id="go-back-cancel"
                        class="reader-mode-btn"
                        node_ref=updates.confirm_cancel
                        on:click=move |_| updates.close_confirm()
                    >
                        "Cancel"
                    </button>
                </div>
            </div>
        </div>
    }
}
