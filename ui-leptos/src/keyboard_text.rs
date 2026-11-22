//! The keyboard dispatcher's rules (`keyboard`), apart from the DOM so they
//! can be tested on the host: which key does what, in the order the page's
//! dispatcher tried them; where j and k move the selection; and how far the
//! page scrolls to show a verse. Every key compared here is in some row's
//! `handles` of the table of shortcuts (`shortcut_table`), which a test of
//! the backend's checks (`every_handled_key_is_in_the_shortcuts_table`).
//!
//! With Aa's "Single-key shortcuts" off ([`route_for`]), a character key
//! pressed on its own ([`character_only`]) is no shortcut (WCAG 2.1.4):
//! only the keys with Ctrl, Cmd or Alt, and those that aren't characters
//! (the arrows, Escape), are.

use crate::reader_text::Which;

/// A key pressed, as its `keydown` says it: `key` and the modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Press<'a> {
    pub key: &'a str,
    pub ctrl: bool,
    pub meta: bool,
    pub alt: bool,
    pub shift: bool,
    /// AltGr is held (`getModifierState("AltGraph")`): the key is the
    /// character AltGr types, as `[` and `]` are on German, French,
    /// Spanish, Italian and Nordic layouts. Windows reports AltGr as Ctrl
    /// and Alt as well, so they don't count against a character then.
    pub alt_graph: bool,
}

impl Press<'_> {
    /// Whether the key is a character typed without Ctrl, Cmd or Alt: as
    /// typed, or with AltGr (whose Ctrl and Alt are AltGr's own).
    fn plain(&self) -> bool {
        if self.meta {
            return false;
        }
        self.alt_graph || (!self.ctrl && !self.alt)
    }
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
    /// B: the selected verses' bookmark, on or off, as ★ Bookmark does;
    /// without them, the key goes on, as N's does (H4).
    Bookmark,
    /// H: the selected verses' highlight, a step along the colors
    /// (`card_text::next_highlight`); without them, the key goes on (H4).
    Highlight,
    /// X: the second pane closed, as its × does (H5); with one pane, the
    /// key goes on.
    ClosePane,
    /// S: this passage opened beside, as the palette's "Open this passage
    /// beside" does, or, with two panes open, the second closed, as X does
    /// (B9-B12); with no chapter shown, the key goes on.
    Split,
    /// M: "Notes beside the text", shown or hidden where the reader is
    /// (`aa_text::notes_toggled`, M2).
    NotesBeside,
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
    /// R: reading mode, on or off.
    ReadingMode,
    /// ← and →: the chapter before or after.
    Turn(Which),
    /// F6 and Shift+F6: the other pane, while two are open (D10). Not a
    /// character, so Single-key shortcuts off leaves it be (WCAG 2.1.4).
    /// P is the same (H1), and is a character, so the switch does turn it
    /// off. It changes which pane is active, never swaps their passages:
    /// the first pane's is the saved reading place (S15).
    SwitchPane,
}

/// Whether a route's `preventDefault` comes with it, whatever happens: the
/// rest depend on what the key did (`Escape`, `Copy`, `CopyAs`, `Note`,
/// `Bookmark`, `Highlight`: only with a selection; `ClosePane`,
/// `SwitchPane`: with one pane, F6 is the webview's; `Split`: with a
/// chapter shown; `NotesBeside`: once it's changed the setting) or never
/// prevent it (`Compare`, `Turn`).
pub fn always_prevented(route: Route) -> bool {
    !matches!(
        route,
        Route::Escape
            | Route::Copy
            | Route::CopyAs
            | Route::Note
            | Route::Bookmark
            | Route::Highlight
            | Route::ClosePane
            | Route::Split
            | Route::NotesBeside
            | Route::Compare
            | Route::Turn(_)
            | Route::SwitchPane
    )
}

/// What `press` is for, in the order the page's dispatcher tried each rule,
/// or `None`.
pub fn route(press: &Press) -> Option<Route> {
    let key = press.key;
    // JavaScript's `toLowerCase()`, for Ctrl's letters.
    let lower = key.to_lowercase();
    let lower_key = lower.as_str();
    let ctrl = press.ctrl || press.meta;
    let plain = press.plain();
    let shift = press.shift;

    if key == "Escape" {
        return Some(Route::Escape);
    }
    if key == "F6" && !ctrl && !press.alt {
        return Some(Route::SwitchPane);
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
    // After Ctrl+B, which stays the Library: B is plain only.
    if plain && !shift && (key == "b" || key == "B") {
        return Some(Route::Bookmark);
    }
    if plain && !shift && (key == "h" || key == "H") {
        return Some(Route::Highlight);
    }
    if plain && !shift && (key == "x" || key == "X") {
        return Some(Route::ClosePane);
    }
    // Plain only, as B and C are: Ctrl+S (Cmd+S) stays the webview's.
    if plain && !shift && (key == "s" || key == "S") {
        return Some(Route::Split);
    }
    if plain && !shift && (key == "m" || key == "M") {
        return Some(Route::NotesBeside);
    }
    // Plain only: Ctrl+P (Cmd+P) stays the webview's print.
    if plain && !shift && (key == "p" || key == "P") {
        return Some(Route::SwitchPane);
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
    if plain && !shift && (key == "r" || key == "R") {
        return Some(Route::ReadingMode);
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

/// Whether `press` is a character key pressed on its own: one character (a
/// letter, a digit or a symbol, with Shift or AltGr or not, as `?`, `J`
/// and a German `[` are), with no Ctrl, Cmd or Alt. These are the
/// shortcuts Aa's "Single-key shortcuts" turns off; the arrows and Escape
/// aren't characters.
pub fn character_only(press: &Press) -> bool {
    press.plain() && press.key.chars().count() == 1
}

/// What `press` is for ([`route`]) with single-key shortcuts on
/// (`single_keys`), or off: then a character key pressed on its own is no
/// shortcut, and goes on as if unheard (typed, where typing is possible).
pub fn route_for(press: &Press, single_keys: bool) -> Option<Route> {
    if !single_keys && character_only(press) {
        return None;
    }
    route(press)
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

    /// C-02: AltGr's characters are the characters, though Windows also
    /// says Ctrl and Alt for AltGr.
    #[test]
    fn altgr_characters_are_plain_keys() {
        let altgr = |key| Press {
            key,
            ctrl: true,
            alt: true,
            alt_graph: true,
            ..Press::default()
        };
        assert_eq!(route(&altgr("[")), Some(Route::Book(Which::Prev)));
        assert_eq!(route(&altgr("]")), Some(Route::Book(Which::Next)));
        assert_eq!(route(&altgr("/")), Some(Route::Search));
        assert_eq!(route(&altgr("?")), Some(Route::Shortcuts));
        // As Linux reports it: AltGr alone.
        let linux = Press {
            key: "]",
            alt_graph: true,
            ..Press::default()
        };
        assert_eq!(route(&linux), Some(Route::Book(Which::Next)));
        // They're single keys, which Aa can turn off.
        assert!(character_only(&altgr("[")));
        assert_eq!(route_for(&altgr("["), false), None);
        assert_eq!(route_for(&altgr("["), true), Some(Route::Book(Which::Prev)));
        // Ctrl and Alt without AltGr are still no character.
        let ctrl_alt = Press {
            alt_graph: false,
            ..altgr("[")
        };
        assert_eq!(route(&ctrl_alt), None);
        assert!(!character_only(&ctrl_alt));
        // Cmd is never AltGr's.
        let cmd = Press {
            meta: true,
            ..altgr("[")
        };
        assert_eq!(route(&cmd), None);
        // AltGr's Ctrl and Alt aren't Ctrl's shortcuts or Alt's history:
        // AltGr+B is B, not the Library.
        assert_eq!(route(&altgr("b")), Some(Route::Bookmark));
        assert_eq!(route(&altgr("c")), Some(Route::Compare));
        assert_eq!(route(&altgr("s")), Some(Route::Split));
        assert_eq!(route(&altgr("m")), Some(Route::NotesBeside));
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
        for key in ["r", "R"] {
            assert_eq!(route(&press(key)), Some(Route::ReadingMode));
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
        assert_eq!(route(&press("z")), None);
        assert_eq!(route(&press("Enter")), None);
    }

    /// R alone, either case (Caps Lock gives `R`), is reading mode; with
    /// Ctrl or Cmd it's left to the WebView (Ctrl+R reloads in WebView2),
    /// and with Alt or Shift to whatever else wants it. Off, Single-key
    /// shortcuts turn it off like the other letters.
    #[test]
    fn r_alone_is_reading_mode() {
        for key in ["r", "R"] {
            assert_eq!(route(&press(key)), Some(Route::ReadingMode), "{key}");
            assert!(always_prevented(Route::ReadingMode));
            assert_eq!(with(key, |p| p.ctrl = true), None, "Ctrl+{key}");
            assert_eq!(with(key, |p| p.meta = true), None, "Cmd+{key}");
            assert_eq!(with(key, |p| p.alt = true), None, "Alt+{key}");
            assert_eq!(with(key, |p| p.shift = true), None, "Shift+{key}");
            assert!(character_only(&press(key)));
            assert_eq!(
                route_for(&press(key), false),
                None,
                "{key} with single keys off"
            );
            assert_eq!(route_for(&press(key), true), Some(Route::ReadingMode));
        }
    }

    /// F6, and Shift+F6, switch panes (D10), with Single-key shortcuts on
    /// or off: F6 is no character. With Ctrl or Alt it's not this key.
    #[test]
    fn f6_and_shift_f6_switch_panes_either_way() {
        for shift in [false, true] {
            let f6 = Press {
                key: "F6",
                shift,
                ..Press::default()
            };
            assert!(!character_only(&f6));
            assert_eq!(route_for(&f6, true), Some(Route::SwitchPane));
            assert_eq!(route_for(&f6, false), Some(Route::SwitchPane));
        }
        assert_eq!(with("F6", |p| p.ctrl = true), None);
        assert_eq!(with("F6", |p| p.meta = true), None);
        assert_eq!(with("F6", |p| p.alt = true), None);
        assert_eq!(route(&press("F7")), None);
    }

    /// B, H, X and P alone, either case (Caps Lock gives the capital), are
    /// the bookmark, the highlight, closing the second pane and switching
    /// panes (H1-H6). With Ctrl, Cmd or Alt they're no part of it: Ctrl+B
    /// stays the Library and Ctrl+P the webview's print. Off, Single-key
    /// shortcuts turn all four off.
    #[test]
    fn b_h_x_and_p_alone_bookmark_highlight_close_and_switch() {
        for (keys, to) in [
            (["b", "B"], Route::Bookmark),
            (["h", "H"], Route::Highlight),
            (["x", "X"], Route::ClosePane),
            (["p", "P"], Route::SwitchPane),
        ] {
            for key in keys {
                assert_eq!(route(&press(key)), Some(to), "{key}");
                assert!(character_only(&press(key)), "{key}");
                assert_eq!(route_for(&press(key), true), Some(to), "{key} on");
                assert_eq!(route_for(&press(key), false), None, "{key} off");
                assert!(!always_prevented(to), "{key} prevents only when it acts");
            }
        }
        let new = [
            Route::Bookmark,
            Route::Highlight,
            Route::ClosePane,
            Route::SwitchPane,
        ];
        let ctrl = |p: &mut Press| p.ctrl = true;
        let cmd = |p: &mut Press| p.meta = true;
        let alt = |p: &mut Press| p.alt = true;
        for key in ["b", "B"] {
            assert_eq!(with(key, ctrl), Some(Route::Library), "Ctrl+{key}");
            assert_eq!(with(key, cmd), Some(Route::Library), "Cmd+{key}");
        }
        for (key, f) in [
            ("p", ctrl as fn(&mut Press)),
            ("P", ctrl),
            ("p", cmd),
            ("p", alt),
            ("x", ctrl),
            ("X", ctrl),
            ("h", ctrl),
            ("H", ctrl),
            ("b", alt),
            ("B", alt),
            ("h", alt),
            ("x", alt),
        ] {
            let routed = with(key, f);
            assert!(
                routed.is_none_or(|r| !new.contains(&r)),
                "{key} with a modifier: {routed:?}"
            );
        }
        // Ctrl+P, Cmd+P, Ctrl+X and Ctrl+H are nothing of the app's.
        for (key, f) in [
            ("p", ctrl as fn(&mut Press)),
            ("p", cmd),
            ("x", ctrl),
            ("h", ctrl),
        ] {
            assert_eq!(with(key, f), None, "{key} with Ctrl or Cmd");
        }
    }

    /// The key values one way of pressing a shortcut (one of the table's
    /// combinations of caps) sends, each with its modifiers (Ctrl, Alt,
    /// Shift): a letter in either case (with Shift, its capital), an arrow
    /// or Esc by its key's name.
    fn presses_of(caps: &[&str]) -> Vec<(String, bool, bool, bool)> {
        let has = |cap: &str| caps.contains(&cap);
        let (ctrl, alt, shift) = (has("Ctrl"), has("Alt"), has("Shift"));
        let Some(cap) = caps
            .iter()
            .find(|c| !matches!(**c, "Ctrl" | "Alt" | "Shift"))
        else {
            return Vec::new();
        };
        let named = match *cap {
            "←" => Some("ArrowLeft"),
            "→" => Some("ArrowRight"),
            "↑" => Some("ArrowUp"),
            "↓" => Some("ArrowDown"),
            "Esc" => Some("Escape"),
            _ => None,
        };
        let keys: Vec<String> = match named {
            Some(name) => vec![name.to_owned()],
            None if shift => vec![cap.to_uppercase()],
            None => vec![cap.to_lowercase(), cap.to_uppercase()],
        };
        let mut keys = keys;
        keys.dedup();
        keys.into_iter().map(|k| (k, ctrl, alt, shift)).collect()
    }

    /// Off, Single-key shortcuts leave every character key pressed on its
    /// own unanswered, and every other way to press a shortcut still does
    /// what it did: each way in the table of shortcuts, so a shortcut added
    /// to it later is covered too.
    #[test]
    fn single_key_shortcuts_off_answer_no_character_key_alone_and_keep_the_rest() {
        let mut turned_off = Vec::new();
        let mut kept = Vec::new();
        for row in crate::shortcut_table::SHORTCUTS {
            for caps in row.keys {
                for (key, ctrl, alt, shift) in presses_of(caps) {
                    for meta in [false, true].into_iter().filter(|m| !m || ctrl) {
                        let press = Press {
                            key: &key,
                            ctrl: ctrl && !meta,
                            meta,
                            alt,
                            shift,
                            alt_graph: false,
                        };
                        let on = route_for(&press, true);
                        assert_eq!(on, route(&press), "{}: on is as before", row.id);
                        let off = route_for(&press, false);
                        let alone = !ctrl && !alt && key.chars().count() == 1;
                        assert_eq!(character_only(&press), alone, "{}: {key}", row.id);
                        if alone {
                            assert_eq!(off, None, "{}: {key} alone is off", row.id);
                            if on.is_some() {
                                turned_off.push(row.id);
                            }
                        } else {
                            assert_eq!(off, on, "{}: {caps:?} works either way", row.id);
                            if on.is_some() {
                                kept.push(row.id);
                            }
                        }
                    }
                }
            }
        }
        // The switch does something both ways: some keys alone are turned
        // off, and Ctrl+K (the palette), Ctrl+B, Alt+← and the arrows stay.
        assert!(turned_off.contains(&"picker") && turned_off.contains(&"shortcuts"));
        for id in ["search", "library", "back", "next-chapter", "close"] {
            assert!(kept.contains(&id), "{id} stays");
        }
        for id in [
            "bookmark",
            "highlight",
            "close-pane",
            "switch-pane",
            "split",
            "notes-beside",
        ] {
            assert!(turned_off.contains(&id), "{id}'s letter is turned off");
        }
        // A key that's no shortcut is no shortcut either way.
        assert_eq!(route_for(&press("z"), false), None);
        // Typed with Ctrl, a letter is never a single-key shortcut.
        let ctrl_k = Press {
            key: "k",
            ctrl: true,
            ..Press::default()
        };
        assert!(!character_only(&ctrl_k));
        assert_eq!(route_for(&ctrl_k, false), Some(Route::Search));
    }

    /// S and M alone, either case (Caps Lock gives the capital), are split
    /// view and "Notes beside the text" (B9, B7). With Ctrl, Cmd or Alt
    /// (or Shift) they're no part of it: Ctrl+S stays the webview's. Off,
    /// Single-key shortcuts turn both off.
    #[test]
    fn s_and_m_alone_split_and_show_the_notes() {
        for (keys, to) in [(["s", "S"], Route::Split), (["m", "M"], Route::NotesBeside)] {
            for key in keys {
                assert_eq!(route(&press(key)), Some(to), "{key}");
                assert!(character_only(&press(key)), "{key}");
                assert_eq!(route_for(&press(key), true), Some(to), "{key} on");
                assert_eq!(route_for(&press(key), false), None, "{key} off");
                assert!(!always_prevented(to), "{key} prevents only when it acts");
                for (what, f) in [
                    ("Ctrl", (|p: &mut Press| p.ctrl = true) as fn(&mut Press)),
                    ("Cmd", |p| p.meta = true),
                    ("Alt", |p| p.alt = true),
                    ("Shift", |p| p.shift = true),
                    ("Ctrl+Alt", |p| {
                        p.ctrl = true;
                        p.alt = true;
                    }),
                ] {
                    assert_eq!(with(key, f), None, "{what}+{key}");
                    let mut pressed = press(key);
                    f(&mut pressed);
                    assert_eq!(route_for(&pressed, false), None, "{what}+{key}, keys off");
                }
            }
        }
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
        for key in ["N", "G", "T", "C", "B", "H", "X", "P", "S", "M"] {
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
            (Route::Bookmark, false),
            (Route::Highlight, false),
            (Route::ClosePane, false),
            (Route::Split, false),
            (Route::NotesBeside, false),
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
            (Route::SwitchPane, false),
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
        // An end not in the column counts from before its first verse (an
        // `indexOf` of -1).
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
