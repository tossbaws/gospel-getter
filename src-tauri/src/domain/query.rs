//! Interpreting what the reader typed into the search box: either a
//! reference to go to (`John 3:16`, `1 Cor 13`, `ps 23:1-3`) or words to
//! search the text for, words in double quotes as a phrase; and finding the references typed in a note
//! ("see Rom 5:8"), by the same rules, for the page to link. Pure — no
//! database access — so it's tested here against the real book list
//! without seeding anything.

use crate::book_aliases::ALIASES;
use crate::db::Book;

/// The most words one text search will match on, a phrase's words each
/// counted; anything past this is ignored rather than building an
/// ever-larger FTS query.
pub const MAX_SEARCH_TERMS: usize = 12;

/// What opens and closes a phrase: a straight double quote, or a curly one
/// either way (a keyboard's smart quotes).
const QUOTES: [char; 3] = ['"', '\u{201c}', '\u{201d}'];

/// A text search: words a verse must have, anywhere, and phrases (words
/// typed in double quotes) it must have as typed: in order, side by side.
/// Every word is lowercased, as [`search_query`] makes them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchQuery {
    pub words: Vec<String>,
    pub phrases: Vec<Vec<String>>,
}

impl SearchQuery {
    /// Free words alone, as a search without quotes is.
    pub fn of_words(words: &[&str]) -> Self {
        Self {
            words: words.iter().map(|w| (*w).to_owned()).collect(),
            phrases: Vec::new(),
        }
    }

    /// Nothing to search for.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.phrases.is_empty()
    }

    /// How many words, a phrase's each counted (see [`MAX_SEARCH_TERMS`]).
    pub fn len(&self) -> usize {
        self.words.len() + self.phrases.iter().map(Vec::len).sum::<usize>()
    }

    /// What was searched for, as the search box says it: each phrase in
    /// curly quotes, then each word.
    pub fn labels(&self) -> Vec<String> {
        self.phrases
            .iter()
            .map(|p| format!("\u{201c}{}\u{201d}", p.join(" ")))
            .chain(self.words.iter().cloned())
            .collect()
    }
}

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
    /// Words and phrases to search the text for (see [`search_query`]).
    Text(SearchQuery),
    /// A book's name or abbreviation alone (`Psalms`, `1 cor`): words to
    /// search the text for, as `Text`, and that book's first chapter to go
    /// to as well, offered first.
    TextOrBook(SearchQuery, PassageRef),
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

    // Quotes mean words to search for: `"John 3:16"` is a phrase.
    let quoted = input.contains(QUOTES);
    let (book_part, spec) = split_reference(input);
    if !quoted && !spec.is_empty() {
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

    let terms = search_query(input);
    if terms.is_empty() {
        return Interpretation::Empty;
    }
    match find_book(input, books).filter(|_| !quoted) {
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

/// The words of `text`: runs of letters and digits, lowercased, in order.
fn words_of(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
}

/// The words and phrases of a text search. Words typed in double quotes
/// (straight `"`, or curly `“ ”`) are a phrase, matched only as typed; the
/// rest are words, matched anywhere in the verse. A word is a run of
/// letters and digits, lowercased, so a quote's other characters (an
/// apostrophe, a comma, a single quote) only part words, as in the verse;
/// and nothing typed is ever query syntax. An unclosed quote's words are
/// words; a phrase of one word is that word; repeats are dropped. At most
/// [`MAX_SEARCH_TERMS`] words in all, in the order typed: a phrase that
/// would go past it keeps the words that fit.
pub fn search_query(input: &str) -> SearchQuery {
    let parts: Vec<&str> = input.split(QUOTES).collect();
    // Split at every quote, the odd parts are inside a pair; the last part
    // of an odd number of quotes (an even count of parts) isn't closed.
    let closed = if parts.len().is_multiple_of(2) {
        parts.len() - 1
    } else {
        parts.len()
    };
    let mut query = SearchQuery::default();
    for (i, part) in parts.iter().enumerate() {
        let room = MAX_SEARCH_TERMS - query.len();
        if room == 0 {
            break;
        }
        if i % 2 == 1 && i < closed {
            let phrase: Vec<String> = words_of(part).take(room).collect();
            match phrase.len() {
                0 => {}
                1 => push_word(&mut query.words, &phrase[0]),
                _ if !query.phrases.contains(&phrase) => query.phrases.push(phrase),
                _ => {}
            }
        } else {
            for word in words_of(part) {
                if query.len() == MAX_SEARCH_TERMS {
                    break;
                }
                push_word(&mut query.words, &word);
            }
        }
    }
    query
}

fn push_word(words: &mut Vec<String>, word: &str) {
    if !words.iter().any(|w| w == word) {
        words.push(word.to_owned());
    }
}

/// Where a text search (`query`, as [`search_query`] gives it) is in a
/// verse's `text`, to be marked in its result: the byte ranges of the
/// words of `text` that are one of its words, or that are a phrase's words
/// where they stand as the phrase (in order, side by side), in order. A
/// word is a run of letters, digits and private-use characters, as
/// SQLite's FTS5 tokenizer splits the text it indexes (`db::search`), and
/// two words are the same if they are once decomposed (NFD), without their
/// marks, and lowercased; what's between two words (a space, a comma) is
/// no word, as for the index. Every word is looked at, so a search marks
/// each place it is in the verse. This is the one rule for it: the page
/// draws the marks where it says.
pub fn term_marks(text: &str, query: &SearchQuery) -> Vec<(usize, usize)> {
    let spans = word_spans_of(text);
    let words: Vec<String> = spans.iter().map(|&(a, b)| folded(&text[a..b])).collect();
    let mut marked = vec![false; spans.len()];
    let free: Vec<String> = query.words.iter().map(|w| folded(w)).collect();
    for (i, word) in words.iter().enumerate() {
        marked[i] |= free.contains(word);
    }
    for phrase in query.phrases.iter().filter(|p| !p.is_empty()) {
        let phrase: Vec<String> = phrase.iter().map(|w| folded(w)).collect();
        for start in 0..words.len().saturating_sub(phrase.len() - 1) {
            if words[start..start + phrase.len()] == phrase[..] {
                marked[start..start + phrase.len()].fill(true);
            }
        }
    }
    spans
        .into_iter()
        .zip(marked)
        .filter_map(|(span, marked)| marked.then_some(span))
        .collect()
}

/// The byte ranges of `text`'s words (see [`term_marks`]), in order.
fn word_spans_of(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = None;
    for (i, c) in text
        .char_indices()
        .chain(std::iter::once((text.len(), ' ')))
    {
        let in_word = i < text.len() && word_char(c);
        match (start, in_word) {
            (None, true) => start = Some(i),
            (Some(from), false) => {
                spans.push((from, i));
                start = None;
            }
            _ => {}
        }
    }
    spans
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

// ---- References typed in a note ("see Rom 5:8"), found so the page can
// make each one a link to its passage.

/// A reference found in a note's text by [`references_in`]: where it is
/// (byte offsets, `start..end`) and the passage it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FoundReference {
    pub start: usize,
    pub end: usize,
    pub passage: PassageRef,
}

/// Abbreviations that are everyday words too (in `book_key`'s form): "Is
/// 5" or "So 3" is much more often a word and a number than Isaiah 5 or
/// the Song of Solomon 3, so these name a book only with a verse ("Is
/// 53:5"). A book's own name ("Acts 2", "Mark 4", "Job 5") always does.
const WORD_LIKE: &[&str] = &[
    "am", "is", "so", "re", "ex", "la", "ho", "na", "de", "ne", "es", "da", "ti", "ac", "ge", "le",
    "nu", "ob", "pr", "ec", "je", "ga", "mar", "pro", "act", "col", "tit", "jud", "lam", "nah",
    "hag", "mic", "sos", "song", "cant",
];

/// The most words a book's name is written with ("Song of Solomon", "1
/// Cor", "First John").
const MAX_NAME_WORDS: usize = 4;

/// The references to passages in `text` (a note, as the reader wrote it),
/// in order, never overlapping: each a book's name or abbreviation (as Go
/// to reads it, `interpret`), its first letter a capital, then a space
/// and a chapter, with a verse or a range of verses or not ("Rom 5:8",
/// "John 3:16-18", "1 Cor 13", "Ps 23", "Jude 5"), on one line, and
/// standing alone: no letter or digit right before or after it, nor a
/// further ":" or "." and digits ("5:30:15"). A chapter or verse the book
/// can't have, a range into another chapter or of chapters, and an
/// everyday word with a chapter alone (`WORD_LIKE`) aren't references;
/// "I Am", "Job well done", "Acts of kindness" and a time like "5:30"
/// never are.
pub fn references_in(text: &str, books: &[Book]) -> Vec<FoundReference> {
    let words = word_spans(text);
    let mut found: Vec<FoundReference> = Vec::new();
    for i in 0..words.len() {
        if found.last().is_some_and(|f| words[i].0 < f.end) {
            continue;
        }
        if let Some(reference) = reference_at(text, &words[i..], books) {
            found.push(reference);
        }
    }
    found
}

/// The byte ranges of `text`'s runs of letters and digits.
fn word_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        match (start, c.is_alphanumeric()) {
            (None, true) => start = Some(i),
            (Some(from), false) => {
                spans.push((from, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        spans.push((from, text.len()));
    }
    spans
}

/// The reference starting at `words[0]`, if one does: the longest run of
/// words naming a book that's followed by a chapter.
fn reference_at(text: &str, words: &[(usize, usize)], books: &[Book]) -> Option<FoundReference> {
    let start = words.first()?.0;
    for count in (1..=MAX_NAME_WORDS.min(words.len())).rev() {
        let name_words = &words[..count];
        // The name's words are apart by spaces alone, on one line.
        let spaced = name_words
            .windows(2)
            .all(|pair| is_spaces(&text[pair[0].1..pair[1].0]));
        let name_end = name_words[count - 1].1;
        let name = &text[start..name_end];
        let capital = name
            .chars()
            .find(|c| c.is_alphabetic())
            .is_some_and(char::is_uppercase);
        if !spaced || !capital {
            continue;
        }
        // An abbreviation's period, then a space or more, then a digit.
        let after_name = name_end + usize::from(text[name_end..].starts_with('.'));
        let gap = text[after_name..].len()
            - text[after_name..]
                .trim_start_matches([' ', '\t', '\u{a0}'])
                .len();
        let spec_start = after_name + gap;
        if gap == 0 || !text[spec_start..].starts_with(|c: char| c.is_ascii_digit()) {
            continue;
        }
        let Some(book) = find_book(&text[start..after_name], books) else {
            continue;
        };
        let spec_end = spec_start + spec_length(&text[spec_start..])?;
        let spec = &text[spec_start..spec_end];
        if !spec.contains([':', '.']) && WORD_LIKE.contains(&book_key(name).as_str()) {
            return None;
        }
        return match passage(book, spec) {
            Interpretation::Passage(passage) => Some(FoundReference {
                start,
                end: spec_end,
                passage,
            }),
            _ => None,
        };
    }
    None
}

/// Whether `s` is one or more spaces or tabs.
fn is_spaces(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| matches!(c, ' ' | '\t' | '\u{a0}'))
}

/// How long the chapter and verses at the start of `s` are ("3", "3:16",
/// "3:16-18", with ":" or "." and any dash), if they stand alone: `None`
/// for a range of chapters ("3-4") or into another chapter ("3:16-4:2"),
/// a number too long to be a chapter or verse, or one run on into another
/// (":30:15") or into a word.
fn spec_length(s: &str) -> Option<usize> {
    let digits = |from: usize| s[from..].bytes().take_while(u8::is_ascii_digit).count();
    // A separator (":" or "." for the verse, a dash for a range) at
    // `at`, followed by a digit: its length.
    let separator = |at: usize, verse: bool| {
        let c = s[at..].chars().next()?;
        let fits = if verse {
            matches!(c, ':' | '.')
        } else {
            matches!(c, '-' | '\u{2013}' | '\u{2014}')
        };
        let after = at + c.len_utf8();
        (fits && s[after..].starts_with(|c: char| c.is_ascii_digit())).then_some(after - at)
    };
    let chapter = digits(0);
    if !(1..=3).contains(&chapter) {
        return None;
    }
    let mut end = chapter;
    if let Some(sep) = separator(end, true) {
        let verse = digits(end + sep);
        if verse > 3 {
            return None;
        }
        end += sep + verse;
        if let Some(dash) = separator(end, false) {
            let last = digits(end + dash);
            if last > 3 {
                return None;
            }
            end += dash + last;
        }
    } else if separator(end, false).is_some() {
        // "John 3-4": chapters, which Go to won't open either.
        return None;
    }
    // Standing alone: no more numbers run on, nor a word.
    let rest = &s[end..];
    let runs_on = rest.starts_with(|c: char| c.is_alphanumeric())
        || separator(end, true).is_some()
        || separator(end, false).is_some();
    (!runs_on).then_some(end)
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
            assert_eq!(
                interpret(input, &books),
                Interpretation::Text(SearchQuery::of_words(terms)),
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
        let terms = search_query("Charity faith");
        let marked: Vec<&str> = term_marks(text, &terms)
            .into_iter()
            .map(|(from, to)| &text[from..to])
            .collect();
        assert_eq!(marked, ["faith", "charity", "charity"]);
        // Whole words only: "these" isn't "the", nor "greatest" "great".
        assert_eq!(term_marks(text, &search_query("the great")), [(55, 58)]);
        assert!(term_marks(text, &SearchQuery::default()).is_empty());
        assert!(term_marks("", &search_query("love")).is_empty());
    }

    #[test]
    fn term_marks_fold_case_and_accents_and_keep_the_texts_bytes() {
        // "Élie" typed plainly; the verse's own letters stay as they are.
        let text = "Élie dit: «Ô Éternel!» ÉLIE.";
        let marks = term_marks(text, &search_query("elie eternel"));
        let marked: Vec<&str> = marks.iter().map(|&(from, to)| &text[from..to]).collect();
        assert_eq!(marked, ["Élie", "Éternel", "ÉLIE"]);
        // A decomposed accent is a mark, not part of a word, as in the
        // index: "e\u{301}" is "e" and an accent, so "elie" isn't there.
        assert!(term_marks("e\u{301}lie", &search_query("elie")).is_empty());
        assert_eq!(term_marks("e\u{301}lie", &search_query("lie")), [(3, 6)]);
        // Digits are words too.
        assert_eq!(
            term_marks("Psalm 23, verse 1", &search_query("23")),
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
            assert_eq!(
                interpret(input, &books),
                Interpretation::TextOrBook(
                    SearchQuery::of_words(terms),
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
        assert_eq!(search_query(&many.join(" ")).len(), MAX_SEARCH_TERMS);
        // A phrase's words count, each: what fits is kept, in order.
        let query = search_query(&format!("a b c d e \"{}\" z", many.join(" ")));
        assert_eq!(query.len(), MAX_SEARCH_TERMS);
        assert_eq!(query.words, ["a", "b", "c", "d", "e"]);
        assert_eq!(query.phrases, [&many[..7]]);
        let full = search_query(&format!("{} \"love one\"", many.join(" ")));
        assert_eq!(full.len(), MAX_SEARCH_TERMS);
        assert!(full.phrases.is_empty());
    }

    fn phrase(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn words_in_double_quotes_are_a_phrase_mixed_with_free_words() {
        let query = search_query("\"love one another\" commandment");
        assert_eq!(query.phrases, [phrase(&["love", "one", "another"])]);
        assert_eq!(query.words, ["commandment"]);
        assert_eq!(query.len(), 4);
        assert_eq!(
            query.labels(),
            ["\u{201c}love one another\u{201d}", "commandment"]
        );
        // Curly quotes, a keyboard's smart ones, either way round, and
        // mixed with straight ones; case is folded, as for words.
        for typed in [
            "\u{201c}Love One Another\u{201d}",
            "\"LOVE one another\u{201d}",
            "\u{201d}love one another\"",
        ] {
            assert_eq!(
                search_query(typed).phrases,
                [phrase(&["love", "one", "another"])],
                "{typed}"
            );
        }
        // Two phrases and words between; repeats once.
        let two = search_query("grace \"by faith\" saved \"by faith\" \"not of works\" grace");
        assert_eq!(two.words, ["grace", "saved"]);
        assert_eq!(
            two.phrases,
            [phrase(&["by", "faith"]), phrase(&["not", "of", "works"])]
        );
        // A one-word phrase is that word; an empty one nothing.
        assert_eq!(
            search_query("\"Jesus\" wept"),
            SearchQuery::of_words(&["jesus", "wept"])
        );
        assert_eq!(
            search_query("\"\" \" \" love"),
            SearchQuery::of_words(&["love"])
        );
        assert!(search_query("\"\"").is_empty());
    }

    #[test]
    fn an_unclosed_quote_s_words_are_ordinary_words() {
        assert_eq!(
            search_query("\"love one another"),
            SearchQuery::of_words(&["love", "one", "another"])
        );
        let after = search_query("\"by grace\" are ye \"saved through");
        assert_eq!(after.phrases, [phrase(&["by", "grace"])]);
        assert_eq!(after.words, ["are", "ye", "saved", "through"]);
    }

    #[test]
    fn a_quote_mark_inside_a_phrase_parts_words_as_in_the_verse() {
        // An apostrophe (straight or curly) or a single quote isn't a
        // phrase's end: it splits the word, as the index splits the verse.
        for typed in [
            "\"my brother's keeper\"",
            "\"my brother\u{2019}s keeper\"",
            "\u{201c}my \u{2018}brother\u{2019}s\u{2019} keeper\u{201d}",
        ] {
            assert_eq!(
                search_query(typed).phrases,
                [phrase(&["my", "brother", "s", "keeper"])],
                "{typed}"
            );
        }
        // A third double quote closes the first phrase and opens another.
        let three = search_query("\"thy word\" is a \"lamp");
        assert_eq!(three.phrases, [phrase(&["thy", "word"])]);
        assert_eq!(three.words, ["is", "a", "lamp"]);
    }

    #[test]
    fn query_syntax_typed_in_quotes_or_out_is_only_words() {
        // FTS5's operators, prefixes, columns and grouping are words, or
        // nothing: never syntax.
        let injected = search_query("\"a\" OR \"b\" NEAR(love hate) * text:faith ^ - +x (\"c d\")");
        assert_eq!(
            injected.words,
            ["a", "or", "b", "near", "love", "hate", "text", "faith", "x"]
        );
        assert_eq!(injected.phrases, [phrase(&["c", "d"])]);
        assert!(search_query("* ^ - + ( ) : ").is_empty());
    }

    #[test]
    fn quotes_make_a_reference_a_phrase_and_a_book_name_words() {
        let books = books();
        assert_eq!(interpret("John 3:16", &books), at(43, 3, Some((16, 16))));
        assert_eq!(
            interpret("\"John 3:16\"", &books),
            Interpretation::Text(SearchQuery {
                words: Vec::new(),
                phrases: vec![phrase(&["john", "3", "16"])],
            })
        );
        assert_eq!(
            interpret("\u{201c}ps 23\u{201d}", &books),
            Interpretation::Text(SearchQuery {
                words: Vec::new(),
                phrases: vec![phrase(&["ps", "23"])],
            })
        );
        // A book's name in quotes is a word to search for, not the book.
        assert_eq!(
            interpret("\"Psalms\"", &books),
            Interpretation::Text(SearchQuery::of_words(&["psalms"]))
        );
        assert!(matches!(
            interpret("Psalms", &books),
            Interpretation::TextOrBook(..)
        ));
    }

    #[test]
    fn a_phrase_is_marked_only_where_it_stands_as_the_phrase() {
        let marked = |text: &'static str, typed: &str| -> Vec<&'static str> {
            term_marks(text, &search_query(typed))
                .into_iter()
                .map(|(from, to)| &text[from..to])
                .collect()
        };
        let john = "A new commandment I give unto you, That ye love one another; as I have loved you, that ye also love one another.";
        assert_eq!(
            marked(john, "\"love one another\""),
            ["love", "one", "another", "love", "one", "another"]
        );
        // Out of order, or apart, the words aren't marked.
        let rom = "Be kindly affectioned one to another with brotherly love; in honour preferring one another;";
        assert_eq!(marked(rom, "\"love one another\""), Vec::<&str>::new());
        assert_eq!(marked(rom, "\"one another\""), ["one", "another"]);
        // A free word is marked anywhere; the phrase only as the phrase.
        assert_eq!(
            marked(rom, "\"one another\" love"),
            ["love", "one", "another"]
        );
        // What's between the words isn't a word: a comma or a line's break.
        assert_eq!(
            marked("Grace, mercy, and peace", "\"grace mercy\""),
            ["Grace", "mercy"]
        );
        // Folding: capitals and accents, the verse's own letters kept.
        assert_eq!(
            marked("Élie dit: «Ô ÉTERNEL Dieu»", "\"o eternel\""),
            ["Ô", "ÉTERNEL"]
        );
        // Overlapping places of the phrase are all marked.
        assert_eq!(
            marked("holy, holy, holy", "\"holy holy\""),
            ["holy", "holy", "holy"]
        );
        // A phrase longer than the verse marks nothing.
        assert_eq!(
            marked("Jesus wept.", "\"jesus wept again\""),
            Vec::<&str>::new()
        );
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

    /// The references `references_in` finds in `text`: each as typed, and
    /// the passage it names.
    fn found(text: &str) -> Vec<(&str, Interpretation)> {
        references_in(text, &books())
            .into_iter()
            .map(|r| (&text[r.start..r.end], Interpretation::Passage(r.passage)))
            .collect()
    }

    #[test]
    fn references_in_a_note_are_found_where_they_are() {
        let text = "See Rom 5:8, and John 3:16-18; then 1 Cor 13 (love) and Ps 23.";
        assert_eq!(
            found(text),
            [
                ("Rom 5:8", at(45, 5, Some((8, 8)))),
                ("John 3:16-18", at(43, 3, Some((16, 18)))),
                ("1 Cor 13", at(46, 13, None)),
                ("Ps 23", at(19, 23, None)),
            ]
        );
        // Their offsets are the note's own bytes, accents and all.
        let text = "Éphèse: Acts 19:1.";
        let r = references_in(text, &books());
        assert_eq!(&text[r[0].start..r[0].end], "Acts 19:1");
    }

    #[test]
    fn every_way_go_to_reads_a_reference_is_found() {
        for (text, expected) in [
            ("Gen. 1:1", at(1, 1, Some((1, 1)))),
            ("Matt. 5:3\u{2013}12", at(40, 5, Some((3, 12)))),
            ("Romans 8:28\u{2014}30", at(45, 8, Some((28, 30)))),
            ("John 3.16", at(43, 3, Some((16, 16)))),
            ("1 John 4:8", at(62, 4, Some((8, 8)))),
            ("I John 4:8", at(62, 4, Some((8, 8)))),
            ("First John 4:8", at(62, 4, Some((8, 8)))),
            ("1Cor 13:4", at(46, 13, Some((4, 4)))),
            ("Song of Solomon 2:1", at(22, 2, Some((1, 1)))),
            ("Song of Songs 2:1", at(22, 2, Some((1, 1)))),
            ("Psalm 119", at(19, 119, None)),
            ("Jude 5", at(65, 1, Some((5, 5)))),
            ("Phm 6", at(57, 1, Some((6, 6)))),
            ("Rev 21:4", at(66, 21, Some((4, 4)))),
            ("Acts 2", at(44, 2, None)),
            ("Mark 4", at(41, 4, None)),
            ("Job 38:4", at(18, 38, Some((4, 4)))),
            ("Is 53:5", at(23, 53, Some((5, 5)))),
            ("Am 5:24", at(30, 5, Some((24, 24)))),
        ] {
            assert_eq!(found(text), [(text, expected)], "{text:?}");
        }
        // The longest name that's a book: "1 John", not "John".
        assert_eq!(found("read 1 John 1:9")[0].0, "1 John 1:9");
        // At the very start and end, and between other characters.
        assert_eq!(found("(Rom 5:8)")[0].0, "Rom 5:8");
        assert_eq!(found("\u{201c}John 3:16\u{201d}")[0].0, "John 3:16");
        assert_eq!(found("Rom 5:8, 10")[0].0, "Rom 5:8");
    }

    #[test]
    fn words_that_only_look_like_references_are_not_found() {
        for text in [
            // No chapter.
            "I Am",
            "I Am that I Am.",
            "Job well done",
            "Acts of kindness",
            "Mark my words, John.",
            "the Song of Solomon",
            // A number but no book.
            "5:30",
            "Meet at 5:30 on 2026-10-27.",
            "Chapter 3:16 of the manual",
            "Room 12, page 4",
            // An everyday word and a chapter alone.
            "Is 5 enough?",
            "So 3 of us went.",
            "Am 5 minutes late",
            "Ex 4 me",
            // Not a capital.
            "see rom 5:8",
            "john 3:16",
            // A chapter or verse the book can't have, or ranges Go to
            // won't open.
            "John 22",
            "John 3:0",
            "Jude 2:1",
            "John 3-4",
            "John 3:16-4:2",
            "John 3:18-16",
            // Run into more: a time, a longer number, a word.
            "Mark 5:30:15",
            "John 2026",
            "Rom 5:8a",
            "John 3:16th",
            // Joined to the word before or after.
            "XJohn 3:16",
            "Rom5:8",
            // Not on one line.
            "John\n3:16",
            "1\nJohn 3",
        ] {
            let found: Vec<&str> = found(text).into_iter().map(|(typed, _)| typed).collect();
            let expected: &[&str] = match text {
                // "John 3" alone, on its own line, is one.
                "1\nJohn 3" => &["John 3"],
                _ => &[],
            };
            assert_eq!(found, expected, "{text:?}");
        }
    }

    #[test]
    fn references_never_overlap_and_keep_their_order() {
        let text = "1 John 1:9 John 1:9 Jude 5 5:30 Rom 1:1";
        let found: Vec<&str> = found(text).into_iter().map(|(typed, _)| typed).collect();
        assert_eq!(found, ["1 John 1:9", "John 1:9", "Jude 5", "Rom 1:1"]);
    }
}
