//! The shapes the three notes-export commands exchange, mirrored from
//! `src-tauri/src/commands.rs` and `src-tauri/src/notes_export.rs` with the
//! same camelCase names the JSON carries.
//!
//! This is the one place the pilot repeats a Rust type. A later shared
//! crate (native and wasm32, serde only) would replace: `NotedBook` and
//! `DatePresets`/`DaySpan` (`notes_export.rs`), `NotesExportOptions` and
//! `NotesExportOutcome` (`commands.rs`), and `NotesFilter` (`notes_export.rs`,
//! which also carries `deny_unknown_fields` and `NaiveDate`s where this
//! side keeps `YYYY-MM-DD` strings, as the page did).

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
