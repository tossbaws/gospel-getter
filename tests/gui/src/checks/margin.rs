//! Checks 60 to 63: margin notes in reading mode (ui-leptos/src/margin.rs),
//! on notes added through the page's own IPC, then read in John 3 (a range,
//! a note on the same verse that would overlap it, a long note, a short
//! one) and Psalm 119 (a note on every verse). In reading mode with the
//! Narrow width, in the app's default 1280 px window:
//!
//! - 60: the margin is a landmark beside the column, never over it, at
//!   least 1.125rem from the window's edge (its scroll bar left out); each
//!   note's top is its verse's (within 2 px) unless it would overlap the
//!   note above, when it's just below it; no two notes overlap; the range's
//!   rule runs beside its verses; the long note is folded with "more"; in
//!   Verses and Flowing (on the verse number), in Chi Rho and Vellum
//!   (screenshots of each). Then the same in the Normal width, at the
//!   default text size, in a 1280x900 window (the app's default): the
//!   margin, not markers (screenshot), unless the screen can't hold a
//!   window that wide.
//! - 61: Aa's "Notes in the margin" off and on again leaves the text
//!   column where it was, to the pixel, and the margin goes and comes.
//! - 62: in a window too narrow for the margin, a marker after each noted
//!   verse, and a marker opens the card with its note (screenshot).
//! - 63: Psalm 119 with 176 notes is placed in under 16 ms (the margin's
//!   own timing, `window.gospelGetterMargin.layoutMs()`), the number in
//!   results.txt.
//!
//! A screen too narrow for a 1280 px window (the Windows runner's) has no
//! room for the margin: 60, 61 and 63 say so and check nothing else there;
//! 62 runs everywhere. The notes are deleted after, and the settings put
//! back as they were found, for the checks that follow.

use anyhow::{Result, bail, ensure};
use serde_json::{Value, json};
use thirtyfour::prelude::*;

use super::Run;

/// The screenshots checks 60 and 62 take (check 4 lists them).
pub(super) const MARGIN_SHOTS: [&str; 6] = [
    "60-margin-verses-chi-rho.png",
    "60-margin-flowing-chi-rho.png",
    "60-margin-verses-vellum.png",
    "60-margin-flowing-vellum.png",
    "60-margin-normal-chi-rho.png",
    "62-margin-markers-chi-rho.png",
];

/// The window the margin is checked in: the app's default.
const MARGIN_WINDOW: u32 = 1280;
/// The app's default window's height, for the Normal width's check.
const MARGIN_HEIGHT: u32 = 900;
/// The room kept between the margin and the window's edge, in rem
/// (`margin_text::EDGE_REM`).
const EDGE_REM: f64 = 1.125;
/// A window too narrow for it.
const NARROW_WINDOW: u32 = 1000;

/// John 3's notes: a range, one on its first verse (which would overlap
/// it), a long one and a short one.
const JOHN_NOTES: [(u32, u32, &str); 4] = [
    (
        16,
        18,
        "For God so loved the world: the whole of it, this range of three verses.",
    ),
    (16, 16, "Whosoever: no one left out."),
    (
        20,
        20,
        "A long note.\nLine two.\nLine three.\nLine four.\nLine five.\nLine six.\nLine seven.\nLine eight.\nLine nine.",
    ),
    (5, 5, "Water and the Spirit."),
];

/// Starts adding the notes `arguments[0]` ([book, chapter, start, end,
/// body] each) through the page's own IPC, in order, keeping how it went in
/// `window.__guiCheckNotes` for [`NOTES_DONE_JS`]: never a WebDriver call
/// waiting on the app (as every other IPC call of the checks).
const ADD_NOTES_JS: &str =
    "const s = window.__guiCheckNotes = { done: false, ids: [], error: null };
    (async () => {
        for (const [bookId, chapter, verseStart, verseEnd, body] of arguments[0]) {
            const note = await window.__TAURI__.core.invoke('add_note',
                { bookId, chapter, verseStart, verseEnd, body });
            s.ids.push(note.id);
        }
    })().then(() => { s.done = true; }, (e) => { s.error = String(e); s.done = true; });
    return null;";

/// Starts deleting the notes `arguments[0]` (ids) through the page's own
/// IPC, as [`ADD_NOTES_JS`] adds them.
const DELETE_NOTES_JS: &str =
    "const s = window.__guiCheckNotes = { done: false, ids: [], error: null };
    (async () => {
        for (const id of arguments[0]) {
            await window.__TAURI__.core.invoke('delete_note', { id });
            s.ids.push(id);
        }
    })().then(() => { s.done = true; }, (e) => { s.error = String(e); s.done = true; });
    return null;";

/// Whether the notes' adding or deleting has finished, and how it went.
const NOTES_DONE_JS: &str = "const s = window.__guiCheckNotes;
    return [!!s && s.done, s ? { done: s.done, count: s.ids.length, error: s.error, ids: s.done ? s.ids : null } : null];";

/// Starts counting two animation frames, kept in
/// `window.__guiCheckFrames`.
const TWO_FRAMES_JS: &str = "const s = window.__guiCheckFrames = { done: false };
    requestAnimationFrame(() => requestAnimationFrame(() => { s.done = true; }));
    return null;";

/// How long adding or deleting Psalm 119's 176 notes may take.
const NOTES_LIMIT: std::time::Duration = std::time::Duration::from_secs(60);

/// The margin as laid out: its box and the column's, each note's box with
/// its verse's (the verse number's in Flowing), the range's rule against
/// its verses, the folded notes and their "more". `arguments[0]`: the
/// notes added ([id, start, end] each); `arguments[1]`: the translation.
const MARGIN_JS: &str = "const [added, code] = arguments;
    const shown = window.gospelGetterMargin ? window.gospelGetterMargin.shown() : null;
    const aside = document.querySelector('.chapter-main .margin-notes');
    const column = document.querySelector('.chapter-main').getBoundingClientRect();
    const flowing = document.documentElement.dataset.layout === 'flowing';
    const verse = (n) => document.querySelector(
        `.chapter-main .chapter-text > .verse[data-verse=\"${n}\"][data-translation=\"${code}\"]`);
    const anchorOf = (n) => { const v = verse(n); return v && flowing ? v.querySelector('.verse-num') : v; };
    const notes = aside ? [...aside.querySelectorAll('.margin-note')].map((el) => {
        const id = Number(el.dataset.noteId);
        const [, start, end] = added.find(([i]) => i === id) || [];
        const box = el.getBoundingClientRect();
        const anchor = start ? anchorOf(start) : null;
        const more = el.querySelector('.margin-note-more');
        return { id, start, end, top: box.top, bottom: box.bottom, left: box.left, right: box.right,
                 anchor: anchor ? anchor.getBoundingClientRect().top : null,
                 folded: el.classList.contains('is-folded'), more: !!more && !more.hidden,
                 label: el.getAttribute('aria-label') };
    }) : [];
    const rules = aside ? [...aside.querySelectorAll('.margin-rule')].map((el) => {
        const id = Number(el.dataset.noteId);
        const [, start, end] = added.find(([i]) => i === id) || [];
        const box = el.getBoundingClientRect();
        return { id, top: box.top, bottom: box.bottom,
                 from: verse(start) ? verse(start).getBoundingClientRect().top : null,
                 to: verse(end) ? verse(end).getBoundingClientRect().bottom : null };
    }) : [];
    const box = aside ? aside.getBoundingClientRect() : null;
    return { shown, flowing, innerWidth: window.innerWidth, screen: `${screen.width}x${screen.height}`,
             clientWidth: document.documentElement.clientWidth,
             rem: parseFloat(getComputedStyle(document.documentElement).fontSize),
             role: aside && aside.getAttribute('role'), label: aside && aside.getAttribute('aria-label'),
             column: { left: column.left, right: column.right, width: column.width },
             aside: box && { left: box.left, width: box.width }, notes, rules,
             layoutMs: window.gospelGetterMargin ? window.gospelGetterMargin.layoutMs() : null };";

/// The text column and its first verses, where they are on screen.
const COLUMN_JS: &str = "const column = document.querySelector('.chapter-main').getBoundingClientRect();
    const verses = [...document.querySelectorAll('.chapter-main .chapter-text > .verse[data-verse]')]
        .slice(0, 8).map((v) => { const b = v.getBoundingClientRect(); return [b.left, b.top, b.width, b.height]; });
    return { column: [column.left, column.top, column.width], verses, y: window.scrollY,
             shown: window.gospelGetterMargin ? window.gospelGetterMargin.shown() : null };";

/// What the margin checks leave to undo: their notes, and the settings
/// (and reading mode) they found.
#[derive(Debug, Default)]
pub(super) struct MarginLeft {
    notes: Vec<i64>,
    found: Option<Value>,
}

/// What there's room for, once the margin has settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Room {
    Margin,
    Markers,
}

impl Run<'_> {
    /// Runs `script` (which starts adding or deleting notes) and waits for
    /// it to finish: the ids it went through.
    async fn notes_through_ipc(&self, what: &str, script: &str, args: Value) -> Result<Vec<i64>> {
        self.ui.js(script, vec![args]).await?;
        let seen = self
            .ui
            .wait_within(what, NOTES_DONE_JS, Vec::new(), Some(NOTES_LIMIT))
            .await?;
        if let Some(error) = seen["error"].as_str() {
            bail!("{what} failed: {error}");
        }
        Ok(seen["ids"]
            .as_array()
            .map(|ids| ids.iter().filter_map(Value::as_i64).collect())
            .unwrap_or_default())
    }

    pub(super) async fn add_notes(
        &self,
        notes: &[(u32, u32, u32, u32, String)],
    ) -> Result<Vec<i64>> {
        let rows: Vec<Value> = notes
            .iter()
            .map(|(book, chapter, start, end, body)| json!([book, chapter, start, end, body]))
            .collect();
        self.notes_through_ipc(
            "the notes to be added through the page's IPC",
            ADD_NOTES_JS,
            json!(rows),
        )
        .await
    }

    pub(super) async fn delete_notes(&self, ids: &[i64]) -> Result<()> {
        self.notes_through_ipc(
            "the notes to be deleted through the page's IPC",
            DELETE_NOTES_JS,
            json!(ids),
        )
        .await?;
        Ok(())
    }

    /// Waits until the margin has settled for what there's room for: the
    /// margin with every note placed, or the markers.
    async fn margin_settled(&self, notes: usize) -> Result<(Room, Value)> {
        // The margin is placed at most once a frame after what changed it
        // (Aa's layout, say): two frames on, a placing already asked for has
        // run, so what's seen next is the new one, not the one before.
        self.ui.js(TWO_FRAMES_JS, Vec::new()).await?;
        self.ui
            .wait(
                "two frames to pass (the margin is placed at most once a frame)",
                "const s = window.__guiCheckFrames; return [!!s && s.done, null];",
            )
            .await?;
        let seen = self
            .ui
            .wait_with(
                "the margin notes to be placed (or their markers)",
                "const m = window.gospelGetterMargin;
                 const placed = [...document.querySelectorAll('.chapter-main .margin-notes .margin-note')];
                 const markers = document.querySelectorAll('.chapter-main .verse .margin-marker').length;
                 const seen = { shown: m ? m.shown() : null, placed: placed.length,
                     topped: placed.filter((n) => n.style.top !== '').length, markers, innerWidth: window.innerWidth };
                 return [(seen.shown === 'margin' && seen.placed === arguments[0] && seen.topped === arguments[0])
                     || (seen.shown === 'markers' && seen.markers === arguments[0]), seen];",
                vec![json!(notes)],
            )
            .await?;
        let room = if seen["shown"] == "margin" {
            Room::Margin
        } else {
            Room::Markers
        };
        self.ui.settled("the page to settle").await?;
        Ok((room, seen))
    }

    /// John 3 in reading mode, Narrow, Verses, in a [`MARGIN_WINDOW`]
    /// window, with [`JOHN_NOTES`], noting what it found and added in
    /// `left`.
    async fn margin_set_up(&mut self, left: &mut MarginLeft) -> Result<()> {
        let mut found = self.look().await?;
        found["readerMode"] = self
            .ui
            .js(
                "return document.documentElement.dataset.readerMode || 'off';",
                Vec::new(),
            )
            .await?;
        left.found = Some(found);
        self.go_to("John 3", "John 3").await?;
        let notes: Vec<(u32, u32, u32, u32, String)> = JOHN_NOTES
            .iter()
            .map(|&(start, end, body)| (43, 3, start, end, body.to_owned()))
            .collect();
        let ids = self.add_notes(&notes).await?;
        left.notes.extend(&ids);
        ensure!(ids.len() == JOHN_NOTES.len(), "added {} notes", ids.len());
        // Read again, with the notes.
        self.go_to("John 3", "John 3").await?;
        let _ = self.window_width(MARGIN_WINDOW).await;
        self.open_aa().await?;
        self.aa_choose("width-choices", "width", "narrow").await?;
        self.aa_choose("layout-choices", "layout", "verses").await?;
        self.aa_choose("theme-choices", "theme", "chi-rho").await?;
        self.close_with_escape("Aa", "aa-popover").await?;
        self.reading_mode_on().await?;
        Ok(())
    }

    /// Puts things back as check 60 found them: none of its notes, the
    /// settings and reading mode as they were, the default window.
    pub(super) async fn margin_clean_up(&mut self, left: &MarginLeft) -> Result<()> {
        if !left.notes.is_empty() {
            self.delete_notes(&left.notes).await?;
        }
        let _ = self.window_width(MARGIN_WINDOW).await;
        let Some(found) = &left.found else {
            return Ok(());
        };
        let mode = self
            .ui
            .js(
                "return document.documentElement.dataset.readerMode || 'off';",
                Vec::new(),
            )
            .await?;
        if mode == "on" && found["readerMode"] != "on" {
            self.ui.click("the exit button", "#reader-exit").await?;
            self.reading_mode_off("reading mode to end").await?;
        }
        let settings = &found["settings"];
        self.open_aa().await?;
        for (group, name) in [
            ("line-spacing-choices", "lineSpacing"),
            ("width-choices", "width"),
            ("layout-choices", "layout"),
            ("theme-choices", "theme"),
        ] {
            if let Some(value) = settings[name].as_str() {
                self.aa_choose(group, name, value).await?;
            }
        }
        self.close_with_escape("Aa", "aa-popover").await?;
        Ok(())
    }

    /// Check 60's measures of the margin as laid out now.
    async fn margin_measured(&self, ids: &[i64]) -> Result<String> {
        let added: Vec<Value> = ids
            .iter()
            .zip(JOHN_NOTES)
            .map(|(id, (start, end, _))| json!([id, start, end]))
            .collect();
        let seen = self
            .ui
            .js(MARGIN_JS, vec![json!(added), json!("kjv")])
            .await?;
        ensure!(seen["shown"] == "margin", "no margin: {seen}");
        ensure!(
            seen["role"] == "complementary" && seen["label"] == "Your notes",
            "the margin isn't the \"Your notes\" landmark: {seen}"
        );
        let num = |v: &Value| v.as_f64().unwrap_or(f64::NAN);
        let column_right = num(&seen["column"]["right"]);
        let aside_left = num(&seen["aside"]["left"]);
        ensure!(
            aside_left >= column_right,
            "the margin is over the column: {seen}"
        );
        let notes = seen["notes"].as_array().cloned().unwrap_or_default();
        ensure!(
            notes.len() == JOHN_NOTES.len(),
            "{} notes in the margin",
            notes.len()
        );
        let mut worst = 0.0_f64;
        let mut edge = f64::INFINITY;
        let mut stacked = 0;
        let mut previous_bottom = f64::NEG_INFINITY;
        for note in &notes {
            let (top, bottom, anchor) = (
                num(&note["top"]),
                num(&note["bottom"]),
                num(&note["anchor"]),
            );
            ensure!(
                top >= previous_bottom - 0.5,
                "notes overlap: {note} under {previous_bottom}"
            );
            // Room to breathe at the window's edge: at least EDGE_REM
            // (`margin_text::EDGE_REM`) between a note and the edge (its
            // scroll bar left out).
            ensure!(
                num(&note["right"])
                    <= num(&seen["clientWidth"]) - EDGE_REM * num(&seen["rem"]) + 0.5,
                "a note has less than {EDGE_REM}rem of room at the window's edge: {note} in {seen}"
            );
            edge = edge.min(num(&seen["clientWidth"]) - num(&note["right"]));
            if anchor >= previous_bottom + 8.0 - 0.5 {
                worst = worst.max((top - anchor).abs());
                ensure!(
                    (top - anchor).abs() <= 2.0,
                    "a note isn't beside its verse: {note}"
                );
            } else {
                stacked += 1;
                ensure!(
                    top >= previous_bottom + 8.0 - 1.0 && top <= previous_bottom + 8.0 + 2.0,
                    "a note that would overlap isn't just below the one above: {note}"
                );
            }
            previous_bottom = bottom;
        }
        ensure!(
            stacked >= 1,
            "the note on John 3:16 should stack under the range's: {seen}"
        );
        let rules = seen["rules"].as_array().cloned().unwrap_or_default();
        ensure!(rules.len() == 1, "one rule, for the range: {seen}");
        let rule = &rules[0];
        ensure!(
            (num(&rule["top"]) - num(&rule["from"])).abs() <= 2.0
                && (num(&rule["bottom"]) - num(&rule["to"])).abs() <= 2.0,
            "the range's rule isn't beside its verses: {rule}"
        );
        let folded: Vec<&Value> = notes.iter().filter(|n| n["folded"] == true).collect();
        ensure!(
            folded.len() == 1 && folded[0]["more"] == true && folded[0]["start"] == 20,
            "only the long note is folded, with more: {seen}"
        );
        Ok(format!(
            "{} notes beside the column (margin {}px wide from x={aside_left:.0}, the column ending at \
             {column_right:.0}, {edge:.0}px of room at the window's edge): unstacked ones within {worst:.2}px \
             of their verse{}, {stacked} stacked just below the note above, none overlapping; the range's rule \
             beside its verses; the long note folded with more; placed in {:.2} ms",
            notes.len(),
            num(&seen["aside"]["width"]),
            if seen["flowing"] == true {
                "'s number"
            } else {
                ""
            },
            num(&seen["layoutMs"]),
        ))
    }

    /// Check 60.
    pub(super) async fn margin_notes(&mut self, left: &mut MarginLeft) -> Result<String> {
        self.margin_set_up(left).await?;
        let ids = left.notes.clone();
        let (room, seen) = self.margin_settled(JOHN_NOTES.len()).await?;
        if room == Room::Markers {
            return Ok(format!(
                "not run on this screen ({}, the window {} px wide): too narrow for the margin beside a \
                 Narrow column; check 62 checks the markers that stand in for it",
                seen["screen"].as_str().unwrap_or("?"),
                seen["innerWidth"]
            ));
        }
        // John 3:5 to 3:21 in view.
        self.ui
            .js(
                "const v = document.querySelector('.chapter-main .chapter-text > .verse[data-verse=\"5\"]');
                 window.scrollBy(0, v.getBoundingClientRect().top - 90); return null;",
                Vec::new(),
            )
            .await?;
        let mut said = Vec::new();
        for (theme, verses_shot, flowing_shot) in [
            (
                "chi-rho",
                "60-margin-verses-chi-rho.png",
                "60-margin-flowing-chi-rho.png",
            ),
            (
                "vellum",
                "60-margin-verses-vellum.png",
                "60-margin-flowing-vellum.png",
            ),
        ] {
            self.open_aa().await?;
            self.aa_choose("theme-choices", "theme", theme).await?;
            self.aa_choose("layout-choices", "layout", "verses").await?;
            self.close_with_escape("Aa", "aa-popover").await?;
            self.margin_settled(JOHN_NOTES.len()).await?;
            said.push(format!(
                "{theme}, Verses: {}",
                self.margin_measured(&ids).await?
            ));
            self.shot(
                &format!("screenshot: margin notes, Verses, {theme}"),
                verses_shot,
            )
            .await;
            self.open_aa().await?;
            self.aa_choose("layout-choices", "layout", "flowing")
                .await?;
            self.close_with_escape("Aa", "aa-popover").await?;
            self.margin_settled(JOHN_NOTES.len()).await?;
            said.push(format!(
                "{theme}, Flowing: {}",
                self.margin_measured(&ids).await?
            ));
            self.shot(
                &format!("screenshot: margin notes, Flowing, {theme}"),
                flowing_shot,
            )
            .await;
        }
        self.open_aa().await?;
        self.aa_choose("theme-choices", "theme", "chi-rho").await?;
        self.aa_choose("layout-choices", "layout", "verses").await?;
        self.close_with_escape("Aa", "aa-popover").await?;
        said.push(self.margin_normal(&ids).await?);
        Ok(said.join("; "))
    }

    /// Check 60 in the Normal width, at the default text size, in a
    /// [`MARGIN_WINDOW`]x[`MARGIN_HEIGHT`] window: the margin, not
    /// markers. Puts back the window and Narrow for the checks that follow.
    async fn margin_normal(&mut self, ids: &[i64]) -> Result<String> {
        let rect = self.ui.driver().get_window_rect().await?;
        self.ui
            .driver()
            .set_window_rect(rect.x, rect.y, MARGIN_WINDOW, MARGIN_HEIGHT)
            .await?;
        let _ = self.window_width(MARGIN_WINDOW).await;
        self.open_aa().await?;
        let look = self.aa_choose("width-choices", "width", "normal").await?;
        self.close_with_escape("Aa", "aa-popover").await?;
        ensure!(
            look["settings"]["textSize"] == "medium",
            "the text size isn't the default (Medium): {look}"
        );
        let (room, seen) = self.margin_settled(JOHN_NOTES.len()).await?;
        let said = if room == Room::Markers
            && seen["innerWidth"].as_u64().unwrap_or(0) < u64::from(MARGIN_WINDOW)
        {
            format!(
                "Normal: not run on this screen ({}, the window {} px wide)",
                seen["screen"].as_str().unwrap_or("?"),
                seen["innerWidth"]
            )
        } else {
            let measured = self.margin_measured(ids).await;
            let size = self
                .ui
                .js(
                    "return `${window.innerWidth}x${window.innerHeight}`;",
                    Vec::new(),
                )
                .await?;
            let size = size.as_str().unwrap_or("?");
            let measured = measured.map_err(|e| {
                e.context(format!(
                    "Normal, a {size} window: the margin should fit beside the column"
                ))
            })?;
            self.shot(
                "screenshot: margin notes, Normal, Chi Rho",
                "60-margin-normal-chi-rho.png",
            )
            .await;
            format!("Normal, a {size} window, Medium text: {measured}")
        };
        self.ui
            .driver()
            .set_window_rect(rect.x, rect.y, rect.width as u32, rect.height as u32)
            .await?;
        let _ = self.window_width(rect.width as u32).await;
        self.open_aa().await?;
        self.aa_choose("width-choices", "width", "narrow").await?;
        self.close_with_escape("Aa", "aa-popover").await?;
        self.margin_settled(JOHN_NOTES.len()).await?;
        Ok(said)
    }

    /// Check 61: the switch off and on leaves the column where it was.
    pub(super) async fn margin_switch(&mut self) -> Result<String> {
        let (room, seen) = self.margin_settled(JOHN_NOTES.len()).await?;
        if room == Room::Markers {
            return Ok(format!(
                "not run on this screen (the window {} px wide): no room for the margin (see check 60)",
                seen["innerWidth"]
            ));
        }
        let before = self.ui.js(COLUMN_JS, Vec::new()).await?;
        let mut seen_off = Value::Null;
        for (pressed, shown) in [("false", "nothing"), ("true", "margin")] {
            self.open_aa().await?;
            self.ui
                .click("Aa: Notes in the margin", "#margin-notes-btn")
                .await?;
            let seen = self
                .ui
                .wait_with(
                    &format!("the margin to show {shown}"),
                    &format!(
                        "const seen = (() => {{ {COLUMN_JS} }})();
                         const pressed = document.getElementById('margin-notes-btn').getAttribute('aria-pressed');
                         seen.pressed = pressed;
                         seen.aside = !!document.querySelector('.chapter-main .margin-notes');
                         return [pressed === arguments[0] && seen.shown === arguments[1]
                             && seen.aside === (arguments[1] === 'margin'), seen];"
                    ),
                    vec![json!(pressed), json!(shown)],
                )
                .await?;
            self.close_with_escape("Aa", "aa-popover").await?;
            ensure!(
                seen["column"] == before["column"]
                    && seen["verses"] == before["verses"]
                    && seen["y"] == before["y"],
                "the text moved when the margin showed {shown}: before {before}, after {seen}"
            );
            if shown == "nothing" {
                seen_off = seen;
            }
        }
        Ok(format!(
            "Notes in the margin off (aria-pressed {}, no margin) and on again: the column at {} and its first \
             verses at {} both times, to the pixel",
            seen_off["pressed"], before["column"], before["verses"]
        ))
    }

    /// Check 62: too narrow for the margin, markers; a marker opens the card.
    pub(super) async fn margin_narrow(&mut self) -> Result<String> {
        let _ = self.window_width(NARROW_WINDOW).await;
        let (room, seen) = self.margin_settled(JOHN_NOTES.len()).await?;
        ensure!(
            room == Room::Markers,
            "a {NARROW_WINDOW} px window still has the margin: {seen}"
        );
        let markers = self
            .ui
            .js(
                "return [...document.querySelectorAll('.chapter-main .verse .margin-marker')]
                     .map((m) => [Number(m.closest('.verse').dataset.verse), m.getAttribute('aria-label')]);",
                Vec::new(),
            )
            .await?;
        ensure!(
            markers
                == json!([
                    [5, "Note on John 3:5"],
                    [16, "Note on John 3:16"],
                    [16, "Note on John 3:16–18"],
                    [20, "Note on John 3:20"]
                ]),
            "the markers: {markers}"
        );
        self.ui
            .js(
                "const m = document.querySelector('.chapter-main .verse[data-verse=\"5\"] .margin-marker');
                 window.scrollBy(0, m.getBoundingClientRect().top - 200); return null;",
                Vec::new(),
            )
            .await?;
        self.ui
            .click(
                "the marker after John 3:5",
                ".chapter-main .verse[data-verse=\"5\"] .margin-marker",
            )
            .await?;
        let card = self
            .ui
            .wait(
                "the card to open on John 3:5 with its note",
                "const selected = [...document.querySelectorAll('.chapter-main .verse-selected')].map((v) => Number(v.dataset.verse));
                 const notes = [...document.querySelectorAll('.verse-notes .note-card-body')].map((b) => b.textContent);
                 return [selected.join() === '5' && notes.includes('Water and the Spirit.'), { selected, notes }];",
            )
            .await?;
        self.shot(
            "screenshot: margin notes' markers, a narrow window, Chi Rho",
            "62-margin-markers-chi-rho.png",
        )
        .await;
        self.ui
            .press("press Escape (clear the selection)", Key::Escape)
            .await?;
        let _ = self.window_width(MARGIN_WINDOW).await;
        Ok(format!(
            "a {} px window: {} markers, after John 3:5, 3:16 (two) and 3:20, each labelled; the one after 3:5 \
             opened the card on {} with its note",
            seen["innerWidth"],
            markers.as_array().map_or(0, Vec::len),
            card["selected"]
        ))
    }

    /// Check 63: Psalm 119 with a note on every verse, placed in under
    /// 16 ms.
    pub(super) async fn margin_psalm_119(&mut self, left: &mut MarginLeft) -> Result<String> {
        self.ui.click("the exit button", "#reader-exit").await?;
        self.reading_mode_off("reading mode to end").await?;
        self.go_to("Psalm 119", "Psalms 119").await?;
        let notes: Vec<(u32, u32, u32, u32, String)> = (1..=176)
            .map(|v| (19, 119, v, v, format!("A note on verse {v}.")))
            .collect();
        let added = self.add_notes(&notes).await?;
        left.notes.extend(&added);
        ensure!(added.len() == 176, "added {} notes", added.len());
        self.go_to("Psalm 119", "Psalms 119").await?;
        self.reading_mode_on().await?;
        let (room, seen) = self.margin_settled(176).await?;
        if room == Room::Markers {
            return Ok(format!(
                "not run on this screen (the window {} px wide): no room for the margin (see check 60)",
                seen["innerWidth"]
            ));
        }
        let drawn = self.margin_layout_ms().await?;
        // Placed again over the same layout: the theme changed (the page
        // keeps its shape; a line spacing or size change would time the
        // browser laying out the whole psalm again, not the margin).
        let theme = self.look().await?["settings"]["theme"]
            .as_str()
            .unwrap_or("chi-rho")
            .to_owned();
        let other = if theme == "vellum" {
            "chi-rho"
        } else {
            "vellum"
        };
        self.open_aa().await?;
        self.aa_choose("theme-choices", "theme", other).await?;
        self.margin_settled(176).await?;
        let placed = self.margin_layout_ms().await?;
        self.aa_choose("theme-choices", "theme", &theme).await?;
        self.close_with_escape("Aa", "aa-popover").await?;
        let measured = self.ui.js(MARGIN_JS, vec![json!([]), json!("kjv")]).await?;
        let tops: Vec<(f64, f64)> = measured["notes"]
            .as_array()
            .map(|notes| {
                notes
                    .iter()
                    .map(|n| {
                        (
                            n["top"].as_f64().unwrap_or(0.0),
                            n["bottom"].as_f64().unwrap_or(0.0),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        ensure!(tops.len() == 176, "{} notes placed", tops.len());
        ensure!(
            tops.windows(2).all(|w| w[1].0 >= w[0].1 - 0.5),
            "two of Psalm 119's notes overlap"
        );
        ensure!(
            drawn < 16.0 && placed < 16.0,
            "Psalm 119's margin took {drawn:.2} ms drawn and placed, {placed:.2} ms placed again (over 16 ms)"
        );
        Ok(format!(
            "Psalm 119, a note on each of its 176 verses: drawn and placed in {drawn:.2} ms, placed again (the \
             theme changed) in {placed:.2} ms, none overlapping"
        ))
    }

    async fn margin_layout_ms(&self) -> Result<f64> {
        let ms = self
            .ui
            .js("return window.gospelGetterMargin.layoutMs();", Vec::new())
            .await?;
        ms.as_f64()
            .ok_or_else(|| anyhow::anyhow!("no layout time: {ms}"))
    }
}
