-- Books are seeded in canonical order (Genesis = 1 ... Revelation = 66), so
-- `id` alone is enough to sort/traverse the whole Bible; no separate
-- ordering column is needed. Book structure (name, testament, chapter
-- count) is shared across translations; only verse text is per-translation.
CREATE TABLE IF NOT EXISTS books (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    testament TEXT NOT NULL CHECK (testament IN ('OT', 'NT')),
    chapter_count INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS translations (
    id INTEGER PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS verses (
    translation_id INTEGER NOT NULL REFERENCES translations(id),
    book_id INTEGER NOT NULL REFERENCES books(id),
    chapter INTEGER NOT NULL,
    verse INTEGER NOT NULL,
    text TEXT NOT NULL,
    PRIMARY KEY (translation_id, book_id, chapter, verse)
);

-- Single-row table (id is always 1) remembering the last chapter read, so
-- the app can reopen where the reader left off.
CREATE TABLE IF NOT EXISTS reading_position (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    translation_id INTEGER NOT NULL REFERENCES translations(id),
    book_id INTEGER NOT NULL REFERENCES books(id),
    chapter INTEGER NOT NULL
);

-- Cross-references are citations between passages, not text, so they're
-- translation-independent — keyed only by book/chapter/verse.
-- `ref_end_verse` is set when the reference is to a verse range.
CREATE TABLE IF NOT EXISTS cross_references (
    book_id INTEGER NOT NULL REFERENCES books(id),
    chapter INTEGER NOT NULL,
    verse INTEGER NOT NULL,
    ref_book_id INTEGER NOT NULL REFERENCES books(id),
    ref_chapter INTEGER NOT NULL,
    ref_verse INTEGER NOT NULL,
    ref_end_verse INTEGER,
    score INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cross_references_verse
    ON cross_references (book_id, chapter, verse);
