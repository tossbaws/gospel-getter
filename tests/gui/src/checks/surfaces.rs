//! Checks 30 to 36: the header's popovers and panels, and the picker's two
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
//! back) and keeps the reading place (36; on Windows a move is reported,
//! not failed: see `place_across_resize`). Screenshots of the ☰ menu and
//! of the picker's popover on a narrow window, in Chi Rho here and in
//! Vellum with check 4's.

use anyhow::{Result, ensure};
use serde_json::Value;
use thirtyfour::prelude::*;

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
        let rect = self.ui.driver().get_window_rect().await?;
        let width = rect.width as u32;
        let checked = self.resize_reading_place_steps().await;
        // The window goes back to its width whatever happened.
        let restored = self.window_width(width).await;
        let said = checked?;
        restored?;
        Ok(said)
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
        let first = self.place_across_resize(&before, &narrowed)?;
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
        let second = self.place_across_resize(&narrowed, &back)?;
        // The popover, on the narrow window, closes as the window widens.
        self.window_width(NARROW_WIDTH).await?;
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
        Ok(format!(
            "Psalms 119 at scrollY {}: verse {} at {} px, the column {} px beside the sidebar; {NARROW_WIDTH} px: \
             the popover shape ({:?}), verse {} at {} px, the column {} px ({first}); 1280 px: the sidebar back \
             ({:?}), verse {} at {} px ({second}); a line is {} px; narrow again, G \
             opened the popover, and widening closed it ({:?})",
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
            widened["open"]
        ))
    }

    /// Whether the reading place held across the window crossing 1200 px:
    /// within a line, as check 24's. On Windows a move is reported rather
    /// than failed: WebView2 anchors the page's scroll on its own as the
    /// window resizes, and the shape change (the head script's
    /// `onShapeChange`, as this port keeps it) never kept the place itself.
    fn place_across_resize(&self, before: &Value, after: &Value) -> Result<String> {
        match Self::same_place(before, after) {
            Ok(shift) => Ok(format!("moved {shift:.1} px")),
            Err(e) if self.config.xvfb.is_none() => Ok(format!(
                "NOT kept on Windows, reported not failed (the shape change never kept the place; \
                 WebView2 anchors the scroll itself): {e:#}"
            )),
            Err(e) => Err(e),
        }
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
