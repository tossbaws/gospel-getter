//! The shapes the commands the pieces call exchange, mirrored from
//! `src-tauri/src/commands/`, `src-tauri/src/notes_export.rs` and
//! `src-tauri/src/reader_data.rs` with the same names the JSON carries
//! (camelCase, except the display preferences' file names).
//!
//! This is the one place the pieces repeat Rust types. A later shared
//! crate (native and wasm32, serde only) would replace: `NotedBook` and
//! `DatePresets`/`DaySpan` (`notes_export.rs`), `NotesExportOptions` and
//! `NotesExportOutcome` (`commands/export_notes.rs`), and `NotesFilter`
//! (`notes_export.rs`, which also carries `deny_unknown_fields` and
//! `NaiveDate`s where this side keeps `YYYY-MM-DD` strings, as the page
//! did); for the import, `ImportChoice`, `ImportPreviewDto`,
//! `PositionPreviewDto` and `ImportResultDto` (`commands/backup.rs`) and
//! `Preferences` (`reader_data.rs`, whose enums travel as their kebab-case
//! names, strings here); for the updater, `UpdateInfo` and `DownloadEvent`
//! (`updater.rs`) and `Block` and `Span` (`release_notes.rs`); for the
//! Library, `BookmarkDto` (`commands/bookmarks.rs`), `HighlightPassageDto`
//! (`commands/highlights.rs`) and `NoteDto` (`commands/notes.rs`), of which
//! these read only what the lists show; for search, `SearchDto`,
//! `SearchResultDto` and `GoToDto` (`commands/search.rs`); for the reader,
//! `HomeDto`'s translations and position, `TranslationDto`,
//! `ReadingPaneDto`, `VerseDto` and `SimpleVerseDto`, `XrefDto`,
//! `ChapterSideDto`, `BibleEdgeDto` and `BookStartDto`
//! (`commands/reading.rs`), and `CompareDto` and `CompareColumnDto`
//! (`commands/compare.rs`); for the selection card, `NoteDto` (what the
//! card shows of it), `PassageDto`, `CopyTextDto` and `CopyFormatsDto`
//! (`commands/passages.rs`), and `HighlightDto` (`commands/highlights.rs`).
//! A verse's parts and footnotes are already shared with the backend
//! (`verse_parts`), and so is how a passage is named (`passage_reference`).

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

/// `add_note`'s arguments.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddNoteArgs<'a> {
    pub book_id: i64,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    pub body: &'a str,
}

/// `update_note`'s arguments.
#[derive(Debug, Serialize)]
pub struct UpdateNoteArgs<'a> {
    pub id: i64,
    pub body: &'a str,
}

/// `delete_note`'s arguments.
#[derive(Debug, Serialize)]
pub struct DeleteNoteArgs {
    pub id: i64,
}

/// An available update, as `check_for_update` answers
/// (`updater::UpdateInfo`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    /// The release notes as plain text (the banner's What's new).
    pub notes: Option<String>,
    /// The same notes as blocks of text (About shows them).
    #[serde(default)]
    pub note_blocks: Vec<NoteBlock>,
    /// Whether this build can install it (else it links to it).
    pub can_self_update: bool,
}

/// A block of release notes (`release_notes::Block`): any heading, a
/// paragraph, or a list item `depth` lists deep (0 at the top).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum NoteBlock {
    Heading { spans: Vec<NoteSpan> },
    Paragraph { spans: Vec<NoteSpan> },
    Item { depth: u8, spans: Vec<NoteSpan> },
}

/// A run of text in the notes, maybe bold or code (`release_notes::Span`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct NoteSpan {
    pub text: String,
    #[serde(default)]
    pub strong: bool,
    #[serde(default)]
    pub code: bool,
}

/// What `install_update` reports on its channel as the update downloads
/// (`updater::DownloadEvent`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum DownloadEvent {
    #[serde(rename_all = "camelCase")]
    Started { content_length: Option<u64> },
    #[serde(rename_all = "camelCase")]
    Progress { chunk_length: u64 },
    /// Downloaded and verified; installing, then restarting.
    Finished,
}

/// A bookmark, as `list_bookmarks` answers (`commands::BookmarkDto`; the
/// list reads only these fields).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    pub id: i64,
    pub book_id: i64,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    /// e.g. "John 3:16–18", the backend's.
    pub reference: String,
    /// The first verse in the translation asked for, or `None` if it
    /// doesn't number it.
    pub preview: Option<String>,
    /// Whether that translation numbers every verse of it.
    pub complete: bool,
}

/// A highlighted passage, as `list_highlight_passages` answers
/// (`commands::HighlightPassageDto`): consecutive verses in one color.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightPassage {
    pub book_id: i64,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    /// One of `library_text::COLORS`.
    pub color: String,
    pub reference: String,
    pub preview: Option<String>,
    pub complete: bool,
}

/// A note, as `list_all_notes` answers (`commands::NoteDto`; the list reads
/// only these fields, and hands the page the note as it came).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: i64,
    pub book_id: i64,
    /// "1 John": the book's name, which the Library's search looks in.
    pub book_name: String,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    pub reference: String,
    pub body: String,
}

/// The arguments of `list_bookmarks` and `list_highlight_passages`: the
/// translation the previews are in.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationArgs<'a> {
    pub translation_code: &'a str,
}

/// `remove_bookmark`'s arguments.
#[derive(Debug, Serialize)]
pub struct RemoveBookmarkArgs {
    pub id: i64,
}

/// A passage's verses: the arguments of `remove_highlight`, `add_bookmark`
/// and `list_notes`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PassageArgs {
    pub book_id: i64,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
}

/// `search`'s arguments: what the box holds, as typed, the translation to
/// search, and where the page of verses starts.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchArgs<'a> {
    pub query: &'a str,
    pub translation_code: &'a str,
    pub offset: i64,
}

/// What the search box should do with what's typed (`commands::SearchDto`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SearchAnswer {
    /// Nothing to search for.
    Empty,
    /// Go to this passage (its verses, or the whole chapter).
    Passage {
        book_id: i64,
        chapter: i64,
        verse_start: Option<i64>,
        verse_end: Option<i64>,
        reference: String,
    },
    /// A reference that doesn't exist; why.
    Invalid { message: String },
    /// Search isn't ready, or failed; why.
    Unavailable { message: String },
    /// One page of matching verses, in Bible order.
    Text {
        terms: Vec<String>,
        total: i64,
        offset: i64,
        /// How many verses a page holds (`commands::SEARCH_PAGE_SIZE`).
        page_size: i64,
        results: Vec<SearchVerse>,
        /// A book typed by name alone: its first chapter, offered first.
        /// Left out of the JSON otherwise.
        #[serde(default)]
        go_to: Option<GoTo>,
    },
}

/// A book typed by name alone: its first chapter, to go to
/// (`commands::GoToDto`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoTo {
    pub book_id: i64,
    /// Always 1.
    pub chapter: i64,
    /// e.g. "Psalms 1".
    pub reference: String,
}

/// A matching verse (`commands::SearchResultDto`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchVerse {
    pub book_id: i64,
    pub chapter: i64,
    pub verse: i64,
    pub reference: String,
    pub text: String,
    /// The words the search matched: byte ranges of `text`.
    #[serde(default)]
    pub marks: Vec<(usize, usize)>,
}

// ---- The reader.

/// What the reader takes from `get_home`'s answer (`commands::HomeDto`):
/// the translations and where the reader was: the chapter, and the verse
/// at the top of the reading pane, its top `current_verse_offset` px below
/// the bar (none: the chapter's top).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Home {
    pub translations: Vec<Translation>,
    pub current_translation_code: String,
    pub current_book_id: i64,
    pub current_chapter: i64,
    #[serde(default)]
    pub current_verse: Option<i64>,
    #[serde(default)]
    pub current_verse_offset: Option<f64>,
}

/// A bundled translation (`commands::TranslationDto`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Translation {
    pub code: String,
    pub name: String,
    /// When it was made ("1611/1769"), if there's one date.
    #[serde(default)]
    pub date: Option<String>,
    pub licence: String,
    /// Its line in About's credits.
    pub credit: String,
}

/// A cross-reference (`commands::XrefDto`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Xref {
    pub book_id: i64,
    pub chapter: i64,
    pub verse: i64,
    #[serde(default)]
    pub end_verse: Option<i64>,
    pub citation: String,
}

/// A verse as shown (`commands::VerseDto`, or `SimpleVerseDto`, which has
/// no cross-references).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Verse {
    pub number: i64,
    /// Its plain text, for anything without parts.
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub parts: Option<Vec<crate::verse_parts::Part>>,
    #[serde(default)]
    pub footnotes: Vec<crate::verse_parts::Footnote>,
    #[serde(default)]
    pub xrefs: Vec<Xref>,
}

/// A neighboring chapter, shown faded (`commands::ChapterSideDto`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterSide {
    pub book_id: i64,
    pub chapter: i64,
    pub book_name: String,
    pub verses: Vec<Verse>,
}

/// The quiet card in a neighbor's place at the Bible's edges
/// (`commands::BibleEdgeDto`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Edge {
    pub title: String,
    #[serde(default)]
    pub detail: Option<String>,
}

/// A book's first chapter, named (`commands::BookStartDto`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookStart {
    pub book_id: i64,
    pub chapter: i64,
    pub book_name: String,
}

/// `get_reading`'s answer (`commands::ReadingPaneDto`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub current_book_id: i64,
    pub current_chapter: i64,
    pub current_book_name: String,
    pub current_verses: Vec<Verse>,
    #[serde(default)]
    pub prev: Option<ChapterSide>,
    #[serde(default)]
    pub next: Option<ChapterSide>,
    #[serde(default)]
    pub prev_edge: Option<Edge>,
    #[serde(default)]
    pub next_edge: Option<Edge>,
    #[serde(default)]
    pub prev_book: Option<BookStart>,
    #[serde(default)]
    pub next_book: Option<BookStart>,
    #[serde(default)]
    pub has_chapter_xrefs: bool,
}

/// `get_reading`'s arguments.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadingArgs<'a> {
    pub book_id: i64,
    pub chapter: i64,
    pub translation_code: &'a str,
}

/// One translation's column (`commands::CompareColumnDto`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CompareColumn {
    pub code: String,
    pub name: String,
    pub verses: Vec<Verse>,
}

/// `get_compare`'s answer (`commands::CompareDto`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Compare {
    pub book_id: i64,
    pub chapter: i64,
    pub columns: Vec<CompareColumn>,
}

/// `get_compare`'s arguments.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareArgs<'a> {
    pub book_id: i64,
    pub chapter: i64,
    pub translation_codes: &'a [String],
}

/// `save_reading_position`'s arguments: the chapter on screen, and where in
/// it the reader is (the verse at the top of the reading pane, its top
/// `offset` px below the bar), left out at the chapter's top.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub book_id: i64,
    pub chapter: i64,
    pub translation_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verse: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<f64>,
}

// ---- The selection card.

/// A note on the selection, as `list_notes` answers (`commands::NoteDto`;
/// the card reads only these fields, and hands the note editor the note as
/// it came).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionNote {
    pub id: i64,
    pub book_id: i64,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    pub reference: String,
    pub body: String,
    pub created_at: String,
    /// "Oct 9, 2026", the backend's (`notes_export::display_date`).
    pub created_at_display: String,
}

/// A cross-reference's passage, as `get_passage` answers
/// (`commands::PassageDto`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PassageText {
    /// "1 John 4:9–10".
    pub reference: String,
    pub verses: Vec<Verse>,
    /// What isn't numbered, if anything.
    #[serde(default)]
    pub notice: Option<String>,
}

/// `get_passage`'s arguments.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PassageTextArgs<'a> {
    pub translation_code: &'a str,
    pub book_id: i64,
    pub chapter: i64,
    pub start: i64,
    pub end: i64,
}

/// A passage's text to copy, as `get_copy_text` answers
/// (`commands::CopyTextDto`): what the status line names, and each format.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CopyText {
    pub reference: String,
    pub formats: CopyFormats,
}

/// The passage in each of Copy as…'s formats (`commands::CopyFormatsDto`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CopyFormats {
    pub reference: String,
    pub numbered: String,
    pub markdown: String,
    pub plain: String,
}

impl CopyFormats {
    /// The text in `format` (`card_text::COPY_FORMATS`' names).
    pub fn get(&self, format: &str) -> Option<&str> {
        match format {
            "reference" => Some(&self.reference),
            "numbered" => Some(&self.numbered),
            "markdown" => Some(&self.markdown),
            "plain" => Some(&self.plain),
            _ => None,
        }
    }
}

/// `get_copy_text`'s arguments: the selected verses, or (without them) the
/// whole chapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyTextArgs {
    pub translation_code: String,
    pub book_id: i64,
    pub chapter: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<i64>,
}

/// A highlighted verse, as `list_highlights` answers
/// (`commands::HighlightDto`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Highlight {
    pub book_id: i64,
    pub chapter: i64,
    pub verse: i64,
    pub color: String,
}

/// `set_highlight`'s arguments: the passage's verses and the color.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetHighlightArgs<'a> {
    pub book_id: i64,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    pub color: &'a str,
}
