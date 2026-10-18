//! The shapes the commands the pieces call exchange, mirrored from
//! `src-tauri/src/commands.rs`, `src-tauri/src/notes_export.rs` and
//! `src-tauri/src/reader_data.rs` with the same names the JSON carries
//! (camelCase, except the display preferences' file names).
//!
//! This is the one place the pieces repeat Rust types. A later shared
//! crate (native and wasm32, serde only) would replace: `NotedBook` and
//! `DatePresets`/`DaySpan` (`notes_export.rs`), `NotesExportOptions` and
//! `NotesExportOutcome` (`commands.rs`), and `NotesFilter` (`notes_export.rs`,
//! which also carries `deny_unknown_fields` and `NaiveDate`s where this
//! side keeps `YYYY-MM-DD` strings, as the page did); for the import,
//! `ImportChoice`, `ImportPreviewDto`, `PositionPreviewDto` and
//! `ImportResultDto` (`commands.rs`) and `Preferences` (`reader_data.rs`,
//! whose enums travel as their kebab-case names, strings here).

use serde::{Deserialize, Serialize};

/// A book with notes, for the checklist (`notes_export::NotedBook`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotedBook {
    pub id: i64,
    pub name: String,
    /// How many notes it has.
    pub notes: u64,
    /// Its first and last chapter with notes.
    pub first_chapter: i64,
    pub last_chapter: i64,
    /// How many of its chapters have notes.
    pub noted_chapters: u64,
    /// How many chapters it has.
    pub chapter_count: i64,
}

/// A span of local days, each end as `YYYY-MM-DD` (`notes_export::DaySpan`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DaySpan {
    pub from: String,
    pub to: String,
}

/// The Written presets, ending today (`notes_export::DatePresets`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatePresets {
    pub today: String,
    pub last_7_days: DaySpan,
    pub last_30_days: DaySpan,
    pub this_year: DaySpan,
}

impl DatePresets {
    /// The preset the Written `<select>`'s value names, if it is one.
    pub fn preset(&self, when: &str) -> Option<&DaySpan> {
        match when {
            "last7Days" => Some(&self.last_7_days),
            "last30Days" => Some(&self.last_30_days),
            "thisYear" => Some(&self.this_year),
            _ => None,
        }
    }
}

/// `get_notes_export_options`'s answer (`commands::NotesExportOptions`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ExportOptions {
    /// The books with notes, in Bible order.
    pub books: Vec<NotedBook>,
    pub dates: DatePresets,
}

/// What `count_notes_export` and `export_notes` read
/// (`notes_export::NotesFilter`): every book is an empty list, and a
/// chapter range around all of a book's notes is none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    pub books: Vec<i64>,
    pub chapters: Option<(i64, i64)>,
    pub written_from: Option<String>,
    pub written_to: Option<String>,
}

/// `count_notes_export`'s arguments.
#[derive(Debug, Serialize)]
pub struct CountArgs<'a> {
    pub filter: &'a Filter,
}

/// `export_notes`'s arguments.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportArgs<'a> {
    pub translation_code: &'a str,
    pub filter: &'a Filter,
}

/// `export_notes`'s answer (`commands::NotesExportOutcome`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ExportOutcome {
    /// The reader closed the save dialog; no file was written.
    Cancelled,
    /// The file was completely written.
    Saved { path: String, notes: u64 },
}

/// What choosing a file to import found (`commands::ImportChoice`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ImportChoice {
    /// The reader closed the open dialog.
    Cancelled,
    /// The file can't be imported; nothing was changed.
    Invalid {
        file_name: String,
        message: String,
        problems: Vec<String>,
    },
    /// What importing it would do; nothing has changed yet.
    Ready { preview: Box<ImportPreview> },
}

/// What's in an import file, and what importing it would do
/// (`commands::ImportPreviewDto`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub token: u64,
    pub file_name: String,
    pub exported_at: String,
    pub app_version: String,
    pub bookmarks_in_file: u64,
    pub duplicates_in_file: u64,
    pub new_bookmarks: u64,
    pub already_present: u64,
    pub current_bookmarks: u64,
    pub sample: Vec<String>,
    pub has_highlights: bool,
    pub highlights_in_file: u64,
    pub highlight_duplicates_in_file: u64,
    pub new_highlights: u64,
    pub already_highlighted: u64,
    pub current_highlights: u64,
    pub has_notes: bool,
    pub notes_in_file: u64,
    pub note_duplicates_in_file: u64,
    pub new_notes: u64,
    pub notes_already_present: u64,
    pub current_notes: u64,
    pub position: Option<PositionPreview>,
    pub preferences: Preferences,
    pub skipped_preferences_notice: Option<String>,
}

/// A reading position, as the import names it
/// (`commands::PositionPreviewDto`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionPreview {
    pub translation_code: String,
    pub book_id: i64,
    pub chapter: i64,
    /// e.g. "Romans 8 (WEB)".
    pub reference: String,
}

/// Display preferences in an import file (`reader_data::Preferences`), by
/// their names in the file; each is there or not.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct Preferences {
    pub theme: Option<String>,
    pub text_size: Option<String>,
    pub line_spacing: Option<String>,
    pub width: Option<String>,
    pub layout: Option<String>,
    pub reading_mode: Option<bool>,
    pub compare: Option<bool>,
    pub compare_with: Option<String>,
}

/// `apply_import`'s arguments.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyImportArgs {
    pub token: u64,
    pub replace: bool,
    pub restore_position: bool,
}

/// `cancel_import`'s arguments.
#[derive(Debug, Serialize)]
pub struct CancelImportArgs {
    pub token: u64,
}

/// What a confirmed import did (`commands::ImportResultDto`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub added: u64,
    pub already_present: u64,
    pub removed: u64,
    pub highlights_added: u64,
    pub highlights_already_present: u64,
    pub highlights_removed: u64,
    pub notes_added: u64,
    pub notes_already_present: u64,
    pub notes_removed: u64,
    pub position: Option<PositionPreview>,
}
