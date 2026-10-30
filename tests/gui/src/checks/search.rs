//! Checks 17 to 19: search and the command palette, which the Rust pieces
//! (ui-leptos) took over from the page's JavaScript, each through the real
//! UI. A word is searched (the verses come with the word marked), Show
//! more adds the next page (the status line counts them) and a result
//! opens its verse, the box out of the way (17); a reference typed is
//! offered as Go to and Enter goes there (18); and a command typed after
//! ">" runs as its own control would, focus back where it goes (19).
//!
//! Check 17 also measures, as evidence and without failing for it, how
//! much of Show more the panel shows once scrolled to its end, at the
//! window's size and at the app's smallest height (480 px tall,
//! `minHeight` in tauri.conf.json): a tester saw it half hidden (D4 in the
//! GUI review). Screenshots in Chi Rho here, and in Vellum with check 4's.

use anyhow::{Result, bail};
use serde_json::{Value, json};
use thirtyfour::prelude::*;

use super::{DESCRIBE_JS, Run, VISIBLE_JS};

/// The search's and the palette's screenshots: in Chi Rho (checks 17 to
/// 19), and in Vellum (with check 4's).
pub(super) const SEARCH_SHOTS: [&str; 7] = [
    "17-search-results-chi-rho.png",
    "17-search-show-more-chi-rho.png",
    "17-search-show-more-small-chi-rho.png",
    "18-search-go-to-chi-rho.png",
    "19-palette-command-chi-rho.png",
    "17-search-show-more-vellum.png",
    "19-palette-command-vellum.png",
];

/// The word check 17 searches for: in every bundled translation, on more
/// than two pages of results.
const WORD: &str = "light";

/// The app's smallest window height (tauri.conf.json `minHeight`).
const SMALLEST_HEIGHT: u32 = 480;

/// What the reader sees of the search box: open or not, the status line,
/// the groups, the results (their references, and the words marked in
/// them), the active option, Show more and the focus.
const BOX_JS: &str = "const panel = document.getElementById('search-panel');
    const input = document.getElementById('search-input');
    const status = document.getElementById('search-status');
    const results = [...document.querySelectorAll('#search-results .search-result:not(.search-go)')];
    const active = input && input.getAttribute('aria-activedescendant');
    const more = document.getElementById('search-more');
    return { open: !panel.hidden && visible(panel),
             status: status ? status.textContent : null,
             groups: [...document.querySelectorAll('#search-results [role=\"group\"]')]
                 .map((g) => document.getElementById(g.getAttribute('aria-labelledby')).textContent),
             refs: results.map((r) => r.querySelector('.search-ref').textContent),
             marks: results.map((r) => [...r.querySelectorAll('.search-text mark')].map((m) => m.textContent)),
             active: active ? document.getElementById(active).textContent.trim() : null,
             activeKey: active ? document.getElementById(active).dataset.key : null,
             more: !!more && !more.hidden,
             focus: describe(document.activeElement) };";

/// Scrolls the panel to its end and measures Show more there: how many of
/// its pixels the panel (inside its border) and the window show.
const SHOW_MORE_AT_END_JS: &str = "const panel = document.getElementById('search-panel');
    panel.scrollTop = panel.scrollHeight;
    const more = document.getElementById('search-more').getBoundingClientRect();
    const box = panel.getBoundingClientRect();
    const inside = { top: box.top + panel.clientTop, bottom: box.top + panel.clientTop + panel.clientHeight };
    const top = Math.max(more.top, inside.top, 0);
    const bottom = Math.min(more.bottom, inside.bottom, window.innerHeight);
    const shown = Math.max(0, bottom - top);
    return { window: `${window.innerWidth}x${window.innerHeight}`,
             panel: { top: Math.round(box.top), bottom: Math.round(box.bottom), scrollHeight: panel.scrollHeight,
                      clientHeight: panel.clientHeight, scrollTop: Math.round(panel.scrollTop) },
             button: { top: Math.round(more.top), bottom: Math.round(more.bottom), height: Math.round(more.height) },
             shownPx: Math.round(shown), hiddenPx: Math.round(more.height - shown),
             fullyShown: more.height > 0 && shown >= more.height - 0.5 };";

/// Scrolls the panel to its end again whenever it isn't there, until it
/// has stayed there for three polls in a row (the count starts again from
/// nothing on each call).
const STAYS_AT_END_JS: &str = "const panel = document.getElementById('search-panel');
    const s = window.__guiCheckAtEnd || (window.__guiCheckAtEnd = { polls: 0 });
    const end = panel.scrollHeight - panel.clientHeight;
    if (Math.abs(panel.scrollTop - end) > 1) { panel.scrollTop = panel.scrollHeight; s.polls = 0; }
    else s.polls += 1;
    return [s.polls >= 3, { scrollTop: Math.round(panel.scrollTop), end, polls: s.polls }];";

impl Run<'_> {
    /// The search box as the reader sees it.
    async fn search_box(&self) -> Result<Value> {
        self.ui
            .js(&format!("{VISIBLE_JS} {DESCRIBE_JS} {BOX_JS}"), Vec::new())
            .await
    }

    /// Waits until `ready` (a JavaScript expression over `seen`, the box as
    /// [`BOX_JS`] sees it) is true, and returns what it saw.
    async fn box_is(&self, what: &str, ready: &str) -> Result<Value> {
        self.ui
            .wait(
                what,
                &format!(
                    "{VISIBLE_JS} {DESCRIBE_JS} const seen = (() => {{ {BOX_JS} }})();
                     return [!!({ready}), seen];"
                ),
            )
            .await
    }

    /// Opens the search box with `/`, from the reading view (focus off any
    /// control first, as a reader's would be).
    pub(super) async fn open_search_by_key(&self) -> Result<Value> {
        let ui = &self.ui;
        ui.js("window.scrollTo(0, 0); return null;", Vec::new())
            .await?;
        ui.click(
            "click the chapter heading (focus off any control)",
            ".chapter-main .chapter-heading",
        )
        .await?;
        ui.press("press / (open search)", "/").await?;
        self.box_is(
            "the search box to open, focus in it",
            "seen.open && seen.focus === '#search-input'",
        )
        .await
    }

    /// Closes the search box with Escape, if it's open.
    async fn close_search(&self) -> Result<()> {
        let open = self.search_box().await?;
        if open["open"] == Value::Bool(true) {
            self.ui
                .press("press Escape (close search)", Key::Escape)
                .await?;
            self.box_is("the search box to close", "!seen.open").await?;
        }
        Ok(())
    }

    /// Types `text` into the search box (which has focus), as keys.
    pub(super) async fn type_in_box(&self, text: &str) -> Result<()> {
        self.ui
            .press(&format!("type \"{text}\" into the box"), text)
            .await
    }

    /// Waits for the first page of [`WORD`]'s results: 50 verses, each with
    /// the word marked, and the status line saying how many of how many.
    async fn word_results(&self) -> Result<Value> {
        self.box_is(
            &format!("50 verses with \"{WORD}\" marked, and the count"),
            &format!(
                "seen.refs.length === 50 && /^[\\d,]+ verses in the [A-Z]+, showing 50\\.$/.test(seen.status)
                 && seen.marks.every((m) => m.length > 0 && m.every((w) => w.toLowerCase() === '{WORD}'))
                 && seen.more"
            ),
        )
        .await
    }

    /// Scrolls the panel to its end and says how much of Show more shows.
    async fn show_more_at_end(&self, what: &str) -> Result<Value> {
        let seen = self
            .ui
            .step(what, async {
                self.ui.js(SHOW_MORE_AT_END_JS, Vec::new()).await
            })
            .await?;
        self.ui
            .log(&format!("  {}: {seen}", Self::show_more_said(&seen)));
        Ok(seen)
    }

    /// Keeps the panel at its end until it stays there, right before Show
    /// more's screenshot: a late redraw of the list (its active option
    /// scrolled into view) could undo the measurement's scroll, and the
    /// screenshot then showed the top of the list.
    async fn stay_at_end(&self) -> Result<()> {
        self.ui
            .js("delete window.__guiCheckAtEnd; return null;", Vec::new())
            .await?;
        self.ui
            .wait("the panel to stay scrolled to its end", STAYS_AT_END_JS)
            .await?;
        Ok(())
    }

    /// Show more at the panel's end, in words, for the check's detail.
    fn show_more_said(seen: &Value) -> String {
        format!(
            "window {}: Show more {} ({} of {} px shown, panel {}..{} px, scrolled to {} of {})",
            seen["window"].as_str().unwrap_or("?"),
            if seen["fullyShown"] == Value::Bool(true) {
                "fully shown".to_string()
            } else {
                format!("cut off by {} px", seen["hiddenPx"])
            },
            seen["shownPx"],
            seen["button"]["height"],
            seen["panel"]["top"],
            seen["panel"]["bottom"],
            seen["panel"]["scrollTop"],
            seen["panel"]["scrollHeight"],
        )
    }

    /// Check 17's measurement at the smallest height: the window made 480
    /// px tall, the word searched again, Show more measured at the panel's
    /// end (with a screenshot), and the window put back. Evidence only: a
    /// window that can't be resized here says so.
    async fn show_more_at_smallest_height(&mut self) -> Result<String> {
        let rect = self.ui.driver().get_window_rect().await?;
        let resized = self
            .ui
            .driver()
            .set_window_rect(rect.x, rect.y, rect.width as u32, SMALLEST_HEIGHT)
            .await;
        if let Err(e) = resized {
            return Ok(format!(
                "at the smallest height: not measured, the window can't be resized here ({e})"
            ));
        }
        let measured = async {
            self.ui
                .wait(
                    "the window to be the smallest height",
                    "return [window.innerHeight <= 480, window.innerHeight];",
                )
                .await?;
            self.open_search_by_key().await?;
            self.type_in_box(WORD).await?;
            self.word_results().await?;
            let seen = self
                .show_more_at_end("measure Show more at the panel's end, smallest height")
                .await?;
            self.stay_at_end().await?;
            self.shot(
                "screenshot: Show more at the panel's end, smallest height, Chi Rho",
                SEARCH_SHOTS[2],
            )
            .await;
            Ok::<_, anyhow::Error>(seen)
        }
        .await;
        // The box closes and the window goes back to its size whatever
        // happened.
        let closed = self.close_search().await;
        self.ui
            .driver()
            .set_window_rect(rect.x, rect.y, rect.width as u32, rect.height as u32)
            .await?;
        let height = rect.height;
        self.ui
            .wait_with(
                "the window to be its size again",
                "return [window.outerHeight >= arguments[0] - 2, window.outerHeight];",
                vec![json!(height)],
            )
            .await?;
        let measured = measured?;
        closed?;
        Ok(format!(
            "at the smallest height, {}",
            Self::show_more_said(&measured)
        ))
    }

    /// Check 17: a word searched, Show more, a result opened.
    pub(super) async fn search_word(&mut self) -> Result<String> {
        let mut said = Vec::new();
        self.plain_reading().await?;
        self.expect_theme("chi-rho").await?;
        let ui = &self.ui;
        ui.js("window.scrollTo(0, 0); return null;", Vec::new())
            .await?;
        ui.click("click ⌕ (open search)", "#search-toggle").await?;
        let empty = self
            .box_is(
                "the box to open, focus in it, suggesting commands",
                "seen.open && seen.focus === '#search-input' && seen.groups.join() === 'Commands'
                 && seen.status === 'Type words to search (\"in quotes\" for an exact phrase), a reference to go there, or > for a command.'",
            )
            .await?;
        said.push(format!(
            "⌕ opens the box, focus in it: \"{}\"",
            empty["status"].as_str().unwrap_or_default()
        ));

        self.type_in_box(WORD).await?;
        let first = self.word_results().await?;
        let first_refs: Vec<String> = first["refs"]
            .as_array()
            .map(|r| {
                r.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let marked: usize = first["marks"]
            .as_array()
            .map(|m| m.iter().filter_map(Value::as_array).map(Vec::len).sum())
            .unwrap_or(0);
        said.push(format!(
            "\"{WORD}\": \"{}\", 50 verses, \"{WORD}\" marked {marked} times (first {})",
            first["status"].as_str().unwrap_or_default(),
            first_refs.first().map_or("?", String::as_str)
        ));
        self.shot(
            "screenshot: the search's first page, Chi Rho",
            SEARCH_SHOTS[0],
        )
        .await;

        let at_end = self
            .show_more_at_end("measure Show more at the panel's end")
            .await?;
        said.push(format!("D4 evidence: {}", Self::show_more_said(&at_end)));
        self.stay_at_end().await?;
        self.shot(
            "screenshot: Show more at the panel's end, Chi Rho",
            SEARCH_SHOTS[1],
        )
        .await;

        self.ui.click("click Show more", "#search-more").await?;
        let both = self
            .box_is(
                "the next 50 verses added, the status saying 100 shown",
                "seen.refs.length === 100 && /, showing 100\\.$/.test(seen.status)",
            )
            .await?;
        let refs: Vec<String> = both["refs"]
            .as_array()
            .map(|r| {
                r.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        if refs[..50] != first_refs[..] {
            bail!("Show more changed the first page: {refs:?}");
        }
        let unique: std::collections::HashSet<&String> = refs.iter().collect();
        if unique.len() != 100 {
            bail!("Show more repeated verses: {refs:?}");
        }
        said.push(format!(
            "Show more: 50 more after the first 50 (no repeats), \"{}\"",
            both["status"].as_str().unwrap_or_default()
        ));

        // Open the first verse of the page Show more added.
        let reference = refs[50].clone();
        let (book_chapter, verse) = reference
            .rsplit_once(':')
            .map(|(head, verse)| (head.to_owned(), verse.parse::<u32>().unwrap_or(0)))
            .unwrap_or_default();
        let ui = &self.ui;
        let marked = ui
            .js(
                "document.querySelectorAll('[data-gui-check-target]').forEach((el) => el.removeAttribute('data-gui-check-target'));
                 const result = [...document.querySelectorAll('#search-results .search-result:not(.search-go)')][50];
                 if (!result) return false;
                 result.setAttribute('data-gui-check-target', '');
                 return true;",
                Vec::new(),
            )
            .await?;
        if marked != Value::Bool(true) {
            bail!("no 51st result");
        }
        ui.click(
            &format!("open the result {reference}"),
            "[data-gui-check-target]",
        )
        .await?;
        let place = ui
            .wait_with(
                &format!("{book_chapter} to be read, verse {verse} selected, the box closed"),
                "const heading = document.querySelector('.chapter-main .chapter-heading');
                 const selected = [...document.querySelectorAll('.chapter-main .verse-selected[data-translation]')]
                     .map((v) => Number(v.dataset.verse));
                 const open = !document.getElementById('search-panel').hidden;
                 const seen = { heading: heading ? heading.textContent.trim() : null, selected, open };
                 return [seen.heading === arguments[0] && JSON.stringify(selected) === JSON.stringify([arguments[1]]) && !open, seen];",
                vec![json!(book_chapter), json!(verse)],
            )
            .await?;
        said.push(format!(
            "opening {reference}: {} read, verse {verse} selected, the box closed",
            place["heading"].as_str().unwrap_or("?")
        ));

        said.push(self.show_more_at_smallest_height().await?);
        Ok(said.join("; "))
    }

    /// Check 18: a reference typed, Go to, Enter.
    pub(super) async fn search_go_to(&mut self) -> Result<String> {
        self.plain_reading().await?;
        self.open_search_by_key().await?;
        self.type_in_box("John 3:16").await?;
        let offered = self
            .box_is(
                "Go to John 3:16 offered, and active",
                "seen.groups[0] === 'Go to' && seen.activeKey === 'go' && seen.active === 'Go to John 3:16'
                 && seen.status === 'Press Enter to go there.'",
            )
            .await?;
        self.shot("screenshot: Go to John 3:16, Chi Rho", SEARCH_SHOTS[3])
            .await;
        self.ui.press("press Enter (go there)", Key::Enter).await?;
        let place = self
            .ui
            .wait(
                "John 3 to be read, verse 16 selected, the box closed",
                "const heading = document.querySelector('.chapter-main .chapter-heading');
                 const selected = [...document.querySelectorAll('.chapter-main .verse-selected[data-translation]')]
                     .map((v) => Number(v.dataset.verse));
                 const open = !document.getElementById('search-panel').hidden;
                 const seen = { heading: heading ? heading.textContent.trim() : null, selected, open };
                 return [seen.heading === 'John 3' && JSON.stringify(selected) === '[16]' && !open, seen];",
            )
            .await?;
        Ok(format!(
            "\"John 3:16\" typed: \"{}\" offered and active, \"{}\"; Enter: {} read, verse 16 selected, the box closed",
            offered["active"].as_str().unwrap_or_default(),
            offered["status"].as_str().unwrap_or_default(),
            place["heading"].as_str().unwrap_or("?")
        ))
    }

    /// Check 19: a command run from the palette.
    pub(super) async fn palette_command(&mut self) -> Result<String> {
        self.plain_reading().await?;
        let here = self.heading().await?;
        self.open_search_by_key().await?;
        self.type_in_box("> next chapter").await?;
        let offered = self
            .box_is(
                "Next chapter, the only command, active and runnable",
                "seen.groups.join() === 'Commands' && seen.activeKey === 'cmd:next-chapter'
                 && !document.querySelector('#search-results .search-command[data-command=\"next-chapter\"]').hasAttribute('aria-disabled')",
            )
            .await?;
        self.shot(
            "screenshot: the palette's Next chapter, Chi Rho",
            SEARCH_SHOTS[4],
        )
        .await;
        self.ui.press("press Enter (run it)", Key::Enter).await?;
        let seen = self
            .ui
            .wait_with(
                &format!("the chapter after {here} to be read, the box closed, focus on ⌕"),
                &format!(
                    "{DESCRIBE_JS}
                     const heading = document.querySelector('.chapter-main .chapter-heading');
                     const seen = {{ heading: heading ? heading.textContent.trim() : null,
                                     open: !document.getElementById('search-panel').hidden,
                                     focus: describe(document.activeElement) }};
                     return [!!seen.heading && seen.heading !== arguments[0] && !seen.open && seen.focus === '#search-toggle', seen];"
                ),
                vec![json!(here)],
            )
            .await?;
        Ok(format!(
            "\"> next chapter\": \"{}\" active; Enter: {here} → {}, the box closed, focus back on ⌕ (where it was: nothing)",
            offered["active"].as_str().unwrap_or_default(),
            seen["heading"].as_str().unwrap_or("?")
        ))
    }

    /// Check 4's Vellum screenshots of the search box: Show more at the
    /// panel's end, and the palette's Next chapter.
    pub(super) async fn search_vellum_screenshots(&mut self) -> Result<()> {
        self.plain_reading().await?;
        self.open_search_by_key().await?;
        self.type_in_box(WORD).await?;
        self.word_results().await?;
        self.show_more_at_end("scroll the panel to its end").await?;
        self.stay_at_end().await?;
        self.shot(
            "screenshot: Show more at the panel's end, Vellum",
            SEARCH_SHOTS[5],
        )
        .await;
        self.close_search().await?;
        self.open_search_by_key().await?;
        self.type_in_box("> next chapter").await?;
        self.box_is(
            "Next chapter, active",
            "seen.activeKey === 'cmd:next-chapter'",
        )
        .await?;
        self.shot(
            "screenshot: the palette's Next chapter, Vellum",
            SEARCH_SHOTS[6],
        )
        .await;
        self.close_search().await
    }
}
