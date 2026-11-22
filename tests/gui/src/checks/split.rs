//! Check 75: two passages side by side (ui-leptos/src/pane.rs), opened
//! through `window.gospelGetterPanes` (the reader has no way in yet), on
//! Psalm 119 in the first pane (the KJV) and in the second (the WEB), long
//! enough for every step to scroll:
//!
//! - Opening the second pane moves the first's place from the window to
//!   its own column: the same verse, the same distance below the bar over
//!   it, within a line. The page itself no longer scrolls.
//! - Each pane's column scrolls on its own: scrolling one leaves the other
//!   where it was.
//! - Page Down and Space, pressed with focus on the page itself, scroll the
//!   active pane's column, and only it (the dispatcher scrolls it, rather
//!   than whichever column the webview would pick), in both panes in turn.
//! - The selection card opened on a verse of the second pane is placed in
//!   that pane, within its view (below its bar, above its foot).
//! - Opening a verse beside again (the second pane's jump) brings it into
//!   the second pane's view (`scrollIntoView` in a column under the bar).
//! - The window crossing 1200 px and back keeps both panes' places, within
//!   a line (not run where the window can't be made that wide: the Windows
//!   runner's screen, and the check says so).
//! - × closes the second pane: the first pane's place goes back to the
//!   window, the same verse within a line, and focus to its reference
//!   button.
//!
//! Whatever happens, the second pane is closed after, and the window's
//! width and the first pane's chapter put back: the checks share one app
//! session.
//!
//! Check 76: the same two panes on a window too narrow for them side by
//! side (S7), and in reading mode (D6), each pane's place kept, within a
//! line, by the same measure:
//!
//! - The window crossing 900 px narrower makes them tabs: only the active
//!   pane (the second) shows, where it was.
//! - Choosing the first pane's tab shows it, at the place it had side by
//!   side; choosing the second's, at its own.
//! - The window crossing 900 px wider puts them side by side again, both
//!   where they were.
//! - R shows the active pane alone, where it was; Escape leaves reading
//!   mode, both side by side, both where they were.
//!
//! Whatever happens, reading mode is left, the second pane closed, and the
//! window's width and the first pane's chapter put back.
//!
//! Check 77: two panes remembered as the app quits (S9), in two sessions of
//! its own after check 65's (not run on Windows, for check 26's reason):
//!
//! - The first opens the second pane on Psalm 119 (the WEB), scrolls verse
//!   40 to 13 px above its bar and the first pane's chapter to a verse of
//!   its own, and waits for the app to keep both places: the second's on
//!   this computer (`gospel-getter-second-pane`, with
//!   `gospel-getter-split` on), the first's with the backend.
//! - The app started again is split from the start, the first pane active
//!   (D8), each pane's verse within a pixel of where it was left, and still
//!   there once the page has settled.
//!
//! Whatever happens, each session closes the second pane before it ends,
//! which forgets both keys: no other session starts split.
//!
//! Check 78: Open beside, the reader's way in (S8), from a real
//! cross-reference preview: John 3:16 selected in the first pane, its
//! Romans 5:8 chip, then Open beside (a pointer's click):
//!
//! - Two panes, side by side and both in view: John 3 in the first,
//!   Romans 5 in the second, in the first pane's translation (D9), verse 8
//!   selected there, the second pane active (S13).
//! - F6 makes the first pane active, focus on its bar's reference button;
//!   Shift+F6 the second again (D10). Whether the webview hands F6 to the
//!   page at all is said either way (the design's §4.5 left it
//!   unverified): if it doesn't, the palette's Switch pane (F6) does the
//!   same, and the check says F6 never arrived rather than failing.
//! - × closes the second pane: one pane, focus on its reference button.
//! - In reading mode (R, a real key), the same Open beside leaves it first,
//!   as S does (B12): reading mode off, both panes side by side and in
//!   view, the second active, the first pane's place (the verse at the top
//!   of its view) where it was in reading mode, within a line. R then shows
//!   the second pane alone, in reading mode.
//!
//! Whatever happens, reading mode is left, the second pane closed, and the
//! window's width and the first pane's chapter put back.

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use thirtyfour::prelude::*;

use super::Run;

/// Psalms, by the id the backend gives it, and its longest chapter.
const PSALMS: u32 = 19;
const PSALM: u32 = 119;

/// The window width the 1200 px crossing goes to and back from (the
/// panes' bars still shown, at 900 px and up).
const NARROW_WIDTH: u32 = 1000;
const WIDE_WIDTH: u32 = 1280;

/// Each pane's place, as the reader sees it: its column's scroll position,
/// the first heading or verse below its bar (or, with no bar shown, as tabs
/// or in reading mode, below the column's top) and how far below it that
/// starts, a line's height, the band in view's height and how far the
/// column can scroll, or `null` for a column hidden; and the window's
/// scroll, whether two panes are open and which is active, whether they're
/// tabs and whether reading mode is on. With one pane, the first pane's
/// place is in the window, below the top bar.
const PANES_JS: &str = "const marks = '.chapter-main .chapter-heading, .chapter-main .verse[data-verse]';
    const first = (root, bar) => {
        for (const mark of root.querySelectorAll(marks)) {
            if (mark.getBoundingClientRect().bottom > bar) return mark;
        }
        return null;
    };
    const place = (root, bar, y, extra) => {
        const mark = root ? first(root, bar) : null;
        if (!mark) return null;
        return Object.assign({ y: Math.round(y * 10) / 10, verse: mark.dataset.verse || 'heading',
            top: Math.round((mark.getBoundingClientRect().top - bar) * 10) / 10,
            line: parseFloat(getComputedStyle(mark).lineHeight) || 0 }, extra);
    };
    const column = (name) => {
        const col = document.querySelector(`.pane[data-pane=\"${name}\"]`);
        if (!col || col.hidden) return null;
        const rect = col.getBoundingClientRect();
        const barBox = col.querySelector('.pane-bar').getBoundingClientRect();
        const bar = barBox.height > 0 ? barBox.bottom : rect.top;
        return place(col, bar, col.scrollTop, { view: Math.round(rect.top + col.clientHeight - bar),
            max: col.scrollHeight - col.clientHeight, bar: Math.round(bar), foot: Math.round(rect.bottom),
            left: Math.round(rect.left), right: Math.round(rect.right) });
    };
    const split = !!document.documentElement.dataset.split;
    const topBar = document.getElementById('top-bar').getBoundingClientRect().bottom;
    return { split, active: window.gospelGetterPanes.active(), windowY: window.scrollY,
        tabs: 'paneTabs' in document.documentElement.dataset,
        reading: document.documentElement.dataset.readerMode === 'on',
        first: split ? column('first') : place(document.getElementById('reading-pane'), topBar, window.scrollY, {}),
        second: column('second'),
        heading2: (document.querySelector('#reading-pane-2 .chapter-heading') || {}).textContent || null,
        focus: document.activeElement ? (document.activeElement.id || document.activeElement.tagName) : null };";

/// The window widths check 76 crosses 900 px between: side by side (wide
/// enough for both bars, not for the sidebar, which every window reaches,
/// the Windows runner's included), and tabs.
const SIDE_BY_SIDE_WIDTH: u32 = 1000;
const TABS_WIDTH: u32 = 800;

/// A press in a pane's bar, away from its controls (what a pointer pressed
/// there does, D8), then focus off any control: keys then go to the page.
const PRESS_IN_BAR_JS: &str = "const spacer = document.querySelector(`.pane[data-pane=\"${arguments[0]}\"] .pane-bar-spacer`);
    spacer.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
    if (document.activeElement) document.activeElement.blur();
    return window.gospelGetterPanes.active();";

impl Run<'_> {
    /// Check 75.
    pub(super) async fn split_panes(&mut self) -> Result<String> {
        self.plain_reading().await?;
        let width = self.split_window_width().await?;
        let was = self
            .ui
            .js(
                "const inner = document.getElementById('reading-pane-inner');
                 const h = document.querySelector('.chapter-main .chapter-heading');
                 return { book: Number(inner.dataset.currentBook), chapter: Number(inner.dataset.currentChapter),
                          heading: h.textContent.trim() };",
                Vec::new(),
            )
            .await?;
        let checked = self.split_panes_steps().await;
        let put_back = self.split_clean_up(width, &was).await;
        let said = checked?;
        put_back?;
        Ok(said)
    }

    /// The window's width now.
    async fn split_window_width(&self) -> Result<u32> {
        let rect = self.ui.driver().get_window_rect().await?;
        Ok(rect.width as u32)
    }

    /// The panes' places now ([`PANES_JS`]).
    async fn panes(&self) -> Result<Value> {
        self.ui.js(PANES_JS, Vec::new()).await
    }

    /// Waits until `ready` holds of the panes' places (`seen`).
    async fn panes_are(&self, what: &str, ready: &str) -> Result<Value> {
        self.ui
            .wait(
                what,
                &format!(
                    "const seen = (() => {{ {PANES_JS} }})();
                     return [!!({ready}), seen];"
                ),
            )
            .await
    }

    /// The same verse, the same distance below the bar over it, within a
    /// line.
    fn same_split_place(what: &str, before: &Value, after: &Value) -> Result<String> {
        let shift = (after["top"].as_f64().unwrap_or(f64::NAN)
            - before["top"].as_f64().unwrap_or(f64::NAN))
        .abs();
        let line = before["line"].as_f64().unwrap_or(0.0);
        if before["verse"] != after["verse"] || shift.is_nan() || shift > line {
            bail!("{what}: the place moved: before {before}, after {after} (a line is {line} px)");
        }
        Ok(format!(
            "{what}: verse {} kept ({shift:.1} px)",
            before["verse"]
        ))
    }

    async fn split_panes_steps(&mut self) -> Result<String> {
        let ui = &self.ui;
        let mut said = Vec::new();
        if self.heading().await? != "Psalms 119" {
            self.read_in_picker(PSALMS, PSALM, "Psalms 119").await?;
        }
        ui.wait(
            "the page to be scrolled into Psalms 119",
            "window.scrollTo(0, 3000); return [window.scrollY > 2500, Math.round(window.scrollY)];",
        )
        .await?;
        ui.js(
            "if (document.activeElement) document.activeElement.blur(); return null;",
            Vec::new(),
        )
        .await?;
        ui.settled("the page to settle").await?;
        let single = self.panes().await?;
        ensure!(
            single["first"]["verse"].is_string(),
            "no place in the first pane: {single}"
        );

        // Opened beside: the second pane, active; the first's place moved
        // into its column.
        ui.js(
            "window.gospelGetterPanes.openBeside({ bookId: arguments[0], chapter: arguments[1], translation: 'web' });
             return null;",
            vec![json!(PSALMS), json!(PSALM)],
        )
        .await?;
        let opened = self
            .panes_are(
                "the second pane to show Psalms 119, active, the page no longer scrolled",
                "seen.split && seen.active === 'second' && seen.heading2 === 'Psalms 119'
                 && seen.second && seen.first && seen.windowY === 0",
            )
            .await?;
        ui.settled("the panes to settle").await?;
        let opened = {
            let now = self.panes().await?;
            if now["first"].is_null() { opened } else { now }
        };
        said.push(Self::same_split_place(
            "opening beside",
            &single["first"],
            &opened["first"],
        )?);

        // Each column scrolls on its own.
        let first_y = opened["first"]["y"].as_f64().unwrap_or(0.0);
        ui.js(
            "document.querySelector('.pane[data-pane=\"second\"]').scrollTop = 2000; return null;",
            Vec::new(),
        )
        .await?;
        let scrolled = self
            .panes_are(
                "the second pane to scroll, the first staying",
                &format!(
                    "seen.second.y > 1900 && Math.abs(seen.first.y - {first_y}) < 1 && seen.windowY === 0"
                ),
            )
            .await?;
        let second_y = scrolled["second"]["y"].as_f64().unwrap_or(0.0);
        ui.js(
            "document.querySelector('.pane[data-pane=\"first\"]').scrollTop += 600; return null;",
            Vec::new(),
        )
        .await?;
        let scrolled = self
            .panes_are(
                "the first pane to scroll, the second staying",
                &format!(
                    "Math.abs(seen.first.y - {first_y} - 600) < 2 && Math.abs(seen.second.y - {second_y}) < 1"
                ),
            )
            .await?;
        said.push(format!(
            "each column scrolls alone (first {} → {}, second {})",
            first_y, scrolled["first"]["y"], scrolled["second"]["y"]
        ));

        // Page Down and Space scroll the active pane only.
        for (pane, other) in [("second", "first"), ("first", "second")] {
            let active = ui
                .js(PRESS_IN_BAR_JS, vec![json!(pane)])
                .await
                .with_context(|| format!("a press in the {pane} pane's bar"))?;
            ensure!(
                active == json!(pane),
                "a press in the {pane} pane's bar left {active} active"
            );
            for (name, key) in [("Page Down", Key::PageDown), ("Space", Key::Space)] {
                let before = self.panes().await?;
                let (from, still) = (
                    before[pane]["y"].as_f64().unwrap_or(0.0),
                    before[other]["y"].as_f64().unwrap_or(0.0),
                );
                let step = before[pane]["view"].as_f64().unwrap_or(0.0) * 0.875;
                ui.press(&format!("press {name} (the {pane} pane active)"), key)
                    .await?;
                let after = self
                    .panes_are(
                        &format!("{name} to scroll the {pane} pane, and not the {other}"),
                        &format!(
                            "Math.abs(seen.{pane}.y - {from} - {step}) < 2
                             && Math.abs(seen.{other}.y - {still}) < 1 && seen.windowY === 0"
                        ),
                    )
                    .await?;
                said.push(format!(
                    "{name}: {pane} {from} → {}, {other} stayed at {still}",
                    after[pane]["y"]
                ));
            }
        }

        // The card, in the second pane's view.
        ui.js(PRESS_IN_BAR_JS, vec![json!("second")]).await?;
        let verse = ui.js(
                    "const col = document.querySelector('.pane[data-pane=\"second\"]');
                     const bar = col.querySelector('.pane-bar').getBoundingClientRect().bottom;
                     const foot = col.getBoundingClientRect().bottom;
                     const middle = (bar + foot) / 2;
                     const verses = [...col.querySelectorAll('.chapter-main .verse[data-translation]')];
                     const v = verses.find((v) => { const r = v.getBoundingClientRect();
                         return r.top > bar + 40 && r.bottom < middle; });
                     return v ? v.dataset.verse : null;",
                    Vec::new(),
                )
                .await?;
        let Some(verse) = verse.as_str().map(str::to_owned) else {
            bail!("no verse of the second pane in its view's upper half");
        };
        ui.click(
            &format!("click verse {verse} in the second pane"),
            &format!("#reading-pane-2 .chapter-main .verse[data-verse=\"{verse}\"]"),
        )
        .await?;
        let card = ui
            .wait(
                "the card to open in the second pane, within its view",
                "const card = document.getElementById('selection-card');
                 const col = document.querySelector('.pane[data-pane=\"second\"]');
                 if (!card || !card.isConnected || !col) return [false, null];
                 const r = card.getBoundingClientRect();
                 const bar = col.querySelector('.pane-bar').getBoundingClientRect().bottom;
                 const c = col.getBoundingClientRect();
                 const seen = { inPane: col.contains(card), top: Math.round(r.top), bottom: Math.round(r.bottom),
                     left: Math.round(r.left), right: Math.round(r.right), bar: Math.round(bar),
                     foot: Math.round(c.bottom), colLeft: Math.round(c.left), colRight: Math.round(c.right) };
                 return [seen.inPane && r.height > 0 && r.top >= bar - 1 && r.bottom <= c.bottom + 1
                     && r.left >= c.left - 1 && r.right <= c.right + 1, seen];",
            )
            .await?;
        said.push(format!("the card in the second pane's view: {card}"));
        ui.press("press Escape (clear the selection)", Key::Escape)
            .await?;
        ui.wait(
            "the selection to clear",
            "return [!document.querySelector('.verse-selected'), null];",
        )
        .await?;

        // Opened beside again, at a verse: in the second pane's view.
        ui.js(
            "window.gospelGetterPanes.openBeside({ bookId: arguments[0], chapter: arguments[1], verse: 100 });
             return null;",
            vec![json!(PSALMS), json!(PSALM)],
        )
        .await?;
        let shown = ui
            .wait(
                "verse 100, selected, in the second pane's view",
                "const v = document.querySelector('#reading-pane-2 .verse[data-verse=\"100\"].verse-selected');
                 const col = document.querySelector('.pane[data-pane=\"second\"]');
                 if (!v || !col) return [false, null];
                 const r = v.getBoundingClientRect();
                 const bar = col.querySelector('.pane-bar').getBoundingClientRect().bottom;
                 const foot = col.getBoundingClientRect().bottom;
                 return [r.top >= bar - 1 && r.bottom <= foot + 1,
                     { top: Math.round(r.top), bottom: Math.round(r.bottom), bar: Math.round(bar), foot: Math.round(foot) }];",
            )
            .await?;
        said.push(format!("verse 100 opened beside, in view: {shown}"));
        ui.press("press Escape (clear the selection)", Key::Escape)
            .await?;
        ui.wait(
            "the selection to clear",
            "return [!document.querySelector('.verse-selected'), null];",
        )
        .await?;

        // Across 1200 px and back.
        match self.ensure_wide().await? {
            Err(why) if self.config.xvfb.is_none() => {
                said.push(format!("the 1200 px crossing not run on Windows: {why}"));
            }
            Err(why) => bail!("{why}"),
            Ok(()) => {
                ui.settled("the panes to settle, wide").await?;
                let wide = self.panes().await?;
                self.window_width(NARROW_WIDTH).await?;
                ui.settled("the panes to settle, narrow").await?;
                let narrow = self.panes().await?;
                for pane in ["first", "second"] {
                    said.push(Self::same_split_place(
                        &format!("{WIDE_WIDTH} → {NARROW_WIDTH} px, the {pane} pane"),
                        &wide[pane],
                        &narrow[pane],
                    )?);
                }
                self.window_width(WIDE_WIDTH).await?;
                ui.settled("the panes to settle, wide again").await?;
                let back = self.panes().await?;
                for pane in ["first", "second"] {
                    said.push(Self::same_split_place(
                        &format!("{NARROW_WIDTH} → {WIDE_WIDTH} px, the {pane} pane"),
                        &narrow[pane],
                        &back[pane],
                    )?);
                }
            }
        }

        // × closes it: the first pane's place back in the window.
        ui.settled("the panes to settle").await?;
        let before = self.panes().await?;
        ui.click(
            "click × (close the second pane)",
            ".pane[data-pane=\"second\"] .pane-close",
        )
        .await?;
        let closed = self
            .panes_are(
                "one pane, its place in the window, focus on its reference button",
                "!seen.split && seen.active === 'first' && seen.first && seen.focus === 'ref-btn'",
            )
            .await?;
        ui.settled("the page to settle").await?;
        let closed = {
            let now = self.panes().await?;
            if now["first"].is_null() { closed } else { now }
        };
        said.push(Self::same_split_place(
            "closing",
            &before["first"],
            &closed["first"],
        )?);
        Ok(said.join("; "))
    }

    /// Check 78.
    pub(super) async fn open_beside(&mut self) -> Result<String> {
        self.plain_reading().await?;
        let width = self.split_window_width().await?;
        let was = self
            .ui
            .js(
                "const inner = document.getElementById('reading-pane-inner');
                 const h = document.querySelector('.chapter-main .chapter-heading');
                 return { book: Number(inner.dataset.currentBook), chapter: Number(inner.dataset.currentChapter),
                          heading: h.textContent.trim() };",
                Vec::new(),
            )
            .await?;
        let checked = self.open_beside_steps().await;
        let heard = self.ui.js(F6_UNHEARD_JS, Vec::new()).await;
        let put_back = self.split_clean_up(width, &was).await;
        let said = checked?;
        heard?;
        put_back?;
        Ok(said)
    }

    /// Presses F6 (`shift`: Shift+F6), and waits for `to` to be the active
    /// pane, focus on its bar's reference button; or, if the page never
    /// heard the key (the webview kept it), says so, and switches through
    /// the palette's Switch pane (F6) instead.
    async fn press_f6(&self, shift: bool, to: &str) -> Result<String> {
        let name = if shift { "Shift+F6" } else { "F6" };
        if shift {
            self.chord(&format!("press {name}"), &[Key::Shift], Key::F6)
                .await?;
        } else {
            self.ui.press(&format!("press {name}"), Key::F6).await?;
        }
        let heard = self
            .ui
            .wait_within(
                &format!("{name} to reach the page"),
                "return [window.__gospelGetterF6.length > 0, window.__gospelGetterF6];",
                Vec::new(),
                Some(std::time::Duration::from_secs(3)),
            )
            .await;
        let switched = format!(
            "{PANES_JS_FOCUS} return [window.gospelGetterPanes.active() === arguments[0]
                 && focusedRef === arguments[0], {{ active: window.gospelGetterPanes.active(), focusedRef }}];"
        );
        match heard {
            Ok(events) => {
                self.ui
                    .js(
                        "window.__gospelGetterF6.length = 0; return null;",
                        Vec::new(),
                    )
                    .await?;
                let seen = self
                    .ui
                    .wait_with(
                        &format!(
                            "{name} to make the {to} pane active, focus on its reference button"
                        ),
                        &switched,
                        vec![json!(to)],
                    )
                    .await?;
                Ok(format!("{name} reached the page ({events}): {seen}"))
            }
            Err(_) => {
                self.open_search_by_key().await?;
                self.type_in_box(">switch pane").await?;
                self.ui
                    .wait(
                        "Switch pane (F6) to be offered",
                        "const o = document.querySelector('#search-results .search-command[data-command=\"switch-pane\"]');
                         return [!!o, o && o.textContent];",
                    )
                    .await?;
                self.ui
                    .press("press Enter (Switch pane)", Key::Enter)
                    .await?;
                let seen = self
                    .ui
                    .wait_with(
                        &format!("Switch pane to make the {to} pane active"),
                        &switched,
                        vec![json!(to)],
                    )
                    .await?;
                Ok(format!(
                    "{name} never reached the page (the webview kept it); the palette's Switch pane did: {seen}"
                ))
            }
        }
    }

    /// Opens Romans 5:8's preview from the card on John 3:16, in the first
    /// pane, by a pointer's clicks: the verse, then its chip.
    async fn preview_romans(&self) -> Result<()> {
        let ui = &self.ui;
        ui.click(
            "click verse 16",
            "#reading-pane .chapter-main .verse[data-translation][data-verse=\"16\"]",
        )
        .await?;
        ui.wait(
            "the card on verse 16, with its cross references",
            "return [!!document.querySelector('#selection-card .xref-citation'), null];",
        )
        .await?;
        ui.step("click the Romans 5:8 chip", async {
            ui.driver()
                .find(By::XPath(
                    "//*[@id='selection-card']//button[contains(@class, 'xref-citation') and normalize-space(.)='Romans 5:8']",
                ))
                .await
                .context("no Romans 5:8 chip on John 3:16")?
                .click()
                .await?;
            Ok(())
        })
        .await?;
        let beside = ui
            .wait(
                "the preview, with Go to and Open beside",
                "const b = document.querySelector('#xref-preview .xref-preview-beside');
                 const r = b && b.getBoundingClientRect();
                 return [!!b && r.width > 0 && b.previousElementSibling.classList.contains('xref-preview-go'),
                     b && { text: b.textContent, name: b.getAttribute('aria-label') }];",
            )
            .await?;
        ensure!(
            beside["text"] == "Open beside" && beside["name"] == "Open Romans 5:8 beside",
            "the preview's Open beside: {beside}"
        );
        Ok(())
    }

    async fn open_beside_steps(&mut self) -> Result<String> {
        let ui = &self.ui;
        let mut said = Vec::new();
        self.go_to("John 3", "John 3").await?;
        self.plain_reading().await?;
        let code = ui
            .js(
                "return document.getElementById('translation-select').value;",
                Vec::new(),
            )
            .await?;
        self.preview_romans().await?;
        ui.click("click Open beside", "#xref-preview .xref-preview-beside")
            .await?;
        let opened = ui
            .wait_with(
                "two panes side by side: John 3, and Romans 5 beside it, active, verse 8 selected",
                "const col = (n) => document.querySelector(`.pane[data-pane=\"${n}\"]`);
                 const rect = (n) => col(n) && !col(n).hidden ? col(n).getBoundingClientRect() : null;
                 const [a, b] = [rect('first'), rect('second')];
                 const heading = (id) => (document.querySelector(`#${id} .chapter-main .chapter-heading`) || {}).textContent || null;
                 const selected = [...document.querySelectorAll('#reading-pane-2 .verse-selected[data-translation]')]
                     .map((v) => [Number(v.dataset.verse), v.dataset.translation]);
                 const seen = { split: !!document.documentElement.dataset.split, active: window.gospelGetterPanes.active(),
                     first: heading('reading-pane'), second: heading('reading-pane-2'), selected,
                     first_box: a && [Math.round(a.left), Math.round(a.right)], second_box: b && [Math.round(b.left), Math.round(b.right)] };
                 return [seen.split && seen.active === 'second' && seen.first === 'John 3' && seen.second === 'Romans 5'
                     && JSON.stringify(selected) === JSON.stringify([[8, arguments[0]]])
                     && !!a && !!b && a.width > 200 && b.width > 200 && a.right <= b.left + 1, seen];",
                vec![code.clone()],
            )
            .await?;
        said.push(format!("Open beside: {opened}"));
        ui.settled("the panes to settle").await?;

        // F6 and Shift+F6. A listener of the check's own, first on the
        // window, says whether the key reached the page at all.
        ui.js(
            "window.__gospelGetterF6 = [];
             window.__gospelGetterF6Listener = (e) => { if (e.key === 'F6') window.__gospelGetterF6.push(e.shiftKey ? 'Shift+F6' : 'F6'); };
             window.addEventListener('keydown', window.__gospelGetterF6Listener, true);
             if (document.activeElement) document.activeElement.blur();
             return null;",
            Vec::new(),
        )
        .await?;
        said.push(self.press_f6(false, "first").await?);
        said.push(self.press_f6(true, "second").await?);

        // × closes it.
        self.ui
            .click(
                "click × (close the second pane)",
                ".pane[data-pane=\"second\"] .pane-close",
            )
            .await?;
        let closed = self
            .panes_are(
                "one pane, focus on its reference button",
                "!seen.split && seen.active === 'first' && seen.focus === 'ref-btn'",
            )
            .await?;
        said.push(format!(
            "× closed it: {}",
            json!({ "split": closed["split"], "focus": closed["focus"] })
        ));
        said.push(self.open_beside_in_reading_mode().await?);
        Ok(said.join("; "))
    }

    /// Check 78's pass in reading mode: R, the preview's Open beside, then
    /// R again.
    async fn open_beside_in_reading_mode(&self) -> Result<String> {
        let ui = &self.ui;
        self.plain_reading().await?;
        ui.js(
            "if (document.activeElement) document.activeElement.blur(); return null;",
            Vec::new(),
        )
        .await?;
        ui.press("press R (reading mode)", "r").await?;
        self.panes_are(
            "reading mode, one pane",
            "seen.reading && !seen.split && seen.first",
        )
        .await?;
        ui.settled("reading mode to settle").await?;
        self.preview_romans().await?;
        ui.settled("the preview to settle").await?;
        let reading = self.panes().await?;
        ensure!(
            reading["reading"] == json!(true) && reading["split"] == json!(false),
            "still one pane in reading mode before Open beside: {reading}"
        );
        ui.click(
            "click Open beside in reading mode",
            "#xref-preview .xref-preview-beside",
        )
        .await?;
        let opened = self
            .panes_are(
                "reading mode left, two panes side by side, Romans 5 beside, active",
                "!seen.reading && seen.split && !seen.tabs && seen.active === 'second' && seen.heading2 === 'Romans 5'
                 && !!seen.first && !!seen.second && seen.first.right <= seen.second.left + 1
                 && seen.first.right - seen.first.left > 200 && seen.second.right - seen.second.left > 200",
            )
            .await?;
        ui.settled("the panes to settle").await?;
        let left = self.panes().await?;
        let kept = Self::same_split_place(
            "reading mode left by Open beside, the first pane",
            &reading["first"],
            &left["first"],
        )?;
        ui.js(
            "if (document.activeElement) document.activeElement.blur(); return null;",
            Vec::new(),
        )
        .await?;
        ui.press("press R (reading mode again)", "r").await?;
        let alone = self
            .panes_are(
                "reading mode, the second pane shown alone",
                "seen.reading && seen.split && seen.active === 'second' && !!seen.second && !seen.first",
            )
            .await?;
        Ok(format!(
            "in reading mode, Open beside left it: {}; {kept}; R showed the second alone: {}",
            json!({ "first": [opened["first"]["left"], opened["first"]["right"]],
                    "second": [opened["second"]["left"], opened["second"]["right"]],
                    "active": opened["active"] }),
            json!({ "reading": alone["reading"], "active": alone["active"] })
        ))
    }

    /// Check 76.
    pub(super) async fn split_tabs(&mut self) -> Result<String> {
        self.plain_reading().await?;
        let width = self.split_window_width().await?;
        let was = self
            .ui
            .js(
                "const inner = document.getElementById('reading-pane-inner');
                 const h = document.querySelector('.chapter-main .chapter-heading');
                 return { book: Number(inner.dataset.currentBook), chapter: Number(inner.dataset.currentChapter),
                          heading: h.textContent.trim() };",
                Vec::new(),
            )
            .await?;
        let checked = self.split_tabs_steps().await;
        let put_back = self.split_clean_up(width, &was).await;
        let said = checked?;
        put_back?;
        Ok(said)
    }

    /// Makes the window `width` wide and waits for the panes to be laid out
    /// for it (`tabs`: as tabs, else side by side) and to settle; returns
    /// their places then.
    async fn split_width(&self, width: u32, tabs: bool) -> Result<Value> {
        self.window_width(width).await?;
        let shape = if tabs { "tabs" } else { "side by side" };
        self.panes_are(
            &format!("the panes {shape} at {width} px"),
            &format!(
                "seen.tabs === {tabs} && window.innerWidth {} 900",
                if tabs { "<" } else { ">=" }
            ),
        )
        .await?;
        self.ui
            .settled(&format!("the panes to settle at {width} px"))
            .await?;
        self.panes().await
    }

    async fn split_tabs_steps(&mut self) -> Result<String> {
        let ui = &self.ui;
        let mut said = Vec::new();
        if self.heading().await? != "Psalms 119" {
            self.read_in_picker(PSALMS, PSALM, "Psalms 119").await?;
        }
        ui.js(
            "window.gospelGetterPanes.openBeside({ bookId: arguments[0], chapter: arguments[1], translation: 'web' });
             return null;",
            vec![json!(PSALMS), json!(PSALM)],
        )
        .await?;
        self.panes_are(
            "the second pane to show Psalms 119, active",
            "seen.split && seen.active === 'second' && seen.heading2 === 'Psalms 119' && seen.second && seen.first",
        )
        .await?;
        self.split_width(SIDE_BY_SIDE_WIDTH, false).await?;
        // Each pane somewhere of its own, well into the psalm, with a
        // verse's top just below its bar. The reading place logic keeps a
        // verse's top where it was (D14): a long verse begun well above the
        // bar, its top kept there, can reflow out of view as the column
        // widens (tabs, reading mode), which would be another verse first
        // under the bar for the reason the logic was made, not a place lost.
        ui.js(
            "for (const [name, y] of [['first', 1500], ['second', 4000]]) {
                 const col = document.querySelector(`.pane[data-pane=\"${name}\"]`);
                 col.scrollTop = y;
                 const bar = col.querySelector('.pane-bar').getBoundingClientRect().bottom;
                 const verse = [...col.querySelectorAll('.chapter-main .verse[data-verse]')]
                     .find((v) => v.getBoundingClientRect().bottom > bar);
                 col.scrollTop += verse.getBoundingClientRect().top - bar - 4;
             }
             return null;",
            Vec::new(),
        )
        .await?;
        self.panes_are(
            "both columns scrolled",
            "seen.first.y > 1400 && seen.second.y > 3900",
        )
        .await?;
        ui.settled("the panes to settle").await?;
        let side = self.panes().await?;

        // Narrower than 900 px: tabs, the second pane alone.
        let tabs = self.split_width(TABS_WIDTH, true).await?;
        ensure!(
            tabs["first"].is_null() && tabs["active"] == json!("second"),
            "as tabs, the second pane should show alone: {tabs}"
        );
        said.push(Self::same_split_place(
            &format!("{SIDE_BY_SIDE_WIDTH} → {TABS_WIDTH} px, the second pane"),
            &side["second"],
            &tabs["second"],
        )?);

        // The first pane's tab, then the second's.
        ui.click("click the first pane's tab", "#pane-tab-first")
            .await?;
        self.panes_are(
            "the first pane shown alone, active",
            "seen.tabs && seen.active === 'first' && seen.first && !seen.second",
        )
        .await?;
        ui.settled("the first pane to settle").await?;
        let first = self.panes().await?;
        said.push(Self::same_split_place(
            "the first pane's tab",
            &side["first"],
            &first["first"],
        )?);
        ui.click("click the second pane's tab", "#pane-tab-second")
            .await?;
        self.panes_are(
            "the second pane shown alone, active",
            "seen.tabs && seen.active === 'second' && seen.second && !seen.first",
        )
        .await?;
        ui.settled("the second pane to settle").await?;
        let second = self.panes().await?;
        said.push(Self::same_split_place(
            "the second pane's tab",
            &tabs["second"],
            &second["second"],
        )?);

        // Wider than 900 px: side by side again.
        let back = self.split_width(SIDE_BY_SIDE_WIDTH, false).await?;
        for (pane, before) in [("first", &first), ("second", &second)] {
            said.push(Self::same_split_place(
                &format!("{TABS_WIDTH} → {SIDE_BY_SIDE_WIDTH} px, the {pane} pane"),
                &before[pane],
                &back[pane],
            )?);
        }

        // Reading mode: the active pane alone, then both again.
        ui.js(
            "if (document.activeElement) document.activeElement.blur(); return null;",
            Vec::new(),
        )
        .await?;
        ui.press("press R (reading mode)", "r").await?;
        self.panes_are(
            "reading mode, the second pane shown alone",
            "seen.reading && !seen.tabs && seen.active === 'second' && seen.second && !seen.first",
        )
        .await?;
        ui.settled("the second pane to settle in reading mode")
            .await?;
        let reading = self.panes().await?;
        said.push(Self::same_split_place(
            "reading mode, the second pane",
            &back["second"],
            &reading["second"],
        )?);
        ui.press("press Escape (leave reading mode)", Key::Escape)
            .await?;
        self.panes_are(
            "reading mode left, both panes shown",
            "!seen.reading && seen.first && seen.second",
        )
        .await?;
        ui.settled("the panes to settle").await?;
        let left = self.panes().await?;
        for (pane, before) in [("first", &back), ("second", &reading)] {
            said.push(Self::same_split_place(
                &format!("reading mode left, the {pane} pane"),
                &before[pane],
                &left[pane],
            )?);
        }
        Ok(said.join("; "))
    }

    /// Puts things back whatever happened: reading mode off, one pane,
    /// nothing selected, the window `width` wide, and the first pane on the
    /// chapter it had (`was`).
    async fn split_clean_up(&mut self, width: u32, was: &Value) -> Result<()> {
        let ui = &self.ui;
        ui.js(
            "if (document.documentElement.dataset.readerMode === 'on') document.getElementById('reader-exit').click();
             return null;",
            Vec::new(),
        )
        .await?;
        ui.wait(
            "reading mode off",
            "return [document.documentElement.dataset.readerMode !== 'on', null];",
        )
        .await?;
        ui.js(
            "if (window.gospelGetterPanes.shown().length > 1) window.gospelGetterPanes.close(); return null;",
            Vec::new(),
        )
        .await?;
        ui.wait(
            "one pane",
            "return [!document.documentElement.dataset.split, null];",
        )
        .await?;
        if self.split_window_width().await? != width {
            self.window_width(width).await?;
        }
        self.plain_reading().await?;
        let heading = was["heading"].as_str().unwrap_or_default();
        if self.heading().await? != heading {
            let book = was["book"].as_u64().unwrap_or(0) as u32;
            let chapter = was["chapter"].as_u64().unwrap_or(0) as u32;
            self.read_in_picker(book, chapter, heading).await?;
        }
        ui.js("window.scrollTo(0, 0); return null;", Vec::new())
            .await?;
        Ok(())
    }
}

/// Which pane's bar's reference button has focus (`focusedRef`: "first",
/// "second" or null), for check 78's F6.
const PANES_JS_FOCUS: &str =
    "const ref = document.activeElement && document.activeElement.closest('.pane-bar .ref-btn');
    const focusedRef = ref ? ref.closest('.pane').dataset.pane : null;";

/// Takes check 78's F6 listener off the window again.
const F6_UNHEARD_JS: &str = "if (window.__gospelGetterF6Listener) {
        window.removeEventListener('keydown', window.__gospelGetterF6Listener, true);
        delete window.__gospelGetterF6Listener;
        delete window.__gospelGetterF6;
    }
    return null;";

/// Check 77's verse in the second pane (Psalm 119 in the WEB), and how far
/// above the pane's bar its top is left.
const RESTART_VERSE: u32 = 40;
const ABOVE_THE_BAR: f64 = 13.0;

/// Scrolls the pane `arguments[0]`'s column so its verse `arguments[1]`
/// (or, if 0, the chapter's tenth verse, or its middle one if it has fewer
/// than twenty) is [`ABOVE_THE_BAR`] above its bar, as far as the column
/// can scroll; returns the verse it aimed at.
const SCROLL_PANE_JS: &str =
    "const col = document.querySelector(`.pane[data-pane=\"${arguments[0]}\"]`);
    const verses = [...col.querySelectorAll('.chapter-main .verse[data-translation]')];
    const verse = arguments[1] ? verses.find((v) => v.dataset.verse === String(arguments[1]))
        : verses[Math.min(9, Math.floor(verses.length / 2))];
    if (!verse) return null;
    const bar = col.querySelector('.pane-bar').getBoundingClientRect().bottom;
    col.scrollTop += verse.getBoundingClientRect().top - (bar - arguments[2]);
    return Number(verse.dataset.verse);";

/// A script returning the two panes' places ([`PANES_JS`]), with the first
/// pane's heading,
/// what this computer keeps for the second pane and for split
/// (`gospel-getter-second-pane`, `gospel-getter-split`), and the first
/// pane's place as the backend last saved it.
fn kept_js() -> String {
    format!(
        "const seen = (() => {{ {PANES_JS} }})();
         seen.heading1 = (document.querySelector('#reading-pane .chapter-heading') || {{}}).textContent || null;
         seen.splitKey = localStorage.getItem('gospel-getter-split');
         const kept = localStorage.getItem('gospel-getter-second-pane');
         seen.kept = kept ? JSON.parse(kept) : null;
         seen.saved = window.gospelGetterPlace.saved();
         return seen;"
    )
}

/// Whether a pane's place (`seen`, as [`PANES_JS`] has it) is the one
/// `want` saw, within a pixel.
const SAME_PLACE_JS: &str =
    "const same = (seen, want) => !!seen && !!want && seen.verse === want.verse
    && Math.abs(seen.top - want.top) <= 1;";

/// Whether `check` is check 77, which GUI_CHECK_REPEAT repeats in sessions
/// of its own ([`split_restart`]), not in the first session's.
pub(super) fn repeats_itself(check: crate::report::Check) -> bool {
    check == crate::report::Check::SplitRestart
}

/// Check 77, in two sessions of its own after check 65 (the app started
/// again on the same data): two panes, each scrolled to a verse, as the app
/// quits; started again, each pane is where it was. Run as many times as
/// GUI_CHECK_REPEAT asks (`77:10`), each run in its own two sessions.
pub(super) async fn split_restart(
    config: &crate::config::Config,
    started: std::time::Instant,
) -> Result<String> {
    let times = crate::repeat::Repeat::times_for(config.repeat, crate::report::Check::SplitRestart);
    if times == 1 {
        return split_restart_once(config, started).await;
    }
    let mut runs = crate::repeat::Runs::default();
    for n in 1..=times {
        let result = split_restart_once(config, started).await;
        let line = runs.add(n, times, result);
        crate::ui::log_line(started, &format!("check 77 split-restart, {line}"));
    }
    runs.verdict()
}

/// One run of check 77: the first session leaves two panes, each at a
/// verse; the second finds them there. Whatever happens, the second pane
/// is closed in each session before it ends, so no other check starts
/// split.
async fn split_restart_once(
    config: &crate::config::Config,
    started: std::time::Instant,
) -> Result<String> {
    let session = |what: &'static str| async move {
        let driver = super::start_session(config, &config.app, started)
            .await
            .with_context(|| format!("no WebDriver session {what}"))?;
        anyhow::Ok(Run {
            config,
            ui: crate::ui::Ui::new(driver, started, config.budget, config.step_timeout),
            shots: super::Shots::default(),
        })
    };
    crate::ui::log_line(
        started,
        "start the app again, to split it and quit (check 77)",
    );
    let mut run = session("to split the panes in").await?;
    let left = run.split_left().await;
    if left.is_err() {
        run.failure_screenshot(crate::report::Check::SplitRestart)
            .await;
        // Nothing left split for the next session to open.
        if let Err(e) = run.split_restart_clean_up().await {
            run.ui
                .log(&format!("putting things back after check 77 failed: {e:#}"));
        }
    }
    super::display::end_session(run, started, "split").await;
    let left = left?;

    crate::ui::log_line(started, "start the app again, split as it quit (check 77)");
    let mut run = session("on the app started again split").await?;
    let back = run.split_reopened(&left).await;
    if back.is_err() {
        run.failure_screenshot(crate::report::Check::SplitRestart)
            .await;
    }
    let put_back = run.split_restart_clean_up().await;
    super::display::end_session(run, started, "split again").await;
    let said = back?;
    put_back?;
    Ok(said)
}

impl Run<'_> {
    /// Check 77's first session: the second pane opened on Psalm 119 (the
    /// WEB), each pane scrolled to a verse, and the app's word that it kept
    /// both places: the second's in local storage, the first's with the
    /// backend. Returns the panes as they were left.
    async fn split_left(&mut self) -> Result<Value> {
        let ui = &self.ui;
        let one = ui
            .wait_within(
                "the page to boot, one pane",
                &format!(
                    "const seen = (() => {{ {} }})();
                     return [!!seen.heading1 && !seen.split && seen.splitKey === null && seen.kept === null, seen];",
                    kept_js()
                ),
                Vec::new(),
                Some(self.config.session_timeout),
            )
            .await?;
        ui.js(
            "window.gospelGetterPanes.openBeside({ bookId: arguments[0], chapter: arguments[1], translation: 'web' });
             return null;",
            vec![json!(PSALMS), json!(PSALM)],
        )
        .await?;
        self.panes_are(
            "the second pane to show Psalms 119",
            "seen.split && seen.heading2 === 'Psalms 119' && seen.first && seen.second",
        )
        .await?;
        ui.settled("the panes to settle").await?;
        let verse = ui
            .js(
                SCROLL_PANE_JS,
                vec![json!("second"), json!(RESTART_VERSE), json!(ABOVE_THE_BAR)],
            )
            .await?;
        ensure!(
            verse == json!(RESTART_VERSE),
            "no verse {RESTART_VERSE} in the second pane: {verse}"
        );
        ui.js(
            SCROLL_PANE_JS,
            vec![json!("first"), json!(0), json!(ABOVE_THE_BAR)],
        )
        .await?;
        ui.settled("the panes to settle, scrolled").await?;
        let left = ui
            .wait(
                "the app to keep both panes' places: the second's on this computer, the first's with the backend",
                &format!(
                    "const seen = (() => {{ {kept} }})();
                     const verse = (place) => place && place.verse !== 'heading' ? Number(place.verse) : undefined;
                     const near = (a, b) => typeof a === 'number' && typeof b === 'number' && Math.abs(a - b) <= 0.5;
                     const second = seen.second, first = seen.first, kept = seen.kept, saved = seen.saved;
                     return [!!second && !!first && second.verse === '{RESTART_VERSE}' && seen.splitKey === 'on'
                         && !!kept && kept.bookId === {PSALMS} && kept.chapter === {PSALM} && kept.translation === 'web'
                         && kept.verse === {RESTART_VERSE} && near(kept.offset, second.top)
                         && !!saved && saved.verse === verse(first)
                         && (saved.verse === undefined || near(saved.offset, first.top)),
                         seen];",
                    kept = kept_js()
                ),
            )
            .await?;
        ui.log(&format!(
            "left split: the first pane at {} verse {} ({} px), the second at Psalms 119 verse {} ({} px); one pane before: {}",
            left["heading1"], left["first"]["verse"], left["first"]["top"], left["second"]["verse"],
            left["second"]["top"], one["heading1"]
        ));
        Ok(left)
    }

    /// Check 77's second session: the panes as the first left them (`left`):
    /// split from the start, the first pane active, each at its verse within
    /// a pixel of where it was, and still there once the page has settled.
    async fn split_reopened(&mut self, left: &Value) -> Result<String> {
        let ready = format!(
            "const seen = (() => {{ {kept} }})();
             {SAME_PLACE_JS}
             const want = arguments[0];
             return [seen.split && seen.active === 'first' && seen.windowY === 0
                 && seen.heading1 === want.heading1 && seen.heading2 === 'Psalms 119'
                 && same(seen.first, want.first) && same(seen.second, want.second), seen];",
            kept = kept_js()
        );
        let seen = self
            .ui
            .wait_within(
                "both panes back where they were, the first active",
                &ready,
                vec![left.clone()],
                Some(self.config.session_timeout),
            )
            .await?;
        self.ui
            .settled("the page to settle after the restart")
            .await?;
        let settled = self
            .ui
            .wait_with(
                "both panes still where they were, the page settled",
                &ready,
                vec![left.clone()],
            )
            .await?;
        ensure!(
            settled["splitKey"] == "on" && settled["kept"]["verse"] == json!(RESTART_VERSE),
            "what's kept for the next launch changed: {settled}"
        );
        let at = |seen: &Value, pane: &str| {
            format!(
                "verse {} {} px below its bar",
                seen[pane]["verse"], seen[pane]["top"]
            )
        };
        Ok(format!(
            "quit with two panes, started again: split from the start, the first pane active, {} at {} \
             (left {}), the second at Psalms 119 (WEB) {} (left {}); settled: {}, {}; then closed",
            seen["heading1"].as_str().unwrap_or("?"),
            at(&seen, "first"),
            at(left, "first"),
            at(&seen, "second"),
            at(left, "second"),
            at(&settled, "first"),
            at(&settled, "second"),
        ))
    }

    /// Check 77's clean-up, in each of its sessions: the second pane closed,
    /// and with it what this computer keeps for it.
    async fn split_restart_clean_up(&mut self) -> Result<()> {
        let ui = &self.ui;
        ui.js(
            "if (window.gospelGetterPanes && window.gospelGetterPanes.shown().length > 1) window.gospelGetterPanes.close();
             return null;",
            Vec::new(),
        )
        .await?;
        ui.wait(
            "one pane, and nothing kept to split the next launch",
            "return [!document.documentElement.dataset.split && localStorage.getItem('gospel-getter-split') === null
                 && localStorage.getItem('gospel-getter-second-pane') === null, null];",
        )
        .await?;
        Ok(())
    }
}
