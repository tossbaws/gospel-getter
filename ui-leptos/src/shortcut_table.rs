//! The keyboard shortcuts: every key the app answers, in one table, free of
//! the DOM so it is tested natively (`cargo test`). The shortcuts sheet
//! (`shortcuts`) lists it, and the palette's commands (`palette`) show
//! their keys from it, by `id`. The key handlers keep their own key
//! comparisons; each key they compare against is in some row's `handles`
//! (the `KeyboardEvent` key or code values it covers;
//! `src-tauri/tests/frontend_markup.rs` checks every one is listed). The
//! page's copy of the table, `window.gospelGetterShortcuts`, is this one,
//! put there when the module loads (`bridge::export_shortcuts`).

use serde::Serialize;

/// One shortcut: its id, its group, the ways to press it (each a list of
/// key caps), the same as `aria-keyshortcuts` says it, what it does, and
/// the key values the handlers compare against for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Shortcut {
    pub id: &'static str,
    pub group: &'static str,
    pub keys: &'static [&'static [&'static str]],
    pub aria: &'static str,
    pub does: &'static str,
    pub handles: &'static [&'static str],
}

impl Shortcut {
    /// The first way to press it, as the palette shows it: its caps joined
    /// by "+" ("Ctrl+=").
    pub fn first_keys(&self) -> String {
        self.keys
            .first()
            .map(|caps| caps.join("+"))
            .unwrap_or_default()
    }
}

/// The sheet's groups, in order.
pub const GROUPS: [&str; 6] = [
    "Reading",
    "Selection",
    "Search & palette",
    "Panels",
    "Text & display",
    "History",
];

/// Every shortcut, in the sheet's order.
#[rustfmt::skip]
pub const SHORTCUTS: &[Shortcut] = &[
    Shortcut { id: "previous-chapter", group: "Reading", keys: &[&["←"]], aria: "ArrowLeft", does: "Previous chapter", handles: &["ArrowLeft"] },
    Shortcut { id: "next-chapter", group: "Reading", keys: &[&["→"]], aria: "ArrowRight", does: "Next chapter", handles: &["ArrowRight"] },
    Shortcut { id: "previous-book", group: "Reading", keys: &[&["["]], aria: "[", does: "Previous book, at chapter 1", handles: &["["] },
    Shortcut { id: "next-book", group: "Reading", keys: &[&["]"]], aria: "]", does: "Next book, at chapter 1", handles: &["]"] },
    Shortcut { id: "picker", group: "Reading", keys: &[&["G"]], aria: "G", does: "Choose a book and chapter", handles: &["g", "G"] },
    Shortcut { id: "compare", group: "Reading", keys: &[&["C"]], aria: "C", does: "Compare translations, on or off", handles: &["c", "C"] },
    Shortcut { id: "split", group: "Reading", keys: &[&["S"]], aria: "S", does: "Open this passage beside, or close the second pane", handles: &["s", "S"] },
    Shortcut { id: "switch-pane", group: "Reading", keys: &[&["F6"], &["Shift", "F6"], &["P"]], aria: "F6 Shift+F6 P", does: "Switch pane", handles: &["F6", "p", "P"] },
    Shortcut { id: "close-pane", group: "Reading", keys: &[&["X"]], aria: "X", does: "Close the second pane", handles: &["x", "X"] },
    Shortcut { id: "next-verse", group: "Selection", keys: &[&["j"]], aria: "J", does: "Select the next verse", handles: &["j", "J"] },
    Shortcut { id: "previous-verse", group: "Selection", keys: &[&["k"]], aria: "K", does: "Select the previous verse", handles: &["k", "K"] },
    Shortcut { id: "extend-down", group: "Selection", keys: &[&["Shift", "j"]], aria: "Shift+J", does: "Extend the selection down a verse", handles: &[] },
    Shortcut { id: "extend-up", group: "Selection", keys: &[&["Shift", "k"]], aria: "Shift+K", does: "Extend the selection up a verse", handles: &[] },
    Shortcut { id: "copy", group: "Selection", keys: &[&["Ctrl", "C"]], aria: "Control+C", does: "Copy the selected verses with their reference", handles: &["c"] },
    Shortcut { id: "copy-as", group: "Selection", keys: &[&["Ctrl", "Shift", "C"]], aria: "Control+Shift+C", does: "Copy as…: with verse numbers, as Markdown, or plain", handles: &["c"] },
    Shortcut { id: "note", group: "Selection", keys: &[&["N"]], aria: "N", does: "Add a note to the selected verses", handles: &["n", "N"] },
    Shortcut { id: "bookmark", group: "Selection", keys: &[&["B"]], aria: "B", does: "Bookmark the selected verses, on or off", handles: &["b", "B"] },
    Shortcut { id: "highlight", group: "Selection", keys: &[&["H"]], aria: "H", does: "Highlight the selected verses: the next color", handles: &["h", "H"] },
    Shortcut { id: "clear-selection", group: "Selection", keys: &[&["Esc"]], aria: "Escape", does: "Clear the selection", handles: &["Escape"] },
    Shortcut { id: "search", group: "Search & palette", keys: &[&["/"], &["Ctrl", "K"]], aria: "/ Control+K", does: "Search, go to a reference, or run a command", handles: &["/", "k"] },
    Shortcut { id: "commands-only", group: "Search & palette", keys: &[&[">"]], aria: ">", does: "Typed first in the box: commands only", handles: &[] },
    Shortcut { id: "palette-move", group: "Search & palette", keys: &[&["↓"], &["↑"]], aria: "ArrowDown ArrowUp", does: "Move through the results and commands", handles: &[] },
    Shortcut { id: "palette-run", group: "Search & palette", keys: &[&["Enter"]], aria: "Enter", does: "Go there, or run the command", handles: &[] },
    Shortcut { id: "translation", group: "Panels", keys: &[&["T"]], aria: "T", does: "Open or close the translations and Compare", handles: &["t", "T"] },
    Shortcut { id: "library", group: "Panels", keys: &[&["Ctrl", "B"]], aria: "Control+B", does: "Open or close the Library", handles: &["b"] },
    Shortcut { id: "shortcuts", group: "Panels", keys: &[&["?"]], aria: "?", does: "These keyboard shortcuts", handles: &["?"] },
    Shortcut { id: "close", group: "Panels", keys: &[&["Esc"]], aria: "Escape", does: "Close the search box, a panel or a popover, one at a time", handles: &["Escape"] },
    Shortcut { id: "text-larger", group: "Text & display", keys: &[&["Ctrl", "="]], aria: "Control+=", does: "Larger text", handles: &["=", "+", "NumpadAdd"] },
    Shortcut { id: "text-smaller", group: "Text & display", keys: &[&["Ctrl", "-"]], aria: "Control+-", does: "Smaller text", handles: &["-", "_", "NumpadSubtract"] },
    Shortcut { id: "text-medium", group: "Text & display", keys: &[&["Ctrl", "0"]], aria: "Control+0", does: "Medium text", handles: &["0", "Numpad0"] },
    Shortcut { id: "reader-mode", group: "Text & display", keys: &[&["R"]], aria: "R", does: "Reading mode, on or off", handles: &["r", "R"] },
    Shortcut { id: "reader-exit", group: "Text & display", keys: &[&["Esc"]], aria: "Escape", does: "Leave reading mode", handles: &["Escape"] },
    Shortcut { id: "notes-beside", group: "Text & display", keys: &[&["M"]], aria: "M", does: "Notes beside the text, on or off where you are", handles: &["m", "M"] },
    Shortcut { id: "back", group: "History", keys: &[&["Alt", "←"]], aria: "Alt+ArrowLeft", does: "Back to the passage you jumped from", handles: &["ArrowLeft"] },
    Shortcut { id: "forward", group: "History", keys: &[&["Alt", "→"]], aria: "Alt+ArrowRight", does: "Forward again", handles: &["ArrowRight"] },
];

/// What the sheet says above its groups while Aa's "Single-key shortcuts"
/// is off: that the keys pressed on their own do nothing, which still
/// work, and how to turn them back on.
pub const SINGLE_KEYS_OFF: &str = "Single-key shortcuts are off, so the keys here that are \
    pressed on their own, such as G, J, / and ?, do nothing. The ones with Ctrl or Alt, the \
    arrow keys, F6 and Esc still work. To turn them back on, open Aa and switch on Single-key \
    shortcuts.";

/// The row `id`, if there is one.
pub fn shortcut(id: &str) -> Option<&'static Shortcut> {
    SHORTCUTS.iter().find(|s| s.id == id)
}

/// The rows of `group`, in order.
pub fn in_group(group: &str) -> impl Iterator<Item = &'static Shortcut> + '_ {
    SHORTCUTS.iter().filter(move |s| s.group == group)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_has_its_own_id_a_group_and_a_way_to_press_it() {
        for (i, row) in SHORTCUTS.iter().enumerate() {
            assert!(
                SHORTCUTS[..i].iter().all(|other| other.id != row.id),
                "{} twice",
                row.id
            );
            assert!(GROUPS.contains(&row.group), "{}: {}", row.id, row.group);
            assert!(
                !row.keys.is_empty() && row.keys.iter().all(|caps| !caps.is_empty()),
                "{}",
                row.id
            );
            assert!(!row.aria.is_empty() && !row.does.is_empty(), "{}", row.id);
        }
        // Every group lists something, and the rows come group by group,
        // so the sheet's order is the table's.
        let order: Vec<&str> = GROUPS
            .iter()
            .flat_map(|g| in_group(g).map(|s| s.id))
            .collect();
        let table: Vec<&str> = SHORTCUTS.iter().map(|s| s.id).collect();
        assert_eq!(order, table);
        assert!(GROUPS.iter().all(|g| in_group(g).next().is_some()));
    }

    #[test]
    fn the_first_way_to_press_one_is_its_caps_joined() {
        assert_eq!(shortcut("text-larger").unwrap().first_keys(), "Ctrl+=");
        assert_eq!(shortcut("back").unwrap().first_keys(), "Alt+←");
        assert_eq!(shortcut("search").unwrap().first_keys(), "/");
        assert_eq!(shortcut("next-chapter").unwrap().first_keys(), "→");
        assert!(shortcut("nothing").is_none());
    }

    #[test]
    fn f6_and_p_switch_pane() {
        let row = shortcut("switch-pane").unwrap();
        assert_eq!(row.group, "Reading");
        assert_eq!(row.handles, ["F6", "p", "P"]);
        assert_eq!(row.aria, "F6 Shift+F6 P");
        assert_eq!(row.does, "Switch pane");
        assert_eq!(row.first_keys(), "F6");
    }

    #[test]
    fn b_h_and_x_have_their_rows() {
        for (id, group, after, key, handles) in [
            ("bookmark", "Selection", "note", "B", ["b", "B"]),
            ("highlight", "Selection", "bookmark", "H", ["h", "H"]),
            ("close-pane", "Reading", "switch-pane", "X", ["x", "X"]),
        ] {
            let row = shortcut(id).unwrap();
            assert_eq!(
                (row.group, row.aria, row.first_keys().as_str(), row.handles),
                (group, key, key, &handles[..]),
                "{id}"
            );
            let at = |id: &str| SHORTCUTS.iter().position(|s| s.id == id);
            assert_eq!(at(after).map(|i| i + 1), at(id), "{id} after {after}");
        }
    }

    #[test]
    fn s_and_m_have_their_rows() {
        let at = |id: &str| SHORTCUTS.iter().position(|s| s.id == id);
        let split = shortcut("split").unwrap();
        assert_eq!(
            (split.group, split.aria, split.handles, split.does),
            (
                "Reading",
                "S",
                &["s", "S"][..],
                "Open this passage beside, or close the second pane"
            )
        );
        assert_eq!(at("split").map(|i| i + 1), at("switch-pane"));
        let notes = shortcut("notes-beside").unwrap();
        assert_eq!(
            (notes.group, notes.aria, notes.handles, notes.does),
            (
                "Text & display",
                "M",
                &["m", "M"][..],
                "Notes beside the text, on or off where you are"
            )
        );
        assert_eq!(notes.first_keys(), "M");
    }

    #[test]
    fn the_t_key_is_the_translation_buttons() {
        let row = shortcut("translation").unwrap();
        assert_eq!(row.group, "Panels");
        assert_eq!(row.handles, ["t", "T"]);
        assert_eq!(row.aria, "T");
    }

    /// Every way to press a page shortcut that isn't a character on its
    /// own still works with Single-key shortcuts off
    /// (`keyboard_text::route_for`), so the sheet's sentence names it. The
    /// search box's own keys (no `handles`) are the box's either way.
    #[test]
    fn the_single_keys_off_sentence_names_every_key_that_still_works() {
        for row in SHORTCUTS.iter().filter(|row| !row.handles.is_empty()) {
            for caps in row.keys {
                let named = if caps.contains(&"Ctrl") {
                    "Ctrl"
                } else if caps.contains(&"Alt") {
                    "Alt"
                } else {
                    match caps.last().copied() {
                        Some("←" | "→" | "↑" | "↓") => "arrow keys",
                        Some(key) if key.chars().count() == 1 => continue,
                        Some(key) => key,
                        None => continue,
                    }
                };
                assert!(
                    SINGLE_KEYS_OFF.contains(named),
                    "{}: {caps:?} still works, but the sentence leaves out {named}",
                    row.id
                );
            }
        }
    }
}
