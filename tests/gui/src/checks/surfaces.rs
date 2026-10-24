//! Checks 30 to 37: the header's popovers and panels, and the picker's two
//! shapes, which the Rust pieces (ui-leptos/src/surfaces.rs) took over from
//! the head script, each through the real UI. One panel at a time: Aa,
//! then the Library (Aa closes), then the translations (the Library
//! closes) (30). Escape closes in order: an open popover first, focus back
//! on its button, then reading mode (31). A click outside a popover closes
//! it; the Library stays (32). The ☰ menu by keyboard: ↓ on ☰ opens it on
//! its first item, the arrows, Home and End move, Enter runs Keyboard
//! shortcuts, Tab closes it (33). About keeps Tab inside it, and Escape
//! gives focus back to ☰ (34; Shift+Tab's wrap where the page is told it's
//! a Tab: WebKitGTK gives it as an "Unidentified" key). G and the sidebar's button (35, on a
//! window wide enough for the sidebar). The window narrowed below 1200 px
//! and widened again changes the picker's shape (sidebar to popover and
//! back) and keeps the reading place, also as it widens with the popover
//! open (36), and also when the page was scrolled just before, with no
//! animation frame between the scroll and the crossing (37, #121's D14).
//! Screenshots of the ☰ menu and
//! of the picker's popover on a narrow window, in Chi Rho here and in
//! Vellum with check 4's.

use std::future::Future;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use thirtyfour::prelude::*;

use super::picker::PLACE_JS;
use super::{DESCRIBE_JS, Run, VISIBLE_JS};

/// The screenshots checks 33 and 36 take (check 4 lists them).
pub(super) const SURFACES_SHOTS: [&str; 4] = [
    "33-menu-chi-rho.png",
    "36-picker-popover-narrow-chi-rho.png",
    "33-menu-vellum.png",
    "36-picker-popover-narrow-vellum.png",
];

/// A window narrower than the sidebar's breakpoint (1200 px).
const NARROW_WIDTH: u32 = 1000;

/// Check 37's crossings: where the page is scrolled to, then the width the
/// window crosses 1200 px to, with no animation frame between the two.
/// Each starts from the place the one before left painted.
const SCROLL_THEN_CROSS: [(u32, u32); 4] = [(3000, 1000), (4200, 1280), (2000, 1000), (5100, 1280)];

/// Holds the page's animation frames (its `requestAnimationFrame`
/// callbacks, the page's and the Rust pieces' alike, wait in a queue) and
/// scrolls to `arguments[0]`, so to the page the scroll and what comes
/// next share a frame; then the reading place, as [`PLACE_JS`] finds it.
/// The page lets them go itself after 10 s, whatever becomes of the check
/// (a step timed out, WebDriver gone), and [`RELEASE_FRAMES_JS`] says so.
/// Prepended to [`PLACE_JS`].
const HOLD_AND_SCROLL_JS: &str = "if (!window.__guiCheckFrames) {
        const original = window.requestAnimationFrame;
        const real = original.bind(window);
        const frames = { original, real, holding: false, held: [], timer: 0, gaveUp: false };
        frames.release = () => {
            clearTimeout(frames.timer);
            frames.holding = false;
            const held = frames.held.splice(0);
            for (const callback of held) real(callback);
            return held.length;
        };
        window.__guiCheckFrames = frames;
        window.requestAnimationFrame = (callback) => {
            if (!frames.holding) return real(callback);
            frames.held.push(callback);
            return 0;
        };
    }
    const frames = window.__guiCheckFrames;
    frames.release();
    frames.gaveUp = false;
    frames.holding = true;
    frames.timer = setTimeout(() => { frames.gaveUp = true; frames.release(); }, 10000);
    if (window.__guiCheckSeen) window.__guiCheckSeen.log.push('frames held');
    window.scrollTo(0, arguments[0]);";

/// Lets the page's animation frames run again: the ones held run in the
/// next frame. With `arguments[0]`, the page's own `requestAnimationFrame`
/// is put back too. Returns how many were held, and whether the page had
/// already let them go (after 10 s).
const RELEASE_FRAMES_JS: &str = "const frames = window.__guiCheckFrames;
    if (!frames) return { held: 0, gaveUp: false };
    const said = { held: frames.release(), gaveUp: frames.gaveUp };
    if (arguments[0]) {
        window.requestAnimationFrame = frames.original;
        delete window.__guiCheckFrames;
    }
    return said;";

/// Check 37's record of what the page saw, for its failure message: each
/// `scroll`, `resize` and width media query change (its listeners run
/// after the app's), and each `window.scrollBy`/`scrollTo` call, with the
/// scroll position, `<html>`'s inline `overflow-anchor` and the reading
/// place there (`verse@top`). With `arguments[0]` false, stops recording.
const WATCH_JS: &str = "const old = window.__guiCheckSeen;
    if (old) old.stop();
    if (!arguments[0]) return null;
    const bar = () => document.getElementById('top-bar').getBoundingClientRect().bottom;
    const place = () => {
        const below = bar();
        for (const mark of document.querySelectorAll('.chapter-main .chapter-heading, .chapter-main .verse[data-verse], .chapter-main .compare-gap')) {
            const box = mark.getBoundingClientRect();
            if (box.bottom > below) return `${mark.dataset.verse || 'heading'}@${Math.round(box.top * 10) / 10}`;
        }
        return 'none';
    };
    const seen = { log: [] };
    const note = (what) => seen.log.push(`${what}: scrollY ${Math.round(window.scrollY * 10) / 10}, `
        + `${window.innerWidth} px, anchor ${document.documentElement.style.overflowAnchor || 'auto'}, ${place()}`);
    const onScroll = () => note('scroll');
    const onResize = () => note('resize');
    const onChange = (e) => note(`media query ${e.matches ? 'wide' : 'narrow'}`);
    const wide = window.matchMedia('(min-width: 1200px)');
    window.addEventListener('scroll', onScroll, { passive: true });
    window.addEventListener('resize', onResize);
    wide.addEventListener('change', onChange);
    const wrapped = {};
    for (const name of ['scrollBy', 'scrollTo']) {
        wrapped[name] = window[name];
        window[name] = function (...args) {
            const result = wrapped[name].apply(window, args);
            note(`${name}(${args.map((a) => typeof a === 'number' ? Math.round(a * 10) / 10 : JSON.stringify(a)).join(', ')})`);
            return result;
        };
    }
    seen.stop = () => {
        window.removeEventListener('scroll', onScroll);
        window.removeEventListener('resize', onResize);
        wide.removeEventListener('change', onChange);
        for (const name of Object.keys(wrapped)) window[name] = wrapped[name];
        delete window.__guiCheckSeen;
    };
    window.__guiCheckSeen = seen;
    return null;";

/// What [`WATCH_JS`] has recorded since it was last asked, and starts
/// again.
const SEEN_JS: &str = "const seen = window.__guiCheckSeen;
    return seen ? seen.log.splice(0) : [];";

/// How long putting the page and the window back may take, after check 36
/// or 37, even once the run's budget has run out.
const PUT_BACK_LIMIT: Duration = Duration::from_secs(15);

/// Waits until two animation frames have passed since it was first asked
/// (so the page has noted the place it painted).
const TWO_FRAMES_JS: &str = "if (!window.__guiCheckTick) {
        const tick = { n: 0 };
        window.__guiCheckTick = tick;
        const next = () => { tick.n += 1; if (tick.n < 2) window.requestAnimationFrame(next); };
        window.requestAnimationFrame(next);
    }
    const n = window.__guiCheckTick.n;
    if (n >= 2) delete window.__guiCheckTick;
    return [n >= 2, n];";

/// What the reader sees of the header's surfaces: which show, each button's
/// `aria-expanded`, the open one as the page's reader asks it, reading
/// mode, the picker's shape, and where focus is. Prepended, with
/// [`VISIBLE_JS`] and [`DESCRIBE_JS`].
const SURFACES_JS: &str = "const ids = { translation: 'translation-popover', aa: 'aa-popover',
        library: 'library-panel', menu: 'app-menu', about: 'about-overlay', picker: 'picker' };
    const open = Object.keys(ids).filter((name) => visible(document.getElementById(ids[name])));
    const expanded = {};
    for (const id of ['translation-toggle', 'aa-toggle', 'library-toggle', 'menu-toggle', 'ref-btn', 'sidebar-toggle']) {
        expanded[id] = document.getElementById(id).getAttribute('aria-expanded');
    }
    const s = window.gospelGetterSurfaces;
    const seen = { open, expanded, current: s ? s.current() : 'none',
        mode: document.documentElement.dataset.readerMode || 'off',
        shape: document.documentElement.dataset.picker || null,
        wide: 'wide' in document.documentElement.dataset,
        sheet: !!document.getElementById('shortcuts-overlay') && !document.getElementById('shortcuts-overlay').hidden,
        focus: describe(document.activeElement) };";

impl Run<'_> {
    /// Waits until `ready` (a JavaScript expression over `seen`, as
    /// [`SURFACES_JS`] sees it) is true, and returns what it saw.
    async fn surfaces_are(&self, what: &str, ready: &str) -> Result<Value> {
        self.ui
            .wait(
                what,
                &format!(
                    "{VISIBLE_JS} {DESCRIBE_JS} {SURFACES_JS}
                     return [!!({ready}), seen];"
                ),
            )
            .await
    }

    /// Nothing open, out of reading mode, plain reading.
    async fn nothing_open(&self) -> Result<()> {
        self.plain_reading().await?;
        let seen = self
            .surfaces_are(
                "every popover and panel closed (the sidebar may show)",
                "seen.open.every((name) => name === 'picker') && seen.current === null && seen.mode === 'off'",
            )
            .await;
        if seen.is_err() {
            self.ui.press("press Escape", Key::Escape).await?;
            self.surfaces_are(
                "every popover and panel closed",
                "seen.open.every((name) => name === 'picker') && seen.current === null",
            )
            .await?;
        }
        Ok(())
    }

    /// The selector of the chapter's last verse: far enough down the page
    /// that no popover covers it once it's scrolled into view.
    async fn last_verse(&self) -> Result<String> {
        let n = self
            .ui
            .js(
                "const verses = document.querySelectorAll('.chapter-main .verse[data-verse]');
                 return verses.length ? verses[verses.length - 1].dataset.verse : null;",
                Vec::new(),
            )
            .await?;
        let Some(n) = n.as_str() else {
            anyhow::bail!("no verses on the page");
        };
        Ok(format!(".chapter-main .verse[data-verse=\"{n}\"]"))
    }

    /// Check 30: one panel at a time.
    pub(super) async fn one_panel(&mut self) -> Result<String> {
        self.nothing_open().await?;
        let ui = &self.ui;
        ui.click("open Aa", "#aa-toggle").await?;
        let aa = self
            .surfaces_are(
                "Aa to open, focus in it",
                "seen.current === 'aa' && seen.open.includes('aa') && seen.expanded['aa-toggle'] === 'true'
                 && seen.focus !== '#aa-toggle'",
            )
            .await?;
        self.ui
            .click("open the Library (Aa closes)", "#library-toggle")
            .await?;
        let library = self
            .surfaces_are(
                "the Library to open and Aa to close",
                "seen.current === 'library' && seen.open.includes('library') && !seen.open.includes('aa')
                 && seen.expanded['aa-toggle'] === 'false' && seen.expanded['library-toggle'] === 'true'",
            )
            .await?;
        self.ui.settled("the Library to slide in").await?;
        self.ui
            .click(
                "open the translations (the Library closes)",
                "#translation-toggle",
            )
            .await?;
        let translation = self
            .surfaces_are(
                "the translations to open and the Library to close",
                "seen.current === 'translation' && seen.open.includes('translation') && !seen.open.includes('library')
                 && seen.expanded['library-toggle'] === 'false' && seen.expanded['translation-toggle'] === 'true'",
            )
            .await?;
        self.ui
            .press("press Escape (close the translations)", Key::Escape)
            .await?;
        self.surfaces_are("the translations to close", "seen.current === null")
            .await?;
        Ok(format!(
            "Aa open ({:?}, focus {}); the Library: open {:?}; the translations: open {:?}, focus {}; Escape closed them",
            aa["open"],
            aa["focus"].as_str().unwrap_or("?"),
            library["open"],
            translation["open"],
            translation["focus"].as_str().unwrap_or("?")
        ))
    }

    /// Check 31: Escape closes the open popover first (focus back on its
    /// button), then leaves reading mode.
    pub(super) async fn escape_chain(&mut self) -> Result<String> {
        self.nothing_open().await?;
        self.ui.click("open Aa", "#aa-toggle").await?;
        self.surfaces_are("Aa to open", "seen.current === 'aa'")
            .await?;
        self.ui
            .press("press Escape (close Aa)", Key::Escape)
            .await?;
        let closed = self
            .surfaces_are(
                "Aa to close, focus back on its button",
                "seen.current === null && !seen.open.includes('aa') && seen.focus === '#aa-toggle'",
            )
            .await?;
        self.ui.click("open Aa", "#aa-toggle").await?;
        self.surfaces_are("Aa to open", "seen.current === 'aa'")
            .await?;
        self.ui
            .click("Aa: Reading mode", "#reader-mode-btn")
            .await?;
        self.surfaces_are(
            "reading mode, Aa closed",
            "seen.mode === 'on' && seen.current === null && seen.focus === '#reader-exit'",
        )
        .await?;
        self.ui
            .click(
                "open the translations in reading mode",
                "#translation-toggle",
            )
            .await?;
        self.surfaces_are(
            "the translations to open over reading mode",
            "seen.current === 'translation' && seen.mode === 'on'",
        )
        .await?;
        self.ui
            .press("press Escape (close the translations)", Key::Escape)
            .await?;
        let first = self
            .surfaces_are(
                "the translations to close first, reading mode staying, focus on their button",
                "seen.current === null && seen.mode === 'on' && seen.focus === '#translation-toggle'",
            )
            .await?;
        self.ui
            .press("press Escape (leave reading mode)", Key::Escape)
            .await?;
        let left = self
            .surfaces_are(
                "reading mode to end",
                "seen.mode === 'off' && seen.focus === '#aa-toggle'",
            )
            .await?;
        Ok(format!(
            "Aa, Escape: closed, focus on {}; reading mode with the translations open: Escape closed them (reading \
             mode {}, focus on {}), Escape again left reading mode (focus on {})",
            closed["focus"].as_str().unwrap_or("?"),
            first["mode"].as_str().unwrap_or("?"),
            first["focus"].as_str().unwrap_or("?"),
            left["focus"].as_str().unwrap_or("?")
        ))
    }

    /// Check 32: a click outside a popover closes it; the Library stays.
    pub(super) async fn outside_click(&mut self) -> Result<String> {
        self.nothing_open().await?;
        let mut said = Vec::new();
        for (button, name, what) in [
            ("#translation-toggle", "translation", "the translations"),
            ("#aa-toggle", "aa", "Aa"),
            ("#menu-toggle", "menu", "the ☰ menu"),
        ] {
            self.ui.click(&format!("open {what}"), button).await?;
            self.surfaces_are(
                &format!("{what} to open"),
                &format!("seen.current === '{name}'"),
            )
            .await?;
            let verse = self.last_verse().await?;
            self.ui
                .click(
                    &format!("click the chapter's last verse (outside {what}, below it)"),
                    &verse,
                )
                .await?;
            let seen = self
                .surfaces_are(
                    &format!("{what} to close"),
                    &format!("seen.current === null && !seen.open.includes('{name}')"),
                )
                .await?;
            said.push(format!(
                "{what} closed (focus {})",
                seen["focus"].as_str().unwrap_or("?")
            ));
            // The click selected the verse too: Escape clears it.
            self.plain_reading().await?;
        }
        self.ui.click("open the Library", "#library-toggle").await?;
        self.surfaces_are("the Library to open", "seen.current === 'library'")
            .await?;
        self.ui.settled("the Library to slide in").await?;
        let verse = self.last_verse().await?;
        self.ui
            .click(
                "click the chapter's last verse (outside the Library)",
                &verse,
            )
            .await?;
        let stays = self
            .surfaces_are(
                "the Library to stay open",
                "seen.current === 'library' && seen.open.includes('library')",
            )
            .await?;
        self.ui.click("close the Library", "#library-close").await?;
        let closed = self
            .surfaces_are(
                "the Library to close, focus on its button",
                "seen.current === null && seen.focus === '#library-toggle'",
            )
            .await?;
        self.plain_reading().await?;
        Ok(format!(
            "a click on the chapter's last verse: {}; the Library stayed ({:?}); its ×: closed, focus on {}",
            said.join(", "),
            stays["open"],
            closed["focus"].as_str().unwrap_or("?")
        ))
    }

    /// Check 33: the ☰ menu by keyboard.
    pub(super) async fn menu_keyboard(&mut self) -> Result<String> {
        self.nothing_open().await?;
        self.ui
            .js(
                "document.getElementById('menu-toggle').focus(); return null;",
                Vec::new(),
            )
            .await?;
        self.ui.press("press ↓ on ☰", Key::Down).await?;
        let opened = self
            .surfaces_are(
                "the menu to open, focus on its first item",
                "seen.current === 'menu' && seen.expanded['menu-toggle'] === 'true' && seen.focus === '#export-data-btn'",
            )
            .await?;
        self.shot("screenshot: the ☰ menu, Chi Rho", SURFACES_SHOTS[0])
            .await;
        let mut moves = Vec::new();
        for (key, name, to) in [
            (Key::Down, "↓", "#import-data-btn"),
            (Key::End, "End", "#about-btn"),
            (Key::Down, "↓", "#export-data-btn"),
            (Key::Up, "↑", "#about-btn"),
            (Key::Home, "Home", "#export-data-btn"),
            (Key::Up, "↑", "#about-btn"),
            (Key::Up, "↑", "#shortcuts-btn"),
        ] {
            self.ui.press(&format!("press {name}"), key).await?;
            self.surfaces_are(
                &format!("{name}: focus on {to}"),
                &format!("seen.current === 'menu' && seen.focus === '{to}'"),
            )
            .await?;
            moves.push(format!("{name} {to}"));
        }
        self.ui
            .press("press Enter (Keyboard shortcuts)", Key::Enter)
            .await?;
        let sheet = self
            .surfaces_are(
                "the shortcuts sheet to open, the menu closed",
                "seen.sheet && seen.current === null && !seen.open.includes('menu')",
            )
            .await?;
        self.ui
            .press("press Escape (close the sheet)", Key::Escape)
            .await?;
        let back = self
            .surfaces_are(
                "the sheet to close, focus back on ☰",
                "!seen.sheet && seen.focus === '#menu-toggle'",
            )
            .await?;
        self.ui.press("press ↑ on ☰", Key::Up).await?;
        self.surfaces_are(
            "the menu to open, focus on its last item",
            "seen.current === 'menu' && seen.focus === '#about-btn'",
        )
        .await?;
        self.ui
            .press("press Tab (close the menu)", Key::Tab)
            .await?;
        let tabbed = self
            .surfaces_are(
                "Tab to close the menu, focus on ☰",
                "seen.current === null && !seen.open.includes('menu') && seen.focus === '#menu-toggle'",
            )
            .await?;
        Ok(format!(
            "↓ on ☰: open, focus on {}; {}; Enter: the shortcuts sheet (menu closed: {:?}); Escape: focus on {}; \
             ↑ on ☰: the last item; Tab: closed, focus on {}",
            opened["focus"].as_str().unwrap_or("?"),
            moves.join(", "),
            sheet["open"],
            back["focus"].as_str().unwrap_or("?"),
            tabbed["focus"].as_str().unwrap_or("?")
        ))
    }

    /// Presses Shift+Tab: on Linux as the keyboard does, through the
    /// private display; on Windows through WebDriver.
    async fn shift_tab(&self) -> Result<()> {
        if let Some(xvfb) = &self.config.xvfb {
            return self
                .ui
                .step("press Shift+Tab (on the private display)", async {
                    let windows = xvfb.windows_named("Gospel Getter").await?;
                    let Some((window, _)) = windows.first() else {
                        anyhow::bail!("no window named \"… Gospel Getter\" on the private display");
                    };
                    xvfb.xdotool(&["windowfocus", "--sync", window]).await?;
                    xvfb.xdotool(&["key", "--clearmodifiers", "shift+Tab"])
                        .await?;
                    Ok(())
                })
                .await;
        }
        self.ui
            .step("press Shift+Tab", async {
                self.ui
                    .driver()
                    .action_chain()
                    .key_down(Key::Shift)
                    .send_keys(Key::Tab)
                    .key_up(Key::Shift)
                    .perform()
                    .await?;
                Ok(())
            })
            .await
    }

    /// Check 34: About keeps Tab inside it; Escape gives focus back to ☰.
    pub(super) async fn about_trap(&mut self) -> Result<String> {
        self.nothing_open().await?;
        self.ui.click("open ☰", "#menu-toggle").await?;
        self.ui
            .click("☰: About Gospel Getter", "#about-btn")
            .await?;
        let opened = self
            .surfaces_are(
                "About to open, focus on its ×",
                "seen.current === 'about' && !seen.open.includes('menu') && seen.focus === '#about-close'",
            )
            .await?;
        let stops = self
            .ui
            .js(
                "const panel = document.getElementById('about-panel');
                 const stops = [...panel.querySelectorAll('input, button, a[href]')]
                     .filter((el) => !el.disabled && !el.closest('[hidden]'));
                 return stops.map((el) => el.id || el.className || el.tagName);",
                Vec::new(),
            )
            .await?;
        let count = stops.as_array().map_or(0, Vec::len);
        ensure!(count >= 2, "About has fewer than two stops: {stops}");
        let mut seen_focus = Vec::new();
        for i in 0..count + 1 {
            self.ui
                .press(&format!("press Tab ({})", i + 1), Key::Tab)
                .await?;
            let seen = self
                .surfaces_are(
                    "focus to stay inside About",
                    "seen.current === 'about'
                     && document.getElementById('about-overlay').contains(document.activeElement)",
                )
                .await?;
            seen_focus.push(seen["focus"].as_str().unwrap_or("?").to_owned());
        }
        // Shift+Tab from the first stop wraps to the last.
        self.ui
            .js(
                "document.getElementById('about-close').focus();
                 window.__guiCheckKeys = [];
                 document.addEventListener('keydown', (e) => window.__guiCheckKeys.push(
                     `${e.key}${e.shiftKey ? '+shift' : ''} on ${document.activeElement && document.activeElement.id}`), true);
                 return null;",
                Vec::new(),
            )
            .await?;
        self.shift_tab().await?;
        let keys = self
            .ui
            .wait(
                "the page to see the Shift+Tab",
                "const keys = window.__guiCheckKeys || [];
                 return [keys.some((k) => !k.startsWith('Shift')), keys];",
            )
            .await?;
        self.ui.log(&format!("  keys seen: {keys}"));
        let as_tab = keys.as_array().is_some_and(|keys| {
            keys.iter()
                .any(|k| k.as_str().is_some_and(|k| k.starts_with("Tab+shift")))
        });
        let backward = if as_tab {
            let wrapped = self
                .surfaces_are(
                    "Shift+Tab to wrap to About's last stop",
                    "seen.current === 'about' && seen.focus !== '#about-close'
                     && document.getElementById('about-overlay').contains(document.activeElement)",
                )
                .await?;
            format!(
                "Shift+Tab on ×: {}",
                wrapped["focus"].as_str().unwrap_or("?")
            )
        } else {
            // WebKitGTK here hands the page Shift+Tab (GTK's ISO_Left_Tab)
            // as an "Unidentified" key, from WebDriver and from the private
            // display's own keyboard alike: no page handler can tell it's a
            // Tab, so the trap's backward wrap can't be checked here. The
            // forward wrap was.
            self.ui
                .js(
                    "document.getElementById('about-close').focus(); return null;",
                    Vec::new(),
                )
                .await?;
            format!(
                "Shift+Tab on × not checked here: the page saw {keys} (WebKitGTK gives Shift+Tab as \
                 \"Unidentified\", so no handler can tell it's a Tab)"
            )
        };
        self.ui
            .press("press Escape (close About)", Key::Escape)
            .await?;
        let closed = self
            .surfaces_are(
                "About to close, focus back on ☰",
                "seen.current === null && !seen.open.includes('about') && seen.focus === '#menu-toggle'",
            )
            .await?;
        Ok(format!(
            "☰ → About: focus on {}; {count} stops; Tab ×{}: {}; {backward}; Escape: focus on {}",
            opened["focus"].as_str().unwrap_or("?"),
            count + 1,
            seen_focus.join(" → "),
            closed["focus"].as_str().unwrap_or("?")
        ))
    }

    /// Check 35: G and the sidebar's button, on a wide window.
    pub(super) async fn g_and_sidebar(&mut self) -> Result<String> {
        self.nothing_open().await?;
        if let Err(why) = self.ensure_wide().await? {
            return self.not_wide(why);
        }
        self.surfaces_are(
            "the sidebar to show",
            "seen.shape === 'sidebar' && seen.open.includes('picker') && seen.expanded['sidebar-toggle'] === 'true'",
        )
        .await?;
        self.focus_off_controls().await?;
        self.ui.press("press G (into the sidebar)", "g").await?;
        let into = self
            .surfaces_are(
                "focus in the sidebar, on the chapter being read, no popover",
                "seen.current === null && seen.shape === 'sidebar'
                 && document.getElementById('picker').contains(document.activeElement)
                 && document.activeElement.hasAttribute('aria-current')",
            )
            .await?;
        self.ui
            .click("click the sidebar's button (hide it)", "#sidebar-toggle")
            .await?;
        let hidden = self
            .surfaces_are(
                "the sidebar to hide",
                "!seen.shape && !seen.open.includes('picker') && seen.expanded['sidebar-toggle'] === 'false'
                 && seen.expanded['ref-btn'] === 'false'",
            )
            .await?;
        self.focus_off_controls().await?;
        self.ui.press("press G (show it again)", "g").await?;
        let shown = self
            .surfaces_are(
                "G to bring the sidebar back, focus in it",
                "seen.shape === 'sidebar' && seen.open.includes('picker') && seen.current === null
                 && seen.expanded['sidebar-toggle'] === 'true'
                 && document.getElementById('picker').contains(document.activeElement)",
            )
            .await?;
        let stored = self
            .ui
            .js(
                "try { return localStorage.getItem('gospel-getter-sidebar'); } catch (e) { return String(e); }",
                Vec::new(),
            )
            .await?;
        Ok(format!(
            "G: focus on {} in the sidebar; its button: hidden ({:?}); G: shown again, focus on {} (saved {stored})",
            into["focus"].as_str().unwrap_or("?"),
            hidden["expanded"]["sidebar-toggle"],
            shown["focus"].as_str().unwrap_or("?")
        ))
    }

    /// Check 36: the window narrowed below 1200 px and widened again: the
    /// sidebar becomes the popover and back, the reading place kept.
    pub(super) async fn resize_reading_place(&mut self) -> Result<String> {
        self.nothing_open().await?;
        if let Err(why) = self.ensure_wide().await? {
            return self.not_wide(why);
        }
        let width = self.window_width_now().await?;
        let checked = self.resize_reading_place_steps().await;
        // The window goes back to its width whatever happened.
        let restored = self
            .put_back("put the window back to its width", self.window_width(width))
            .await;
        let said = checked?;
        restored?;
        Ok(said)
    }

    /// The window's width now.
    async fn window_width_now(&self) -> Result<u32> {
        let rect = self
            .ui
            .step("the window's width", async {
                Ok(self.ui.driver().get_window_rect().await?)
            })
            .await?;
        Ok(rect.width as u32)
    }

    /// Puts something back after check 36 or 37 (the page's frames, the
    /// window's width), within [`PUT_BACK_LIMIT`] whatever is left of the
    /// run's budget.
    async fn put_back<T>(&self, what: &str, action: impl Future<Output = Result<T>>) -> Result<T> {
        self.ui.log(what);
        let done = tokio::time::timeout(PUT_BACK_LIMIT, action)
            .await
            .with_context(|| format!("\"{what}\" timed out after {}s", PUT_BACK_LIMIT.as_secs()))?
            .with_context(|| format!("\"{what}\" failed"));
        if let Err(e) = &done {
            self.ui.log(&format!("  FAILED: {e:#}"));
        }
        done
    }

    async fn resize_reading_place_steps(&mut self) -> Result<String> {
        self.surfaces_are(
            "the sidebar to show",
            "seen.shape === 'sidebar' && seen.open.includes('picker')",
        )
        .await?;
        if self.heading().await? != "Psalms 119" {
            self.read_in_picker(19, 119, "Psalms 119").await?;
        }
        self.ui
            .wait(
                "the page to be scrolled into Psalms 119",
                "window.scrollTo(0, 3000); return [window.scrollY > 2500, Math.round(window.scrollY)];",
            )
            .await?;
        self.ui
            .js(
                "if (document.activeElement) document.activeElement.blur(); return null;",
                Vec::new(),
            )
            .await?;
        let before = self
            .reading_place("the first verse under the bar, with the sidebar")
            .await?;
        self.window_width(NARROW_WIDTH).await?;
        let narrow = self
            .surfaces_are(
                "the popover shape: no sidebar, the picker hidden",
                "!seen.wide && !seen.shape && !seen.open.includes('picker') && seen.expanded['ref-btn'] === 'false'",
            )
            .await?;
        self.ui.settled("the page to settle, narrow").await?;
        let narrowed = self
            .reading_place("the first verse under the bar, narrow")
            .await?;
        let first = Self::place_across_resize(&before, &narrowed)?;
        self.window_width(1280).await?;
        let wide = self
            .surfaces_are(
                "the sidebar to come back",
                "seen.wide && seen.shape === 'sidebar' && seen.current === null && seen.open.includes('picker')",
            )
            .await?;
        self.ui.settled("the page to settle, wide").await?;
        let back = self
            .reading_place("the first verse under the bar, wide again")
            .await?;
        let second = Self::place_across_resize(&narrowed, &back)?;
        // The popover, on the narrow window, closes as the window widens,
        // and the place stays then too.
        self.window_width(NARROW_WIDTH).await?;
        self.ui.settled("the page to settle, narrow again").await?;
        let opened = self
            .reading_place("the first verse under the bar, narrow again")
            .await?;
        self.ui.press("press G (the popover)", "g").await?;
        self.surfaces_are(
            "the popover to open",
            "seen.current === 'picker' && seen.open.includes('picker')",
        )
        .await?;
        self.shot(
            "screenshot: the picker's popover, narrow, Chi Rho",
            SURFACES_SHOTS[1],
        )
        .await;
        self.window_width(1280).await?;
        let widened = self
            .surfaces_are(
                "the popover to close as the window widens, the sidebar back",
                "seen.wide && seen.shape === 'sidebar' && seen.current === null && seen.open.includes('picker')
                 && seen.expanded['ref-btn'] === 'true'",
            )
            .await?;
        self.ui
            .settled("the page to settle, wide with the popover closed")
            .await?;
        let closed = self
            .reading_place("the first verse under the bar, the popover closed by widening")
            .await?;
        let third = Self::place_across_resize(&opened, &closed)?;
        Ok(format!(
            "Psalms 119 at scrollY {}: verse {} at {} px, the column {} px beside the sidebar; {NARROW_WIDTH} px: \
             the popover shape ({:?}), verse {} at {} px, the column {} px ({first}); 1280 px: the sidebar back \
             ({:?}), verse {} at {} px ({second}); a line is {} px; narrow again, verse {} at {} px, G \
             opened the popover, and widening closed it ({:?}), verse {} at {} px ({third})",
            before["scrollY"],
            before["verse"],
            before["top"],
            before["column"],
            narrow["open"],
            narrowed["verse"],
            narrowed["top"],
            narrowed["column"],
            wide["open"],
            back["verse"],
            back["top"],
            before["line"],
            opened["verse"],
            opened["top"],
            widened["open"],
            closed["verse"],
            closed["top"]
        ))
    }

    /// Check 37: the page scrolled and the window crossing 1200 px with no
    /// animation frame between them (#121's D14: the place the page noted
    /// in the frame before the scroll is out of date by then). The reading
    /// place found just after the scroll is kept, on every platform, within
    /// a line as check 36's.
    pub(super) async fn scroll_then_resize(&mut self) -> Result<String> {
        self.nothing_open().await?;
        if let Err(why) = self.ensure_wide().await? {
            return self.not_wide(why);
        }
        let width = self.window_width_now().await?;
        let checked = self.scroll_then_resize_steps().await;
        // The frames run again (the page lets them go itself after 10 s
        // anyway), the page's own requestAnimationFrame and scroll calls are
        // put back, and the window goes back to its width, whatever
        // happened.
        let released = self
            .put_back(
                "let the page's frames go, and put its requestAnimationFrame back",
                self.ui.js(RELEASE_FRAMES_JS, vec![json!(true)]),
            )
            .await;
        let unwatched = self
            .put_back(
                "stop recording what the page sees",
                self.ui.js(WATCH_JS, vec![json!(false)]),
            )
            .await;
        let restored = self
            .put_back("put the window back to its width", self.window_width(width))
            .await;
        let said = checked?;
        released?;
        unwatched?;
        restored?;
        Ok(said)
    }

    async fn scroll_then_resize_steps(&mut self) -> Result<String> {
        self.surfaces_are(
            "the sidebar to show",
            "seen.shape === 'sidebar' && seen.open.includes('picker')",
        )
        .await?;
        if self.heading().await? != "Psalms 119" {
            self.read_in_picker(19, 119, "Psalms 119").await?;
        }
        self.ui
            .step(
                "start at the top, and record what the page sees",
                self.ui.js(
                    &format!(
                        "if (document.activeElement) document.activeElement.blur(); \
                         window.scrollTo(0, 0); {WATCH_JS}"
                    ),
                    vec![json!(true)],
                ),
            )
            .await?;
        let mut said = Vec::new();
        for (y, width) in SCROLL_THEN_CROSS {
            // The place painted where the last crossing left the page (the
            // top of the chapter, first).
            self.ui
                .wait("two animation frames (the place painted)", TWO_FRAMES_JS)
                .await?;
            let painted = self
                .reading_place("the place painted, before the frames are held")
                .await?;
            self.ui.js(SEEN_JS, Vec::new()).await?;
            let before = self
                .ui
                .step(
                    &format!("hold the page's frames and scroll to {y}"),
                    self.ui
                        .js(&format!("{HOLD_AND_SCROLL_JS} {PLACE_JS}"), vec![json!(y)]),
                )
                .await?;
            ensure!(
                !before.is_null(),
                "nothing of the chapter under the bar at scrollY {y}"
            );
            let scrolled = before["scrollY"].as_f64().unwrap_or(0.0);
            ensure!(
                (scrolled - f64::from(y)).abs() < 1.0,
                "the page didn't scroll to {y}: {before}"
            );
            self.ui.log(&format!("  place: {before}"));
            self.window_width(width).await?;
            let resized = self
                .reading_place(&format!("the place at {width} px, the frames still held"))
                .await?;
            let released = self
                .ui
                .step(
                    "let the page's frames go",
                    self.ui.js(RELEASE_FRAMES_JS, vec![json!(false)]),
                )
                .await?;
            self.ui
                .settled(&format!("the page to settle at {width} px"))
                .await?;
            let after = self
                .reading_place(&format!("the first verse under the bar, at {width} px"))
                .await?;
            let seen = self.ui.js(SEEN_JS, Vec::new()).await?;
            let held = released["held"].as_u64().unwrap_or(0);
            let steps = || {
                format!(
                    "the place painted {painted}; after scrollTo {before}; at {width} px with the frames \
                     held {resized}; after they ran ({held} held) {after}; the page saw: {}",
                    Self::seen_list(&seen)
                )
            };
            if released["gaveUp"] == Value::Bool(true) {
                bail!(
                    "scrolled to {y}, then {width} px: the page let its frames go itself (10 s) before \
                     the window had crossed; {}",
                    steps()
                );
            }
            let shift = Self::same_place(&before, &after)
                .map_err(|e| e.context(format!("scrolled to {y}, then {width} px; {}", steps())))?;
            said.push(format!(
                "scrollY {y}, then {width} px with no frame between ({held} frame callbacks held): verse {} \
                 at {} px, then verse {} at {} px, scrollY {} (moved {shift:.1} px)",
                before["verse"], before["top"], after["verse"], after["top"], after["scrollY"]
            ));
        }
        Ok(format!("Psalms 119: {}", said.join("; ")))
    }

    /// [`SEEN_JS`]'s record, one entry after another.
    fn seen_list(seen: &Value) -> String {
        let entries: Vec<&str> = seen
            .as_array()
            .map(|list| list.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if entries.is_empty() {
            "nothing".to_owned()
        } else {
            entries.join("; ")
        }
    }

    /// Whether the reading place held across the window crossing 1200 px:
    /// within a line, as check 24's, on every platform.
    fn place_across_resize(before: &Value, after: &Value) -> Result<String> {
        Self::same_place(before, after).map(|shift| format!("moved {shift:.1} px"))
    }

    /// Check 4's Vellum screenshots of the ☰ menu and of the picker's
    /// popover on a narrow window.
    pub(super) async fn surfaces_vellum_screenshots(&mut self) -> Result<()> {
        self.nothing_open().await?;
        self.ui.click("open ☰", "#menu-toggle").await?;
        self.surfaces_are("the menu to open", "seen.current === 'menu'")
            .await?;
        self.shot("screenshot: the ☰ menu, Vellum", SURFACES_SHOTS[2])
            .await;
        self.ui
            .press("press Escape (close the menu)", Key::Escape)
            .await?;
        self.surfaces_are("the menu to close", "seen.current === null")
            .await?;
        let width = self.window_width(NARROW_WIDTH).await?;
        let shot = async {
            self.focus_off_controls().await?;
            self.ui.press("press G (open the picker)", "g").await?;
            self.surfaces_are("the popover to open", "seen.current === 'picker'")
                .await?;
            self.shot(
                "screenshot: the picker's popover, narrow, Vellum",
                SURFACES_SHOTS[3],
            )
            .await;
            self.ui
                .press("press Escape (close the picker)", Key::Escape)
                .await?;
            self.surfaces_are("the popover to close", "seen.current === null")
                .await?;
            Ok::<(), anyhow::Error>(())
        }
        .await;
        let restored = self.window_width(width).await;
        shot?;
        restored?;
        Ok(())
    }
}
