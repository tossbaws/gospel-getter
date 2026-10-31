//! Checks 53 to 58: the keyboard, the palette's page commands and startup,
//! which the Rust pieces (ui-leptos) took over from the page's main script
//! in step 7c, each through the real UI and real keys (WebDriver's). Chapters
//! turned by ← and →, books by `[` and `]` (53); verses selected by j and k,
//! extended with Shift, each shown on screen, and Escape backing out a
//! cross-reference read in the card, then the selection (54); Ctrl+C with
//! the clipboard read back from outside the page (on Linux pasted through
//! the private display, on Windows by `Get-Clipboard`), and Ctrl+Shift+C
//! opening Copy as… (55); Alt+← and Alt+→ through the history, and N opening
//! the note editor on the selection (56); the palette running commands whose
//! code was the page's: Next chapter, Back and Copy chapter, the clipboard
//! checked (57); and, after the restart, a cold start straight to the
//! chapter the first session left (58). Screenshots in Chi Rho and Vellum.

use anyhow::{Result, ensure};
use serde_json::{Value, json};
use thirtyfour::prelude::*;

use super::Run;

/// The screenshots checks 54 and 56 take (check 4 lists them).
pub(super) const KEYS_SHOTS: [&str; 3] = [
    "54-keyboard-selection-chi-rho.png",
    "56-note-by-key-chi-rho.png",
    "54-keyboard-selection-vellum.png",
];

/// Where the reader is, and what's selected and shown of it: the heading,
/// the title, the verses selected, whether the card is open and a
/// cross-reference read in it, and whether each selected verse is in view
/// below the top bar.
const READING_JS: &str = "const heading = document.querySelector('.chapter-main .chapter-heading');
    const selected = [...document.querySelectorAll('.chapter-main .verse-selected[data-translation]')];
    const bar = document.getElementById('top-bar').getBoundingClientRect().bottom;
    const card = document.getElementById('selection-card');
    const preview = document.getElementById('xref-preview');
    const menu = document.getElementById('copy-as-menu');
    const editor = document.getElementById('note-overlay');
    return { heading: heading ? heading.textContent.trim() : null, title: document.title,
             selected: selected.map((v) => Number(v.dataset.verse)),
             shown: selected.every((v) => { const r = v.getBoundingClientRect();
                 return r.top >= bar - 1 && r.bottom <= window.innerHeight + 1; }),
             card: !!card && card.isConnected,
             preview: !!preview && preview.isConnected && !preview.hidden,
             menu: !!menu && menu.isConnected && !menu.hidden,
             menuFocus: !!menu && menu.contains(document.activeElement),
             editor: !!editor && !editor.hidden,
             editorTitle: editor && !editor.hidden ? document.getElementById('note-title').textContent : null,
             toast: document.getElementById('data-status').classList.contains('is-hidden') ? ''
                 : document.getElementById('data-status').textContent,
             scrollY: Math.round(window.scrollY) };";

impl Run<'_> {
    /// Focus on the page itself (nothing focused), as after reading: keys
    /// then go to the keyboard dispatcher.
    async fn focus_page(&self) -> Result<()> {
        self.ui
            .js(
                "if (document.activeElement && document.activeElement !== document.body) document.activeElement.blur(); return null;",
                Vec::new(),
            )
            .await?;
        Ok(())
    }

    /// Waits until `ready` (a JavaScript expression over `seen`, as
    /// [`READING_JS`] sees it) is true, and returns what it saw.
    async fn reading_is(&self, what: &str, ready: &str, args: Vec<Value>) -> Result<Value> {
        self.ui
            .wait_with(
                what,
                &format!("const seen = (() => {{ {READING_JS} }})(); return [!!({ready}), seen];"),
                args,
            )
            .await
    }

    /// Presses `key` with `modifiers` held, all at once.
    pub(super) async fn chord(
        &self,
        what: &str,
        modifiers: &[Key],
        key: impl Into<TypingData>,
    ) -> Result<()> {
        let key = key.into();
        let pressed = self
            .ui
            .step(what, async {
                let mut chain = self.ui.driver().action_chain();
                for modifier in modifiers {
                    chain = chain.key_down(modifier.clone());
                }
                chain = chain.send_keys(key);
                for modifier in modifiers.iter().rev() {
                    chain = chain.key_up(modifier.clone());
                }
                chain.perform().await?;
                Ok(())
            })
            .await;
        if pressed.is_err() {
            // Never leave a modifier held for the checks after this one.
            let mut chain = self.ui.driver().action_chain();
            for modifier in modifiers {
                chain = chain.key_up(modifier.clone());
            }
            let _ = chain.perform().await;
        }
        pressed
    }

    /// Verse `target` selected by keyboard alone: j (with nothing selected,
    /// the first verse in view), then j again until it's there.
    async fn select_by_keys(&self, target: u64) -> Result<()> {
        self.focus_page().await?;
        self.ui.press("press j (nothing selected)", "j").await?;
        let first = self
            .reading_is(
                "a verse selected by j",
                "seen.selected.length === 1",
                Vec::new(),
            )
            .await?;
        let n = first["selected"][0].as_u64().unwrap_or(0);
        ensure!(n <= target, "j selected verse {n}, past {target}");
        for _ in n..target {
            self.ui.press("press j", "j").await?;
        }
        self.reading_is(
            &format!("verse {target} selected by keyboard"),
            "JSON.stringify(seen.selected) === JSON.stringify([arguments[0]]) && seen.shown",
            vec![json!(target)],
        )
        .await?;
        Ok(())
    }

    /// Presses `key` on the page and waits for `heading`.
    async fn turn(&self, label: &str, key: impl Into<TypingData>, heading: &str) -> Result<Value> {
        self.focus_page().await?;
        self.ui.press(&format!("press {label}"), key).await?;
        self.reading_is(
            &format!("{heading} after {label}"),
            "seen.heading === arguments[0] && seen.title.startsWith(arguments[0])",
            vec![json!(heading)],
        )
        .await
    }

    /// Check 53: ← and → turn the chapter (the pane's chapters around it),
    /// `[` and `]` the book, at chapter 1.
    pub(super) async fn key_turns(&mut self) -> Result<String> {
        self.go_to("John 3", "John 3").await?;
        self.turn("→", Key::Right, "John 4").await?;
        self.turn("←", Key::Left, "John 3").await?;
        self.turn("]", "]", "Acts 1").await?;
        self.turn("[", "[", "John 1").await?;
        self.turn("[", "[", "Luke 1").await?;
        // Across a book's end: Luke 1 ← is Mark 16.
        let back = self.turn("←", Key::Left, "Mark 16").await?;
        Ok(format!(
            "John 3 → John 4 ← John 3, ] Acts 1, [ John 1, [ Luke 1, ← {} (\"{}\")",
            back["heading"].as_str().unwrap_or("?"),
            back["title"].as_str().unwrap_or("?"),
        ))
    }

    /// Check 54: j and k select the verse after and before (with nothing
    /// selected, the last verse in view for k), Shift extends, each verse
    /// scrolled into view; then Escape closes a cross-reference read in the
    /// card, then clears the selection.
    pub(super) async fn key_selection(&mut self) -> Result<String> {
        self.go_to("John 3", "John 3").await?;
        self.focus_page().await?;
        self.ui.press("press j (nothing selected)", "j").await?;
        let first = self
            .reading_is(
                "a verse selected by j, in view, the card open",
                "seen.selected.length === 1 && seen.card && seen.shown",
                Vec::new(),
            )
            .await?;
        let n = first["selected"][0].as_u64().unwrap_or(0);
        let mut said = vec![format!("j: {n}")];
        for (label, shift, key, want) in [
            ("j", false, "j", vec![n + 1]),
            ("Shift+J", true, "J", vec![n + 1, n + 2]),
            ("Shift+J", true, "J", vec![n + 1, n + 2, n + 3]),
            ("Shift+K", true, "K", vec![n + 1, n + 2]),
            ("k", false, "k", vec![n]),
        ] {
            if shift {
                self.chord(&format!("press {label}"), &[Key::Shift], key)
                    .await?;
            } else {
                self.ui.press(&format!("press {label}"), key).await?;
            }
            self.reading_is(
                &format!("{want:?} selected after {label}, in view"),
                "JSON.stringify(seen.selected) === JSON.stringify(arguments[0]) && seen.card && seen.shown",
                vec![json!(want)],
            )
            .await?;
            said.push(format!("{label}: {want:?}"));
        }
        // Far down the chapter: each step scrolls just enough.
        for _ in 0..12 {
            self.ui.press("press j", "j").await?;
        }
        let far = self
            .reading_is(
                "verse 12 on, in view (the page scrolled)",
                "JSON.stringify(seen.selected) === JSON.stringify([arguments[0]]) && seen.shown && seen.scrollY > 0",
                vec![json!(n + 12)],
            )
            .await?;
        said.push(format!(
            "12 × j: {} in view, scrolled to {}",
            n + 12,
            far["scrollY"]
        ));
        self.shot(
            "screenshot: verses selected by keyboard, Chi Rho",
            KEYS_SHOTS[0],
        )
        .await;

        // Escape: the cross-reference read in the card, then the selection.
        self.go_to("John 3", "John 3").await?;
        self.select_by_keys(16).await?;
        self.ui
            .click(
                "open a cross-reference in the card",
                "#selection-card .xref-citation",
            )
            .await?;
        self.reading_is(
            "the cross-reference read in the card",
            "seen.preview",
            Vec::new(),
        )
        .await?;
        self.focus_page().await?;
        self.ui
            .press("press Escape (the cross-reference)", Key::Escape)
            .await?;
        self.reading_is(
            "the cross-reference closed, verse 16 still selected",
            "!seen.preview && JSON.stringify(seen.selected) === '[16]'",
            Vec::new(),
        )
        .await?;
        self.ui
            .press("press Escape (the selection)", Key::Escape)
            .await?;
        self.reading_is(
            "the selection cleared, the card gone",
            "seen.selected.length === 0 && !seen.card",
            Vec::new(),
        )
        .await?;
        said.push("Escape: the preview, then the selection".to_owned());
        Ok(said.join("; "))
    }

    /// Check 55: Ctrl+C copies the selected verses (the clipboard read back
    /// from outside the page, the backend's text), and Ctrl+Shift+C opens
    /// Copy as… with focus in it; Escape closes it.
    pub(super) async fn key_copy(&mut self) -> Result<String> {
        self.go_to("John 3", "John 3").await?;
        self.select_by_keys(16).await?;
        self.chord("press Shift+J", &[Key::Shift], "J").await?;
        let seen = self
            .reading_is(
                "verses 16 and 17 selected",
                "JSON.stringify(seen.selected) === '[16,17]'",
                Vec::new(),
            )
            .await?;
        let code = self
            .ui
            .js(
                "return document.querySelector('.chapter-main .verse[data-translation]').dataset.translation;",
                Vec::new(),
            )
            .await?;
        let book = self
            .ui
            .js(
                "return Number(document.getElementById('reading-pane-inner').dataset.currentBook);",
                Vec::new(),
            )
            .await?;
        let expected = self
            .expected_copy(json!({ "translationCode": code, "bookId": book, "chapter": 3, "start": 16, "end": 17 }))
            .await?;
        let reference = expected["reference"].as_str().unwrap_or("?").to_owned();
        self.focus_page().await?;
        self.chord("press Ctrl+C", &[Key::Control], "c").await?;
        self.wait_for_status(&format!("Copied {reference}")).await?;
        self.expect_clipboard(
            "Ctrl+C",
            expected["formats"]["reference"].as_str().unwrap_or(""),
        )
        .await?;

        self.focus_page().await?;
        self.chord("press Ctrl+Shift+C", &[Key::Control, Key::Shift], "C")
            .await?;
        self.reading_is(
            "Copy as… open, focus in it",
            "seen.menu && seen.menuFocus",
            Vec::new(),
        )
        .await?;
        self.ui
            .press("press Escape (close Copy as…)", Key::Escape)
            .await?;
        self.reading_is(
            "Copy as… closed, the selection kept",
            "!seen.menu && JSON.stringify(seen.selected) === '[16,17]'",
            Vec::new(),
        )
        .await?;
        self.focus_page().await?;
        self.ui.press("press Escape", Key::Escape).await?;
        let read_by = if self.config.xvfb.is_some() {
            "pasted on the private display"
        } else {
            "Get-Clipboard"
        };
        Ok(format!(
            "{:?} selected by keyboard; Ctrl+C: \"Copied {reference}\", the clipboard ({read_by}) the backend's \
             text; Ctrl+Shift+C: Copy as… open with focus in it, Escape closed it",
            seen["selected"],
        ))
    }

    /// Check 56: Alt+← and Alt+→ go back and forward through the passages
    /// jumped to; N opens the note editor on the selected verses (Escape
    /// closes it, nothing saved).
    pub(super) async fn key_history_note(&mut self) -> Result<String> {
        self.go_to("John 3", "John 3").await?;
        self.go_to("Romans 8", "Romans 8").await?;
        self.focus_page().await?;
        self.chord("press Alt+←", &[Key::Alt], Key::Left).await?;
        self.reading_is("back to John 3", "seen.heading === 'John 3'", Vec::new())
            .await?;
        self.focus_page().await?;
        self.chord("press Alt+→", &[Key::Alt], Key::Right).await?;
        self.reading_is(
            "forward to Romans 8",
            "seen.heading === 'Romans 8'",
            Vec::new(),
        )
        .await?;

        self.focus_page().await?;
        self.ui.press("press j", "j").await?;
        let selected = self
            .reading_is("a verse selected", "seen.selected.length === 1", Vec::new())
            .await?;
        let n = selected["selected"][0].as_u64().unwrap_or(0);
        self.focus_page().await?;
        self.ui.press("press N", "n").await?;
        let open = self
            .reading_is(
                "the note editor open on the verse",
                "seen.editor && seen.editorTitle && seen.editorTitle.includes(arguments[0])",
                vec![json!(format!("Romans 8:{n}"))],
            )
            .await?;
        self.shot(
            "screenshot: the note editor opened by N, Chi Rho",
            KEYS_SHOTS[1],
        )
        .await;
        self.ui
            .press("press Escape (close the editor)", Key::Escape)
            .await?;
        self.reading_is("the editor closed", "!seen.editor", Vec::new())
            .await?;
        self.focus_page().await?;
        self.ui.press("press Escape", Key::Escape).await?;
        Ok(format!(
            "John 3, Romans 8; Alt+← John 3, Alt+→ Romans 8; N on Romans 8:{n}: \"{}\"",
            open["editorTitle"].as_str().unwrap_or("?")
        ))
    }

    /// Runs the palette's command `id`, found by typing `words` after `>`,
    /// with Enter.
    async fn palette_run(&self, words: &str, id: &str) -> Result<()> {
        self.open_search_by_key().await?;
        self.type_in_box(&format!(">{words}")).await?;
        self.ui
            .wait_with(
                &format!("the command {id} to be offered first, enabled"),
                "const option = document.querySelector('#search-results .search-command');
                 return [!!option && option.dataset.command === arguments[0] && !option.hasAttribute('aria-disabled'),
                     option && option.textContent];",
                vec![json!(id)],
            )
            .await?;
        self.ui
            .press(&format!("press Enter (run {id})"), Key::Enter)
            .await
    }

    /// Check 57: the palette runs commands whose code was the page's: Next
    /// chapter, Back (after a jump), and Copy chapter (the clipboard read
    /// back from outside the page).
    pub(super) async fn palette_page_commands(&mut self) -> Result<String> {
        self.go_to("John 3", "John 3").await?;
        self.palette_run("next chapter", "next-chapter").await?;
        self.reading_is("John 4", "seen.heading === 'John 4'", Vec::new())
            .await?;
        self.go_to("Romans 8", "Romans 8").await?;
        self.palette_run("back history", "back").await?;
        self.reading_is("back to John 4", "seen.heading === 'John 4'", Vec::new())
            .await?;
        let code = self
            .ui
            .js(
                "return document.querySelector('.chapter-main .verse[data-translation]').dataset.translation;",
                Vec::new(),
            )
            .await?;
        let book = self
            .ui
            .js(
                "return Number(document.getElementById('reading-pane-inner').dataset.currentBook);",
                Vec::new(),
            )
            .await?;
        let chapter = self
            .expected_copy(json!({ "translationCode": code, "bookId": book, "chapter": 4 }))
            .await?;
        let reference = chapter["reference"].as_str().unwrap_or("?").to_owned();
        self.palette_run("copy chapter", "copy-chapter").await?;
        let toast = self
            .reading_is(
                "the toast to say the chapter was copied",
                "seen.toast === arguments[0]",
                vec![json!(format!("Copied {reference}"))],
            )
            .await?;
        self.expect_clipboard(
            "the palette's Copy chapter",
            chapter["formats"]["reference"].as_str().unwrap_or(""),
        )
        .await?;
        self.ui
            .wait_within(
                "the toast to go",
                "return [document.getElementById('data-status').classList.contains('is-hidden'), null];",
                Vec::new(),
                Some(std::time::Duration::from_secs(20)),
            )
            .await?;
        Ok(format!(
            "Next chapter: John 4; Back after Romans 8: John 4; Copy chapter: \"{}\", the clipboard the backend's text",
            toast["toast"].as_str().unwrap_or("?")
        ))
    }

    /// Check 54's second screenshot, in Vellum.
    pub(super) async fn keys_vellum_screenshots(&mut self) -> Result<()> {
        self.go_to("John 3", "John 3").await?;
        self.select_by_keys(16).await?;
        self.chord("press Shift+J", &[Key::Shift], "J").await?;
        self.reading_is(
            "verses 16 and 17 selected",
            "JSON.stringify(seen.selected) === '[16,17]'",
            Vec::new(),
        )
        .await?;
        self.shot(
            "screenshot: verses selected by keyboard, Vellum",
            KEYS_SHOTS[2],
        )
        .await;
        self.focus_page().await?;
        self.ui.press("press Escape", Key::Escape).await?;
        self.plain_reading().await
    }

    /// Check 58, first after the restart: a cold start goes straight to the
    /// chapter the first session left (`saved`, its heading), its verses on
    /// screen, the window titled, with nothing said above it.
    pub(super) async fn cold_start(&mut self, saved: Option<&str>) -> Result<String> {
        let Some(saved) = saved else {
            anyhow::bail!("not run: the first session didn't say where it left the reader");
        };
        let seen = self
            .ui
            .wait_within(
                "the saved chapter, drawn by the reader, after a cold start",
                "const heading = document.querySelector('.chapter-main .chapter-heading');
                 const verses = document.querySelectorAll('.chapter-main .verse[data-translation]');
                 const notice = document.getElementById('reader-notice');
                 const nav = performance.getEntriesByType('navigation')[0];
                 const seen = { heading: heading ? heading.textContent.trim() : null, verses: verses.length,
                     inner: !!document.getElementById('reading-pane-inner'), title: document.title,
                     notice: notice ? (notice.hidden ? '' : notice.textContent) : null,
                     loaded: nav ? Math.round(nav.domContentLoadedEventEnd) : null };
                 return [seen.heading === arguments[0] && seen.verses > 0 && seen.inner
                     && seen.title.startsWith(arguments[0]) && seen.notice === '', seen];",
                vec![json!(saved)],
                Some(self.config.session_timeout),
            )
            .await?;
        ensure!(seen["verses"].as_u64().unwrap_or(0) > 0, "no verses");
        Ok(format!(
            "after the restart: {} ({} verses, the reader's markup), titled \"{}\", nothing above it; page read in {} ms",
            seen["heading"].as_str().unwrap_or("?"),
            seen["verses"],
            seen["title"].as_str().unwrap_or("?"),
            seen["loaded"],
        ))
    }
}
