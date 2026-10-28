//! Check 64: the Library's Notes tab's search (ui-leptos/src/library.rs),
//! on notes of its own added through the page's own IPC and deleted after.
//! The box is reached from the Notes tab with Tab; typing in it with the
//! keyboard filters the list once typing pauses, says how many of how many
//! notes it shows, and marks what it found in the theme's colours; Escape
//! clears it (every note again, the Library still open), and the next
//! Escape closes the Library, as before. Screenshots in Chi Rho and Vellum.

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
