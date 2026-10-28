//! Check 64: the Library's Notes tab's search (ui-leptos/src/library.rs),
//! on notes of its own added through the page's own IPC and deleted after.
//! The box is reached from the Notes tab with Tab; typing in it with the
//! keyboard filters the list once typing pauses, says how many of how many
//! notes it shows, and marks what it found in the theme's colours; Escape
//! clears it (every note again, the Library still open), and the next
//! Escape closes the Library, as before. Screenshots in Chi Rho and Vellum.
//!
//! Check 65: a note's draft survives the app quitting. At the end of the
//! second session a new note is typed with the keyboard on verse 2 and
//! left in the open editor, kept as a draft once typing pauses; the app is
//! quit (the WebDriver session ended) and started again, and + Add note on
//! that verse offers the draft, its text box empty until Restore puts it
//! in; Cancel, Discard then forgets it (screenshots of the offer in Chi
//! Rho and Vellum).
//!
//! Check 66: a note's Markdown (bold, italics, a list, code, a web
//! address) and the references typed in it (Rom 5:8, Ps 23), on notes of
//! its own: shown in the card, the Library and the margin (reading mode, a
//! Narrow column, where the screen has room for it); Copy as Markdown puts
//! the backend's text on the system clipboard (read back from outside the
//! page); a reference clicked in the card goes there (a chapter alone, at
//! its top), a step in the history (Alt+← comes back), and one clicked in
//! the Library goes there too (its verse selected), the Library closed. Screenshots of each in Chi Rho and Vellum. No web link is clicked:
//! that would open the system's browser.

use anyhow::{Result, ensure};
use serde_json::Value;
use thirtyfour::prelude::*;

use super::{DESCRIBE_JS, Run, VISIBLE_JS};

/// The screenshots check 64 takes (check 4 lists them).
pub(super) const NOTES_SHOTS: [&str; 2] =
    ["64-notes-search-chi-rho.png", "64-notes-search-vellum.png"];

/// Check 64's notes: [book, chapter, first verse, last verse, text].
const SEARCH_NOTES: [(u32, u32, u32, u32, &str); 3] = [
    (43, 1, 16, 16, "gui-check search: grace upon grace"),
    (44, 19, 1, 1, "gui-check search: Paul came to Éphèse"),
    (45, 5, 8, 8, "gui-check search: God commendeth his love"),
];

/// Asks the Library to load its notes again, as the card does after a
/// change (`bridge::LIBRARY_REFRESH_EVENT`): the notes check 64 adds and
/// deletes through the IPC are no change the page made itself.
const REFRESH_NOTES_JS: &str =
    "document.dispatchEvent(new CustomEvent('gospel-getter:library-refresh',
        { cancelable: true, detail: { lists: ['notes'] } }));
    return null;";

/// The Notes tab as the reader sees it: the box, the line under it, the
/// rows' references, the marks (their text, and whether each has a colour
/// of its own) and the focus.
const SEARCH_JS: &str = "const box = document.getElementById('note-search');
    const count = document.getElementById('note-search-count');
    const rows = [...document.querySelectorAll('#note-list .bookmark-item .bookmark-ref')].map((r) => r.textContent);
    const marks = [...document.querySelectorAll('#note-list mark')].map((m) => {
        const style = getComputedStyle(m);
        return { text: m.textContent, coloured: style.backgroundColor !== 'rgba(0, 0, 0, 0)'
            && style.backgroundColor !== 'transparent', shown: visible(m) };
    });
    const empty = document.querySelector('#note-list .bookmark-empty');
    return { value: box ? box.value : null, count: count && visible(count) ? count.textContent : '',
             rows, marks, empty: empty ? empty.textContent : null,
             open: !document.getElementById('library-panel').hidden, focus: describe(document.activeElement) };";

impl Run<'_> {
    /// Waits until the Notes tab satisfies `test` (a JavaScript expression
    /// on `seen`, what [`SEARCH_JS`] returns).
    async fn note_search_is(&self, what: &str, test: &str) -> Result<Value> {
        self.ui
            .wait(
                what,
                &format!(
                    "{VISIBLE_JS} {DESCRIBE_JS} const seen = (() => {{ {SEARCH_JS} }})();
                     return [!!({test}), seen];"
                ),
            )
            .await
    }

    /// Check 64: the Notes tab's search.
    pub(super) async fn notes_search(&mut self) -> Result<String> {
        let mut said = Vec::new();
        self.plain_reading().await?;
        self.expect_theme("chi-rho").await?;
        let notes: Vec<(u32, u32, u32, u32, String)> = SEARCH_NOTES
            .iter()
            .map(|&(book, chapter, start, end, body)| (book, chapter, start, end, body.to_owned()))
            .collect();
        let ids = self.add_notes(&notes).await?;
        let checked = self.notes_search_with_notes(&mut said).await;
        // Put back as found, whatever happened: no notes of its own, the
        // box empty, the Library closed, Chi Rho.
        let cleaned = self.notes_search_clean_up(&ids).await;
        checked?;
        cleaned?;
        Ok(said.join("; "))
    }

    async fn notes_search_with_notes(&mut self, said: &mut Vec<String>) -> Result<()> {
        self.ui.js(REFRESH_NOTES_JS, Vec::new()).await?;
        self.library_tab("notes").await?;
        let all = self
            .note_search_is(
                "the Library's Notes to list check 64's notes, the box empty",
                "seen.value === '' && ['John 1:16', 'Acts 19:1', 'Romans 5:8'].every((r) => seen.rows.includes(r))",
            )
            .await?;
        let total = all["rows"].as_array().map_or(0, Vec::len);

        // From the tab, Tab reaches the box first.
        self.ui
            .press("press Tab (from the Notes tab)", Key::Tab)
            .await?;
        self.note_search_is(
            "focus to be in the search box",
            "seen.focus === '#note-search'",
        )
        .await?;
        said.push("Tab from the Notes tab reaches the search box".to_owned());

        self.ui.press("type “EPHESE” in the box", "EPHESE").await?;
        let found = self
            .note_search_is(
                "the list to show Acts 19:1 alone, marked, with its count",
                &format!(
                    "JSON.stringify(seen.rows) === JSON.stringify(['Acts 19:1'])
                     && seen.count === '1 of {total} notes'
                     && seen.marks.length === 1 && seen.marks[0].text === 'Éphèse'
                     && seen.marks[0].coloured && seen.marks[0].shown"
                ),
            )
            .await?;
        said.push(format!(
            "typing EPHESE: {} row ({}), \"{}\", \"{}\" marked in a colour of its own",
            found["rows"].as_array().map_or(0, Vec::len),
            found["rows"][0].as_str().unwrap_or_default(),
            found["count"].as_str().unwrap_or_default(),
            found["marks"][0]["text"].as_str().unwrap_or_default(),
        ));
        self.shot(
            "screenshot: the Notes tab searched, Chi Rho",
            NOTES_SHOTS[0],
        )
        .await;

        // A word more that nothing has: said plainly.
        self.ui.press("type “ zzz” after it", " zzz").await?;
        let none = self
            .note_search_is(
                "the list to say no notes match",
                &format!(
                    "seen.rows.length === 0 && seen.empty === 'No notes match “EPHESE zzz”.'
                     && seen.count === '0 of {total} notes'"
                ),
            )
            .await?;
        said.push(format!(
            "\"EPHESE zzz\": \"{}\" (\"{}\")",
            none["empty"].as_str().unwrap_or_default(),
            none["count"].as_str().unwrap_or_default()
        ));

        self.ui.press("Escape (clear the box)", Key::Escape).await?;
        let cleared = self
            .note_search_is(
                "Escape to clear the box: every note again, the Library still open",
                &format!(
                    "seen.value === '' && seen.rows.length === {total} && seen.count === '' && seen.open
                     && seen.focus === '#note-search'"
                ),
            )
            .await?;
        said.push(format!(
            "Escape cleared it: {} notes again, the Library still open",
            cleared["rows"].as_array().map_or(0, Vec::len)
        ));
        // By its reference: the book begun, and the chapter.
        self.ui.press("type “rom 5”", "rom 5").await?;
        self.note_search_is(
            "the list to show Romans 5:8, its book and chapter marked",
            "JSON.stringify(seen.rows) === JSON.stringify(['Romans 5:8'])
             && JSON.stringify(seen.marks.map((m) => m.text)) === JSON.stringify(['Rom', '5'])",
        )
        .await?;
        said.push("\"rom 5\": Romans 5:8, \"Rom\" and \"5\" marked in its reference".to_owned());

        // Vellum, then back.
        self.plain_reading().await?;
        self.open_aa().await?;
        self.aa_choose("theme-choices", "theme", "vellum").await?;
        self.close_with_escape("Aa", "aa-popover").await?;
        self.library_tab("notes").await?;
        self.note_search_is(
            "the search kept: Romans 5:8 still shown",
            "JSON.stringify(seen.rows) === JSON.stringify(['Romans 5:8']) && seen.marks.every((m) => m.coloured)",
        )
        .await?;
        self.shot("screenshot: the Notes tab searched, Vellum", NOTES_SHOTS[1])
            .await;
        self.open_aa().await?;
        self.aa_choose("theme-choices", "theme", "chi-rho").await?;
        self.close_with_escape("Aa", "aa-popover").await?;
        said.push("kept while the Library was closed; marked in Vellum too".to_owned());

        // Escape, Escape: the box, then the Library.
        self.library_tab("notes").await?;
        self.ui
            .click("click in the search box", "#note-search")
            .await?;
        self.ui.press("Escape (clear the box)", Key::Escape).await?;
        self.note_search_is(
            "Escape to clear the box, the Library still open",
            "seen.value === '' && seen.open",
        )
        .await?;
        self.ui
            .press("Escape again (close the Library)", Key::Escape)
            .await?;
        let closed = self
            .note_search_is("the next Escape to close the Library", "!seen.open")
            .await?;
        ensure!(closed["value"] == "", "the box is empty: {closed}");
        said.push("the next Escape closed the Library, as before".to_owned());
        Ok(())
    }

    /// Deletes check 64's notes, empties the box, closes the Library and
    /// puts Chi Rho back.
    async fn notes_search_clean_up(&mut self, ids: &[i64]) -> Result<()> {
        if !ids.is_empty() {
            self.delete_notes(ids).await?;
        }
        self.ui
            .js(
                "const box = document.getElementById('note-search');
                 if (box && box.value) { box.value = ''; box.dispatchEvent(new Event('input', { bubbles: true })); }
                 return null;",
                Vec::new(),
            )
            .await?;
        self.ui.js(REFRESH_NOTES_JS, Vec::new()).await?;
        self.plain_reading().await?;
        if self.expect_theme("chi-rho").await.is_err() {
            self.open_aa().await?;
            self.aa_choose("theme-choices", "theme", "chi-rho").await?;
            self.close_with_escape("Aa", "aa-popover").await?;
        }
        Ok(())
    }
}

/// The screenshots check 66 takes (check 4 lists them; the margin's only
/// where there's room for it).
pub(super) const MARKDOWN_SHOTS: [&str; 6] = [
    "66-note-markdown-card-chi-rho.png",
    "66-note-markdown-library-chi-rho.png",
    "66-note-markdown-margin-chi-rho.png",
    "66-note-markdown-card-vellum.png",
    "66-note-markdown-library-vellum.png",
    "66-note-markdown-margin-vellum.png",
];

/// The text check 65 types into a new note on verse 2, and looks for
/// after the app is quit and started again.
const DRAFT_TEXT: &str = "gui-check draft: typed, then the app was quit";

/// Check 65's screenshots, in Chi Rho and Vellum.
const DRAFT_SHOTS: [&str; 2] = [
    "65-note-draft-offer-chi-rho.png",
    "65-note-draft-offer-vellum.png",
];

/// Shows the page in the theme `arguments[0]` for a screenshot, as the
/// pre-paint script would (Chi Rho, the default, has no attribute), without
/// saving it; returns the theme it had, to be put back the same way. The
/// note editor is open, so Aa can't be.
const SHOW_THEME_JS: &str = "const had = document.documentElement.dataset.theme || 'chi-rho';
    if (arguments[0] === 'chi-rho') delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = arguments[0];
    return had;";

/// The verse check 65's draft is on.
const DRAFT_VERSE: u32 = 2;

/// The drafts the page keeps in local storage (`note_text::DRAFTS_KEY`):
/// their keys and texts.
const DRAFTS_JS: &str = "const kept = JSON.parse(localStorage.getItem('gospel-getter-note-drafts') || '{\"drafts\":[]}');
    return kept.drafts.map((d) => [d.key, d.text]);";

/// Where check 65 left its draft: the chapter it's in.
pub(super) struct DraftLeft {
    pub(super) heading: String,
}

impl Run<'_> {
    /// Check 65, before the app quits: a new note on verse 2 of the
    /// chapter being read, typed with the keyboard and left in the open
    /// editor, kept as a draft once typing pauses.
    pub(super) async fn draft_typed(&mut self) -> Result<DraftLeft> {
        self.plain_reading().await?;
        let heading = self.heading().await?;
        self.select(DRAFT_VERSE).await?;
        let ui = &self.ui;
        ui.press("press N (write a note)", "n").await?;
        ui.wait(
            "the note editor to show, focus in its text box",
            &format!(
                "{VISIBLE_JS} return [visible(document.getElementById('note-panel'))
                     && document.activeElement && document.activeElement.id === 'note-input', null];"
            ),
        )
        .await?;
        ui.press("type the note", DRAFT_TEXT).await?;
        ui.wait_with(
            "the text to be kept as a draft once typing pauses",
            &format!(
                "const kept = (() => {{ {DRAFTS_JS} }})();
                 return [kept.some(([, text]) => text === arguments[0]), kept];"
            ),
            vec![serde_json::json!(DRAFT_TEXT)],
        )
        .await?;
        Ok(DraftLeft { heading })
    }

    /// Check 65, in the app started again: + Add note on the same verse
    /// offers the draft (the text box empty till Restore), Restore puts it
    /// in, and Cancel, Discard forgets it.
    pub(super) async fn draft_offered(&mut self, left: &DraftLeft) -> Result<String> {
        let ui = &self.ui;
        ui.wait_within(
            "the page to boot again",
            "const h = document.querySelector('.chapter-main .chapter-heading');
             return [!!h && !!document.getElementById('note-input'), h ? h.textContent.trim() : null];",
            Vec::new(),
            Some(self.config.session_timeout),
        )
        .await?;
        let here = self.heading().await?;
        ensure!(
            here == left.heading,
            "the app started again at {here}, not {} where the draft was typed",
            left.heading
        );
        self.select(DRAFT_VERSE).await?;
        let ui = &self.ui;
        ui.press("press N (write a note)", "n").await?;
        let offer = ui
            .wait(
                "the editor to offer the draft, its text box empty",
                &format!(
                    "{VISIBLE_JS}
                     const offer = document.getElementById('note-draft');
                     const input = document.getElementById('note-input');
                     const text = document.getElementById('note-draft-text');
                     return [visible(offer) && input.value === '' && /^You have unsaved text from /.test(text.textContent),
                         {{ shown: visible(offer), said: text.textContent, value: input.value }}];"
                ),
            )
            .await?;
        let had = self
            .ui
            .js(SHOW_THEME_JS, vec![serde_json::json!("chi-rho")])
            .await?;
        self.shot(
            "screenshot: the note editor offering a draft, Chi Rho",
            DRAFT_SHOTS[0],
        )
        .await;
        self.ui
            .js(SHOW_THEME_JS, vec![serde_json::json!("vellum")])
            .await?;
        self.shot(
            "screenshot: the note editor offering a draft, Vellum",
            DRAFT_SHOTS[1],
        )
        .await;
        self.ui.js(SHOW_THEME_JS, vec![had]).await?;
        let ui = &self.ui;
        ui.click("click Restore", "#note-draft-restore").await?;
        let restored = ui
            .wait_with(
                "the draft to be in the text box",
                "const input = document.getElementById('note-input');
                 return [input.value === arguments[0] && document.activeElement === input, input.value];",
                vec![serde_json::json!(DRAFT_TEXT)],
            )
            .await?;
        // Put back: Cancel, Discard, and the draft is gone.
        ui.click("click Cancel", "#note-cancel").await?;
        ui.click("click Discard", "#note-discard-yes").await?;
        ui.wait(
            "the editor to close and the draft to be forgotten",
            &format!(
                "const kept = (() => {{ {DRAFTS_JS} }})();
                 return [document.getElementById('note-overlay').hidden && kept.length === 0, kept];"
            ),
        )
        .await?;
        Ok(format!(
            "typed in a new note on {}:{DRAFT_VERSE}, the app quit with the editor open and started again: \
             + Add note there said \"{}\" with the text box empty; Restore put \"{}\" in it; Cancel, Discard forgot it",
            left.heading,
            offer["said"].as_str().unwrap_or_default(),
            restored.as_str().unwrap_or_default()
        ))
    }
}

/// Check 66's note, on John 3:16: bold, italics, a list, code, two
/// references and a web address.
const MARKDOWN_NOTE: &str = "**For God so loved** the *world*:\n- see Rom 5:8\n- and Ps 23\n\n`whosoever` https://example.com";

/// The note's text where it shows (`arguments[0]`: a selector for its
/// text): its bold, italics, list items and code, its references and its
/// web links.
const MARKDOWN_JS: &str = "const body = document.querySelector(arguments[0]);
    if (!body) return null;
    const texts = (selector) => [...body.querySelectorAll(selector)].map((el) => el.textContent);
    return { strong: texts('strong'), em: texts('em'), items: texts('li'), code: texts('code'),
             refs: texts('a.note-ref-link'), web: [...body.querySelectorAll('a.note-web-link')].map((a) => a.getAttribute('href')),
             shown: visible(body) };";

/// Starts asking for note `arguments[0]` as Markdown in the translation
/// being read, through the page's own IPC, kept in
/// `window.__guiCheckMarkdown` for [`MARKDOWN_DONE_JS`].
const MARKDOWN_ASK_JS: &str = "const s = window.__guiCheckMarkdown = { done: false, text: null, error: null };
    const code = document.querySelector('.chapter-main .verse[data-translation]').dataset.translation;
    window.__TAURI__.core.invoke('get_note_markdown', { id: arguments[0], translationCode: code })
        .then((text) => { s.text = text; }, (e) => { s.error = String(e); })
        .finally(() => { s.done = true; });
    return null;";

const MARKDOWN_DONE_JS: &str = "const s = window.__guiCheckMarkdown;
    return [!!s && s.done, s ? { text: s.text, error: s.error } : null];";

/// Scrolls the note whose text `arguments[0]` finds to the middle of what
/// holds it, for a screenshot (the note, not its text, which is cut to its
/// lines and would scroll within itself).
const SCROLL_TO_JS: &str = "const el = document.querySelector(arguments[0]);
    const note = el && (el.closest('.note-item, .note-card, .margin-note') || el);
    if (note) note.scrollIntoView({ block: 'center' });
    return null;";

/// A string the page reported, as text.
fn text(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

/// Strings the page reported, quoted and joined.
fn texts(value: &Value) -> String {
    value
        .as_array()
        .map(|all| {
            all.iter()
                .map(|v| format!("\"{}\"", text(v)))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

/// What check 66 changes, to put back after.
struct MarkdownLeft {
    notes: Vec<i64>,
    found: Value,
}

impl Run<'_> {
    /// Waits for the note's text at `selector` to show its Markdown.
    async fn markdown_shown(&self, what: &str, selector: &str, items: bool) -> Result<Value> {
        self.ui
            .wait_with(
                what,
                &format!(
                    "{VISIBLE_JS} const seen = (() => {{ {MARKDOWN_JS} }})();
                     return [!!seen && seen.shown && seen.strong[0] === 'For God so loved' && seen.em[0] === 'world'
                         && JSON.stringify(seen.refs) === JSON.stringify(['Rom 5:8', 'Ps 23'])
                         && JSON.stringify(seen.web) === JSON.stringify(['https://example.com'])
                         && (!arguments[1] || JSON.stringify(seen.items) === JSON.stringify(['see Rom 5:8', 'and Ps 23'])),
                         seen];"
                ),
                vec![serde_json::json!(selector), serde_json::json!(items)],
            )
            .await
    }

    /// Clicks the reference `text` in the note at `selector`.
    async fn click_reference(&self, selector: &str, text: &str) -> Result<()> {
        let marked = self
            .ui
            .js(
                "document.querySelectorAll('[data-gui-check-target]').forEach((el) => el.removeAttribute('data-gui-check-target'));
                 const link = [...document.querySelectorAll(arguments[0] + ' a.note-ref-link')]
                     .find((a) => a.textContent === arguments[1]);
                 if (!link) return false;
                 // Its note into view, not the link within it: a note's text
                 // is cut to its lines (overflow hidden), which scrolling
                 // the link into view would scroll too.
                 (link.closest('.note-item, .note-card, .margin-note') || link).scrollIntoView({ block: 'center' });
                 link.setAttribute('data-gui-check-target', '');
                 return true;",
                vec![serde_json::json!(selector), serde_json::json!(text)],
            )
            .await?;
        ensure!(
            marked == Value::Bool(true),
            "no reference {text} in {selector}"
        );
        self.ui
            .click(
                &format!("click the reference {text}"),
                "[data-gui-check-target]",
            )
            .await
    }

    /// Waits for `heading` to be read with `selected` selected.
    async fn read_at(&self, heading: &str, selected: &[u32]) -> Result<()> {
        self.ui
            .wait_with(
                &format!("{heading} to be read, {selected:?} selected"),
                "const h = document.querySelector('.chapter-main .chapter-heading');
                 const selected = [...document.querySelectorAll('.chapter-main .verse-selected[data-translation]')]
                     .map((v) => Number(v.dataset.verse));
                 return [!!h && h.textContent.trim() === arguments[0]
                     && JSON.stringify(selected) === JSON.stringify(arguments[1]),
                     { heading: h && h.textContent.trim(), selected }];",
                vec![serde_json::json!(heading), serde_json::json!(selected)],
            )
            .await?;
        Ok(())
    }

    /// Check 66: a note's Markdown and references in the card, the
    /// Library and the margin; Copy as Markdown.
    pub(super) async fn note_markdown(&mut self) -> Result<String> {
        let mut said = Vec::new();
        self.plain_reading().await?;
        self.expect_theme("chi-rho").await?;
        let mut found = self.look().await?;
        found["readerMode"] = self
            .ui
            .js(
                "return document.documentElement.dataset.readerMode || 'off';",
                Vec::new(),
            )
            .await?;
        let notes = self
            .add_notes(&[(43, 3, 16, 16, MARKDOWN_NOTE.to_owned())])
            .await?;
        // Listed in the Library too: it was no change the page made.
        self.ui.js(REFRESH_NOTES_JS, Vec::new()).await?;
        let left = MarkdownLeft { notes, found };
        let checked = self.note_markdown_with(&left, &mut said).await;
        let cleaned = self.note_markdown_clean_up(&left).await;
        checked?;
        cleaned?;
        Ok(said.join("; "))
    }

    async fn note_markdown_with(
        &mut self,
        left: &MarkdownLeft,
        said: &mut Vec<String>,
    ) -> Result<()> {
        let id = *left
            .notes
            .first()
            .ok_or_else(|| anyhow::anyhow!("no note added"))?;
        // Its own note's text, wherever it shows (other checks' notes are
        // in the Library too).
        let card_body = format!(".verse-notes .note-card[data-note-id=\"{id}\"] .note-card-body");
        let row_preview = format!("#note-list .note-item[data-note-id=\"{id}\"] .note-preview");
        let margin_body = format!(
            ".chapter-main .margin-notes .margin-note[data-note-id=\"{id}\"] .margin-note-body"
        );
        // The card.
        self.go_to("John 3", "John 3").await?;
        self.select(16).await?;
        let card = self
            .markdown_shown("the card's note to show its Markdown", &card_body, true)
            .await?;
        said.push(format!(
            "the card: bold \"{}\", italics \"{}\", a list ({}), code \"{}\", references {} as links, {} a web link",
            text(&card["strong"][0]),
            text(&card["em"][0]),
            texts(&card["items"]),
            text(&card["code"][0]),
            texts(&card["refs"]),
            text(&card["web"][0])
        ));
        self.shot(
            "screenshot: a note's Markdown in the card, Chi Rho",
            MARKDOWN_SHOTS[0],
        )
        .await;

        // Copy as Markdown: the clipboard holds the backend's text.
        self.ui
            .js(MARKDOWN_ASK_JS, vec![serde_json::json!(id)])
            .await?;
        let asked = self
            .ui
            .wait("the note's Markdown from the backend", MARKDOWN_DONE_JS)
            .await?;
        let Some(want) = asked["text"].as_str().map(str::to_owned) else {
            anyhow::bail!("the note's Markdown couldn't be read: {asked}");
        };
        ensure!(
            want.starts_with("## John 3:16\n\n> **16** ") && want.ends_with(MARKDOWN_NOTE),
            "the note's Markdown: {want:?}"
        );
        self.ui
            .click("click Copy as Markdown", ".verse-notes .note-copy")
            .await?;
        self.expect_clipboard("Copy as Markdown", &want).await?;
        said.push(format!(
            "Copy as Markdown: the clipboard holds the backend's text ({} lines: the heading, the quote, the note as written)",
            want.lines().count()
        ));

        // A reference: a step in the history, as Go to is; a chapter
        // alone opens at its top, nothing selected.
        self.select(16).await?;
        self.click_reference(&card_body, "Ps 23").await?;
        self.read_at("Psalms 23", &[]).await?;
        self.chord("press Alt+← (back)", &[Key::Alt], Key::Left)
            .await?;
        self.ui
            .wait(
                "Back to return to John 3",
                "const h = document.querySelector('.chapter-main .chapter-heading');
                 return [!!h && h.textContent.trim() === 'John 3', h && h.textContent.trim()];",
            )
            .await?;
        said.push(
            "its reference Ps 23 clicked: Psalms 23 read from its top; Alt+← back to John 3"
                .to_owned(),
        );

        // The Library.
        self.library_tab("notes").await?;
        let row = self
            .markdown_shown(
                "the Library's row to show the note's start as Markdown",
                &row_preview,
                false,
            )
            .await?;
        said.push(format!(
            "the Library: bold \"{}\", references {} as links, on one line",
            text(&row["strong"][0]),
            texts(&row["refs"])
        ));
        self.ui
            .js(SCROLL_TO_JS, vec![serde_json::json!(row_preview)])
            .await?;
        self.shot(
            "screenshot: a note's Markdown in the Library, Chi Rho",
            MARKDOWN_SHOTS[1],
        )
        .await;
        self.click_reference(&row_preview, "Rom 5:8").await?;
        self.read_at("Romans 5", &[8]).await?;
        self.ui
            .wait(
                "the Library to have closed",
                "return [document.getElementById('library-panel').hidden, null];",
            )
            .await?;
        said.push(
            "its reference Rom 5:8 clicked from the Library: the Library closed, Romans 5 read, verse 8 selected"
                .to_owned(),
        );

        // The margin: reading mode, a Narrow column in the app's window.
        self.go_to("John 3", "John 3").await?;
        let _ = self.window_width(1280).await;
        self.open_aa().await?;
        self.aa_choose("width-choices", "width", "narrow").await?;
        self.aa_choose("layout-choices", "layout", "verses").await?;
        self.close_with_escape("Aa", "aa-popover").await?;
        self.reading_mode_on().await?;
        let shown = self
            .ui
            .wait(
                "the margin (or its markers) to be placed",
                "const m = window.gospelGetterMargin;
                 const shown = m ? m.shown() : null;
                 return [shown === 'margin' || shown === 'markers', shown];",
            )
            .await?;
        if shown == "margin" {
            self.ui
                .js(
                    "const v = document.querySelector('.chapter-main .chapter-text > .verse[data-verse=\"16\"]');
                     window.scrollBy(0, v.getBoundingClientRect().top - 120); return null;",
                    Vec::new(),
                )
                .await?;
            let margin = self
                .markdown_shown("the margin's note to show its Markdown", &margin_body, true)
                .await?;
            said.push(format!(
                "the margin: bold \"{}\", a list ({}), references {} as links",
                text(&margin["strong"][0]),
                texts(&margin["items"]),
                texts(&margin["refs"])
            ));
            self.shot(
                "screenshot: a note's Markdown in the margin, Chi Rho",
                MARKDOWN_SHOTS[2],
            )
            .await;
            self.open_aa().await?;
            self.aa_choose("theme-choices", "theme", "vellum").await?;
            self.close_with_escape("Aa", "aa-popover").await?;
            self.shot(
                "screenshot: a note's Markdown in the margin, Vellum",
                MARKDOWN_SHOTS[5],
            )
            .await;
        } else {
            said.push(
                "the margin: not on this screen (too narrow for a 1280 px window; markers instead)"
                    .to_owned(),
            );
            self.open_aa().await?;
            self.aa_choose("theme-choices", "theme", "vellum").await?;
            self.close_with_escape("Aa", "aa-popover").await?;
        }
        self.ui.click("the exit button", "#reader-exit").await?;
        self.reading_mode_off("reading mode to end").await?;
        // The card and the Library in Vellum.
        self.select(16).await?;
        self.markdown_shown(
            "the card's note to show its Markdown (Vellum)",
            &card_body,
            true,
        )
        .await?;
        self.shot(
            "screenshot: a note's Markdown in the card, Vellum",
            MARKDOWN_SHOTS[3],
        )
        .await;
        self.library_tab("notes").await?;
        self.ui
            .js(SCROLL_TO_JS, vec![serde_json::json!(row_preview)])
            .await?;
        self.shot(
            "screenshot: a note's Markdown in the Library, Vellum",
            MARKDOWN_SHOTS[4],
        )
        .await;
        Ok(())
    }

    /// Deletes check 66's note and puts the settings, reading mode and
    /// window back as it found them.
    async fn note_markdown_clean_up(&mut self, left: &MarkdownLeft) -> Result<()> {
        if !left.notes.is_empty() {
            self.delete_notes(&left.notes).await?;
        }
        self.ui.js(REFRESH_NOTES_JS, Vec::new()).await?;
        self.plain_reading().await?;
        let _ = self.window_width(1280).await;
        let mode = self
            .ui
            .js(
                "return document.documentElement.dataset.readerMode || 'off';",
                Vec::new(),
            )
            .await?;
        if mode == "on" && left.found["readerMode"] != "on" {
            self.ui.click("the exit button", "#reader-exit").await?;
            self.reading_mode_off("reading mode to end").await?;
        }
        let settings = &left.found["settings"];
        self.open_aa().await?;
        for (group, name) in [
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
}
