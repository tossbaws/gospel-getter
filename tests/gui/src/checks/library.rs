//! Checks 14 to 16: the Library's three lists, which the Rust pieces
//! (ui-leptos) took over from the page's JavaScript, each through the real
//! UI. Each list fills as the reader marks a verse (the selection card's
//! Bookmark, a highlighter color, a note written with N), opens a row (the
//! reader jumps to its passage, selected, and the Library gets out of the
//! way, focus back on its button) and removes one (the row goes, focus
//! stays in the list where it always did, and the verse's mark in the
//! reader goes with it). A note has no × in the list: its Edit opens the
//! note editor, whose Delete removes it. Screenshots in Chi Rho here, and
//! in Vellum with check 4's.

use anyhow::{Result, bail};
use serde_json::{Value, json};
use thirtyfour::prelude::*;

use super::{DESCRIBE_JS, Run, VISIBLE_JS};

/// The Library's screenshots: each list in Chi Rho (checks 14 to 16), and
/// in Vellum (with check 4's).
pub(super) const LIBRARY_SHOTS: [&str; 6] = [
    "14-library-bookmarks-chi-rho.png",
    "15-library-highlights-chi-rho.png",
    "16-library-notes-chi-rho.png",
    "14-library-bookmarks-vellum.png",
    "15-library-highlights-vellum.png",
    "16-library-notes-vellum.png",
];

/// The note check 16 writes, and deletes.
const NOTE_TEXT: &str = "gui-check Library note: and God saw the light";

/// Selects verse `arguments[0]` of the chapter being read.
fn verse(n: u32) -> String {
    format!(".chapter-main .verse[data-translation][data-verse=\"{n}\"]")
}

/// What the reader sees of the list `arguments[0]` ("bookmark",
/// "highlight" or "note"): its rows' text, its count, whether the Library
/// and the tab are showing, and the focus.
const LIST_JS: &str = "const list = arguments[0];
    const ul = document.getElementById(list + '-list');
    const rows = ul ? [...ul.querySelectorAll('.bookmark-item')].map((li) => {
        const ref = li.querySelector('.bookmark-ref').textContent;
        const color = li.querySelector('.highlight-color');
        return color ? ref + ' ' + color.textContent : ref;
    }) : null;
    const empty = ul && ul.querySelector('.bookmark-empty');
    const count = document.getElementById(list + '-count');
    return { rows, empty: empty ? empty.textContent : null, count: count ? count.textContent : null,
             open: !document.getElementById('library-panel').hidden,
             focus: describe(document.activeElement) };";

/// The reader's marks on verse `arguments[0]` of the chapter being read.
const MARKS_JS: &str = "const v = document.querySelector(
        `.chapter-main .verse[data-translation][data-verse=\"${arguments[0]}\"]`);
    return v ? { bookmarked: v.classList.contains('is-bookmarked'), highlight: v.dataset.highlight || null,
                 noted: v.classList.contains('has-notes') } : null;";

/// Where the reader is and what's selected, and the focus.
const PLACE_JS: &str = "const heading = document.querySelector('.chapter-main .chapter-heading');
    return { heading: heading ? heading.textContent.trim() : null,
             selected: [...document.querySelectorAll('.chapter-main .verse-selected[data-translation]')]
                 .map((v) => Number(v.dataset.verse)),
             library: document.getElementById('library-panel').hidden ? 'closed' : 'open',
             focus: describe(document.activeElement) };";

impl Run<'_> {
    /// Back to plain reading: no Library, no selection.
    pub(super) async fn plain_reading(&self) -> Result<()> {
        let ui = &self.ui;
        let open = ui
            .js(
                "return !document.getElementById('library-panel').hidden;",
                Vec::new(),
            )
            .await?;
        if open == Value::Bool(true) {
            ui.click("close the Library", "#library-close").await?;
            ui.wait(
                "the Library to close",
                "return [document.getElementById('library-panel').hidden, null];",
            )
            .await?;
        }
        let selected = ui
            .js(
                "return !!document.querySelector('.verse-selected');",
                Vec::new(),
            )
            .await?;
        if selected == Value::Bool(true) {
            ui.press("press Escape (clear the verse selection)", Key::Escape)
                .await?;
            ui.wait(
                "the verse selection to clear",
                "return [!document.querySelector('.verse-selected'), null];",
            )
            .await?;
        }
        Ok(())
    }

    /// The chapter being read, as its heading says.
    pub(super) async fn heading(&self) -> Result<String> {
        let heading = self
            .ui
            .js(
                "const h = document.querySelector('.chapter-main .chapter-heading');
                 return h ? h.textContent.trim() : null;",
                Vec::new(),
            )
            .await?;
        match heading.as_str() {
            Some(heading) if !heading.is_empty() => Ok(heading.to_owned()),
            _ => bail!("no chapter heading"),
        }
    }

    /// Turns the chapter with `key` (→ or ←) from `from`, in the reading
    /// view, and returns the chapter it turned to.
    async fn turn_from(&self, key: Key, from: &str) -> Result<String> {
        let ui = &self.ui;
        // The heading is under the sticky bar once the page has scrolled
        // (a note's selection, say): back to the top of the chapter first.
        ui.js("window.scrollTo(0, 0); return null;", Vec::new())
            .await?;
        ui.click(
            "click the chapter heading (focus off any control)",
            ".chapter-main .chapter-heading",
        )
        .await?;
        ui.press(&format!("press {key:?} (turn the chapter)"), key)
            .await?;
        let seen = ui
            .wait_with(
                &format!("another chapter than {from} to be read"),
                "const h = document.querySelector('.chapter-main .chapter-heading');
                 const text = h ? h.textContent.trim() : null;
                 return [!!text && text !== arguments[0], text];",
                vec![json!(from)],
            )
            .await?;
        Ok(seen.as_str().unwrap_or_default().to_owned())
    }

    /// Clicks verse `n` and waits for the selection card. Any selection
    /// goes first (Escape), as its card may cover the verse.
    async fn select(&self, n: u32) -> Result<()> {
        self.plain_reading().await?;
        let ui = &self.ui;
        ui.click(&format!("select verse {n}"), &verse(n)).await?;
        ui.wait_with(
            &format!("verse {n} to be selected, with its card"),
            &format!(
                "{VISIBLE_JS}
                 const v = document.querySelector(`.chapter-main .verse[data-translation][data-verse=\"${{arguments[0]}}\"]`);
                 return [!!v && v.classList.contains('verse-selected')
                     && visible(document.querySelector('.verse-actions')), null];"
            ),
            vec![json!(n)],
        )
        .await?;
        Ok(())
    }

    /// Screenshot `name` of the Library, once the toast has gone.
    async fn library_shot(&mut self, what: &str, name: &'static str) -> Result<()> {
        self.ui
            .wait(
                "the toast to go",
                "return [document.getElementById('data-status').classList.contains('is-hidden'), null];",
            )
            .await?;
        self.shot(what, name).await;
        Ok(())
    }

    /// The list `list` as the reader sees it.
    async fn list(&self, list: &str) -> Result<Value> {
        self.ui
            .js(&format!("{DESCRIBE_JS} {LIST_JS}"), vec![json!(list)])
            .await
    }

    /// Waits until the list `list`'s rows are `rows`, its count theirs.
    async fn list_is(&self, what: &str, list: &str, rows: &[String]) -> Result<Value> {
        self.ui
            .wait_with(
                what,
                &format!(
                    "{DESCRIBE_JS} const seen = (() => {{ {LIST_JS} }})();
                     const want = arguments[1];
                     return [JSON.stringify(seen.rows) === JSON.stringify(want)
                         && seen.count === String(want.length), seen];"
                ),
                vec![json!(list), json!(rows)],
            )
            .await
    }

    /// Waits until verse `n`'s marks have `mark` (`bookmarked`,
    /// `highlight` or `noted`) equal to `value`.
    async fn mark_is(&self, what: &str, n: u32, mark: &str, value: Value) -> Result<()> {
        self.ui
            .wait_with(
                what,
                &format!(
                    "const marks = (() => {{ {MARKS_JS} }})();
                     return [!!marks && JSON.stringify(marks[arguments[1]]) === JSON.stringify(arguments[2]), marks];"
                ),
                vec![json!(n), json!(mark), value],
            )
            .await?;
        Ok(())
    }

    /// Opens the Library (if it isn't) at the tab `tab`, and waits for its
    /// panel to show.
    pub(super) async fn library_tab(&self, tab: &str) -> Result<()> {
        let ui = &self.ui;
        let open = ui
            .js(
                "return !document.getElementById('library-panel').hidden;",
                Vec::new(),
            )
            .await?;
        if open != Value::Bool(true) {
            ui.click("open the Library", "#library-toggle").await?;
            ui.wait(
                "the Library to show, focus on its chosen tab",
                &format!(
                    "{VISIBLE_JS}
                     const tab = document.activeElement;
                     return [visible(document.getElementById('library-panel')) && !!tab
                         && tab.getAttribute('role') === 'tab' && tab.getAttribute('aria-selected') === 'true',
                         tab ? tab.id : null];"
                ),
            )
            .await?;
        }
        ui.settled("the Library to finish sliding in").await?;
        ui.click(
            &format!("the Library's {tab} tab"),
            &format!("#library-tab-{tab}"),
        )
        .await?;
        ui.wait_with(
            &format!("the {tab} tab to be chosen, its panel shown"),
            &format!(
                "{VISIBLE_JS}
                 const tab = document.getElementById('library-tab-' + arguments[0]);
                 const panel = document.getElementById(tab.getAttribute('aria-controls'));
                 return [tab.getAttribute('aria-selected') === 'true' && tab.tabIndex === 0 && visible(panel),
                     {{ selected: tab.getAttribute('aria-selected'), panel: visible(panel) }}];"
            ),
            vec![json!(tab)],
        )
        .await?;
        Ok(())
    }

    /// Clicks the button `selector` in the row of `list` whose reference is
    /// `reference`.
    async fn click_in_row(
        &self,
        what: &str,
        list: &str,
        reference: &str,
        selector: &str,
    ) -> Result<()> {
        let ui = &self.ui;
        let mark = ui
            .js(
                "const [list, reference, selector] = arguments;
                 document.querySelectorAll('[data-gui-check-target]').forEach((el) => el.removeAttribute('data-gui-check-target'));
                 const row = [...document.querySelectorAll(`#${list}-list .bookmark-item`)]
                     .find((li) => li.querySelector('.bookmark-ref').textContent === reference);
                 const button = row && row.querySelector(selector);
                 if (!button) return false;
                 button.setAttribute('data-gui-check-target', '');
                 return true;",
                vec![json!(list), json!(reference), json!(selector)],
            )
            .await?;
        if mark != Value::Bool(true) {
            bail!("no {selector} in the {list} list's row {reference}");
        }
        ui.click(what, "[data-gui-check-target]").await
    }

    /// A row opened: the reader is at `heading` with `verse` selected, the
    /// Library closed and focus back on its button.
    async fn opened(&self, heading: &str, verse: u32) -> Result<Value> {
        self.ui
            .wait_with(
                &format!("{heading} to be read, verse {verse} selected, the Library closed, focus on its button"),
                &format!(
                    "{DESCRIBE_JS} const place = (() => {{ {PLACE_JS} }})();
                     return [place.heading === arguments[0] && JSON.stringify(place.selected) === JSON.stringify([arguments[1]])
                         && place.library === 'closed' && place.focus === '#library-toggle', place];"
                ),
                vec![json!(heading), json!(verse)],
            )
            .await
    }

    /// Waits for focus on the button `selector` in the `list` row whose
    /// reference is `reference`.
    async fn focus_in_row(
        &self,
        what: &str,
        list: &str,
        reference: &str,
        selector: &str,
    ) -> Result<()> {
        self.ui
            .wait_with(
                what,
                &format!(
                    "{DESCRIBE_JS}
                     const [list, reference, selector] = arguments;
                     const row = [...document.querySelectorAll(`#${{list}}-list .bookmark-item`)]
                         .find((li) => li.querySelector('.bookmark-ref').textContent === reference);
                     const button = row && row.querySelector(selector);
                     return [!!button && document.activeElement === button, describe(document.activeElement)];"
                ),
                vec![json!(list), json!(reference), json!(selector)],
            )
            .await?;
        Ok(())
    }

    /// Check 14: Bookmarks.
    pub(super) async fn library_bookmarks(&mut self) -> Result<String> {
        let mut said = Vec::new();
        self.plain_reading().await?;
        self.expect_theme("chi-rho").await?;
        let here = self.heading().await?;
        let (three, five) = (format!("{here}:3"), format!("{here}:5"));

        for n in [3, 5] {
            self.select(n).await?;
            self.ui
                .click(
                    &format!("click ★ Bookmark on verse {n}'s card"),
                    ".verse-actions .verse-bookmark-btn",
                )
                .await?;
            self.mark_is(
                &format!("verse {n} to be marked bookmarked"),
                n,
                "bookmarked",
                json!(true),
            )
            .await?;
        }
        self.list_is(
            "the Library's Bookmarks to list both, newest first, as they were made",
            "bookmark",
            &[five.clone(), three.clone()],
        )
        .await?;
        said.push(format!(
            "★ Bookmark on verses 3 and 5's cards: both marked in the reader and listed at once ({five}, {three}; count 2)"
        ));

        self.plain_reading().await?;
        let away = self.turn_from(Key::Right, &here).await?;
        self.library_tab("bookmarks").await?;
        self.library_shot(
            "screenshot: the Library's Bookmarks, Chi Rho",
            LIBRARY_SHOTS[0],
        )
        .await?;

        self.click_in_row(
            &format!("open the row {three}"),
            "bookmark",
            &three,
            ".bookmark-open",
        )
        .await?;
        self.opened(&here, 3).await?;
        said.push(format!(
            "opening {three} from {away}: {here} read, verse 3 selected, the Library closed, focus on its button"
        ));

        self.plain_reading().await?;
        self.library_tab("bookmarks").await?;
        self.click_in_row(
            &format!("remove {five} (×)"),
            "bookmark",
            &five,
            ".bookmark-remove",
        )
        .await?;
        self.list_is(
            &format!("{five} to be gone from the list"),
            "bookmark",
            std::slice::from_ref(&three),
        )
        .await?;
        self.focus_in_row(
            &format!("focus on the next bookmark's × ({three})"),
            "bookmark",
            &three,
            ".bookmark-remove",
        )
        .await?;
        self.mark_is(
            "verse 5's bookmark mark to be gone",
            5,
            "bookmarked",
            json!(false),
        )
        .await?;
        self.mark_is("verse 3 to stay marked", 3, "bookmarked", json!(true))
            .await?;
        said.push(format!(
            "× on {five}: the row went (count 1), focus on the next row's × ({three}), verse 5 unmarked in the \
             reader, verse 3 still marked"
        ));
        Ok(said.join("; "))
    }

    /// Check 15: Highlights.
    pub(super) async fn library_highlights(&mut self) -> Result<String> {
        let mut said = Vec::new();
        self.plain_reading().await?;
        self.expect_theme("chi-rho").await?;
        let here = self.heading().await?;
        let (seven, nine) = (format!("{here}:7 Green"), format!("{here}:9 Pink"));

        for (n, color) in [(7, "green"), (9, "pink")] {
            self.select(n).await?;
            self.ui
                .click(
                    &format!("click the {color} highlighter on verse {n}'s card"),
                    &format!(".verse-actions .highlight-swatch[data-color=\"{color}\"]"),
                )
                .await?;
            self.mark_is(
                &format!("verse {n} to be highlighted {color}"),
                n,
                "highlight",
                json!(color),
            )
            .await?;
        }
        self.list_is(
            "the Library's Highlights to list both, in Bible order, with their colors",
            "highlight",
            &[seven.clone(), nine.clone()],
        )
        .await?;
        said.push(format!(
            "verse 7 green and verse 9 pink from the card: both shown in the reader and listed at once ({seven}, {nine})"
        ));

        self.plain_reading().await?;
        let away = self.turn_from(Key::Right, &here).await?;
        self.library_tab("highlights").await?;
        self.library_shot(
            "screenshot: the Library's Highlights, Chi Rho",
            LIBRARY_SHOTS[1],
        )
        .await?;

        let nine_ref = format!("{here}:9");
        let seven_ref = format!("{here}:7");
        self.click_in_row(
            &format!("open the row {nine_ref}"),
            "highlight",
            &nine_ref,
            ".bookmark-open",
        )
        .await?;
        self.opened(&here, 9).await?;
        said.push(format!(
            "opening {nine_ref} from {away}: {here} read, verse 9 selected, the Library closed, focus on its button"
        ));

        self.plain_reading().await?;
        self.library_tab("highlights").await?;
        self.click_in_row(
            &format!("remove the highlight {seven_ref} (×)"),
            "highlight",
            &seven_ref,
            ".bookmark-remove",
        )
        .await?;
        self.list_is(
            &format!("{seven_ref} to be gone from the list"),
            "highlight",
            std::slice::from_ref(&nine),
        )
        .await?;
        self.focus_in_row(
            &format!("focus on the next passage's × ({nine_ref})"),
            "highlight",
            &nine_ref,
            ".bookmark-remove",
        )
        .await?;
        self.mark_is(
            "verse 7's highlight to be gone",
            7,
            "highlight",
            Value::Null,
        )
        .await?;
        self.mark_is("verse 9 to stay pink", 9, "highlight", json!("pink"))
            .await?;
        said.push(format!(
            "× on {seven_ref}: the row went (count 1), focus on the next row's × ({nine_ref}), verse 7's \
             highlight gone from the reader, verse 9 still pink"
        ));
        Ok(said.join("; "))
    }

    /// Check 16: Notes. A note written with N shows in the list; opening
    /// its row reads it; its Edit opens the note editor, whose Delete
    /// removes it from the list and the reader.
    pub(super) async fn library_notes(&mut self) -> Result<String> {
        let mut said = Vec::new();
        self.plain_reading().await?;
        self.expect_theme("chi-rho").await?;
        let here = self.heading().await?;
        let eleven = format!("{here}:11");
        let before = self.list("note").await?;
        let mut rows: Vec<String> = before["rows"]
            .as_array()
            .map(|r| {
                r.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();

        self.select(11).await?;
        {
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
            ui.type_into("type the note", "#note-input", NOTE_TEXT)
                .await?;
            ui.wait(
                "Save to be enabled",
                "return [!document.getElementById('note-save').disabled, null];",
            )
            .await?;
            ui.click("click Save", "#note-save").await?;
            ui.wait(
                "the note editor to close",
                "return [document.getElementById('note-overlay').hidden, null];",
            )
            .await?;
        }
        self.mark_is("verse 11 to be marked noted", 11, "noted", json!(true))
            .await?;
        // In Bible order: after the earlier checks' note on verse 1.
        let mut with = rows.clone();
        with.push(eleven.clone());
        self.list_is("the Library's Notes to list it at once", "note", &with)
            .await?;
        said.push(format!(
            "a note on verse 11 (N, Save): marked in the reader and listed at once ({})",
            with.join(", ")
        ));

        self.plain_reading().await?;
        let away = self.turn_from(Key::Right, &here).await?;
        self.library_tab("notes").await?;
        self.library_shot("screenshot: the Library's Notes, Chi Rho", LIBRARY_SHOTS[2])
            .await?;

        self.click_in_row(
            &format!("open the row {eleven}"),
            "note",
            &eleven,
            ".bookmark-open",
        )
        .await?;
        self.opened(&here, 11).await?;
        said.push(format!(
            "opening {eleven} from {away}: {here} read, verse 11 selected, the Library closed, focus on its button"
        ));

        self.plain_reading().await?;
        self.library_tab("notes").await?;
        self.click_in_row(
            &format!("Edit on {eleven}"),
            "note",
            &eleven,
            ".note-list-edit",
        )
        .await?;
        let editing = self
            .ui
            .wait_with(
                "the note editor to open on the note, the Library closed",
                &format!(
                    "{VISIBLE_JS} {DESCRIBE_JS}
                     const input = document.getElementById('note-input');
                     return [visible(document.getElementById('note-panel')) && input.value === arguments[0]
                         && document.getElementById('library-panel').hidden
                         && !document.getElementById('note-delete').hidden,
                         {{ title: document.getElementById('note-title').textContent, value: input.value,
                            focus: describe(document.activeElement) }}];"
                ),
                vec![json!(NOTE_TEXT)],
            )
            .await?;
        self.ui.click("click Delete", "#note-delete").await?;
        let deleted = self
            .ui
            .wait(
                "the editor to close, focus on the card's + Add note, and the card to offer Undo",
                &format!(
                    "{DESCRIBE_JS}
                     const focus = document.activeElement;
                     const status = document.querySelector('.verse-actions-status');
                     return [document.getElementById('note-overlay').hidden
                         && !!focus && focus.matches('.verse-actions .verse-note-btn')
                         && !!status && status.textContent.startsWith('Note deleted'),
                         {{ focus: describe(focus), status: status ? status.textContent : null }}];"
                ),
            )
            .await?;
        self.mark_is("verse 11's note mark to be gone", 11, "noted", json!(false))
            .await?;
        rows.retain(|r| *r != eleven);
        self.list_is(&format!("{eleven} to be gone from the list"), "note", &rows)
            .await?;
        said.push(format!(
            "Edit on {eleven}: the editor opened on it (\"{}\"), the Library closed; Delete: the row went ({} left), \
             verse 11 unmarked in the reader, focus on {} with \"{}\"",
            editing["title"].as_str().unwrap_or_default(),
            rows.len(),
            deleted["focus"].as_str().unwrap_or_default(),
            deleted["status"].as_str().unwrap_or_default().trim()
        ));
        self.plain_reading().await?;
        Ok(said.join("; "))
    }

    /// Check 4's Library screenshots in Vellum: each tab as checks 14 to
    /// 16 left it.
    pub(super) async fn library_vellum_screenshots(&mut self) -> Result<()> {
        for (tab, name) in ["bookmarks", "highlights", "notes"]
            .into_iter()
            .zip(&LIBRARY_SHOTS[3..])
        {
            self.library_tab(tab).await?;
            self.library_shot(&format!("screenshot: the Library's {tab}, Vellum"), name)
                .await?;
        }
        self.plain_reading().await
    }
}
