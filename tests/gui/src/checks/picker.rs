//! Checks 20 to 24: the book and chapter picker, whose books and chapters
//! the Rust pieces (ui-leptos) took over from the page's JavaScript, each
//! through the real UI. On a narrow window it's a popover: G opens it, focus
//! on the chapter being read, and choosing another book and chapter reads
//! it and closes the popover (20). On a wide window it's the sidebar, shown
//! from the start, and stays as a chapter is chosen in it (21). Random
//! chapter reads a chapter at its top (22). The keyboard moves through the
//! chapters and the books as it always did, and Enter reads the chapter
//! (23). Hiding and showing the sidebar keeps the reader's place: the first
//! verse under the bar stays the same, within a line (24, at Aa's Wide
//! width, where the chapter's column is narrower beside the sidebar, so the
//! text reflows). Screenshots in Chi Rho here, and in Vellum with check
//! 4's.

use anyhow::{Result, bail};
use serde_json::{Value, json};
use thirtyfour::prelude::*;

use super::{DESCRIBE_JS, Run, VISIBLE_JS};

/// The picker's screenshots: the popover and the sidebar in Chi Rho
/// (checks 20 and 21), and in Vellum (with check 4's).
pub(super) const PICKER_SHOTS: [&str; 4] = [
    "20-picker-popover-chi-rho.png",
    "21-picker-sidebar-chi-rho.png",
    "20-picker-popover-vellum.png",
    "21-picker-sidebar-vellum.png",
];

/// A window narrower than the sidebar's breakpoint (1200 px).
const NARROW_WIDTH: u32 = 1000;

/// Books, by the ids the backend gives them.
const PSALMS: u32 = 19;
const JOHN: u32 = 43;
const ROMANS: u32 = 45;
const MARK: u32 = 41;

/// A description of where focus is, in the picker's terms where it's
/// there: "chapter 3 (John chapters)", "book Luke", else as `describe`.
/// Prepended, with [`DESCRIBE_JS`].
const FOCUS_JS: &str = "const focusOf = (el) => {
        if (!el) return null;
        if (el.classList.contains('chapter-btn')) {
            const grid = el.closest('.chapter-grid');
            return `chapter ${el.dataset.chapter} (${grid ? grid.getAttribute('aria-label') : '?'})`;
        }
        if (el.classList.contains('book-item')) return `book ${el.textContent.trim()}`;
        return describe(el);
    };";

/// What the reader sees of the picker and the chapter: its shape (the
/// sidebar or the popover), whether it shows, its buttons' `aria-expanded`,
/// the book whose chapters show, the marks, the heading, the scroll and the
/// focus.
const PICKER_JS: &str = "const picker = document.getElementById('picker');
    const open = picker.querySelector('.book-item[aria-expanded=\"true\"]');
    const grid = document.getElementById('picker-chapters');
    const book = picker.querySelector('.book-item[aria-current=\"true\"]');
    const chapter = picker.querySelector('.chapter-btn[aria-current=\"page\"]');
    const heading = document.querySelector('.chapter-main .chapter-heading');
    return { shown: !picker.hidden && visible(picker),
             role: picker.getAttribute('role'),
             shape: document.documentElement.dataset.picker || null,
             wide: 'wide' in document.documentElement.dataset,
             refExpanded: document.getElementById('ref-btn').getAttribute('aria-expanded'),
             toggleExpanded: document.getElementById('sidebar-toggle').getAttribute('aria-expanded'),
             books: picker.querySelectorAll('.book-item').length,
             open: open ? open.textContent.trim() : null,
             grid: grid ? grid.getAttribute('aria-label') : null,
             compact: !!grid && grid.classList.contains('is-compact'),
             marked: [book ? book.textContent.trim() : null, chapter ? chapter.dataset.chapter : null],
             heading: heading ? heading.textContent.trim() : null,
             scrollY: Math.round(window.scrollY),
             focus: focusOf(document.activeElement) };";

/// The first heading or verse of the chapter that reaches below the bar
/// (as the head script's keepReadingPlace finds it): which, where it is on
/// screen, and a line's height of its text.
const PLACE_JS: &str = "const bar = document.getElementById('top-bar').getBoundingClientRect().bottom;
    const marks = document.querySelectorAll('.chapter-main .chapter-heading, .chapter-main .verse[data-verse], .chapter-main .compare-gap');
    let first = null;
    for (const mark of marks) {
        if (mark.getBoundingClientRect().bottom > bar) { first = mark; break; }
    }
    if (!first) return null;
    const column = document.querySelector('.chapter-main').getBoundingClientRect();
    return { verse: first.dataset.verse || 'heading',
             column: Math.round(column.width), left: Math.round(column.left),
             top: Math.round(first.getBoundingClientRect().top * 10) / 10,
             line: parseFloat(getComputedStyle(first).lineHeight) || 0,
             scrollY: Math.round(window.scrollY),
             sidebar: document.documentElement.dataset.picker === 'sidebar' };";

/// The selector for a book's button in the picker.
fn book(id: u32) -> String {
    format!("#picker .book-item[data-book-id=\"{id}\"]")
}

/// The selector for a chapter in the chapters that show.
fn chapter(n: u32) -> String {
    format!("#picker-chapters .chapter-btn[data-chapter=\"{n}\"]")
}

impl Run<'_> {
    /// The picker and the chapter as the reader sees them.
    async fn picker_state(&self) -> Result<Value> {
        self.ui
            .js(
                &format!("{VISIBLE_JS} {DESCRIBE_JS} {FOCUS_JS} {PICKER_JS}"),
                Vec::new(),
            )
            .await
    }

    /// Waits until `ready` (a JavaScript expression over `seen`, the picker
    /// as [`PICKER_JS`] sees it) is true, and returns what it saw.
    async fn picker_is(&self, what: &str, ready: &str) -> Result<Value> {
        self.ui
            .wait(
                what,
                &format!(
                    "{VISIBLE_JS} {DESCRIBE_JS} {FOCUS_JS}
                     const seen = (() => {{ {PICKER_JS} }})();
                     return [!!({ready}), seen];"
                ),
            )
            .await
    }

    /// Focus off any control, at the top of the chapter, as a reader's
    /// would be before a key.
    async fn focus_off_controls(&self) -> Result<()> {
        let ui = &self.ui;
        ui.js("window.scrollTo(0, 0); return null;", Vec::new())
            .await?;
        ui.click(
            "click the chapter heading (focus off any control)",
            ".chapter-main .chapter-heading",
        )
        .await
    }

    /// The window made `width` wide (its height kept), until the page sees
    /// it; returns the width it had.
    async fn window_width(&self, width: u32) -> Result<u32> {
        let rect = self.ui.driver().get_window_rect().await?;
        self.ui
            .driver()
            .set_window_rect(rect.x, rect.y, width, rect.height as u32)
            .await?;
        self.ui
            .wait_with(
                &format!("the window to be {width} px wide"),
                "return [Math.abs(window.outerWidth - arguments[0]) <= 2, window.outerWidth];",
                vec![json!(width)],
            )
            .await?;
        Ok(rect.width as u32)
    }

    /// Back to the window's own width, with the sidebar shown in it.
    async fn wide_again(&self, width: u32) -> Result<()> {
        self.window_width(width).await?;
        self.picker_is(
            "the sidebar to show again",
            "seen.wide && seen.shape === 'sidebar' && seen.shown",
        )
        .await?;
        Ok(())
    }

    /// Reads `book_id`'s chapter `n` through the sidebar (its chapters shown
    /// first, if they aren't), and waits until it's read: `heading`.
    async fn read_in_sidebar(&self, book_id: u32, n: u32, heading: &str) -> Result<Value> {
        let ui = &self.ui;
        let open = ui
            .js(
                "const b = document.querySelector(arguments[0]);
                 return !!b && b.getAttribute('aria-expanded') === 'true';",
                vec![json!(book(book_id))],
            )
            .await?;
        if open != Value::Bool(true) {
            ui.click(&format!("click the book {book_id}"), &book(book_id))
                .await?;
        }
        ui.click(&format!("click chapter {n}"), &chapter(n)).await?;
        self.picker_is(
            &format!("{heading} to be read, marked in the sidebar, which stays"),
            &format!(
                "seen.heading === '{heading}' && seen.shown && seen.shape === 'sidebar'
                 && seen.marked[1] === '{n}' && '{heading}'.startsWith(seen.marked[0] + ' ')"
            ),
        )
        .await
    }

    /// Check 20: the popover, on a narrow window.
    pub(super) async fn picker_popover(&mut self) -> Result<String> {
        self.plain_reading().await?;
        self.expect_theme("chi-rho").await?;
        let width = self.window_width(NARROW_WIDTH).await?;
        let checked = self.picker_popover_steps().await;
        // The window goes back to its width whatever happened.
        let restored = self.wide_again(width).await;
        let said = checked?;
        restored?;
        Ok(said)
    }

    async fn picker_popover_steps(&mut self) -> Result<String> {
        let mut said = Vec::new();
        let start = self
            .picker_is(
                "the popover shape: no sidebar, the picker hidden",
                "!seen.wide && !seen.shape && !seen.shown && seen.refExpanded === 'false'",
            )
            .await?;
        let here = start["heading"].as_str().unwrap_or_default().to_owned();
        self.focus_off_controls().await?;
        self.ui.press("press G (open the picker)", "g").await?;
        let opened = self
            .picker_is(
                "the popover to open, focus on the chapter being read",
                "seen.shown && seen.role === 'dialog' && seen.refExpanded === 'true'
                 && seen.heading === `${seen.marked[0]} ${seen.marked[1]}`
                 && seen.focus === `chapter ${seen.marked[1]} (${seen.marked[0]} chapters)`",
            )
            .await?;
        said.push(format!(
            "{NARROW_WIDTH} px wide, reading {here}: G opens the popover (role dialog, #ref-btn expanded), focus on {}",
            opened["focus"].as_str().unwrap_or("?")
        ));
        self.shot(
            "screenshot: the picker as a popover, Chi Rho",
            PICKER_SHOTS[0],
        )
        .await;

        let (book_id, n, name) = if here.starts_with("Romans ") {
            (MARK, 4, "Mark")
        } else {
            (ROMANS, 8, "Romans")
        };
        self.ui
            .click(&format!("click {name} in the popover"), &book(book_id))
            .await?;
        self.picker_is(
            &format!("{name}'s chapters to show, the popover staying open"),
            &format!("seen.shown && seen.grid === '{name} chapters'"),
        )
        .await?;
        self.ui
            .js("window.scrollTo(0, 200); return null;", Vec::new())
            .await?;
        self.ui
            .click(&format!("click chapter {n}"), &chapter(n))
            .await?;
        let read = self
            .picker_is(
                &format!("{name} {n} to be read at its top, the popover closed, focus on the reference button"),
                &format!(
                    "seen.heading === '{name} {n}' && !seen.shown && seen.refExpanded === 'false'
                     && seen.focus === '#ref-btn' && seen.scrollY === 0
                     && seen.marked.join(' ') === '{name} {n}'"
                ),
            )
            .await?;
        said.push(format!(
            "{name} shows its chapters; chapter {n}: {} read from its top, the popover closed, focus back on {}",
            read["heading"].as_str().unwrap_or("?"),
            read["focus"].as_str().unwrap_or("?")
        ));
        Ok(said.join("; "))
    }

    /// Check 21: the sidebar, on a wide window.
    pub(super) async fn picker_sidebar(&mut self) -> Result<String> {
        self.plain_reading().await?;
        let start = self
            .picker_is(
                "the sidebar to show: a landmark, not a dialog, with every book",
                "seen.wide && seen.shape === 'sidebar' && seen.shown && seen.role === null
                 && seen.toggleExpanded === 'true' && seen.books === 66",
            )
            .await?;
        let here = start["heading"].as_str().unwrap_or_default().to_owned();
        let n = if here == "Psalms 119" { 118 } else { 119 };
        self.ui
            .click("click Psalms in the sidebar", &book(PSALMS))
            .await?;
        let psalms = self
            .picker_is(
                "Psalms' compact grid to show",
                "seen.grid === 'Psalms chapters' && seen.compact && seen.shown",
            )
            .await?;
        let read = self
            .read_in_sidebar(PSALMS, n, &format!("Psalms {n}"))
            .await?;
        self.shot(
            "screenshot: the picker as the sidebar, Chi Rho",
            PICKER_SHOTS[1],
        )
        .await;
        Ok(format!(
            "reading {here}: the sidebar shows (no role, #sidebar-toggle expanded, {} books); \
             Psalms shows its compact grid ({}); chapter {n}: {} read, the sidebar stays, marked {}",
            start["books"],
            psalms["grid"].as_str().unwrap_or("?"),
            read["heading"].as_str().unwrap_or("?"),
            read["marked"]
        ))
    }

    /// Check 22: Random chapter reads a chapter at its top.
    pub(super) async fn random_chapter(&mut self) -> Result<String> {
        self.plain_reading().await?;
        self.picker_is(
            "the sidebar to show",
            "seen.shape === 'sidebar' && seen.shown",
        )
        .await?;
        // A chapter longer than the window, scrolled down.
        let here = self.heading().await?;
        if here != "Psalms 119" {
            self.read_in_sidebar(PSALMS, 119, "Psalms 119").await?;
        }
        // Random may pick the chapter on screen (1 in 1,189), which then
        // stays where it is: another try.
        for attempt in 1..=3 {
            let down = self
                .ui
                .wait(
                    "the page to be scrolled down",
                    "window.scrollTo(0, 1500); return [window.scrollY > 1000, Math.round(window.scrollY)];",
                )
                .await?;
            self.ui
                .click("click Random chapter", "#random-chapter-btn")
                .await?;
            let read = self
                .picker_is(
                    "another chapter to be read at its top, marked, the sidebar staying",
                    "seen.heading !== 'Psalms 119' && seen.heading === `${seen.marked[0]} ${seen.marked[1]}`
                     && seen.scrollY === 0 && seen.shown",
                )
                .await;
            let read = match read {
                Ok(read) => read,
                Err(e) if self.heading().await? == "Psalms 119" && attempt < 3 => {
                    self.ui.log(&format!(
                        "  Random chapter read the chapter on screen, or nothing (try {attempt}): {e:#}"
                    ));
                    continue;
                }
                Err(e) => return Err(e),
            };
            return Ok(format!(
                "Psalms 119 scrolled to {down} px; Random chapter: {} read, at its top (scrollY {}), \
                 marked {} in the sidebar, which stays",
                read["heading"].as_str().unwrap_or("?"),
                read["scrollY"],
                read["marked"]
            ));
        }
        bail!("Random chapter picked the chapter on screen three times")
    }

    /// Check 23: the keyboard through the chapters and the books, then
    /// Enter.
    pub(super) async fn picker_keyboard(&mut self) -> Result<String> {
        self.plain_reading().await?;
        if self.heading().await? != "John 3" {
            self.read_in_sidebar(JOHN, 3, "John 3").await?;
        }
        self.focus_off_controls().await?;
        self.ui.press("press G (into the sidebar)", "g").await?;
        self.picker_is(
            "focus on John 3 in the sidebar",
            "seen.shown && seen.shape === 'sidebar' && seen.focus === 'chapter 3 (John chapters)'",
        )
        .await?;
        let moves: [(Key, &str, &str); 8] = [
            (Key::Right, "→", "chapter 4 (John chapters)"),
            (Key::Down, "↓", "chapter 10 (John chapters)"),
            (Key::Up, "↑", "chapter 4 (John chapters)"),
            (Key::Up, "↑", "book John"),
            (Key::Up, "↑", "book Luke"),
            (Key::Right, "→", "book Luke"),
            (Key::Right, "→", "chapter 1 (Luke chapters)"),
            (Key::Down, "↓", "chapter 7 (Luke chapters)"),
        ];
        let mut path = vec!["chapter 3 (John chapters)".to_owned()];
        for (key, arrow, focus) in moves {
            self.ui.press(&format!("press {arrow}"), key).await?;
            let ready = if focus == "book Luke" && path.last().is_some_and(|f| f == "book Luke") {
                // → on a book shows its chapters, focus staying on it.
                format!("seen.focus === '{focus}' && seen.grid === 'Luke chapters'")
            } else {
                format!("seen.focus === '{focus}'")
            };
            self.picker_is(&format!("focus on {focus}"), &ready).await?;
            path.push(format!("{arrow} {focus}"));
        }
        let unturned = self.picker_state().await?;
        if unturned["heading"] != json!("John 3") {
            bail!("an arrow turned the chapter: {}", unturned["heading"]);
        }
        self.ui.press("press Enter (read it)", Key::Enter).await?;
        let read = self
            .picker_is(
                "Luke 7 to be read, the sidebar staying, focus on its chapter",
                "seen.heading === 'Luke 7' && seen.shown && seen.marked.join(' ') === 'Luke 7'
                 && seen.focus === 'chapter 7 (Luke chapters)'",
            )
            .await?;
        Ok(format!(
            "G: focus on John 3; {}; the chapter stayed John 3; Enter: {} read, focus still on {}",
            path[1..].join(", "),
            read["heading"].as_str().unwrap_or("?"),
            read["focus"].as_str().unwrap_or("?")
        ))
    }

    /// Where the reader is in the chapter: the first verse under the bar.
    async fn reading_place(&self, what: &str) -> Result<Value> {
        let place = self
            .ui
            .step(what, async { self.ui.js(PLACE_JS, Vec::new()).await })
            .await?;
        if place.is_null() {
            bail!("nothing of the chapter under the bar");
        }
        self.ui.log(&format!("  place: {place}"));
        Ok(place)
    }

    /// Chooses Aa's Width `value` ("normal" or "wide"), and closes Aa.
    async fn reading_width(&self, value: &str) -> Result<()> {
        let ui = &self.ui;
        ui.click("open Aa", "#aa-toggle").await?;
        ui.wait(
            "the Aa panel to show",
            &format!("{VISIBLE_JS} return [visible(document.getElementById('aa-popover')), null];"),
        )
        .await?;
        ui.click(
            &format!("choose the {value} width"),
            &format!("#width-choices [data-value=\"{value}\"]"),
        )
        .await?;
        ui.wait_with(
            &format!("the {value} width to be on"),
            "const now = document.documentElement.dataset.width || 'normal';
             return [now === arguments[0], now];",
            vec![json!(value)],
        )
        .await?;
        ui.press("press Escape (close Aa)", Key::Escape).await?;
        ui.wait(
            "the Aa panel to close",
            "return [document.getElementById('aa-popover').hidden, null];",
        )
        .await?;
        Ok(())
    }

    /// Whether `after` is the place `before` was, within a line.
    fn same_place(before: &Value, after: &Value) -> Result<f64> {
        let shift = (after["top"].as_f64().unwrap_or(f64::NAN)
            - before["top"].as_f64().unwrap_or(f64::NAN))
        .abs();
        let line = before["line"].as_f64().unwrap_or(0.0);
        if before["verse"] != after["verse"] || shift.is_nan() || shift > line {
            bail!("the reading place moved: before {before}, after {after} (a line is {line} px)");
        }
        Ok(shift)
    }

    /// Check 24: hiding and showing the sidebar keeps the reading place,
    /// at the Wide width (Normal is put back whatever happens).
    pub(super) async fn sidebar_reading_place(&mut self) -> Result<String> {
        self.plain_reading().await?;
        self.reading_width("wide").await?;
        let checked = self.sidebar_reading_place_steps().await;
        let normal = self.reading_width("normal").await;
        let said = checked?;
        normal?;
        Ok(said)
    }

    async fn sidebar_reading_place_steps(&mut self) -> Result<String> {
        self.picker_is(
            "the sidebar to show",
            "seen.shape === 'sidebar' && seen.shown && seen.toggleExpanded === 'true'",
        )
        .await?;
        if self.heading().await? != "Psalms 119" {
            self.read_in_sidebar(PSALMS, 119, "Psalms 119").await?;
        }
        self.ui
            .wait(
                "the page to be scrolled into Psalms 119",
                "window.scrollTo(0, 3000); return [window.scrollY > 2500, Math.round(window.scrollY)];",
            )
            .await?;
        let before = self
            .reading_place("the first verse under the bar, with the sidebar")
            .await?;

        self.ui
            .click("click the sidebar's button (hide it)", "#sidebar-toggle")
            .await?;
        self.picker_is(
            "the sidebar to hide, its button saying so",
            "!seen.shown && !seen.shape && seen.toggleExpanded === 'false'",
        )
        .await?;
        self.ui
            .settled("the page to settle without the sidebar")
            .await?;
        let hidden = self
            .reading_place("the first verse under the bar, without the sidebar")
            .await?;
        // Without a reflow there is no place to keep, and nothing to check.
        let widths = (before["column"].as_f64(), hidden["column"].as_f64());
        let (Some(beside), Some(alone)) = widths else {
            bail!("the chapter's column wasn't measured: {before}, {hidden}");
        };
        if (alone - beside).abs() < 20.0 {
            bail!(
                "hiding the sidebar didn't reflow the chapter (its column {beside} px, then {alone} px): \
                 nothing to keep"
            );
        }
        let first = Self::same_place(&before, &hidden)?;
        let stored = self
            .ui
            .js(
                "try { return localStorage.getItem('gospel-getter-sidebar'); } catch (e) { return String(e); }",
                Vec::new(),
            )
            .await?;

        self.ui
            .click("click the sidebar's button (show it)", "#sidebar-toggle")
            .await?;
        self.picker_is(
            "the sidebar to show again",
            "seen.shown && seen.shape === 'sidebar' && seen.toggleExpanded === 'true'",
        )
        .await?;
        self.ui
            .settled("the page to settle with the sidebar")
            .await?;
        let shown = self
            .reading_place("the first verse under the bar, with the sidebar again")
            .await?;
        let second = Self::same_place(&hidden, &shown)?;
        Ok(format!(
            "Wide width, Psalms 119 at scrollY {}: verse {} first under the bar at {} px, the column {beside} px; \
             hidden (remembered: {stored}): the column {alone} px, verse {} at {} px (moved {first:.1} px, \
             scrollY {}); shown: verse {} at {} px (moved {second:.1} px, scrollY {}); a line is {} px",
            before["scrollY"],
            before["verse"],
            before["top"],
            hidden["verse"],
            hidden["top"],
            hidden["scrollY"],
            shown["verse"],
            shown["top"],
            shown["scrollY"],
            before["line"]
        ))
    }

    /// Check 4's Vellum screenshots of the picker: the sidebar, then the
    /// popover on a narrow window.
    pub(super) async fn picker_vellum_screenshots(&mut self) -> Result<()> {
        self.plain_reading().await?;
        self.picker_is(
            "the sidebar to show",
            "seen.shape === 'sidebar' && seen.shown",
        )
        .await?;
        self.ui
            .js("window.scrollTo(0, 0); return null;", Vec::new())
            .await?;
        self.shot(
            "screenshot: the picker as the sidebar, Vellum",
            PICKER_SHOTS[3],
        )
        .await;
        let width = self.window_width(NARROW_WIDTH).await?;
        let shot = async {
            self.picker_is("the popover shape", "!seen.wide && !seen.shown")
                .await?;
            self.focus_off_controls().await?;
            self.ui.press("press G (open the picker)", "g").await?;
            self.picker_is(
                "the popover to open",
                "seen.shown && seen.role === 'dialog'",
            )
            .await?;
            self.shot(
                "screenshot: the picker as a popover, Vellum",
                PICKER_SHOTS[2],
            )
            .await;
            self.ui
                .press("press Escape (close the picker)", Key::Escape)
                .await?;
            self.picker_is("the popover to close", "!seen.shown")
                .await?;
            Ok::<_, anyhow::Error>(())
        }
        .await;
        let restored = self.wide_again(width).await;
        shot?;
        restored
    }
}
