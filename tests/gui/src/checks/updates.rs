//! Checks 9 to 13: the updater, in a second session on the GUI checks'
//! test build of the app (feature `gui-test-updater`), against the run's
//! own update server (`update_server`), with a key made for the run:
//!
//! - 9 update-banner: the launch check (About closed) shows the banner,
//!   with its notes, Update and restart and Later; and the shipped app's
//!   session (checks 1 to 8, 14 to 16) sent the test server nothing.
//! - 10 update-about: Check now in About shows the update there, its notes
//!   as blocks of text, Update now and Close; in both looks.
//! - 11 update-download: Update now downloads it, its progress moving in
//!   steps to the end, the signature checks out with the run's key, and the
//!   test build stops there, saying so, and keeps running. It never
//!   replaces itself: the test build never installs.
//! - 12 update-link-only: a build that can't update itself (the test
//!   build's settings say so) shows Download page instead of Update now.
//!   Not clicked: it would start the system's browser.
//! - 13 update-offline: a server that never answers: Check now gives up
//!   after the check's time limit (shortened to 2 s through the test
//!   settings) with the "couldn't reach GitHub" message.

use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde_json::json;
use thirtyfour::prelude::*;

use super::{Run, Shots, VISIBLE_JS, start_session};
use crate::config::Config;
use crate::report::{Check, Report};
use crate::ui::{Ui, log_line};
use crate::update_server::{self, UpdateServer};

/// The backend's message for a check that runs out of time
/// (`updater::messages::OFFLINE`).
const OFFLINE: &str =
    "Couldn’t reach GitHub to check for updates. Check your internet connection and try again.";

/// What the test build says instead of installing
/// (`updater::test_hook::DOESNT_INSTALL`).
const DOESNT_INSTALL: &str =
    "Test build: the update was downloaded and verified, and not installed.";

/// How long the download may take (it's sent slowly on purpose).
const DOWNLOAD: Duration = Duration::from_secs(60);

/// Starts recording a status line: each text it shows, with the time since
/// now (ms), in `window.__guiCheckStatus`.
const RECORD_JS: &str = "
    const el = document.getElementById(arguments[0]);
    if (!el) return false;
    const log = [{ at: 0, text: el.textContent }];
    window.__guiCheckStatus = log;
    const t0 = performance.now();
    if (window.__guiCheckObserver) window.__guiCheckObserver.disconnect();
    window.__guiCheckObserver = new MutationObserver((records) => {
        for (const r of records) {
            const text = r.type === 'characterData' ? r.target.textContent
                : [...r.addedNodes].map((n) => n.textContent).join('');
            if (r.type === 'characterData' || r.addedNodes.length) {
                log.push({ at: Math.round(performance.now() - t0), text });
            }
        }
    });
    window.__guiCheckObserver.observe(el, { childList: true, characterData: true, subtree: true });
    return true;
";

/// The theme in use (Chi Rho has no attribute).
const THEME_JS: &str = "return document.documentElement.dataset.theme || 'chi-rho';";

pub(super) async fn run(
    config: &Config,
    report: &mut Report,
    started: Instant,
    server: &UpdateServer,
) {
    let (Some(app), Some(settings)) = (&config.updater_app, &config.updater_settings) else {
        report
            .fail_the_rest(&Check::UPDATER, "not run: no GUI-check test build")
            .await;
        return;
    };
    // Before the test build starts: the shipped app ignored the settings.
    let from_shipped = server.manifests() + server.packages();

    let driver = match start_session(config, app, started).await {
        Ok(driver) => driver,
        Err(e) => {
            report
                .fail_the_rest(
                    &Check::UPDATER,
                    &format!("not run: no WebDriver session on the test build: {e:#}"),
                )
                .await;
            return;
        }
    };
    let mut run = Run {
        config,
        ui: Ui::new(driver, started, config.budget, config.step_timeout),
        shots: Shots::default(),
    };
    let checks = Updates {
        server,
        settings,
        from_shipped,
    };

    let banner = checks.banner(&mut run).await;
    let banner_ok = banner.is_ok();
    if !banner_ok {
        run.failure_screenshot(Check::UpdateBanner).await;
    }
    report.record(Check::UpdateBanner, banner).await;

    let about = checks.about(&mut run).await;
    let about_ok = about.is_ok();
    if !about_ok {
        run.failure_screenshot(Check::UpdateAbout).await;
    }
    report.record(Check::UpdateAbout, about).await;

    if about_ok {
        let download = checks.download(&mut run).await;
        if download.is_err() {
            run.failure_screenshot(Check::UpdateDownload).await;
        }
        report.record(Check::UpdateDownload, download).await;

        let link = checks.link_only(&mut run).await;
        if link.is_err() {
            run.failure_screenshot(Check::UpdateLinkOnly).await;
        }
        report.record(Check::UpdateLinkOnly, link).await;

        let offline = checks.offline(&mut run).await;
        if offline.is_err() {
            run.failure_screenshot(Check::UpdateOffline).await;
        }
        report.record(Check::UpdateOffline, offline).await;
    }
    report
        .fail_the_rest(&Check::UPDATER, "not run: About's check failed (check 10)")
        .await;

    log_line(started, "close the test build (end the WebDriver session)");
    let driver = run.ui.into_driver();
    match tokio::time::timeout(Duration::from_secs(10), driver.quit()).await {
        Ok(Ok(())) => log_line(started, "session ended, test build closed"),
        Ok(Err(e)) => log_line(started, &format!("ending the session failed: {e}")),
        Err(_) => log_line(started, "ending the session timed out after 10s"),
    }
}

struct Updates<'a> {
    server: &'a UpdateServer,
    settings: &'a std::path::Path,
    /// Requests the server had before the test build started.
    from_shipped: u64,
}

impl Updates<'_> {
    /// Check 9.
    async fn banner(&self, run: &mut Run<'_>) -> Result<String> {
        ensure!(
            self.from_shipped == 0,
            "the shipped app sent the test update server {} requests: it must ignore the test settings",
            self.from_shipped
        );
        let ui = &run.ui;
        ui.wait_within(
            "the test build's page to boot",
            "const heading = document.querySelector('.chapter-main .chapter-heading');
             return [!!heading && !!heading.textContent.trim() && !!document.getElementById('update-banner'),
                 { heading: heading ? heading.textContent.trim() : null }];",
            Vec::new(),
            Some(run.config.session_timeout),
        )
        .await?;
        let available = format!("Gospel Getter {} is available", update_server::VERSION);
        let seen = ui
            .wait_with(
                "the launch check's banner",
                &format!(
                    "{VISIBLE_JS} const id = (i) => document.getElementById(i);
                     const title = (id('update-title') || {{}}).textContent || '';
                     return [visible(id('update-banner')) && title === arguments[0], {{
                         title, notes: visible(id('update-notes')),
                         install: visible(id('update-install')), later: visible(id('update-later')),
                         download: visible(id('update-download')),
                         notesText: (id('update-notes-text') || {{}}).textContent || '' }}];"
                ),
                vec![json!(available)],
            )
            .await?;
        ensure!(
            seen["notes"] == true && seen["install"] == true && seen["later"] == true,
            "the banner lacks its notes, Update and restart or Later: {seen}"
        );
        ensure!(
            seen["download"] == false,
            "a self-updating build shows Download page: {seen}"
        );
        ensure!(
            seen["notesText"].as_str() == Some(update_server::NOTES),
            "the banner's notes aren't the release's: {seen}"
        );
        ensure!(
            self.server.manifests() == 1,
            "expected one manifest request (the launch check), saw {}",
            self.server.manifests()
        );
        let theme = theme(ui).await?;
        let shot = format!("9-update-banner-{theme}.png");
        let bytes = ui
            .screenshot(
                &format!("screenshot: the update banner, {theme}"),
                &run.shot_path(&shot),
            )
            .await?;

        ui.click("Later", "#update-later").await?;
        ui.wait(
            "the banner to go",
            "return [document.getElementById('update-banner').hidden, null];",
        )
        .await?;
        Ok(format!(
            "the shipped app's session sent the test server nothing; the launch check found {} \
             (About closed): banner \"{available}\" with What's new, Update and restart and Later; \
             Later put it away; screenshots/{shot} ({} KB)",
            update_server::VERSION,
            bytes / 1024
        ))
    }

    /// Check 10.
    async fn about(&self, run: &mut Run<'_>) -> Result<String> {
        let first = theme(&run.ui).await?;
        open_about(&run.ui).await?;
        let before = self.server.manifests();
        run.ui.click("Check now", "#update-check-btn").await?;
        let seen = run
            .ui
            .wait_with(
                "the update in About",
                &format!(
                    "{VISIBLE_JS} const id = (i) => document.getElementById(i);
                     const notes = id('about-update-notes');
                     const title = (id('about-update-title') || {{}}).textContent || '';
                     return [visible(id('about-update')) && title === arguments[0], {{
                         title,
                         status: id('update-check-status').textContent,
                         statusHidden: id('update-check-status').classList.contains('visually-hidden'),
                         blocks: notes ? [...notes.children].map((el) => el.tagName) : [],
                         heading: notes && notes.querySelector('h5') ? notes.querySelector('h5').textContent : null,
                         strong: notes && notes.querySelector('li strong') ? notes.querySelector('li strong').textContent : null,
                         nested: notes && notes.querySelector('li ul li code') ? notes.querySelector('li ul li code').textContent : null,
                         label: notes ? notes.getAttribute('aria-label') : null,
                         install: visible(id('about-update-install')),
                         download: visible(id('about-update-download')),
                         close: visible(id('about-update-close')) }}];"
                ),
                vec![json!(format!(
                    "Gospel Getter {} is available",
                    update_server::VERSION
                ))],
            )
            .await?;
        ensure!(
            self.server.manifests() == before + 1,
            "Check now didn't ask the test server"
        );
        ensure!(
            seen["blocks"] == json!(["P", "H5", "UL"])
                && seen["heading"] == "Fixes"
                && seen["strong"] == "Search"
                && seen["nested"] == "G",
            "the notes aren't shown as their blocks: {seen}"
        );
        ensure!(
            seen["label"].as_str() == Some(&*format!("What’s new in {}", update_server::VERSION)),
            "the notes box isn't named: {seen}"
        );
        ensure!(
            seen["install"] == true && seen["close"] == true && seen["download"] == false,
            "About should offer Update now and Close: {seen}"
        );
        ensure!(
            seen["statusHidden"] == true,
            "Check now's line should be said once, then hidden: {seen}"
        );
        let shot_first = format!("10-about-update-{first}.png");
        let first_bytes = run
            .ui
            .screenshot(
                &format!("screenshot: the update in About, {first}"),
                &run.shot_path(&shot_first),
            )
            .await?;

        // The other look: About closes (Aa is behind it), the look changes,
        // and About, reopened, still offers the update.
        let other = if first == "vellum" {
            "chi-rho"
        } else {
            "vellum"
        };
        close_about(&run.ui).await?;
        choose_theme(run, other).await?;
        open_about(&run.ui).await?;
        run.ui
            .wait(
                "About still offering the update",
                &format!(
                    "{VISIBLE_JS} return [visible(document.getElementById('about-update')), null];"
                ),
            )
            .await?;
        let shot_other = format!("10-about-update-{other}.png");
        let other_bytes = run
            .ui
            .screenshot(
                &format!("screenshot: the update in About, {other}"),
                &run.shot_path(&shot_other),
            )
            .await?;
        Ok(format!(
            "Check now in About found {}: shown in About with its notes as blocks (P, H5 \"Fixes\", \
             a list with \"Search\" in bold and a nested item with `G`), named \"{}\", Update now \
             and Close; reopened after a change of look, still offered; screenshots/{shot_first} \
             ({} KB), screenshots/{shot_other} ({} KB)",
            update_server::VERSION,
            seen["label"].as_str().unwrap_or_default(),
            first_bytes / 1024,
            other_bytes / 1024
        ))
    }

    /// Check 11.
    async fn download(&self, run: &mut Run<'_>) -> Result<String> {
        let ui = &run.ui;
        let recording = ui.js(RECORD_JS, vec![json!("about-update-status")]).await?;
        ensure!(recording == true, "no #about-update-status to watch");
        let bytes_before = self.server.package_bytes();
        ui.click("Update now", "#about-update-install").await?;
        let moving = ui
            .wait(
                "the progress bar to move",
                &format!(
                    "{VISIBLE_JS} const bar = document.getElementById('about-update-progress');
                     const status = document.getElementById('about-update-status').textContent;
                     return [visible(bar) && bar.value > 0 && bar.value < bar.max,
                         {{ value: bar.value, max: bar.max, status }}];"
                ),
            )
            .await?;
        let theme = theme(ui).await?;
        let shot = format!("11-update-progress-{theme}.png");
        let shot_bytes = ui
            .screenshot(
                &format!("screenshot: the download's progress, {theme}"),
                &run.shot_path(&shot),
            )
            .await?;
        let end = ui
            .wait_within(
                "the download to end (the test build doesn't install)",
                "const status = document.getElementById('about-update-status');
                 return [status.classList.contains('is-error'),
                     { status: status.textContent, log: window.__guiCheckStatus }];",
                Vec::new(),
                Some(DOWNLOAD),
            )
            .await?;
        ensure!(
            end["status"] == DOESNT_INSTALL,
            "the download ended with {:?}, not the test build's stop (a failed signature check \
             or download says why here)",
            end["status"]
        );
        let texts: Vec<String> = end["log"]
            .as_array()
            .context("no status log")?
            .iter()
            .filter_map(|entry| entry["text"].as_str().map(str::to_owned))
            .collect();
        let percents: Vec<u32> = texts
            .iter()
            .filter_map(|t| t.strip_prefix("Downloading the update… "))
            .filter_map(|t| t.strip_suffix('%'))
            .filter_map(|n| n.parse().ok())
            .collect();
        ensure!(
            percents.len() >= 3 && percents.windows(2).all(|w| w[0] <= w[1]),
            "the progress didn't move in steps: {texts:?}"
        );
        ensure!(
            percents.iter().any(|&p| p > 0 && p < 100),
            "no step between 0% and 100%: {percents:?}"
        );
        ensure!(
            texts
                .iter()
                .any(|t| t == "Installing the update. Gospel Getter will restart when it’s done."),
            "the download never said it finished (verified): {texts:?}"
        );
        let after = ui
            .wait(
                "Update now back, the app running",
                &format!(
                    "{VISIBLE_JS} const id = (i) => document.getElementById(i);
                     const heading = document.querySelector('.chapter-main .chapter-heading');
                     return [visible(id('about-update-install')) && !visible(id('about-update-progress'))
                         && !!heading, {{ heading: heading ? heading.textContent.trim() : null }}];"
                ),
            )
            .await?;
        let sent = self.server.package_bytes() - bytes_before;
        ensure!(
            sent >= self.server.package_size,
            "the server sent {sent} of {} bytes",
            self.server.package_size
        );
        let distinct = {
            let mut p = percents.clone();
            p.dedup();
            p.len()
        };
        Ok(format!(
            "Update now downloaded {} KB from the test server, the bar moving ({} at the first \
             look, {distinct} steps from {}% to {}%), \"Installing the update…\" once verified \
             with the run's key, then the test build's stop: \"{DOESNT_INSTALL}\"; Update now \
             back, the app still running ({}); screenshots/{shot} ({} KB)",
            sent / 1024,
            moving["status"].as_str().unwrap_or("?"),
            percents.first().copied().unwrap_or_default(),
            percents.last().copied().unwrap_or_default(),
            after["heading"].as_str().unwrap_or("?"),
            shot_bytes / 1024
        ))
    }

    /// Check 12.
    async fn link_only(&self, run: &mut Run<'_>) -> Result<String> {
        self.server
            .write_settings(self.settings, "link", false)
            .await?;
        let ui = &run.ui;
        let before = self.server.manifests();
        ui.click(
            "Check now (a build that can't update itself)",
            "#update-check-btn",
        )
        .await?;
        let seen = ui
            .wait(
                "Download page in place of Update now",
                &format!(
                    "{VISIBLE_JS} const id = (i) => document.getElementById(i);
                     const download = id('about-update-download');
                     return [visible(download) && !visible(id('about-update-install')),
                         {{ download: download ? download.textContent : null,
                            title: id('about-update-title').textContent }}];"
                ),
            )
            .await?;
        ensure!(
            seen["download"] == "Download page",
            "the button says {:?}",
            seen["download"]
        );
        ensure!(
            self.server.manifests() == before + 1,
            "Check now didn't ask the test server"
        );
        let theme = theme(ui).await?;
        let shot = format!("12-update-link-only-{theme}.png");
        let bytes = ui
            .screenshot(
                &format!("screenshot: a build that links to the download page, {theme}"),
                &run.shot_path(&shot),
            )
            .await?;
        Ok(format!(
            "with the settings of a build that can't update itself, Check now shows \"{}\" with \
             Download page and no Update now (not clicked: it starts the system's browser); \
             screenshots/{shot} ({} KB)",
            seen["title"].as_str().unwrap_or("?"),
            bytes / 1024
        ))
    }

    /// Check 13.
    async fn offline(&self, run: &mut Run<'_>) -> Result<String> {
        self.server
            .write_settings(self.settings, "self", true)
            .await?;
        let ui = &run.ui;
        let recording = ui.js(RECORD_JS, vec![json!("update-check-status")]).await?;
        ensure!(recording == true, "no #update-check-status to watch");
        let before = self.server.manifests();
        ui.click(
            "Check now (a server that never answers)",
            "#update-check-btn",
        )
        .await?;
        let end = ui
            .wait_within(
                "Check now to give up",
                "const status = document.getElementById('update-check-status');
                 const button = document.getElementById('update-check-btn');
                 return [status.classList.contains('is-error') && !button.disabled,
                     { status: status.textContent, log: window.__guiCheckStatus }];",
                Vec::new(),
                Some(Duration::from_secs(30)),
            )
            .await?;
        ensure!(
            end["status"] == OFFLINE,
            "Check now said {:?}",
            end["status"]
        );
        let log = end["log"].as_array().context("no status log")?;
        let at = |text: &str| {
            log.iter()
                .find(|e| e["text"] == text)
                .and_then(|e| e["at"].as_u64())
        };
        let checking = at("Checking for updates…").context("never said it was checking")?;
        let gave_up = at(OFFLINE).context("the message wasn't seen arriving")?;
        let waited = Duration::from_millis(gave_up.saturating_sub(checking));
        let limit = update_server::OFFLINE_LIMIT;
        if waited + Duration::from_millis(200) < limit || waited > limit * 5 {
            bail!(
                "it gave up after {:.1}s, not after the {}s limit",
                waited.as_secs_f64(),
                limit.as_secs()
            );
        }
        ensure!(
            self.server.manifests() == before,
            "the check reached the test server, not the black hole"
        );
        let theme = theme(ui).await?;
        let shot = format!("13-update-offline-{theme}.png");
        let bytes = ui
            .screenshot(
                &format!("screenshot: a check that ran out of time, {theme}"),
                &run.shot_path(&shot),
            )
            .await?;
        close_about(ui).await?;
        Ok(format!(
            "against a server that takes the connection and never answers, with the limit set to \
             {}s through the test settings, Check now said \"Checking for updates…\", gave up \
             {:.1}s later with \"{OFFLINE}\" and could be pressed again; screenshots/{shot} ({} KB)",
            limit.as_secs(),
            waited.as_secs_f64(),
            bytes / 1024
        ))
    }
}

async fn theme(ui: &Ui) -> Result<String> {
    Ok(ui
        .js(THEME_JS, Vec::new())
        .await?
        .as_str()
        .unwrap_or("chi-rho")
        .to_owned())
}

/// ☰ → About Gospel Getter, and focus on its close button.
async fn open_about(ui: &Ui) -> Result<()> {
    ui.click("open ☰", "#menu-toggle").await?;
    ui.wait(
        "the menu to open",
        &format!("{VISIBLE_JS} return [visible(document.getElementById('about-btn')), null];"),
    )
    .await?;
    ui.click("About Gospel Getter", "#about-btn").await?;
    ui.wait(
        "About to open, focus on its close button",
        &format!(
            "{VISIBLE_JS} const focus = document.activeElement;
             return [visible(document.getElementById('about-panel')) && !!focus && focus.id === 'about-close',
                 {{ focus: focus ? focus.id : null }}];"
        ),
    )
    .await?;
    Ok(())
}

/// Escape closes About, and focus goes back to ☰.
async fn close_about(ui: &Ui) -> Result<()> {
    ui.press("press Escape (close About)", Key::Escape).await?;
    ui.wait(
        "About to close, focus back on ☰",
        "const focus = document.activeElement;
         return [document.getElementById('about-overlay').hidden && !!focus && focus.id === 'menu-toggle',
             { focus: focus ? focus.id : null }];",
    )
    .await?;
    Ok(())
}

/// Picks `theme` in the Aa panel, as a reader would.
async fn choose_theme(run: &Run<'_>, theme: &str) -> Result<()> {
    let ui = &run.ui;
    ui.click("open Aa", "#aa-toggle").await?;
    ui.wait(
        "the Aa panel to show",
        &format!("{VISIBLE_JS} return [visible(document.getElementById('aa-popover')), null];"),
    )
    .await?;
    ui.click(
        &format!("choose the {theme} theme"),
        &format!("#theme-choices .theme-swatch[data-value=\"{theme}\"]"),
    )
    .await?;
    run.expect_theme(theme).await?;
    ui.press("press Escape (close Aa)", Key::Escape).await?;
    ui.wait(
        "the Aa panel to close",
        "return [document.getElementById('aa-popover').hidden, null];",
    )
    .await?;
    Ok(())
}
