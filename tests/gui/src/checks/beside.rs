//! Check 69: notes beside the text, "Always", outside reading mode
//! (ui-leptos/src/margin.rs, M1), on John 3 in the KJV with five notes added
//! through the page's own IPC (a verse, a range, a note starting on the
//! verse after the range's first, a long sermon outline over two verses,
//! and one far down), in Verses, in a 1440x900 window with the sidebar open:
//!
//! - Aa's "Notes beside the text: Always" shows the margin, not markers,
//!   beside a text column that keeps the bar and the sidebar, the column and
//!   the margin one group beside the sidebar; the margin's left edge is at
//!   or right of the column's right edge.
//! - The text never moves: every verse's `offsetTop` is the same, to the
//!   pixel, the moment Always is chosen (the page in its shape, the notes
//!   not yet placed: they come in a later frame), with the notes placed, and
//!   after a note is edited in the card.
//! - Each note's top is at or below its first verse's top (beside it, or
//!   just below the note above where they'd overlap); none overlap.
//! - S, pressed as a real key, opens this passage beside (each pane then
//!   has markers, not a margin), and S again closes the second pane; in
//!   reading mode, S leaves it and opens both panes (B9-B12).
//!
//! A screen that can't hold a 1440 px window (the Windows runner's) has the
//! margin or markers as its room allows: the check says which, and checks
//! the margin's measures only where it shows. Screenshots in Chi Rho,
//! after the edit (the card open on the note), and in Vellum. The notes are
//! deleted after, and the settings, the window and reading mode put back.

use anyhow::{Result, ensure};
use serde_json::{Value, json};

use super::{Run, VISIBLE_JS};

/// The screenshots check 69 takes (check 4 lists them).
pub(super) const BESIDE_SHOTS: [&str; 3] = [
    "69-notes-always-chi-rho.png",
    "69-notes-always-edited-chi-rho.png",
    "69-notes-always-vellum.png",
];

/// The window it's checked in: wide enough for the sidebar and the margin
/// beside the Normal column (M1's measures).
const BESIDE_WIDTH: u32 = 1440;
const BESIDE_HEIGHT: u32 = 900;

/// The sermon-prep notes of the mockup (in the reader's own words; no
/// verse's words are touched): John 3:2, 3–5, 4, 7–8 and 16.
const JOHN_NOTES: [(u32, u32, &str); 5] = [
    (
        2,
        2,
        "He comes *by night*. Light and darkness again, as in 1:5 and vv. 19\u{2013}21.",
    ),
    (
        3,
        5,
        "**Born again:** *anothen* also means \u{201c}from above\u{201d} (v. 31). Nicodemus hears only \
         \u{201c}a second time\u{201d}. Not reform but a birth God gives. Ask the room: what do we try \
         to fix in ourselves that only God can make new? Tie to Ezek 36:26.",
    ),
    (
        4,
        4,
        "He takes it literally. Misunderstanding moves John's dialogues along; see John 4:11 and John 6:52.",
    ),
    (
        7,
        8,
        "Sermon outline, *You must be born again*:\n\
         - **The need** (v. 3): not a better life but a new one.\n\
         - **The means** (vv. 5\u{2013}8): water and Spirit; like the wind, we see what the Spirit does, \
         not where he comes from.\n\
         - **The object** (vv. 14\u{2013}15): the Son lifted up; see Num 21:8-9, look and live.\n\n\
         Close on v. 16 and an invitation. Illustration: a kite on a still day, and on a windy one.",
    ),
    (
        16,
        16,
        "**Love's measure:** he *gave*. See Rom 5:8 and 1 John 4:9-10.",
    ),
];

/// What the edit adds to John 3:16's note.
const EDITED: &str = " Edited in the card.";

/// Every verse's `offsetTop` in the chapter being read.
const VERSE_TOPS_JS: &str =
    "return [...document.querySelectorAll('.chapter-main .chapter-text > .verse[data-verse]')]
    .map((v) => v.offsetTop);";

/// Chooses Always in Aa's "Notes beside the text" (Aa open) and, in the
/// same task, before the frame that places the notes, measures the text:
/// the page has its shape for Always by then, the margin not yet drawn.
const CHOOSE_ALWAYS_JS: &str = "const choice = document.querySelector('#notes-beside-choices [data-value=\"always\"]');
    if (!choice) return { error: 'no Always in Aa' };
    choice.click();
    return { pressed: choice.getAttribute('aria-pressed'),
             setting: document.documentElement.dataset.marginNotes || null,
             drawn: !!document.querySelector('.chapter-main .margin-notes, .chapter-main .margin-marker'),
             tops: [...document.querySelectorAll('.chapter-main .chapter-text > .verse[data-verse]')].map((v) => v.offsetTop) };";

/// The margin as laid out, against the notes added (`arguments[0]`: [id,
/// start, end] each): each note's box with its first verse's, the margin
/// against the column, and the page around them.
const MARGIN_JS: &str = "const added = arguments[0];
    const main = document.querySelector('#reading-pane .chapter-main');
    const aside = main && main.querySelector('.margin-notes');
    const verse = (n) => main.querySelector(`.chapter-text > .verse[data-verse=\"${n}\"]`);
    const notes = aside ? [...aside.querySelectorAll('.margin-note')].map((el) => {
        const id = Number(el.dataset.noteId);
        const [, start, end] = added.find(([i]) => i === id) || [];
        const box = el.getBoundingClientRect();
        const more = el.querySelector('.margin-note-more');
        return { id, start, end, top: box.top, bottom: box.bottom,
                 verse: start && verse(start) ? verse(start).getBoundingClientRect().top : null,
                 label: el.querySelector('.margin-note-ref').textContent,
                 body: el.querySelector('.margin-note-body').textContent,
                 folded: el.classList.contains('is-folded'), more: !!more && !more.hidden };
    }) : [];
    const shown = (el) => !!el && getComputedStyle(el).display !== 'none';
    const column = main.getBoundingClientRect();
    const margin = aside ? aside.getBoundingClientRect() : null;
    const picker = document.getElementById('picker');
    return { shown: window.gospelGetterMargin ? window.gospelGetterMargin.shown() : null,
             column: [Math.round(column.left), Math.round(column.width), Math.round(column.right)],
             margin: margin && [Math.round(margin.left), Math.round(margin.width), Math.round(margin.right)],
             sidebar: document.documentElement.dataset.picker === 'sidebar' && shown(picker) && !picker.hidden,
             bar: shown(document.getElementById('library-toggle')) && shown(document.getElementById('menu-toggle')),
             neighbours: [...document.querySelectorAll('.chapter-side')].filter(shown).length,
             markers: main.querySelectorAll('.margin-marker').length,
             readerMode: document.documentElement.dataset.readerMode || 'off',
             reference: document.getElementById('ref-text').textContent,
             notes, innerWidth: window.innerWidth };";

/// The panes, and what the margin shows in each pane laid out.
const PANES_JS: &str = "const html = document.documentElement;
    const panes = window.gospelGetterPanes.shown();
    const each = panes.map((name) => {
        const root = document.getElementById(name === 'first' ? 'reading-pane' : 'reading-pane-2');
        const col = root.closest('.pane');
        const hidden = !!(col && col.hidden);
        return { name, hidden, heading: (root.querySelector('.chapter-heading') || {}).textContent || null,
                 margin: !!root.querySelector('.margin-notes'),
                 markers: [...root.querySelectorAll('.verse .margin-marker')].map((m) => Number(m.closest('.verse').dataset.verse)) };
    });
    return { panes, each, readerMode: html.dataset.readerMode || 'off', split: !!html.dataset.split,
             active: window.gospelGetterPanes.active(),
             shown: window.gospelGetterMargin ? window.gospelGetterMargin.shown() : null,
             focus: document.activeElement ? (document.activeElement.id || document.activeElement.tagName) : null };";

/// What check 69 leaves to undo: its notes, the window it found, and what
/// else it found.
#[derive(Debug, Default)]
struct Left {
    notes: Vec<i64>,
    window: Option<(i64, i64, u32, u32)>,
    found: Option<Value>,
}

impl Run<'_> {
    /// Check 69.
    pub(super) async fn notes_beside(&mut self) -> Result<String> {
        let mut left = Left::default();
        let result = self.notes_beside_checked(&mut left).await;
        let cleaned = self.beside_clean_up(&left).await;
        match (result, cleaned) {
            (Ok(said), Ok(())) => Ok(said),
            (Ok(_), Err(e)) => Err(e.context("putting things back after check 69")),
            (Err(e), _) => Err(e),
        }
    }

    async fn notes_beside_checked(&mut self, left: &mut Left) -> Result<String> {
        let mut found = self.look().await?;
        found["page"] = self
            .ui
            .js(
                "const data = document.documentElement.dataset;
                 return { readerMode: data.readerMode || 'off', compare: data.compare || 'off',
                          marginNotes: data.marginNotes || 'reading' };",
                Vec::new(),
            )
            .await?;
        left.found = Some(found.clone());
        let rect = self.ui.driver().get_window_rect().await?;
        left.window = Some((rect.x, rect.y, rect.width as u32, rect.height as u32));
        if found["page"]["readerMode"] == "on" {
            self.ui.click("the exit button", "#reader-exit").await?;
            self.reading_mode_off("reading mode to end").await?;
        }
        if found["page"]["compare"] == "on" {
            self.open_translations().await?;
            self.ui.click("Compare off", "#compare-btn").await?;
            self.close_translations().await?;
        }
        self.go_to("John 3", "John 3").await?;
        let notes: Vec<(u32, u32, u32, u32, String)> = JOHN_NOTES
            .iter()
            .map(|&(start, end, body)| (43, 3, start, end, body.to_owned()))
            .collect();
        let ids = self.add_notes(&notes).await?;
        left.notes.extend(&ids);
        ensure!(ids.len() == JOHN_NOTES.len(), "added {} notes", ids.len());
        let added: Vec<Value> = ids
            .iter()
            .zip(JOHN_NOTES)
            .map(|(id, (start, end, _))| json!([id, start, end]))
            .collect();
        // The window M1 was measured in; a screen that can't hold it keeps
        // what it can.
        let _ = self
            .ui
            .driver()
            .set_window_rect(rect.x, rect.y, BESIDE_WIDTH, BESIDE_HEIGHT)
            .await;
        let _ = self.window_width(BESIDE_WIDTH).await;
        // Read again, with the notes.
        self.go_to("John 3", "John 3").await?;
        self.open_aa().await?;
        self.aa_choose("layout-choices", "layout", "verses").await?;
        self.aa_choose("theme-choices", "theme", "chi-rho").await?;
        let chosen = self.ui.js(CHOOSE_ALWAYS_JS, Vec::new()).await?;
        ensure!(chosen["error"].is_null(), "{chosen}");
        ensure!(
            chosen["pressed"] == "true" && chosen["setting"] == "always",
            "Always isn't chosen: {chosen}"
        );
        ensure!(
            chosen["drawn"] == false,
            "the margin should be drawn in a later frame: {chosen}"
        );
        let before = chosen["tops"].clone();
        ensure!(
            before.as_array().is_some_and(|t| t.len() == 36),
            "John 3 has 36 verses: {chosen}"
        );
        self.close_with_escape("Aa", "aa-popover").await?;

        let placed = self.beside_settled(&added).await?;
        let said_placed = self.beside_measured(&placed, JOHN_NOTES.len())?;
        let after = self.ui.js(VERSE_TOPS_JS, Vec::new()).await?;
        ensure!(
            after == before,
            "the text moved when the notes came: {before} then {after}"
        );
        self.shot(
            "screenshot: notes beside the text, Always, Chi Rho",
            BESIDE_SHOTS[0],
        )
        .await;

        let edited = if placed["shown"] == "margin" {
            self.beside_edit(ids[4], &before, &added).await?
        } else {
            "not edited from the margin (markers here)".to_owned()
        };
        self.shot(
            "screenshot: Always, a note edited, Chi Rho",
            BESIDE_SHOTS[1],
        )
        .await;
        self.plain_reading().await?;
        self.open_aa().await?;
        self.aa_choose("theme-choices", "theme", "vellum").await?;
        self.close_with_escape("Aa", "aa-popover").await?;
        self.two_frames().await?;
        self.shot("screenshot: Always, Vellum", BESIDE_SHOTS[2])
            .await;
        let s_key = self.beside_s_key().await?;
        Ok(format!(
            "Always: {said_placed}; every verse's offsetTop the same the moment Always was chosen (no margin \
             drawn), with the notes placed{edited}; {s_key}"
        ))
    }

    /// Opens John 3:16's note (`love`) from the margin, edits it in the
    /// card, and checks the text stayed where it was (`before`).
    async fn beside_edit(&mut self, love: i64, before: &Value, added: &[Value]) -> Result<String> {
        self.ui
            .click(
                "John 3:16's note's reference",
                &format!(".margin-notes .margin-note[data-note-id=\"{love}\"] .margin-note-ref"),
            )
            .await?;
        self.ui
            .wait(
                "the card on John 3:16, the note in it",
                &format!(
                    "{VISIBLE_JS} const card = document.getElementById('selection-card');
                     const selected = [...document.querySelectorAll('.chapter-main .verse.verse-selected')].map((v) => v.dataset.verse);
                     return [!!card && visible(card) && selected.join() === '16' && !!card.querySelector('.note-edit'), selected];"
                ),
            )
            .await?;
        self.ui
            .click("click the note's Edit", "#selection-card .note-edit")
            .await?;
        self.ui
            .wait(
                "the editor to open on the note",
                &format!(
                    "{VISIBLE_JS} const input = document.getElementById('note-input');
                     return [visible(document.getElementById('note-panel')) && input.value.startsWith('**Love'), input.value];"
                ),
            )
            .await?;
        self.ui
            .js(
                "const input = document.getElementById('note-input'); input.focus();
                 input.setSelectionRange(input.value.length, input.value.length); return null;",
                Vec::new(),
            )
            .await?;
        self.ui
            .type_into("add to the note", "#note-input", EDITED)
            .await?;
        self.ui.click("click Save", "#note-save").await?;
        let edited = self
            .ui
            .wait_with(
                "the margin to show the note edited",
                "const note = document.querySelector(`.margin-notes .margin-note[data-note-id=\"${arguments[0]}\"]`);
                 const body = note ? note.querySelector('.margin-note-body').textContent : null;
                 return [!!body && body.endsWith(arguments[1].trim()) && note.style.top !== '', body];",
                vec![json!(love), json!(EDITED)],
            )
            .await?;
        self.two_frames().await?;
        let after_edit = self.ui.js(VERSE_TOPS_JS, Vec::new()).await?;
        ensure!(
            &after_edit == before,
            "the text moved when a note was edited: {before} then {after_edit}"
        );
        let replaced = self.ui.js(MARGIN_JS, vec![json!(added)]).await?;
        self.beside_measured(&replaced, JOHN_NOTES.len())?;
        Ok(format!(
            ", and after John 3:16's note was edited in the card (the margin then saying \"{}\")",
            edited.as_str().unwrap_or("?")
        ))
    }

    /// S as a real key: this passage beside, then the second pane closed;
    /// then in reading mode, which S leaves first.
    async fn beside_s_key(&mut self) -> Result<String> {
        self.focus_off_controls().await?;
        self.ui
            .press("press S (open this passage beside)", "s")
            .await?;
        let opened = self
            .ui
            .wait(
                "two panes, John 3 in each, markers in each pane laid out",
                &format!(
                    "const seen = (() => {{ {PANES_JS} }})();
                     const laidOut = seen.each.filter((p) => !p.hidden);
                     return [seen.split && seen.panes.length === 2 && seen.active === 'second'
                             && seen.each.every((p) => p.heading === 'John 3')
                             && laidOut.every((p) => !p.margin && p.markers.length > 0), seen];"
                ),
            )
            .await?;
        ensure!(
            opened["shown"] == "markers",
            "two panes should have markers: {opened}"
        );
        self.ui
            .press("press S (close the second pane)", "s")
            .await?;
        let closed = self
            .ui
            .wait(
                "one pane again",
                &format!(
                    "const seen = (() => {{ {PANES_JS} }})();
                     return [!seen.split && seen.panes.length === 1, seen];"
                ),
            )
            .await?;
        self.reading_mode_on().await?;
        self.ui
            .press("press S in reading mode (leave it, open both panes)", "s")
            .await?;
        let both = self
            .ui
            .wait(
                "reading mode left and both panes open, side by side",
                &format!(
                    "const seen = (() => {{ {PANES_JS} }})();
                     return [seen.readerMode === 'off' && seen.split && seen.panes.length === 2
                             && seen.each.every((p) => !p.hidden || window.innerWidth < 900), seen];"
                ),
            )
            .await?;
        self.ui
            .press("press S (close the second pane)", "s")
            .await?;
        self.ui
            .wait(
                "one pane again",
                &format!(
                    "const seen = (() => {{ {PANES_JS} }})();
                     return [!seen.split && seen.panes.length === 1, seen];"
                ),
            )
            .await?;
        let markers = |seen: &Value| -> Vec<String> {
            seen["each"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|p| format!("{}: {}", p["name"].as_str().unwrap_or("?"), p["markers"]))
                .collect()
        };
        Ok(format!(
            "S (a real key) opened John 3 beside it, the second active, markers in each pane ({}), and S again \
             closed it (focus on {}); in reading mode S left it and opened both panes ({}), and S closed the \
             second",
            markers(&opened).join("; "),
            closed["focus"].as_str().unwrap_or("?"),
            both["panes"]
        ))
    }

    /// Two frames on: a placing already asked for has run.
    async fn two_frames(&self) -> Result<()> {
        self.ui
            .js(
                "const s = window.__guiCheckFrames = { done: false };
                 requestAnimationFrame(() => requestAnimationFrame(() => { s.done = true; }));
                 return null;",
                Vec::new(),
            )
            .await?;
        self.ui
            .wait(
                "two frames to pass (the notes are placed at most once a frame)",
                "const s = window.__guiCheckFrames; return [!!s && s.done, null];",
            )
            .await?;
        self.ui.settled("the page to settle").await
    }

    /// Waits until the margin holds `added`'s notes, each placed (or, with
    /// no room for it, the markers are in), and the page has settled: the
    /// margin as laid out then.
    async fn beside_settled(&self, added: &[Value]) -> Result<Value> {
        self.ui
            .wait_with(
                "the margin to be drawn and placed",
                "const aside = document.querySelector('#reading-pane .chapter-main .margin-notes');
                 const notes = aside ? [...aside.querySelectorAll('.margin-note')] : [];
                 const shown = window.gospelGetterMargin ? window.gospelGetterMargin.shown() : null;
                 const placed = shown === 'margin' && notes.length === arguments[0] && notes.every((n) => n.style.top !== '');
                 return [placed || shown === 'markers', { shown, notes: notes.length }];",
                vec![json!(added.len())],
            )
            .await?;
        self.two_frames().await?;
        self.ui.js(MARGIN_JS, vec![json!(added)]).await
    }

    /// Check 69's measures of the margin: the margin where the window is
    /// 1440 px wide (markers only on a screen that can't hold it).
    fn beside_measured(&self, seen: &Value, count: usize) -> Result<String> {
        let num = |v: &Value| v.as_f64().unwrap_or(f64::NAN);
        let width = num(&seen["innerWidth"]);
        ensure!(
            seen["readerMode"] == "off" && seen["bar"] == true,
            "not normal reading's bar, out of reading mode: {seen}"
        );
        ensure!(seen["neighbours"] == 0, "the neighbours show: {seen}");
        ensure!(seen["reference"] == "John 3 · KJV", "the reference: {seen}");
        if width >= f64::from(BESIDE_WIDTH) - 40.0 {
            ensure!(
                seen["shown"] == "margin",
                "a {width} px window should have the margin, not {}: {seen}",
                seen["shown"]
            );
            ensure!(seen["sidebar"] == true, "the sidebar isn't open: {seen}");
        }
        let column = format!(
            "column (left, width, right) {}",
            seen["column"].to_string().replace(',', ", ")
        );
        if seen["shown"] != "margin" {
            ensure!(
                seen["shown"] == "markers" && seen["markers"].as_u64() == Some(count as u64),
                "neither the margin nor a marker for each note: {seen}"
            );
            return Ok(format!(
                "markers in a {width} px window (no room for the margin there), {column}"
            ));
        }
        let column_right = num(&seen["column"][2]);
        let margin_left = num(&seen["margin"][0]);
        ensure!(
            margin_left >= column_right,
            "the margin is over the column: {seen}"
        );
        let notes = seen["notes"].as_array().cloned().unwrap_or_default();
        ensure!(notes.len() == count, "{} notes in the margin", notes.len());
        let (mut beside, mut stacked) = (0, 0);
        let mut previous_bottom = f64::NEG_INFINITY;
        for note in &notes {
            let (top, bottom, verse) =
                (num(&note["top"]), num(&note["bottom"]), num(&note["verse"]));
            ensure!(
                top >= verse - 0.5,
                "a note is above its first verse: {note}"
            );
            ensure!(
                top >= previous_bottom - 0.5,
                "notes overlap: {note} under {previous_bottom}"
            );
            if verse >= previous_bottom + 8.0 - 0.5 {
                beside += 1;
            } else {
                stacked += 1;
            }
            previous_bottom = bottom;
        }
        let labels: Vec<&str> = notes.iter().filter_map(|n| n["label"].as_str()).collect();
        let folded = notes.iter().filter(|n| n["folded"] == true).count();
        Ok(format!(
            "the margin in a {width} px window, the sidebar open: {column}, margin (left, width, right) {}; \
             labels {labels:?}, {beside} beside their first verse and {stacked} just below the note above, \
             none above its verse or overlapping, {folded} folded past 10 lines",
            seen["margin"].to_string().replace(',', ", "),
        ))
    }

    /// Puts things back as check 69 found them: none of its notes, one
    /// pane, the look, notes beside the text, Compare, reading mode and the
    /// window.
    async fn beside_clean_up(&mut self, left: &Left) -> Result<()> {
        let ui = &self.ui;
        ui.js(
            "if (document.documentElement.dataset.readerMode === 'on') document.getElementById('reader-exit').click();
             if (window.gospelGetterPanes.shown().length > 1) window.gospelGetterPanes.close();
             return null;",
            Vec::new(),
        )
        .await?;
        ui.wait(
            "one pane, out of reading mode",
            "const html = document.documentElement;
             return [html.dataset.readerMode !== 'on' && !html.dataset.split, null];",
        )
        .await?;
        if !left.notes.is_empty() {
            self.delete_notes(&left.notes).await?;
        }
        if let Some((x, y, width, height)) = left.window {
            let _ = self.ui.driver().set_window_rect(x, y, width, height).await;
            let _ = self.window_width(width).await;
        }
        let Some(found) = &left.found else {
            return Ok(());
        };
        self.plain_reading().await?;
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
        let notes = found["page"]["marginNotes"].as_str().unwrap_or("reading");
        self.ui
            .click(
                &format!("Aa: notes beside the text {notes}"),
                &format!("#notes-beside-choices [data-value=\"{notes}\"]"),
            )
            .await?;
        self.close_with_escape("Aa", "aa-popover").await?;
        if found["page"]["compare"] == "on" {
            self.open_translations().await?;
            self.ui.click("Compare on", "#compare-btn").await?;
            self.close_translations().await?;
        }
        if found["page"]["readerMode"] == "on" {
            self.reading_mode_on().await?;
        }
        Ok(())
    }
}
