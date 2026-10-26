//! The checks, in the order a person would meet them: the app starts and
//! its page boots (1), and on a fresh install the welcome greets them (5);
//! the Leptos pieces mounted and the Content Security Policy blocked
//! nothing (2, judged at the end of the run); Export notes end to end (3);
//! the note editor editing and asking before discarding (6); the keyboard
//! shortcuts sheet (7); Export data… then Import data… to the preview (8);
//! the Library's Bookmarks, Highlights and Notes, each filled from the
//! reader, opening a row and removing one (14 to 16, `library`); search, a
//! reference gone to and a palette command run (17 to 19, `search`); the
//! book and chapter picker as a popover and as the sidebar, Random chapter,
//! its keyboard, and the sidebar keeping the reading place (20 to 24,
//! `picker`); a translation switch, Compare and reading mode (27 to 29),
//! and every Aa control (25, `display`); the header's popovers and panels:
//! one at a time, Escape's order, a click outside, the ☰ menu by keyboard,
//! About's Tab, G and the sidebar's button, and the window crossing 1200 px
//! (30 to 37, `surfaces`); the reader: the chapter's end, Compare's grid,
//! the chapter's lists, a cross-reference and the mouse's back and forward
//! buttons, Psalm 119 and Try again (38 to 43, `reader`); the selection
//! card: verses selected with the mouse and the card placed by them, Copy
//! and Copy as… read back from the clipboard, a bookmark, highlights and a
//! note from the card, a cross-reference read in it, and the Shift ghost
//! (46 to 52, `card`); and, last, the plain-text fallback (44, `reader`);
//! screenshots in Chi Rho and Vellum (4, most taken by the other checks).
//! Then the app started again on the same data, keeping Aa's settings (26)
//! and the chapter's lists as left (45), a second session; and, in a third
//! session on the GUI checks' test build, the updater (9 to 13, `updates`).

mod card;
mod dialogs;
mod display;
mod keys;
mod library;
mod picker;
mod reader;
mod search;
mod surfaces;
mod updates;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use thirtyfour::prelude::*;

use crate::config::Config;
use crate::native;
use crate::report::{Check, Report};
use crate::ui::{Ui, log_line};
use crate::update_server::UpdateServer;

/// How long the app's first-run search index build may take (3 to 4
/// seconds on a desktop; CI runners are slower).
const INDEX_BUILD: Duration = Duration::from_secs(60);

/// Asks the app, through the page's own IPC, whether its first-run search
/// index build has ended: `search` answers `unavailable` while it runs,
/// and after it failed ("couldn't be built"). Ready once it answers with
/// results, or says it failed (an app fault the check reports, and the
/// build no longer competes for the database). One call in flight at a
/// time; each poll reports the last answer. Read-only.
const INDEX_READY_JS: &str = "
    const s = window.__guiCheckIndex || (window.__guiCheckIndex = {});
    if (s.kind === 'text') return [true, 'ready'];
    if (s.kind === 'unavailable' && /couldn.t be built/.test(s.message || '')) return [true, 'failed: ' + s.message];
    if (!s.pending) {
        s.pending = true;
        window.__TAURI__.core.invoke('search', { query: 'beginning', translationCode: null, offset: 0 })
            .then((r) => { s.kind = r.kind; s.message = r.message || null; },
                  (e) => { s.kind = 'error'; s.message = String(e); })
            .finally(() => { s.pending = false; });
    }
    return [false, { kind: s.kind || 'asking', message: s.message || null }];
";

/// The note check 3 writes, and looks for in the exported file.
const NOTE_TEXT: &str = "gui-check smoke note: In the beginning";

/// Where the exported file goes, in the run's own folder.
const EXPORT_FILE: &str = "gui-check-notes.md";

/// The save dialog's title (`commands::export_notes`).
const SAVE_DIALOG_TITLE: &str = "Export notes";

/// Whether an element is shown: not hidden itself or inside something
/// hidden, and laid out. Prepended to the scripts below.
const VISIBLE_JS: &str = "const visible = (el) => !!el && !el.hidden && !el.closest('[hidden]') \
    && el.getClientRects().length > 0;";

/// A short description of an element, for logs and failures.
const DESCRIBE_JS: &str = "const describe = (el) => !el ? null : el.id ? '#' + el.id \
    : el.tagName.toLowerCase() + (el.className ? '.' + String(el.className).split(' ').join('.') : '') \
    + (el.textContent ? ' \"' + el.textContent.trim().slice(0, 30) + '\"' : '');";

/// Installs the `securitypolicyviolation` listener on the app's own page,
/// the moment WebDriver can first see it (the wait polls this). Not on
/// about:blank or an earlier document, whose listener would be lost.
const CSP_LISTENER_JS: &str = "
    const href = String(location.href);
    if (!/^(tauri:\\/\\/localhost|https?:\\/\\/tauri\\.localhost)/.test(href)) return [false, href];
    if (!window.__guiCheckCsp) {
        const seen = [];
        window.__guiCheckCsp = seen;
        document.addEventListener('securitypolicyviolation', (e) => seen.push(
            `${e.effectiveDirective} blocked ${e.blockedURI || '(inline)'} at ${e.sourceFile || '?'}:${e.lineNumber}`
            + (e.sample ? ` (${e.sample})` : '')));
        window.__guiCheckCspAt = document.readyState;
    }
    return [true, { href, readyState: window.__guiCheckCspAt }];
";

/// The screenshots check 4 asks for, by file name.
const SHOTS: [&str; 9] = [
    "1-reading-chi-rho.png",
    "2-export-dialog-chi-rho.png",
    "3-reading-vellum.png",
    "4-export-dialog-vellum.png",
    "5-welcome-vellum.png",
    "6-note-editor-chi-rho.png",
    "7-shortcuts-chi-rho.png",
    "8-import-preview-chi-rho.png",
    "9-translation-chi-rho.png",
];

/// The import preview's screenshot: on Windows the native open dialog is
/// only seen to appear, so no preview opens there.
const IMPORT_SHOT: &str = SHOTS[7];

/// The screenshots taken so far, and why any is missing.
#[derive(Debug, Default)]
struct Shots {
    taken: Vec<(&'static str, Result<u64, String>)>,
}

impl Shots {
    fn note(&mut self, name: &'static str, outcome: Result<u64>) {
        self.taken
            .push((name, outcome.map_err(|e| format!("{e:#}"))));
    }

    fn verdict(&self, expected: &[&'static str]) -> Result<String> {
        let mut done = Vec::new();
        let mut missing = Vec::new();
        for &name in expected {
            match self.taken.iter().find(|(n, _)| *n == name) {
                Some((_, Ok(bytes))) => done.push(format!("{name} ({} KB)", bytes / 1024)),
                Some((_, Err(why))) => missing.push(format!("{name}: {why}")),
                None => missing.push(format!("{name}: not taken (an earlier step failed)")),
            }
        }
        if missing.is_empty() {
            Ok(format!("screenshots/ {}", done.join(", ")))
        } else {
            bail!("{}", missing.join("; "))
        }
    }
}

/// Everything the checks share.
struct Run<'a> {
    config: &'a Config,
    ui: Ui,
    shots: Shots,
}

pub async fn run(config: &Config, report: &mut Report) {
    let started = Instant::now();
    log_line(
        started,
        &format!(
            "budget {}s, step timeout {}s",
            config.budget.as_secs(),
            config.step_timeout.as_secs()
        ),
    );

    // The updater checks' own server, up before anything else: the app as
    // shipped (checks 1 to 8, 14 to 24) must ignore the test settings and send it
    // nothing.
    let server = start_update_server(config).await;
    if let Err(e) = &server {
        log_line(started, &format!("no updater checks: {e:#}"));
    }

    let (looks, saved) = shipped(config, report, started).await;
    display::restart(config, report, started, looks, saved).await;

    match server {
        Ok(server) => updates::run(config, report, started, &server).await,
        Err(e) => {
            report
                .fail_the_rest(&Check::UPDATER, &format!("{e:#}"))
                .await
        }
    }
    log_line(
        started,
        &format!("done in {:.1}s", started.elapsed().as_secs_f64()),
    );
}

/// The update server and the test build's first settings (a release
/// installer, checking that server), if the run has a test build.
async fn start_update_server(config: &Config) -> Result<UpdateServer> {
    let (Some(_), Some(settings)) = (&config.updater_app, &config.updater_settings) else {
        bail!(
            "not run: no GUI-check test build of the app (set GUI_CHECK_UPDATER_APP for \
             scripts/gui-check.sh: a build with the gui-test-updater feature)"
        );
    };
    let server = UpdateServer::start().await?;
    server.write_settings(settings, "self", false).await?;
    Ok(server)
}

/// Checks 1 to 8, 14 to 25, 27 to 44 and 46 to 57, on the app as shipped;
/// what check 25 left, for check 26 (or why there's nothing), and the
/// chapter the reader is left on, for check 58.
async fn shipped(
    config: &Config,
    report: &mut Report,
    started: Instant,
) -> (std::result::Result<display::Looks, String>, Option<String>) {
    // Check 1 starts with the session; without one nothing else can run.
    let driver = match start_session(config, &config.app, started).await {
        Ok(driver) => driver,
        Err(e) => {
            log_line(started, &format!("FAILED: {e:#}"));
            report.record(Check::Driver, Err(e)).await;
            report
                .fail_the_rest(&Check::SHIPPED, "not run: no WebDriver session (check 1)")
                .await;
            return (Err("no WebDriver session (check 1)".to_owned()), None);
        }
    };
    let mut looks = Err("the page did not boot (check 1)".to_owned());
    let mut saved = None;
    let mut run = Run {
        config,
        ui: Ui::new(driver, started, config.budget, config.step_timeout),
        shots: Shots::default(),
    };

    // Before anything else, so it sees as much of the run as it can.
    let listener = run
        .ui
        .wait(
            "the app's page, to listen for CSP violations on it",
            CSP_LISTENER_JS,
        )
        .await;

    let boot = run.boot().await;
    let booted = boot.is_ok();
    report.record(Check::Driver, boot).await;
    if !booted {
        run.failure_screenshot(Check::Driver).await;
    }

    // The welcome is open over the page once boot is done: it goes first.
    if booted {
        let welcome = run.welcome().await;
        if welcome.is_err() {
            run.failure_screenshot(Check::Welcome).await;
        }
        report.record(Check::Welcome, welcome).await;
    }

    let mounted = run.leptos_mounted().await;

    if booted {
        let theme = run.expect_theme("chi-rho").await;
        let shot = match theme {
            Ok(()) => {
                run.ui
                    .screenshot(
                        "screenshot: reading view, Chi Rho",
                        &run.shot_path(SHOTS[0]),
                    )
                    .await
            }
            Err(e) => Err(e),
        };
        run.shots.note(SHOTS[0], shot);
        if let Err(e) = run.translation_screenshot().await {
            run.failure_screenshot(Check::Screenshots).await;
            if !run.shots.taken.iter().any(|(n, _)| *n == SHOTS[8]) {
                run.shots.note(SHOTS[8], Err(anyhow::anyhow!("{e:#}")));
            }
        }

        let export = run.export_notes().await;
        if export.is_err() {
            run.failure_screenshot(Check::ExportNotes).await;
        }
        report.record(Check::ExportNotes, export).await;

        let editor = run.note_editor().await;
        if editor.is_err() {
            run.failure_screenshot(Check::NoteEditor).await;
        }
        report.record(Check::NoteEditor, editor).await;

        let sheet = run.shortcuts().await;
        if sheet.is_err() {
            run.failure_screenshot(Check::Shortcuts).await;
        }
        report.record(Check::Shortcuts, sheet).await;

        let import = run.import_preview().await;
        if import.is_err() {
            run.failure_screenshot(Check::ImportPreview).await;
        }
        report.record(Check::ImportPreview, import).await;

        for (check, result) in [
            (Check::LibraryBookmarks, run.library_bookmarks().await),
            (Check::LibraryHighlights, run.library_highlights().await),
            (Check::LibraryNotes, run.library_notes().await),
            (Check::Search, run.search_word().await),
            (Check::SearchGoTo, run.search_go_to().await),
            (Check::PaletteCommand, run.palette_command().await),
            (Check::PickerPopover, run.picker_popover().await),
            (Check::PickerSidebar, run.picker_sidebar().await),
            (Check::RandomChapter, run.random_chapter().await),
            (Check::PickerKeyboard, run.picker_keyboard().await),
            (Check::SidebarPlace, run.sidebar_reading_place().await),
            (Check::TranslationSwitch, run.translation_switch().await),
            (Check::Compare, run.compare().await),
            (Check::ReadingMode, run.reading_mode().await),
            (Check::OnePanel, run.one_panel().await),
            (Check::EscapeChain, run.escape_chain().await),
            (Check::OutsideClick, run.outside_click().await),
            (Check::MenuKeyboard, run.menu_keyboard().await),
            (Check::AboutTrap, run.about_trap().await),
            (Check::GSidebar, run.g_and_sidebar().await),
            (Check::ResizePlace, run.resize_reading_place().await),
            (Check::ScrollThenResize, run.scroll_then_resize().await),
            (Check::ChapterSteps, run.chapter_steps().await),
            (Check::CompareGrid, run.compare_grid().await),
            (Check::Disclosures, run.disclosures().await),
            (Check::BackForward, run.back_forward().await),
            (Check::LongChapter, run.long_chapter().await),
            (Check::TryAgain, run.try_again().await),
            (Check::SelectVerses, run.select_verses().await),
            (Check::CopyFormats, run.copy_formats().await),
            (Check::CardBookmark, run.card_bookmark().await),
            (Check::CardHighlight, run.card_highlight().await),
            (Check::CardNote, run.card_note().await),
            (Check::XrefPreview, run.xref_preview().await),
            (Check::ShiftGhost, run.shift_ghost().await),
            (Check::KeyTurns, run.key_turns().await),
            (Check::KeySelection, run.key_selection().await),
            (Check::KeyCopy, run.key_copy().await),
            (Check::KeyHistoryNote, run.key_history_note().await),
            (
                Check::PalettePageCommands,
                run.palette_page_commands().await,
            ),
        ] {
            if result.is_err() {
                run.failure_screenshot(check).await;
            }
            report.record(check, result).await;
        }

        if let Err(e) = run.vellum_screenshots().await {
            run.failure_screenshot(Check::Screenshots).await;
            for name in &SHOTS[2..4] {
                if !run.shots.taken.iter().any(|(n, _)| n == name) {
                    run.shots.note(name, Err(anyhow::anyhow!("{e:#}")));
                }
            }
        }
        // Every Aa control, ending in the look check 26 looks for after
        // the restart.
        let controls = run.aa_controls().await;
        if controls.is_err() {
            run.failure_screenshot(Check::AaControls).await;
        }
        let (controls, left) = match controls {
            Ok((said, left)) => (Ok(said), Ok(left)),
            Err(e) => {
                let why = format!("check 25 failed: {e:#}");
                (Err(e), Err(why))
            }
        };
        looks = left;
        report.record(Check::AaControls, controls).await;
        // The chapter the reader is left on: the second session starts
        // there (check 58).
        saved = run
            .ui
            .js(
                "const h = document.querySelector('.chapter-main .chapter-heading'); return h ? h.textContent.trim() : null;",
                Vec::new(),
            )
            .await
            .ok()
            .and_then(|h| h.as_str().map(str::to_owned));
        // Last: it leaves the page showing the fallback.
        let plain = run.plain_text_fallback().await;
        if plain.is_err() {
            run.failure_screenshot(Check::PlainText).await;
        }
        report.record(Check::PlainText, plain).await;
        let expected: Vec<&'static str> = SHOTS
            .into_iter()
            .filter(|name| config.xvfb.is_some() || *name != IMPORT_SHOT)
            .chain(library::LIBRARY_SHOTS)
            .chain(search::SEARCH_SHOTS)
            // The sidebar's, only where the window can be wide enough for
            // it (not the Windows runner's screen; see `picker`).
            .chain(
                picker::PICKER_SHOTS
                    .into_iter()
                    .filter(|name| config.xvfb.is_some() || !name.starts_with("21-")),
            )
            .chain(display::DISPLAY_SHOTS)
            .chain(reader::READER_SHOTS)
            .chain(card::CARD_SHOTS)
            .chain(keys::KEYS_SHOTS)
            // The popover on a narrow window, only where the window can be
            // wide enough first (check 36).
            .chain(surfaces::SURFACES_SHOTS.into_iter().filter(|name| {
                config.xvfb.is_some() || !name.starts_with("36-picker-popover-narrow-chi-rho")
            }))
            .collect();
        report
            .record(Check::Screenshots, run.shots.verdict(&expected))
            .await;
    }

    let csp = run.csp_verdict(listener, mounted).await;
    report.record(Check::Csp, csp).await;
    report
        .fail_the_rest(&Check::SHIPPED, "not run: the page did not boot (check 1)")
        .await;

    log_line(started, "close the app (end the WebDriver session)");
    let driver = run.ui.into_driver();
    match tokio::time::timeout(Duration::from_secs(10), driver.quit()).await {
        Ok(Ok(())) => log_line(started, "session ended, app closed"),
        Ok(Err(e)) => log_line(started, &format!("ending the session failed: {e}")),
        Err(_) => log_line(started, "ending the session timed out after 10s"),
    }
    (looks, saved)
}

/// Starts `app` through tauri-driver: a WebDriver session whose browser
/// is the app.
async fn start_session(config: &Config, app: &str, started: Instant) -> Result<WebDriver> {
    log_line(
        started,
        &format!("start the app through tauri-driver ({app})"),
    );
    let mut caps = Capabilities::new();
    caps.set("tauri:options", json!({ "application": app }))?;
    let limit = config.session_timeout.min(config.budget);
    let session = WebDriver::builder(config.webdriver_url.as_str(), caps).request_timeout(limit);
    let driver = tokio::time::timeout(limit, session)
        .await
        .with_context(|| {
            format!(
                "starting the app through tauri-driver timed out after {}s",
                limit.as_secs()
            )
        })?
        .context("tauri-driver could not start a session on the app")?;
    log_line(started, "  ok: session started");
    Ok(driver)
}

impl Run<'_> {
    fn shot_path(&self, name: &str) -> PathBuf {
        self.config.out_dir.join("screenshots").join(name)
    }

    /// A screenshot of how things looked when `check` failed, if one can
    /// still be taken.
    async fn failure_screenshot(&self, check: Check) {
        let name = format!("fail-{}-{}.png", check.number(), check.name());
        if let Err(e) = self
            .ui
            .screenshot(
                &format!("failure screenshot {name}"),
                &self.shot_path(&name),
            )
            .await
        {
            self.ui.log(&format!("no failure screenshot: {e:#}"));
        }
        // Linux: the whole private display too, native dialogs and all.
        if let Some(xvfb) = &self.config.xvfb {
            let name = format!("fail-{}-{}-display.png", check.number(), check.name());
            let path = self.shot_path(&name);
            match tokio::time::timeout(Duration::from_secs(10), xvfb.screenshot_root(&path)).await {
                Ok(Ok(())) => self.ui.log(&format!("display screenshot {name}")),
                Ok(Err(e)) => self.ui.log(&format!("no display screenshot: {e:#}")),
                Err(_) => self.ui.log("no display screenshot: timed out"),
            }
        }
    }

    /// Check 1: the page booted: the chapter heading is shown, the window
    /// title carries the reference (on Linux, the native X window's too),
    /// and boot ran to the end (a fresh install's welcome is up; check 5
    /// takes it from there).
    async fn boot(&self) -> Result<String> {
        let ui = &self.ui;
        let seen = ui
            .wait(
                "the chapter heading, and the reference in the title",
                "const heading = document.querySelector('.chapter-main .chapter-heading');
                 const reference = (document.getElementById('ref-text') || {}).textContent || '';
                 const text = heading ? heading.textContent.trim() : '';
                 const ok = !!text && !!reference && reference.startsWith(text)
                     && document.title === `${reference} — Gospel Getter`;
                 return [ok, { heading: text, reference, title: document.title, href: String(location.href) }];",
            )
            .await?;
        let title = seen["title"].as_str().unwrap_or_default().to_owned();
        let heading = seen["heading"].as_str().unwrap_or_default().to_owned();

        let window = match &self.config.xvfb {
            Some(xvfb) => {
                let xvfb = xvfb.clone();
                let wanted = title.clone();
                ui.step(
                    "the native window's title says the same (on the private display)",
                    async move {
                        loop {
                            let windows = xvfb.windows_named("Gospel Getter").await?;
                            if windows.iter().any(|(_, name)| *name == wanted) {
                                return Ok(format!("X window \"{wanted}\" on {}", xvfb.display()));
                            }
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    },
                )
                .await?
            }
            None => {
                let driver_title = ui.driver().title().await?;
                if driver_title != title {
                    bail!("WebDriver's title {driver_title:?} is not the page's {title:?}");
                }
                format!(
                    "title \"{driver_title}\" (WebDriver; the native window title isn't read on Windows)"
                )
            }
        };

        // The welcome is the Rust pieces' (ui-leptos), which may load just
        // after the page: its overlay may not exist yet.
        ui.wait(
            "boot to finish (a fresh install shows the welcome)",
            &format!(
                "{VISIBLE_JS} const overlay = document.getElementById('welcome-overlay');
                 return [visible(document.getElementById('welcome-panel')),
                     {{ welcome: overlay ? (overlay.hidden ? 'hidden' : 'shown') : 'not loaded yet' }}];"
            ),
        )
        .await?;

        Ok(format!(
            "session on {}; heading \"{heading}\"; {window}; boot finished (the first-run welcome is up)",
            seen["href"].as_str().unwrap_or("?")
        ))
    }

    /// Check 2's first half: the Leptos pieces mounted their dialogs (all
    /// closed by now).
    async fn leptos_mounted(&self) -> Result<String> {
        self.ui
            .wait(
                "the Leptos pieces to mount their five dialogs",
                "const ids = ['export-notes-overlay', 'shortcuts-overlay', 'welcome-overlay', 'import-overlay', 'note-overlay'];
                 const state = Object.fromEntries(ids.map((id) => {
                     const el = document.getElementById(id);
                     return [id, el ? (el.hidden ? 'closed' : 'open') : 'missing'];
                 }));
                 return [Object.values(state).every((s) => s === 'closed'), state];",
            )
            .await?;
        Ok("the Leptos pieces mounted their five dialogs (#export-notes-overlay, #shortcuts-overlay, \
            #welcome-overlay, #import-overlay, #note-overlay; all closed)"
            .to_owned())
    }

    /// Check 2: the Leptos piece mounted and no CSP violation was reported
    /// on the page, from the listener's install to the end of the run.
    async fn csp_verdict(
        &self,
        listener: Result<Value>,
        mounted: Result<String>,
    ) -> Result<String> {
        let installed = listener.context("the CSP listener could not be installed")?;
        let mounted = mounted?;
        let seen = self
            .ui
            .step("read the CSP violations the page reported", async {
                self.ui
                    .js(
                        "return window.__guiCheckCsp ? { violations: window.__guiCheckCsp } : null;",
                        Vec::new(),
                    )
                    .await
            })
            .await?;
        if seen.is_null() {
            bail!(
                "the CSP listener is gone: the page reloaded during the run, so violations before that aren't known"
            );
        }
        let violations: Vec<String> = seen["violations"]
            .as_array()
            .map(|v| {
                v.iter()
                    .map(|x| x.as_str().unwrap_or_default().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        if !violations.is_empty() {
            bail!(
                "{} CSP violation(s): {}",
                violations.len(),
                violations.join(" | ")
            );
        }
        Ok(format!(
            "{mounted}; no CSP violations reported from the listener's install (document readyState \"{}\") \
             to the end of the run",
            installed["readyState"].as_str().unwrap_or("?")
        ))
    }

    /// Fails unless the page shows `theme` (Chi Rho is the default, so it
    /// has no data-theme attribute).
    async fn expect_theme(&self, theme: &str) -> Result<()> {
        self.ui
            .wait_with(
                &format!("the {theme} theme to be on"),
                "const want = arguments[0];
                 const select = document.getElementById('theme-select').value;
                 const attr = document.documentElement.dataset.theme || null;
                 return [select === want && attr === (want === 'chi-rho' ? null : want), { select, attr }];",
                vec![json!(theme)],
            )
            .await?;
        Ok(())
    }

    /// Opens the Library at its Notes tab, if it isn't already.
    async fn open_library_notes(&self) -> Result<()> {
        let ui = &self.ui;
        let open = ui
            .js(
                "return !document.getElementById('library-panel').hidden;",
                Vec::new(),
            )
            .await?;
        if open != Value::Bool(true) {
            ui.click("open the Library", "#library-toggle").await?;
            ui.wait(
                "the Library to show",
                &format!(
                    "{VISIBLE_JS} return [visible(document.getElementById('library-panel')), null];"
                ),
            )
            .await?;
        }
        ui.click("the Library's Notes tab", "#library-tab-notes")
            .await?;
        ui.wait(
            "Export notes… to be shown and enabled",
            &format!(
                "{VISIBLE_JS} const button = document.getElementById('export-notes-btn');
                 return [visible(button) && !button.disabled,
                     {{ shown: visible(button), disabled: button.disabled }}];"
            ),
        )
        .await?;
        Ok(())
    }

    /// Clicks Export notes… and waits for the dialog, with focus in it.
    async fn open_export_dialog(&self) -> Result<Value> {
        let ui = &self.ui;
        ui.click("click Export notes…", "#export-notes-btn").await?;
        ui.wait(
            "the Export notes dialog to show, with focus inside it",
            &format!(
                "{VISIBLE_JS} {DESCRIBE_JS}
                 const panel = document.getElementById('export-notes-panel');
                 const confirm = document.getElementById('export-notes-confirm');
                 return [visible(panel) && panel.contains(document.activeElement) && !confirm.disabled,
                     {{ title: (document.getElementById('export-notes-title') || {{}}).textContent,
                        count: (document.getElementById('export-notes-count') || {{}}).textContent,
                        focus: describe(document.activeElement) }}];"
            ),
        )
        .await
    }

    /// Escape closes the dialog and focus goes back to Export notes….
    async fn escape_export_dialog(&self) -> Result<()> {
        let ui = &self.ui;
        ui.press("press Escape in the dialog", Key::Escape).await?;
        ui.wait(
            "the dialog to close, focus back on #export-notes-btn",
            &format!(
                "{DESCRIBE_JS}
                 const overlay = document.getElementById('export-notes-overlay');
                 return [overlay.hidden && document.activeElement && document.activeElement.id === 'export-notes-btn',
                     {{ hidden: overlay.hidden, focus: describe(document.activeElement) }}];"
            ),
        )
        .await?;
        Ok(())
    }

    /// Check 3: a note added through the page, then exported through the
    /// dialog and the native save dialog.
    async fn export_notes(&mut self) -> Result<String> {
        let mut said = Vec::new();

        // A note on the first verse: select it, N, type, Save. Not until
        // the app has built its search index, which a fresh install does
        // in the background on its first start: a note saved meanwhile
        // fails ("database is locked").
        let index = {
            let ui = &self.ui;
            let index = ui
                .wait_within(
                    "the first-run search index build to end (asked through the page's IPC)",
                    INDEX_READY_JS,
                    Vec::new(),
                    Some(INDEX_BUILD),
                )
                .await?;
            let verse = ".chapter-main .verse[data-translation][data-verse=\"1\"]";
            ui.click("select verse 1", verse).await?;
            ui.wait(
                "verse 1 to be selected",
                "const v = document.querySelector('.chapter-main .verse[data-translation][data-verse=\"1\"]');
                 return [!!v && v.classList.contains('verse-selected'), null];",
            )
            .await?;
            ui.press("press N (write a note)", "n").await?;
            ui.wait(
                "the note dialog to show, focus in its text box",
                &format!(
                    "{VISIBLE_JS} {DESCRIBE_JS}
                     return [visible(document.getElementById('note-panel'))
                         && document.activeElement && document.activeElement.id === 'note-input',
                         {{ title: document.getElementById('note-title').textContent,
                            focus: describe(document.activeElement) }}];"
                ),
            )
            .await?;
            ui.type_into("type the note", "#note-input", NOTE_TEXT)
                .await?;
            ui.wait(
                "Save to be enabled",
                "return [!document.getElementById('note-save').disabled, null];",
            )
            .await?;
            ui.click("click Save", "#note-save").await?;
            ui.wait(
                "the note dialog to close",
                "return [document.getElementById('note-overlay').hidden, null];",
            )
            .await?;
            index
        };
        said.push(match index.as_str() {
            Some(failed) if failed.starts_with("failed") => format!(
                "note added (verse 1, N, Save) after the first-run search index build ended; THE APP'S \
                 INDEX BUILD FAILED ({failed}), an app fault outside this check"
            ),
            _ => "note added (verse 1, N, Save, after the first-run search index build)".to_owned(),
        });

        self.open_library_notes().await?;
        let shown = self.open_export_dialog().await?;
        said.push(format!(
            "dialog opened from Library → Notes (\"{}\", \"{}\")",
            shown["title"].as_str().unwrap_or_default(),
            shown["count"].as_str().unwrap_or_default().trim()
        ));
        let theme = self.expect_theme("chi-rho").await;
        let shot = match theme {
            Ok(()) => {
                self.ui
                    .screenshot(
                        "screenshot: Export notes dialog, Chi Rho",
                        &self.shot_path(SHOTS[1]),
                    )
                    .await
            }
            Err(e) => Err(e),
        };
        self.shots.note(SHOTS[1], shot);

        self.escape_export_dialog().await?;
        said.push("Escape closed it, focus back on #export-notes-btn".to_owned());

        self.open_export_dialog().await?;
        said.push(self.tab_cycle().await?);

        // Export…: the dialog closes and the native save dialog opens.
        let ui = &self.ui;
        ui.click("click Export…", "#export-notes-confirm").await?;
        ui.wait(
            "the Export notes dialog to close for the save dialog",
            "return [document.getElementById('export-notes-overlay').hidden, null];",
        )
        .await?;

        match &self.config.xvfb {
            Some(xvfb) => {
                let window = ui
                    .step(
                        "the GTK save dialog to appear on the private display",
                        async {
                            loop {
                                let windows = xvfb.windows_named(SAVE_DIALOG_TITLE).await?;
                                if let Some((id, _)) =
                                    windows.iter().find(|(_, name)| name == SAVE_DIALOG_TITLE)
                                {
                                    return Ok(id.clone());
                                }
                                tokio::time::sleep(Duration::from_millis(100)).await;
                            }
                        },
                    )
                    .await?;
                let path = self.config.export_dir.join(EXPORT_FILE);
                ui.step(
                    "type the file's path into the save dialog and press Enter (xdotool)",
                    async { xvfb.save_as(&window, &path).await },
                )
                .await?;
                let bytes = ui
                    .step("the exported file, with the note in it", async {
                        loop {
                            if let Ok(text) = tokio::fs::read_to_string(&path).await
                                && text.contains(NOTE_TEXT)
                            {
                                return Ok(text.len());
                            }
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    })
                    .await?;
                let toast = ui
                    .wait(
                        "the toast \"Exported 1 note\"",
                        "const toast = document.getElementById('data-status');
                         const text = toast.textContent;
                         return [text.startsWith('Exported 1 note') && !toast.classList.contains('is-hidden')
                             && !toast.classList.contains('is-error'), text];",
                    )
                    .await?;
                ui.step("the save dialog to be gone", async {
                    while !xvfb.windows_named(SAVE_DIALOG_TITLE).await?.is_empty() {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    Ok(())
                })
                .await?;
                said.push(format!(
                    "GTK save dialog on {} took the path from xdotool; {EXPORT_FILE} has the note ({bytes} bytes); \
                     toast \"{}\"",
                    xvfb.display(),
                    toast.as_str().unwrap_or_default()
                ));
            }
            None => {
                let limit = self.config.step_timeout;
                let class = ui
                    .step(
                        "the save dialog to appear (UI Automation), then close it",
                        async {
                            native::windows_find_and_close(
                                SAVE_DIALOG_TITLE,
                                limit,
                                &self.config.export_dir,
                            )
                            .await
                        },
                    )
                    .await?;
                said.push(format!(
                    "the native save dialog appeared (window class {class}) and was closed; \
                     on Windows the check stops there"
                ));
            }
        }
        Ok(said.join("; "))
    }

    /// Tab, pressed once more than the dialog has controls, keeps focus
    /// inside the dialog and comes back round.
    async fn tab_cycle(&self) -> Result<String> {
        let ui = &self.ui;
        let state = format!(
            "{DESCRIBE_JS}
             const panel = document.getElementById('export-notes-panel');
             const controls = [...panel.querySelectorAll('button, input, select, textarea, a[href], [tabindex]')]
                 .filter((el) => !el.disabled && el.tabIndex >= 0 && el.getClientRects().length > 0);
             return {{ inside: panel.contains(document.activeElement), focus: describe(document.activeElement),
                       controls: controls.length }};"
        );
        let start = ui.js(&state, Vec::new()).await?;
        let controls = start["controls"].as_u64().unwrap_or(0);
        if controls < 2 {
            bail!("the dialog has {controls} focusable control(s); expected several");
        }
        let mut focus = vec![start["focus"].clone()];
        for press in 1..=controls + 1 {
            ui.press(&format!("Tab {press} of {}", controls + 1), Key::Tab)
                .await?;
            let now = ui.js(&state, Vec::new()).await?;
            if now["inside"] != Value::Bool(true) {
                bail!(
                    "Tab {press} moved focus out of the dialog, to {}",
                    now["focus"]
                );
            }
            focus.push(now["focus"].clone());
        }
        let wrapped = focus
            .iter()
            .enumerate()
            .any(|(i, f)| focus[i + 1..].contains(f));
        if !wrapped {
            bail!(
                "Tab didn't come back round in {} presses: {focus:?}",
                controls + 1
            );
        }
        Ok(format!(
            "Tab stayed inside and cycled ({controls} controls)"
        ))
    }

    /// Check 4: the translation button's popover open over the reading
    /// view, in Chi Rho. Opened by its button, which gives focus to the
    /// chosen translation; Escape closes it, focus going back to the
    /// button.
    async fn translation_screenshot(&mut self) -> Result<()> {
        {
            let ui = &self.ui;
            ui.click("open the translations", "#translation-toggle")
                .await?;
            ui.wait(
                "the translations to show, focus on the chosen one",
                &format!(
                    "{VISIBLE_JS} const popover = document.getElementById('translation-popover');
                     const active = document.activeElement;
                     return [visible(popover) && popover.contains(active)
                             && active.getAttribute('aria-checked') === 'true',
                             {{ focus: active && active.dataset.value }}];"
                ),
            )
            .await?;
        }
        self.shot(
            "screenshot: the translation button's popover, Chi Rho",
            SHOTS[8],
        )
        .await;
        let ui = &self.ui;
        ui.press("press Escape (close the translations)", Key::Escape)
            .await?;
        ui.wait(
            "the translations to close, focus back on their button",
            "return [document.getElementById('translation-popover').hidden
                     && document.activeElement === document.getElementById('translation-toggle'), null];",
        )
        .await?;
        Ok(())
    }

    /// Check 4's second half: switch to Vellum through the Aa panel, then
    /// the reading view, the dialog and the Library's lists again.
    async fn vellum_screenshots(&mut self) -> Result<()> {
        {
            let ui = &self.ui;
            // Back to plain reading: no Library, no selection, no toast.
            let library_open = ui
                .js(
                    "return !document.getElementById('library-panel').hidden;",
                    Vec::new(),
                )
                .await?;
            if library_open == Value::Bool(true) {
                ui.click("close the Library", "#library-close").await?;
                ui.wait(
                    "the Library to close",
                    "return [document.getElementById('library-panel').hidden, null];",
                )
                .await?;
            }
            let selected = ui
                .js(
                    "return !!document.querySelector('.verse-selected');",
                    Vec::new(),
                )
                .await?;
            if selected == Value::Bool(true) {
                ui.press("press Escape (clear the verse selection)", Key::Escape)
                    .await?;
                ui.wait(
                    "the verse selection to clear",
                    "return [!document.querySelector('.verse-selected'), null];",
                )
                .await?;
            }
            ui.wait(
                "the toast to go",
                "return [document.getElementById('data-status').classList.contains('is-hidden'), null];",
            )
            .await?;

            ui.click("open Aa", "#aa-toggle").await?;
            ui.wait(
                "the Aa panel to show",
                &format!(
                    "{VISIBLE_JS} return [visible(document.getElementById('aa-popover')), null];"
                ),
            )
            .await?;
            ui.click(
                "choose the Vellum theme",
                "#theme-choices .theme-swatch[data-value=\"vellum\"]",
            )
            .await?;
        }
        self.expect_theme("vellum").await?;
        {
            let ui = &self.ui;
            ui.press("press Escape (close Aa)", Key::Escape).await?;
            ui.wait(
                "the Aa panel to close",
                "return [document.getElementById('aa-popover').hidden, null];",
            )
            .await?;
        }
        let shot = self
            .ui
            .screenshot(
                "screenshot: reading view, Vellum",
                &self.shot_path(SHOTS[2]),
            )
            .await;
        self.shots.note(SHOTS[2], shot);

        self.open_library_notes().await?;
        self.open_export_dialog().await?;
        let shot = self
            .ui
            .screenshot(
                "screenshot: Export notes dialog, Vellum",
                &self.shot_path(SHOTS[3]),
            )
            .await;
        self.shots.note(SHOTS[3], shot);
        self.escape_export_dialog().await?;
        self.library_vellum_screenshots().await?;
        self.search_vellum_screenshots().await?;
        self.picker_vellum_screenshots().await?;
        self.display_vellum_screenshots().await?;
        self.surfaces_vellum_screenshots().await?;
        self.reader_vellum_screenshots().await?;
        self.card_vellum_screenshots().await?;
        self.keys_vellum_screenshots().await
    }
}
