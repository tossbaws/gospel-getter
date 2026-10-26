//! Checks 46 to 52: the selection card, which the Rust pieces (ui-leptos)
//! took over from the page's main script in step 7b, each through the real
//! UI. Verses selected with the mouse, a click and a Shift-click (through
//! the card, a ghost while Shift is held), the card placed by them, beside
//! the selected verses and inside their column (46); Copy, Ctrl+C, each of
//! Copy as…'s formats and Copy chapter…, each time the clipboard read back
//! from outside the page (on Linux pasted through the private display, on
//! Windows by `Get-Clipboard`) and compared with what the backend gives for
//! it: WebKitGTK and WebView2 write it only within the click or key that
//! asks, so this is the clipboard-gesture rule's proof (47); a bookmark from
//! the card, marked and listed, then removed (48); each highlighter color
//! from the card, Remove highlight, and a highlight shown on the next
//! chapter's faded neighbor (49); a note from the card, its marker and its
//! card, Edit, Delete and Undo (50); a cross-reference read in the card,
//! closed, and Go to (51); and the Shift ghost, not while focus is in the
//! card (52). Screenshots in Chi Rho and Vellum.

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use thirtyfour::prelude::*;

use super::{DESCRIBE_JS, Run, VISIBLE_JS};
use crate::native;

/// The screenshots checks 46 to 51 take (check 4 lists them).
pub(super) const CARD_SHOTS: [&str; 7] = [
    "46-selection-card-chi-rho.png",
    "47-copy-as-chi-rho.png",
    "49-highlights-chi-rho.png",
    "50-card-notes-chi-rho.png",
    "51-xref-preview-chi-rho.png",
    "46-selection-card-vellum.png",
    "51-xref-preview-vellum.png",
];

/// The note check 50 writes from the card.
const CARD_NOTE: &str = "Written from the selection card (gui-check).";

/// The card as the page shows it: open or not, its label, the verse it
/// follows, the verses selected, its status line, whether it's a ghost, and
/// the column's translation.
const CARD_JS: &str = "const card = document.getElementById('selection-card');
    const status = card ? card.querySelector('.verse-actions-status') : null;
    const before = card ? card.previousElementSibling : null;
    const verse = document.querySelector('.chapter-main .verse[data-translation]');
    return { open: !!card && card.isConnected,
             label: card ? card.getAttribute('aria-label') : null,
             after: before && before.dataset.verse ? Number(before.dataset.verse) : null,
             selected: [...document.querySelectorAll('.chapter-main .verse-selected[data-translation]')]
                 .map((v) => Number(v.dataset.verse)),
             status: status ? status.textContent : null,
             ghost: !!card && card.classList.contains('is-ghost'),
             code: verse ? verse.dataset.translation : null,
             upper: verse ? verse.dataset.code : null };";

/// Where the card is against the selected verses `arguments[0]` to
/// `arguments[1]` and their column: under the last, above the first, or
/// docked; inside the column.
const PLACED_JS: &str = "const card = document.getElementById('selection-card');
    const at = (n) => document.querySelector(`.chapter-main .verse[data-translation][data-verse=\"${n}\"]`).getBoundingClientRect();
    const c = card.getBoundingClientRect(), first = at(arguments[0]), last = at(arguments[1]);
    const column = card.closest('.chapter-main').getBoundingClientRect();
    const side = card.classList.contains('is-docked') ? 'docked' : card.classList.contains('is-above') ? 'above' : 'below';
    const beside = side === 'docked' || (side === 'below' ? c.top >= last.bottom - 1 : c.bottom <= first.top + 1);
    const inside = c.left >= column.left - 1 && c.right <= column.right + 1;
    return { side, beside, inside, card: [Math.round(c.left), Math.round(c.top), Math.round(c.width), Math.round(c.height)],
             verses: [Math.round(first.top), Math.round(last.bottom)], column: [Math.round(column.left), Math.round(column.width)] };";

/// The backend's text for a passage in every format (`get_copy_text`, as
/// the card asks for it), asked once through the page's IPC: `arguments[0]`
/// is its arguments.
const EXPECTED_JS: &str = "const key = JSON.stringify(arguments[0]);
    const s = window.__guiCheckCopy || (window.__guiCheckCopy = {});
    const got = s[key];
    if (got && got.done) return [true, got];
    if (!got) {
        s[key] = { done: false };
        window.__TAURI__.core.invoke('get_copy_text', arguments[0])
            .then((r) => { s[key] = { done: true, reference: r.reference, formats: r.formats }; },
                  (e) => { s[key] = { done: true, error: String(e) }; });
    }
    return [false, null];";

/// A text box for the clipboard's text to be pasted into (focused without
/// a click, so the selection stays), on Linux.
const PASTE_BOX_JS: &str = "let box = document.getElementById('gui-check-paste');
    if (!box) { box = document.createElement('textarea'); box.id = 'gui-check-paste';
        box.style.position = 'fixed'; box.style.left = '0'; box.style.bottom = '0';
        box.style.width = '10px'; box.style.height = '10px'; box.style.opacity = '0';
        document.body.append(box); }
    box.value = ''; box.focus({ preventScroll: true });
    return document.activeElement === box;";

/// A verse of the chapter being read, by number.
fn verse(number: u32) -> String {
    format!(".chapter-main .verse[data-translation][data-verse=\"{number}\"]")
}

impl Run<'_> {
    /// Clicks verse `number` (a plain click: it's selected, or cleared if it
    /// was the one selected).
    async fn click_verse(&self, number: u32) -> Result<()> {
        self.ui
            .click(&format!("click verse {number}"), &verse(number))
            .await
    }

    /// John 3, at its top, nothing selected, then verses `start` to `end`
    /// selected with the mouse (a click, and a Shift-click for a range).
    async fn select_in_john_3(&self, start: u32, end: u32) -> Result<Value> {
        self.go_to("John 3", "John 3").await?;
        self.plain_reading().await?;
        self.click_verse(start).await?;
        if end != start {
            self.shift_click_verse(end).await?;
        }
        self.wait_for_selection(start, end).await
    }

    /// A Shift-click on verse `number`, as the pointer does it: Shift held
    /// (so the card turns into a ghost and the click goes through it), the
    /// pointer moved to the verse and clicked.
    async fn shift_click_verse(&self, number: u32) -> Result<()> {
        let ui = &self.ui;
        let target = ui
            .driver()
            .find(By::Css(verse(number)))
            .await
            .with_context(|| format!("no verse {number}"))?;
        // The pointer only goes where the window shows.
        ui.js(
            "arguments[0].scrollIntoView({ block: 'center' }); return null;",
            vec![target.to_json()?],
        )
        .await?;
        let clicked = ui
            .step(&format!("Shift-click verse {number}"), async {
                ui.driver()
                    .action_chain()
                    .key_down(Key::Shift)
                    .move_to_element_center(&target)
                    .click()
                    .key_up(Key::Shift)
                    .perform()
                    .await?;
                Ok(())
            })
            .await;
        if clicked.is_err() {
            // Never leave Shift held for the checks after this one.
            self.release_shift().await;
        }
        clicked
    }

    /// Lets go of Shift, whatever happened before (a failed step must not
    /// leave it held: every key after would be shifted).
    async fn release_shift(&self) {
        let _ = self
            .ui
            .driver()
            .action_chain()
            .key_up(Key::Shift)
            .perform()
            .await;
    }

    async fn wait_for_selection(&self, start: u32, end: u32) -> Result<Value> {
        self.ui
            .wait_with(
                &format!("verses {start} to {end} to be selected, the card after verse {end}"),
                &format!(
                    "const seen = (() => {{ {CARD_JS} }})(); const want = [];
                     for (let n = arguments[0]; n <= arguments[1]; n++) want.push(n);
                     return [seen.open && seen.after === arguments[1]
                         && JSON.stringify(seen.selected) === JSON.stringify(want), seen];"
                ),
                vec![json!(start), json!(end)],
            )
            .await
    }

    /// Waits for the card's status line to say `said`.
    async fn wait_for_status(&self, said: &str) -> Result<Value> {
        self.ui
            .wait_with(
                &format!("the card's status line to say \"{said}\""),
                &format!(
                    "const seen = (() => {{ {CARD_JS} }})();
                     return [seen.status === arguments[0], seen];"
                ),
                vec![json!(said)],
            )
            .await
    }

    /// The backend's text for `args` (`get_copy_text`'s) in every format.
    async fn expected_copy(&self, args: Value) -> Result<Value> {
        let got = self
            .ui
            .wait_with(
                "the backend's text for the passage (get_copy_text, through the page's IPC)",
                EXPECTED_JS,
                vec![args],
            )
            .await?;
        if let Some(error) = got["error"].as_str() {
            bail!("get_copy_text failed: {error}");
        }
        Ok(got)
    }

    /// The text on the system clipboard, read from outside the page: on
    /// Linux pasted (Ctrl+V, on the private display) into a text box put on
    /// the page for it, on Windows by `Get-Clipboard`.
    async fn clipboard(&self) -> Result<String> {
        let ui = &self.ui;
        let Some(xvfb) = &self.config.xvfb else {
            return ui
                .step("read the clipboard (Get-Clipboard)", async {
                    native::windows_clipboard(&self.config.export_dir).await
                })
                .await;
        };
        let focused = ui.js(PASTE_BOX_JS, Vec::new()).await?;
        ensure!(
            focused == Value::Bool(true),
            "the paste box couldn't be focused"
        );
        ui.step(
            "paste the clipboard (Ctrl+V, on the private display)",
            async {
                let windows = xvfb.windows_named("Gospel Getter").await?;
                let Some((window, _)) = windows.first() else {
                    bail!("no window named \"… Gospel Getter\" on the private display");
                };
                xvfb.xdotool(&["windowfocus", "--sync", window]).await?;
                xvfb.xdotool(&["key", "--clearmodifiers", "ctrl+v"]).await?;
                Ok(())
            },
        )
        .await?;
        let pasted = ui
            .wait(
                "the clipboard's text to be pasted",
                "const box = document.getElementById('gui-check-paste');
                 return [!!box && box.value !== '', box ? box.value : null];",
            )
            .await?;
        ui.js(
            "const box = document.getElementById('gui-check-paste'); if (box) box.remove(); return null;",
            Vec::new(),
        )
        .await?;
        pasted
            .as_str()
            .map(str::to_owned)
            .context("nothing was pasted")
    }

    /// Waits for the clipboard to hold `want` (the paste reads what's there
    /// once the copy's promise has settled).
    async fn expect_clipboard(&self, what: &str, want: &str) -> Result<()> {
        let got = self.clipboard().await?;
        ensure!(
            got == want,
            "{what}: the clipboard holds {got:?}, not the backend's {want:?}"
        );
        Ok(())
    }

    /// Check 46: verses selected with the mouse, a click then a Shift-click
    /// (through the card, a ghost while Shift is held), and the card placed
    /// by them: beside the selected verses, never over them unless docked,
    /// inside their column.
    pub(super) async fn select_verses(&mut self) -> Result<String> {
        self.go_to("John 3", "John 3").await?;
        self.plain_reading().await?;
        self.click_verse(16).await?;
        let one = self.wait_for_selection(16, 16).await?;
        let code = one["upper"].as_str().unwrap_or("?").to_owned();
        ensure!(
            one["label"] == format!("Selection: John 3:16 ({code})"),
            "the card's label: {one}"
        );
        let placed_one = self.ui.js(PLACED_JS, vec![json!(16), json!(16)]).await?;
        ensure!(
            placed_one["beside"] == true && placed_one["inside"] == true,
            "the card isn't beside verse 16 in its column: {placed_one}"
        );
        // Verse 18 is likely under the card: the Shift-click reaches it
        // through the ghost.
        self.shift_click_verse(18).await?;
        let range = self.wait_for_selection(16, 18).await?;
        ensure!(
            range["label"] == format!("Selection: John 3:16–18 ({code})"),
            "the range's label: {range}"
        );
        let placed = self.ui.js(PLACED_JS, vec![json!(16), json!(18)]).await?;
        ensure!(
            placed["beside"] == true && placed["inside"] == true,
            "the card isn't beside verses 16–18 in their column: {placed}"
        );
        let xrefs = self
            .ui
            .js(
                "return document.querySelectorAll('#selection-card .selection-xref-row').length;",
                Vec::new(),
            )
            .await?;
        self.shot(
            "screenshot: the selection card on John 3:16–18, Chi Rho",
            CARD_SHOTS[0],
        )
        .await;
        // Clicked again, a single verse clears it.
        self.click_verse(17).await?;
        self.wait_for_selection(17, 17).await?;
        self.click_verse(17).await?;
        self.ui
            .wait(
                "a second click on the verse to clear the selection, the card gone",
                &format!(
                    "const seen = (() => {{ {CARD_JS} }})();
                     return [!seen.open && seen.selected.length === 0, seen];"
                ),
            )
            .await?;
        Ok(format!(
            "click: John 3:16 selected, \"{}\", card {} it (left, top, width, height {}); Shift-click on \
             verse 18 (through the ghost card): 16–18, \"{}\", card {} them ({}, column {}), {} \
             cross-reference rows; a second click on a single verse cleared it",
            one["label"].as_str().unwrap_or("?"),
            placed_one["side"].as_str().unwrap_or("?"),
            placed_one["card"],
            range["label"].as_str().unwrap_or("?"),
            placed["side"].as_str().unwrap_or("?"),
            placed["card"],
            placed["column"],
            xrefs,
        ))
    }

    /// Check 47: Copy, Ctrl+C, each of Copy as…'s formats, and Copy
    /// chapter…: each time the clipboard, read from outside the page, holds
    /// exactly what the backend gives for it, and the status says so.
    pub(super) async fn copy_formats(&mut self) -> Result<String> {
        let selected = self.select_in_john_3(16, 18).await?;
        let code = selected["code"].as_str().unwrap_or("kjv").to_owned();
        let book = self
            .ui
            .js(
                "return Number(document.getElementById('reading-pane-inner').dataset.currentBook);",
                Vec::new(),
            )
            .await?;
        let passage = self
            .expected_copy(json!({ "translationCode": code, "bookId": book, "chapter": 3, "start": 16, "end": 18 }))
            .await?;
        let reference = passage["reference"].as_str().unwrap_or("?").to_owned();
        let text = |format: &str| passage["formats"][format].as_str().unwrap_or("").to_owned();
        let mut said = Vec::new();

        self.ui.click("click Copy", ".verse-copy-btn").await?;
        self.wait_for_status(&format!("Copied {reference}")).await?;
        self.expect_clipboard("Copy", &text("reference")).await?;
        said.push(format!("Copy: \"Copied {reference}\""));

        for (format, status) in [
            ("numbered", " with verse numbers"),
            ("markdown", " as a Markdown quote"),
            ("plain", " as plain text"),
            ("reference", ""),
        ] {
            self.ui
                .click("click Copy as…", ".verse-copy-as-btn")
                .await?;
            self.ui
                .wait(
                    "Copy as… to open, focus on its first item",
                    &format!(
                        "{VISIBLE_JS} {DESCRIBE_JS} const menu = document.getElementById('copy-as-menu');
                         return [visible(menu) && document.activeElement && document.activeElement.classList.contains('copy-as-item')
                             && document.querySelector('.verse-copy-as-btn').getAttribute('aria-expanded') === 'true',
                             {{ label: menu && menu.getAttribute('aria-label'), focus: describe(document.activeElement) }}];"
                    ),
                )
                .await?;
            if format == "numbered" {
                self.shot("screenshot: Copy as…, Chi Rho", CARD_SHOTS[1])
                    .await;
            }
            self.ui
                .click(
                    &format!("choose {format}"),
                    &format!("#copy-as-menu .copy-as-item[data-format=\"{format}\"]"),
                )
                .await?;
            self.wait_for_status(&format!("Copied {reference}{status}"))
                .await?;
            self.expect_clipboard(&format!("Copy as… {format}"), &text(format))
                .await?;
            said.push(format!("{format}: \"Copied {reference}{status}\""));
        }

        // Ctrl+C after another copy: the selection again, in the first
        // format.
        self.ui
            .click("click Copy as…", ".verse-copy-as-btn")
            .await?;
        self.ui
            .click(
                "choose plain",
                "#copy-as-menu .copy-as-item[data-format=\"plain\"]",
            )
            .await?;
        self.wait_for_status(&format!("Copied {reference} as plain text"))
            .await?;
        self.ui
            .js(
                "if (document.activeElement && document.activeElement !== document.body) document.activeElement.blur(); return null;",
                Vec::new(),
            )
            .await?;
        self.ui
            .step("press Ctrl+C", async {
                self.ui
                    .driver()
                    .action_chain()
                    .key_down(Key::Control)
                    .send_keys("c")
                    .key_up(Key::Control)
                    .perform()
                    .await?;
                Ok(())
            })
            .await?;
        self.wait_for_status(&format!("Copied {reference}")).await?;
        self.expect_clipboard("Ctrl+C", &text("reference")).await?;
        said.push("Ctrl+C: the first format again".to_owned());

        // Copy chapter…, as Markdown: its toast.
        self.plain_reading().await?;
        let chapter = self
            .expected_copy(json!({ "translationCode": code, "bookId": book, "chapter": 3 }))
            .await?;
        self.ui
            .js(
                "document.querySelector('.chapter-copy').scrollIntoView({ block: 'center' }); return null;",
                Vec::new(),
            )
            .await?;
        self.ui
            .click("click Copy chapter…", ".chapter-copy-btn")
            .await?;
        self.ui
            .click(
                "choose markdown",
                "#copy-as-menu .copy-as-item[data-format=\"markdown\"]",
            )
            .await?;
        let chapter_reference = chapter["reference"].as_str().unwrap_or("?").to_owned();
        let toast = self
            .ui
            .wait_with(
                "the toast to say the chapter was copied",
                "const toast = document.getElementById('data-status');
                 return [!toast.classList.contains('is-hidden') && toast.textContent === arguments[0], toast.textContent];",
                vec![json!(format!("Copied {chapter_reference} as a Markdown quote"))],
            )
            .await?;
        self.expect_clipboard(
            "Copy chapter… as Markdown",
            chapter["formats"]["markdown"].as_str().unwrap_or(""),
        )
        .await?;
        said.push(format!(
            "Copy chapter… markdown: toast \"{}\"",
            toast.as_str().unwrap_or("?")
        ));
        self.ui
            .wait_within(
                "the toast to go",
                "return [document.getElementById('data-status').classList.contains('is-hidden'), null];",
                Vec::new(),
                Some(std::time::Duration::from_secs(20)),
            )
            .await?;
        let read_by = if self.config.xvfb.is_some() {
            "pasted on the private display"
        } else {
            "Get-Clipboard"
        };
        Ok(format!(
            "each copy within its click or key, the clipboard ({read_by}) exactly the backend's text: {}",
            said.join("; ")
        ))
    }

    /// Check 48: a bookmark from the card: Bookmark, the verse marked and
    /// listed in the Library, the button then Remove bookmark; removed, the
    /// mark and the row gone.
    pub(super) async fn card_bookmark(&mut self) -> Result<String> {
        self.select_in_john_3(16, 16).await?;
        let seen_js = "const v = document.querySelector('.chapter-main .verse[data-translation][data-verse=\"16\"]');
            const rows = [...document.querySelectorAll('#bookmark-list .bookmark-ref')].map((r) => r.textContent);
            return { marked: v.classList.contains('is-bookmarked'), button: document.querySelector('.verse-bookmark-btn').textContent,
                     listed: rows.includes('John 3:16'), rows: rows.length };";
        self.ui
            .click("click Bookmark", ".verse-bookmark-btn")
            .await?;
        self.wait_for_status("Bookmarked").await?;
        let added = self
            .ui
            .wait(
                "John 3:16 marked and listed, the button to say Remove bookmark",
                &format!(
                    "const seen = (() => {{ {seen_js} }})();
                     return [seen.marked && seen.listed && seen.button === '★ Remove bookmark', seen];"
                ),
            )
            .await?;
        self.ui
            .click("click Remove bookmark", ".verse-bookmark-btn")
            .await?;
        self.wait_for_status("Bookmark removed").await?;
        let removed = self
            .ui
            .wait(
                "the mark and the row gone, the button Bookmark again",
                &format!(
                    "const seen = (() => {{ {seen_js} }})();
                     return [!seen.marked && !seen.listed && seen.button === '★ Bookmark', seen];"
                ),
            )
            .await?;
        Ok(format!(
            "Bookmark: \"Bookmarked\", verse 16 marked, listed ({} rows), \"{}\"; Remove bookmark: \"Bookmark \
             removed\", unmarked, unlisted ({} rows)",
            added["rows"],
            added["button"].as_str().unwrap_or("?"),
            removed["rows"],
        ))
    }

    /// Check 49: each highlighter color from the card on John 3:16–17 (the
    /// verses colored, its swatch pressed, Remove highlight shown), Remove
    /// highlight; and a highlight on John 3:1 shown on the next chapter's
    /// faded neighbor, then removed.
    pub(super) async fn card_highlight(&mut self) -> Result<String> {
        self.select_in_john_3(16, 17).await?;
        let seen_js = "const color = (n) => { const v = document.querySelector(`.chapter-main .verse[data-translation][data-verse=\"${n}\"]`);
                return v ? v.dataset.highlight || null : null; };
            const pressed = [...document.querySelectorAll('#selection-card .highlight-swatch')]
                .filter((b) => b.getAttribute('aria-pressed') === 'true').map((b) => b.dataset.color);
            const remove = document.querySelector('#selection-card .highlight-remove');
            return { colors: [color(16), color(17)], pressed, remove: !!remove && !remove.hidden };";
        let mut said = Vec::new();
        for color in ["yellow", "green", "blue", "pink"] {
            self.ui
                .click(
                    &format!("click the {color} swatch"),
                    &format!("#selection-card .highlight-swatch[data-color=\"{color}\"]"),
                )
                .await?;
            self.wait_for_status(&format!("Highlighted {color}"))
                .await?;
            let seen = self
                .ui
                .wait_with(
                    &format!("verses 16 and 17 {color}, its swatch pressed, Remove highlight shown"),
                    &format!(
                        "const seen = (() => {{ {seen_js} }})(); const c = arguments[0];
                         return [seen.colors[0] === c && seen.colors[1] === c && seen.pressed.length === 1
                             && seen.pressed[0] === c && seen.remove, seen];"
                    ),
                    vec![json!(color)],
                )
                .await?;
            if color == "green" {
                self.shot(
                    "screenshot: a highlight from the card, Chi Rho",
                    CARD_SHOTS[2],
                )
                .await;
            }
            said.push(format!("{color} {}", seen["colors"]));
        }
        self.ui
            .click(
                "click Remove highlight",
                "#selection-card .highlight-remove",
            )
            .await?;
        self.wait_for_status("Highlight removed").await?;
        self.ui
            .wait(
                "the verses' color gone, no swatch pressed, Remove highlight hidden",
                &format!(
                    "const seen = (() => {{ {seen_js} }})();
                     return [seen.colors[0] === null && seen.colors[1] === null && seen.pressed.length === 0
                         && !seen.remove, seen];"
                ),
            )
            .await?;
        // The next chapter's faded neighbor shows John 3:1's highlight.
        self.plain_reading().await?;
        self.ui
            .js("window.scrollTo(0, 0); return null;", Vec::new())
            .await?;
        self.click_verse(1).await?;
        self.wait_for_selection(1, 1).await?;
        self.ui
            .click(
                "click the yellow swatch",
                "#selection-card .highlight-swatch[data-color=\"yellow\"]",
            )
            .await?;
        self.wait_for_status("Highlighted yellow").await?;
        self.go_to("John 4", "John 4").await?;
        let side = self
            .ui
            .wait(
                "John 3:1 yellow on John 4's faded neighbor",
                "const v = document.querySelector('.chapter-side-prev .verse[data-chapter=\"3\"][data-verse=\"1\"]');
                 const other = document.querySelector('.chapter-side-prev .verse[data-chapter=\"3\"][data-verse=\"2\"]');
                 return [!!v && v.dataset.highlight === 'yellow' && !!other && !other.dataset.highlight,
                         { neighbor: v ? v.dataset.highlight || null : 'none', next: other ? other.dataset.highlight || null : 'none' }];",
            )
            .await?;
        self.go_to("John 3", "John 3").await?;
        self.plain_reading().await?;
        self.click_verse(1).await?;
        self.wait_for_selection(1, 1).await?;
        self.ui
            .click(
                "click Remove highlight",
                "#selection-card .highlight-remove",
            )
            .await?;
        self.wait_for_status("Highlight removed").await?;
        self.plain_reading().await?;
        Ok(format!(
            "John 3:16–17 from the card: {}; Remove highlight cleared them; John 3:1 yellow shown on John 4's \
             faded neighbor ({side}), then removed",
            said.join(", ")
        ))
    }

    /// Check 50: a note from the card on John 3:16: the editor, Save, the
    /// verse's marker and the card's Notes section; Edit opens it; Delete,
    /// then the status line's Undo brings it back; deleted again.
    pub(super) async fn card_note(&mut self) -> Result<String> {
        self.select_in_john_3(16, 16).await?;
        let notes_js = "const v = document.querySelector('.chapter-main .verse[data-translation][data-verse=\"16\"]');
            const section = document.querySelector('#selection-card .verse-notes');
            const title = section ? section.querySelector('.verse-notes-title') : null;
            return { marked: v.classList.contains('has-notes') && !!v.querySelector(':scope > .note-cue'),
                     title: title ? title.textContent : null,
                     bodies: section ? [...section.querySelectorAll('.note-card-body')].map((b) => b.textContent) : [] };";
        self.ui
            .click("click + Add note", "#selection-card .verse-note-btn")
            .await?;
        let editor = self
            .ui
            .wait(
                "the note editor to open on John 3:16, focus in its text box",
                &format!(
                    "{VISIBLE_JS} return [visible(document.getElementById('note-panel'))
                         && document.activeElement && document.activeElement.id === 'note-input',
                         document.getElementById('note-title').textContent];"
                ),
            )
            .await?;
        self.ui
            .type_into("type the note", "#note-input", CARD_NOTE)
            .await?;
        self.ui.click("click Save", "#note-save").await?;
        self.wait_for_status("Note added").await?;
        let added = self
            .ui
            .wait_with(
                "the verse marked, the card's Notes section showing the note",
                &format!(
                    "const seen = (() => {{ {notes_js} }})();
                     return [seen.marked && seen.title === 'Notes (1)' && seen.bodies[0] === arguments[0], seen];"
                ),
                vec![json!(CARD_NOTE)],
            )
            .await?;
        self.shot("screenshot: a note in the card, Chi Rho", CARD_SHOTS[3])
            .await;
        self.ui
            .click("click the note's Edit", "#selection-card .note-edit")
            .await?;
        self.ui
            .wait_with(
                "the editor to open on the note",
                &format!(
                    "{VISIBLE_JS} const input = document.getElementById('note-input');
                     return [visible(document.getElementById('note-panel')) && input.value === arguments[0]
                         && visible(document.getElementById('note-delete')), input.value];"
                ),
                vec![json!(CARD_NOTE)],
            )
            .await?;
        self.ui.click("click Delete", "#note-delete").await?;
        self.wait_for_status("Note deleted · Undo").await?;
        self.ui
            .wait(
                "the marker and the Notes section gone",
                &format!(
                    "const seen = (() => {{ {notes_js} }})();
                     return [!seen.marked && seen.title === null, seen];"
                ),
            )
            .await?;
        self.ui
            .click("click Undo", "#selection-card .status-undo")
            .await?;
        self.wait_for_status("Note restored").await?;
        let restored = self
            .ui
            .wait_with(
                "the note back, marked and in the card",
                &format!(
                    "const seen = (() => {{ {notes_js} }})();
                     return [seen.marked && seen.title === 'Notes (1)' && seen.bodies[0] === arguments[0], seen];"
                ),
                vec![json!(CARD_NOTE)],
            )
            .await?;
        // Left as it was found: deleted.
        self.ui
            .click("click the note's Edit", "#selection-card .note-edit")
            .await?;
        self.ui
            .wait(
                "the editor to open on the note",
                &format!(
                    "{VISIBLE_JS} return [visible(document.getElementById('note-delete')), null];"
                ),
            )
            .await?;
        self.ui.click("click Delete", "#note-delete").await?;
        self.wait_for_status("Note deleted · Undo").await?;
        self.plain_reading().await?;
        Ok(format!(
            "+ Add note: the editor on \"{}\", saved: \"Note added\", verse 16 marked, {} {:?}; Edit opened it; \
             Delete: \"Note deleted · Undo\", unmarked; Undo: \"Note restored\", {} again; deleted again",
            editor.as_str().unwrap_or("?"),
            added["title"].as_str().unwrap_or("?"),
            added["bodies"][0].as_str().unwrap_or("?"),
            restored["title"].as_str().unwrap_or("?"),
        ))
    }

    /// Opens the first cross-reference chip's preview in the card, and
    /// returns what it shows.
    async fn open_preview(&self) -> Result<Value> {
        self.ui
            .click(
                "click the first cross-reference chip",
                "#selection-card .selection-xrefs .xref-citation",
            )
            .await?;
        self.ui
            .wait(
                "the chip's passage to show in the card, with Go to",
                &format!(
                    "{VISIBLE_JS} const preview = document.getElementById('xref-preview');
                     const chip = document.querySelector('#selection-card .xref-citation');
                     const title = preview.querySelector('.xref-preview-title');
                     const text = preview.querySelector('.xref-preview-text');
                     const go = preview.querySelector('.xref-preview-go');
                     return [visible(preview) && chip.getAttribute('aria-expanded') === 'true' && !!title && !!text && !!go,
                             {{ chip: chip.textContent, title: title && title.textContent,
                                text: text && text.textContent.slice(0, 50),
                                go: go && {{ book: go.dataset.book, chapter: go.dataset.chapter,
                                    verse: Number(go.dataset.verse), end: Number(go.dataset.endVerse) }} }}];"
                ),
            )
            .await
    }

    /// Check 51: a cross-reference read in the card: a chip opens its
    /// passage under its row, the same chip closes it, and Go to reads it,
    /// its verses selected.
    pub(super) async fn xref_preview(&mut self) -> Result<String> {
        self.select_in_john_3(16, 16).await?;
        let shown = self.open_preview().await?;
        self.shot(
            "screenshot: a cross-reference read in the card, Chi Rho",
            CARD_SHOTS[4],
        )
        .await;
        self.ui
            .click("click the chip again", "#selection-card .xref-citation")
            .await?;
        self.ui
            .wait(
                "the preview to close",
                "const preview = document.getElementById('xref-preview');
                 const chip = document.querySelector('#selection-card .xref-citation');
                 return [preview.hidden && preview.innerHTML === '' && chip.getAttribute('aria-expanded') === 'false', null];",
            )
            .await?;
        self.open_preview().await?;
        self.ui
            .click("click Go to", "#xref-preview .xref-preview-go")
            .await?;
        let go = &shown["go"];
        let landed = self
            .ui
            .wait_with(
                "the passage to be read, its verses selected",
                "const go = arguments[0];
                 const inner = document.getElementById('reading-pane-inner');
                 const selected = [...document.querySelectorAll('.chapter-main .verse-selected[data-translation]')]
                     .map((v) => Number(v.dataset.verse));
                 const heading = document.querySelector('.chapter-main .chapter-heading');
                 return [!!inner && inner.dataset.currentBook === go.book && inner.dataset.currentChapter === go.chapter
                     && selected.length > 0 && selected[0] === go.verse && selected[selected.length - 1] === go.end,
                     { heading: heading && heading.textContent, selected }];",
                vec![go.clone()],
            )
            .await?;
        self.plain_reading().await?;
        Ok(format!(
            "chip {} opened \"{}\" (\"{}…\") under its row; again: closed; Go to: {} read, {} selected",
            shown["chip"].as_str().unwrap_or("?"),
            shown["title"].as_str().unwrap_or("?"),
            shown["text"].as_str().unwrap_or("?"),
            landed["heading"].as_str().unwrap_or("?"),
            landed["selected"],
        ))
    }

    async fn hold_shift(&self, down: bool) -> Result<()> {
        let what = if down {
            "hold Shift"
        } else {
            "let go of Shift"
        };
        self.ui
            .step(what, async {
                let chain = self.ui.driver().action_chain();
                let chain = if down {
                    chain.key_down(Key::Shift)
                } else {
                    chain.key_up(Key::Shift)
                };
                chain.perform().await?;
                Ok(())
            })
            .await
    }

    async fn wait_for_ghost(&self, ghost: bool, what: &str) -> Result<Value> {
        self.ui
            .wait_with(
                what,
                &format!(
                    "const seen = (() => {{ {CARD_JS} }})();
                     return [seen.open && seen.ghost === arguments[0], seen];"
                ),
                vec![json!(ghost)],
            )
            .await
    }

    /// Check 52: the Shift ghost: Shift held, the card is a ghost; let go,
    /// it's itself again; with focus in the card, Shift leaves it be.
    pub(super) async fn shift_ghost(&mut self) -> Result<String> {
        let seen = self.shift_ghost_steps().await;
        self.release_shift().await;
        seen
    }

    async fn shift_ghost_steps(&mut self) -> Result<String> {
        self.select_in_john_3(16, 16).await?;
        self.ui
            .js(
                "if (document.activeElement && document.activeElement !== document.body) document.activeElement.blur(); return null;",
                Vec::new(),
            )
            .await?;
        self.hold_shift(true).await?;
        self.wait_for_ghost(true, "the card to be a ghost while Shift is held")
            .await?;
        self.hold_shift(false).await?;
        self.wait_for_ghost(false, "the card to be itself once Shift is let go")
            .await?;
        self.ui
            .js(
                "document.querySelector('#selection-card .verse-copy-btn').focus(); return null;",
                Vec::new(),
            )
            .await?;
        self.hold_shift(true).await?;
        let focused = self
            .ui
            .wait(
                "Shift with focus in the card to leave it be",
                &format!(
                    "{DESCRIBE_JS} const seen = (() => {{ {CARD_JS} }})();
                     return [seen.open && !seen.ghost && document.activeElement.classList.contains('verse-copy-btn'),
                             {{ ...seen, focus: describe(document.activeElement) }}];"
                ),
            )
            .await?;
        self.hold_shift(false).await?;
        self.plain_reading().await?;
        Ok(format!(
            "Shift held: a ghost; let go: itself; Shift with focus in the card ({}): not a ghost",
            focused["focus"].as_str().unwrap_or("?"),
        ))
    }

    /// Checks 46's and 51's second screenshots, in Vellum.
    pub(super) async fn card_vellum_screenshots(&mut self) -> Result<()> {
        self.select_in_john_3(16, 18).await?;
        self.shot(
            "screenshot: the selection card on John 3:16–18, Vellum",
            CARD_SHOTS[5],
        )
        .await;
        self.click_verse(16).await?;
        self.wait_for_selection(16, 16).await?;
        self.open_preview().await?;
        self.shot(
            "screenshot: a cross-reference read in the card, Vellum",
            CARD_SHOTS[6],
        )
        .await;
        self.plain_reading().await
    }
}
