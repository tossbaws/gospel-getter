-- Schema version 1, frozen: the SQL `sqlite_master` holds for each object a
-- new 2.32.2 database has, in the order 2.32.2 creates them. SQLite makes
-- verse_search's own tables (verse_search_data, _idx, _docsize, _config)
-- itself, so they aren't here. Never edit this: `migrate` gives a database
-- at version 0 or 1 whatever of it that database lacks, before any step,
-- and the tests build their version-1 databases from it.

CREATE TABLE books (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            testament TEXT NOT NULL CHECK (testament IN ('OT', 'NT')),
            chapter_count INTEGER NOT NULL
        );

CREATE TABLE translations (
            id INTEGER PRIMARY KEY,
            code TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL
        );

CREATE TABLE verses (
            translation_id INTEGER NOT NULL REFERENCES translations(id),
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL,
            verse INTEGER NOT NULL,
            text TEXT NOT NULL,
            PRIMARY KEY (translation_id, book_id, chapter, verse)
        );

CREATE TABLE reading_position (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            translation_id INTEGER NOT NULL REFERENCES translations(id),
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL
        );

CREATE TABLE reading_place (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            translation_id INTEGER NOT NULL REFERENCES translations(id),
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL,
            verse INTEGER NOT NULL CHECK (verse >= 1),
            offset_px REAL
        );

CREATE TABLE cross_references (
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL,
            verse INTEGER NOT NULL,
            ref_book_id INTEGER NOT NULL REFERENCES books(id),
            ref_chapter INTEGER NOT NULL,
            ref_verse INTEGER NOT NULL,
            ref_end_verse INTEGER,
            score INTEGER NOT NULL
        );

CREATE INDEX idx_cross_references_verse
            ON cross_references (book_id, chapter, verse);

CREATE TABLE bookmarks (
            id INTEGER PRIMARY KEY,
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL CHECK (chapter >= 1),
            verse_start INTEGER NOT NULL CHECK (verse_start >= 1),
            verse_end INTEGER NOT NULL CHECK (verse_end >= verse_start),
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            UNIQUE (book_id, chapter, verse_start, verse_end)
        );

CREATE TABLE highlights (
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL CHECK (chapter >= 1),
            verse INTEGER NOT NULL CHECK (verse >= 1),
            color TEXT NOT NULL CHECK (color IN ('yellow', 'green', 'blue', 'pink')),
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            PRIMARY KEY (book_id, chapter, verse)
        );

CREATE TABLE notes (
            id INTEGER PRIMARY KEY,
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL CHECK (chapter >= 1),
            verse_start INTEGER NOT NULL CHECK (verse_start >= 1),
            verse_end INTEGER NOT NULL CHECK (verse_end >= verse_start),
            body TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 10000),
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );

CREATE INDEX idx_notes_chapter ON notes (book_id, chapter);

CREATE TABLE ai_findings (
            id INTEGER PRIMARY KEY,
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL CHECK (chapter >= 1),
            verse_start INTEGER NOT NULL CHECK (verse_start >= 1),
            verse_end INTEGER NOT NULL CHECK (verse_end >= verse_start),
            agent TEXT NOT NULL CHECK (length(agent) BETWEEN 1 AND 60),
            request TEXT NOT NULL CHECK (length(request) BETWEEN 1 AND 60),
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );

CREATE UNIQUE INDEX idx_ai_findings_unique ON ai_findings (book_id, chapter, verse_start, verse_end, request);

CREATE INDEX idx_ai_findings_chapter ON ai_findings (book_id, chapter);

CREATE INDEX idx_ai_findings_request ON ai_findings (request);

CREATE TABLE app_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

CREATE TABLE footnotes (
            translation_id INTEGER NOT NULL REFERENCES translations(id),
            book_id INTEGER NOT NULL REFERENCES books(id),
            chapter INTEGER NOT NULL,
            verse INTEGER NOT NULL,
            ordinal INTEGER NOT NULL,
            position INTEGER NOT NULL CHECK (position >= 0),
            usfm TEXT NOT NULL,
            PRIMARY KEY (translation_id, book_id, chapter, verse, ordinal)
        );

CREATE VIRTUAL TABLE verse_search USING fts5(
            text,
            content = '',
            contentless_delete = 1
        );

CREATE TABLE search_index_state (
            translation_id INTEGER PRIMARY KEY,
            verse_count INTEGER NOT NULL
        );

CREATE TABLE search_index_version (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            version INTEGER NOT NULL
        );

CREATE TABLE translation_data_version (
            translation_id INTEGER PRIMARY KEY,
            version INTEGER NOT NULL CHECK (version >= 1)
        );
