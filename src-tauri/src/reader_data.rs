//! The reader's own data — bookmarks, highlights, last reading position
//! and display preferences — as a portable, human-readable JSON file, for
//! moving between installs or keeping a backup.
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
//! This module is the format and its validation; reading and writing the
//! database is `db::reader_data`, and the commands (with the native file
//! pickers) are in `commands`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::db::{Book, HighlightColor, Translation};

/// The `format` every reader-data file declares.
pub const FORMAT: &str = "gospel-getter-reader-data";
/// The `format_version` this version writes. It reads this and
/// `FORMAT_VERSION_1`; anything newer is refused, since it may mean things
/// this version would misread.
pub const FORMAT_VERSION: u64 = 2;
/// The first format: bookmarks, reading position and preferences, but no
/// highlights.
pub const FORMAT_VERSION_1: u64 = 1;
/// The suggested file-name suffix.
pub const FILE_SUFFIX: &str = ".gospel-getter.json";
/// The largest file import will read. A file with the maximum number of
/// bookmarks and highlights is under this.
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
/// The most bookmarks one file may hold.
pub const MAX_BOOKMARKS: usize = 10_000;
/// The most highlights one file may hold: more than there are verses.
pub const MAX_HIGHLIGHTS: usize = 40_000;
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
    #[serde(default)]
    pub preferences: Preferences,
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

/// Display preferences. Each is optional: a file only changes the ones it
/// names. The values are the ones Aa offers (and the
/// `<head>` preferences script in `ui/index.html` accepts — a test keeps
/// the two lists in step).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<Theme>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_size: Option<TextSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_spacing: Option<LineSpacing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reading_mode: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compare: Option<bool>,
}

impl Preferences {
    /// How many preferences are set.
    pub fn count(&self) -> usize {
        [
            self.theme.is_some(),
            self.text_size.is_some(),
            self.line_spacing.is_some(),
            self.reading_mode.is_some(),
            self.compare.is_some(),
        ]
        .into_iter()
        .filter(|&set| set)
        .count()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    Vaporwave,
    ClassicDark,
    ClassicLight,
    Matrix,
    BeastSlayer,
    HotPink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextSize {
    Small,
    Medium,
    Large,
    XLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineSpacing {
    Compact,
    Normal,
    Relaxed,
}

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
                    "This isn't a Gospel Getter file of bookmarks, highlights and settings."
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
        // A format 1 file is read exactly as format 1 always was: one that
        // names highlights has a field format 1 doesn't define.
        Some(FORMAT_VERSION_1) => {
            if value.get("highlights").is_some() {
                return Err(ImportError::Malformed(
                    "The file doesn't have the expected layout: unknown field `highlights` \
                     (data format 1 has no highlights)."
                        .to_string(),
                ));
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
                 reads (it reads formats {FORMAT_VERSION_1} and {FORMAT_VERSION})."
            )));
        }
    }
    serde_json::from_value(value).map_err(|e| {
        ImportError::Malformed(format!("The file doesn't have the expected layout: {e}."))
    })
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
    pub position: Option<NewPosition>,
    pub preferences: Preferences,
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
    let name_matches = |book: &Book, name: &Option<String>| {
        name.as_deref()
            .is_none_or(|n| n.trim().eq_ignore_ascii_case(&book.name))
    };

    if data.bookmarks.len() > MAX_BOOKMARKS {
        return Err(ImportError::Invalid(vec![format!(
            "It has {} bookmarks; the most one file can hold is {MAX_BOOKMARKS}.",
            data.bookmarks.len()
        )]));
    }
    if data.highlights.len() > MAX_HIGHLIGHTS {
        return Err(ImportError::Invalid(vec![format!(
            "It has {} highlights; the most one file can hold is {MAX_HIGHLIGHTS}.",
            data.highlights.len()
        )]));
    }

    let mut bookmarks: Vec<NewBookmark> = Vec::new();
    let mut index_of: HashMap<(i64, i64, i64, i64), usize> = HashMap::new();
    let mut duplicates_in_file = 0;
    for (i, b) in data.bookmarks.into_iter().enumerate() {
        let label = format!("Bookmark {}", i + 1);
        let Some(found) = book(b.book) else {
            problems.push(format!("{label}: there's no book number {}.", b.book));
            continue;
        };
        if !name_matches(found, &b.book_name) {
            problems.push(format!(
                "{label}: book {} is {}, not {}.",
                b.book,
                found.name,
                b.book_name.as_deref().unwrap_or_default()
            ));
            continue;
        }
        if !(1..=found.chapter_count).contains(&b.chapter) {
            problems.push(format!(
                "{label}: there's no {} {} ({} has {} chapters).",
                found.name, b.chapter, found.name, found.chapter_count
            ));
            continue;
        }
        let reference = format!(
            "{} {}:{}{}",
            found.name,
            b.chapter,
            b.verse_start,
            if b.verse_end == b.verse_start {
                String::new()
            } else {
                format!("\u{2013}{}", b.verse_end)
            }
        );
        let last = last_verses
            .get(&(found.id, b.chapter))
            .copied()
            .unwrap_or(0);
        if b.verse_start < 1 || b.verse_end < b.verse_start {
            problems.push(format!("{label}: {reference} isn't a valid verse range."));
            continue;
        }
        if b.verse_end > last {
            problems.push(format!(
                "{label}: there's no {reference}; {} {} ends at verse {last}.",
                found.name, b.chapter
            ));
            continue;
        }
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
                h.book_name.as_deref().unwrap_or_default()
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
                        p.translation,
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
                            p.book_name.as_deref().unwrap_or_default()
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
        position,
        preferences: data.preferences,
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

    const HTML: &str = include_str!("../../ui/index.html");

    /// The `<option value>`s of one preference `<select>` (Aa's record of
    /// that choice).
    fn select_options(id: &str) -> Vec<String> {
        let start = HTML
            .find(&format!(r#"<select id="{id}""#))
            .expect("select exists");
        let end = start + HTML[start..].find("</select>").expect("select closes");
        HTML[start..end]
            .split(r#"<option value=""#)
            .skip(1)
            .map(|s| s[..s.find('"').expect("quoted")].to_string())
            .collect()
    }

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

    #[test]
    fn preference_values_match_what_aa_offers() {
        use LineSpacing as L;
        use TextSize as S;
        use Theme as T;
        assert_eq!(
            names(&[
                T::Vaporwave,
                T::ClassicDark,
                T::ClassicLight,
                T::Matrix,
                T::BeastSlayer,
                T::HotPink
            ]),
            select_options("theme-select")
        );
        assert_eq!(
            names(&[S::Small, S::Medium, S::Large, S::XLarge]),
            select_options("text-size-select")
        );
        assert_eq!(
            names(&[L::Compact, L::Normal, L::Relaxed]),
            select_options("line-spacing-select")
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
            br#"{"format": "gospel-getter-reader-data", "format_version": 3, "brand_new": true}"#,
        )
        .unwrap_err();
        assert!(
            newer
                .to_string()
                .contains("newer version of Gospel Getter (data format 3)"),
            "{newer}"
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
        let bad_theme = parse(
            br#"{"format": "gospel-getter-reader-data", "format_version": 1, "exported_at": "x",
                 "app_version": "2.2.0", "reading_position": null, "preferences": {"theme": "neon"}}"#,
        )
        .unwrap_err();
        assert!(bad_theme.to_string().contains("neon"), "{bad_theme}");
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
