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
/// the first heading or verse below its bar and how far below it that
/// starts, a line's height, the band in view's height and how far the
/// column can scroll; and the window's scroll, whether two panes are open
/// and which is active. With one pane, the first pane's place is in the
/// window, below the top bar.
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
        if (!col) return null;
        const bar = col.querySelector('.pane-bar').getBoundingClientRect().bottom;
        const rect = col.getBoundingClientRect();
        return place(col, bar, col.scrollTop, { view: Math.round(rect.top + col.clientHeight - bar),
            max: col.scrollHeight - col.clientHeight, bar: Math.round(bar), foot: Math.round(rect.bottom),
            left: Math.round(rect.left), right: Math.round(rect.right) });
    };
    const split = !!document.documentElement.dataset.split;
    const topBar = document.getElementById('top-bar').getBoundingClientRect().bottom;
    return { split, active: window.gospelGetterPanes.active(), windowY: window.scrollY,
        first: split ? column('first') : place(document.getElementById('reading-pane'), topBar, window.scrollY, {}),
        second: column('second'),
        heading2: (document.querySelector('#reading-pane-2 .chapter-heading') || {}).textContent || null,
        focus: document.activeElement ? (document.activeElement.id || document.activeElement.tagName) : null };";

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

    /// Puts things back whatever happened: one pane, nothing selected, the
    /// window `width` wide, and the first pane on the chapter it had
    /// (`was`).
    async fn split_clean_up(&mut self, width: u32, was: &Value) -> Result<()> {
        let ui = &self.ui;
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
