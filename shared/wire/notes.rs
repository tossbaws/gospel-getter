//! The notes' shapes: the commentary `get_notes_commentary` answers with
//! (`notes_export`), the references found in a note (`commands::notes`),
//! the AI-flagged findings (`commands::findings`), and the notes export's
//! books and outcomes (`commands::export_notes`).

use serde::{Deserialize, Serialize};

/// The notes as a commentary (see `notes_export::NotesExport::commentary`):
/// what the file says, and what the page prints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Commentary {
    /// "Commentary on John", or "Gospel Getter commentary".
    pub title: String,
    /// "Exported Oct 10, 2026 · 3 notes".
    pub exported: String,
    /// "Written Oct 1 – Oct 12, 2026", when the filter limits the dates.
    #[serde(default)]
    pub written: Option<String>,
    /// Whether each book has its own heading: not when the export is of
    /// one book, which the title names.
    pub book_headings: bool,
    pub books: Vec<CommentaryBook>,
}

/// One book's part of a commentary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentaryBook {
    pub name: String,
    pub chapters: Vec<CommentaryChapter>,
}

/// One chapter's part: its heading ("John 3") and its notes, in Bible
/// order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentaryChapter {
    pub heading: String,
    pub notes: Vec<CommentaryNote>,
}

/// One note's line: its verses ("3:16", "3:16–18") and its text, as the
/// reader wrote it (plain text).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentaryNote {
    pub verses: String,
    pub body: String,
}

/// A reference typed in a note ("see Rom 5:8"): where it is in the note's
/// `body` (byte offsets, `start..end`) and the passage it names (see
/// `domain::query::references_in`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteLinkDto {
    pub start: usize,
    pub end: usize,
    pub book_id: i64,
    pub chapter: i64,
    /// Its verses, or none for the whole chapter.
    pub verse_start: Option<i64>,
    pub verse_end: Option<i64>,
    /// "Romans 5:8": the passage, as the app names it.
    pub reference: String,
}

/// An AI-flagged finding: where an AI agent, at the reader's request,
/// marked a verse. A marker only: who found it, for which request, and
/// nothing about the verse (C3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingDto {
    pub id: i64,
    pub book_id: i64,
    pub book_name: String,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    /// "John 11:35".
    pub reference: String,
    /// Who found it: "Claude".
    pub agent: String,
    /// What the reader asked for: "grief".
    pub request: String,
    /// What the app shows for it, built from `agent` and `request` alone:
    /// "Found by Claude for your request “grief”." (C3).
    pub text: String,
    pub created_at: String,
    /// The day it was found, as the app shows dates ("Nov 7, 2026").
    pub created_at_display: String,
}

/// One request's findings, as the Library groups them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingRequestDto {
    pub request: String,
    /// Who found them: "Claude", or "Claude and Codex".
    pub agents: String,
    /// How many verses they mark.
    pub verse_count: i64,
    /// The day the latest was found ("Nov 7, 2026").
    pub latest_display: String,
    /// In Bible order.
    pub findings: Vec<FindingDto>,
}

/// A book with notes, as the export dialog lists it (`notes_export`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotedBook {
    pub id: i64,
    pub name: String,
    /// How many notes it has.
    pub notes: u64,
    /// The first and last chapters with notes: the chapter range's
    /// starting values.
    pub first_chapter: i64,
    pub last_chapter: i64,
    /// How many chapters have notes. With only one there's no range to
    /// choose, so the dialog doesn't offer one.
    pub noted_chapters: u64,
    /// The range's limits are 1 and this.
    pub chapter_count: i64,
}

/// How exporting notes ended (`export_notes`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NotesExportOutcome {
    /// The reader closed the save dialog; no file was written.
    Cancelled,
    /// The file was completely written.
    Saved { path: String, notes: u64 },
}

/// How exporting notes as one file per book ended, or paused
/// (`export_notes_by_book`, `confirm_notes_export_by_book`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NotesFolderOutcome {
    /// The reader closed the folder dialog, or chose not to replace the
    /// files already there; nothing was written.
    Cancelled,
    /// Some of the files (`existing`, by name) are already in `folder`:
    /// nothing is written until the reader says whether to replace them
    /// (`confirm_notes_export_by_book` with `token`).
    Exists {
        token: u64,
        folder: String,
        existing: Vec<String>,
    },
    /// Every file was completely written.
    Saved {
        folder: String,
        files: u64,
        notes: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::super::pin;
    use super::*;

    fn note() -> CommentaryNote {
        CommentaryNote {
            verses: "16–18".into(),
            body: "**Grace**, not works.".into(),
        }
    }

    fn chapter() -> CommentaryChapter {
        CommentaryChapter {
            heading: "Chapter 3".into(),
            notes: vec![note()],
        }
    }

    fn book() -> CommentaryBook {
        CommentaryBook {
            name: "John".into(),
            chapters: vec![chapter()],
        }
    }

    fn finding() -> FindingDto {
        FindingDto {
            id: 7,
            book_id: 43,
            book_name: "John".into(),
            chapter: 11,
            verse_start: 35,
            verse_end: 35,
            reference: "John 11:35".into(),
            agent: "Claude".into(),
            request: "grief".into(),
            text: "Found by Claude for your request “grief”.".into(),
            created_at: "2026-11-07 09:30:00".into(),
            created_at_display: "Nov 7, 2026".into(),
        }
    }

    #[test]
    fn commentary_note() {
        pin(
            &note(),
            r#"{"verses":"16–18","body":"**Grace**, not works."}"#,
        );
    }

    #[test]
    fn commentary_chapter() {
        pin(
            &chapter(),
            r#"{"heading":"Chapter 3","notes":[{"verses":"16–18","body":"**Grace**, not works."}]}"#,
        );
    }

    #[test]
    fn commentary_book() {
        pin(
            &book(),
            r#"{"name":"John","chapters":[{"heading":"Chapter 3","notes":[{"verses":"16–18","body":"**Grace**, not works."}]}]}"#,
        );
    }

    #[test]
    fn commentary() {
        let commentary = Commentary {
            title: "My notes".into(),
            exported: "Nov 14, 2026".into(),
            written: None,
            book_headings: true,
            books: vec![book()],
        };
        pin(
            &commentary,
            r#"{"title":"My notes","exported":"Nov 14, 2026","written":null,"bookHeadings":true,"books":[{"name":"John","chapters":[{"heading":"Chapter 3","notes":[{"verses":"16–18","body":"**Grace**, not works."}]}]}]}"#,
        );
    }

    #[test]
    fn note_link() {
        let link = NoteLinkDto {
            start: 4,
            end: 14,
            book_id: 45,
            chapter: 5,
            verse_start: Some(8),
            verse_end: None,
            reference: "Romans 5:8".into(),
        };
        pin(
            &link,
            r#"{"start":4,"end":14,"bookId":45,"chapter":5,"verseStart":8,"verseEnd":null,"reference":"Romans 5:8"}"#,
        );
    }

    #[test]
    fn finding_dto() {
        pin(
            &finding(),
            r#"{"id":7,"bookId":43,"bookName":"John","chapter":11,"verseStart":35,"verseEnd":35,"reference":"John 11:35","agent":"Claude","request":"grief","text":"Found by Claude for your request “grief”.","createdAt":"2026-11-07 09:30:00","createdAtDisplay":"Nov 7, 2026"}"#,
        );
    }

    #[test]
    fn finding_request() {
        let request = FindingRequestDto {
            request: "grief".into(),
            agents: "Claude".into(),
            verse_count: 1,
            latest_display: "Nov 7, 2026".into(),
            findings: vec![finding()],
        };
        pin(
            &request,
            r#"{"request":"grief","agents":"Claude","verseCount":1,"latestDisplay":"Nov 7, 2026","findings":[{"id":7,"bookId":43,"bookName":"John","chapter":11,"verseStart":35,"verseEnd":35,"reference":"John 11:35","agent":"Claude","request":"grief","text":"Found by Claude for your request “grief”.","createdAt":"2026-11-07 09:30:00","createdAtDisplay":"Nov 7, 2026"}]}"#,
        );
    }

    #[test]
    fn noted_book() {
        let book = NotedBook {
            id: 43,
            name: "John".into(),
            notes: 3,
            first_chapter: 1,
            last_chapter: 3,
            noted_chapters: 2,
            chapter_count: 21,
        };
        pin(
            &book,
            r#"{"id":43,"name":"John","notes":3,"firstChapter":1,"lastChapter":3,"notedChapters":2,"chapterCount":21}"#,
        );
    }

    #[test]
    fn notes_export_outcome() {
        pin(&NotesExportOutcome::Cancelled, r#"{"status":"cancelled"}"#);
        let saved = NotesExportOutcome::Saved {
            path: "/home/r/notes.md".into(),
            notes: 4,
        };
        pin(
            &saved,
            r#"{"status":"saved","path":"/home/r/notes.md","notes":4}"#,
        );
    }

    #[test]
    fn notes_folder_outcome() {
        pin(&NotesFolderOutcome::Cancelled, r#"{"status":"cancelled"}"#);
        let exists = NotesFolderOutcome::Exists {
            token: 2,
            folder: "/home/r/Notes".into(),
            existing: vec!["43 John.md".into()],
        };
        pin(
            &exists,
            r#"{"status":"exists","token":2,"folder":"/home/r/Notes","existing":["43 John.md"]}"#,
        );
        let saved = NotesFolderOutcome::Saved {
            folder: "/home/r/Notes".into(),
            files: 3,
            notes: 4,
        };
        pin(
            &saved,
            r#"{"status":"saved","folder":"/home/r/Notes","files":3,"notes":4}"#,
        );
    }
}
