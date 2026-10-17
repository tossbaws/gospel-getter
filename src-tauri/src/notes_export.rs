//! The notes export: the reader's notes as a readable Markdown file, for a
//! notes app, a "second brain" or printing. Every note's verses are quoted
//! in one translation, named once in the file's header; notes themselves
//! belong to no translation. It's never imported back (the JSON export in
//! `reader_data` is the backup).
//!
//! Which notes go in is a `NotesFilter`: some books (or all), a range of
//! one book's chapters, and the days the notes were written on. The same
//! filter counts the notes the export dialog says match, and picks the
//! ones the file holds.
//!
//! Rendering is a pure function of the notes, the verses they quote, the
//! translation, the filter and a time zone (see `NotesExport::render`), so
//! it's tested without a database or the machine's own time zone.
//!
//! Note bodies and verse text are plain text: everything Markdown or HTML
//! could read as structure is escaped, so the file shows exactly the
//! characters written (and stored), never markup. The one exception is the
//! KJV's own `{…}` markup, which the file shows as a printed KJV would
//! (see `domain::kjv_markup`). The verses are quoted by
//! `passage_copy::markdown_quote`, as Copy as… quotes them.

use std::collections::HashMap;
use std::fmt::Write as _;

use chrono::{DateTime, Datelike, Days, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::db::notes::{grouped, in_bible_order};
use crate::db::{Book, Note, Verse};
use crate::passage_copy;

/// Which notes an export holds. Each part narrows it; the default (no
/// part set) is every note.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct NotesFilter {
    /// The books whose notes go in; empty is every book.
    pub books: Vec<i64>,
    /// First and last chapter, inclusive. Only with exactly one book. A
    /// note is on one chapter, so it's in the range or out of it.
    pub chapters: Option<(u32, u32)>,
    /// The first local day a note may have been written on (its
    /// `created_at`; editing it later doesn't count), inclusive; `None`
    /// is no limit.
    pub written_from: Option<NaiveDate>,
    /// The last local day, inclusive; `None` is no limit.
    pub written_to: Option<NaiveDate>,
}

impl NotesFilter {
    /// Whether this filter asks for anything that `books` and the
    /// calendar can't hold: a book that doesn't exist, a chapter range
    /// without exactly one book or outside it, or dates the wrong way
    /// round. The message is the reader's.
    pub fn check(&self, books: &[Book]) -> Result<(), String> {
        for &id in &self.books {
            book_named(books, id).ok_or_else(|| format!("There's no book {id}."))?;
        }
        if let Some((first, last)) = self.chapters {
            let [id] = self.book_ids()[..] else {
                return Err("A chapter range needs exactly one book.".to_string());
            };
            if first > last {
                return Err(format!("Chapter {first} is after chapter {last}."));
            }
            let book = book_named(books, id).expect("checked above");
            if first < 1 || i64::from(last) > book.chapter_count {
                return Err(format!(
                    "Chapters {first}\u{2013}{last} aren\u{2019}t all in {}, which has {}.",
                    book.name, book.chapter_count
                ));
            }
        }
        if let (Some(from), Some(to)) = (self.written_from, self.written_to)
            && from > to
        {
            return Err("The From date is after the To date.".to_string());
        }
        Ok(())
    }

    /// Whether `note` is one this filter asks for, its date read in `tz`.
    /// With a date limit, a note whose date can't be read isn't.
    pub fn matches<Tz: TimeZone>(&self, note: &Note, tz: &Tz) -> bool {
        if !self.books.is_empty() && !self.books.contains(&note.book_id) {
            return false;
        }
        if let Some((first, last)) = self.chapters
            && !(i64::from(first)..=i64::from(last)).contains(&note.chapter)
        {
            return false;
        }
        if self.written_from.is_none() && self.written_to.is_none() {
            return true;
        }
        let Ok(written) = DateTime::parse_from_rfc3339(&note.created_at) else {
            return false;
        };
        let day = local_date(written.to_utc(), tz);
        self.written_from.is_none_or(|from| day >= from)
            && self.written_to.is_none_or(|to| day <= to)
    }

    /// The books asked for, each once, in Bible order; empty is all.
    fn book_ids(&self) -> Vec<i64> {
        let mut ids = self.books.clone();
        ids.sort_unstable();
        ids.dedup();
        ids
    }
}

/// Book `id`, if it's one of the 66.
fn book_named(books: &[Book], id: i64) -> Option<&Book> {
    books.iter().find(|b| b.id == id && (1..=66).contains(&id))
}

/// Everything one export is made from.
#[derive(Debug, Clone)]
pub struct NotesExport<'a> {
    /// Every note, in any order. Only those `filter` asks for are written,
    /// in Bible order (see `in_bible_order`).
    pub notes: &'a [Note],
    /// Every book, for names.
    pub books: &'a [Book],
    /// The quoted translation's verses, keyed by (book, chapter): every
    /// verse it numbers in each chapter a note is on. A chapter that's
    /// missing quotes nothing (every verse of its notes is "not numbered").
    pub verses: &'a HashMap<(i64, i64), Vec<Verse>>,
    /// e.g. "kjv", shown as "KJV" when a verse isn't numbered in it.
    pub translation_code: &'a str,
    /// e.g. "King James Version", named in the header.
    pub translation_name: &'a str,
    /// Which notes go in. Checked already (see `NotesFilter::check`).
    pub filter: &'a NotesFilter,
    pub exported_at: DateTime<Utc>,
}

impl NotesExport<'_> {
    /// The notes this export writes: those the filter asks for (their
    /// dates read in `tz`), in Bible order.
    pub fn notes_in_scope<Tz: TimeZone>(&self, tz: &Tz) -> Vec<Note> {
        in_bible_order(
            self.notes
                .iter()
                .filter(|n| self.filter.matches(n, tz))
                .cloned()
                .collect(),
        )
    }

    /// What the export is of, for its title and file name: "John",
    /// "John 3–5", "John and Romans", "Psalms, John and 1 John",
    /// "Psalms, John and 2 more", or `None` for every book. A chapter
    /// range shows only when it leaves out some of the book's notes, so
    /// a range around all of them is just the book.
    pub fn subject(&self) -> Option<String> {
        let ids = self.filter.book_ids();
        let names: Vec<&str> = ids.iter().map(|&id| self.book_name(id)).collect();
        match names[..] {
            [] => None,
            [name] => Some(match self.filter.chapters {
                Some((first, last))
                    if self.notes.iter().any(|n| {
                        n.book_id == ids[0]
                            && !(i64::from(first)..=i64::from(last)).contains(&n.chapter)
                    }) =>
                {
                    if first == last {
                        format!("{name} {first}")
                    } else {
                        format!("{name} {first}\u{2013}{last}")
                    }
                }
                _ => name.to_string(),
            }),
            [first, second] => Some(format!("{first} and {second}")),
            [first, second, third] => Some(format!("{first}, {second} and {third}")),
            [first, second, ..] => Some(format!("{first}, {second} and {} more", names.len() - 2)),
        }
    }

    /// "Written Oct 1 – Oct 12, 2026", "Written since Oct 1, 2026"
    /// or "Written up to Oct 12, 2026", when the filter limits the dates.
    fn written(&self) -> Option<String> {
        let range = match (self.filter.written_from, self.filter.written_to) {
            (None, None) => return None,
            (Some(from), None) => format!("since {}", display_date(from)),
            (None, Some(to)) => format!("up to {}", display_date(to)),
            (Some(from), Some(to)) if from == to => display_date(to),
            (Some(from), Some(to)) if from.year() == to.year() => {
                format!("{} \u{2013} {}", from.format("%b %-d"), display_date(to))
            }
            (Some(from), Some(to)) => {
                format!("{} \u{2013} {}", display_date(from), display_date(to))
            }
        };
        Some(format!("Written {range}"))
    }

    /// The name the save dialog suggests, with the export's local date.
    pub fn file_name<Tz: TimeZone>(&self, tz: &Tz) -> String {
        file_name(self.subject().as_deref(), local_date(self.exported_at, tz))
    }

    fn book_name(&self, book_id: i64) -> &str {
        self.books
            .iter()
            .find(|b| b.id == book_id)
            .map_or("?", |b| b.name.as_str())
    }

    /// The file: UTF-8 (no byte-order mark), LF line endings, dates in
    /// `tz`.
    pub fn render<Tz: TimeZone>(&self, tz: &Tz) -> String {
        let notes = self.notes_in_scope(tz);
        let mut out = String::new();
        let title = match self.subject() {
            Some(subject) => format!("Notes on {subject}"),
            None => "Gospel Getter notes".to_string(),
        };
        let count = match notes.len() {
            1 => "1 note".to_string(),
            n => format!("{} notes", grouped(n)),
        };
        let _ = write!(
            out,
            "# {}\n\nExported {} \u{b7} Verse text: {} \u{b7} {count}\n",
            escape_inline(&title),
            display_date(local_date(self.exported_at, tz)),
            escape_inline(self.translation_name),
        );
        if let Some(written) = self.written() {
            let _ = writeln!(out, "\n{}", escape_inline(&written));
        }
        // With more than one book, each has its own heading, and notes go
        // a level below it.
        let one_book = self.filter.book_ids().len() == 1;
        let note_level = if one_book { "##" } else { "###" };
        let mut book_shown = None;
        for note in &notes {
            if !one_book && book_shown != Some(note.book_id) {
                book_shown = Some(note.book_id);
                let _ = write!(
                    out,
                    "\n## {}\n",
                    escape_inline(self.book_name(note.book_id))
                );
            }
            out.push('\n');
            out.push_str(note_level);
            out.push(' ');
            self.render_note(&mut out, note, tz);
        }
        out
    }

    fn render_note<Tz: TimeZone>(&self, out: &mut String, note: &Note, tz: &Tz) {
        let reference = passage_reference(
            self.book_name(note.book_id),
            note.chapter,
            note.verse_start,
            note.verse_end,
        );
        let _ = writeln!(out, "{}", escape_inline(&reference));

        // The verses exactly as this translation stores and numbers them;
        // any it doesn't number are said to be missing, never dropped.
        let quoted: Vec<&Verse> = self
            .verses
            .get(&(note.book_id, note.chapter))
            .map(|verses| {
                verses
                    .iter()
                    .filter(|v| (note.verse_start..=note.verse_end).contains(&v.verse))
                    .collect()
            })
            .unwrap_or_default();
        if !quoted.is_empty() {
            let _ = writeln!(
                out,
                "\n{}",
                passage_copy::markdown_quote(self.translation_code, &quoted)
            );
        }
        let wanted = note.verse_end - note.verse_start + 1;
        if (quoted.len() as i64) < wanted {
            let code = escape_inline(&self.translation_code.to_uppercase());
            let _ = writeln!(
                out,
                "\n*{} numbered in the {code}.*",
                if quoted.is_empty() {
                    "It isn\u{2019}t"
                } else {
                    "Not all of it is"
                },
            );
        }

        let _ = writeln!(out, "\n{}", escape_block(&note.body));

        let written = stored_date(&note.created_at, tz);
        let edited = stored_date(&note.updated_at, tz);
        let _ = write!(out, "\n*Written {}", escape_inline(&written));
        if edited != written {
            let _ = write!(out, " \u{b7} edited {}", escape_inline(&edited));
        }
        out.push_str("*\n");
    }
}

/// "John 3:16" or "John 3:16–18", as the app writes a passage.
fn passage_reference(book_name: &str, chapter: i64, first: i64, last: i64) -> String {
    if first == last {
        format!("{book_name} {chapter}:{first}")
    } else {
        format!("{book_name} {chapter}:{first}\u{2013}{last}")
    }
}

/// The calendar date `at` falls on in `tz`.
pub fn local_date<Tz: TimeZone>(at: DateTime<Utc>, tz: &Tz) -> NaiveDate {
    at.with_timezone(tz).date_naive()
}

/// "Oct 10, 2026": the Library's style (`toLocaleDateString('en', { year:
/// 'numeric', month: 'short', day: 'numeric' })`).
pub fn display_date(date: NaiveDate) -> String {
    date.format("%b %-d, %Y").to_string()
}

/// A stored UTC timestamp's date in `tz`, for display; the stored text
/// itself if it can't be read (so nothing is silently dropped).
fn stored_date<Tz: TimeZone>(stored: &str, tz: &Tz) -> String {
    DateTime::parse_from_rfc3339(stored).map_or_else(
        |_| stored.to_string(),
        |at| display_date(local_date(at.to_utc(), tz)),
    )
}

/// The export's suggested file name: `gospel-getter-notes-<subject>-<date>.md`
/// for some books ("John 3–5" → `…-notes-john-3-5-…`, "John, Romans and 2
/// more" → `…-notes-john-romans-and-2-more-…`), or
/// `gospel-getter-notes-<date>.md` for all of them, with the local `date`
/// (the export's, never the filter's).
pub fn file_name(subject: Option<&str>, date: NaiveDate) -> String {
    let date = date.format("%Y-%m-%d");
    match subject {
        Some(subject) => format!("gospel-getter-notes-{}-{date}.md", slug(subject)),
        None => format!("gospel-getter-notes-{date}.md"),
    }
}

/// A first and last day, inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DaySpan {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

/// The export dialog's date choices, in the reader's local days: each
/// ends today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatePresets {
    pub today: NaiveDate,
    /// Today and the 6 days before it.
    pub last_7_days: DaySpan,
    /// Today and the 29 days before it.
    pub last_30_days: DaySpan,
    /// Jan 1 of this year to today.
    pub this_year: DaySpan,
}

/// The date choices when it's `today`.
pub fn date_presets(today: NaiveDate) -> DatePresets {
    let back = |days| DaySpan {
        from: today - Days::new(days),
        to: today,
    };
    DatePresets {
        today,
        last_7_days: back(6),
        last_30_days: back(29),
        this_year: DaySpan {
            from: today.with_ordinal(1).expect("every year has a first day"),
            to: today,
        },
    }
}

/// A book with notes, as the export dialog lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotedBook {
    pub id: i64,
    pub name: String,
    /// How many notes it has.
    pub notes: usize,
    /// The first and last chapters with notes: the chapter range's
    /// starting values.
    pub first_chapter: i64,
    pub last_chapter: i64,
    /// How many chapters have notes. With only one there's no range to
    /// choose, so the dialog doesn't offer one.
    pub noted_chapters: usize,
    /// The range's limits are 1 and this.
    pub chapter_count: i64,
}

/// The books `notes` are on, in Bible order.
pub fn noted_books(notes: &[Note], books: &[Book]) -> Vec<NotedBook> {
    let mut noted: Vec<NotedBook> = Vec::new();
    for note in in_bible_order(notes.to_vec()) {
        match noted.last_mut() {
            Some(book) if book.id == note.book_id => {
                book.notes += 1;
                if note.chapter != book.last_chapter {
                    book.noted_chapters += 1;
                }
                book.last_chapter = note.chapter;
            }
            _ => {
                let Some(book) = book_named(books, note.book_id) else {
                    continue;
                };
                noted.push(NotedBook {
                    id: book.id,
                    name: book.name.clone(),
                    notes: 1,
                    first_chapter: note.chapter,
                    last_chapter: note.chapter,
                    noted_chapters: 1,
                    chapter_count: book.chapter_count,
                });
            }
        }
    }
    noted
}

/// A name for a file name: lowercase, with a hyphen for each run of
/// anything but letters and digits ("1 John" → "1-john").
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for word in name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        if !out.is_empty() {
            out.push('-');
        }
        out.push_str(&word.to_ascii_lowercase());
    }
    out
}

/// Characters that could start inline Markdown or HTML anywhere in a line:
/// emphasis, code, links and images, raw HTML and autolinks, entities,
/// GFM strikethrough and tables, and the backslash itself.
const INLINE_SPECIAL: &[char] = &['\\', '`', '*', '_', '[', ']', '<', '>', '&', '~', '|'];

/// Characters that could start a block (a heading, list, quote, setext
/// underline or thematic break) at the start of a line, beyond those
/// `INLINE_SPECIAL` already escapes.
const LINE_START_SPECIAL: &[char] = &['#', '-', '+', '='];

/// Plain text within a line, with every Markdown/HTML character
/// backslash-escaped (CommonMark shows an escaped punctuation character as
/// itself).
pub fn escape_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if INLINE_SPECIAL.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// One line of plain text, escaped so it can't start a block either.
/// Leading spaces and tabs (which could make an indented code block)
/// become no-break spaces, so the indent still shows.
fn escape_line(line: &str) -> String {
    let rest = line.trim_start_matches([' ', '\t']);
    let mut out = String::new();
    for c in line[..line.len() - rest.len()].chars() {
        out.push_str(if c == '\t' {
            "\u{a0}\u{a0}\u{a0}\u{a0}"
        } else {
            "\u{a0}"
        });
    }
    let escaped = escape_inline(rest);
    // "1." or "1)" would start an ordered list.
    let digits = escaped.bytes().take_while(u8::is_ascii_digit).count();
    if (1..=9).contains(&digits) && matches!(escaped.as_bytes().get(digits), Some(b'.' | b')')) {
        out.push_str(&escaped[..digits]);
        out.push('\\');
        out.push_str(&escaped[digits..]);
    } else {
        if escaped.starts_with(LINE_START_SPECIAL) {
            out.push('\\');
        }
        out.push_str(&escaped);
    }
    out
}

/// Plain text that may run over several lines, as Markdown that shows the
/// same lines: blank lines separate paragraphs (several count as one),
/// and each single line break is a hard break (a backslash at the end of
/// the line). Trailing spaces, which show as nothing, are dropped.
pub fn escape_block(text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut blank = false;
    for line in text.split('\n') {
        let line = line.trim_end_matches([' ', '\t', '\r']);
        if line.trim().is_empty() {
            blank = !lines.is_empty();
            continue;
        }
        if blank {
            lines.push(String::new());
            blank = false;
        }
        lines.push(escape_line(line));
    }
    for i in 1..lines.len() {
        if !lines[i].is_empty() && !lines[i - 1].is_empty() {
            lines[i - 1].push('\\');
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::passage_copy::{kjv_markdown, verse_markdown};
    use chrono::FixedOffset;

    const JOHN: i64 = 43;

    fn books() -> Vec<Book> {
        [
            (19, "Psalms"),
            (40, "Matthew"),
            (JOHN, "John"),
            (62, "1 John"),
        ]
        .into_iter()
        .map(|(id, name)| Book {
            id,
            name: name.to_string(),
            testament: if id < 40 { "OT" } else { "NT" }.to_string(),
            chapter_count: 50,
        })
        .collect()
    }

    fn note(id: i64, book_id: i64, chapter: i64, (start, end): (i64, i64), body: &str) -> Note {
        Note {
            id,
            book_id,
            chapter,
            verse_start: start,
            verse_end: end,
            body: body.to_string(),
            created_at: "2026-10-10T15:00:00.000Z".to_string(),
            updated_at: "2026-10-10T15:00:00.000Z".to_string(),
        }
    }

    fn verses(numbers: impl IntoIterator<Item = i64>) -> Vec<Verse> {
        numbers
            .into_iter()
            .map(|verse| Verse {
                verse,
                text: format!("Verse {verse}."),
                footnotes: Vec::new(),
            })
            .collect()
    }

    fn mountain() -> FixedOffset {
        FixedOffset::west_opt(6 * 3600).unwrap()
    }

    fn at(stamp: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(stamp).unwrap().to_utc()
    }

    /// The file for `notes`, quoting `chapters`, in KJV, at -06:00.
    fn render(notes: &[Note], chapters: &[((i64, i64), Vec<Verse>)], book: Option<i64>) -> String {
        render_in("kjv", notes, chapters, book)
    }

    /// The same, quoting translation `code`.
    fn render_in(
        code: &str,
        notes: &[Note],
        chapters: &[((i64, i64), Vec<Verse>)],
        book: Option<i64>,
    ) -> String {
        let filter = NotesFilter {
            books: book.into_iter().collect(),
            ..NotesFilter::default()
        };
        render_filtered(code, notes, chapters, &filter)
    }

    /// The same, with any `filter`.
    fn render_filtered(
        code: &str,
        notes: &[Note],
        chapters: &[((i64, i64), Vec<Verse>)],
        filter: &NotesFilter,
    ) -> String {
        let books = books();
        let verses: HashMap<_, _> = chapters.iter().cloned().collect();
        export(notes, &books, &verses, code, filter).render(&mountain())
    }

    fn export<'a>(
        notes: &'a [Note],
        books: &'a [Book],
        verses: &'a HashMap<(i64, i64), Vec<Verse>>,
        code: &'a str,
        filter: &'a NotesFilter,
    ) -> NotesExport<'a> {
        NotesExport {
            notes,
            books,
            verses,
            translation_code: code,
            translation_name: if code == "kjv" {
                "King James Version"
            } else {
                "World English Bible"
            },
            filter,
            exported_at: at("2026-10-10T18:00:00.000Z"),
        }
    }

    /// The parts of `file` that are about one note: from its heading to
    /// the next heading.
    fn sections(file: &str) -> Vec<&str> {
        file.split("\n## ")
            .skip(1)
            .flat_map(|s| s.split("\n### "))
            .collect()
    }

    #[test]
    fn a_line_start_can_never_begin_a_block() {
        for (line, expected) in [
            ("# Heading", r"\# Heading"),
            ("## Two", r"\## Two"),
            ("- item", r"\- item"),
            ("* item", r"\* item"),
            ("+ item", r"\+ item"),
            ("> quote", r"\> quote"),
            ("1. first", r"1\. first"),
            ("12) twelfth", r"12\) twelfth"),
            ("```rust", r"\`\`\`rust"),
            ("~~~", r"\~\~\~"),
            ("---", r"\---"),
            ("===", r"\==="),
            ("___", r"\_\_\_"),
            ("| a | b |", r"\| a \| b \|"),
            ("2026 was a year.", "2026 was a year."),
            ("1.5 miles", r"1\.5 miles"),
        ] {
            assert_eq!(escape_block(line), expected, "{line:?}");
        }
    }

    #[test]
    fn inline_markdown_and_html_are_shown_as_typed() {
        assert_eq!(
            escape_block("so *loved* the _world_, `code`, [a](b) and \\ too"),
            r"so \*loved\* the \_world\_, \`code\`, \[a\](b) and \\ too"
        );
        assert_eq!(
            escape_block("<script>alert(1)</script> &amp; <b>bold</b>"),
            r"\<script\>alert(1)\</script\> \&amp; \<b\>bold\</b\>"
        );
        assert_eq!(escape_block("~~struck~~"), r"\~\~struck\~\~");
        // Indentation stays, but never makes a code block.
        assert_eq!(
            escape_block("    indented\n\tone tab"),
            "\u{a0}\u{a0}\u{a0}\u{a0}indented\\\n\u{a0}\u{a0}\u{a0}\u{a0}one tab"
        );
    }

    #[test]
    fn line_breaks_and_blank_lines_are_kept() {
        assert_eq!(
            escape_block("First line\nsecond line\n\nNew paragraph"),
            "First line\\\nsecond line\n\nNew paragraph"
        );
        // Several blank (or blank-looking) lines are one paragraph break;
        // trailing spaces go.
        assert_eq!(escape_block("a  \n\n \n\t\nb"), "a\n\nb");
        // A line ending in a backslash keeps it, and still breaks.
        assert_eq!(escape_block("C:\\\nnext"), "C:\\\\\\\nnext");
    }

    #[test]
    fn overlapping_notes_each_quote_their_own_verses() {
        let notes = [
            note(1, JOHN, 3, (16, 18), "The range."),
            note(2, JOHN, 3, (16, 16), "Just 16."),
            note(3, JOHN, 3, (16, 16), "Also 16."),
        ];
        let file = render(&notes, &[((JOHN, 3), verses(1..=36))], Some(JOHN));
        assert_eq!(
            sections(&file),
            [
                "John 3:16\n\n> **16** Verse 16.\n\nJust 16.\n\n*Written Oct 10, 2026*\n",
                "John 3:16\n\n> **16** Verse 16.\n\nAlso 16.\n\n*Written Oct 10, 2026*\n",
                "John 3:16\u{2013}18\n\n> **16** Verse 16. **17** Verse 17. **18** Verse 18.\n\n\
                 The range.\n\n*Written Oct 10, 2026*\n",
            ]
        );
    }

    #[test]
    fn verses_the_translation_does_not_number_are_said_to_be_missing() {
        // Matthew 2 has 23 verses in the WEB but, say, 22 here.
        let notes = [
            note(1, 40, 2, (22, 23), "Partly there."),
            note(2, 40, 2, (23, 23), "Not there at all."),
        ];
        let file = render(&notes, &[((40, 2), verses(1..=22))], None);
        assert_eq!(
            sections(&file),
            [
                "Matthew\n",
                "Matthew 2:22\u{2013}23\n\n> **22** Verse 22.\n\n\
                 *Not all of it is numbered in the KJV.*\n\nPartly there.\n\n\
                 *Written Oct 10, 2026*\n",
                "Matthew 2:23\n\n*It isn\u{2019}t numbered in the KJV.*\n\n\
                 Not there at all.\n\n*Written Oct 10, 2026*\n",
            ]
        );
        // A chapter that wasn't looked up quotes nothing, but keeps the note.
        let file = render(&notes[1..], &[], None);
        assert!(file.contains("*It isn\u{2019}t numbered in the KJV.*\n\nNot there at all."));
    }

    #[test]
    fn verse_text_is_quoted_exactly_but_escaped() {
        let notes = [note(1, JOHN, 1, (1, 2), "Note.")];
        let chapter = vec![
            Verse {
                verse: 1,
                text: "In the beginning [was] the {Word}.".to_string(),
                footnotes: Vec::new(),
            },
            Verse {
                verse: 2,
                text: "The same*was <in>\u{a0}the beginning.".to_string(),
                footnotes: Vec::new(),
            },
        ];
        // Braces too: only the KJV's are markup.
        let file = render_in("web", &notes, &[((JOHN, 1), chapter.clone())], Some(JOHN));
        assert!(file.contains(
            "> **1** In the beginning \\[was\\] the {Word}. \
             **2** The same\\*was \\<in\\>\u{a0}the beginning.\n"
        ));
        let file = render(&notes, &[((JOHN, 1), chapter)], Some(JOHN));
        assert!(file.contains("> **1** In the beginning \\[was\\] the *Word*. **2** The same"));
    }

    /// Only the KJV's braces are markup: every WEB verse is quoted exactly
    /// as before (escaped), as is a KJV verse with malformed markup.
    #[test]
    fn other_translations_are_quoted_exactly() {
        for book in crate::db::test_support::bundled("web") {
            for text in book.chapters.iter().flatten() {
                assert_eq!(verse_markdown("web", text), escape_block(text));
            }
        }
        let hebrews = "substance. {in yourselves...: or, that ye have in or, for} yourselves}";
        assert_eq!(verse_markdown("kjv", hebrews), escape_block(hebrews));
        assert_eq!(verse_markdown("web", "the {Word}"), "the {Word}");
    }

    #[test]
    fn kjv_supplied_words_are_escaped_then_italic_and_notes_left_out() {
        let md = |text: &str| kjv_markdown(text);
        assert_eq!(
            md("The LORD {is} my shepherd; I shall not want.").as_deref(),
            Some("The LORD *is* my shepherd; I shall not want.")
        );
        // The words are escaped before the asterisks go on.
        assert_eq!(md("a {*b_ [c]} d").as_deref(), Some(r"a *\*b\_ \[c\]* d"));
        // Notes go, with the space before each.
        assert_eq!(
            md("So foolish {was} I. {ignorant: Heb. I knew not} {before Heb. with}").as_deref(),
            Some("So foolish *was* I.")
        );
        // Malformed, or nothing to rewrite: quoted exactly instead.
        assert_eq!(md("a {{and from} b: or, c}"), None);
        assert_eq!(md("In the beginning God created"), None);
    }

    /// The export of every KJV verse is byte for byte what 2.9.1 wrote:
    /// a fingerprint (FNV-1a, each verse's Markdown followed by 0xff) of
    /// 2.9.1's own output, taken before the rule moved to
    /// `domain::kjv_markup`.
    #[test]
    fn every_kjv_verse_exports_as_in_2_9_1() {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut verses = 0;
        for book in crate::db::test_support::bundled("kjv") {
            for chapter in &book.chapters {
                for text in chapter {
                    let out = kjv_markdown(text).unwrap_or_default();
                    for byte in out.bytes().chain([0xff]) {
                        hash ^= u64::from(byte);
                        hash = hash.wrapping_mul(0x0100_0000_01b3);
                    }
                    verses += 1;
                }
            }
        }
        assert_eq!((verses, hash), (31_100, 0x5d3b_b105_7221_fd2b));
    }

    #[test]
    fn kjv_markup_is_shown_as_a_printed_kjv_would() {
        let notes = [note(1, 19, 23, (1, 3), "Shepherd.")];
        let chapter = vec![
            Verse {
                verse: 1,
                text: "[A Psalm of David.] The LORD {is} my shepherd; I shall not want."
                    .to_string(),
                footnotes: Vec::new(),
            },
            Verse {
                verse: 2,
                text: "He leadeth me beside the still waters. {green...: Heb. pastures of tender \
                       grass} {still...: Heb. waters of quietness}"
                    .to_string(),
                footnotes: Vec::new(),
            },
            Verse {
                verse: 3,
                text: "He restoreth my soul.".to_string(),
                footnotes: Vec::new(),
            },
        ];
        let file = render(&notes, &[((19, 23), chapter)], Some(19));
        // Notes go with their space: no space before the next number.
        assert!(file.contains(
            "> **1** \\[A Psalm of David.\\] The LORD *is* my shepherd; I shall not want. \
             **2** He leadeth me beside the still waters. **3** He restoreth my soul.\n"
        ));
    }

    #[test]
    fn dates_are_in_the_local_time_zone() {
        // 11 pm on Oct 9 at -06:00 is 05:00 UTC on Oct 10.
        let late = at("2026-10-10T05:00:00.000Z");
        assert_eq!(
            local_date(late, &mountain()),
            NaiveDate::from_ymd_opt(2026, 10, 9).unwrap()
        );
        assert_eq!(
            local_date(late, &Utc),
            NaiveDate::from_ymd_opt(2026, 10, 10).unwrap()
        );
        let mut n = note(1, JOHN, 3, (16, 16), "Late.");
        n.created_at = "2026-10-10T05:00:00.000Z".to_string();
        n.updated_at = n.created_at.clone();
        let file = render(&[n], &[((JOHN, 3), verses(1..=36))], Some(JOHN));
        assert!(file.contains("*Written Oct 9, 2026*\n"), "{file}");
        // The export date too: 03:00 UTC on Jan 1 is still Dec 31 here.
        let books = books();
        let file = NotesExport {
            notes: &[note(1, JOHN, 3, (16, 16), "x")],
            books: &books,
            verses: &HashMap::new(),
            translation_code: "web",
            translation_name: "World English Bible",
            filter: &NotesFilter {
                books: vec![JOHN],
                ..NotesFilter::default()
            },
            exported_at: at("2027-01-01T03:00:00Z"),
        }
        .render(&mountain());
        assert!(file.starts_with(
            "# Notes on John\n\nExported Dec 31, 2026 \u{b7} Verse text: World English Bible \u{b7} 1 note\n"
        ));
    }

    #[test]
    fn the_edited_date_shows_only_when_it_differs() {
        let mut same_day = note(1, JOHN, 3, (16, 16), "Edited later that day.");
        same_day.created_at = "2026-10-10T14:00:00.000Z".to_string();
        same_day.updated_at = "2026-10-11T04:00:00.000Z".to_string(); // 10 pm local
        let mut next_day = note(2, JOHN, 3, (17, 17), "Edited the next day.");
        next_day.created_at = "2026-10-10T14:00:00.000Z".to_string();
        next_day.updated_at = "2026-10-12T16:30:00.000Z".to_string();
        let file = render(
            &[same_day, next_day],
            &[((JOHN, 3), verses(1..=36))],
            Some(JOHN),
        );
        let sections = sections(&file);
        assert!(sections[0].ends_with("\n*Written Oct 10, 2026*\n"));
        assert!(sections[1].ends_with("\n*Written Oct 10, 2026 \u{b7} edited Oct 12, 2026*\n"));
    }

    #[test]
    fn notes_are_in_bible_order_and_same_passage_notes_keep_theirs() {
        let mut first = note(5, JOHN, 3, (16, 16), "Older.");
        first.created_at = "2025-01-01T00:00:00.000Z".to_string();
        let notes = [
            note(1, 62, 1, (9, 9), "1 John."),
            note(2, JOHN, 3, (16, 16), "Newer."),
            note(3, JOHN, 1, (1, 1), "John 1."),
            note(4, 19, 23, (1, 2), "Psalms."),
            first,
        ];
        let file = render(&notes, &[], None);
        let headings: Vec<&str> = file.lines().filter(|l| l.starts_with('#')).collect();
        assert_eq!(
            headings,
            [
                "# Gospel Getter notes",
                "## Psalms",
                "### Psalms 23:1\u{2013}2",
                "## John",
                "### John 1:1",
                "### John 3:16",
                "### John 3:16",
                "## 1 John",
                "### 1 John 1:9",
            ]
        );
        let older = file.find("Older.").unwrap();
        let newer = file.find("Newer.").unwrap();
        assert!(older < newer);
    }

    #[test]
    fn one_book_and_all_notes_have_their_own_headers() {
        let notes = [
            note(1, JOHN, 3, (16, 16), "John."),
            note(2, 62, 1, (9, 9), "1 John."),
        ];
        let all = render(&notes, &[], None);
        assert!(all.starts_with(
            "# Gospel Getter notes\n\n\
             Exported Oct 10, 2026 \u{b7} Verse text: King James Version \u{b7} 2 notes\n\n\
             ## John\n\n### John 3:16\n"
        ));
        let one = render(&notes, &[], Some(62));
        assert!(one.starts_with(
            "# Notes on 1 John\n\n\
             Exported Oct 10, 2026 \u{b7} Verse text: King James Version \u{b7} 1 note\n\n\
             ## 1 John 1:9\n"
        ));
        assert!(!one.contains("John 3:16"));
        assert!(!one.contains('\r') && !one.starts_with('\u{feff}'));
        assert!(one.ends_with("*\n") && !one.ends_with("\n\n"));
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn written(mut n: Note, stamp: &str) -> Note {
        n.created_at = stamp.to_string();
        n
    }

    /// Psalms 23, John 1, 3 and 5, Romans 8 and 1 John 1, written over a
    /// few days; John 3:17 was written at 10 pm on Oct 9 at -06:00, which
    /// is already Oct 10 in UTC.
    fn spread() -> Vec<Note> {
        vec![
            written(
                note(1, 19, 23, (1, 1), "Psalms."),
                "2026-10-01T15:00:00.000Z",
            ),
            written(
                note(2, JOHN, 1, (1, 1), "John 1."),
                "2026-10-05T15:00:00.000Z",
            ),
            written(
                note(3, JOHN, 3, (16, 16), "John 3."),
                "2026-10-09T15:00:00.000Z",
            ),
            written(
                note(4, JOHN, 3, (17, 17), "Late."),
                "2026-10-10T04:00:00.000Z",
            ),
            written(
                note(5, JOHN, 5, (24, 24), "John 5."),
                "2026-10-12T15:00:00.000Z",
            ),
            written(
                note(6, 45, 8, (28, 28), "Romans."),
                "2026-10-11T15:00:00.000Z",
            ),
            written(
                note(7, 62, 1, (9, 9), "1 John."),
                "2026-10-12T15:00:00.000Z",
            ),
        ]
    }

    fn with_romans() -> Vec<Book> {
        let mut books = books();
        books.push(Book {
            id: 45,
            name: "Romans".to_string(),
            testament: "NT".to_string(),
            chapter_count: 16,
        });
        books.sort_by_key(|b| b.id);
        books
    }

    /// The bodies of the notes `filter` asks for, in Bible order.
    fn picked<Tz: TimeZone>(filter: &NotesFilter, tz: &Tz) -> Vec<String> {
        let notes = spread();
        let books = with_romans();
        let verses = HashMap::new();
        export(&notes, &books, &verses, "kjv", filter)
            .notes_in_scope(tz)
            .into_iter()
            .map(|n| n.body)
            .collect()
    }

    #[test]
    fn the_filter_picks_by_book_chapter_and_local_day_written() {
        let all = NotesFilter::default();
        assert_eq!(
            picked(&all, &mountain()),
            [
                "Psalms.", "John 1.", "John 3.", "Late.", "John 5.", "Romans.", "1 John."
            ]
        );
        let books = |ids: &[i64]| NotesFilter {
            books: ids.to_vec(),
            ..NotesFilter::default()
        };
        // Books in any order, each once, come out in Bible order.
        assert_eq!(
            picked(&books(&[62, 19, 62]), &mountain()),
            ["Psalms.", "1 John."]
        );
        // A chapter range is inclusive at both ends.
        let john = |first, last| NotesFilter {
            books: vec![JOHN],
            chapters: Some((first, last)),
            ..NotesFilter::default()
        };
        assert_eq!(
            picked(&john(3, 5), &mountain()),
            ["John 3.", "Late.", "John 5."]
        );
        assert_eq!(
            picked(&john(1, 3), &mountain()),
            ["John 1.", "John 3.", "Late."]
        );
        assert_eq!(picked(&john(2, 2), &mountain()), Vec::<String>::new());

        // Dates are inclusive local days: from Oct 5 to Oct 9 at -06:00
        // holds the 10 pm note, which is Oct 10 in UTC.
        let dates = |from: Option<NaiveDate>, to: Option<NaiveDate>| NotesFilter {
            written_from: from,
            written_to: to,
            ..NotesFilter::default()
        };
        let oct = |d| Some(day(2026, 10, d));
        assert_eq!(
            picked(&dates(oct(5), oct(9)), &mountain()),
            ["John 1.", "John 3.", "Late."]
        );
        assert_eq!(picked(&dates(oct(5), oct(9)), &Utc), ["John 1.", "John 3."]);
        assert_eq!(picked(&dates(oct(10), oct(10)), &Utc), ["Late."]);
        assert_eq!(
            picked(&dates(oct(10), oct(10)), &mountain()),
            Vec::<String>::new()
        );
        // Open at either end.
        assert_eq!(
            picked(&dates(oct(11), None), &mountain()),
            ["John 5.", "Romans.", "1 John."]
        );
        assert_eq!(picked(&dates(None, oct(1)), &mountain()), ["Psalms."]);

        // Everything together.
        let narrow = NotesFilter {
            written_from: oct(9),
            ..john(3, 5)
        };
        assert_eq!(
            picked(&narrow, &mountain()),
            ["John 3.", "Late.", "John 5."]
        );
        let narrower = NotesFilter {
            written_to: oct(9),
            ..narrow
        };
        assert_eq!(picked(&narrower, &mountain()), ["John 3.", "Late."]);
    }

    #[test]
    fn written_means_created_never_edited() {
        let mut edited = written(
            note(1, JOHN, 3, (16, 16), "Edited."),
            "2026-10-01T15:00:00.000Z",
        );
        edited.updated_at = "2026-10-12T15:00:00.000Z".to_string();
        let since_oct_12 = NotesFilter {
            written_from: Some(day(2026, 10, 12)),
            ..NotesFilter::default()
        };
        assert!(!since_oct_12.matches(&edited, &Utc));
        let up_to_oct_1 = NotesFilter {
            written_to: Some(day(2026, 10, 1)),
            ..NotesFilter::default()
        };
        assert!(up_to_oct_1.matches(&edited, &Utc));
        // A date that can't be read matches no date limit, but isn't
        // dropped without one.
        let unreadable = written(note(2, JOHN, 3, (16, 16), "?"), "yesterday");
        assert!(!up_to_oct_1.matches(&unreadable, &Utc));
        assert!(NotesFilter::default().matches(&unreadable, &Utc));
    }

    #[test]
    fn a_filter_that_cannot_be_met_is_refused_plainly() {
        let books = with_romans();
        let check = |filter: NotesFilter| filter.check(&books);
        assert_eq!(check(NotesFilter::default()), Ok(()));
        for id in [0, 67, -1, 2] {
            // Book 2 is real, but not one these tests know.
            assert_eq!(
                check(NotesFilter {
                    books: vec![JOHN, id],
                    ..NotesFilter::default()
                }),
                Err(format!("There's no book {id}."))
            );
        }
        let range = |ids: Vec<i64>, chapters| NotesFilter {
            books: ids,
            chapters: Some(chapters),
            ..NotesFilter::default()
        };
        for ids in [vec![], vec![JOHN, 45]] {
            assert_eq!(
                check(range(ids, (1, 2))),
                Err("A chapter range needs exactly one book.".to_string())
            );
        }
        // The same book twice is still one book.
        assert_eq!(check(range(vec![JOHN, JOHN], (1, 50))), Ok(()));
        assert_eq!(
            check(range(vec![45], (5, 3))),
            Err("Chapter 5 is after chapter 3.".to_string())
        );
        for chapters in [(0, 3), (3, 17)] {
            assert_eq!(
                check(range(vec![45], chapters)),
                Err(format!(
                    "Chapters {}\u{2013}{} aren\u{2019}t all in Romans, which has 16.",
                    chapters.0, chapters.1
                ))
            );
        }
        assert_eq!(check(range(vec![45], (1, 16))), Ok(()));
        let dates = |from, to| NotesFilter {
            written_from: Some(day(2026, 10, from)),
            written_to: Some(day(2026, 10, to)),
            ..NotesFilter::default()
        };
        assert_eq!(
            check(dates(12, 11)),
            Err("The From date is after the To date.".to_string())
        );
        assert_eq!(check(dates(12, 12)), Ok(()));
    }

    #[test]
    fn the_filter_arrives_from_the_frontend_as_json() {
        let filter: NotesFilter = serde_json::from_value(serde_json::json!({
            "books": [43],
            "chapters": [3, 5],
            "writtenFrom": "2026-10-01",
            "writtenTo": null,
        }))
        .unwrap();
        assert_eq!(
            filter,
            NotesFilter {
                books: vec![43],
                chapters: Some((3, 5)),
                written_from: Some(day(2026, 10, 1)),
                written_to: None,
            }
        );
        let empty: NotesFilter = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(empty, NotesFilter::default());
        // A misspelled part is an error, not every note.
        assert!(
            serde_json::from_value::<NotesFilter>(serde_json::json!({ "book": [43] })).is_err()
        );
        assert!(
            serde_json::from_value::<NotesFilter>(serde_json::json!({ "writtenFrom": "Oct 1" }))
                .is_err()
        );
    }

    /// The title and file name for `filter` over `spread()`.
    fn named(filter: &NotesFilter) -> (String, String) {
        let notes = spread();
        let books = with_romans();
        let verses = HashMap::new();
        let export = export(&notes, &books, &verses, "kjv", filter);
        let file = export.render(&mountain());
        let header = file.split("\n\n## ").next().unwrap().to_string();
        (header, export.file_name(&mountain()))
    }

    #[test]
    fn titles_and_file_names_say_what_the_export_holds() {
        let only = |ids: &[i64]| NotesFilter {
            books: ids.to_vec(),
            ..NotesFilter::default()
        };
        let exported = "Exported Oct 10, 2026 \u{b7} Verse text: King James Version";
        let cases = [
            (
                NotesFilter::default(),
                "# Gospel Getter notes",
                "7 notes",
                "gospel-getter-notes-2026-10-10.md",
            ),
            (
                only(&[JOHN]),
                "# Notes on John",
                "4 notes",
                "gospel-getter-notes-john-2026-10-10.md",
            ),
            (
                NotesFilter {
                    chapters: Some((3, 5)),
                    ..only(&[JOHN])
                },
                "# Notes on John 3\u{2013}5",
                "3 notes",
                "gospel-getter-notes-john-3-5-2026-10-10.md",
            ),
            (
                NotesFilter {
                    chapters: Some((3, 3)),
                    ..only(&[JOHN])
                },
                "# Notes on John 3",
                "2 notes",
                "gospel-getter-notes-john-3-2026-10-10.md",
            ),
            // A range around all of the book's notes is just the book.
            (
                NotesFilter {
                    chapters: Some((1, 5)),
                    ..only(&[JOHN])
                },
                "# Notes on John",
                "4 notes",
                "gospel-getter-notes-john-2026-10-10.md",
            ),
            (
                only(&[45, JOHN]),
                "# Notes on John and Romans",
                "5 notes",
                "gospel-getter-notes-john-and-romans-2026-10-10.md",
            ),
            (
                only(&[62, JOHN, 19]),
                "# Notes on Psalms, John and 1 John",
                "6 notes",
                "gospel-getter-notes-psalms-john-and-1-john-2026-10-10.md",
            ),
            (
                only(&[62, 45, JOHN, 19]),
                "# Notes on Psalms, John and 2 more",
                "7 notes",
                "gospel-getter-notes-psalms-john-and-2-more-2026-10-10.md",
            ),
        ];
        for (filter, title, count, file) in cases {
            let (header, name) = named(&filter);
            assert_eq!(
                header,
                format!("{title}\n\n{exported} \u{b7} {count}"),
                "{filter:?}"
            );
            assert_eq!(name, file, "{filter:?}");
        }
        // One book: notes straight under the title; several: under their
        // books.
        let notes = spread();
        let books = with_romans();
        let verses = HashMap::new();
        let one = export(&notes, &books, &verses, "kjv", &only(&[JOHN])).render(&mountain());
        assert!(one.contains("\n\n## John 1:1\n") && !one.contains("## John\n"));
        let two_filter = only(&[JOHN, 45]);
        let two = export(&notes, &books, &verses, "kjv", &two_filter).render(&mountain());
        assert!(two.contains("\n\n## John\n\n### John 1:1\n") && two.contains("\n## Romans\n"));
    }

    #[test]
    fn a_date_limit_adds_a_written_line() {
        let dates = |from: Option<NaiveDate>, to: Option<NaiveDate>| NotesFilter {
            books: vec![JOHN],
            chapters: Some((3, 5)),
            written_from: from,
            written_to: to,
        };
        for (from, to, line, count) in [
            (
                Some(day(2026, 10, 1)),
                Some(day(2026, 10, 12)),
                "Written Oct 1 \u{2013} Oct 12, 2026",
                "3 notes",
            ),
            (
                Some(day(2025, 12, 20)),
                Some(day(2026, 10, 9)),
                "Written Dec 20, 2025 \u{2013} Oct 9, 2026",
                "2 notes",
            ),
            (
                Some(day(2026, 10, 9)),
                Some(day(2026, 10, 9)),
                "Written Oct 9, 2026",
                "2 notes",
            ),
            (
                Some(day(2026, 10, 10)),
                None,
                "Written since Oct 10, 2026",
                "1 note",
            ),
            (
                None,
                Some(day(2026, 10, 9)),
                "Written up to Oct 9, 2026",
                "2 notes",
            ),
        ] {
            let (header, name) = named(&dates(from, to));
            assert_eq!(
                header,
                format!(
                    "# Notes on John 3\u{2013}5\n\n\
                     Exported Oct 10, 2026 \u{b7} Verse text: King James Version \u{b7} {count}\n\n\
                     {line}"
                )
            );
            // The file name has the export's date, never the filter's.
            assert_eq!(name, "gospel-getter-notes-john-3-5-2026-10-10.md");
        }
    }

    #[test]
    fn date_presets_end_today_in_local_days() {
        let presets = date_presets(day(2026, 10, 12));
        assert_eq!(presets.today, day(2026, 10, 12));
        assert_eq!(
            presets.last_7_days,
            DaySpan {
                from: day(2026, 10, 6),
                to: day(2026, 10, 12)
            }
        );
        assert_eq!(
            presets.last_30_days,
            DaySpan {
                from: day(2026, 9, 13),
                to: day(2026, 10, 12)
            }
        );
        assert_eq!(
            presets.this_year,
            DaySpan {
                from: day(2026, 1, 1),
                to: day(2026, 10, 12)
            }
        );
        // Across a year's start, and on Jan 1 itself.
        let presets = date_presets(day(2027, 1, 1));
        assert_eq!(presets.last_7_days.from, day(2026, 12, 26));
        assert_eq!(
            presets.this_year,
            DaySpan {
                from: day(2027, 1, 1),
                to: day(2027, 1, 1)
            }
        );
        // A leap day counts as a day.
        assert_eq!(
            date_presets(day(2028, 3, 2)).last_7_days.from,
            day(2028, 2, 25)
        );
        assert_eq!(
            serde_json::to_value(date_presets(day(2026, 10, 12))).unwrap(),
            serde_json::json!({
                "today": "2026-10-12",
                "last7Days": { "from": "2026-10-06", "to": "2026-10-12" },
                "last30Days": { "from": "2026-09-13", "to": "2026-10-12" },
                "thisYear": { "from": "2026-01-01", "to": "2026-10-12" },
            })
        );
    }

    #[test]
    fn noted_books_are_in_bible_order_with_counts_and_chapter_spans() {
        let books = with_romans();
        assert_eq!(
            noted_books(&spread(), &books)
                .iter()
                .map(|b| (
                    b.id,
                    b.name.as_str(),
                    b.notes,
                    b.first_chapter,
                    b.last_chapter,
                    b.noted_chapters,
                    b.chapter_count
                ))
                .collect::<Vec<_>>(),
            [
                (19, "Psalms", 1, 23, 23, 1, 50),
                // Chapters 1, 3 (twice) and 5: three chapters with notes.
                (JOHN, "John", 4, 1, 5, 3, 50),
                (45, "Romans", 1, 8, 8, 1, 16),
                (62, "1 John", 1, 1, 1, 1, 50),
            ]
        );
        assert_eq!(noted_books(&[], &books), []);
    }

    #[test]
    fn file_names_use_a_slug_and_the_local_date() {
        let date = NaiveDate::from_ymd_opt(2026, 10, 9).unwrap();
        assert_eq!(slug("1 John"), "1-john");
        assert_eq!(slug("Song of Solomon"), "song-of-solomon");
        assert_eq!(slug("John"), "john");
        assert_eq!(
            file_name(Some("Song of Solomon"), date),
            "gospel-getter-notes-song-of-solomon-2026-10-09.md"
        );
        assert_eq!(file_name(None, date), "gospel-getter-notes-2026-10-09.md");
    }
}
