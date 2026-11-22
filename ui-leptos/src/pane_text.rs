//! The reading panes' words and rules, free of the DOM (`pane` puts them on
//! the page): how two open panes are laid out ([`shape`]: side by side,
//! tabs, or reading mode's one column), what each pane's bar and the tabs
//! are called, which tab an arrow key goes to, what S does
//! ([`split_key`]), what Compare says while two panes are open, and how far
//! the keys that scroll a page scroll a pane's own column, which the
//! keyboard dispatcher does itself while the panes are split (no webview
//! can be trusted to pick the right one of two scrolling columns when focus
//! is on the page itself); and the second pane's place as this computer
//! keeps it for the next launch ([`SECOND_PANE_KEY`]).

use serde::{Deserialize, Serialize};

use crate::nav_text::MAX_VERSE;
use crate::picker_text::Book;

/// While two panes are open, at least this wide, they sit side by side,
/// each with its bar; narrower, they're tabs (S7). The stylesheet doesn't
/// ask the width itself: it follows `html[data-pane-tabs]`, which `pane`
/// sets as this query's answer changes, after the reading place logic has
/// found each pane's place in the shape the reader last saw (as
/// `html[data-wide]` follows `preferences::WIDE_QUERY`).
pub const BARS_QUERY: &str = "(min-width: 900px)";

/// How the panes are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shape {
    /// One pane, the page as it has always been.
    #[default]
    One,
    /// Two, side by side, each with its bar.
    SideBySide,
    /// Two, as tabs: only the active one shows, and the top bar's own
    /// reference, arrows and translation button act on it.
    Tabs,
    /// Two, in reading mode: only the active one shows, as reading mode's
    /// one centred column (D6), with neither bars nor tabs. Two columns and
    /// their margins wouldn't fit, and the margin follows one chapter.
    Reading,
}

impl Shape {
    /// Only the active pane shows.
    pub fn alone(self) -> bool {
        matches!(self, Self::Tabs | Self::Reading)
    }

    /// The panes' bars show (and the top bar's own reference, arrows and
    /// translation button give way to them).
    pub fn bars(self) -> bool {
        self == Self::SideBySide
    }
}

/// What S does (B9-B11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitKey {
    /// "Open this passage beside" (`reader::open_here_beside`, which leaves
    /// reading mode first, B12).
    OpenBeside,
    /// The second pane closed, as X closes it.
    CloseSecond,
    /// Nothing: the key goes on, as the palette's command is disabled.
    Nothing,
}

/// What S does: with two panes open (`split`), closes the second; with one,
/// opens the active pane's chapter beside it, if a chapter is shown
/// (`chapter_shown`), in reading mode or not.
pub fn split_key(split: bool, chapter_shown: bool) -> SplitKey {
    if split {
        SplitKey::CloseSecond
    } else if chapter_shown {
        SplitKey::OpenBeside
    } else {
        SplitKey::Nothing
    }
}

/// How the panes are laid out: `split`, two open; `two_across`, the window
/// [`BARS_QUERY`] wide; `reading`, reading mode on.
pub fn shape(split: bool, two_across: bool, reading: bool) -> Shape {
    match (split, two_across, reading) {
        (false, _, _) => Shape::One,
        (true, _, true) => Shape::Reading,
        (true, true, false) => Shape::SideBySide,
        (true, false, false) => Shape::Tabs,
    }
}

/// The tabs' list's name, for screen readers.
pub const TABS_LABEL: &str = "Passages";

/// The tab `key` moves to from tab `index` of `count`, as the WAI-ARIA
/// tabs pattern has it: ← and → to the one before and after, round the
/// ends, Home and End to the first and last. `None` for any other key.
pub fn tab_key(key: &str, index: usize, count: usize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    match key {
        "ArrowRight" => Some((index + 1) % count),
        "ArrowLeft" => Some((index + count - 1) % count),
        "Home" => Some(0),
        "End" => Some(count - 1),
        _ => None,
    }
}

/// The second pane's close button, named and titled.
pub const CLOSE_SECOND: &str = "Close the second pane";

/// What the Compare button and C say while two panes are open (S14): the
/// setting stays as it was, and comes back as the second pane closes.
pub const COMPARE_PAUSED: &str =
    "Compare shows one passage at a time; close the second pane first.";

/// A pane's bar's name, for screen readers: `first` says which pane.
pub fn bar_label(first: bool) -> &'static str {
    if first {
        "First passage"
    } else {
        "Second passage"
    }
}

/// A key that scrolls the page, as a pane's column scrolls for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollKey {
    /// Page Down, Space (1); Page Up, Shift+Space (-1).
    Page(i8),
    /// ↓ (1) and ↑ (-1).
    Line(i8),
    /// Home.
    Top,
    /// End.
    Bottom,
}

/// The scrolling key `key` is, with Shift (`shift`) or with Ctrl, Cmd or
/// Alt (`other`), aimed at a control (`on_control`, a button or a link),
/// or `None`. Space and the arrows on a control are the control's (Space
/// presses a button), so they're left to it; a key with Ctrl, Cmd or Alt
/// is something else's.
pub fn scroll_key(key: &str, shift: bool, other: bool, on_control: bool) -> Option<ScrollKey> {
    if other {
        return None;
    }
    match key {
        "PageDown" if !shift => Some(ScrollKey::Page(1)),
        "PageUp" if !shift => Some(ScrollKey::Page(-1)),
        " " if !on_control => Some(ScrollKey::Page(if shift { -1 } else { 1 })),
        "ArrowDown" if !shift && !on_control => Some(ScrollKey::Line(1)),
        "ArrowUp" if !shift && !on_control => Some(ScrollKey::Line(-1)),
        "Home" if !shift => Some(ScrollKey::Top),
        "End" if !shift => Some(ScrollKey::Bottom),
        _ => None,
    }
}

/// How far a page step goes, as a share of the band in view: Chromium's,
/// so a line from the foot of the last view stays in sight at the top.
const PAGE_SHARE: f64 = 0.875;

/// How far ↓ and ↑ go, in px: Chromium's line step.
const LINE: f64 = 40.0;

/// Where a scrolling key takes a column whose band in view is `height` px
/// tall.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollMove {
    /// Down by this much (up, if negative).
    By(f64),
    Top,
    Bottom,
}

/// Where `key` takes a column `height` px tall in view.
pub fn scroll_move(key: ScrollKey, height: f64) -> ScrollMove {
    match key {
        ScrollKey::Page(step) => ScrollMove::By(f64::from(step) * (height * PAGE_SHARE).max(LINE)),
        ScrollKey::Line(step) => ScrollMove::By(f64::from(step) * LINE),
        ScrollKey::Top => ScrollMove::Top,
        ScrollKey::Bottom => ScrollMove::Bottom,
    }
}

/// The second pane's place's key in local storage: kept on this computer
/// only, as Recent places are (`nav_text::RECENT_KEY`), and in no backup or
/// export (S9). It isn't one of the saved settings (`preferences`): the
/// pre-paint script writes each of those to `<html>`, and checks it against
/// a list of values, which a place isn't. `preferences::SPLIT` says whether
/// the panes were split; this says where the second one was.
pub const SECOND_PANE_KEY: &str = "gospel-getter-second-pane";

/// Where the second pane is: its chapter, in its translation (by code,
/// "kjv"), and, once the reader has a place in it, the verse at the top of
/// its view and how far that verse's top is below its bar, as the backend
/// keeps the first pane's (S15: the second's never goes there).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecondPlace {
    pub book_id: i64,
    pub chapter: i64,
    pub translation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verse: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<f64>,
}

/// The longest kept place read back, in UTF-16 code units (one is under
/// 120): a longer value isn't this app's, and isn't read.
pub const PLACE_MAX: u32 = 512;

impl SecondPlace {
    /// Whether this can be a place at all, by the books there are (`books`)
    /// and the bundled translations (`codes`), as a recent place is checked
    /// (`nav_text`): a book there is, one of its chapters, a translation
    /// that's bundled, a verse that could be in it, and an offset that's a
    /// number. Anything else, and the second pane isn't opened again.
    pub fn is_valid(&self, books: &[Book], codes: &[String]) -> bool {
        let chapter_ok = books
            .iter()
            .find(|b| b.id == self.book_id)
            .is_some_and(|b| (1..=b.chapter_count).contains(&self.chapter));
        let verse_ok = self
            .verse
            .is_none_or(|verse| (1..=MAX_VERSE).contains(&verse));
        let offset_ok = self.offset.is_none_or(f64::is_finite);
        chapter_ok && verse_ok && offset_ok && codes.contains(&self.translation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_keys_scroll_from_a_control_too_but_space_and_arrows_do_not() {
        for on_control in [false, true] {
            assert_eq!(
                scroll_key("PageDown", false, false, on_control),
                Some(ScrollKey::Page(1))
            );
            assert_eq!(
                scroll_key("PageUp", false, false, on_control),
                Some(ScrollKey::Page(-1))
            );
            assert_eq!(
                scroll_key("Home", false, false, on_control),
                Some(ScrollKey::Top)
            );
            assert_eq!(
                scroll_key("End", false, false, on_control),
                Some(ScrollKey::Bottom)
            );
        }
        assert_eq!(
            scroll_key(" ", false, false, false),
            Some(ScrollKey::Page(1))
        );
        assert_eq!(
            scroll_key(" ", true, false, false),
            Some(ScrollKey::Page(-1))
        );
        assert_eq!(
            scroll_key("ArrowDown", false, false, false),
            Some(ScrollKey::Line(1))
        );
        assert_eq!(
            scroll_key("ArrowUp", false, false, false),
            Some(ScrollKey::Line(-1))
        );
        for key in [" ", "ArrowDown", "ArrowUp"] {
            assert_eq!(
                scroll_key(key, false, false, true),
                None,
                "{key:?} on a control"
            );
        }
    }

    #[test]
    fn keys_with_ctrl_or_alt_and_other_keys_are_not_scrolling_keys() {
        for key in ["PageDown", " ", "ArrowDown", "Home", "End"] {
            assert_eq!(scroll_key(key, false, true, false), None, "{key:?}");
        }
        for key in ["ArrowLeft", "j", "Enter", "Tab"] {
            assert_eq!(scroll_key(key, false, false, false), None, "{key:?}");
        }
        assert_eq!(scroll_key("PageDown", true, false, false), None);
    }

    #[test]
    fn a_page_is_most_of_the_band_in_view_and_a_line_is_fixed() {
        assert_eq!(
            scroll_move(ScrollKey::Page(1), 800.0),
            ScrollMove::By(700.0)
        );
        assert_eq!(
            scroll_move(ScrollKey::Page(-1), 800.0),
            ScrollMove::By(-700.0)
        );
        // A tiny view still moves by a line.
        assert_eq!(scroll_move(ScrollKey::Page(1), 10.0), ScrollMove::By(40.0));
        assert_eq!(
            scroll_move(ScrollKey::Line(-1), 800.0),
            ScrollMove::By(-40.0)
        );
        assert_eq!(scroll_move(ScrollKey::Top, 800.0), ScrollMove::Top);
        assert_eq!(scroll_move(ScrollKey::Bottom, 800.0), ScrollMove::Bottom);
    }

    /// B9-B11: S opens the passage beside with one pane, in reading mode
    /// or not (B12 is `reader::open_here_beside`'s), and closes the second
    /// with two, whatever's shown; with no chapter shown, it does nothing.
    #[test]
    fn s_opens_this_passage_beside_or_closes_the_second_pane() {
        assert_eq!(split_key(false, true), SplitKey::OpenBeside);
        assert_eq!(split_key(true, true), SplitKey::CloseSecond);
        assert_eq!(split_key(true, false), SplitKey::CloseSecond);
        assert_eq!(split_key(false, false), SplitKey::Nothing);
    }

    #[test]
    fn two_panes_are_tabs_below_the_bars_width() {
        assert_eq!(shape(false, true, false), Shape::One);
        assert_eq!(shape(false, false, false), Shape::One);
        assert_eq!(shape(true, true, false), Shape::SideBySide);
        assert_eq!(shape(true, false, false), Shape::Tabs);
        assert!(Shape::Tabs.alone() && !Shape::Tabs.bars());
        assert!(!Shape::SideBySide.alone() && Shape::SideBySide.bars());
        assert!(!Shape::One.alone() && !Shape::One.bars());
    }

    #[test]
    fn reading_mode_shows_the_active_pane_alone_at_any_width() {
        for two_across in [true, false] {
            assert_eq!(shape(true, two_across, true), Shape::Reading);
            // One pane is reading mode as ever.
            assert_eq!(shape(false, two_across, true), Shape::One);
        }
        assert!(Shape::Reading.alone() && !Shape::Reading.bars());
    }

    #[test]
    fn the_arrows_go_round_the_tabs_and_home_and_end_to_their_ends() {
        assert_eq!(tab_key("ArrowRight", 0, 2), Some(1));
        assert_eq!(tab_key("ArrowRight", 1, 2), Some(0));
        assert_eq!(tab_key("ArrowLeft", 0, 2), Some(1));
        assert_eq!(tab_key("ArrowLeft", 1, 2), Some(0));
        assert_eq!(tab_key("Home", 1, 2), Some(0));
        assert_eq!(tab_key("End", 0, 2), Some(1));
        for key in ["ArrowUp", "ArrowDown", "Enter", " ", "j"] {
            assert_eq!(tab_key(key, 0, 2), None, "{key:?}");
        }
        assert_eq!(tab_key("ArrowRight", 0, 0), None);
    }

    fn books() -> Vec<Book> {
        vec![
            Book {
                id: 19,
                name: "Psalms".into(),
                chapter_count: 150,
            },
            Book {
                id: 45,
                name: "Romans".into(),
                chapter_count: 16,
            },
        ]
    }

    fn codes() -> Vec<String> {
        vec!["kjv".into(), "bsb".into()]
    }

    /// The place kept as `json`, read back and checked, as `pane::reopen`
    /// reads it.
    fn read(json: &str) -> Option<SecondPlace> {
        serde_json::from_str::<SecondPlace>(json)
            .ok()
            .filter(|place| place.is_valid(&books(), &codes()))
    }

    fn romans(verse: Option<i64>, offset: Option<f64>) -> SecondPlace {
        SecondPlace {
            book_id: 45,
            chapter: 5,
            translation: "bsb".into(),
            verse,
            offset,
        }
    }

    #[test]
    fn a_kept_place_is_read_back_as_it_was_kept() {
        let place = romans(Some(3), Some(-12.5));
        let json = serde_json::to_string(&place).unwrap();
        assert_eq!(
            json,
            r#"{"bookId":45,"chapter":5,"translation":"bsb","verse":3,"offset":-12.5}"#
        );
        assert_eq!(read(&json), Some(place));
        // At the chapter's top: no verse, no offset.
        let top = romans(None, None);
        let json = serde_json::to_string(&top).unwrap();
        assert_eq!(json, r#"{"bookId":45,"chapter":5,"translation":"bsb"}"#);
        assert_eq!(read(&json), Some(top));
        assert_eq!(
            read(r#"{"bookId":19,"chapter":119,"translation":"kjv","verse":176,"offset":null}"#),
            Some(SecondPlace {
                book_id: 19,
                chapter: 119,
                translation: "kjv".into(),
                verse: Some(176),
                offset: None,
            })
        );
    }

    #[test]
    fn an_unknown_book_or_translation_is_no_place() {
        for json in [
            r#"{"bookId":99,"chapter":1,"translation":"kjv"}"#,
            r#"{"bookId":0,"chapter":1,"translation":"kjv"}"#,
            r#"{"bookId":45,"chapter":5,"translation":"esv"}"#,
            // Codes are lowercase, as the translation button's record has them.
            r#"{"bookId":45,"chapter":5,"translation":"BSB"}"#,
            r#"{"bookId":45,"chapter":5,"translation":""}"#,
        ] {
            assert_eq!(read(json), None, "{json}");
        }
    }

    #[test]
    fn a_chapter_or_verse_out_of_range_is_no_place() {
        for json in [
            r#"{"bookId":45,"chapter":17,"translation":"bsb"}"#,
            r#"{"bookId":45,"chapter":0,"translation":"bsb"}"#,
            r#"{"bookId":45,"chapter":-1,"translation":"bsb"}"#,
            r#"{"bookId":45,"chapter":5,"translation":"bsb","verse":0}"#,
            r#"{"bookId":45,"chapter":5,"translation":"bsb","verse":177}"#,
        ] {
            assert_eq!(read(json), None, "{json}");
        }
        // An offset that isn't a number a page can scroll by (JSON can't
        // say one, but JavaScript's parse makes 1e400 Infinity).
        assert!(!romans(Some(3), Some(f64::INFINITY)).is_valid(&books(), &codes()));
        assert!(!romans(Some(3), Some(f64::NAN)).is_valid(&books(), &codes()));
        assert!(romans(Some(3), Some(0.0)).is_valid(&books(), &codes()));
    }

    #[test]
    fn missing_fields_or_garbage_are_no_place() {
        for json in [
            r#"{"chapter":5,"translation":"bsb"}"#,
            r#"{"bookId":45,"translation":"bsb"}"#,
            r#"{"bookId":45,"chapter":5}"#,
            r#"{"bookId":"45","chapter":5,"translation":"bsb"}"#,
            r#"{"bookId":45,"chapter":5.5,"translation":"bsb"}"#,
            r#"{"bookId":45,"chapter":5,"translation":"bsb","verse":"3"}"#,
            r#"[45,5]"#,
            "null",
            "",
            "on",
            "{",
        ] {
            assert_eq!(read(json), None, "{json:?}");
        }
    }

    #[test]
    fn the_bars_are_named_for_their_pane() {
        assert_eq!(bar_label(true), "First passage");
        assert_eq!(bar_label(false), "Second passage");
    }
}
