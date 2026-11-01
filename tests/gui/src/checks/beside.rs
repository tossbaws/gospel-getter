//! Check 69: Compare with Notes (ui-leptos/src/margin.rs), on John 3 in
//! the KJV with five notes added through the page's own IPC (a verse, a
//! range, a note starting on the verse after the range's first, a long
//! sermon outline over two verses, and one far down), in Verses:
//!
//! - Notes, chosen among Compare's "with" choices, puts a "My notes"
//!   region beside the text, headed like a translation's column, and the
//!   reference still says just the KJV.
//! - The text never moves: every verse's `offsetTop` is the same, to the
//!   pixel, the moment Notes is chosen (the chapter drawn, its notes not
//!   yet: they come in a later frame), with the notes placed, and after a
//!   note is edited in the card; and the first verse's once every note is
//!   gone (the column's one line; the reader's own ✎ after a noted verse's
//!   number goes with the notes, and can rewrap that verse).
//! - Each note's top is at or below its first verse's top (beside it, or
//!   just below the note above where they'd overlap); none overlap; the
//!   range's rule runs beside its verses; the column is beside the text,
//!   never over it.
//! - A note's label opens the card on its verses; the note edited there is
//!   shown edited in the column.
//!
//! Screenshots in Chi Rho, after the edit (the card open on the note, the
//! column showing it edited), and in Vellum. The notes are
//! deleted after, and the settings, Compare and its "with" put back.

use anyhow::{Result, ensure};
use serde_json::{Value, json};

use super::{Run, VISIBLE_JS};

/// The screenshots check 69 takes (check 4 lists them).
pub(super) const BESIDE_SHOTS: [&str; 3] = [
    "69-notes-beside-chi-rho.png",
    "69-notes-beside-edited-chi-rho.png",
    "69-notes-beside-vellum.png",
];

/// The window it's checked in: the app's default.
const BESIDE_WINDOW: u32 = 1280;

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

/// Chooses Notes among Compare's "with" choices (its popover open) and, in
/// the same task, before the frame that places the notes, measures the
/// text: the chapter is drawn in Compare's Notes layout by then, its
/// column still empty.
const CHOOSE_NOTES_JS: &str = "const choice = document.querySelector('#compare-with-choices [data-beside=\"notes\"]');
    if (!choice) return { error: 'no Notes among the \"with\" choices' };
    choice.click();
    const column = document.querySelector('.chapter-main .notes-column');
    return { pressed: choice.getAttribute('aria-pressed'),
             empty: !!column && column.childElementCount === 0 && !column.dataset.drawn,
             tops: [...document.querySelectorAll('.chapter-main .chapter-text > .verse[data-verse]')].map((v) => v.offsetTop) };";

/// The column as laid out, against the notes added (`arguments[0]`: [id,
/// start, end] each): each note's box with its first verse's, the range's
/// rule against its verses, and the column against the text.
const COLUMN_JS: &str = "const added = arguments[0];
    const column = document.querySelector('.chapter-main .notes-column');
    const text = document.querySelector('.chapter-main .chapter-text').getBoundingClientRect();
    const verse = (n) => document.querySelector(`.chapter-main .chapter-text > .verse[data-verse=\"${n}\"]`);
    const notes = column ? [...column.querySelectorAll('.margin-note')].map((el) => {
        const id = Number(el.dataset.noteId);
        const [, start, end] = added.find(([i]) => i === id) || [];
        const box = el.getBoundingClientRect();
        const more = el.querySelector('.margin-note-more');
        return { id, start, end, top: box.top, bottom: box.bottom, left: box.left,
                 verse: start && verse(start) ? verse(start).getBoundingClientRect().top : null,
                 label: el.querySelector('.margin-note-ref').textContent,
                 body: el.querySelector('.margin-note-body').textContent,
                 folded: el.classList.contains('is-folded'), more: !!more && !more.hidden };
    }) : [];
    const rules = column ? [...column.querySelectorAll('.margin-rule')].map((el) => {
        const id = Number(el.dataset.noteId);
        const [, start, end] = added.find(([i]) => i === id) || [];
        const box = el.getBoundingClientRect();
        return { top: box.top, bottom: box.bottom,
                 from: verse(start) ? verse(start).getBoundingClientRect().top : null,
                 to: verse(end) ? verse(end).getBoundingClientRect().bottom : null };
    }) : [];
    const box = column ? column.getBoundingClientRect() : null;
    return { shown: window.gospelGetterMargin ? window.gospelGetterMargin.shown() : null,
             tag: column && column.tagName, label: column && column.getAttribute('aria-label'),
             heads: [...document.querySelectorAll('.chapter-main .compare-head')].map((h) => h.textContent.trim()),
             reference: document.getElementById('ref-text').textContent, title: document.title,
             text: { left: text.left, right: text.right }, column: box && { left: box.left, right: box.right },
             empty: column ? (column.querySelector('.notes-column-empty') || {}).textContent || null : null,
             notes, rules, innerWidth: window.innerWidth };";

/// What check 69 leaves to undo: its notes, and what it found.
#[derive(Debug, Default)]
struct Left {
    notes: Vec<i64>,
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
        found["readerMode"] = self
            .ui
            .js(
                "return document.documentElement.dataset.readerMode || 'off';",
                Vec::new(),
            )
            .await?;
        found["compare"] = self
            .ui
            .js(
                "return document.documentElement.dataset.compare || 'off';",
                Vec::new(),
            )
            .await?;
        left.found = Some(found.clone());
        if found["readerMode"] == "on" {
            self.ui.click("the exit button", "#reader-exit").await?;
            self.reading_mode_off("reading mode to end").await?;
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
        // Read again, with the notes.
        self.go_to("John 3", "John 3").await?;
        let _ = self.window_width(BESIDE_WINDOW).await;
        self.open_aa().await?;
        self.aa_choose("layout-choices", "layout", "verses").await?;
        self.aa_choose("theme-choices", "theme", "chi-rho").await?;
        self.close_with_escape("Aa", "aa-popover").await?;

        // Compare with the WEB, then Notes.
        self.open_translations().await?;
        self.ui
            .click(
                "Compare with: the WEB",
                "#compare-with-choices [data-value=\"web\"]",
            )
            .await?;
        if found["compare"] != "on" {
            self.ui.click("Compare on", "#compare-btn").await?;
        }
        self.ui
            .wait(
                "the KJV and the WEB side by side",
                "const heads = [...document.querySelectorAll('.chapter-main .compare-head')].map((h) => h.textContent.trim());
                 return [!document.querySelector('.notes-grid') && heads.length === 2, heads];",
            )
            .await?;
        let chosen = self.ui.js(CHOOSE_NOTES_JS, Vec::new()).await?;
        ensure!(chosen["error"].is_null(), "{chosen}");
        ensure!(chosen["pressed"] == "true", "Notes isn't pressed: {chosen}");
        ensure!(
            chosen["empty"] == true,
            "the column should be empty before its notes' frame: {chosen}"
        );
        let before = chosen["tops"].clone();
        ensure!(
            before.as_array().is_some_and(|t| t.len() == 36),
            "John 3 has 36 verses: {chosen}"
        );
        self.close_translations().await?;

        let placed = self.column_settled(&added).await?;
        let said_placed = self.column_checked(&placed, JOHN_NOTES.len())?;
        let after = self.ui.js(VERSE_TOPS_JS, Vec::new()).await?;
        ensure!(
            after == before,
            "the text moved when the notes came: {before} then {after}"
        );
        let first = before[0].as_f64().unwrap_or(f64::NAN);
        self.shot("screenshot: Compare with Notes, Chi Rho", BESIDE_SHOTS[0])
            .await;

        // A note opened from the column, edited in the card.
        let love = ids[4];
        self.ui
            .click(
                "John 3:16's note's label",
                &format!(".notes-column .margin-note[data-note-id=\"{love}\"] .margin-note-ref"),
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
                "the column to show the note edited",
                "const note = document.querySelector(`.notes-column .margin-note[data-note-id=\"${arguments[0]}\"]`);
                 const body = note ? note.querySelector('.margin-note-body').textContent : null;
                 return [!!body && body.endsWith(arguments[1].trim()) && note.style.top !== '', body];",
                vec![json!(love), json!(EDITED)],
            )
            .await?;
        self.two_frames().await?;
        let after_edit = self.ui.js(VERSE_TOPS_JS, Vec::new()).await?;
        ensure!(
            after_edit == before,
            "the text moved when a note was edited: {before} then {after_edit}"
        );
        let replaced = self.ui.js(COLUMN_JS, vec![json!(added)]).await?;
        self.column_checked(&replaced, JOHN_NOTES.len())?;
        self.shot(
            "screenshot: Compare with Notes, a note edited, Chi Rho",
            BESIDE_SHOTS[1],
        )
        .await;

        self.open_aa().await?;
        self.aa_choose("theme-choices", "theme", "vellum").await?;
        self.close_with_escape("Aa", "aa-popover").await?;
        self.two_frames().await?;
        self.shot("screenshot: Compare with Notes, Vellum", BESIDE_SHOTS[2])
            .await;

        // Every note gone: the column's one line, the text where it was.
        self.delete_notes(&ids).await?;
        left.notes.clear();
        self.go_to("John 3", "John 3").await?;
        let empty = self
            .ui
            .wait(
                "the column to say the chapter has no notes",
                "const column = document.querySelector('.chapter-main .notes-column');
                 const line = column && column.querySelector('.notes-column-empty');
                 return [!!line && !column.querySelector('.margin-note'), line ? line.textContent : null];",
            )
            .await?;
        let tops_empty = self.ui.js(VERSE_TOPS_JS, Vec::new()).await?;
        // The verses before the first noted one (John 3:2): a noted verse's
        // ✎ after its number is the reader's own mark, gone with the notes,
        // and can rewrap its verse.
        ensure!(
            tops_empty.get(0) == before.get(0),
            "the first verse isn't where it was with the notes: {before} then {tops_empty}"
        );
        Ok(format!(
            "Notes chosen: {said_placed}; every verse's offsetTop the same (verse 1 at {first}px) the moment \
             Notes was chosen (column empty), with the notes placed, after John 3:16's note was edited in the \
             card (the column then saying \"{}\"); verse 1's too with no notes left ({empty})",
            edited.as_str().unwrap_or("?"),
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

    /// Waits until the column holds `added`'s notes, each placed, and the
    /// page has settled: the column as laid out then.
    async fn column_settled(&self, added: &[Value]) -> Result<Value> {
        self.ui
            .wait_with(
                "the notes column to be drawn and placed",
                "const column = document.querySelector('.chapter-main .notes-column');
                 const notes = column ? [...column.querySelectorAll('.margin-note')] : [];
                 const shown = window.gospelGetterMargin ? window.gospelGetterMargin.shown() : null;
                 return [shown === 'column' && notes.length === arguments[0] && notes.every((n) => n.style.top !== ''),
                         { shown, notes: notes.length }];",
                vec![json!(added.len())],
            )
            .await?;
        self.two_frames().await?;
        self.ui.js(COLUMN_JS, vec![json!(added)]).await
    }

    /// Check 69's measures of the column.
    fn column_checked(&self, seen: &Value, count: usize) -> Result<String> {
        ensure!(seen["shown"] == "column", "no notes column: {seen}");
        ensure!(
            seen["tag"] == "SECTION" && seen["label"] == "My notes",
            "the column isn't the \"My notes\" region: {seen}"
        );
        ensure!(
            seen["heads"] == json!(["King James Version (KJV)", "My notes"]),
            "the column heads: {seen}"
        );
        ensure!(seen["reference"] == "John 3 · KJV", "the reference: {seen}");
        ensure!(
            seen["title"]
                .as_str()
                .is_some_and(|t| t.starts_with("John 3 · KJV — ")),
            "the title: {seen}"
        );
        let num = |v: &Value| v.as_f64().unwrap_or(f64::NAN);
        let (text_right, column_left) = (num(&seen["text"]["right"]), num(&seen["column"]["left"]));
        ensure!(
            column_left >= text_right,
            "the column is over the text: {seen}"
        );
        let notes = seen["notes"].as_array().cloned().unwrap_or_default();
        ensure!(notes.len() == count, "{} notes in the column", notes.len());
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
                ensure!(
                    (top - verse).abs() <= 2.0,
                    "a note isn't beside its first verse: {note}"
                );
                beside += 1;
            } else {
                ensure!(
                    (top - (previous_bottom + 8.0)).abs() <= 2.0,
                    "a note that would overlap isn't just below the one above: {note}"
                );
                stacked += 1;
            }
            previous_bottom = bottom;
        }
        let labels: Vec<&str> = notes.iter().filter_map(|n| n["label"].as_str()).collect();
        ensure!(
            labels == ["2", "3–5", "4", "7–8", "16"],
            "the labels: {labels:?}"
        );
        let rules = seen["rules"].as_array().cloned().unwrap_or_default();
        ensure!(rules.len() == 2, "a rule for each range: {seen}");
        for rule in &rules {
            ensure!(
                (num(&rule["top"]) - num(&rule["from"])).abs() <= 2.0
                    && (num(&rule["bottom"]) - num(&rule["to"])).abs() <= 2.0,
                "a range's rule isn't beside its verses: {rule}"
            );
        }
        let folded = notes.iter().filter(|n| n["folded"] == true).count();
        Ok(format!(
            "the \"My notes\" region beside the text (from x={column_left:.0}, the text ending at \
             {text_right:.0}), labels {labels:?}, {beside} beside their first verse and {stacked} just below \
             the note above, none above its verse or overlapping, both ranges' rules beside their verses, \
             {folded} folded past 10 lines, \"{}\"",
            seen["reference"].as_str().unwrap_or("?"),
        ))
    }

    /// Puts things back as check 69 found them: none of its notes, the look,
    /// Compare (with the WEB), reading mode and the default window.
    async fn beside_clean_up(&mut self, left: &Left) -> Result<()> {
        if !left.notes.is_empty() {
            self.delete_notes(&left.notes).await?;
        }
        let _ = self.window_width(BESIDE_WINDOW).await;
        let Some(found) = &left.found else {
            return Ok(());
        };
        let open = self
            .ui
            .js(
                "return !document.getElementById('translation-popover').hidden;",
                Vec::new(),
            )
            .await?;
        if open != true {
            self.open_translations().await?;
        }
        self.ui
            .click(
                "Compare with: the WEB",
                "#compare-with-choices [data-value=\"web\"]",
            )
            .await?;
        let compare = self
            .ui
            .js(
                "return document.documentElement.dataset.compare || 'off';",
                Vec::new(),
            )
            .await?;
        if compare != found["compare"] {
            self.ui.click("Compare as it was", "#compare-btn").await?;
        }
        self.close_translations().await?;
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
        if found["readerMode"] == "on" {
            self.reading_mode_on().await?;
        }
        Ok(())
    }
}
