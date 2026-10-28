//! Checks 25 to 29: Aa, the translations and the saved settings, which the
//! Rust pieces (ui-leptos) took over from the head script, each through the
//! real UI. Every Aa control visibly changes the page: the text size by
//! its stepper and by Ctrl+=, Ctrl+- and Ctrl+0 (with its toast), line
//! spacing, width, layout and theme, each read back from the page's
//! computed style (25). All of them survive a restart: the app is closed
//! and started again on the same data, and the page and Aa show what was
//! chosen (26, in a session of its own; the settings are then put back as
//! they were, for the updater's checks; not run on Windows, where Edge
//! WebDriver gives each session a fresh WebView2 data folder). That session
//! ends with a note being typed, which the app started once more offers
//! back (65, `notes`, in a session of its own). A
//! translation chosen from the translation button is read (27); Compare
//! goes on and off (28); reading mode comes and goes, by its exit button
//! and by Escape (29). Screenshots in Chi Rho and Vellum.

use anyhow::{Result, bail, ensure};
use serde_json::{Value, json};
use thirtyfour::prelude::*;

use super::{DESCRIBE_JS, Run, Shots, VISIBLE_JS, start_session};
use crate::config::Config;
use crate::report::{Check, Report};
use crate::ui::{Ui, log_line};

/// The screenshots checks 25 and 27 to 29 take (check 4 lists them): Aa
/// in Vellum and Chi Rho, the WEB chosen, and Compare and reading mode in
/// each.
pub(super) const DISPLAY_SHOTS: [&str; 7] = [
    "25-aa-vellum.png",
    "25-aa-chi-rho.png",
    "28-compare-chi-rho.png",
    "29-reading-mode-chi-rho.png",
    "28-compare-vellum.png",
    "29-reading-mode-vellum.png",
    "27-translation-web-chi-rho.png",
];

/// Check 26's screenshot, in its own session.
const RESTORED_SHOT: &str = "26-restored-vellum.png";

/// The look as the page shows it, none of it depending on the window's
/// size: Aa's settings as `<html>` has them, the first verse's computed
/// size, line height and display (inline when flowing), the column's
/// largest width (Aa's Width), and the page's background (the theme).
const LOOK_JS: &str = "const data = document.documentElement.dataset;
    const verse = document.querySelector('.chapter-main .chapter-text > .verse[data-verse]');
    const style = verse ? getComputedStyle(verse) : null;
    const main = document.querySelector('.chapter-main');
    return { settings: { theme: data.theme || 'chi-rho', textSize: data.textSize || 'medium',
                         lineSpacing: data.lineSpacing || 'normal', width: data.width || 'normal',
                         layout: data.layout || 'verses' },
             fontSize: style ? parseFloat(style.fontSize) : null,
             lineHeight: style ? parseFloat(style.lineHeight) : null,
             display: style ? style.display : null,
             column: main ? parseFloat(getComputedStyle(main).maxWidth) : null,
             columnNow: main ? Math.round(main.getBoundingClientRect().width) : null,
             background: getComputedStyle(document.body).backgroundColor };";

/// What Aa shows: its choices marked, the size's name, the stepper's ends.
const AA_JS: &str =
    "const pressed = (id) => [...document.querySelectorAll(`#${id} [aria-pressed=\"true\"]`)]
        .map((b) => b.dataset.value);
    return { size: document.getElementById('text-size-value').textContent,
             lineSpacing: pressed('line-spacing-choices'), width: pressed('width-choices'),
             layout: pressed('layout-choices'), theme: pressed('theme-choices'),
             themeName: document.getElementById('theme-name').textContent,
             down: document.getElementById('text-size-down').disabled,
             up: document.getElementById('text-size-up').disabled };";

/// The settings check 25 leaves, for check 26 to find after the restart.
const SAVED: [(&str, &str); 5] = [
    ("theme", "vellum"),
    ("textSize", "large"),
    ("lineSpacing", "relaxed"),
    ("width", "wide"),
    ("layout", "flowing"),
];

/// The look before check 25 (to put back after check 26) and after it
/// (what check 26 expects to find).
pub(super) struct Looks {
    before: Value,
    after: Value,
}

/// Aa's choice of `value` in its group `group`.
fn choice(group: &str, value: &str) -> String {
    format!("#{group} [data-value=\"{value}\"]")
}

fn number(look: &Value, field: &str) -> f64 {
    look[field].as_f64().unwrap_or(f64::NAN)
}

impl Run<'_> {
    /// The look as [`LOOK_JS`] reads it.
    pub(super) async fn look(&self) -> Result<Value> {
        self.ui.js(LOOK_JS, Vec::new()).await
    }

    /// Waits until the setting `name` (as `<html>` has it, its default when
    /// it has none) is `value`, and returns the look then.
    async fn setting_is(&self, name: &str, value: &str) -> Result<Value> {
        self.ui
            .wait_with(
                &format!("{name} to be {value}"),
                &format!(
                    "const seen = (() => {{ {LOOK_JS} }})();
                     return [seen.settings[arguments[0]] === arguments[1], seen];"
                ),
                vec![json!(name), json!(value)],
            )
            .await
    }

    pub(super) async fn open_aa(&self) -> Result<()> {
        let ui = &self.ui;
        ui.click("open Aa", "#aa-toggle").await?;
        ui.wait(
            "the Aa panel to show, focus in it",
            &format!(
                "{VISIBLE_JS} const aa = document.getElementById('aa-popover');
                 return [visible(aa) && aa.contains(document.activeElement), null];"
            ),
        )
        .await?;
        Ok(())
    }

    pub(super) async fn close_with_escape(&self, what: &str, id: &str) -> Result<()> {
        let ui = &self.ui;
        ui.press(&format!("press Escape (close {what})"), Key::Escape)
            .await?;
        ui.wait_with(
            &format!("{what} to close"),
            "return [document.getElementById(arguments[0]).hidden, null];",
            vec![json!(id)],
        )
        .await?;
        Ok(())
    }

    /// Chooses `value` in Aa's group `group` (Aa open), and waits for the
    /// setting `name` to follow.
    pub(super) async fn aa_choose(&self, group: &str, name: &str, value: &str) -> Result<Value> {
        self.ui
            .click(&format!("Aa: choose {value}"), &choice(group, value))
            .await?;
        self.setting_is(name, value).await
    }

    /// Presses Ctrl and `key` together.
    async fn ctrl(&self, key: char) -> Result<()> {
        self.ui
            .step(&format!("press Ctrl+{key}"), async {
                self.ui
                    .driver()
                    .action_chain()
                    .key_down(Key::Control)
                    .key_down(key)
                    .key_up(key)
                    .key_up(Key::Control)
                    .perform()
                    .await?;
                Ok(())
            })
            .await
    }

    /// Focus on the page, off every control, as a reader's is before a key.
    async fn focus_on_page(&self) -> Result<()> {
        self.ui
            .js(
                "if (document.activeElement) document.activeElement.blur(); return null;",
                Vec::new(),
            )
            .await?;
        Ok(())
    }

    /// Check 25.
    pub(super) async fn aa_controls(&mut self) -> Result<(String, Looks)> {
        let mut said = Vec::new();
        let before = self.look().await?;
        self.ui.log(&format!("  look before: {before}"));
        self.open_aa().await?;
        let vellum = self.aa_choose("theme-choices", "theme", "vellum").await?;
        self.shot("screenshot: Aa, Vellum", DISPLAY_SHOTS[0]).await;

        // Theme: the page's own colors.
        let matrix = self.aa_choose("theme-choices", "theme", "matrix").await?;
        let chi_rho = self.aa_choose("theme-choices", "theme", "chi-rho").await?;
        ensure!(
            matrix["background"] != vellum["background"]
                && chi_rho["background"] != matrix["background"],
            "the theme didn't change the page's background: {} → {} → {}",
            vellum["background"],
            matrix["background"],
            chi_rho["background"]
        );
        said.push(format!(
            "theme: Vellum ({}), Matrix ({}), Chi Rho ({}), the page's background each time",
            vellum["background"], matrix["background"], chi_rho["background"]
        ));
        self.shot("screenshot: Aa, Chi Rho", DISPLAY_SHOTS[1]).await;

        // Text size by the stepper.
        let start = self.setting_is("textSize", "medium").await;
        let start = match start {
            Ok(look) => look,
            Err(_) => {
                bail!(
                    "the text size isn't Medium to start with: {}",
                    self.look().await?
                )
            }
        };
        self.ui.click("Aa: A+ (larger)", "#text-size-up").await?;
        let larger = self.setting_is("textSize", "large").await?;
        self.ui.click("Aa: A− (smaller)", "#text-size-down").await?;
        self.ui.click("Aa: A− (smaller)", "#text-size-down").await?;
        let smaller = self.setting_is("textSize", "small").await?;
        let (medium, large, small) = (
            number(&start, "fontSize"),
            number(&larger, "fontSize"),
            number(&smaller, "fontSize"),
        );
        ensure!(
            large > medium && small < medium,
            "the stepper didn't change the verses' size: Medium {medium}px, Large {large}px, Small {small}px"
        );
        let aa = self.ui.js(AA_JS, Vec::new()).await?;
        ensure!(
            aa["size"] == "Small" && aa["down"] == true && aa["up"] == false,
            "Aa's stepper doesn't show Small at its end: {aa}"
        );
        said.push(format!(
            "A+ and A−: the verses {medium}px → {large}px (Large) → {small}px (Small, A− disabled)"
        ));

        // Line spacing.
        let compact = self
            .aa_choose("line-spacing-choices", "lineSpacing", "compact")
            .await?;
        let relaxed = self
            .aa_choose("line-spacing-choices", "lineSpacing", "relaxed")
            .await?;
        ensure!(
            number(&relaxed, "lineHeight") > number(&compact, "lineHeight"),
            "line spacing didn't change the lines: Compact {}, Relaxed {}",
            compact["lineHeight"],
            relaxed["lineHeight"]
        );
        said.push(format!(
            "line spacing: Compact {}px, Relaxed {}px a line",
            compact["lineHeight"], relaxed["lineHeight"]
        ));

        // Width: the column's measure.
        let narrow = self.aa_choose("width-choices", "width", "narrow").await?;
        let wide = self.aa_choose("width-choices", "width", "wide").await?;
        ensure!(
            number(&wide, "column") > number(&narrow, "column"),
            "width didn't change the column: Narrow {}, Wide {}",
            narrow["column"],
            wide["column"]
        );
        said.push(format!(
            "width: the column up to {}px (Narrow, {} px now) then {}px (Wide, {} px now)",
            narrow["column"], narrow["columnNow"], wide["column"], wide["columnNow"]
        ));

        // Layout: a verse to a line, or flowing on.
        let flowing = self
            .aa_choose("layout-choices", "layout", "flowing")
            .await?;
        let verses = self.aa_choose("layout-choices", "layout", "verses").await?;
        ensure!(
            flowing["display"] == "inline" && verses["display"] == "block",
            "layout didn't change how verses are set: Flowing {}, Verses {}",
            flowing["display"],
            verses["display"]
        );
        said.push("layout: Flowing sets the verses inline, Verses a verse to a line".to_owned());
        self.close_with_escape("Aa", "aa-popover").await?;

        // The size keys, on the page: the size and its toast.
        self.focus_on_page().await?;
        self.ctrl('0').await?;
        let reset = self.setting_is("textSize", "medium").await?;
        self.ctrl('=').await?;
        let up = self.setting_is("textSize", "large").await?;
        let toast = self
            .ui
            .wait(
                "the toast \"Text size: Large\"",
                "const toast = document.getElementById('size-status');
                 return [toast.textContent === 'Text size: Large' && !toast.classList.contains('is-hidden'),
                         toast.textContent];",
            )
            .await?;
        self.ctrl('-').await?;
        let down = self.setting_is("textSize", "medium").await?;
        ensure!(
            number(&up, "fontSize") > number(&reset, "fontSize")
                && number(&down, "fontSize") == number(&reset, "fontSize"),
            "the size keys didn't change the verses' size: Ctrl+0 {}, Ctrl+= {}, Ctrl+- {}",
            reset["fontSize"],
            up["fontSize"],
            down["fontSize"]
        );
        said.push(format!(
            "Ctrl+0 Medium ({}px), Ctrl+= Large ({}px, toast \"{}\"), Ctrl+- Medium",
            reset["fontSize"],
            up["fontSize"],
            toast.as_str().unwrap_or_default()
        ));

        // What check 26 will find after the restart.
        self.ctrl('=').await?;
        self.setting_is("textSize", "large").await?;
        self.open_aa().await?;
        self.aa_choose("layout-choices", "layout", "flowing")
            .await?;
        self.aa_choose("theme-choices", "theme", "vellum").await?;
        self.close_with_escape("Aa", "aa-popover").await?;
        let after = self.look().await?;
        for (name, value) in SAVED {
            ensure!(
                after["settings"][name] == value,
                "{name} isn't {value} to restart with: {after}"
            );
        }
        let saved: Vec<String> = SAVED.iter().map(|(n, v)| format!("{n} {v}")).collect();
        said.push(format!("left at {} for the restart", saved.join(", ")));
        Ok((said.join("; "), Looks { before, after }))
    }

    /// Check 27: a translation chosen from the translation button is read,
    /// and the button says so; then back.
    pub(super) async fn translation_switch(&mut self) -> Result<String> {
        const VERSE_JS: &str = "const verse = document.querySelector('.chapter-main .verse[data-translation][data-verse=\"1\"]');
            return { code: verse ? verse.dataset.translation : null,
                     text: verse ? verse.querySelector('.verse-text').textContent.trim().slice(0, 60) : null,
                     button: document.getElementById('translation-toggle').textContent,
                     heading: (document.querySelector('.chapter-main .chapter-heading') || {}).textContent };";
        let ui = &self.ui;
        let kjv = ui.js(VERSE_JS, Vec::new()).await?;
        ensure!(
            kjv["code"] == "kjv",
            "not reading the KJV to start with: {kjv}"
        );
        ui.click("open the translations", "#translation-toggle")
            .await?;
        ui.click(
            "choose the WEB",
            "#translation-choices .translation-choice[data-value=\"web\"]",
        )
        .await?;
        let web = ui
            .wait(
                "the chapter in the WEB, the button saying WEB",
                &format!(
                    "const seen = (() => {{ {VERSE_JS} }})();
                     const radio = document.querySelector('#translation-choices [aria-checked=\"true\"]');
                     return [seen.code === 'web' && seen.button === 'WEB' && !!radio && radio.dataset.value === 'web', seen];"
                ),
            )
            .await?;
        ensure!(
            web["text"] != kjv["text"] && web["heading"] == kjv["heading"],
            "the WEB's verse 1 should differ from the KJV's, in the same chapter: {kjv} → {web}"
        );
        self.shot("screenshot: the WEB chosen, Chi Rho", DISPLAY_SHOTS[6])
            .await;
        let ui = &self.ui;
        ui.click(
            "choose the KJV again",
            "#translation-choices .translation-choice[data-value=\"kjv\"]",
        )
        .await?;
        ui.wait(
            "the chapter in the KJV again",
            &format!(
                "const seen = (() => {{ {VERSE_JS} }})(); return [seen.code === 'kjv' && seen.button === 'KJV', seen];"
            ),
        )
        .await?;
        self.close_with_escape("the translations", "translation-popover")
            .await?;
        Ok(format!(
            "from the translation button: {} read in the WEB (\"{}\"), the button WEB; back to the KJV (\"{}\")",
            web["heading"].as_str().unwrap_or("?"),
            web["text"].as_str().unwrap_or("?"),
            kjv["text"].as_str().unwrap_or("?")
        ))
    }

    /// Compare on (or off) from the translation button's popover, which
    /// stays open; then closed.
    async fn set_compare(&self, on: bool) -> Result<Value> {
        let ui = &self.ui;
        ui.click("open the translations", "#translation-toggle")
            .await?;
        ui.click(
            if on { "Compare on" } else { "Compare off" },
            "#compare-btn",
        )
        .await?;
        let seen = ui
            .wait_with(
                if on {
                    "the chapter side by side"
                } else {
                    "the chapter on its own"
                },
                "const heads = [...document.querySelectorAll('.compare-head')].map((h) => h.textContent.trim());
                 const pressed = document.getElementById('compare-btn').getAttribute('aria-pressed');
                 const grid = !!document.querySelector('.compare-grid');
                 return [grid === arguments[0] && pressed === String(arguments[0])
                         && (!arguments[0] || heads.length === 2),
                         { heads, pressed, compare: document.documentElement.dataset.compare || 'off' }];",
                vec![json!(on)],
            )
            .await?;
        self.close_with_escape("the translations", "translation-popover")
            .await?;
        Ok(seen)
    }

    /// Check 28.
    pub(super) async fn compare(&mut self) -> Result<String> {
        let on = self.set_compare(true).await?;
        self.shot("screenshot: Compare, Chi Rho", DISPLAY_SHOTS[2])
            .await;
        let off = self.set_compare(false).await?;
        Ok(format!(
            "Compare from the translations: on, side by side {} (aria-pressed {}); off, the chapter on its own \
             (aria-pressed {})",
            on["heads"], on["pressed"], off["pressed"]
        ))
    }

    /// Reading mode on from Aa: the bar's other buttons go, the exit shows,
    /// with focus.
    pub(super) async fn reading_mode_on(&self) -> Result<Value> {
        self.open_aa().await?;
        let ui = &self.ui;
        ui.click("Aa: Reading mode", "#reader-mode-btn").await?;
        ui.wait(
            "reading mode: Aa closed, the exit shown with focus, the Library and ☰ gone",
            &format!(
                "{DESCRIBE_JS} const shown = (id) => getComputedStyle(document.getElementById(id)).display !== 'none';
                 const seen = {{ mode: document.documentElement.dataset.readerMode || 'off',
                     aa: !document.getElementById('aa-popover').hidden,
                     exit: shown('reader-exit'), library: shown('library-toggle'), menu: shown('menu-toggle'),
                     focus: describe(document.activeElement) }};
                 return [seen.mode === 'on' && !seen.aa && seen.exit && !seen.library && !seen.menu
                         && seen.focus === '#reader-exit', seen];"
            ),
        )
        .await
    }

    /// Waits for reading mode to be off: the exit gone, focus on Aa.
    pub(super) async fn reading_mode_off(&self, what: &str) -> Result<Value> {
        self.ui
            .wait(
                what,
                &format!(
                    "{DESCRIBE_JS} const shown = (id) => getComputedStyle(document.getElementById(id)).display !== 'none';
                     const seen = {{ mode: document.documentElement.dataset.readerMode || 'off',
                         exit: shown('reader-exit'), menu: shown('menu-toggle'),
                         focus: describe(document.activeElement) }};
                     return [seen.mode === 'off' && !seen.exit && seen.menu, seen];"
                ),
            )
            .await
    }

    /// Check 29.
    pub(super) async fn reading_mode(&mut self) -> Result<String> {
        let on = self.reading_mode_on().await?;
        self.shot("screenshot: reading mode, Chi Rho", DISPLAY_SHOTS[3])
            .await;
        self.ui.click("the exit button", "#reader-exit").await?;
        let off = self
            .reading_mode_off("reading mode to end, focus back on Aa")
            .await?;
        ensure!(
            off["focus"] == "#aa-toggle",
            "focus didn't go back to Aa: {off}"
        );
        self.reading_mode_on().await?;
        self.ui
            .press("press Escape (leave reading mode)", Key::Escape)
            .await?;
        let escaped = self.reading_mode_off("Escape to end reading mode").await?;
        Ok(format!(
            "Aa → Reading mode: Aa closed, the Library and ☰ gone, focus on {}; the exit button: off, focus on {}; \
             on again, Escape: off ({})",
            on["focus"].as_str().unwrap_or("?"),
            off["focus"].as_str().unwrap_or("?"),
            escaped["mode"].as_str().unwrap_or("?")
        ))
    }

    /// Check 4's Compare and reading mode in Vellum.
    pub(super) async fn display_vellum_screenshots(&mut self) -> Result<()> {
        self.set_compare(true).await?;
        self.shot("screenshot: Compare, Vellum", DISPLAY_SHOTS[4])
            .await;
        self.set_compare(false).await?;
        self.reading_mode_on().await?;
        self.shot("screenshot: reading mode, Vellum", DISPLAY_SHOTS[5])
            .await;
        self.ui.click("the exit button", "#reader-exit").await?;
        self.reading_mode_off("reading mode to end").await?;
        Ok(())
    }
}

/// The second session: the app closed and started again, on the same data.
/// Check 58: a cold start goes straight to the chapter the first session
/// left (`saved`). Check 59: at the verse it left at the top (`left_at`,
/// see `place`). Check 26 (check 25 left its settings): the page and Aa
/// show every setting as it was left; then they're put back as they were
/// before check 25. Check 45: the lists check 40 left open are open.
/// Why checks 26, 45 and 65 aren't run on Windows.
const NOT_ON_WINDOWS: &str = "not run on Windows: Edge WebDriver gives each session's WebView2 a fresh \
    user data folder (WEBVIEW2_USER_DATA_FOLDER, a new scoped_dir per session), so the app started again \
    in a new session can't see the first session's local storage (the app as shipped keeps one folder)";

/// Ends `run`'s WebDriver session, which closes the app.
async fn end_session(run: Run<'_>, started: std::time::Instant, which: &str) {
    log_line(
        started,
        &format!("close the app (end the {which} WebDriver session)"),
    );
    let driver = run.ui.into_driver();
    match tokio::time::timeout(std::time::Duration::from_secs(10), driver.quit()).await {
        Ok(Ok(())) => log_line(started, "session ended, app closed"),
        Ok(Err(e)) => log_line(started, &format!("ending the session failed: {e}")),
        Err(_) => log_line(started, "ending the session timed out after 10s"),
    }
}

pub(super) async fn restart(
    config: &Config,
    report: &mut Report,
    started: std::time::Instant,
    looks: Result<Looks, String>,
    saved: Option<String>,
    left_at: Result<Value, String>,
) {
    let kept_here = [Check::AaRestart, Check::DisclosuresKept];
    // Windows: Edge WebDriver starts each session's WebView2 with a user
    // data folder of its own (WEBVIEW2_USER_DATA_FOLDER, a new
    // `scoped_dir…` each time), so the app started again in a new session
    // can't see what the first one saved in local storage; the app itself
    // always uses the one folder. Checks 26 and 45 aren't run there. The
    // reading position is the backend's (its database, in the account's
    // own folder), so check 58 is.
    let windows = config.xvfb.is_none();
    if windows {
        for check in kept_here.into_iter().chain([Check::NoteDraft]) {
            report.record(check, Ok(NOT_ON_WINDOWS.to_owned())).await;
        }
    }
    let looks = match looks {
        Ok(looks) => Some(looks),
        Err(why) => {
            report
                .fail_the_rest(&kept_here, &format!("not run: {why}"))
                .await;
            None
        }
    };
    log_line(
        started,
        "start the app again, on the same data (checks 58, 59, 26 and 45)",
    );
    let driver = match start_session(config, &config.app, started).await {
        Ok(driver) => driver,
        Err(e) => {
            report
                .record(
                    Check::ColdStart,
                    Err(e.context("no WebDriver session on the app started again")),
                )
                .await;
            report
                .fail_the_rest(
                    &Check::RELAUNCH,
                    "not run: no WebDriver session on the app started again",
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
    let cold = run.cold_start(saved.as_deref()).await;
    if cold.is_err() {
        run.failure_screenshot(Check::ColdStart).await;
    }
    report.record(Check::ColdStart, cold).await;
    let reopened = match &left_at {
        Ok(left) => run.reopened_at_verse(left).await,
        Err(why) => Err(anyhow::anyhow!(
            "not run: the first session didn't leave the reader at a verse: {why}"
        )),
    };
    if reopened.is_err() {
        run.failure_screenshot(Check::ReopenAtVerse).await;
    }
    report.record(Check::ReopenAtVerse, reopened).await;
    if let (false, Some(looks)) = (windows, looks) {
        let result = run.restored(&looks).await;
        if result.is_err() {
            run.failure_screenshot(Check::AaRestart).await;
        }
        report.record(Check::AaRestart, result).await;
        let kept = run.disclosures_kept().await;
        if kept.is_err() {
            run.failure_screenshot(Check::DisclosuresKept).await;
        }
        report.record(Check::DisclosuresKept, kept).await;
    }
    // Check 65: a new note typed and left open as the app quits.
    let draft = if windows {
        None
    } else {
        let typed = run.draft_typed().await;
        if typed.is_err() {
            run.failure_screenshot(Check::NoteDraft).await;
        }
        Some(typed)
    };

    end_session(run, started, "second").await;

    match draft {
        None => {}
        Some(Err(e)) => {
            report
                .record(Check::NoteDraft, Err(e.context("typing the draft")))
                .await;
        }
        Some(Ok(left)) => draft_after_restart(config, report, started, &left).await,
    }
}

/// Check 65's second half: the app started again after it quit with a
/// draft typed, in a session of its own.
async fn draft_after_restart(
    config: &Config,
    report: &mut Report,
    started: std::time::Instant,
    left: &super::notes::DraftLeft,
) {
    log_line(
        started,
        "start the app again, after it quit with a note being typed (check 65)",
    );
    let driver = match start_session(config, &config.app, started).await {
        Ok(driver) => driver,
        Err(e) => {
            report
                .record(
                    Check::NoteDraft,
                    Err(e.context("no WebDriver session on the app started again")),
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
    let offered = run.draft_offered(left).await;
    if offered.is_err() {
        run.failure_screenshot(Check::NoteDraft).await;
    }
    report.record(Check::NoteDraft, offered).await;
    end_session(run, started, "third").await;
}

impl Run<'_> {
    async fn restored(&mut self, looks: &Looks) -> Result<String> {
        let after = &looks.after;
        let seen = self
            .ui
            .wait_within(
                "the page to boot again, in the look check 25 left",
                &format!(
                    "const heading = document.querySelector('.chapter-main .chapter-heading');
                     const seen = (() => {{ {LOOK_JS} }})();
                     const want = arguments[0].settings;
                     return [!!heading && !!seen.fontSize
                         && Object.keys(want).every((name) => seen.settings[name] === want[name]), seen];"
                ),
                vec![after.clone()],
                Some(self.config.session_timeout),
            )
            .await?;
        for field in ["fontSize", "lineHeight", "display", "column", "background"] {
            ensure!(
                seen[field] == after[field],
                "{field} is {} after the restart, was {}",
                seen[field],
                after[field]
            );
        }
        self.open_aa().await?;
        let aa = self.ui.js(AA_JS, Vec::new()).await?;
        ensure!(
            aa["size"] == "Large"
                && aa["lineSpacing"] == json!(["relaxed"])
                && aa["width"] == json!(["wide"])
                && aa["layout"] == json!(["flowing"])
                && aa["theme"] == json!(["vellum"])
                && aa["themeName"] == "Vellum",
            "Aa doesn't show what was saved: {aa}"
        );
        self.close_with_escape("Aa", "aa-popover").await?;
        let shot = self
            .ui
            .screenshot(
                "screenshot: the look after the restart, Vellum",
                &self.shot_path(RESTORED_SHOT),
            )
            .await?;

        // Back as they were before check 25, for the updater's checks.
        let before = &looks.before["settings"];
        self.open_aa().await?;
        for (group, name) in [
            ("line-spacing-choices", "lineSpacing"),
            ("width-choices", "width"),
            ("layout-choices", "layout"),
            ("theme-choices", "theme"),
        ] {
            let value = before[name].as_str().unwrap_or_default();
            self.aa_choose(group, name, value).await?;
        }
        if before["textSize"] == "medium" {
            self.ui.click("Aa: A− (smaller)", "#text-size-down").await?;
            self.setting_is("textSize", "medium").await?;
        }
        self.close_with_escape("Aa", "aa-popover").await?;
        let back = self.look().await?;
        ensure!(
            back["settings"] == *before,
            "the settings couldn't be put back: {} (were {before})",
            back["settings"]
        );

        Ok(format!(
            "closed and started again on the same data: {} (the verses {}px, {}px a line, {}; the column up to \
             {}px; background {}), as left; Aa shows Large, Relaxed, Wide, Flowing and Vellum; \
             screenshots/{RESTORED_SHOT} ({} KB); then put back as before check 25 ({})",
            seen["settings"],
            seen["fontSize"],
            seen["lineHeight"],
            seen["display"],
            seen["column"],
            seen["background"],
            shot / 1024,
            back["settings"]
        ))
    }
}
