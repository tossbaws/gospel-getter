//! The notes export: the reader's notes as a readable Markdown file, for a
//! notes app, a "second brain" or printing. Every note's verses are quoted
//! in one translation, named once in the file's header; notes themselves
//! belong to no translation. It's never imported back (the JSON export in
//! `reader_data` is the backup).
//!
//! Rendering is a pure function of the notes, the verses they quote, the
//! translation, the scope and a time zone (see `NotesExport::render`), so
//! it's tested without a database or the machine's own time zone.
//!
//! Note bodies and verse text are plain text: everything Markdown or HTML
//! could read as structure is escaped, so the file shows exactly the
//! characters written (and stored), never markup. The one exception is the
//! KJV's own `{…}` markup, which the file shows as a printed KJV would
//! (see `kjv_markup`).

mod kjv_markup;

use std::collections::HashMap;
use std::fmt::Write as _;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};

use crate::db::notes::{grouped, in_bible_order};
use crate::db::{Book, Note, Verse};

/// Everything one export is made from.
#[derive(Debug, Clone)]
pub struct NotesExport<'a> {
    /// The notes to export, in any order. Only those in `book` (when given)
    /// are written, in Bible order (see `in_bible_order`).
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
    /// One book's notes, or `None` for all of them.
    pub book: Option<i64>,
    pub exported_at: DateTime<Utc>,
}

impl NotesExport<'_> {
    /// The notes this export writes: those in scope, in Bible order.
    pub fn notes_in_scope(&self) -> Vec<Note> {
        in_bible_order(
            self.notes
                .iter()
                .filter(|n| self.book.is_none_or(|b| n.book_id == b))
                .cloned()
                .collect(),
        )
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
        let notes = self.notes_in_scope();
        let mut out = String::new();
        let title = match self.book {
            Some(id) => format!("Notes on {}", self.book_name(id)),
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
        // With every book, each has its own heading, and notes go a level
        // below it.
        let note_level = if self.book.is_some() { "##" } else { "###" };
        let mut book_shown = None;
        for note in &notes {
            if self.book.is_none() && book_shown != Some(note.book_id) {
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
            let text = quoted
                .iter()
                .map(|v| {
                    format!(
                        "**{}** {}",
                        v.verse,
                        verse_markdown(self.translation_code, &v.text)
                    )
                })
                .collect::<Vec<_>>()
                .join(" ");
            out.push('\n');
            for line in text.split('\n') {
                let _ = writeln!(out, "> {line}");
            }
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

/// One verse's stored text as Markdown. The KJV's `{…}` markup becomes
/// italics for supplied words, and its margin notes are left out (see
/// `kjv_markup`); any other translation's text, and a KJV verse whose
/// markup isn't well formed, is quoted exactly, escaped.
fn verse_markdown(translation_code: &str, text: &str) -> String {
    (translation_code == "kjv")
        .then(|| kjv_markup::to_markdown(text))
        .flatten()
        .unwrap_or_else(|| escape_block(text))
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

/// The export's suggested file name: `gospel-getter-notes-<book>-<date>.md`
/// for one book, `gospel-getter-notes-<date>.md` for all of them, with the
/// local `date`.
pub fn file_name(book_name: Option<&str>, date: NaiveDate) -> String {
    let date = date.format("%Y-%m-%d");
    match book_name {
        Some(name) => format!("gospel-getter-notes-{}-{date}.md", slug(name)),
        None => format!("gospel-getter-notes-{date}.md"),
    }
}

/// A book's name for a file name: lowercase, with a hyphen for each run of
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
        let books = books();
        let verses: HashMap<_, _> = chapters.iter().cloned().collect();
        NotesExport {
            notes,
            books: &books,
            verses: &verses,
            translation_code: code,
            translation_name: if code == "kjv" {
                "King James Version"
            } else {
                "World English Bible"
            },
            book,
            exported_at: at("2026-10-10T18:00:00.000Z"),
        }
        .render(&mountain())
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
            },
            Verse {
                verse: 2,
                text: "The same*was <in>\u{a0}the beginning.".to_string(),
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
    fn kjv_markup_is_shown_as_a_printed_kjv_would() {
        let notes = [note(1, 19, 23, (1, 3), "Shepherd.")];
        let chapter = vec![
            Verse {
                verse: 1,
                text: "[A Psalm of David.] The LORD {is} my shepherd; I shall not want."
                    .to_string(),
            },
            Verse {
                verse: 2,
                text: "He leadeth me beside the still waters. {green...: Heb. pastures of tender \
                       grass} {still...: Heb. waters of quietness}"
                    .to_string(),
            },
            Verse {
                verse: 3,
                text: "He restoreth my soul.".to_string(),
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
            book: Some(JOHN),
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
