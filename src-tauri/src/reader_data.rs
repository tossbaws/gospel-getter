//! The reader's own data — bookmarks, highlights, notes, last reading
//! position and display preferences — as a portable, human-readable JSON
//! file, for moving between installs or keeping a backup.
//!
//! The file never contains Bible text, cross-references, the search index,
//! database ids or anything else the app can rebuild from its bundled data.
//! Passages are canonical coordinates (book number 1–66, chapter, inclusive
//! verse range, or one verse for a highlight) and the reading position
//! names its translation by code, so a file imports into any install that
//! has those books and translations.
//!
//! Format 2 (from v2.5.0) adds `highlights`; format 1 files, which can't
//! have them, still import exactly as before.
//!
//! Format 3 (from v2.8.0) adds notes: `notes`, each a passage and the
//! reader's plain text about it. Format 1 and 2 files, which can't have
//! them, still import exactly as before and leave the reader's notes
//! alone.
//!
//! Display preferences are read leniently: one this version
//! doesn't know, or with a value it doesn't offer (a theme added later,
//! say), is skipped and named in `ReaderData::skipped_preferences`, and the
//! rest of the file imports. Everything else is checked strictly.
//!
//! This module is the format and its validation; reading and writing the
//! database is `db::reader_data`, and the commands (with the native file
//! pickers) are in `commands`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::db::notes::normalize_body;
use crate::db::{Book, HighlightColor, Translation};

/// The `format` every reader-data file declares.
pub const FORMAT: &str = "gospel-getter-reader-data";
/// The `format_version` this version writes. It reads this,
/// `FORMAT_VERSION_2` and `FORMAT_VERSION_1`; anything newer is refused,
/// since it may mean things this version would misread.
pub const FORMAT_VERSION: u64 = 3;
/// The second format: format 1 and highlights, but no notes.
pub const FORMAT_VERSION_2: u64 = 2;
/// The first format: bookmarks, reading position and preferences, but no
/// highlights or notes.
pub const FORMAT_VERSION_1: u64 = 1;
/// The suggested file-name suffix.
pub const FILE_SUFFIX: &str = ".gospel-getter.json";
/// The largest file import will read: 128 MiB. Every export imports
/// again (bookmarking, highlighting and merging an import have no limit,
/// so neither does importing), and this is the only limit on bookmarks
/// and highlights in a file: it refuses a file that can't be an export
/// without reading all of it. Worst cases, pretty-printed as
/// `to_file_contents` writes them, with the longest book name, the largest
/// book, chapter and verse numbers and every optional field present
/// (`a_maximal_file_fits_the_size_limit` builds one and checks every
/// figure here):
///
/// - a bookmark takes 188 bytes, a highlight 231;
/// - a note takes 254 bytes besides its text: 10,000 of them (`MAX_NOTES`),
///   2,540,000 bytes;
/// - the notes' text is at most `MAX_TOTAL_NOTE_CHARS` (25,000,000)
///   characters in all, of plain text (see `db::notes::normalize_body`),
///   and none of those takes more than 4 bytes in the file — 4 bytes of
///   UTF-8 unescaped, or 2 for `\n`, `\t`, `\"` and `\\`; the control
///   characters JSON would escape in 6, like `\u0001`, are refused — so
///   100,000,000 bytes;
/// - the rest of the file (its header, reading position and every
///   preference) takes 562 bytes.
///
/// A highlight is one verse, and the bundled translations number 31,109
/// verses between them, so at most 7,186,179 bytes of highlights. With
/// every verse highlighted and the notes at both their limits, a file of
/// 50,000 bookmarks is 119,126,741 bytes (113.6 MiB), and one of 130,271
/// fits. Without notes, every bookmark there can be — each of the 530,288
/// verse ranges within a chapter — and every highlight fit, in
/// 106,880,885 bytes (101.9 MiB). Only a reader with notes near both
/// their limits (25 million characters, every one 4 bytes long) and more
/// than 130,271 bookmarks could export a file too large to import.
///
/// Format 2's limit was 16 MiB; it rose because the reader's notes can
/// hold far more text than every other record together, and a backup of
/// them must always import. Checking a file of 128 MiB briefly holds a
/// few times that in memory, which a desktop has to spare.
pub const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
/// The most notes one file may hold: as many as the reader can keep.
pub const MAX_NOTES: usize = crate::db::notes::MAX_NOTES;
/// The most characters the notes in one file may hold together: as many
/// as the reader's notes can.
pub const MAX_TOTAL_NOTE_CHARS: usize = crate::db::notes::MAX_TOTAL_NOTE_CHARS;
/// How many problems an invalid file reports before summarizing the rest.
const MAX_REPORTED_PROBLEMS: usize = 12;

/// A whole reader-data file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReaderData {
    pub format: String,
    pub format_version: u64,
    /// When the file was written (UTC, ISO 8601). Informational.
    pub exported_at: String,
    /// The Gospel Getter version that wrote it. Informational.
    pub app_version: String,
    pub reading_position: Option<PositionRecord>,
    #[serde(default)]
    pub bookmarks: Vec<BookmarkRecord>,
    /// Format 2 only. Left out when there are none, so a format 1 file
    /// written back out stays a valid format 1 file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub highlights: Vec<HighlightRecord>,
    /// Format 3 only, and likewise left out when there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<NoteRecord>,
    #[serde(default)]
    pub preferences: Preferences,
    /// The display preferences in the file that `parse` skipped, each as
    /// the reader should see it named, e.g. `theme “neon”`. Never written.
    #[serde(skip)]
    pub skipped_preferences: Vec<String>,
}

/// The last chapter read, and in which translation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositionRecord {
    /// Translation code, e.g. `kjv`.
    pub translation: String,
    /// Canonical book number, Genesis = 1 … Revelation = 66.
    pub book: i64,
    /// The book's name, for people reading the file. If present on import,
    /// it must match `book`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub book_name: Option<String>,
    pub chapter: i64,
}

/// One bookmarked passage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BookmarkRecord {
    /// Canonical book number, Genesis = 1 … Revelation = 66.
    pub book: i64,
    /// The book's name, for people reading the file. If present on import,
    /// it must match `book`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub book_name: Option<String>,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    /// When it was bookmarked (UTC, ISO 8601). Kept on import; a bookmark
    /// without one is dated when it's imported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

/// One highlighted verse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HighlightRecord {
    /// Canonical book number, Genesis = 1 … Revelation = 66.
    pub book: i64,
    /// The book's name, for people reading the file. If present on import,
    /// it must match `book`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub book_name: Option<String>,
    pub chapter: i64,
    pub verse: i64,
    pub color: HighlightColor,
    /// When the verse was first highlighted, and when its color was last
    /// set (UTC, ISO 8601). Kept on import; missing ones are "now".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// One note: a passage and the reader's text about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteRecord {
    /// Canonical book number, Genesis = 1 … Revelation = 66.
    pub book: i64,
    /// The book's name, for people reading the file. If present on import,
    /// it must match `book`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub book_name: Option<String>,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    /// Plain text (see `db::notes::normalize_body`).
    pub body: String,
    /// When the note was written, and when its text was last changed (UTC,
    /// ISO 8601). Kept on import; missing ones are "now".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Display preferences. Each is optional: a file only changes the ones it
/// names. The values are the ones Aa offers and the page keeps: the enums
/// are the saved settings' one table's (`preferences`, shared with the
/// page's Rust pieces).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<Theme>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_size: Option<TextSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_spacing: Option<LineSpacing>,
    /// The reading column's width. Added in 2.13; older versions skip it
    /// (2.12) or refuse the file (2.11 and older, which also refuse 2.12's
    /// themes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<Width>,
    /// Verses one to a line, or flowing as prose. Added in 2.14; 2.12 and
    /// 2.13 skip it, with their notice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<Layout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reading_mode: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compare: Option<bool>,
    /// The translation compare shows beside the one being read, once the
    /// reader has chosen one. Added in 2.15; 2.12 to 2.14 skip it, with
    /// their notice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compare_with: Option<TranslationCode>,
}

impl Preferences {
    /// Every preference's name in the file.
    pub const NAMES: [&'static str; 8] = [
        "theme",
        "text_size",
        "line_spacing",
        "width",
        "layout",
        "reading_mode",
        "compare",
        "compare_with",
    ];

    /// How many preferences are set.
    pub fn count(&self) -> usize {
        [
            self.theme.is_some(),
            self.text_size.is_some(),
            self.line_spacing.is_some(),
            self.width.is_some(),
            self.layout.is_some(),
            self.reading_mode.is_some(),
            self.compare.is_some(),
            self.compare_with.is_some(),
        ]
        .into_iter()
        .filter(|&set| set)
        .count()
    }
}

/// A bundled translation's code ("kjv", "bsb", "web"). Any other is
/// refused, so a file naming one this version doesn't have skips it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct TranslationCode(String);

impl TranslationCode {
    /// `code`, if it's a bundled translation's.
    pub fn new(code: &str) -> Option<Self> {
        crate::db::seed::source(code).map(|t| Self(t.code.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for TranslationCode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = String::deserialize(deserializer)?;
        Self::new(&code).ok_or_else(|| {
            serde::de::Error::custom(format!("no translation \"{}\"", from_file(&code)))
        })
    }
}

pub use crate::preferences::{Layout, LineSpacing, TextSize, Theme, Width};

/// Why a file can't be imported, in words for the reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    TooLarge {
        bytes: u64,
    },
    NotJson(String),
    NotReaderData,
    UnsupportedVersion(String),
    Malformed(String),
    /// The file is well-formed, but these things in it don't fit this
    /// install's Bible (or aren't valid at all).
    Invalid(Vec<String>),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { bytes } => write!(
                f,
                "The file is too large to be a Gospel Getter export ({} KB; the limit is {} KB).",
                bytes / 1024,
                MAX_FILE_BYTES / 1024
            ),
            Self::NotJson(detail) => write!(f, "The file isn't valid JSON ({detail})."),
            Self::NotReaderData => {
                write!(
                    f,
                    "This isn't a Gospel Getter file of bookmarks, highlights, notes and settings."
                )
            }
            Self::UnsupportedVersion(message) | Self::Malformed(message) => f.write_str(message),
            Self::Invalid(problems) => write!(
                f,
                "The file can't be imported: {} {} found.",
                problems.len(),
                if problems.len() == 1 {
                    "problem was"
                } else {
                    "problems were"
                }
            ),
        }
    }
}

impl std::error::Error for ImportError {}

impl ImportError {
    /// The individual problems, for an `Invalid` file.
    pub fn problems(&self) -> &[String] {
        match self {
            Self::Invalid(problems) => problems,
            _ => &[],
        }
    }
}

/// Parse a file's bytes: size, JSON, format and version first (so a newer
/// file gets a clear "newer version" message rather than a confusing
/// field error), then the full structure.
pub fn parse(bytes: &[u8]) -> Result<ReaderData, ImportError> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(ImportError::TooLarge {
            bytes: bytes.len() as u64,
        });
    }
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|e| ImportError::NotJson(format!("line {}, column {}", e.line(), e.column())))?;
    if value.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(ImportError::NotReaderData);
    }
    match value.get("format_version").and_then(Value::as_u64) {
        Some(FORMAT_VERSION) => {}
        // A format 1 or 2 file is read exactly as it always was: one that
        // names highlights (format 1) or notes has a field its format
        // doesn't define.
        Some(old @ (FORMAT_VERSION_1 | FORMAT_VERSION_2)) => {
            for (field, since) in [("highlights", FORMAT_VERSION_2), ("notes", FORMAT_VERSION)] {
                if old < since && value.get(field).is_some() {
                    return Err(ImportError::Malformed(format!(
                        "The file doesn't have the expected layout: unknown field `{field}` \
                         (data format {old} has no {field})."
                    )));
                }
            }
        }
        Some(newer) if newer > FORMAT_VERSION => {
            return Err(ImportError::UnsupportedVersion(format!(
                "This file was made by a newer version of Gospel Getter (data format \
                 {newer}). This version reads format {FORMAT_VERSION}; update Gospel Getter \
                 to import it."
            )));
        }
        _ => {
            return Err(ImportError::UnsupportedVersion(format!(
                "This file's data format version isn't one this version of Gospel Getter \
                 reads (it reads formats {FORMAT_VERSION_1} to {FORMAT_VERSION})."
            )));
        }
    }
    let mut value = value;
    let skipped_preferences = value
        .get_mut("preferences")
        .map(drop_unknown_preferences)
        .unwrap_or_default();
    let mut data: ReaderData = serde_json::from_value(value).map_err(|e| {
        ImportError::Malformed(format!(
            "The file doesn't have the expected layout: {}.",
            shorten_quoted(&e.to_string())
        ))
    })?;
    data.skipped_preferences = skipped_preferences;
    Ok(data)
}

/// The longest a skipped preference's name or value is shown, in
/// characters, before it's cut short with "…".
const MAX_SHOWN_PREFERENCE_CHARS: usize = 40;

/// The longest any other string from the file (a book name, a translation
/// code, a value the layout check quotes) is shown in a problem message, in
/// characters, before it's cut short with "…".
pub const MAX_SHOWN_FILE_CHARS: usize = 80;

/// The longest the layout check's whole message is shown, in characters,
/// even after each string in it is cut short (a string can be full of the
/// quote marks the cutting goes by).
const MAX_LAYOUT_DETAIL_CHARS: usize = 400;

/// `text`, or, if it's longer than `max` characters, its first `max - 1`
/// and "…".
fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        let cut: String = text.chars().take(max - 1).collect();
        format!("{cut}…")
    } else {
        text.to_owned()
    }
}

/// A string from the file, as a problem message shows it.
fn from_file(text: &str) -> String {
    shorten(text, MAX_SHOWN_FILE_CHARS)
}

/// serde's description of what's wrong with the file's layout, with each
/// string it quotes from the file (an `unknown variant` or field in
/// backticks, a `string "…"` in Rust's escaped quotes) cut short, then the
/// whole message.
fn shorten_quoted(message: &str) -> String {
    let mut out = String::new();
    let mut chars = message.chars();
    while let Some(open) = chars.next() {
        out.push(open);
        if open != '`' && open != '"' {
            continue;
        }
        // Up to the matching mark (in "…", one not escaped with \).
        let mut quoted = String::new();
        let mut closed = false;
        let mut escaped = false;
        for q in chars.by_ref() {
            if q == open && !escaped {
                closed = true;
                break;
            }
            escaped = open == '"' && q == '\\' && !escaped;
            quoted.push(q);
        }
        out.push_str(&from_file(&quoted));
        if closed {
            out.push(open);
        }
    }
    shorten(&out, MAX_LAYOUT_DETAIL_CHARS)
}

/// Removes from a file's `preferences` object every entry this version
/// can't apply: a name it doesn't have, or a value it doesn't offer for
/// that name. Returns them as the reader should see them named: the
/// settings this version has first, in Aa's order, then any others by
/// name. Anything but an object is left for the structural check to
/// refuse.
fn drop_unknown_preferences(preferences: &mut Value) -> Vec<String> {
    let Some(entries) = preferences.as_object_mut() else {
        return Vec::new();
    };
    let mut skipped = Vec::new();
    entries.retain(|name, value| {
        let single = Value::Object([(name.clone(), value.clone())].into_iter().collect());
        if serde_json::from_value::<Preferences>(single).is_ok() {
            return true;
        }
        let known = Preferences::NAMES.iter().position(|n| n == name);
        skipped.push((
            known.unwrap_or(Preferences::NAMES.len()),
            name.clone(),
            describe_skipped_preference(name, value),
        ));
        false
    });
    skipped.sort();
    skipped.into_iter().map(|(_, _, shown)| shown).collect()
}

/// `theme “neon”`, `text size “huge”` or `reading mode 3` for a value this
/// version doesn't offer; `“font” (not a setting in this version)` for a
/// name it doesn't have.
fn describe_skipped_preference(name: &str, value: &Value) -> String {
    let shown = |text: &str| shorten(text, MAX_SHOWN_PREFERENCE_CHARS);
    if !Preferences::NAMES.contains(&name) {
        return format!("“{}” (not a setting in this version)", shown(name));
    }
    let label = name.replace('_', " ");
    match value {
        Value::String(text) => format!("{label} “{}”", shown(text)),
        other => format!("{label} {}", shown(&other.to_string())),
    }
}

/// The notice for the reader when an import skipped display preferences,
/// naming each one; `None` when it skipped none.
pub fn skipped_preferences_notice(skipped: &[String]) -> Option<String> {
    match skipped {
        [] => None,
        [one] => Some(format!(
            "Skipped a display setting this version of Gospel Getter doesn’t know: {one}."
        )),
        many => Some(format!(
            "Skipped {} display settings this version of Gospel Getter doesn’t know: {}.",
            many.len(),
            many.join(", ")
        )),
    }
}

/// Serialize for writing: pretty-printed, with a trailing newline. A
/// serialization failure is returned, never written as an empty file.
pub fn to_file_contents(data: &ReaderData) -> Result<String, serde_json::Error> {
    pretty_json_line(data)
}

fn pretty_json_line<T: Serialize + ?Sized>(value: &T) -> Result<String, serde_json::Error> {
    let mut out = serde_json::to_string_pretty(value)?;
    out.push('\n');
    Ok(out)
}

/// A bookmark ready to insert, after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBookmark {
    pub book_id: i64,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    /// Normalized to the database's own format; `None` means "now".
    pub created_at: Option<String>,
}

impl NewBookmark {
    pub fn coordinates(&self) -> (i64, i64, i64, i64) {
        (self.book_id, self.chapter, self.verse_start, self.verse_end)
    }
}

/// A highlight ready to save, after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewHighlight {
    pub book_id: i64,
    pub chapter: i64,
    pub verse: i64,
    pub color: HighlightColor,
    /// Normalized to the database's own format; `None` means "now".
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

impl NewHighlight {
    pub fn coordinates(&self) -> (i64, i64, i64) {
        (self.book_id, self.chapter, self.verse)
    }
}

/// The passage and text of a note: two notes with the same key are the
/// same note, for importing.
pub type NoteKey = (i64, i64, i64, i64, String);

/// A note ready to save, after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewNote {
    pub book_id: i64,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    /// Normalized, as it's stored.
    pub body: String,
    /// Normalized to the database's own format; `None` means "now".
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

impl NewNote {
    pub fn key(&self) -> NoteKey {
        (
            self.book_id,
            self.chapter,
            self.verse_start,
            self.verse_end,
            self.body.clone(),
        )
    }
}

/// A reading position ready to save, after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPosition {
    pub translation_id: i64,
    pub translation_code: String,
    pub translation_name: String,
    pub book_id: i64,
    pub book_name: String,
    pub chapter: i64,
}

/// A file that has passed every check against this install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedImport {
    pub exported_at: String,
    pub app_version: String,
    /// De-duplicated, in the file's order.
    pub bookmarks: Vec<NewBookmark>,
    /// How many bookmarks in the file repeated another one in it.
    pub duplicates_in_file: usize,
    /// De-duplicated, in the file's order. `None` for a format 1 file,
    /// which can't hold highlights, so importing it leaves them alone.
    pub highlights: Option<Vec<NewHighlight>>,
    /// How many highlights in the file repeated a verse already in it.
    pub highlight_duplicates_in_file: usize,
    /// De-duplicated, in the file's order. `None` for a format 1 or 2 file,
    /// which can't hold notes, so importing it leaves them alone.
    pub notes: Option<Vec<NewNote>>,
    /// How many notes in the file repeated another one in it (the same
    /// passage and text).
    pub note_duplicates_in_file: usize,
    pub position: Option<NewPosition>,
    pub preferences: Preferences,
    /// The file's display preferences that were skipped (see
    /// `ReaderData::skipped_preferences`).
    pub skipped_preferences: Vec<String>,
}

/// Check one passage in a file (a bookmark's or a note's, labelled
/// `label`) against this install's books and verse numbering. Returns the
/// book and the passage's reference, e.g. "John 3:16–18", or the problem.
fn check_passage_record<'a>(
    label: &str,
    (book_number, book_name, chapter, verse_start, verse_end): (
        i64,
        &Option<String>,
        i64,
        i64,
        i64,
    ),
    books: &'a [Book],
    last_verses: &HashMap<(i64, i64), i64>,
) -> Result<(&'a Book, String), String> {
    let Some(found) = books.iter().find(|b| b.id == book_number) else {
        return Err(format!("{label}: there's no book number {book_number}."));
    };
    if !name_matches(found, book_name) {
        return Err(format!(
            "{label}: book {book_number} is {}, not {}.",
            found.name,
            from_file(book_name.as_deref().unwrap_or_default())
        ));
    }
    if !(1..=found.chapter_count).contains(&chapter) {
        return Err(format!(
            "{label}: there's no {} {chapter} ({} has {} chapters).",
            found.name, found.name, found.chapter_count
        ));
    }
    let reference = format!(
        "{} {chapter}:{verse_start}{}",
        found.name,
        if verse_end == verse_start {
            String::new()
        } else {
            format!("\u{2013}{verse_end}")
        }
    );
    let last = last_verses.get(&(found.id, chapter)).copied().unwrap_or(0);
    if verse_start < 1 || verse_end < verse_start {
        return Err(format!("{label}: {reference} isn't a valid verse range."));
    }
    if verse_end > last {
        return Err(format!(
            "{label}: there's no {reference}; {} {chapter} ends at verse {last}.",
            found.name
        ));
    }
    Ok((found, reference))
}

/// Whether a file's optional book name (if any) names `book`.
fn name_matches(book: &Book, name: &Option<String>) -> bool {
    name.as_deref()
        .is_none_or(|n| n.trim().eq_ignore_ascii_case(&book.name))
}

/// Check every passage and the reading position against this install's
/// books, translations and verse numbering (`last_verses` maps
/// `(book, chapter)` to the highest verse number any installed
/// translation has, the same rule bookmarking uses). Either everything is
/// valid, or nothing is imported and every problem (up to a limit) is
/// listed.
pub fn validate(
    data: ReaderData,
    books: &[Book],
    translations: &[Translation],
    last_verses: &HashMap<(i64, i64), i64>,
) -> Result<ValidatedImport, ImportError> {
    let mut problems = Vec::new();
    let book = |id: i64| books.iter().find(|b| b.id == id);

    if data.notes.len() > MAX_NOTES {
        return Err(ImportError::Invalid(vec![format!(
            "It has {} notes; the most one file can hold is {MAX_NOTES}.",
            data.notes.len()
        )]));
    }

    let mut bookmarks: Vec<NewBookmark> = Vec::new();
    let mut index_of: HashMap<(i64, i64, i64, i64), usize> = HashMap::new();
    let mut duplicates_in_file = 0;
    for (i, b) in data.bookmarks.into_iter().enumerate() {
        let label = format!("Bookmark {}", i + 1);
        let (found, reference) = match check_passage_record(
            &label,
            (b.book, &b.book_name, b.chapter, b.verse_start, b.verse_end),
            books,
            last_verses,
        ) {
            Ok(checked) => checked,
            Err(problem) => {
                problems.push(problem);
                continue;
            }
        };
        let created_at = match b.created_at.as_deref().map(normalize_timestamp) {
            None => None,
            Some(Some(t)) => Some(t),
            Some(None) => {
                problems.push(format!(
                    "{label} ({reference}): its date isn't a valid UTC time like \
                     2026-10-04T12:30:00Z."
                ));
                continue;
            }
        };
        let new = NewBookmark {
            book_id: found.id,
            chapter: b.chapter,
            verse_start: b.verse_start,
            verse_end: b.verse_end,
            created_at,
        };
        // The same passage twice: keep one, with the earliest date.
        match index_of.get(&new.coordinates()) {
            Some(&j) => {
                duplicates_in_file += 1;
                let kept = &mut bookmarks[j];
                if let Some(t) = new.created_at
                    && kept.created_at.as_ref().is_none_or(|k| &t < k)
                {
                    kept.created_at = Some(t);
                }
            }
            None => {
                index_of.insert(new.coordinates(), bookmarks.len());
                bookmarks.push(new);
            }
        }
    }

    let mut highlights: Vec<NewHighlight> = Vec::new();
    let mut highlighted: HashSet<(i64, i64, i64)> = HashSet::new();
    let mut highlight_duplicates_in_file = 0;
    for (i, h) in data.highlights.into_iter().enumerate() {
        let label = format!("Highlight {}", i + 1);
        let Some(found) = book(h.book) else {
            problems.push(format!("{label}: there's no book number {}.", h.book));
            continue;
        };
        if !name_matches(found, &h.book_name) {
            problems.push(format!(
                "{label}: book {} is {}, not {}.",
                h.book,
                found.name,
                from_file(h.book_name.as_deref().unwrap_or_default())
            ));
            continue;
        }
        if !(1..=found.chapter_count).contains(&h.chapter) {
            problems.push(format!(
                "{label}: there's no {} {} ({} has {} chapters).",
                found.name, h.chapter, found.name, found.chapter_count
            ));
            continue;
        }
        let reference = format!("{} {}:{}", found.name, h.chapter, h.verse);
        let last = last_verses
            .get(&(found.id, h.chapter))
            .copied()
            .unwrap_or(0);
        if h.verse < 1 {
            problems.push(format!("{label}: {reference} isn't a valid verse."));
            continue;
        }
        if h.verse > last {
            problems.push(format!(
                "{label}: there's no {reference}; {} {} ends at verse {last}.",
                found.name, h.chapter
            ));
            continue;
        }
        let created_at = h.created_at.as_deref().map(normalize_timestamp);
        let updated_at = h.updated_at.as_deref().map(normalize_timestamp);
        if matches!(created_at, Some(None)) || matches!(updated_at, Some(None)) {
            problems.push(format!(
                "{label} ({reference}): its date isn't a valid UTC time like \
                 2026-10-04T12:30:00Z."
            ));
            continue;
        }
        let new = NewHighlight {
            book_id: found.id,
            chapter: h.chapter,
            verse: h.verse,
            color: h.color,
            created_at: created_at.flatten(),
            updated_at: updated_at.flatten(),
        };
        // The same verse twice: the first one counts.
        if highlighted.insert(new.coordinates()) {
            highlights.push(new);
        } else {
            highlight_duplicates_in_file += 1;
        }
    }

    let mut notes: Vec<NewNote> = Vec::new();
    let mut noted: HashSet<NoteKey> = HashSet::new();
    let mut note_duplicates_in_file = 0;
    for (i, n) in data.notes.into_iter().enumerate() {
        let label = format!("Note {}", i + 1);
        let (found, reference) = match check_passage_record(
            &label,
            (n.book, &n.book_name, n.chapter, n.verse_start, n.verse_end),
            books,
            last_verses,
        ) {
            Ok(checked) => checked,
            Err(problem) => {
                problems.push(problem);
                continue;
            }
        };
        let body = match normalize_body(&n.body) {
            Ok(body) => body,
            Err(e) => {
                problems.push(format!("{label} ({reference}): {e}"));
                continue;
            }
        };
        let created_at = n.created_at.as_deref().map(normalize_timestamp);
        let updated_at = n.updated_at.as_deref().map(normalize_timestamp);
        if matches!(created_at, Some(None)) || matches!(updated_at, Some(None)) {
            problems.push(format!(
                "{label} ({reference}): its date isn't a valid UTC time like \
                 2026-10-04T12:30:00Z."
            ));
            continue;
        }
        let new = NewNote {
            book_id: found.id,
            chapter: n.chapter,
            verse_start: n.verse_start,
            verse_end: n.verse_end,
            body,
            created_at: created_at.flatten(),
            updated_at: updated_at.flatten(),
        };
        // The same note (passage and text) twice: the first one counts.
        if noted.insert(new.key()) {
            notes.push(new);
        } else {
            note_duplicates_in_file += 1;
        }
    }

    let note_chars: usize = notes.iter().map(|n| n.body.chars().count()).sum();
    if note_chars > MAX_TOTAL_NOTE_CHARS {
        problems.push(format!(
            "Its notes hold {note_chars} characters in all; the most one file can hold is \
             {MAX_TOTAL_NOTE_CHARS}."
        ));
    }

    let position = match data.reading_position {
        None => None,
        Some(p) => {
            let translation = translations
                .iter()
                .find(|t| t.code.eq_ignore_ascii_case(p.translation.trim()));
            match (translation, book(p.book)) {
                (None, _) => {
                    let codes: Vec<&str> = translations.iter().map(|t| t.code.as_str()).collect();
                    problems.push(format!(
                        "Reading position: this install has no \u{201c}{}\u{201d} translation \
                         (it has {}).",
                        from_file(&p.translation),
                        codes.join(", ")
                    ));
                    None
                }
                (_, None) => {
                    problems.push(format!(
                        "Reading position: there's no book number {}.",
                        p.book
                    ));
                    None
                }
                (Some(t), Some(found)) => {
                    if !name_matches(found, &p.book_name) {
                        problems.push(format!(
                            "Reading position: book {} is {}, not {}.",
                            p.book,
                            found.name,
                            from_file(p.book_name.as_deref().unwrap_or_default())
                        ));
                        None
                    } else if !(1..=found.chapter_count).contains(&p.chapter) {
                        problems.push(format!(
                            "Reading position: there's no {} {} ({} has {} chapters).",
                            found.name, p.chapter, found.name, found.chapter_count
                        ));
                        None
                    } else {
                        Some(NewPosition {
                            translation_id: t.id,
                            translation_code: t.code.clone(),
                            translation_name: t.name.clone(),
                            book_id: found.id,
                            book_name: found.name.clone(),
                            chapter: p.chapter,
                        })
                    }
                }
            }
        }
    };

    if !problems.is_empty() {
        let extra = problems.len().saturating_sub(MAX_REPORTED_PROBLEMS);
        problems.truncate(MAX_REPORTED_PROBLEMS);
        if extra > 0 {
            problems.push(format!("…and {extra} more."));
        }
        return Err(ImportError::Invalid(problems));
    }

    Ok(ValidatedImport {
        exported_at: data.exported_at,
        app_version: data.app_version,
        bookmarks,
        duplicates_in_file,
        highlights: (data.format_version != FORMAT_VERSION_1).then_some(highlights),
        highlight_duplicates_in_file,
        notes: (data.format_version >= FORMAT_VERSION).then_some(notes),
        note_duplicates_in_file,
        position,
        preferences: data.preferences,
        skipped_preferences: data.skipped_preferences,
    })
}

/// A UTC timestamp `YYYY-MM-DDTHH:MM:SS[.fraction]Z`, rewritten in the
/// database's own form (`…SS.mmmZ`, so bookmarks still sort by date), or
/// `None` if it isn't one.
pub fn normalize_timestamp(input: &str) -> Option<String> {
    let s = input.trim();
    let body = s.strip_suffix('Z')?;
    let (main, fraction) = match body.split_once('.') {
        Some((main, fraction)) => (main, fraction),
        None => (body, ""),
    };
    let b = main.as_bytes();
    if b.len() != 19
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let num = |range: std::ops::Range<usize>| -> Option<u32> {
        let part = &main[range];
        part.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| part.parse().ok())?
    };
    let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hour, minute, second) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => return None,
    };
    if year < 1970 || day == 0 || day > days_in_month || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    if fraction.len() > 9 || !fraction.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let millis = format!("{fraction:0<3}");
    Some(format!("{main}.{}Z", &millis[..3]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names<T: Serialize>(values: &[T]) -> Vec<String> {
        values
            .iter()
            .map(|v| {
                serde_json::to_value(v)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect()
    }

    fn values(choices: &[crate::preferences::Choice]) -> Vec<String> {
        choices.iter().map(|c| c.value.to_string()).collect()
    }

    /// Each enum is written in a file as the value Aa offers and the page
    /// saves: its choice in the one table, in order.
    #[test]
    fn preference_values_match_what_aa_offers() {
        use LineSpacing as L;
        use TextSize as S;
        use Theme as T;
        assert_eq!(
            names(&[
                T::ChiRho,
                T::Vellum,
                T::System,
                T::Vaporwave,
                T::ClassicDark,
                T::ClassicLight,
                T::Matrix,
                T::BeastSlayer,
                T::HotPink
            ]),
            values(Theme::CHOICES)
        );
        assert_eq!(
            names(&[S::Small, S::Medium, S::Large, S::XLarge]),
            values(TextSize::CHOICES)
        );
        assert_eq!(
            names(&[L::Compact, L::Normal, L::Relaxed]),
            values(LineSpacing::CHOICES)
        );
        assert_eq!(
            names(&[Width::Narrow, Width::Normal, Width::Wide]),
            values(Width::CHOICES)
        );
        assert_eq!(
            names(&[Layout::Verses, Layout::Flowing]),
            values(Layout::CHOICES)
        );
        // And a file's preferences are the table's exported ones, by the
        // same names, in the same order.
        let exported: Vec<&str> = crate::preferences::PREFERENCES
            .iter()
            .filter_map(|p| p.file_name)
            .collect();
        assert_eq!(exported, Preferences::NAMES);
    }

    #[test]
    fn the_layout_checks_quotes_from_the_file_are_cut_short() {
        let long = "y".repeat(200);
        let cut = format!("{}…", "y".repeat(MAX_SHOWN_FILE_CHARS - 1));
        // Short quotes, and the words around them, are left as they are.
        let short = "unknown variant `teal`, expected one of `yellow`, `green`";
        assert_eq!(shorten_quoted(short), short);
        assert_eq!(
            shorten_quoted(&format!("unknown field `{long}`, expected `book`")),
            format!("unknown field `{cut}`, expected `book`")
        );
        // Rust's escaped quotes: an escaped " doesn't end the string.
        assert_eq!(
            shorten_quoted(&format!(
                r#"invalid type: string "a\"{long}", expected i64"#
            )),
            format!(
                r#"invalid type: string "a\"{}…", expected i64"#,
                "y".repeat(MAX_SHOWN_FILE_CHARS - 4)
            )
        );
        assert_eq!(
            shorten_quoted(r#"string "a\\", then `b`"#),
            r#"string "a\\", then `b`"#
        );
        // A quote that never closes isn't closed for it.
        assert_eq!(
            shorten_quoted(&format!("unknown variant `{long}")),
            format!("unknown variant `{cut}")
        );
        // However the quotes fall, the whole message is bounded.
        let many = format!("unknown variant `{}`", "y`".repeat(5_000));
        assert_eq!(
            shorten_quoted(&many).chars().count(),
            MAX_LAYOUT_DETAIL_CHARS
        );
    }

    #[test]
    fn timestamps_are_checked_and_normalized() {
        for (input, expected) in [
            ("2026-10-04T12:30:05Z", Some("2026-10-04T12:30:05.000Z")),
            ("2026-10-04T12:30:05.1Z", Some("2026-10-04T12:30:05.100Z")),
            (
                "2026-10-04T12:30:05.123456Z",
                Some("2026-10-04T12:30:05.123Z"),
            ),
            (" 2024-02-29T00:00:00Z ", Some("2024-02-29T00:00:00.000Z")),
            ("2023-02-29T00:00:00Z", None),
            ("2026-13-01T00:00:00Z", None),
            ("2026-10-04T24:00:00Z", None),
            ("2026-10-04 12:30:05Z", None),
            ("2026-10-04T12:30:05", None),
            ("2026-10-04T12:30:05+02:00", None),
            ("2026-10-04T12:30:05.Z", Some("2026-10-04T12:30:05.000Z")),
            ("2026-10-04T12:30:05.1x3Z", None),
            ("yesterday", None),
            ("", None),
        ] {
            assert_eq!(normalize_timestamp(input).as_deref(), expected, "{input:?}");
        }
    }

    #[test]
    fn parse_checks_size_json_format_and_version_first() {
        let too_big = vec![b' '; MAX_FILE_BYTES as usize + 1];
        assert!(matches!(parse(&too_big), Err(ImportError::TooLarge { .. })));
        assert!(matches!(parse(b"{not json"), Err(ImportError::NotJson(_))));
        assert_eq!(parse(b"[]"), Err(ImportError::NotReaderData));
        assert_eq!(
            parse(br#"{"format": "something-else", "format_version": 1}"#),
            Err(ImportError::NotReaderData)
        );
        let newer = parse(
            br#"{"format": "gospel-getter-reader-data", "format_version": 4, "brand_new": true}"#,
        )
        .unwrap_err();
        assert_eq!(
            newer,
            ImportError::UnsupportedVersion(
                "This file was made by a newer version of Gospel Getter (data format 4). This \
                 version reads format 3; update Gospel Getter to import it."
                    .to_string()
            )
        );
        // Format 1 never had highlights: a format 1 file naming them is
        // refused, as format 1 always refused fields it doesn't define.
        let v1_with_highlights = parse(
            br#"{"format": "gospel-getter-reader-data", "format_version": 1, "exported_at": "x",
                 "app_version": "2.4.0", "reading_position": null, "highlights": []}"#,
        )
        .unwrap_err();
        assert!(
            matches!(v1_with_highlights, ImportError::Malformed(_)),
            "{v1_with_highlights:?}"
        );
        // Nor did format 1 or 2 have notes.
        for version in [1, 2] {
            let with_notes = parse(
                format!(
                    r#"{{"format": "gospel-getter-reader-data", "format_version": {version},
                         "exported_at": "x", "app_version": "2.7.0", "reading_position": null,
                         "notes": []}}"#
                )
                .as_bytes(),
            )
            .unwrap_err();
            assert_eq!(
                with_notes,
                ImportError::Malformed(format!(
                    "The file doesn't have the expected layout: unknown field `notes` (data \
                     format {version} has no notes)."
                ))
            );
        }
        let bad_color = parse(
            br#"{"format": "gospel-getter-reader-data", "format_version": 2, "exported_at": "x",
                 "app_version": "2.5.0", "reading_position": null,
                 "highlights": [{"book": 43, "chapter": 3, "verse": 16, "color": "purple"}]}"#,
        )
        .unwrap_err();
        assert!(
            matches!(bad_color, ImportError::Malformed(_))
                && bad_color.to_string().contains("purple"),
            "{bad_color}"
        );
        for version in ["0", "\"1\"", "\"2\"", "1.5", "-1", "null"] {
            let file = format!(
                r#"{{"format": "gospel-getter-reader-data", "format_version": {version}}}"#
            );
            assert!(
                matches!(
                    parse(file.as_bytes()),
                    Err(ImportError::UnsupportedVersion(_))
                ),
                "version {version}"
            );
        }
        let unknown = parse(
            br#"{"format": "gospel-getter-reader-data", "format_version": 1, "exported_at": "x",
                 "app_version": "2.2.0", "reading_position": null, "verses": []}"#,
        )
        .unwrap_err();
        assert!(matches!(unknown, ImportError::Malformed(_)), "{unknown:?}");
        // A theme this version doesn't have is skipped, not a reason to
        // refuse the file (see `unknown_preferences_are_skipped_and_the_rest_imports`).
        let bad_theme = parse(
            br#"{"format": "gospel-getter-reader-data", "format_version": 1, "exported_at": "x",
                 "app_version": "2.2.0", "reading_position": null, "preferences": {"theme": "neon"}}"#,
        )
        .unwrap();
        assert_eq!(bad_theme.preferences, Preferences::default());
        assert_eq!(bad_theme.skipped_preferences, ["theme “neon”"]);
        // But `preferences` itself must still be an object.
        let not_an_object = parse(
            br#"{"format": "gospel-getter-reader-data", "format_version": 1, "exported_at": "x",
                 "app_version": "2.2.0", "reading_position": null, "preferences": "neon"}"#,
        )
        .unwrap_err();
        assert!(
            matches!(not_an_object, ImportError::Malformed(_)),
            "{not_an_object:?}"
        );
    }

    /// Compare's partner is a bundled translation's code: one this version
    /// has round-trips, any other (a translation from a later version, or
    /// none at all) is skipped with the notice, as 2.12 to 2.14 skip the
    /// setting itself.
    #[test]
    fn compare_with_is_a_bundled_translation_or_skipped() {
        let file = |preferences: &str| {
            format!(
                r#"{{"format": "gospel-getter-reader-data", "format_version": 3,
                     "exported_at": "x", "app_version": "9.0.0", "reading_position": null,
                     "bookmarks": [], "preferences": {preferences}}}"#
            )
        };
        for code in ["kjv", "bsb", "web"] {
            let data = parse(file(&format!(r#"{{"compare_with": "{code}"}}"#)).as_bytes()).unwrap();
            assert_eq!(
                data.preferences
                    .compare_with
                    .as_ref()
                    .map(TranslationCode::as_str),
                Some(code)
            );
            assert!(data.skipped_preferences.is_empty());
            let written = serde_json::to_value(&data.preferences).unwrap();
            assert_eq!(written, serde_json::json!({ "compare_with": code }));
        }
        for (value, shown) in [
            (r#""niv""#, "compare with “niv”"),
            (r#""KJV""#, "compare with “KJV”"),
            ("3", "compare with 3"),
        ] {
            let data =
                parse(file(&format!(r#"{{"compare_with": {value}, "compare": true}}"#)).as_bytes())
                    .unwrap();
            assert_eq!(data.preferences.compare_with, None);
            assert_eq!(data.preferences.compare, Some(true), "the rest imports");
            assert_eq!(data.skipped_preferences, [shown]);
        }
    }

    /// A file from a later version, with display settings this one doesn't
    /// know, imports everything else: each unknown or invalid preference is
    /// skipped and named, and the valid ones still apply. Bookmarks,
    /// highlights and notes stay strict.
    #[test]
    fn unknown_preferences_are_skipped_and_the_rest_imports() {
        let file = |preferences: &str, bookmarks: &str| {
            format!(
                r#"{{"format": "gospel-getter-reader-data", "format_version": 3,
                     "exported_at": "x", "app_version": "9.0.0", "reading_position": null,
                     "bookmarks": {bookmarks}, "preferences": {preferences}}}"#
            )
        };
        let bookmark = r#"[{"book": 43, "chapter": 3, "verse_start": 16, "verse_end": 16}]"#;
        let data = parse(
            file(
                r#"{"theme": "aurora", "text_size": "huge", "line_spacing": "relaxed",
                    "reading_mode": "yes", "compare": true, "font": "serif",
                    "accent": {"hue": 40}}"#,
                bookmark,
            )
            .as_bytes(),
        )
        .unwrap();
        assert_eq!(
            data.preferences,
            Preferences {
                line_spacing: Some(LineSpacing::Relaxed),
                compare: Some(true),
                ..Preferences::default()
            }
        );
        assert_eq!(
            data.skipped_preferences,
            [
                "theme “aurora”",
                "text size “huge”",
                "reading mode “yes”",
                "“accent” (not a setting in this version)",
                "“font” (not a setting in this version)",
            ]
        );
        assert_eq!(data.bookmarks.len(), 1);
        assert_eq!(
            skipped_preferences_notice(&data.skipped_preferences).unwrap(),
            "Skipped 5 display settings this version of Gospel Getter doesn’t know: \
             theme “aurora”, text size “huge”, reading mode “yes”, \
             “accent” (not a setting in this version), “font” (not a setting in this version)."
        );

        // Numbers, nulls and long values: null is "not set", the others are
        // named, and anything long is cut short.
        let long = "x".repeat(100);
        let data = parse(
            file(
                &format!(r#"{{"theme": null, "text_size": 3, "compare": "{long}"}}"#),
                "[]",
            )
            .as_bytes(),
        )
        .unwrap();
        assert_eq!(data.preferences, Preferences::default());
        assert_eq!(
            data.skipped_preferences,
            [
                "text size 3".to_string(),
                format!("compare “{}…”", "x".repeat(39))
            ]
        );
        assert_eq!(
            skipped_preferences_notice(&data.skipped_preferences[..1]).unwrap(),
            "Skipped a display setting this version of Gospel Getter doesn’t know: text size 3."
        );
        assert_eq!(skipped_preferences_notice(&[]), None);

        // Only preferences are lenient: a bad bookmark still refuses the file.
        let bad_bookmark = parse(
            file(
                r#"{"theme": "aurora"}"#,
                r#"[{"book": 43, "chapter": 3, "verse": 16}]"#,
            )
            .as_bytes(),
        )
        .unwrap_err();
        assert!(
            matches!(bad_bookmark, ImportError::Malformed(_)),
            "{bad_bookmark:?}"
        );
        // And so does an unknown field anywhere else.
        let unknown = parse(
            file("{}", "[]")
                .replace("\"bookmarks\"", "\"bookmark\"")
                .as_bytes(),
        )
        .unwrap_err();
        assert!(matches!(unknown, ImportError::Malformed(_)), "{unknown:?}");
    }

    /// `Preferences::NAMES` is exactly the file's preference names, in Aa's
    /// order.
    #[test]
    fn preference_names_match_the_file() {
        let all = Preferences {
            theme: Some(Theme::ChiRho),
            text_size: Some(TextSize::Small),
            line_spacing: Some(LineSpacing::Normal),
            width: Some(Width::Wide),
            layout: Some(Layout::Flowing),
            reading_mode: Some(true),
            compare: Some(false),
            compare_with: TranslationCode::new("bsb"),
        };
        let value = serde_json::to_value(all).unwrap();
        let mut names: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let mut expected = Preferences::NAMES.to_vec();
        names.sort_unstable();
        expected.sort_unstable();
        assert_eq!(names, expected);
    }

    /// The themes added after 2.11 import, and the older ones still do.
    #[test]
    fn the_brand_themes_and_system_import() {
        let with_theme = |theme: &str| {
            parse(
                format!(
                    r#"{{"format": "gospel-getter-reader-data", "format_version": 3,
                         "exported_at": "x", "app_version": "2.11.0", "reading_position": null,
                         "bookmarks": [], "highlights": [], "notes": [],
                         "preferences": {{"theme": "{theme}"}}}}"#
                )
                .as_bytes(),
            )
            .unwrap_or_else(|e| panic!("{theme}: {e}"))
            .preferences
            .theme
        };
        assert_eq!(with_theme("chi-rho"), Some(Theme::ChiRho));
        assert_eq!(with_theme("vellum"), Some(Theme::Vellum));
        assert_eq!(with_theme("system"), Some(Theme::System));
        assert_eq!(with_theme("vaporwave"), Some(Theme::Vaporwave));
    }

    #[test]
    fn serialization_failures_are_reported_not_written_as_empty_files() {
        struct Unserializable;
        impl Serialize for Unserializable {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("cannot be represented"))
            }
        }
        let error = pretty_json_line(&Unserializable).unwrap_err();
        assert!(error.to_string().contains("cannot be represented"));

        let data = parse(
            br#"{"format": "gospel-getter-reader-data", "format_version": 1,
                 "exported_at": "2026-10-04T00:00:00.000Z", "app_version": "2.2.0",
                 "reading_position": null}"#,
        )
        .unwrap();
        let contents = to_file_contents(&data).unwrap();
        assert!(contents.starts_with('{') && contents.ends_with("}\n"));
        assert_eq!(parse(contents.as_bytes()).unwrap(), data);
    }

    /// A file with `bookmarks`, `highlights` and `notes` copies of the
    /// largest record of each kind, and every other field at its largest,
    /// as `to_file_contents` writes it: its size in bytes.
    fn maximal_file_size(bookmarks: usize, highlights: usize, notes: usize) -> usize {
        let kjv = crate::db::test_support::bundled("kjv");
        let longest_name = kjv.iter().map(|b| b.name.as_str()).max_by_key(|n| n.len());
        let book_name = longest_name.map(str::to_string);
        let book = kjv.len() as i64;
        let chapter = kjv.iter().map(|b| b.chapters.len()).max().unwrap() as i64;
        let verse = kjv
            .iter()
            .flat_map(|b| b.chapters.iter().map(Vec::len))
            .max()
            .unwrap() as i64;
        let at = Some("2026-10-04T12:30:05.123Z".to_string());
        // The character that takes the most bytes in the file, of those a
        // note may hold (see `worst_case_note_characters_take_4_bytes`).
        let body = "\u{10ffff}".repeat(crate::db::notes::MAX_NOTE_CHARS);
        assert_eq!(normalize_body(&body).as_deref(), Ok(body.as_str()));
        let data = ReaderData {
            format: FORMAT.to_string(),
            format_version: FORMAT_VERSION,
            exported_at: at.clone().unwrap(),
            app_version: "10.10.10".to_string(),
            reading_position: Some(PositionRecord {
                translation: "kjv".to_string(),
                book,
                book_name: book_name.clone(),
                chapter,
            }),
            bookmarks: vec![
                BookmarkRecord {
                    book,
                    book_name: book_name.clone(),
                    chapter,
                    verse_start: verse,
                    verse_end: verse,
                    created_at: at.clone(),
                };
                bookmarks
            ],
            highlights: vec![
                HighlightRecord {
                    book,
                    book_name: book_name.clone(),
                    chapter,
                    verse,
                    color: HighlightColor::Yellow,
                    created_at: at.clone(),
                    updated_at: at.clone(),
                };
                highlights
            ],
            notes: vec![
                NoteRecord {
                    book,
                    book_name,
                    chapter,
                    verse_start: verse,
                    verse_end: verse,
                    body,
                    created_at: at.clone(),
                    updated_at: at,
                };
                notes
            ],
            preferences: Preferences {
                theme: Some(Theme::ClassicLight),
                text_size: Some(TextSize::Medium),
                line_spacing: Some(LineSpacing::Compact),
                width: Some(Width::Narrow),
                layout: Some(Layout::Flowing),
                reading_mode: Some(false),
                compare: Some(false),
                // Every code is three letters.
                compare_with: TranslationCode::new("web"),
            },
            skipped_preferences: Vec::new(),
        };
        to_file_contents(&data).unwrap().len()
    }

    /// Of every character a note may hold, none takes more than 4 bytes in
    /// a file: the control characters JSON would escape as 6 (`\u0001`)
    /// are refused.
    #[test]
    fn worst_case_note_characters_take_4_bytes() {
        let mut widest = 0;
        for c in (0..=0x10_ffff).filter_map(char::from_u32) {
            // Inside the text, so trimming can't drop it.
            if let Ok(text) = normalize_body(&format!("a{c}b")) {
                let encoded = serde_json::to_string(&text).unwrap().len() - 4;
                widest = widest.max(encoded);
            }
        }
        assert_eq!(widest, 4);
    }

    /// The arithmetic in `MAX_FILE_BYTES`'s documentation, with every
    /// record as large as it can be and the notes at both their limits.
    #[test]
    fn a_maximal_file_fits_the_size_limit() {
        use crate::db::notes::MAX_NOTE_CHARS;
        use crate::db::test_support::bundled_bsb;
        let base = maximal_file_size(1, 1, 1);
        let bookmark = maximal_file_size(2, 1, 1) - base;
        let highlight = maximal_file_size(1, 2, 1) - base;
        let note = maximal_file_size(1, 1, 2) - base;
        // Each record adds the same bytes, so the size is linear in each count.
        assert_eq!(maximal_file_size(3, 1, 1) - base, 2 * bookmark);
        assert_eq!(maximal_file_size(1, 3, 1) - base, 2 * highlight);
        assert_eq!(maximal_file_size(1, 1, 3) - base, 2 * note);
        let rest = base - bookmark - highlight - note;
        // A maximal note's text is 4 bytes a character; the rest is the record.
        let note_record = note - 4 * MAX_NOTE_CHARS;
        // As documented on `MAX_FILE_BYTES`.
        assert_eq!(
            (bookmark, highlight, note_record, rest),
            (188, 231, 254, 562)
        );
        let notes = MAX_NOTES * note_record + 4 * MAX_TOTAL_NOTE_CHARS;
        assert_eq!(notes, 102_540_000);

        // The verses a highlight or bookmark can be on: in each chapter,
        // up to the last any bundled translation numbers.
        let mut last_verses: HashMap<(usize, usize), usize> = HashMap::new();
        let mut count = |book: usize, chapter: usize, verses: usize| {
            let last = last_verses.entry((book, chapter)).or_default();
            *last = (*last).max(verses);
        };
        for code in ["kjv", "web"] {
            for (b, book) in crate::db::test_support::bundled(code).iter().enumerate() {
                for (c, verses) in book.chapters.iter().enumerate() {
                    count(b, c, verses.len());
                }
            }
        }
        for (b, book) in bundled_bsb().iter().enumerate() {
            for (c, verses) in book.chapters.iter().enumerate() {
                count(b, c, verses.len());
            }
        }
        let verses: usize = last_verses.values().sum();
        let ranges: usize = last_verses.values().map(|n| n * (n + 1) / 2).sum();
        assert_eq!((verses, ranges), (31_109, 530_288));

        let size = |bookmarks: usize, notes: usize| {
            (rest + bookmarks * bookmark + verses * highlight + notes) as u64
        };
        assert_eq!(size(50_000, notes), 119_126_741);
        assert!(size(130_271, notes) <= MAX_FILE_BYTES);
        assert!(size(130_272, notes) > MAX_FILE_BYTES);
        assert_eq!(size(ranges, 0), 106_880_885);
        assert!(size(ranges, 0) <= MAX_FILE_BYTES);
    }

    /// More notes than the reader can keep are refused, before anything
    /// else is checked. Bookmarks and highlights have no such limit: any
    /// number the file holds is checked as usual.
    #[test]
    fn only_notes_have_a_count_limit() {
        let empty = || ReaderData {
            format: FORMAT.to_string(),
            format_version: FORMAT_VERSION,
            exported_at: "x".to_string(),
            app_version: "2.8.0".to_string(),
            reading_position: None,
            bookmarks: Vec::new(),
            highlights: Vec::new(),
            notes: Vec::new(),
            preferences: Preferences::default(),
            skipped_preferences: Vec::new(),
        };
        let bookmark = BookmarkRecord {
            book: 43,
            book_name: None,
            chapter: 3,
            verse_start: 16,
            verse_end: 16,
            created_at: None,
        };
        let highlight = HighlightRecord {
            book: 43,
            book_name: None,
            chapter: 3,
            verse: 16,
            color: HighlightColor::Blue,
            created_at: None,
            updated_at: None,
        };
        let note = NoteRecord {
            book: 43,
            book_name: None,
            chapter: 3,
            verse_start: 16,
            verse_end: 16,
            body: "x".to_string(),
            created_at: None,
            updated_at: None,
        };
        let refused = |data: ReaderData| validate(data, &[], &[], &HashMap::new()).unwrap_err();
        let many = refused(ReaderData {
            bookmarks: vec![bookmark; 100_000],
            highlights: vec![highlight; 100_000],
            ..empty()
        });
        // Here the book is what's wrong, in every one of them.
        assert_eq!(many.problems()[0], "Bookmark 1: there's no book number 43.");
        assert_eq!(many.problems().last().unwrap(), "…and 199988 more.");
        assert_eq!(
            refused(ReaderData {
                notes: vec![note.clone(); MAX_NOTES + 1],
                ..empty()
            })
            .problems(),
            ["It has 10001 notes; the most one file can hold is 10000."]
        );
        // At the limit, the count is fine.
        let at_limit = refused(ReaderData {
            notes: vec![note; MAX_NOTES],
            ..empty()
        });
        assert_eq!(at_limit.problems()[0], "Note 1: there's no book number 43.");
    }

    #[test]
    fn a_minimal_file_parses() {
        let data = parse(
            br#"{"format": "gospel-getter-reader-data", "format_version": 1,
                 "exported_at": "2026-10-04T00:00:00.000Z", "app_version": "2.2.0",
                 "reading_position": null}"#,
        )
        .unwrap();
        assert!(data.bookmarks.is_empty());
        assert_eq!(data.preferences, Preferences::default());
        assert_eq!(data.preferences.count(), 0);
    }
}
