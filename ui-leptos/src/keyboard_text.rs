//! The keyboard dispatcher's rules (`keyboard`), apart from the DOM so they
//! can be tested on the host: which key does what, in the order the page's
//! dispatcher tried them; where j and k move the selection; and how far the
//! page scrolls to show a verse. Every key compared here is in some row's
//! `handles` of the table of shortcuts (`shortcut_table`), which a test of
//! the backend's checks (`every_handled_key_is_in_the_shortcuts_table`).

use crate::reader_text::Which;

/// A key pressed, as its `keydown` says it: `key` and the modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Press<'a> {
    pub key: &'a str,
    pub ctrl: bool,
    pub meta: bool,
    pub alt: bool,
    pub shift: bool,
}

/// What a key is for, once nothing else has it (a field, the search box,
/// Copy as…, a dialog).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Escape backs out one step: the search box, then (with no popover or
    /// panel open) the card's preview, then the selection.
    Escape,
    /// Alt+← (-1) and Alt+→ (1): back and forward through the history.
    History(i32),
    /// `?`: the keyboard shortcuts.
    Shortcuts,
    /// `/`, Ctrl+K (Cmd+K): search.
    Search,
    /// Ctrl+C (Cmd+C): copies the selected verses, unless text is selected.
    Copy,
    /// Ctrl+Shift+C: Copy as… for the selected verses.
    CopyAs,
    /// Ctrl+B: the Library, opened or closed.
    Library,
    /// N: a note on the selected verses; without them, the key goes on, to
    /// nothing (no rule after this one takes `n`).
    Note,
    /// G: the book and chapter picker.
    Picker,
    /// T: the translations and Compare, opened or closed.
    Translation,
    /// j and k (1 and -1): the next and previous verse; with Shift,
    /// `extend`ing the selection.
    Step { step: i64, extend: bool },
    /// `[` and `]`: the book before or after, at chapter 1.
    Book(Which),
    /// C: Compare, through its own toggle.
    Compare,
    /// ← and →: the chapter before or after.
    Turn(Which),
}

/// Whether a route's `preventDefault` comes with it, whatever happens: the
/// rest depend on what the key did (`Escape`, `Copy`, `CopyAs`, `Note`) or
/// never prevent it (`Compare`, `Turn`).
pub fn always_prevented(route: Route) -> bool {
    !matches!(
        route,
        Route::Escape | Route::Copy | Route::CopyAs | Route::Note | Route::Compare | Route::Turn(_)
    )
}

/// What `press` is for, in the order the page's dispatcher tried each rule,
/// or `None`.
pub fn route(press: &Press) -> Option<Route> {
    let key = press.key;
    // JavaScript's `toLowerCase()`, as the page compared Ctrl's letters.
    let lower = key.to_lowercase();
    let lower_key = lower.as_str();
    let ctrl = press.ctrl || press.meta;
    let plain = !ctrl && !press.alt;
    let shift = press.shift;

    if key == "Escape" {
        return Some(Route::Escape);
    }
    if press.alt && !ctrl && !shift && (key == "ArrowLeft" || key == "ArrowRight") {
        return Some(Route::History(if key == "ArrowLeft" { -1 } else { 1 }));
    }
    // ? (Shift+/) is never search.
    if plain && key == "?" {
        return Some(Route::Shortcuts);
    }
    if (plain && key == "/") || (ctrl && !press.alt && !shift && lower_key == "k") {
        return Some(Route::Search);
    }
    if ctrl && !press.alt && !shift && lower_key == "c" {
        return Some(Route::Copy);
    }
    if ctrl && !press.alt && shift && lower_key == "c" {
        return Some(Route::CopyAs);
    }
    if ctrl && !press.alt && !shift && lower_key == "b" {
        return Some(Route::Library);
    }
    if plain && !shift && (key == "n" || key == "N") {
        return Some(Route::Note);
    }
    if plain && !shift && (key == "g" || key == "G") {
        return Some(Route::Picker);
    }
    if plain && !shift && (key == "t" || key == "T") {
        return Some(Route::Translation);
    }
    if plain && (key == "j" || key == "J" || key == "k" || key == "K") {
        let step = if lower_key == "j" { 1 } else { -1 };
        return Some(Route::Step {
            step,
            extend: shift,
        });
    }
    if plain && (key == "[" || key == "]") {
        return Some(Route::Book(if key == "[" {
            Which::Prev
        } else {
            Which::Next
        }));
    }
    if plain && !shift && (key == "c" || key == "C") {
        return Some(Route::Compare);
    }
    // Any modifiers: the page's last two lines compared the key alone.
    if key == "ArrowLeft" {
        return Some(Route::Turn(Which::Prev));
    }
    if key == "ArrowRight" {
        return Some(Route::Turn(Which::Next));
    }
    None
}

/// The selection, as the card has it: its column, the verse it started
/// from, and its ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected<'a> {
    pub translation_code: &'a str,
    pub anchor: i64,
    pub start: i64,
    pub end: i64,
}

/// Where j (`step` 1) or k (-1) moves the selection, as `(anchor, focus)`,
/// or `None` (nowhere: the chapter's first or last verse, or no verses).
/// `numbers` are the column being read's verse numbers, `code` its
/// translation. With no selection there, j takes the first verse in view
/// and k the last (`in_view`, asked only then), or else the chapter's first
/// or last. With `extend` (Shift), the selection's moving end goes instead,
/// as Shift-click moves it. The verse to show is the focus.
pub fn step_selection(
    numbers: &[i64],
    selection: Option<&Selected>,
    code: &str,
    step: i64,
    extend: bool,
    in_view: impl FnOnce() -> Option<i64>,
) -> Option<(i64, i64)> {
    let mut numbers = numbers.to_vec();
    numbers.sort_unstable();
    let (first, last) = (*numbers.first()?, *numbers.last()?);
    let Some(selection) = selection.filter(|s| s.translation_code == code) else {
        let n = in_view().unwrap_or(if step > 0 { first } else { last });
        return Some((n, n));
    };
    let moving = if selection.anchor == selection.start {
        selection.end
    } else {
        selection.start
    };
    let from = if extend {
        moving
    } else if step > 0 {
        selection.end
    } else {
        selection.start
    };
    // As `numbers[numbers.indexOf(from) + step]`: an end not in the column
    // counts from before its first verse.
    let at = numbers
        .iter()
        .position(|&n| n == from)
        .map_or(-1, |i| i as i64)
        + step;
    let next = *numbers.get(usize::try_from(at).ok()?)?;
    Some(if extend {
        (selection.anchor, next)
    } else {
        (next, next)
    })
}

/// At least this much of a verse shows for it to count as in view: a line,
/// not the last few pixels of one under the bar.
pub const IN_VIEW: f64 = 24.0;

/// A verse's box on screen, as `getBoundingClientRect` gives it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub top: f64,
    pub bottom: f64,
    pub height: f64,
}

/// Whether a verse at `rect` is in view: laid out, below the top bar
/// (`view_top`, its bottom) by a line, and above the window's foot
/// (`window`, its inner height) by a line.
pub fn in_view(rect: Rect, view_top: f64, window: f64) -> bool {
    rect.height > 0.0 && rect.bottom >= view_top + IN_VIEW && rect.top <= window - IN_VIEW
}

/// How far to scroll (down, if positive) to bring a verse at `rect` into
/// view, keeping the card's gap (`card_text::CARD_GAP`) from the top bar
/// (`view_top`) and the window's foot (`window`): just enough, its top
/// first; or `None`, if it's in view.
pub fn reveal(rect: Rect, view_top: f64, window: f64) -> Option<f64> {
    let gap = crate::card_text::CARD_GAP;
    let top = view_top + gap;
    if rect.top < top {
        Some(rect.top - top)
    } else if rect.bottom > window - gap {
        Some((rect.bottom - window + gap).min(rect.top - top))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: &str) -> Press<'_> {
        Press {
            key,
            ..Press::default()
        }
    }

    fn with(key: &str, f: impl FnOnce(&mut Press)) -> Option<Route> {
        let mut p = press(key);
        f(&mut p);
        route(&p)
    }

    #[test]
    fn each_key_has_its_route() {
        assert_eq!(route(&press("Escape")), Some(Route::Escape));
        assert_eq!(route(&press("?")), Some(Route::Shortcuts));
        assert_eq!(route(&press("/")), Some(Route::Search));
        for key in ["n", "N"] {
            assert_eq!(route(&press(key)), Some(Route::Note));
        }
        for key in ["g", "G"] {
            assert_eq!(route(&press(key)), Some(Route::Picker));
        }
        for key in ["t", "T"] {
            assert_eq!(route(&press(key)), Some(Route::Translation));
        }
        for key in ["c", "C"] {
            assert_eq!(route(&press(key)), Some(Route::Compare));
        }
        let step = |step, extend| Some(Route::Step { step, extend });
        assert_eq!(route(&press("j")), step(1, false));
        assert_eq!(route(&press("k")), step(-1, false));
        assert_eq!(with("J", |p| p.shift = true), step(1, true));
        assert_eq!(with("K", |p| p.shift = true), step(-1, true));
        // A capital without Shift (Caps Lock) is still j or k, not extending.
        assert_eq!(route(&press("J")), step(1, false));
        assert_eq!(route(&press("[")), Some(Route::Book(Which::Prev)));
        assert_eq!(route(&press("]")), Some(Route::Book(Which::Next)));
        assert_eq!(route(&press("ArrowLeft")), Some(Route::Turn(Which::Prev)));
        assert_eq!(route(&press("ArrowRight")), Some(Route::Turn(Which::Next)));
        assert_eq!(route(&press("x")), None);
        assert_eq!(route(&press("Enter")), None);
    }

    #[test]
    fn ctrl_and_cmd_keys_go_by_the_letter_in_either_case() {
        for f in [|p: &mut Press| p.ctrl = true, |p: &mut Press| p.meta = true] {
            assert_eq!(with("k", f), Some(Route::Search));
            assert_eq!(with("K", f), Some(Route::Search));
            assert_eq!(with("c", f), Some(Route::Copy));
            assert_eq!(with("b", f), Some(Route::Library));
            assert_eq!(with("B", f), Some(Route::Library));
            assert_eq!(
                with("C", |p| {
                    f(p);
                    p.shift = true
                }),
                Some(Route::CopyAs)
            );
            // Ctrl+K and Ctrl+B with Shift are nothing.
            assert_eq!(
                with("K", |p| {
                    f(p);
                    p.shift = true
                }),
                None
            );
            assert_eq!(
                with("B", |p| {
                    f(p);
                    p.shift = true
                }),
                None
            );
            // Not the plain keys' meanings with Ctrl.
            for key in ["j", "n", "g", "t", "[", "?", "/"] {
                assert_eq!(with(key, f), None, "{key}");
            }
        }
        // Alt with Ctrl is none of them.
        for key in ["k", "c", "b"] {
            assert_eq!(
                with(key, |p| {
                    p.ctrl = true;
                    p.alt = true
                }),
                None
            );
        }
    }

    #[test]
    fn alt_arrows_are_history_and_other_arrows_turn() {
        assert_eq!(
            with("ArrowLeft", |p| p.alt = true),
            Some(Route::History(-1))
        );
        assert_eq!(
            with("ArrowRight", |p| p.alt = true),
            Some(Route::History(1))
        );
        // With Shift or Ctrl too, Alt's arrows aren't history: they turn,
        // as any arrow does whatever the modifiers.
        for f in [
            |p: &mut Press| {
                p.alt = true;
                p.shift = true
            },
            |p: &mut Press| {
                p.alt = true;
                p.ctrl = true
            },
            |p: &mut Press| p.shift = true,
            |p: &mut Press| p.ctrl = true,
        ] {
            assert_eq!(with("ArrowLeft", f), Some(Route::Turn(Which::Prev)));
            assert_eq!(with("ArrowRight", f), Some(Route::Turn(Which::Next)));
        }
    }

    #[test]
    fn shift_takes_some_letters_and_not_others() {
        // ? is typed with Shift, and still opens the sheet.
        assert_eq!(with("?", |p| p.shift = true), Some(Route::Shortcuts));
        for key in ["N", "G", "T", "C"] {
            assert_eq!(with(key, |p| p.shift = true), None, "{key}");
        }
        assert_eq!(with("{", |p| p.shift = true), None);
        assert_eq!(
            with("]", |p| p.shift = true),
            Some(Route::Book(Which::Next))
        );
        // Alt alone isn't plain.
        for key in ["j", "n", "c", "/", "?"] {
            assert_eq!(with(key, |p| p.alt = true), None, "{key}");
        }
    }

    #[test]
    fn n_falls_through_to_nothing() {
        // N without a selection goes on through the rules after it: none
        // takes n.
        let rules_after_note = [
            Route::Picker,
            Route::Translation,
            Route::Compare,
            Route::Turn(Which::Next),
        ];
        assert!(!rules_after_note.contains(&route(&press("n")).unwrap()));
        assert_eq!(route(&press("n")), Some(Route::Note));
    }

    #[test]
    fn which_routes_always_prevent_the_default() {
        for (route, prevented) in [
            (Route::Escape, false),
            (Route::History(1), true),
            (Route::Shortcuts, true),
            (Route::Search, true),
            (Route::Copy, false),
            (Route::CopyAs, false),
            (Route::Library, true),
            (Route::Note, false),
            (Route::Picker, true),
            (Route::Translation, true),
            (
                Route::Step {
                    step: 1,
                    extend: false,
                },
                true,
            ),
            (Route::Book(Which::Next), true),
            (Route::Compare, false),
            (Route::Turn(Which::Prev), false),
        ] {
            assert_eq!(always_prevented(route), prevented, "{route:?}");
        }
    }

    fn selected(anchor: i64, start: i64, end: i64) -> Selected<'static> {
        Selected {
            translation_code: "kjv",
            anchor,
            start,
            end,
        }
    }

    #[test]
    fn j_and_k_move_the_selection_a_verse() {
        let numbers = [1, 2, 3, 4, 5];
        let none = || panic!("not asked with a selection");
        let s = selected(2, 2, 2);
        assert_eq!(
            step_selection(&numbers, Some(&s), "kjv", 1, false, none),
            Some((3, 3))
        );
        assert_eq!(
            step_selection(&numbers, Some(&s), "kjv", -1, false, none),
            Some((1, 1))
        );
        // From a range, j goes on from its end, k from its start.
        let range = selected(2, 2, 4);
        assert_eq!(
            step_selection(&numbers, Some(&range), "kjv", 1, false, none),
            Some((5, 5))
        );
        assert_eq!(
            step_selection(&numbers, Some(&range), "kjv", -1, false, none),
            Some((1, 1))
        );
        // Never past the chapter's ends.
        let last = selected(5, 5, 5);
        assert_eq!(
            step_selection(&numbers, Some(&last), "kjv", 1, false, none),
            None
        );
        let first = selected(1, 1, 1);
        assert_eq!(
            step_selection(&numbers, Some(&first), "kjv", -1, false, none),
            None
        );
    }

    #[test]
    fn shift_moves_the_selections_moving_end() {
        let numbers = [1, 2, 3, 4, 5];
        let none = || panic!("not asked with a selection");
        // Anchored at its start: the end moves, either way.
        let down = selected(2, 2, 3);
        assert_eq!(
            step_selection(&numbers, Some(&down), "kjv", 1, true, none),
            Some((2, 4))
        );
        assert_eq!(
            step_selection(&numbers, Some(&down), "kjv", -1, true, none),
            Some((2, 2))
        );
        // Anchored at its end: the start moves.
        let up = selected(4, 3, 4);
        assert_eq!(
            step_selection(&numbers, Some(&up), "kjv", -1, true, none),
            Some((4, 2))
        );
        assert_eq!(
            step_selection(&numbers, Some(&up), "kjv", 1, true, none),
            Some((4, 4))
        );
    }

    #[test]
    fn with_no_selection_j_and_k_start_from_what_is_in_view() {
        let numbers = [3, 1, 2, 5, 4];
        assert_eq!(
            step_selection(&numbers, None, "kjv", 1, false, || Some(3)),
            Some((3, 3))
        );
        assert_eq!(
            step_selection(&numbers, None, "kjv", -1, true, || Some(4)),
            Some((4, 4))
        );
        // Nothing in view: the chapter's first or last.
        assert_eq!(
            step_selection(&numbers, None, "kjv", 1, false, || None),
            Some((1, 1))
        );
        assert_eq!(
            step_selection(&numbers, None, "kjv", -1, false, || None),
            Some((5, 5))
        );
        // A selection in another column counts as none.
        let other = Selected {
            translation_code: "web",
            ..selected(2, 2, 2)
        };
        assert_eq!(
            step_selection(&numbers, Some(&other), "kjv", 1, false, || None),
            Some((1, 1))
        );
        // No verses: nothing.
        assert_eq!(step_selection(&[], None, "kjv", 1, false, || Some(1)), None);
    }

    #[test]
    fn verses_the_translation_does_not_number_are_skipped() {
        // The BSB doesn't number some verses: j and k go over them.
        let numbers = [1, 2, 4, 5];
        let none = || panic!("not asked with a selection");
        let s = selected(2, 2, 2);
        assert_eq!(
            step_selection(&numbers, Some(&s), "kjv", 1, false, none),
            Some((4, 4))
        );
        // An end not in the column counts from before its first verse, as
        // the page's `indexOf` did.
        let missing = selected(3, 3, 3);
        assert_eq!(
            step_selection(&numbers, Some(&missing), "kjv", 1, false, none),
            Some((1, 1))
        );
        assert_eq!(
            step_selection(&numbers, Some(&missing), "kjv", -1, false, none),
            None
        );
    }

    fn rect(top: f64, bottom: f64) -> Rect {
        Rect {
            top,
            bottom,
            height: bottom - top,
        }
    }

    #[test]
    fn a_verse_is_in_view_by_a_line() {
        // The bar's foot at 50, the window 800 high.
        assert!(in_view(rect(100.0, 150.0), 50.0, 800.0));
        assert!(
            in_view(rect(0.0, 74.0), 50.0, 800.0),
            "a line below the bar"
        );
        assert!(!in_view(rect(0.0, 73.0), 50.0, 800.0));
        assert!(
            in_view(rect(776.0, 900.0), 50.0, 800.0),
            "a line above the foot"
        );
        assert!(!in_view(rect(777.0, 900.0), 50.0, 800.0));
        assert!(!in_view(Rect::default(), 50.0, 800.0), "not laid out");
    }

    #[test]
    fn revealing_a_verse_scrolls_just_enough() {
        // In view: nothing.
        assert_eq!(reveal(rect(100.0, 150.0), 50.0, 800.0), None);
        // Under the bar: up, to the gap below it.
        assert_eq!(reveal(rect(40.0, 90.0), 50.0, 800.0), Some(-18.0));
        // Below the foot: down, to the gap above it.
        assert_eq!(reveal(rect(780.0, 830.0), 50.0, 800.0), Some(38.0));
        // Taller than the room: its top goes to the gap below the bar.
        assert_eq!(reveal(rect(300.0, 1200.0), 50.0, 800.0), Some(242.0));
    }
}
