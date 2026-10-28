//! Interpreting what the reader typed into the search box: either a
//! reference to go to (`John 3:16`, `1 Cor 13`, `ps 23:1-3`) or words to
//! search the text for. Pure — no database access — so it's tested here
//! against the real book list without seeding anything.

use crate::book_aliases::ALIASES;
use crate::db::Book;

/// The most words one text search will match on; anything past this is
/// ignored rather than building an ever-larger FTS query.
pub const MAX_SEARCH_TERMS: usize = 12;

/// A passage to go to: a whole chapter, or an inclusive verse range in it.
/// Chapter numbers are checked against the book; verse numbers can only be
/// checked once the translation is known (see `commands::search`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PassageRef {
    pub book_id: i64,
    pub chapter: i64,
    /// `(first, last)`, inclusive; equal for a single verse.
    pub verses: Option<(i64, i64)>,
}

/// What the search box input means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Interpretation {
    /// Nothing searchable was typed.
    Empty,
    /// A reference to go to.
    Passage(PassageRef),
    /// Recognizably a reference, but not one that exists or can be read;
    /// the message says why, for the reader.
    InvalidReference(String),
    /// Words to search the text for, lowercased and de-duplicated.
    Text(Vec<String>),
    /// A book's name or abbreviation alone (`Psalms`, `1 cor`): words to
    /// search the text for, as `Text`, and that book's first chapter to go
    /// to as well, offered first.
    TextOrBook(Vec<String>, PassageRef),
}

/// Decide whether `input` is a reference or a text search. A reference is
/// a book name or abbreviation followed by a chapter, optionally with a
/// verse or verse range (`:`, or `.`, between chapter and verse; `-`, `–`
/// or `—` for a range). Words that aren't a book are a text search. A book
/// name or abbreviation with no number is both: a text search, so `mark` or
/// `numbers` can still be searched for, and the book's first chapter to go
/// to. Only the whole input counts as a book, matched exactly as a
/// reference's book is; there's no guessing from a prefix.
pub fn interpret(input: &str, books: &[Book]) -> Interpretation {
    let input = input.trim();
    if input.is_empty() {
        return Interpretation::Empty;
    }

    let (book_part, spec) = split_reference(input);
    if !spec.is_empty() {
        let looks_like_verse_ref = spec.contains([':', '.']);
        if book_part.chars().any(char::is_alphabetic) {
            match find_book(book_part, books) {
                Some(book) => return passage(book, spec),
                None if looks_like_verse_ref => {
                    return Interpretation::InvalidReference(format!(
                        "There's no book called \u{201c}{}\u{201d}.",
                        book_part.trim().trim_end_matches('.')
                    ));
                }
                None => {}
            }
        } else if looks_like_verse_ref {
            return Interpretation::InvalidReference(
                "Which book? Try something like John 3:16.".to_string(),
            );
        }
    }

    let terms = search_terms(input);
    if terms.is_empty() {
        return Interpretation::Empty;
    }
    match find_book(input, books) {
        Some(book) => Interpretation::TextOrBook(
            terms,
            PassageRef {
                book_id: book.id,
                chapter: 1,
                verses: None,
            },
        ),
        None => Interpretation::Text(terms),
    }
}

/// The words of a text search: runs of letters and digits, lowercased, in
/// the order typed, without repeats, at most `MAX_SEARCH_TERMS`.
pub fn search_terms(input: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    for word in input.split(|c: char| !c.is_alphanumeric()) {
        if word.is_empty() {
            continue;
        }
        let word = word.to_lowercase();
        if !terms.contains(&word) {
            terms.push(word);
        }
        if terms.len() == MAX_SEARCH_TERMS {
            break;
        }
    }
    terms
}

/// Where `terms` (a text search's, as [`search_terms`] gives them) are in a
/// verse's `text`, to be marked in its result: the byte ranges of the
/// words of `text` that are one of them, in order. A word is a run of
/// letters, digits and private-use characters, as SQLite's FTS5 tokenizer
/// splits the text it indexes (`db::search`), and two words are the same
/// if they are once decomposed (NFD), without their marks, and lowercased.
/// Every word is looked at, so a term marks each place it is in the verse.
/// This is the one rule for it: the page draws the marks where it says.
pub fn term_marks(text: &str, terms: &[String]) -> Vec<(usize, usize)> {
    let wanted: Vec<String> = terms.iter().map(|t| folded(t)).collect();
    let mut marks = Vec::new();
    let mut start = None;
    for (i, c) in text
        .char_indices()
        .chain(std::iter::once((text.len(), ' ')))
    {
        let in_word = i < text.len() && word_char(c);
        match (start, in_word) {
            (None, true) => start = Some(i),
            (Some(from), false) => {
                if wanted.contains(&folded(&text[from..i])) {
                    marks.push((from, i));
                }
                start = None;
            }
            _ => {}
        }
    }
    marks
}

/// A character words are made of: a letter, a digit or a private-use
/// character (`\p{L}`, `\p{N}`, `\p{Co}`).
fn word_char(c: char) -> bool {
    use unicode_properties::{GeneralCategory, GeneralCategoryGroup, UnicodeGeneralCategory};
    matches!(
        c.general_category_group(),
        GeneralCategoryGroup::Letter | GeneralCategoryGroup::Number
    ) || c.general_category() == GeneralCategory::PrivateUse
}

/// `word` decomposed (NFD), without its marks, lowercased.
fn folded(word: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    use unicode_properties::{GeneralCategoryGroup, UnicodeGeneralCategory};
    word.nfd()
        .filter(|c| c.general_category_group() != GeneralCategoryGroup::Mark)
        .collect::<String>()
        .to_lowercase()
}

/// Split off a trailing chapter/verse specification: the longest run at the
/// end of `input` made only of digits, separators and spaces, starting at a
/// digit. Returns `(book part, spec)`; the spec is empty when there's none.
fn split_reference(input: &str) -> (&str, &str) {
    let is_spec_char = |c: char| c.is_ascii_digit() || " \t:.-\u{2013}\u{2014}".contains(c);
    let start = input
        .char_indices()
        .rev()
        .take_while(|&(_, c)| is_spec_char(c))
        .last()
        .map_or(input.len(), |(i, _)| i);
    // The spec starts at its first digit; anything before that (a space,
    // or the period of an abbreviation like "Gen.") belongs to the book.
    match input[start..].find(|c: char| c.is_ascii_digit()) {
        Some(offset) => input.split_at(start + offset),
        None => (input, ""),
    }
}

/// A book name in the normalized form the alias table uses: lowercase,
/// periods and spaces removed, and a leading `I`/`II`/`III`, `1st`,
/// `First` (etc.) turned into a digit.
fn book_key(name: &str) -> String {
    let lower = name.to_lowercase().replace('.', " ");
    let mut words: Vec<&str> = lower.split_whitespace().collect();
    if words.len() > 1 {
        let number = match words[0] {
            "i" | "1st" | "first" => Some("1"),
            "ii" | "2nd" | "second" => Some("2"),
            "iii" | "3rd" | "third" => Some("3"),
            _ => None,
        };
        if let Some(number) = number {
            words[0] = number;
        }
    }
    words.concat()
}

fn find_book<'a>(name: &str, books: &'a [Book]) -> Option<&'a Book> {
    let key = book_key(name);
    if key.is_empty() {
        return None;
    }
    let by_alias = ALIASES
        .iter()
        .find(|(_, aliases)| aliases.contains(&key.as_str()))
        .map(|&(id, _)| id);
    books
        .iter()
        .find(|b| Some(b.id) == by_alias || book_key(&b.name) == key)
}

/// One whole number of at most four digits (no chapter or verse number
/// comes close), or `None`.
fn number(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() || s.len() > 4 || !s.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn passage(book: &Book, spec: &str) -> Interpretation {
    let unreadable = || {
        Interpretation::InvalidReference(format!(
            "Couldn't read \u{201c}{} {}\u{201d} as a reference. Try something like {} 1:1 or {} 1:1\u{2013}3.",
            book.name,
            spec.trim(),
            book.name,
            book.name,
        ))
    };

    let (chapter_part, verse_part) = match spec.split_once([':', '.']) {
        Some((c, v)) => (c, Some(v)),
        None => (spec, None),
    };
    let Some(chapter) = number(chapter_part) else {
        if verse_part.is_none() && chapter_part.contains(['-', '\u{2013}', '\u{2014}']) {
            return Interpretation::InvalidReference(format!(
                "Go to one chapter at a time, for example {} {}.",
                book.name,
                chapter_part
                    .split(['-', '\u{2013}', '\u{2014}'])
                    .next()
                    .unwrap_or("1")
                    .trim(),
            ));
        }
        return unreadable();
    };

    let verses = match verse_part {
        None => None,
        Some(v) => {
            let (first, last) = match v.split_once(['-', '\u{2013}', '\u{2014}']) {
                // "John 3:16-4:2": a range into another chapter.
                Some((a, b)) if number(a).is_some() && b.contains([':', '.']) => {
                    return Interpretation::InvalidReference(
                        "A passage has to stay within one chapter.".to_string(),
                    );
                }
                Some((a, b)) => (number(a), number(b)),
                None => (number(v), number(v)),
            };
            let (Some(first), Some(last)) = (first, last) else {
                return unreadable();
            };
            Some((first, last))
        }
    };

    // In a one-chapter book, "Jude 5" conventionally means verse 5.
    let (chapter, verses) = match verses {
        None if book.chapter_count == 1 && chapter > 1 => (1, Some((chapter, chapter))),
        _ => (chapter, verses),
    };

    if chapter < 1 || chapter > book.chapter_count {
        let chapters = if book.chapter_count == 1 {
            "1 chapter".to_string()
        } else {
            format!("{} chapters", book.chapter_count)
        };
        return Interpretation::InvalidReference(format!(
            "No such chapter or verse: {} {chapter}. {} has {chapters}.",
            book.name, book.name,
        ));
    }
    if let Some((first, last)) = verses {
        if first < 1 || last < 1 {
            return Interpretation::InvalidReference(format!(
                "No such chapter or verse: {} {chapter}:{first}. Verses start at 1.",
                book.name,
            ));
        }
        if last < first {
            return Interpretation::InvalidReference(format!(
                "That range runs backwards. Try {} {chapter}:{last}\u{2013}{first}.",
                book.name,
            ));
        }
    }

    Interpretation::Passage(PassageRef {
        book_id: book.id,
        chapter,
        verses,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    /// The real book list, straight from the bundled KJV file the seed step
    /// takes it from.
    fn books() -> Vec<Book> {
        #[derive(Deserialize)]
        struct RawBook {
            name: String,
            testament: String,
            chapters: Vec<serde::de::IgnoredAny>,
        }
        let raw: Vec<RawBook> =
            serde_json::from_str(include_str!("../../data/kjv.json")).expect("bundled kjv.json");
        raw.into_iter()
            .enumerate()
            .map(|(i, b)| Book {
                id: i as i64 + 1,
                name: b.name,
                testament: b.testament,
                chapter_count: b.chapters.len() as i64,
            })
            .collect()
    }

    fn at(book_id: i64, chapter: i64, verses: Option<(i64, i64)>) -> Interpretation {
        Interpretation::Passage(PassageRef {
            book_id,
            chapter,
            verses,
        })
    }

    #[test]
    fn references_parse_to_passages() {
        let books = books();
        let cases: &[(&str, Interpretation)] = &[
            ("John 3:16", at(43, 3, Some((16, 16)))),
            ("john 3:16", at(43, 3, Some((16, 16)))),
            ("JOHN 3:16", at(43, 3, Some((16, 16)))),
            ("jn 3:16", at(43, 3, Some((16, 16)))),
            ("Jn. 3:16", at(43, 3, Some((16, 16)))),
            ("jhn 3", at(43, 3, None)),
            ("John 3:16-18", at(43, 3, Some((16, 18)))),
            ("John 3:16\u{2013}18", at(43, 3, Some((16, 18)))),
            ("John 3:16 \u{2014} 18", at(43, 3, Some((16, 18)))),
            ("John 3.16", at(43, 3, Some((16, 16)))),
            ("  John   3 : 16  ", at(43, 3, Some((16, 16)))),
            ("1 Cor 13", at(46, 13, None)),
            ("1cor 13:4-7", at(46, 13, Some((4, 7)))),
            ("1co 13", at(46, 13, None)),
            ("I Cor 13", at(46, 13, None)),
            ("1 Corinthians 13:13", at(46, 13, Some((13, 13)))),
            ("First Corinthians 13", at(46, 13, None)),
            ("Psalm 23", at(19, 23, None)),
            ("Psalms 23:1", at(19, 23, Some((1, 1)))),
            ("ps 119:105", at(19, 119, Some((105, 105)))),
            ("psa 1", at(19, 1, None)),
            ("Gen 1:1", at(1, 1, Some((1, 1)))),
            ("Gen. 1", at(1, 1, None)),
            ("ex 20:13", at(2, 20, Some((13, 13)))),
            ("mt 5:3", at(40, 5, Some((3, 3)))),
            ("Matt 5", at(40, 5, None)),
            ("rev 22:21", at(66, 22, Some((21, 21)))),
            ("1 John 4:8", at(62, 4, Some((8, 8)))),
            ("1jn 4:8", at(62, 4, Some((8, 8)))),
            ("I John 4", at(62, 4, None)),
            ("1john4:8", at(62, 4, Some((8, 8)))),
            ("3 John 1:4", at(64, 1, Some((4, 4)))),
            ("II Kings 2", at(12, 2, None)),
            ("2 Sam 7", at(10, 7, None)),
            ("Song of Solomon 2:1", at(22, 2, Some((1, 1)))),
            ("song of songs 2", at(22, 2, None)),
            ("sos 2", at(22, 2, None)),
            ("Jude 1", at(65, 1, None)),
            ("Jude 5", at(65, 1, Some((5, 5)))),
            ("Jude 1:3", at(65, 1, Some((3, 3)))),
            ("Obadiah 1:4", at(31, 1, Some((4, 4)))),
            ("phm 6", at(57, 1, Some((6, 6)))),
            ("Revelation 1", at(66, 1, None)),
        ];
        for (input, expected) in cases {
            assert_eq!(&interpret(input, &books), expected, "input {input:?}");
        }
    }

    #[test]
    fn invalid_references_explain_themselves() {
        let books = books();
        let cases: &[(&str, &str)] = &[
            (
                "John 22",
                "No such chapter or verse: John 22. John has 21 chapters.",
            ),
            (
                "Jude 2:1",
                "No such chapter or verse: Jude 2. Jude has 1 chapter.",
            ),
            (
                "Psalm 151",
                "No such chapter or verse: Psalms 151. Psalms has 150 chapters.",
            ),
            (
                "Gen 0",
                "No such chapter or verse: Genesis 0. Genesis has 50 chapters.",
            ),
            (
                "John 3:0",
                "No such chapter or verse: John 3:0. Verses start at 1.",
            ),
            (
                "John 3:18-16",
                "That range runs backwards. Try John 3:16\u{2013}18.",
            ),
            ("John 3:16-4:2", "A passage has to stay within one chapter."),
            (
                "John 3-4",
                "Go to one chapter at a time, for example John 3.",
            ),
            ("Jhon 3:16", "There's no book called \u{201c}Jhon\u{201d}."),
            (
                "Hezekiah 1:1",
                "There's no book called \u{201c}Hezekiah\u{201d}.",
            ),
            ("3:16", "Which book? Try something like John 3:16."),
            (
                "John 3::16",
                "Couldn't read \u{201c}John 3::16\u{201d} as a reference. Try something like John 1:1 or John 1:1\u{2013}3.",
            ),
            (
                "John 99999",
                "Couldn't read \u{201c}John 99999\u{201d} as a reference. Try something like John 1:1 or John 1:1\u{2013}3.",
            ),
        ];
        for (input, message) in cases {
            assert_eq!(
                interpret(input, &books),
                Interpretation::InvalidReference((*message).to_string()),
                "input {input:?}"
            );
        }
    }

    #[test]
    fn words_are_text_searches() {
        let books = books();
        let cases: &[(&str, &[&str])] = &[
            ("faith hope love", &["faith", "hope", "love"]),
            ("Faith, HOPE & love!", &["faith", "hope", "love"]),
            ("love love", &["love"]),
            ("love 1", &["love", "1"]),
            // Close to a book, but not one a reference accepts.
            ("jo", &["jo"]),
            ("psal", &["psal"]),
            ("johns", &["johns"]),
            ("mark john", &["mark", "john"]),
            ("4 john", &["4", "john"]),
            ("the song", &["the", "song"]),
            ("brother\u{2019}s keeper", &["brother", "s", "keeper"]),
            ("don't", &["don", "t"]),
        ];
        for (input, terms) in cases {
            let terms = terms.iter().map(|t| (*t).to_string()).collect();
            assert_eq!(
                interpret(input, &books),
                Interpretation::Text(terms),
                "input {input:?}"
            );
        }
        assert_eq!(interpret("", &books), Interpretation::Empty);
        assert_eq!(interpret("   ", &books), Interpretation::Empty);
        assert_eq!(interpret("!?\u{2014}", &books), Interpretation::Empty);
    }

    #[test]
    fn term_marks_find_each_word_a_term_is() {
        let text = "And now abideth faith, hope, charity, these three; but the greatest of these is charity.";
        let terms = search_terms("Charity faith");
        let marked: Vec<&str> = term_marks(text, &terms)
            .into_iter()
            .map(|(from, to)| &text[from..to])
            .collect();
        assert_eq!(marked, ["faith", "charity", "charity"]);
        // Whole words only: "these" isn't "the", nor "greatest" "great".
        assert_eq!(term_marks(text, &search_terms("the great")), [(55, 58)]);
        assert!(term_marks(text, &[]).is_empty());
        assert!(term_marks("", &search_terms("love")).is_empty());
    }

    #[test]
    fn term_marks_fold_case_and_accents_and_keep_the_texts_bytes() {
        // "Élie" typed plainly; the verse's own letters stay as they are.
        let text = "Élie dit: «Ô Éternel!» ÉLIE.";
        let marks = term_marks(text, &search_terms("elie eternel"));
        let marked: Vec<&str> = marks.iter().map(|&(from, to)| &text[from..to]).collect();
        assert_eq!(marked, ["Élie", "Éternel", "ÉLIE"]);
        // A decomposed accent is a mark, not part of a word, as in the
        // index: "e\u{301}" is "e" and an accent, so "elie" isn't there.
        assert!(term_marks("e\u{301}lie", &search_terms("elie")).is_empty());
        assert_eq!(term_marks("e\u{301}lie", &search_terms("lie")), [(3, 6)]);
        // Digits are words too.
        assert_eq!(
            term_marks("Psalm 23, verse 1", &search_terms("23")),
            [(6, 8)]
        );
    }

    #[test]
    fn a_book_alone_is_a_text_search_that_offers_its_first_chapter() {
        let books = books();
        let cases: &[(&str, &[&str], i64)] = &[
            ("Psalms", &["psalms"], 19),
            ("psalm", &["psalm"], 19),
            ("PS", &["ps"], 19),
            ("  psalms  ", &["psalms"], 19),
            ("John", &["john"], 43),
            ("jn.", &["jn"], 43),
            ("mark", &["mark"], 41),
            ("Numbers", &["numbers"], 4),
            ("acts", &["acts"], 44),
            ("job", &["job"], 18),
            ("Judges", &["judges"], 7),
            ("Revelation", &["revelation"], 66),
            ("revelations", &["revelations"], 66),
            ("1 cor", &["1", "cor"], 46),
            ("1 Corinthians", &["1", "corinthians"], 46),
            ("1 john", &["1", "john"], 62),
            ("1john", &["1john"], 62),
            ("I John", &["i", "john"], 62),
            ("First   John", &["first", "john"], 62),
            ("3 jn", &["3", "jn"], 64),
            ("song of songs", &["song", "of", "songs"], 22),
            ("Song of Solomon", &["song", "of", "solomon"], 22),
            ("Jude", &["jude"], 65),
        ];
        for (input, terms, book_id) in cases {
            let terms = terms.iter().map(|t| (*t).to_string()).collect();
            assert_eq!(
                interpret(input, &books),
                Interpretation::TextOrBook(
                    terms,
                    PassageRef {
                        book_id: *book_id,
                        chapter: 1,
                        verses: None,
                    },
                ),
                "input {input:?}"
            );
        }
        // Every book, by its own name.
        for book in &books {
            assert!(
                matches!(
                    interpret(&book.name, &books),
                    Interpretation::TextOrBook(_, PassageRef { book_id, chapter: 1, verses: None })
                        if book_id == book.id
                ),
                "{}",
                book.name
            );
        }
        // With a number it's still a reference, as before.
        assert_eq!(interpret("Psalms 1", &books), at(19, 1, None));
        assert_eq!(interpret("mark 2", &books), at(41, 2, None));
    }

    #[test]
    fn search_terms_are_capped() {
        let many: Vec<String> = (0..40).map(|i| format!("w{i}")).collect();
        assert_eq!(search_terms(&many.join(" ")).len(), MAX_SEARCH_TERMS);
    }

    #[test]
    fn aliases_are_unique_and_never_shadow_a_book_name() {
        let books = books();
        let mut seen = std::collections::HashMap::new();
        for &(id, aliases) in ALIASES {
            for alias in aliases {
                assert_eq!(
                    book_key(alias),
                    *alias,
                    "alias {alias} should be pre-normalized"
                );
                if let Some(other) = seen.insert(*alias, id) {
                    panic!("alias {alias} used for both book {other} and book {id}");
                }
                if let Some(book) = books.iter().find(|b| book_key(&b.name) == *alias) {
                    assert_eq!(book.id, id, "alias {alias} shadows {}", book.name);
                }
            }
        }
        assert_eq!(ALIASES.len(), 66);
        for (i, &(id, _)) in ALIASES.iter().enumerate() {
            assert_eq!(id, i as i64 + 1);
        }
    }

    #[test]
    fn every_book_is_reachable_by_its_own_name() {
        let books = books();
        for book in &books {
            assert_eq!(
                interpret(&format!("{} 1", book.name), &books),
                at(book.id, 1, None),
                "{}",
                book.name
            );
        }
    }
}
