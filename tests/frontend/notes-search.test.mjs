// The Library's Notes tab's search: a box at its top filters the list as
// it's typed (once typing pauses), on the notes the Library already loaded,
// says how many of how many it shows, marks what it found, and clears with
// Escape — against the real backend and database. Export notes… keeps its
// own filters.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, closeBridge, openApp, settle } from './harness.mjs';

after(closeBridge);

const JOHN = bookId('John');

const NOTES = [
    [bookId('Genesis'), 1, 1, 1, 'In the beginning God created.'],
    [JOHN, 3, 16, 18, 'God so loved the world.'],
    [JOHN, 3, 21, 21, 'Light: coming to it is the whole of it.'],
    [bookId('Acts'), 19, 1, 1, 'Paul came to Éphèse.'],
    [bookId('Romans'), 5, 8, 8, 'God commendeth his love toward us.'],
    [bookId('1 Corinthians'), 13, 4, 4, 'Charity suffereth long.'],
];

/** The notes above (or `notes`), then the app, in `book` `chapter` (Psalms 1: no notes). */
async function withNotes(body, { notes = NOTES, book = 'Psalms', chapter = 1 } = {}) {
    await backend('__reset', { bookId: JOHN, chapter: 3, translationCode: 'kjv' });
    for (const [bookIdOf, ch, verseStart, verseEnd, text] of notes) {
        await backend('add_note', { bookId: bookIdOf, chapter: ch, verseStart, verseEnd, body: text });
    }
    const app = await openApp({ book, chapter, keepBookmarks: true });
    try {
        app.openLibrary('notes');
        await body(app);
    } finally {
        app.close();
    }
}

const el = (app, id) => app.document.getElementById(id);
const box = (app) => el(app, 'note-search');
const countLine = (app) => el(app, 'note-search-count').textContent;
const references = (app) => [...app.document.querySelectorAll('#note-list .bookmark-item .bookmark-ref')].map((r) => r.textContent);
const marks = (app) => [...app.document.querySelectorAll('#note-list mark')].map((m) => m.textContent);
const listText = (app) => el(app, 'note-list').textContent.trim();

/** How long the list waits for typing to pause (`library_text::SEARCH_DELAY_MS`). */
const SEARCH_DELAY_MS = 150;

/** Types `query` into the box, as a reader would, and waits for the list to follow. */
async function search(app, query) {
    app.type(box(app), query);
    await new Promise((resolve) => setTimeout(resolve, SEARCH_DELAY_MS + 50));
    await settle();
}

test('typing filters the list once typing pauses, says how many of how many, and asks nothing of the backend', () =>
    withNotes(async (app) => {
        assert.equal(references(app).length, 6);
        assert.equal(countLine(app), '', 'nothing to say before a search');
        const before = app.calls.length;
        app.type(box(app), 'love');
        // Not at once: the list waits for the typing to pause.
        await settle();
        assert.equal(references(app).length, 6);
        await app.until(() => references(app).length === 2, 'the list to filter');
        assert.deepEqual(references(app), ['John 3:16–18', 'Romans 5:8']);
        assert.equal(countLine(app), '2 of 6 notes');
        assert.equal(el(app, 'note-search-count').getAttribute('aria-live'), 'polite');
        // The list already loaded: no request of its own.
        assert.deepEqual(app.calls.slice(before).map((c) => c.cmd), []);
        // The tab still counts every note.
        assert.equal(el(app, 'note-count').textContent, '6');

        // Every word must be found.
        await search(app, 'love world');
        assert.deepEqual(references(app), ['John 3:16–18']);
        await search(app, 'god');
        assert.deepEqual(references(app), ['Genesis 1:1', 'John 3:16–18', 'Romans 5:8']);
        assert.equal(countLine(app), '3 of 6 notes');
        // Emptied: every note again, nothing to say.
        await search(app, '');
        assert.equal(references(app).length, 6);
        assert.equal(countLine(app), '');
    }));

test('a note is found by its reference: the book, its usual abbreviations, the chapter and the verses', () =>
    withNotes(async (app) => {
        await search(app, 'Rom');
        assert.deepEqual(references(app), ['Romans 5:8']);
        await search(app, '1 cor 13');
        assert.deepEqual(references(app), ['1 Corinthians 13:4']);
        await search(app, 'Jn 3:17');
        assert.deepEqual(references(app), ['John 3:16–18'], '17 is one of 16–18; Jn is John');
        await search(app, 'john light');
        assert.deepEqual(references(app), ['John 3:21']);
        await search(app, 'Mk');
        assert.deepEqual(references(app), []);
    }));

test('case and accents do not matter, and what was found is marked', () =>
    withNotes(async (app) => {
        await search(app, 'EPHESE');
        assert.deepEqual(references(app), ['Acts 19:1']);
        assert.deepEqual(marks(app), ['Éphèse']);
        await search(app, 'lov gen');
        assert.deepEqual(references(app), []);
        await search(app, 'lov');
        assert.deepEqual(marks(app), ['lov', 'lov']);
        // The marks are the search's own, in the row, around the note's own
        // characters: the text reads as before.
        const preview = app.document.querySelector('#note-list .note-preview');
        assert.equal(preview.textContent, 'God so loved the world.');
        assert.ok(preview.querySelector('mark').classList.contains('note-search-mark'));
        await search(app, 'rom 5');
        assert.deepEqual(marks(app), ['Rom', '5']);
        assert.equal(app.document.querySelector('#note-list .bookmark-ref').textContent, 'Romans 5:8');
    }));

test('a search that finds nothing says so plainly', () =>
    withNotes(async (app) => {
        await search(app, '  zzz  ');
        assert.equal(listText(app), 'No notes match “zzz”.');
        assert.equal(countLine(app), '0 of 6 notes');
    }));

test('Escape in the box clears it; the next Escape closes the Library, as before', () =>
    withNotes(async (app) => {
        await search(app, 'love');
        const escape = app.key('Escape', {}, box(app));
        assert.ok(escape.defaultPrevented, 'the Escape was the box’s');
        await settle();
        assert.equal(box(app).value, '');
        assert.equal(references(app).length, 6, 'at once, no pause');
        assert.equal(countLine(app), '');
        assert.ok(!app.surface('library').hidden, 'the Library stays open');
        app.key('Escape', {}, box(app));
        await settle();
        assert.ok(app.surface('library').hidden, 'then Escape is the page’s again');
    }));

test('the box is labelled, keyboard reachable before Export notes…, and off without notes', () =>
    withNotes(async (app) => {
        const input = box(app);
        assert.equal(input.type, 'search');
        assert.equal(input.getAttribute('aria-label'), 'Search notes');
        assert.equal(input.getAttribute('aria-controls'), 'note-list');
        assert.equal(input.getAttribute('aria-describedby'), 'note-search-count');
        assert.ok(!input.disabled);
        assert.ok(input.tabIndex === 0);
        // At the top of the tab, then Export notes…, then the rows.
        const stops = [...el(app, 'library-notes').querySelectorAll('input, button')];
        assert.equal(stops[0], input);
        assert.equal(stops[1], el(app, 'export-notes-btn'));
        // A letter typed in it is a letter, not a shortcut.
        app.key('n', {}, input);
        await settle();
        assert.ok(el(app, 'note-overlay').hidden);
    }).then(() => withNotes(async (app) => {
        assert.ok(box(app).disabled);
    }, { notes: [] })));

test('the search is kept while the list reloads, and follows a note edited from it', () =>
    withNotes(async (app) => {
        await search(app, 'love');
        assert.deepEqual(references(app), ['John 3:16–18', 'Romans 5:8']);
        // Edit Romans 5:8 from the filtered list; its new words no longer match.
        const before = app.calls.length;
        app.click(app.document.querySelectorAll('#note-list .note-list-edit')[1]);
        await app.idle(before, 'list_notes');
        assert.equal(el(app, 'note-input').value, 'God commendeth his love toward us.');
        app.type(el(app, 'note-input'), 'While we were yet sinners.');
        await settle();
        const saving = app.calls.length;
        app.click(el(app, 'note-save'));
        await app.idle(saving, 'list_all_notes');
        app.openLibrary('notes');
        assert.equal(box(app).value, 'love');
        assert.deepEqual(references(app), ['John 3:16–18']);
        assert.equal(countLine(app), '1 of 6 notes');
    }, { book: 'Romans', chapter: 5 }));

test('Export notes… keeps its own filters, whatever the search shows', () =>
    withNotes(async (app) => {
        await search(app, 'charity');
        assert.deepEqual(references(app), ['1 Corinthians 13:4']);
        const before = app.calls.length;
        app.click(el(app, 'export-notes-btn'));
        await app.idle(before, 'count_notes_export');
        // Reading Psalms, which has no notes: every book, every note.
        assert.equal(el(app, 'export-notes-count').textContent, '6 notes match');
        const filter = app.calls.filter((c) => c.cmd === 'count_notes_export').at(-1).args.filter;
        assert.deepEqual(JSON.parse(JSON.stringify(filter)), { books: [], chapters: null, writtenFrom: null, writtenTo: null });
    }));
