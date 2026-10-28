//! The Library's words and rules (`library`), free of the DOM so they are
//! tested natively (`cargo test`): the highlighter colors and their names,
//! the lists' empty and error rows, a preview the translation doesn't
//! number, a note's one-line preview, which highlights the color filter
//! shows, which notes the Notes tab's search shows and what it marks in
//! them, and the tabs' and the lists' focus arithmetic. The words that
//! were the page's are its (`ui/index.html` at 6d8d9a5), byte for byte.

use unicode_properties::{GeneralCategoryGroup, UnicodeGeneralCategory};

use crate::book_aliases::aliases_of;
use crate::palette::{fold_word, words_of};
use crate::text::thousands;

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

// ---- The Notes tab's search: the box at its top filters the list as it's
// typed, on the list already loaded (no request of its own).

/// How long after the last key the list is filtered, in milliseconds.
pub const SEARCH_DELAY_MS: i32 = 150;

/// A search's words, each once, in the order typed: its runs of letters
/// and digits, folded as the palette folds them (decomposed, without
/// marks, lowercased), so "Éphèse" and "ephese" are the same word.
pub fn query_words(query: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for word in words_of(query) {
        if !words.contains(&word) {
            words.push(word);
        }
    }
    words
}

/// A word of the query made only of the digits 0 to 9, as a number.
fn number_of(word: &str) -> Option<i64> {
    (!word.is_empty() && word.len() <= 4 && word.bytes().all(|b| b.is_ascii_digit()))
        .then(|| word.parse().ok())
        .flatten()
}

/// What the search looks for in one note, worked out once when the list
/// is loaded, so typing only compares words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Searchable {
    /// The folded words of the note's text.
    text: Vec<String>,
    /// The folded words of its book's name ("1", "john").
    book: Vec<String>,
    /// Its book's abbreviations ("jn", "jhn"), as the backend reads a
    /// reference with them (`book_aliases`).
    aliases: &'static [&'static str],
    chapter: i64,
    verses: (i64, i64),
}

impl Searchable {
    /// The note on `book_name` (book `book_id`) `chapter`:`first`–`last`
    /// that says `body`.
    pub fn new(
        book_id: i64,
        book_name: &str,
        chapter: i64,
        (first, last): (i64, i64),
        body: &str,
    ) -> Self {
        Self {
            text: words_of(body),
            book: words_of(book_name),
            aliases: aliases_of(book_id),
            chapter,
            verses: (first, last),
        }
    }

    /// Whether the note is one the search's `words` (from [`query_words`])
    /// find: each of them begins a word of its text, or is in its
    /// reference. Every note matches no words.
    pub fn matches(&self, words: &[String]) -> bool {
        words
            .iter()
            .all(|word| self.text.iter().any(|w| w.starts_with(word.as_str())) || self.names(word))
    }

    /// Whether `word` is in the note's reference: it begins a word of the
    /// book's name, or is one of its abbreviations whole; a number is the
    /// chapter, one of the verses (17 finds a note on 16–18), or a number
    /// in the book's name (the 1 of "1 John").
    fn names(&self, word: &str) -> bool {
        if let Some(n) = number_of(word) {
            return n == self.chapter
                || (self.verses.0..=self.verses.1).contains(&n)
                || self.book.iter().any(|w| w == word);
        }
        self.book.iter().any(|w| w.starts_with(word)) || self.aliases.contains(&word)
    }
}

/// Which of `notes` the search's `words` find: their places, in order.
pub fn found(notes: &[&Searchable], words: &[String]) -> Vec<usize> {
    notes
        .iter()
        .enumerate()
        .filter(|(_, note)| note.matches(words))
        .map(|(index, _)| index)
        .collect()
}

/// The line under the search box while it finds some: "3 of 12 notes".
pub fn search_count(shown: usize, total: usize) -> String {
    format!(
        "{} of {} {}",
        thousands(shown as u64),
        thousands(total as u64),
        if total == 1 { "note" } else { "notes" }
    )
}

/// The list's row when the search finds none.
pub fn no_notes_match(query: &str) -> String {
    format!("No notes match \u{201c}{}\u{201d}.", query.trim())
}

/// A character words are made of: a letter, a digit, or a mark on one (so
/// a decomposed "é" is one word, as it is once folded).
fn in_word(c: char) -> bool {
    matches!(
        c.general_category_group(),
        GeneralCategoryGroup::Letter | GeneralCategoryGroup::Number | GeneralCategoryGroup::Mark
    )
}

/// The byte ranges of `text`'s words, in order.
fn word_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        match (start, in_word(c)) {
            (None, true) => start = Some(i),
            (Some(from), false) => {
                ranges.push((from, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        ranges.push((from, text.len()));
    }
    ranges
}

/// How much of `word` (from its start, in bytes) `prefix` (folded) is,
/// if `word` folded begins with it: the shortest start of `word` that
/// folds to at least as long, with any marks after it.
fn prefix_end(word: &str, prefix: &str) -> Option<usize> {
    if prefix.is_empty() || !fold_word(word).starts_with(prefix) {
        return None;
    }
    if word.is_ascii() {
        return Some(prefix.len());
    }
    let mut end = word
        .char_indices()
        .map(|(i, c)| i + c.len_utf8())
        .find(|&i| fold_word(&word[..i]).len() >= prefix.len())
        .unwrap_or(word.len());
    end += word[end..]
        .chars()
        .take_while(|c| c.general_category_group() == GeneralCategoryGroup::Mark)
        .map(char::len_utf8)
        .sum::<usize>();
    Some(end)
}

/// What to mark in a note's `text` for the search's `words`: the start of
/// each word that one of them begins (as much of it as the longest
/// does), as byte ranges in order.
pub fn text_marks(text: &str, words: &[String]) -> Vec<(usize, usize)> {
    word_ranges(text)
        .into_iter()
        .filter_map(|(from, to)| {
            let word = &text[from..to];
            words
                .iter()
                .filter_map(|w| prefix_end(word, w))
                .max()
                .map(|end| (from, from + end))
        })
        .collect()
}

/// What to mark in a note's `reference` ("1 John 4:9–10", of book
/// `book_id`) for the search's `words`: each number one of them is, and
/// the start of each word of the book's name that one begins, or the
/// whole name where one is its abbreviation and begins none of it ("Mk").
pub fn reference_marks(reference: &str, book_id: i64, words: &[String]) -> Vec<(usize, usize)> {
    let aliases = aliases_of(book_id);
    // The book's name is everything before the chapter's number: the
    // reference's last word but its verses.
    let ranges = word_ranges(reference);
    let name_words = ranges
        .iter()
        .rposition(|&(from, to)| number_of(&reference[from..to]).is_none())
        .map_or(0, |last| last + 1);
    let by_alias = words.iter().any(|w| {
        aliases.contains(&w.as_str())
            && !ranges[..name_words]
                .iter()
                .any(|&(from, to)| prefix_end(&reference[from..to], w).is_some())
    });
    ranges
        .into_iter()
        .enumerate()
        .filter_map(|(i, (from, to))| {
            let word = &reference[from..to];
            if number_of(word).is_some() {
                return words.iter().any(|w| w == word).then_some((from, to));
            }
            if i >= name_words {
                return None;
            }
            if by_alias {
                return Some((from, to));
            }
            words
                .iter()
                .filter_map(|w| prefix_end(word, w))
                .max()
                .map(|end| (from, from + end))
        })
        .collect()
}

/// `text` cut at `marks` (from [`text_marks`] or [`reference_marks`]):
/// each piece, and whether it's marked.
pub fn marked<'a>(text: &'a str, marks: &[(usize, usize)]) -> Vec<(&'a str, bool)> {
    let mut pieces = Vec::new();
    let mut at = 0;
    for &(from, to) in marks {
        if from > at {
            pieces.push((&text[at..from], false));
        }
        pieces.push((&text[from..to], true));
        at = to;
    }
    if at < text.len() {
        pieces.push((&text[at..], false));
    }
    pieces
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

    fn note(book_id: i64, book: &str, chapter: i64, verses: (i64, i64), body: &str) -> Searchable {
        Searchable::new(book_id, book, chapter, verses, body)
    }

    fn finds(note: &Searchable, query: &str) -> bool {
        note.matches(&query_words(query))
    }

    #[test]
    fn a_search_s_words_are_folded_and_each_kept_once() {
        assert_eq!(query_words("  Grace, grace GRÂCE "), ["grace"]);
        assert_eq!(query_words("John 3:16–18"), ["john", "3", "16", "18"]);
        assert_eq!(query_words(" \t "), Vec::<String>::new());
    }

    #[test]
    fn every_word_must_begin_a_word_of_the_note_or_be_in_its_reference() {
        let n = note(
            45,
            "Romans",
            5,
            (8, 8),
            "God commendeth his love toward us.",
        );
        assert!(finds(&n, ""), "no words: every note");
        assert!(finds(&n, "love"));
        assert!(finds(&n, "LOVE toward"));
        assert!(finds(&n, "comm"), "a word begun is enough, as it's typed");
        assert!(!finds(&n, "ove"), "inside a word isn't");
        assert!(!finds(&n, "love hate"), "every word, not any");
        // Its reference: the book's name begun, the chapter, the verse.
        assert!(finds(&n, "Rom"));
        assert!(finds(&n, "romans 5 8"));
        assert!(
            finds(&n, "love rom"),
            "some in the text, some in the reference"
        );
        assert!(!finds(&n, "romans 6"));
        assert!(!finds(&n, "Corinthians"));
    }

    #[test]
    fn a_book_is_found_by_its_usual_abbreviations() {
        let mark = note(41, "Mark", 4, (3, 9), "The sower.");
        assert!(finds(&mark, "Mk 4"));
        assert!(finds(&mark, "mrk"));
        assert!(!finds(&mark, "m4"), "an abbreviation is matched whole");
        let first_cor = note(46, "1 Corinthians", 13, (4, 7), "Charity suffereth long.");
        assert!(finds(&first_cor, "1 Cor 13"));
        assert!(finds(&first_cor, "1cor"));
        assert!(finds(&first_cor, "1co 13:4"));
        assert!(!finds(&first_cor, "2 Cor"));
        let psalms = note(19, "Psalms", 23, (1, 1), "My shepherd.");
        assert!(finds(&psalms, "Ps 23"));
        assert!(finds(&psalms, "psa"));
    }

    #[test]
    fn a_number_is_the_chapter_or_one_of_the_verses_never_part_of_one() {
        let n = note(43, "John", 3, (16, 18), "Loved.");
        assert!(finds(&n, "John 3:17"), "17 is one of 16–18");
        assert!(finds(&n, "3:16"));
        assert!(!finds(&n, "John 1"));
        assert!(!finds(&n, "John 3:19"));
        // "3" isn't the start of 30, nor "1" of 16.
        assert!(!finds(&note(43, "John", 1, (30, 30), "Behold."), "3"));
        // The 1 of "1 John".
        assert!(finds(
            &note(62, "1 John", 4, (8, 8), "God is love."),
            "1 John 8"
        ));
        // In the note's text a number is a word like any other.
        assert!(finds(
            &note(1, "Genesis", 1, (1, 1), "Read in 2026."),
            "202"
        ));
    }

    #[test]
    fn case_and_accents_do_not_matter() {
        let n = note(44, "Acts", 19, (1, 1), "Paul came to Éphèse; CAFÉ talk.");
        assert!(finds(&n, "ephese"));
        assert!(finds(&n, "ÉPHÈSE"));
        assert!(finds(&n, "cafe"));
        // Decomposed in the note, precomposed in the search, or the other
        // way round.
        assert!(finds(
            &note(44, "Acts", 19, (1, 1), "Caf\u{0065}\u{0301}"),
            "café"
        ));
        assert!(finds(&n, "Cafe\u{0301}"));
    }

    #[test]
    fn found_keeps_the_list_s_order() {
        let notes = [
            note(1, "Genesis", 1, (1, 1), "In the beginning."),
            note(43, "John", 1, (1, 1), "In the beginning was the Word."),
            note(43, "John", 3, (16, 16), "Loved."),
        ];
        let all: Vec<&Searchable> = notes.iter().collect();
        assert_eq!(found(&all, &query_words("beginning")), [0, 1]);
        assert_eq!(found(&all, &query_words("john")), [1, 2]);
        assert_eq!(found(&all, &query_words("")), [0, 1, 2]);
        assert_eq!(found(&all, &query_words("nowhere")), Vec::<usize>::new());
    }

    #[test]
    fn the_search_says_how_many_of_how_many_or_that_none_match() {
        assert_eq!(search_count(3, 12), "3 of 12 notes");
        assert_eq!(search_count(1, 1), "1 of 1 note");
        assert_eq!(search_count(0, 1_204), "0 of 1,204 notes");
        assert_eq!(
            no_notes_match("  grace "),
            "No notes match \u{201c}grace\u{201d}."
        );
    }

    fn marks_of<'a>(text: &'a str, marks: &[(usize, usize)]) -> Vec<&'a str> {
        marks.iter().map(|&(from, to)| &text[from..to]).collect()
    }

    #[test]
    fn the_start_of_each_word_found_is_marked() {
        let text = "Love is patient; love is kind. Lovely.";
        let words = query_words("love");
        assert_eq!(
            marks_of(text, &text_marks(text, &words)),
            ["Love", "love", "Love"]
        );
        // The longest start a word of the search begins.
        let words = query_words("pa patient");
        assert_eq!(marks_of(text, &text_marks(text, &words)), ["patient"]);
        assert_eq!(text_marks(text, &query_words("")), []);
        assert_eq!(
            marked("a love b", &text_marks("a love b", &query_words("lo"))),
            [("a ", false), ("lo", true), ("ve b", false)]
        );
    }

    #[test]
    fn marks_fall_on_the_note_s_own_characters_accented_or_not() {
        let text = "Paul à Éphèse, e\u{0301}crit.";
        let marks = text_marks(text, &query_words("eph ecrit"));
        assert_eq!(marks_of(text, &marks), ["Éph", "e\u{0301}crit"]);
        // A mark never ends between a letter and its accent.
        let text = "cafe\u{0301}s";
        assert_eq!(
            marks_of(text, &text_marks(text, &query_words("cafe"))),
            ["cafe\u{0301}"]
        );
    }

    #[test]
    fn the_reference_marks_its_book_and_its_numbers() {
        let reference = "1 Corinthians 13:4\u{2013}7";
        let marks = |q: &str| marks_of(reference, &reference_marks(reference, 46, &query_words(q)));
        assert_eq!(marks("cor 13"), ["Cor", "13"]);
        assert_eq!(marks("1 cor 4"), ["1", "Cor", "4"]);
        // By its abbreviation: the whole name, unless it begins it.
        assert_eq!(marks("1co"), ["Corinthians"]);
        assert_eq!(marks("cor"), ["Cor"]);
        let romans = "Romans 5:8";
        assert_eq!(
            marks_of(romans, &reference_marks(romans, 45, &query_words("rom rm"))),
            ["Romans"],
            "rom begins it, rm doesn't"
        );
        assert_eq!(
            marks_of(romans, &reference_marks(romans, 45, &query_words("rom"))),
            ["Rom"]
        );
        // Within the verses but not one of the numbers written: no mark.
        assert_eq!(marks("5"), Vec::<&str>::new());
        assert_eq!(marks("charity"), Vec::<&str>::new());
        let reference = "Song of Solomon 2:1";
        assert_eq!(
            marks_of(
                reference,
                &reference_marks(reference, 22, &query_words("sol 2"))
            ),
            ["Sol", "2"]
        );
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
