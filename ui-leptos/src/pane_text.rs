//! The reading panes' words and rules, free of the DOM (`pane` puts them on
//! the page): what each pane's bar is called, what Compare says while two
//! panes are open, and how far the keys that scroll a page scroll a pane's
//! own column, which the keyboard dispatcher does itself while the panes
//! are split (no webview can be trusted to pick the right one of two
//! scrolling columns when focus is on the page itself).

/// While two panes are open, at least this wide, each pane's bar has its
/// controls and the top bar's own reference, arrows and translation button
/// give way; the page's stylesheet hides them by the same width (move both
/// together).
pub const BARS_QUERY: &str = "(min-width: 900px)";

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

    #[test]
    fn the_bars_are_named_for_their_pane() {
        assert_eq!(bar_label(true), "First passage");
        assert_eq!(bar_label(false), "Second passage");
    }
}
