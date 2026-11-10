//! Derives the bundled King James Version (`data/kjv.json`) from
//! eBible.org's KJV USFM in `sources/kjv/` (see its README).
//!
//! Not part of the app: `examples/kjv_ingest.rs` runs it to write the
//! bundled file, and `tests/kjv_source.rs` runs it again to check that
//! file, byte for byte.
//!
//! The file keeps the shape and markup the app already reads
//! (`core/src/domain/kjv_markup.rs`): an array of books, each
//! `{"name", "testament", "chapters"}`, every chapter an array of its
//! verses' text. Each verse is the USFM's own characters, in the USFM's
//! order, with only its markers mapped or taken out. What it does, and all
//! it does:
//!
//! - **Supplied words** (`\add … \add*`, and `\+add … \+add*` inside
//!   another span) become `{…}`: the opening marker is `{`, the closing
//!   one `}`, and everything between them is copied as it stands.
//! - **Margin notes** (`\f + \fr c.v \ft … \f*`) become a `{…}` span each,
//!   after the rest of the verse, a space before each, in the USFM's
//!   order: the run of spans that ends the verse. Each is the characters
//!   after `\ft`; the caller (`+`) and the note's own reference (`\fr`)
//!   aren't copied. Every note in the source stands at the end of its
//!   verse; a note with more of the verse after it is an error, never
//!   moved.
//! - **A Psalm's title** (`\d`, before its verse 1) becomes a `[…]` prefix
//!   on that verse, a space after it.
//! - **A heading before a verse** (`\s1`: in this source only Psalm 119's
//!   acrostic letters, `\s1 \tl א ALEPH.\tl*`) becomes a prefix on that
//!   verse, a space after it.
//! - **A heading after a book's last verse** (`\s1`: the epistles'
//!   subscriptions, "Written to the Romans from Corinthus, …") becomes a
//!   `«…»` span after that verse's text, a space before it, ahead of its
//!   notes.
//! - **Markers that only mark words** are taken out and their words kept:
//!   `\w … \w*` and `\+w … \+w*` (each word's Strong's number, the part
//!   after `|`, isn't copied), `\nd … \nd*` and `\+nd … \+nd*` (the
//!   LORD's small capitals: the letters stay as the source has them),
//!   `\wj … \wj*` (the words of Christ) and `\tl … \tl*` (the acrostic's
//!   Hebrew letter: the letter stays).
//! - **Structure** isn't copied: chapter and verse numbers (`\c`, `\v`),
//!   paragraph and poetry breaks (`\p`, `\q1`, `\b`, each a space), and the
//!   book's identification and titles (`\id`, `\h`, `\toc1`–`\toc3`,
//!   `\mt1`, `\mt2`). The books' names are the ones the app has always
//!   used ([`BOOK_NAMES`]).
//! - **Spacing** is USFM's: a run of spaces and line breaks is one space,
//!   and each part (prefix, verse, subscription, note) is trimmed at its
//!   ends. A brace isn't a character of the text, so the run is counted
//!   across it: where the source has a space just inside supplied words
//!   (`\add  which\add*`, with two spaces, in 31 places), that space is
//!   one with the run outside the span, and the one space left stands
//!   outside the brace ("people {which} thou", not "people { which}").
//!
//! Anything else is an error: a marker this doesn't know, a `{`, `}`,
//! `«`, `»` or `|` already in the source's text, text outside a verse or
//! after a heading before its verse, a verse or chapter out of order, a
//! note anywhere but at its verse's end, or a book that isn't the one
//! expected. No verse is special-cased.

use serde::Serialize;

/// The USFM books, in canonical order, as the files in the zip name them.
pub const USFM_BOOKS: [&str; 66] = [
    "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA", "1KI", "2KI", "1CH",
    "2CH", "EZR", "NEH", "EST", "JOB", "PSA", "PRO", "ECC", "SNG", "ISA", "JER", "LAM", "EZK",
    "DAN", "HOS", "JOL", "AMO", "OBA", "JON", "MIC", "NAM", "HAB", "ZEP", "HAG", "ZEC", "MAL",
    "MAT", "MRK", "LUK", "JHN", "ACT", "ROM", "1CO", "2CO", "GAL", "EPH", "PHP", "COL", "1TH",
    "2TH", "1TI", "2TI", "TIT", "PHM", "HEB", "JAS", "1PE", "2PE", "1JN", "2JN", "3JN", "JUD",
    "REV",
];

/// The books' names in the bundled file, the ones the app has always
/// shown for the KJV.
pub const BOOK_NAMES: [&str; 66] = [
    "Genesis",
    "Exodus",
    "Leviticus",
    "Numbers",
    "Deuteronomy",
    "Joshua",
    "Judges",
    "Ruth",
    "1 Samuel",
    "2 Samuel",
    "1 Kings",
    "2 Kings",
    "1 Chronicles",
    "2 Chronicles",
    "Ezra",
    "Nehemiah",
    "Esther",
    "Job",
    "Psalms",
    "Proverbs",
    "Ecclesiastes",
    "Song of Solomon",
    "Isaiah",
    "Jeremiah",
    "Lamentations",
    "Ezekiel",
    "Daniel",
    "Hosea",
    "Joel",
    "Amos",
    "Obadiah",
    "Jonah",
    "Micah",
    "Nahum",
    "Habakkuk",
    "Zephaniah",
    "Haggai",
    "Zechariah",
    "Malachi",
    "Matthew",
    "Mark",
    "Luke",
    "John",
    "Acts",
    "Romans",
    "1 Corinthians",
    "2 Corinthians",
    "Galatians",
    "Ephesians",
    "Philippians",
    "Colossians",
    "1 Thessalonians",
    "2 Thessalonians",
    "1 Timothy",
    "2 Timothy",
    "Titus",
    "Philemon",
    "Hebrews",
    "James",
    "1 Peter",
    "2 Peter",
    "1 John",
    "2 John",
    "3 John",
    "Jude",
    "Revelation",
];

/// One book of the bundled KJV.
#[derive(Debug, Serialize)]
pub struct Book {
    pub name: &'static str,
    pub testament: &'static str,
    pub chapters: Vec<Vec<String>>,
}

/// What `derive` makes: the books and the bundled file's contents.
pub struct Derived {
    pub books: Vec<Book>,
    pub json: String,
}

/// The characters the mapping writes; the source's text must not have
/// them, or the mapping couldn't be read back.
const MARKUP: [char; 4] = ['{', '}', '«', '»'];

/// Book identification and titles: not verse text.
const BOOK_TITLES: [&str; 7] = ["id", "h", "toc1", "toc2", "toc3", "mt1", "mt2"];
/// Paragraph and poetry breaks: a space in the verse.
const BREAKS: [&str; 3] = ["p", "q1", "b"];
/// Character markers taken out, their words kept.
const WORD_MARKERS: [&str; 8] = ["nd", "nd*", "+nd", "+nd*", "wj", "wj*", "tl", "tl*"];

/// A verse while it's read: its parts, each mapped as the module docs say.
#[derive(Default)]
struct Verse {
    prefix: Vec<String>,
    body: String,
    subscriptions: Vec<String>,
    notes: Vec<String>,
}

impl Verse {
    fn text(&self) -> String {
        let mut out = String::new();
        for p in &self.prefix {
            out.push_str(p);
            out.push(' ');
        }
        out.push_str(&spaced(&self.body));
        for s in &self.subscriptions {
            out.push_str(" «");
            out.push_str(s);
            out.push('»');
        }
        for n in &self.notes {
            out.push_str(" {");
            out.push_str(n);
            out.push('}');
        }
        out
    }
}

/// A heading read before the verse it goes with.
enum Heading {
    /// `\d`: a Psalm's title.
    Title(String),
    /// `\s1`.
    Section(String),
}

/// Where the text being read goes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    /// Nowhere: text here is an error.
    Nowhere,
    /// Dropped: a book title's.
    Dropped,
    /// The current verse.
    Verse,
    /// The last heading read.
    Heading,
}

/// Where a note's text being read goes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Note {
    /// Not in a note.
    Out,
    /// Its caller or reference: not copied.
    Apparatus,
    /// Its text (`\ft`).
    Text,
}

/// USFM's spacing: runs of whitespace as one space, the ends trimmed, a
/// run counted across a brace and left outside it (see the module docs).
fn spaced(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    // A space read but not yet written: it's written before the next
    // character that isn't a closing brace, so it never ends up just inside
    // a span, and never at either end.
    let mut space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            space = !out.is_empty();
        } else if c == '}' {
            out.push(c);
        } else {
            if space && out.ends_with('{') {
                // "{ x": the space goes before the brace, if there isn't
                // one there already.
                out.pop();
                if !out.is_empty() && !out.ends_with(' ') {
                    out.push(' ');
                }
                out.push('{');
            } else if space {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    out
}

/// One book's chapters, each its verses' text.
pub fn read_book(code: &str, usfm: &str) -> Result<Vec<Vec<String>>, String> {
    if usfm.contains('\r') {
        return Err(format!("{code}: a carriage return"));
    }
    let mut chapters: Vec<Vec<Verse>> = Vec::new();
    let mut pending: Vec<Heading> = Vec::new();
    let mut into = Target::Nowhere;
    let mut note = Note::Out;
    let mut note_text = String::new();
    // In a `\w`, and past its `|`.
    let (mut in_word, mut in_attributes) = (false, false);

    for line in usfm.split('\n') {
        let mut rest = line;
        while !rest.is_empty() {
            if let Some(after) = rest.strip_prefix('\\') {
                let mut name_len = after
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '+'))
                    .unwrap_or(after.len());
                if after[name_len..].starts_with('*') {
                    name_len += 1;
                }
                let name = &after[..name_len];
                let mut tail = &after[name_len..];
                // An opening marker's delimiting space isn't text.
                if !name.ends_with('*') {
                    tail = tail.strip_prefix(' ').unwrap_or(tail);
                }
                rest = tail;
                let chapter = chapters.len();
                let at = || format!("{code} {chapter}: \\{name} in {line:?}");
                if note != Note::Out && !matches!(name, "fr" | "ft" | "f*" | "+nd" | "+nd*") {
                    return Err(format!("{}: in a note", at()));
                }
                match name {
                    "c" => {
                        let end = rest.find(' ').unwrap_or(rest.len());
                        let n: usize = rest[..end].parse().map_err(|_| at())?;
                        if n != chapters.len() + 1 || !pending.is_empty() {
                            return Err(format!("{}: out of order", at()));
                        }
                        chapters.push(Vec::new());
                        into = Target::Nowhere;
                        rest = &rest[end..];
                    }
                    "v" => {
                        let end = rest.find(' ').unwrap_or(rest.len());
                        let n: usize = rest[..end].parse().map_err(|_| at())?;
                        let verses = chapters.last_mut().ok_or_else(at)?;
                        if n != verses.len() + 1 {
                            return Err(format!("{}: out of order", at()));
                        }
                        let mut verse = Verse::default();
                        for heading in pending.drain(..) {
                            verse.prefix.push(match heading {
                                Heading::Title(t) => format!("[{}]", spaced(&t)),
                                Heading::Section(t) => spaced(&t),
                            });
                        }
                        verses.push(verse);
                        into = Target::Verse;
                        rest = rest[end..].strip_prefix(' ').unwrap_or(&rest[end..]);
                    }
                    "d" => {
                        pending.push(Heading::Title(String::new()));
                        into = Target::Heading;
                    }
                    "s1" => {
                        pending.push(Heading::Section(String::new()));
                        into = Target::Heading;
                    }
                    _ if BOOK_TITLES.contains(&name) => into = Target::Dropped,
                    _ if BREAKS.contains(&name) => {
                        // A break inside a verse is a space; text after
                        // one goes on with the verse, but not past a
                        // heading.
                        if let Some(verse) = chapters.last_mut().and_then(|c| c.last_mut()) {
                            verse.body.push(' ');
                            into = if pending.is_empty() {
                                Target::Verse
                            } else {
                                Target::Nowhere
                            };
                        }
                    }
                    "w" | "+w" => in_word = true,
                    "w*" | "+w*" => (in_word, in_attributes) = (false, false),
                    "add" | "+add" | "add*" | "+add*" => {
                        if into == Target::Verse
                            && chapters
                                .last()
                                .and_then(|c| c.last())
                                .is_some_and(|v| !v.notes.is_empty())
                        {
                            return Err(format!("{}: supplied words after a note", at()));
                        }
                        let brace = if name.ends_with('*') { "}" } else { "{" };
                        push(&mut chapters, &mut pending, into, brace)
                            .map_err(|e| format!("{}: {e}", at()))?;
                    }
                    "f" => {
                        if into != Target::Verse {
                            return Err(format!("{}: a note outside a verse", at()));
                        }
                        note = Note::Apparatus;
                        note_text.clear();
                    }
                    "fr" if note != Note::Out => note = Note::Apparatus,
                    "ft" if note != Note::Out => note = Note::Text,
                    "f*" if note != Note::Out => {
                        let verse = chapters
                            .last_mut()
                            .and_then(|c| c.last_mut())
                            .ok_or_else(at)?;
                        verse.notes.push(spaced(&note_text));
                        note = Note::Out;
                    }
                    _ if WORD_MARKERS.contains(&name) => {}
                    _ => return Err(format!("{}: a marker this doesn't read", at())),
                }
                continue;
            }
            let end = rest.find('\\').unwrap_or(rest.len());
            let mut text = &rest[..end];
            rest = &rest[end..];
            if in_word {
                if in_attributes {
                    continue;
                }
                if let Some((word, _attributes)) = text.split_once('|') {
                    text = word;
                    in_attributes = true;
                }
            }
            if text.contains(MARKUP) || text.contains('|') {
                return Err(format!("{code}: text with {MARKUP:?} or '|': {text:?}"));
            }
            match note {
                Note::Text => note_text.push_str(text),
                Note::Apparatus => {}
                Note::Out => {
                    if into == Target::Verse
                        && let Some(verse) = chapters.last().and_then(|c| c.last())
                        && !verse.notes.is_empty()
                        && !text.trim().is_empty()
                    {
                        return Err(format!("{code}: verse text after a note: {line:?}"));
                    }
                    push(&mut chapters, &mut pending, into, text)
                        .map_err(|e| format!("{code}: {e}: {line:?}"))?;
                }
            }
        }
        // A line break is a space; a line ends a heading, and a book title.
        match into {
            Target::Verse => {
                if let Some(verse) = chapters.last_mut().and_then(|c| c.last_mut()) {
                    verse.body.push(' ');
                }
            }
            Target::Heading | Target::Dropped => into = Target::Nowhere,
            Target::Nowhere => {}
        }
        if note != Note::Out {
            return Err(format!("{code}: a note runs past its line: {line:?}"));
        }
    }

    // A heading after the book's last verse is its subscription.
    let last = chapters
        .last_mut()
        .and_then(|c| c.last_mut())
        .ok_or_else(|| format!("{code}: no verses"))?;
    for heading in pending {
        match heading {
            Heading::Section(t) => last.subscriptions.push(spaced(&t)),
            Heading::Title(_) => return Err(format!("{code}: a title after the last verse")),
        }
    }
    Ok(chapters
        .iter()
        .map(|verses| verses.iter().map(Verse::text).collect())
        .collect())
}

/// Adds `text` to where it goes.
fn push(
    chapters: &mut [Vec<Verse>],
    pending: &mut [Heading],
    into: Target,
    text: &str,
) -> Result<(), String> {
    match into {
        Target::Verse => {
            let verse = chapters
                .last_mut()
                .and_then(|c| c.last_mut())
                .ok_or("text before the first verse")?;
            verse.body.push_str(text);
        }
        Target::Heading => match pending.last_mut() {
            Some(Heading::Title(t) | Heading::Section(t)) => t.push_str(text),
            None => return Err("a heading without its marker".to_string()),
        },
        Target::Dropped => {}
        Target::Nowhere if text.trim().is_empty() => {}
        Target::Nowhere => return Err(format!("text outside a verse: {text:?}")),
    }
    Ok(())
}

/// Derive the bundled KJV from the USFM books' texts, in canonical order.
pub fn derive(usfm_books: &[String]) -> Result<Derived, String> {
    if usfm_books.len() != 66 {
        return Err(format!("{} USFM books, not 66", usfm_books.len()));
    }
    let mut books = Vec::new();
    for (i, usfm) in usfm_books.iter().enumerate() {
        let code = USFM_BOOKS[i];
        let id = format!("\\id {code} ");
        if !usfm.starts_with(&id) {
            return Err(format!("book {} isn't {code}", i + 1));
        }
        books.push(Book {
            name: BOOK_NAMES[i],
            testament: if i < 39 { "OT" } else { "NT" },
            chapters: read_book(code, usfm)?,
        });
    }
    let json = serde_json::to_string(&books).map_err(|e| e.to_string())?;
    Ok(Derived { books, json })
}

/// The zip's file for the book `code`, among its file `names`: the one
/// named "NN-{code}eng-kjv2006.usfm".
pub fn entry_name(names: &[String], code: &str) -> Result<String, String> {
    let suffix = format!("-{code}eng-kjv2006.usfm");
    let mut found = names.iter().filter(|n| n.ends_with(&suffix));
    match (found.next(), found.next()) {
        (Some(name), None) => Ok(name.clone()),
        _ => Err(format!("no one file for {code} in the zip")),
    }
}
