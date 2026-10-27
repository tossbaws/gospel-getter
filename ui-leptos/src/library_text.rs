//! The Library's words and rules (`library`), free of the DOM so they are
//! tested natively (`cargo test`): the highlighter colors and their names,
//! the lists' empty and error rows, a preview the translation doesn't
//! number, a note's one-line preview, which highlights the color filter
//! shows, and the tabs' and the lists' focus arithmetic. The words are the
//! page's (`ui/index.html` at 6d8d9a5), byte for byte.

/// The Library's three lists, in the tabs' order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum List {
    Bookmarks,
    Highlights,
    Notes,
}

impl List {
    pub const ALL: [List; 3] = [List::Bookmarks, List::Highlights, List::Notes];

    /// The list's name as it's asked for (`bridge::LIBRARY_REFRESH_EVENT`).
    pub fn name(self) -> &'static str {
        match self {
            List::Bookmarks => "bookmarks",
            List::Highlights => "highlights",
            List::Notes => "notes",
        }
    }

    /// The list `name` names, if it is one.
    pub fn named(name: &str) -> Option<List> {
        List::ALL.into_iter().find(|list| list.name() == name)
    }

    /// Its place among the tabs.
    pub fn index(self) -> usize {
        self as usize
    }

    /// The row that says the list couldn't be loaded, with why.
    pub fn load_failed(self, error: &str) -> String {
        match self {
            List::Bookmarks => format!("Bookmarks couldn't be loaded: {error}"),
            List::Highlights => format!("Highlights couldn't be loaded: {error}"),
            List::Notes => format!("Notes couldn\u{2019}t be loaded: {error}"),
        }
    }

    /// The row an empty list shows.
    pub fn empty(self) -> &'static str {
        match self {
            List::Bookmarks => "No bookmarks yet. Click a verse, then \u{2605} Bookmark.",
            List::Highlights => "No highlights yet. Click a verse, then pick a color.",
            List::Notes => "No notes yet. Click a verse, then + Add note (or press N).",
        }
    }
}

/// The highlighter colors, in the order the swatches and the filter show
/// them: the backend's `HighlightColor`, as it names them.
pub const COLORS: [&str; 4] = ["yellow", "green", "blue", "pink"];

/// The color filter's choice that shows every highlight.
pub const ALL: &str = "all";

/// A color's name for the reader: "yellow" is "Yellow".
pub fn color_name(color: &str) -> String {
    let mut chars = color.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// A color filter button's words: All, or the color's name.
pub fn filter_label(filter: &str) -> String {
    if filter == ALL {
        "All".to_string()
    } else {
        color_name(filter)
    }
}

/// The color filter's choices, in its order: All, then each color.
pub fn filters() -> impl Iterator<Item = &'static str> {
    std::iter::once(ALL).chain(COLORS)
}

/// The row the filter shows when no highlight has its color.
pub fn no_highlights_in(color: &str) -> String {
    format!("No {color} highlights.")
}

/// Which highlights the filter shows: their places in the list, in order.
pub fn shown<'a>(colors: impl IntoIterator<Item = &'a str>, filter: &str) -> Vec<usize> {
    colors
        .into_iter()
        .enumerate()
        .filter(|(_, color)| filter == ALL || *color == filter)
        .map(|(index, _)| index)
        .collect()
}

/// A preview the translation doesn't number at all, by its code ("kjv").
pub fn not_numbered(code: &str) -> String {
    format!("Not numbered in the {}", code.to_uppercase())
}

/// Said under a preview when the translation numbers only part of the
/// passage.
pub fn partly_numbered(code: &str) -> String {
    format!("Not all of it is numbered in the {}.", code.to_uppercase())
}

/// The row that says removing `reference` failed, with why.
pub fn remove_failed(reference: &str, error: &str) -> String {
    format!("Couldn't remove {reference}: {error}")
}

/// A note's text on one line: each run of white space one space, as the
/// page's `replace(/\s+/g, ' ')` (JavaScript's white space and line
/// terminators, which differ from Rust's `char::is_whitespace` in two
/// characters). Not trimmed: the backend already trims notes.
pub fn note_preview(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut in_space = false;
    for c in body.chars() {
        if is_js_space(c) {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(c);
            in_space = false;
        }
    }
    out
}

/// What JavaScript's `\s` matches.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// The tab a key moves to from tab `index` of `count`: ArrowRight and
/// ArrowLeft go round, Home and End go to either end; any other key
/// doesn't move.
pub fn tab_after(index: usize, count: usize, key: &str) -> Option<usize> {
    if count == 0 || index >= count {
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

/// After a row is removed from place `position`, the row focus goes to
/// among the `count` left: the one now in its place, else the new last;
/// none when the list is empty.
pub fn next_after_removing(position: usize, count: usize) -> Option<usize> {
    count.checked_sub(1).map(|last| position.min(last))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_are_named_as_the_page_asks_for_them() {
        for list in List::ALL {
            assert_eq!(List::named(list.name()), Some(list));
        }
        assert_eq!(List::named("search"), None);
        assert_eq!(
            List::ALL.map(List::index),
            [0, 1, 2],
            "the tabs' order: Bookmarks, Highlights, Notes"
        );
    }

    #[test]
    fn rows_say_what_the_page_said() {
        assert_eq!(
            List::Bookmarks.empty(),
            "No bookmarks yet. Click a verse, then ★ Bookmark."
        );
        assert_eq!(
            List::Notes.load_failed("Gone."),
            "Notes couldn’t be loaded: Gone."
        );
        assert_eq!(
            List::Bookmarks.load_failed("Gone."),
            "Bookmarks couldn't be loaded: Gone."
        );
        assert_eq!(
            List::Highlights.load_failed("Gone."),
            "Highlights couldn't be loaded: Gone."
        );
        assert_eq!(
            remove_failed("John 3:16", "Gone."),
            "Couldn't remove John 3:16: Gone."
        );
        assert_eq!(not_numbered("kjv"), "Not numbered in the KJV");
        assert_eq!(
            partly_numbered("web"),
            "Not all of it is numbered in the WEB."
        );
        assert_eq!(no_highlights_in("blue"), "No blue highlights.");
    }

    #[test]
    fn colors_are_named_with_a_capital() {
        assert_eq!(
            filters().map(filter_label).collect::<Vec<_>>(),
            ["All", "Yellow", "Green", "Blue", "Pink"]
        );
        assert_eq!(color_name(""), "");
    }

    #[test]
    fn the_filter_shows_one_color_or_all() {
        let colors = ["green", "pink", "green"];
        assert_eq!(shown(colors, ALL), [0, 1, 2]);
        assert_eq!(shown(colors, "green"), [0, 2]);
        assert_eq!(shown(colors, "blue"), Vec::<usize>::new());
    }

    #[test]
    fn a_note_preview_is_one_line_as_javascript_would_make_it() {
        assert_eq!(note_preview("One\n\n  two\tthree"), "One two three");
        assert_eq!(note_preview("a\u{A0}\u{2003}b\u{FEFF}c"), "a b c");
        // Rust's white space, not JavaScript's: kept.
        assert_eq!(note_preview("a\u{85}b"), "a\u{85}b");
        assert_eq!(note_preview(" edge "), " edge ");
    }

    #[test]
    fn arrows_go_round_the_tabs_and_home_and_end_go_to_the_ends() {
        assert_eq!(tab_after(0, 3, "ArrowRight"), Some(1));
        assert_eq!(tab_after(2, 3, "ArrowRight"), Some(0));
        assert_eq!(tab_after(0, 3, "ArrowLeft"), Some(2));
        assert_eq!(tab_after(1, 3, "Home"), Some(0));
        assert_eq!(tab_after(0, 3, "End"), Some(2));
        assert_eq!(tab_after(0, 3, "ArrowDown"), None);
        assert_eq!(tab_after(3, 3, "Home"), None);
    }

    #[test]
    fn focus_after_a_removal_stays_in_place_or_moves_up() {
        assert_eq!(next_after_removing(1, 3), Some(1));
        assert_eq!(next_after_removing(3, 3), Some(2));
        assert_eq!(next_after_removing(0, 0), None);
    }
}
