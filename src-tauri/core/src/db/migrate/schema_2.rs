//! Schema 2: the reader's four tables rebuilt, `bookmarks`, `highlights`,
//! `notes` and `ai_findings`, every row copied as it is, ids and dates
//! included. Frozen, like every step (see `migrate`'s doc).
//!
//! An id is never handed out twice. The three tables with an `id` get
//! `INTEGER PRIMARY KEY AUTOINCREMENT`: without it SQLite gives a new row
//! the largest id there plus one, so deleting the newest bookmark, note or
//! finding hands its id to the next, and a view still holding the old one
//! (the Library, a card, an Undo) could change or remove the new one
//! (GG-06, GG32-04, GG32-08). The next id after the rebuild is past the
//! largest copied. The `(id, created_at)` match each command makes stays,
//! as a second guard, for a database still at version 1.
//!
//! The product's limits are the app's, in Rust, and no longer the
//! table's: a note's length (`notes::MAX_NOTE_CHARS`), the highlight
//! colors (`HighlightColor`), and the length of an agent's name and a
//! request (`findings::{MAX_STORED_AGENT_CHARS, MAX_REQUEST_CHARS}`). A
//! CHECK can't change without another rebuild, and `CREATE TABLE IF NOT
//! EXISTS` never touched an existing table, so a limit raised in both
//! places would have refused the new value on every database made before.
//! The structural CHECKs stay (a chapter and a verse start at 1, a range
//! doesn't end before it starts), with every NOT NULL, foreign key, UNIQUE
//! constraint, index and highlights' key.
//!
//! The first rebuild of a reader's tables can be undone: before it runs,
//! `migrate` writes a whole copy of the database beside it,
//! `<database>.before-schema-2`, once, and never deletes it (the README's
//! data section says it's there, and that it can be deleted).
//!
//! 2.32.x reads and writes the result as it did version 1: its statements
//! name the same columns, its own limits are these, and its `migrate` keeps
//! a higher version (`migrate_tests` runs its SQL on an upgraded database).

use super::{Rebuild, Step};

/// The step.
pub(super) const STEP: Step = Step {
    to: 2,
    keep_copy: true,
    rebuilds: &[
        Rebuild {
            table: "bookmarks",
            create: BOOKMARKS,
            columns: &[
                "id",
                "book_id",
                "chapter",
                "verse_start",
                "verse_end",
                "created_at",
            ],
            autoincrement: true,
        },
        Rebuild {
            table: "highlights",
            create: HIGHLIGHTS,
            columns: &[
                "rowid",
                "book_id",
                "chapter",
                "verse",
                "color",
                "created_at",
                "updated_at",
            ],
            autoincrement: false,
        },
        Rebuild {
            table: "notes",
            create: NOTES,
            columns: &[
                "id",
                "book_id",
                "chapter",
                "verse_start",
                "verse_end",
                "body",
                "created_at",
                "updated_at",
            ],
            autoincrement: true,
        },
        Rebuild {
            table: "ai_findings",
            create: AI_FINDINGS,
            columns: &[
                "id",
                "book_id",
                "chapter",
                "verse_start",
                "verse_end",
                "agent",
                "request",
                "created_at",
            ],
            autoincrement: true,
        },
    ],
};

/// The reader's bookmarks, version 2.
pub(super) const BOOKMARKS: &str = "CREATE TABLE bookmarks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL CHECK (chapter >= 1),
            verse_start INTEGER NOT NULL CHECK (verse_start >= 1),
            verse_end INTEGER NOT NULL CHECK (verse_end >= verse_start),
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            UNIQUE (book_id, chapter, verse_start, verse_end)
        )";

/// The reader's highlights, version 2: no list of colors.
pub(super) const HIGHLIGHTS: &str = "CREATE TABLE highlights (
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL CHECK (chapter >= 1),
            verse INTEGER NOT NULL CHECK (verse >= 1),
            color TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            PRIMARY KEY (book_id, chapter, verse)
        )";

/// The reader's notes, version 2: no length limit.
pub(super) const NOTES: &str = "CREATE TABLE notes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL CHECK (chapter >= 1),
            verse_start INTEGER NOT NULL CHECK (verse_start >= 1),
            verse_end INTEGER NOT NULL CHECK (verse_end >= verse_start),
            body TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        )";

/// AI-flagged findings, version 2: no length limits.
pub(super) const AI_FINDINGS: &str = "CREATE TABLE ai_findings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL CHECK (chapter >= 1),
            verse_start INTEGER NOT NULL CHECK (verse_start >= 1),
            verse_end INTEGER NOT NULL CHECK (verse_end >= verse_start),
            agent TEXT NOT NULL,
            request TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        )";
