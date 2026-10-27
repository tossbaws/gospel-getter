//! Check 59: the app reopens at the verse the reader was at. In the first
//! session, after check 25 (so in the look it leaves: Large, Relaxed, Wide,
//! Flowing, Vellum), the page is scrolled so a verse of the chapter on
//! screen is 13px above the bar's bottom, and the check waits for the app
//! to say it saved the reader's place there
//! (`window.gospelGetterPlace.saved()`, the last `save_reading_position` the
//! backend answered): the first heading or verse of the chapter that
//! reaches below the bar, as the reading place logic finds it (in Flowing,
//! the verse before, if it ends on the line this one starts on), and its
//! top's offset below the bar. In the second session, after the cold start (58),
//! that verse is at the top of the reading pane again, within 1px of where
//! the reading place logic (`keepReadingPlace`) puts one: its top the saved
//! offset below the bar's bottom, and still there once the page has
//! settled.

use anyhow::{Result, ensure};
use serde_json::{Value, json};

use super::Run;

/// How far above the bar's bottom the first session leaves the verse's top.
const ABOVE_THE_BAR: f64 = 13.0;

impl Run<'_> {
    /// The first session's half: scrolls a verse of the chapter on screen
    /// to [`ABOVE_THE_BAR`] above the bar and waits for the app to save it;
    /// returns what it saved (`save_reading_position`'s arguments).
    pub(super) async fn leave_at_verse(&mut self) -> Result<Value> {
        let scrolled = self
            .ui
            .js(
                "const verses = [...document.querySelectorAll('.chapter-main .verse[data-translation]')];
                 if (verses.length < 8) return null;
                 const verse = verses[Math.min(9, Math.floor(verses.length / 2))];
                 const bar = document.getElementById('top-bar').getBoundingClientRect().bottom;
                 window.scrollBy(0, verse.getBoundingClientRect().top - (bar - arguments[0]));
                 const marks = document.querySelectorAll(
                     '.chapter-main .chapter-heading, .chapter-main .verse[data-verse], .chapter-main .compare-gap');
                 const place = [...marks].find((m) => m.getBoundingClientRect().bottom > bar);
                 return { scrolledTo: Number(verse.dataset.verse), y: window.scrollY,
                     verse: place && place.dataset.verse ? Number(place.dataset.verse) : null,
                     code: place ? place.dataset.translation : null,
                     offset: place ? place.getBoundingClientRect().top - bar : null };",
                vec![json!(ABOVE_THE_BAR)],
            )
            .await?;
        ensure!(
            scrolled.is_object() && scrolled["verse"].is_number(),
            "the chapter on screen hasn't 8 verses to scroll through, or no verse is the place: {scrolled}"
        );
        let saved = self
            .ui
            .wait_with(
                "the app to save the verse at the top of the reading pane",
                "const place = window.gospelGetterPlace && window.gospelGetterPlace.saved();
                 return [!!place && place.verse === arguments[0].verse
                     && place.translationCode === arguments[0].code
                     && typeof place.offset === 'number' && Math.abs(place.offset - arguments[0].offset) <= 0.5,
                     { place, scrolled: arguments[0] }];",
                vec![scrolled],
            )
            .await?;
        Ok(saved["place"].clone())
    }

    /// Check 59, the second session's half, after the cold start: the verse
    /// the first one left (`left`, what it saved) is back at the top.
    pub(super) async fn reopened_at_verse(&mut self, left: &Value) -> Result<String> {
        const AT_JS: &str = "const want = arguments[0];
             const bar = document.getElementById('top-bar').getBoundingClientRect().bottom;
             const verse = document.querySelector(
                 `.chapter-main .verse[data-verse=\"${want.verse}\"][data-translation=\"${want.translationCode}\"]`);
             const inner = document.getElementById('reading-pane-inner');
             const seen = { at: verse ? verse.getBoundingClientRect().top - bar : null, want: want.offset,
                 y: window.scrollY, book: inner ? Number(inner.dataset.currentBook) : null,
                 chapter: inner ? Number(inner.dataset.currentChapter) : null,
                 heading: (document.querySelector('.chapter-main .chapter-heading') || {}).textContent };
             return [verse !== null && seen.book === want.bookId && seen.chapter === want.chapter
                 && Math.abs(seen.at - want.offset) <= 1, seen];";
        let seen = self
            .ui
            .wait_within(
                "the verse read last at the top again, where keepReadingPlace puts one",
                AT_JS,
                vec![left.clone()],
                Some(self.config.session_timeout),
            )
            .await?;
        // Still there once the page has settled (its marks, the fonts).
        self.ui
            .settled("the page to settle after the restart")
            .await?;
        let settled = self
            .ui
            .wait_with(
                "the verse still at the top, the page settled",
                AT_JS,
                vec![left.clone()],
            )
            .await?;
        let off = |seen: &Value| {
            seen["at"].as_f64().unwrap_or(f64::NAN) - seen["want"].as_f64().unwrap_or(f64::NAN)
        };
        Ok(format!(
            "after the restart, {} verse {} ({}) is {:.2}px below the bar's bottom, saved at {}px ({:+.2}px; \
             scrolled to {}); settled: {:.2}px ({:+.2}px)",
            seen["heading"].as_str().unwrap_or("?"),
            left["verse"],
            left["translationCode"].as_str().unwrap_or("?"),
            seen["at"].as_f64().unwrap_or(f64::NAN),
            left["offset"],
            off(&seen),
            seen["y"],
            settled["at"].as_f64().unwrap_or(f64::NAN),
            off(&settled),
        ))
    }
}
