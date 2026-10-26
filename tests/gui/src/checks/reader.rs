//! Checks 38 to 45: the reader, which the Rust pieces (ui-leptos) took over
//! from the page's main script in step 7a, each through the real UI. The
//! chapter's end turns the chapter, from its top (38); Compare's grid, with
//! its column count set through the CSSOM, and another translation to
//! compare with (39); the chapter's Footnotes and Cross references open and
//! close, stay as left from one chapter to the next (40), and after a
//! restart (45, in the second session; not run on Windows, for check 26's
//! reason); a cross-reference opens its passage as a jump, and the mouse's
//! back and forward buttons go back and forth (41); Psalm 119, the longest
//! chapter, in full (42); a chapter that couldn't be read says so, and Try
//! again reads it (43, the failure made by a stand-in for the page's own
//! `invoke`, turning down one `get_reading`); and the plain-text fallback the page keeps for
//! when the Rust pieces fail to load (44, last of the first session, which
//! it leaves showing the fallback: forced by the loader's own failure
//! event, it reads the chapter again and shows it as plain text).
//! Screenshots in Chi Rho and Vellum.

use std::time::Instant;

use anyhow::{Result, bail, ensure};
use serde_json::{Value, json};
use thirtyfour::prelude::*;

use super::{DESCRIBE_JS, Run, VISIBLE_JS};

/// The screenshots checks 38 to 44 take (check 4 lists them).
pub(super) const READER_SHOTS: [&str; 7] = [
    "38-chapter-end-chi-rho.png",
    "39-compare-bsb-chi-rho.png",
    "40-lists-open-chi-rho.png",
    "42-psalm-119-chi-rho.png",
    "43-try-again-chi-rho.png",
    "42-psalm-119-vellum.png",
    "44-plain-text-vellum.png",
];

/// The chapter on screen as the reader shows it: the heading, the
/// reference button, the title, the pane's neighbors, the selection, the
/// notice, and how far down the page is.
const HERE_JS: &str = "const heading = document.querySelector('.chapter-main .chapter-heading');
    const inner = document.getElementById('reading-pane-inner');
    const notice = document.getElementById('reader-notice');
    return { heading: heading ? heading.textContent.trim() : null,
             reference: document.getElementById('ref-text').textContent,
             title: document.title,
             next: inner ? inner.dataset.nextChapter || null : null,
             selected: [...document.querySelectorAll('.chapter-main .verse-selected[data-translation]')]
                 .map((v) => Number(v.dataset.verse)),
             notice: notice.hidden ? '' : notice.textContent,
             scrollY: Math.round(window.scrollY) };";

/// The disclosures as the page shows and keeps them.
const LISTS_JS: &str = "const toggle = (kind) => document.querySelector(`.disclosure-toggle[data-disclosure=\"${kind}\"]`);
    const state = (kind) => { const t = toggle(kind); if (!t) return null;
        const body = document.getElementById(t.getAttribute('aria-controls'));
        return { expanded: t.getAttribute('aria-expanded'), shown: !!body && !body.hidden }; };
    const stored = (key) => { try { return localStorage.getItem(key); } catch (e) { return 'unreadable'; } };
    return { footnotes: state('footnotes'), xrefs: state('xrefs'),
             saved: { footnotes: stored('gospel-getter-footnotes'), xrefs: stored('gospel-getter-cross-references') } };";

/// The page's own `invoke`, made to turn down the next `get_reading` (as a
/// chapter that couldn't be read): the reader calls it through
/// `window.__TAURI__.core`, looked up on each call. Tauri freezes `core`
/// itself, so a copy takes its place, with an `invoke` of its own. Put back
/// by [`RESTORE_INVOKE_JS`].
const FAIL_NEXT_READING_JS: &str = "const tauri = window.__TAURI__;
    if (!window.__guiCheckCore) window.__guiCheckCore = tauri.core;
    const real = window.__guiCheckCore;
    let failed = false;
    tauri.core = Object.assign({}, real, {
        invoke(cmd, args, options) {
            if (cmd === 'get_reading' && !failed) {
                failed = true;
                return Promise.reject('Psalms 121 couldn’t be loaded. (gui-check)');
            }
            return real.invoke(cmd, args, options);
        },
    });
    return tauri.core !== real;";

const RESTORE_INVOKE_JS: &str =
    "if (window.__guiCheckCore) window.__TAURI__.core = window.__guiCheckCore;
    return window.__TAURI__.core === window.__guiCheckCore;";

/// The plain-text fallback's line (`ui/index.html`).
const PLAIN_TEXT: &str =
    "This chapter is shown as plain text: the rest of the reader isn’t available.";

impl Run<'_> {
    async fn here(&self) -> Result<Value> {
        self.ui.js(HERE_JS, Vec::new()).await
    }

    /// Goes to `reference` through the search box (`/`, the reference,
    /// Enter), and waits for `heading` to be read.
    pub(super) async fn go_to(&self, reference: &str, heading: &str) -> Result<Value> {
        self.plain_reading().await?;
        self.open_search_by_key().await?;
        self.type_in_box(reference).await?;
        self.ui
            .wait_with(
                &format!("Go to {reference} to be offered"),
                "const status = document.getElementById('search-status');
                 return [!!status && status.textContent === 'Press Enter to go there.', status && status.textContent];",
                Vec::new(),
            )
            .await?;
        self.ui.press("press Enter (go there)", Key::Enter).await?;
        self.ui
            .wait_with(
                &format!("{heading} to be read, the box closed"),
                &format!(
                    "const seen = (() => {{ {HERE_JS} }})();
                     return [seen.heading === arguments[0] && seen.title === `${{seen.reference}} — Gospel Getter`
                         && document.getElementById('search-panel').hidden, seen];"
                ),
                vec![json!(heading)],
            )
            .await
    }

    /// Check 38: the chapter's end turns the chapter, from its top: Next,
    /// then Previous, the reference button and the title following.
    pub(super) async fn chapter_steps(&mut self) -> Result<String> {
        self.go_to("John 3", "John 3").await?;
        let ui = &self.ui;
        ui.js(
            "document.querySelector('.chapter-end').scrollIntoView({ block: 'center' }); return null;",
            Vec::new(),
        )
        .await?;
        self.shot("screenshot: the chapter's end, Chi Rho", READER_SHOTS[0])
            .await;
        let ui = &self.ui;
        let before = self.here().await?;
        ensure!(
            before["scrollY"].as_i64().unwrap_or(0) > 0,
            "the chapter's end didn't scroll into view: {before}"
        );
        ui.click("click the next chapter's button", ".chapter-step-next")
            .await?;
        let next = ui
            .wait(
                "John 4 to be read, from its top",
                &format!(
                    "const seen = (() => {{ {HERE_JS} }})();
                     return [seen.heading === 'John 4' && seen.reference === 'John 4 · KJV'
                         && seen.title === 'John 4 · KJV — Gospel Getter' && seen.scrollY === 0, seen];"
                ),
            )
            .await?;
        ui.js(
            "document.querySelector('.chapter-end').scrollIntoView({ block: 'center' }); return null;",
            Vec::new(),
        )
        .await?;
        ui.click("click the previous chapter's button", ".chapter-step-prev")
            .await?;
        ui.wait(
            "John 3 to be read again, from its top",
            &format!(
                "const seen = (() => {{ {HERE_JS} }})();
                 return [seen.heading === 'John 3' && seen.scrollY === 0, seen];"
            ),
        )
        .await?;
        Ok(format!(
            "from John 3's end (scrolled {}px): Next read {} from its top (\"{}\", title \"{}\"); Previous read \
             John 3 again, from its top",
            before["scrollY"],
            next["heading"].as_str().unwrap_or("?"),
            next["reference"].as_str().unwrap_or("?"),
            next["title"].as_str().unwrap_or("?"),
        ))
    }

    /// Opens the translation button's popover.
    async fn open_translations(&self) -> Result<()> {
        let ui = &self.ui;
        ui.click("open the translations", "#translation-toggle")
            .await?;
        ui.wait(
            "the translations to show",
            &format!(
                "{VISIBLE_JS} return [visible(document.getElementById('translation-popover')), null];"
            ),
        )
        .await?;
        Ok(())
    }

    async fn close_translations(&self) -> Result<()> {
        let ui = &self.ui;
        ui.press("press Escape (close the translations)", Key::Escape)
            .await?;
        ui.wait(
            "the translations to close",
            "return [document.getElementById('translation-popover').hidden, null];",
        )
        .await?;
        Ok(())
    }

    /// The grid as it's laid out: its heads, its column count (set through
    /// the CSSOM, as the computed style reads it), and its gaps.
    async fn grid(&self, what: &str, heads: &[&str]) -> Result<Value> {
        self.ui
            .wait_with(
                what,
                "const grid = document.querySelector('.compare-grid');
                 const heads = [...document.querySelectorAll('.compare-head')].map((h) => h.textContent.trim());
                 const seen = { heads, columns: grid ? getComputedStyle(grid).getPropertyValue('--compare-columns').trim() : null,
                     gaps: [...document.querySelectorAll('.compare-gap')].map((g) => `${g.dataset.code} ${g.dataset.gapVerse}`),
                     reference: document.getElementById('ref-text').textContent };
                 return [JSON.stringify(heads) === JSON.stringify(arguments[0]) && seen.columns === String(arguments[0].length), seen];",
                vec![json!(heads)],
            )
            .await
    }

    /// Check 39: Compare on (C), its grid two columns wide by the CSSOM;
    /// the BSB chosen to compare with, its gaps marked; the KJV's partner
    /// put back, Compare off.
    pub(super) async fn compare_grid(&mut self) -> Result<String> {
        self.go_to("Matthew 17", "Matthew 17").await?;
        // Read in the KJV, compared with the WEB, whatever came before.
        self.open_translations().await?;
        self.ui
            .click("read the KJV", "#translation-choices [data-value=\"kjv\"]")
            .await?;
        self.ui
            .click(
                "Compare with: the WEB",
                "#compare-with-choices [data-value=\"web\"]",
            )
            .await?;
        self.close_translations().await?;
        let ui = &self.ui;
        ui.click(
            "click the chapter heading (focus off any control)",
            ".chapter-main .chapter-heading",
        )
        .await?;
        ui.press("press C (Compare on)", "c").await?;
        let on = self
            .grid(
                "the KJV and the WEB side by side",
                &["King James Version (KJV)", "World English Bible (WEB)"],
            )
            .await?;
        self.open_translations().await?;
        self.ui
            .click(
                "Compare with: the BSB",
                "#compare-with-choices [data-value=\"bsb\"]",
            )
            .await?;
        self.close_translations().await?;
        let bsb = self
            .grid(
                "the KJV and the BSB side by side",
                &["King James Version (KJV)", "Berean Standard Bible (BSB)"],
            )
            .await?;
        let gaps: Vec<&str> = bsb["gaps"]
            .as_array()
            .map(|g| g.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        ensure!(
            gaps.contains(&"BSB 21"),
            "Matthew 17:21 isn't marked as not numbered in the BSB: {bsb}"
        );
        self.shot("screenshot: Compare with the BSB, Chi Rho", READER_SHOTS[1])
            .await;
        // Back as it was: the WEB beside the KJV, Compare off.
        self.open_translations().await?;
        self.ui
            .click(
                "Compare with: the WEB",
                "#compare-with-choices [data-value=\"web\"]",
            )
            .await?;
        self.close_translations().await?;
        let ui = &self.ui;
        ui.click(
            "click the chapter heading (focus off any control)",
            ".chapter-main .chapter-heading",
        )
        .await?;
        ui.press("press C (Compare off)", "c").await?;
        ui.wait(
            "the chapter on its own again",
            "return [!document.querySelector('.compare-grid') && !!document.querySelector('.chapter-main .chapter-text'),
                     document.getElementById('ref-text').textContent];",
        )
        .await?;
        Ok(format!(
            "C: {} ({} columns by the CSSOM, \"{}\"); with the BSB: {}, its gaps {gaps:?}; back to the WEB, C: \
             one column",
            on["heads"],
            on["columns"],
            on["reference"].as_str().unwrap_or("?"),
            bsb["heads"],
        ))
    }

    async fn lists(&self) -> Result<Value> {
        self.ui.js(LISTS_JS, Vec::new()).await
    }

    /// Opens (or closes) a disclosure by its button, and waits for it.
    async fn set_list(&self, kind: &str, open: bool) -> Result<Value> {
        let ui = &self.ui;
        let selector = format!(".disclosure-toggle[data-disclosure=\"{kind}\"]");
        ui.js(
            "document.querySelector(arguments[0]).scrollIntoView({ block: 'center' }); return null;",
            vec![json!(selector)],
        )
        .await?;
        ui.click(
            &format!("{} the {kind} list", if open { "open" } else { "close" }),
            &selector,
        )
        .await?;
        ui.wait_with(
            &format!("the {kind} list {}", if open { "open" } else { "closed" }),
            &format!(
                "const seen = (() => {{ {LISTS_JS} }})(); const want = arguments[0], kind = arguments[1];
                 return [seen[kind].expanded === String(want) && seen[kind].shown === want
                     && seen.saved[kind] === (want ? 'open' : 'closed'), seen];"
            ),
            vec![json!(open), json!(kind)],
        )
        .await
    }

    /// Check 40: Footnotes and Cross references open, and stay open into
    /// the next chapter (Psalm 119's into Psalm 120); left open for check 45.
    pub(super) async fn disclosures(&mut self) -> Result<String> {
        self.go_to("Psalm 119", "Psalms 119").await?;
        let closed = self.lists().await?;
        ensure!(
            closed["footnotes"]["expanded"] == "false" && closed["xrefs"]["expanded"] == "false",
            "the lists aren't closed at first: {closed}"
        );
        self.set_list("footnotes", true).await?;
        let open = self.set_list("xrefs", true).await?;
        self.shot(
            "screenshot: Psalm 119's lists open, Chi Rho",
            READER_SHOTS[2],
        )
        .await;
        self.set_list("footnotes", false).await?;
        self.set_list("footnotes", true).await?;
        let ui = &self.ui;
        ui.js("window.scrollTo(0, 0); return null;", Vec::new())
            .await?;
        ui.click(
            "click the chapter heading (focus off any control)",
            ".chapter-main .chapter-heading",
        )
        .await?;
        ui.press("press → (Psalm 120)", Key::Right).await?;
        let next = ui
            .wait(
                "Psalm 120 to be read, its lists still open",
                &format!(
                    "const seen = (() => {{ {LISTS_JS} }})();
                     const heading = document.querySelector('.chapter-main .chapter-heading').textContent.trim();
                     return [heading === 'Psalms 120' && (!seen.footnotes || seen.footnotes.expanded === 'true')
                         && seen.xrefs && seen.xrefs.expanded === 'true' && seen.xrefs.shown, {{ heading, ...seen }}];"
                ),
            )
            .await?;
        Ok(format!(
            "Psalm 119: both closed at first; opened (saved {}), Footnotes closed and opened again; → Psalm 120: \
             still open ({})",
            open["saved"], next["xrefs"]
        ))
    }

    /// Check 41: a citation in the chapter's Cross references opens its
    /// passage as a jump (its verses selected); the mouse's back button
    /// returns, its forward button goes there again.
    pub(super) async fn back_forward(&mut self) -> Result<String> {
        let ui = &self.ui;
        let from = self.here().await?;
        let chip = ui
            .js(
                "const chip = document.querySelector('.xref-summary-list:not([hidden]) .xref-summary-chip');
                 if (!chip) return null;
                 chip.scrollIntoView({ block: 'center' });
                 return { text: chip.textContent, book: chip.dataset.book, chapter: chip.dataset.chapter,
                          verse: Number(chip.dataset.verse), end: Number(chip.dataset.endVerse || chip.dataset.verse) };",
                Vec::new(),
            )
            .await?;
        if chip.is_null() {
            bail!("no cross-reference to open in the chapter's list: {from}");
        }
        ui.click(
            &format!("open the cross-reference {}", chip["text"]),
            ".xref-summary-list:not([hidden]) .xref-summary-chip",
        )
        .await?;
        let landed = ui
            .wait_with(
                "its passage to be read, its verses selected",
                &format!(
                    "const seen = (() => {{ {HERE_JS} }})(); const chip = arguments[0], from = arguments[1];
                     return [seen.heading !== from && seen.selected.length > 0 && seen.selected[0] === chip.verse
                         && seen.selected[seen.selected.length - 1] === chip.end, seen];"
                ),
                vec![chip.clone(), from["heading"].clone()],
            )
            .await?;
        let mouse = |button: u8| {
            format!(
                "window.dispatchEvent(new MouseEvent('mouseup', {{ button: {button}, bubbles: true, cancelable: true }})); return null;"
            )
        };
        ui.js(&mouse(3), Vec::new()).await?;
        let back = ui
            .wait_with(
                "the mouse's back button: the chapter before, where it was left",
                &format!(
                    "const seen = (() => {{ {HERE_JS} }})();
                     return [seen.heading === arguments[0], seen];"
                ),
                vec![from["heading"].clone()],
            )
            .await?;
        ui.js(&mouse(4), Vec::new()).await?;
        let forward = ui
            .wait_with(
                "the mouse's forward button: the passage again, selected",
                &format!(
                    "const seen = (() => {{ {HERE_JS} }})();
                     return [seen.heading === arguments[0] && JSON.stringify(seen.selected) === JSON.stringify(arguments[1]), seen];"
                ),
                vec![landed["heading"].clone(), landed["selected"].clone()],
            )
            .await?;
        Ok(format!(
            "{} opened {} with {} selected; back button: {} (scrolled {}px); forward button: {} with {} selected",
            chip["text"].as_str().unwrap_or("?"),
            landed["heading"].as_str().unwrap_or("?"),
            landed["selected"],
            back["heading"].as_str().unwrap_or("?"),
            back["scrollY"],
            forward["heading"].as_str().unwrap_or("?"),
            forward["selected"],
        ))
    }

    /// Check 42: Psalm 119, the longest chapter: all 176 verses, the last
    /// one there to read, and how long it took to show.
    pub(super) async fn long_chapter(&mut self) -> Result<String> {
        self.go_to("John 1", "John 1").await?;
        let begun = Instant::now();
        self.go_to("Psalm 119", "Psalms 119").await?;
        let shown_in = begun.elapsed();
        let seen = self
            .ui
            .js(
                "const verses = [...document.querySelectorAll('.chapter-main .chapter-text > .verse[data-translation]')];
                 const last = verses[verses.length - 1];
                 last.scrollIntoView({ block: 'center' });
                 return { count: verses.length, last: last ? Number(last.dataset.verse) : null,
                          text: last ? last.querySelector('.verse-text').textContent.slice(0, 40) : null };",
                Vec::new(),
            )
            .await?;
        ensure!(
            seen["count"] == 176 && seen["last"] == 176,
            "Psalm 119 isn't all there: {seen}"
        );
        self.shot("screenshot: Psalm 119, Chi Rho", READER_SHOTS[3])
            .await;
        Ok(format!(
            "Psalm 119: {} verses, the last {} (\"{}…\"); from Enter to the chapter read in {:.0} ms (with the box's \
             own steps)",
            seen["count"],
            seen["last"],
            seen["text"].as_str().unwrap_or("?"),
            shown_in.as_secs_f64() * 1000.0
        ))
    }

    /// Check 43: a chapter that couldn't be read (one `get_reading` turned
    /// down by the page's `invoke`) says so, the chapter on screen staying,
    /// with a toast; Try again reads it.
    pub(super) async fn try_again(&mut self) -> Result<String> {
        self.go_to("Psalm 120", "Psalms 120").await?;
        let ui = &self.ui;
        let rigged = ui.js(FAIL_NEXT_READING_JS, Vec::new()).await?;
        ensure!(
            rigged == Value::Bool(true),
            "the page's invoke couldn't be wrapped"
        );
        ui.click(
            "click the chapter heading (focus off any control)",
            ".chapter-main .chapter-heading",
        )
        .await?;
        ui.press("press → (Psalm 121, which fails)", Key::Right)
            .await?;
        let failed = ui
            .wait(
                "the notice to say so, with Try again; Psalm 120 stays, and the toast says it too",
                &format!(
                    "{VISIBLE_JS} const seen = (() => {{ {HERE_JS} }})();
                     const toast = document.getElementById('data-status');
                     const retry = document.querySelector('#reader-notice .reading-retry');
                     return [seen.heading === 'Psalms 120' && seen.notice.startsWith('Psalms 121 couldn’t be loaded.')
                         && visible(retry) && !toast.classList.contains('is-hidden') && toast.classList.contains('is-error'),
                         {{ ...seen, toast: toast.textContent, retry: retry && retry.textContent }}];"
                ),
            )
            .await?;
        self.shot(
            "screenshot: a chapter that couldn't be read, Try again, Chi Rho",
            READER_SHOTS[4],
        )
        .await;
        let ui = &self.ui;
        let restored = ui.js(RESTORE_INVOKE_JS, Vec::new()).await?;
        ensure!(
            restored == Value::Bool(true),
            "the page's invoke couldn't be put back"
        );
        ui.js("window.scrollTo(0, 0); return null;", Vec::new())
            .await?;
        ui.click("click Try again", "#reader-notice .reading-retry")
            .await?;
        let again = ui
            .wait(
                "Psalm 121 to be read, the notice gone",
                &format!(
                    "const seen = (() => {{ {HERE_JS} }})();
                     return [seen.heading === 'Psalms 121' && seen.notice === '', seen];"
                ),
            )
            .await?;
        // An error's toast stays 12 s: the next checks' screenshots wait for
        // none.
        ui.wait_within(
            "the toast to go",
            "return [document.getElementById('data-status').classList.contains('is-hidden'), null];",
            Vec::new(),
            Some(std::time::Duration::from_secs(20)),
        )
        .await?;
        Ok(format!(
            "→ from Psalm 120 with get_reading turned down: \"{}\" ({}), Psalm 120 still on screen, toast \"{}\"; \
             Try again: {} read, the notice gone",
            failed["notice"].as_str().unwrap_or("?"),
            failed["retry"].as_str().unwrap_or("?"),
            failed["toast"].as_str().unwrap_or("?"),
            again["heading"].as_str().unwrap_or("?"),
        ))
    }

    /// Check 42's second screenshot: Psalm 119 in Vellum.
    pub(super) async fn reader_vellum_screenshots(&mut self) -> Result<()> {
        self.go_to("Psalm 119", "Psalms 119").await?;
        self.shot("screenshot: Psalm 119, Vellum", READER_SHOTS[5])
            .await;
        Ok(())
    }

    /// Check 44, last of the first session: the page's plain-text fallback,
    /// as the loader would have it if the Rust pieces failed to load (its
    /// event, sent to the page here): the chapter the app saved, read again
    /// and shown as plain text, nothing in it to select, and the line saying
    /// so. It runs under the Content Security Policy as the page's own
    /// inline script (check 2 sees any violation).
    pub(super) async fn plain_text_fallback(&mut self) -> Result<String> {
        let ui = &self.ui;
        let saved = self.here().await?;
        ui.js(
            "window.scrollTo(0, 0); document.dispatchEvent(new CustomEvent('gospel-getter:pieces-failed')); return null;",
            Vec::new(),
        )
        .await?;
        let seen = ui
            .wait_with(
                "the chapter as plain text, with the line saying the rest isn't available",
                &format!(
                    "{DESCRIBE_JS} const notice = document.getElementById('reader-notice');
                     const heading = document.querySelector('#reading-pane .chapter-main .chapter-heading');
                     const verses = document.querySelectorAll('#reading-pane .chapter-main .chapter-text > .verse');
                     const seen = {{ notice: notice.hidden ? '' : notice.textContent,
                         heading: heading ? heading.textContent : null, verses: verses.length,
                         selectable: document.querySelectorAll('.verse[data-translation]').length,
                         inner: !!document.getElementById('reading-pane-inner'),
                         first: verses[0] ? verses[0].textContent.slice(0, 40) : null }};
                     return [seen.notice === arguments[0] && seen.heading === arguments[1] && seen.verses > 0
                         && seen.selectable === 0 && !seen.inner, seen];"
                ),
                vec![json!(PLAIN_TEXT), saved["heading"].clone()],
            )
            .await?;
        self.shot(
            "screenshot: the plain-text fallback, Vellum",
            READER_SHOTS[6],
        )
        .await;
        Ok(format!(
            "the loader's failure event: \"{}\"; {} as plain text ({} verses, \"{}…\"), none selectable, no \
             #reading-pane-inner",
            seen["notice"].as_str().unwrap_or("?"),
            seen["heading"].as_str().unwrap_or("?"),
            seen["verses"],
            seen["first"].as_str().unwrap_or("?"),
        ))
    }

    /// Check 45, after the restart: the lists check 40 left open are open
    /// in a chapter that has them; then closed again, as they were.
    pub(super) async fn disclosures_kept(&mut self) -> Result<String> {
        self.go_to("Psalm 119", "Psalms 119").await?;
        let seen = self.lists().await?;
        ensure!(
            seen["footnotes"]["expanded"] == "true"
                && seen["footnotes"]["shown"] == true
                && seen["xrefs"]["expanded"] == "true"
                && seen["xrefs"]["shown"] == true,
            "the lists aren't open after the restart: {seen}"
        );
        self.set_list("footnotes", false).await?;
        let closed = self.set_list("xrefs", false).await?;
        Ok(format!(
            "after the restart, Psalm 119's Footnotes and Cross references open as left (saved {}); closed \
             again (saved {})",
            seen["saved"], closed["saved"]
        ))
    }
}
