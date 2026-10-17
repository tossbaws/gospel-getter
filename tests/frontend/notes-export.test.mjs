// Export notes…: the notes chosen by book, chapters and when they were
// written, as a readable Markdown file with each note's verses quoted in
// the translation being read. A button at the top of the Library's Notes
// tab opens a small modal; the real backend offers the choices, counts
// what they match, and builds and writes the file (the save dialog is
// scripted, and the export's clock can be fixed).

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { backend, bookId, closeBridge, openApp, settle, storedVerse } from './harness.mjs';

const dir = mkdtempSync(join(tmpdir(), 'gospel-getter-notes-export-'));
after(async () => {
    await closeBridge();
    rmSync(dir, { recursive: true, force: true });
});

const JOHN = bookId('John');
const PSALMS = bookId('Psalms');
const FIRST_JOHN = bookId('1 John');

/** Opens the app at `book` `chapter` with `notes` ([book, chapter, start, end, body]) already written. */
async function withNotes(notes, { book = 'John', chapter = 3, ...options }, body) {
    await backend('__reset', { bookId: bookId(book), chapter, translationCode: options.translation || 'kjv' });
    for (const [id, ch, verseStart, verseEnd, text] of notes) {
        await backend('add_note', { bookId: id, chapter: ch, verseStart, verseEnd, body: text });
    }
    const app = await openApp({ book, chapter, keepBookmarks: true, ...options });
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const SAMPLE = [
    [PSALMS, 23, 1, 2, 'The shepherd.'],
    [JOHN, 3, 16, 16, 'Loved.'],
    [JOHN, 1, 1, 5, 'The Word.'],
    [FIRST_JOHN, 1, 9, 9, 'Confess.'],
    [JOHN, 3, 16, 18, 'Believe.'],
];

const el = (app, id) => app.document.getElementById(id);
const button = (app) => el(app, 'export-notes-btn');
const dialogOpen = (app) => !el(app, 'export-notes-overlay').hidden;
const boxes = (app) => [...el(app, 'export-notes-books').querySelectorAll('input[type="checkbox"]')];
const box = (app, id) => boxes(app).find((b) => Number(b.value) === id);
/** The checklist: [book id, its row's text, ticked]. */
const bookRows = (app) => boxes(app).map((b) => [Number(b.value), b.closest('label').textContent, b.checked]);
const ticked = (app) => boxes(app).filter((b) => b.checked).map((b) => Number(b.value));
const countLine = (app) => el(app, 'export-notes-count').textContent;
const errorLine = (app) => el(app, 'export-notes-error').textContent;
const confirm = (app) => el(app, 'export-notes-confirm');
const chaptersShown = (app) => !el(app, 'export-notes-chapters').hidden;
const chapterFields = (app) => [el(app, 'export-notes-chapter-from').value, el(app, 'export-notes-chapter-to').value];
// The page's own objects, as plain ones (to compare with deepEqual).
const argsOf = (app, cmd) => app.calls.filter((c) => c.cmd === cmd).map((c) => JSON.parse(JSON.stringify(c.args)));
const exportCalls = (app) => argsOf(app, 'export_notes');
const lastFilter = (app) => argsOf(app, 'count_notes_export').at(-1).filter;
const EVERY = { books: [], chapters: null, writtenFrom: null, writtenTo: null };

async function clickAndWait(app, target, cmd) {
    const before = app.calls.length;
    app.click(target);
    await app.idle(before, cmd);
}

/** Opens the dialog, and waits for its first count. */
async function openDialog(app) {
    app.openLibrary('notes');
    await clickAndWait(app, button(app), 'count_notes_export');
    assert.ok(dialogOpen(app));
}

/** Sets a field (or the Written list) as the reader would, and waits for the count. */
async function setField(app, id, value) {
    const before = app.calls.length;
    const field = el(app, id);
    field.value = value;
    field.dispatchEvent(new app.window.Event('input', { bubbles: true }));
    field.dispatchEvent(new app.window.Event('change', { bubbles: true }));
    await app.idle(before, 'count_notes_export');
}

/** Ticks or unticks a book (by clicking it), and lets the count settle. */
async function toggle(app, id) {
    const before = app.calls.length;
    app.click(box(app, id));
    await app.idle(before);
}

/** Today is Oct 12, 2026, at -06:00, for the export's dates and presets. */
const OCT_12 = () => backend('__set_clock', { now: '2026-10-12T18:00:00Z', offsetMinutes: -360 });
const written = (body, createdAt) => backend('__note_written', { body, createdAt });

test('the button is at the top of the Notes tab, and disabled without notes', () =>
    withNotes([], {}, async (app) => {
        app.openLibrary('notes');
        const notes = el(app, 'library-notes');
        assert.equal(notes.firstElementChild, button(app));
        assert.equal(button(app).textContent, 'Export notes…');
        assert.ok(button(app).disabled);
        app.click(button(app));
        assert.ok(!dialogOpen(app));
    }));

test('the button follows the notes live: add, delete, undo and import', () =>
    withNotes([], {}, async (app) => {
        assert.ok(button(app).disabled);

        // Add one through the note panel.
        const before = app.calls.length;
        app.click(app.verse(16));
        await app.idle(before, 'list_notes');
        app.click(app.actions().querySelector('.verse-note-btn'));
        app.type(el(app, 'note-input'), 'Added.');
        await settle();
        await clickAndWait(app, el(app, 'note-save'), 'list_all_notes');
        assert.ok(!button(app).disabled, 'enabled once there is a note');

        // Delete it, then undo.
        app.click(app.document.querySelector('.verse-notes .note-edit'));
        await clickAndWait(app, el(app, 'note-delete'), 'list_all_notes');
        assert.ok(button(app).disabled, 'disabled once it is deleted');
        await clickAndWait(app, app.status().querySelector('.status-undo'), 'list_all_notes');
        assert.ok(!button(app).disabled, 'enabled again on undo');

        // Delete again, and import one back.
        app.click(app.document.querySelector('.verse-notes .note-edit'));
        await clickAndWait(app, el(app, 'note-delete'), 'list_all_notes');
        assert.ok(button(app).disabled);
        const path = join(dir, 'import.gospel-getter.json');
        writeFileSync(path, JSON.stringify({
            format: 'gospel-getter-reader-data',
            format_version: 3,
            exported_at: '2026-10-10T00:00:00.000Z',
            app_version: '2.8.0',
            reading_position: null,
            bookmarks: [],
            notes: [{ book: JOHN, chapter: 3, verse_start: 17, verse_end: 17, body: 'Imported.' }],
        }));
        await app.pick(path);
        app.open('menu');
        await clickAndWait(app, el(app, 'import-data-btn'), 'choose_import_file');
        await clickAndWait(app, el(app, 'import-confirm'), 'list_all_notes');
        assert.ok(!button(app).disabled, 'enabled after the import');
    }));

test('the checklist lists only books with notes, in Bible order with counts, and ticks just the book being read', () =>
    withNotes(SAMPLE, {}, async (app) => {
        await openDialog(app);
        const panel = el(app, 'export-notes-panel');
        assert.equal(panel.getAttribute('role'), 'dialog');
        assert.equal(panel.getAttribute('aria-modal'), 'true');
        assert.equal(el(app, panel.getAttribute('aria-labelledby')).textContent, 'Export notes');
        assert.equal(el(app, panel.getAttribute('aria-describedby')).textContent,
            'A Markdown file of your notes, each with its verses quoted in the King James Version.');
        // A fieldset with a legend, operated with ordinary checkboxes.
        const list = el(app, 'export-notes-books').closest('fieldset');
        assert.equal(list.querySelector('legend').textContent, 'Books');
        assert.deepEqual(bookRows(app), [
            [PSALMS, 'Psalms (1)', false],
            [JOHN, 'John (3)', true],
            [FIRST_JOHN, '1 John (1)', false],
        ]);
        assert.equal(app.document.activeElement, box(app, JOHN));
        // The match line is the backend's count, announced politely.
        assert.equal(el(app, 'export-notes-count').getAttribute('aria-live'), 'polite');
        assert.equal(el(app, 'export-notes-error').getAttribute('role'), 'alert');
        assert.equal(countLine(app), '3 notes match');
        assert.equal(errorLine(app), '');
        assert.ok(!confirm(app).disabled);
        assert.deepEqual(lastFilter(app), { ...EVERY, books: [JOHN] });
    }));

test('reading a book without notes, every book is ticked', () =>
    withNotes(SAMPLE, { book: 'Genesis', chapter: 1 }, async (app) => {
        await openDialog(app);
        assert.deepEqual(ticked(app), [PSALMS, JOHN, FIRST_JOHN]);
        assert.equal(app.document.activeElement, box(app, PSALMS));
        assert.ok(!chaptersShown(app));
        // Every book is no book list at all.
        assert.deepEqual(lastFilter(app), EVERY);
        assert.equal(countLine(app), '5 notes match');
    }));

test('All and None tick every book or none; none matches nothing, and Export is off', () =>
    withNotes(SAMPLE, {}, async (app) => {
        await openDialog(app);
        const counts = () => argsOf(app, 'count_notes_export').length;
        const before = counts();
        app.click(el(app, 'export-notes-none'));
        await settle();
        assert.deepEqual(ticked(app), []);
        // An empty book list would be every book, so it isn't asked.
        assert.equal(counts(), before);
        assert.equal(countLine(app), 'No notes match');
        assert.ok(confirm(app).disabled);
        assert.ok(!chaptersShown(app));

        await clickAndWait(app, el(app, 'export-notes-all'), 'count_notes_export');
        assert.deepEqual(ticked(app), [PSALMS, JOHN, FIRST_JOHN]);
        assert.equal(countLine(app), '5 notes match');
        assert.ok(!confirm(app).disabled);

        await toggle(app, JOHN);
        assert.deepEqual(lastFilter(app), { ...EVERY, books: [PSALMS, FIRST_JOHN] });
        assert.equal(countLine(app), '2 notes match');
        await toggle(app, PSALMS);
        assert.equal(countLine(app), '1 note matches');
    }));

test('the chapter fields show only with exactly one book, starting on its chapters with notes', () =>
    withNotes(SAMPLE, {}, async (app) => {
        await openDialog(app);
        // John's notes are on chapters 1 and 3; it has 21.
        assert.ok(chaptersShown(app));
        assert.deepEqual(chapterFields(app), ['1', '3']);
        assert.equal(el(app, 'export-notes-chapter-from').max, '21');
        assert.equal(el(app, 'export-notes-chapters').querySelector('legend').textContent, 'Chapters');
        // A range around all of them is no range.
        assert.deepEqual(lastFilter(app), { ...EVERY, books: [JOHN] });

        await setField(app, 'export-notes-chapter-from', '3');
        assert.deepEqual(lastFilter(app), { ...EVERY, books: [JOHN], chapters: [3, 3] });
        assert.equal(countLine(app), '2 notes match');
        await setField(app, 'export-notes-chapter-to', '2');
        assert.equal(errorLine(app), 'Chapter 3 is after chapter 2.');
        assert.equal(countLine(app), '');
        assert.ok(confirm(app).disabled);
        // Held to the book's chapters once left.
        await setField(app, 'export-notes-chapter-from', '0');
        await setField(app, 'export-notes-chapter-to', '99');
        assert.deepEqual(chapterFields(app), ['1', '21']);
        assert.equal(errorLine(app), '');
        assert.equal(countLine(app), '3 notes match');

        // Two books: hidden, and ignored.
        await toggle(app, PSALMS);
        assert.ok(!chaptersShown(app));
        assert.deepEqual(lastFilter(app), { ...EVERY, books: [PSALMS, JOHN] });
        // One again, another book: its own chapters, but Psalms' notes are
        // all in Psalm 23, so there's no range to choose.
        await toggle(app, JOHN);
        assert.ok(!chaptersShown(app));
        assert.deepEqual(chapterFields(app), ['23', '23']);
        assert.deepEqual(lastFilter(app), { ...EVERY, books: [PSALMS] });
        app.click(el(app, 'export-notes-none'));
        await settle();
        assert.ok(!chaptersShown(app));
    }));

test('the chapter range hides when the one ticked book has notes in only one chapter', () =>
    withNotes([[FIRST_JOHN, 1, 9, 9, 'Confess.'], [FIRST_JOHN, 1, 7, 7, 'Light.'], [JOHN, 3, 16, 16, 'Loved.'], [JOHN, 5, 24, 24, 'Life.']], { book: '1 John', chapter: 1 }, async (app) => {
        await openDialog(app);
        // The dialog opens on the book being read: 1 John, notes in
        // chapter 1 only (two of them).
        assert.deepEqual(ticked(app), [FIRST_JOHN]);
        assert.ok(!chaptersShown(app));
        assert.deepEqual(lastFilter(app), { ...EVERY, books: [FIRST_JOHN] });
        assert.equal(countLine(app), '2 notes match');
        // John has two chapters with notes: the range is back.
        await toggle(app, FIRST_JOHN);
        await toggle(app, JOHN);
        assert.ok(chaptersShown(app));
        assert.deepEqual(chapterFields(app), ['3', '5']);
        // And gone again for 1 John.
        await toggle(app, JOHN);
        await toggle(app, FIRST_JOHN);
        assert.ok(!chaptersShown(app));
        await clickAndWait(app, confirm(app), 'export_notes');
        assert.deepEqual(exportCalls(app).at(-1).filter, { ...EVERY, books: [FIRST_JOHN] });
    }));

test('the Written presets are local days ending today, and Custom shows From and To', () =>
    withNotes(SAMPLE, {}, async (app) => {
        await OCT_12();
        await written('Loved.', '2026-10-12T15:00:00.000Z');
        await written('Believe.', '2026-09-20T15:00:00.000Z');
        await written('The Word.', '2025-12-31T15:00:00.000Z');
        await openDialog(app);
        const when = el(app, 'export-notes-when');
        assert.deepEqual([...when.options].map((o) => o.textContent),
            ['All time', 'Last 7 days', 'Last 30 days', 'This year', 'Custom']);
        assert.equal(when.value, 'all');
        assert.ok(el(app, 'export-notes-custom').hidden);
        assert.equal(countLine(app), '3 notes match');
        for (const [preset, writtenFrom, count] of [
            ['last7Days', '2026-10-06', '1 note matches'],
            ['last30Days', '2026-09-13', '2 notes match'],
            ['thisYear', '2026-01-01', '2 notes match'],
        ]) {
            await setField(app, 'export-notes-when', preset);
            assert.deepEqual(lastFilter(app), { ...EVERY, books: [JOHN], writtenFrom, writtenTo: '2026-10-12' }, preset);
            assert.equal(countLine(app), count, preset);
            assert.ok(el(app, 'export-notes-custom').hidden);
        }
        await setField(app, 'export-notes-when', 'all');
        assert.deepEqual(lastFilter(app), { ...EVERY, books: [JOHN] });

        // Custom: both ends optional.
        await setField(app, 'export-notes-when', 'custom');
        assert.ok(!el(app, 'export-notes-custom').hidden);
        assert.equal(el(app, 'export-notes-from').type, 'date');
        assert.deepEqual(lastFilter(app), { ...EVERY, books: [JOHN] });
        await setField(app, 'export-notes-from', '2026-01-01');
        assert.deepEqual(lastFilter(app), { ...EVERY, books: [JOHN], writtenFrom: '2026-01-01' });
        assert.equal(countLine(app), '2 notes match');
        await setField(app, 'export-notes-to', '2026-09-30');
        assert.deepEqual(lastFilter(app), { ...EVERY, books: [JOHN], writtenFrom: '2026-01-01', writtenTo: '2026-09-30' });
        assert.equal(countLine(app), '1 note matches');
    }));

test('Custom From after To is an announced error, and Export is off until it is fixed', () =>
    withNotes(SAMPLE, {}, async (app) => {
        await openDialog(app);
        await setField(app, 'export-notes-when', 'custom');
        await setField(app, 'export-notes-from', '2026-10-12');
        await setField(app, 'export-notes-to', '2026-10-01');
        assert.equal(errorLine(app), 'The From date is after the To date.');
        assert.equal(el(app, 'export-notes-error').getAttribute('role'), 'alert');
        assert.equal(el(app, 'export-notes-from').getAttribute('aria-describedby'), 'export-notes-error');
        assert.equal(countLine(app), '');
        assert.ok(confirm(app).disabled);
        // Clicking it anyway does nothing.
        app.click(confirm(app));
        await settle();
        assert.deepEqual(exportCalls(app), []);
        assert.ok(dialogOpen(app));

        await setField(app, 'export-notes-to', '');
        assert.equal(errorLine(app), '');
        assert.ok(countLine(app).endsWith('match') || countLine(app).endsWith('matches'));
    }));

test('choices that match no notes say so, and Export is off', () =>
    withNotes(SAMPLE, {}, async (app) => {
        await OCT_12();
        for (const [, , , , text] of SAMPLE) await written(text, '2026-10-01T15:00:00.000Z');
        await openDialog(app);
        await setField(app, 'export-notes-when', 'last7Days');
        assert.equal(countLine(app), 'No notes match');
        assert.equal(errorLine(app), '');
        assert.ok(confirm(app).disabled);
        await setField(app, 'export-notes-when', 'last30Days');
        assert.equal(countLine(app), '3 notes match');
        assert.ok(!confirm(app).disabled);
    }));

test('it opens fresh every time: just the book being read, All time, nothing kept', () =>
    withNotes(SAMPLE, {}, async (app) => {
        await openDialog(app);
        await clickAndWait(app, el(app, 'export-notes-all'), 'count_notes_export');
        await setField(app, 'export-notes-when', 'custom');
        await setField(app, 'export-notes-from', '2026-01-01');
        app.click(el(app, 'export-notes-cancel'));
        assert.ok(!dialogOpen(app));

        await clickAndWait(app, button(app), 'count_notes_export');
        assert.deepEqual(ticked(app), [JOHN]);
        assert.ok(chaptersShown(app));
        assert.deepEqual(chapterFields(app), ['1', '3']);
        assert.equal(el(app, 'export-notes-when').value, 'all');
        assert.ok(el(app, 'export-notes-custom').hidden);
        assert.equal(el(app, 'export-notes-from').value, '');
        assert.deepEqual(lastFilter(app), { ...EVERY, books: [JOHN] });
        assert.equal(countLine(app), '3 notes match');
    }));

test('book names and counts are shown as text, never markup', () =>
    withNotes(SAMPLE, {}, async (app) => {
        await openDialog(app);
        app.click(el(app, 'export-notes-cancel'));
    }).then(() => withNotes(SAMPLE, {
        alter: {
            get_notes_export_options: (options) => ({
                ...options,
                books: options.books.map((b) => (b.id === JOHN
                    ? { ...b, name: '<img src=x onerror="window.__pwned=1">J&amp;hn', notes: 1234 }
                    : b)),
            }),
        },
    }, async (app) => {
        await openDialog(app);
        const row = box(app, JOHN).closest('label');
        assert.equal(row.textContent, '<img src=x onerror="window.__pwned=1">J&amp;hn (1,234)');
        assert.equal(row.querySelector('img'), null);
        assert.equal(app.window.__pwned, undefined);
    })));

test('Export sends the translation and the filter, and reports the file', () =>
    withNotes(SAMPLE, {}, async (app) => {
        await OCT_12();
        const path = join(dir, 'john.md');
        await app.pick(path);
        await openDialog(app);
        await clickAndWait(app, confirm(app), 'export_notes');
        assert.deepEqual(exportCalls(app), [{ translationCode: 'kjv', filter: { ...EVERY, books: [JOHN] } }]);
        assert.ok(!dialogOpen(app));
        assert.equal(app.toast().textContent, `Exported 3 notes on John to ${path}.`);
        assert.ok(!app.toast().classList.contains('is-error'));
        const file = readFileSync(path, 'utf8');
        assert.ok(file.startsWith('# Notes on John\n\nExported Oct 12, 2026 · Verse text: King James Version · 3 notes\n\n## John 1:1'), file);
        assert.ok(file.includes(`## John 3:16\n\n> **16** ${storedVerse('kjv', 'John', 3, 16)}\n\nLoved.\n`));
        assert.ok(!file.includes('Confess.'));

        // One book's chapters, and dates.
        const range = join(dir, 'john-3.md');
        await app.pick(range);
        await clickAndWait(app, button(app), 'count_notes_export');
        await setField(app, 'export-notes-chapter-from', '3');
        await setField(app, 'export-notes-when', 'custom');
        await setField(app, 'export-notes-from', '2026-10-01');
        await clickAndWait(app, confirm(app), 'export_notes');
        assert.deepEqual(exportCalls(app)[1].filter, { books: [JOHN], chapters: [3, 3], writtenFrom: '2026-10-01', writtenTo: null });
        assert.equal(app.toast().textContent, `Exported 2 notes on John to ${range}.`);
        assert.ok(readFileSync(range, 'utf8').startsWith(
            '# Notes on John 3\n\nExported Oct 12, 2026 · Verse text: King James Version · 2 notes\n\nWritten since Oct 1, 2026\n\n## John 3:16'));

        // Several books, each under its heading.
        const two = join(dir, 'two.md');
        await app.pick(two);
        await clickAndWait(app, button(app), 'count_notes_export');
        await toggle(app, FIRST_JOHN);
        await clickAndWait(app, confirm(app), 'export_notes');
        assert.deepEqual(exportCalls(app)[2].filter, { ...EVERY, books: [JOHN, FIRST_JOHN] });
        assert.equal(app.toast().textContent, `Exported 4 notes to ${two}.`);
        const both = readFileSync(two, 'utf8');
        assert.ok(both.startsWith('# Notes on John and 1 John\n'), both);
        assert.ok(both.includes('\n## John\n\n### John 1:1') && both.includes('\n## 1 John\n\n### 1 John 1:9'));

        // Every book: the file as before.
        const all = join(dir, 'all.md');
        await app.pick(all);
        await clickAndWait(app, button(app), 'count_notes_export');
        await clickAndWait(app, el(app, 'export-notes-all'), 'count_notes_export');
        await clickAndWait(app, confirm(app), 'export_notes');
        assert.deepEqual(exportCalls(app)[3], { translationCode: 'kjv', filter: EVERY });
        assert.equal(app.toast().textContent, `Exported 5 notes to ${all}.`);
        assert.ok(readFileSync(all, 'utf8').startsWith('# Gospel Getter notes\n'));
    }));

test('in compare mode the verses come from the translation chosen in Aa, not the other column', () =>
    withNotes([[bookId('Matthew'), 2, 23, 23, 'Nazareth.']], { book: 'Matthew', chapter: 2, translation: 'web', compare: true }, async (app) => {
        assert.ok(app.isComparing());
        // The WEB, chosen in Aa, is the left column; the KJV is beside it.
        assert.deepEqual(app.compareColumns(), ['World English Bible (WEB)', 'King James Version (KJV)']);
        assert.equal(app.document.getElementById('translation-select').value, 'web');
        const path = join(dir, 'web.md');
        await app.pick(path);
        await openDialog(app);
        assert.ok(el(app, 'export-notes-text').textContent.endsWith('quoted in the World English Bible.'));
        await clickAndWait(app, confirm(app), 'export_notes');
        // Matthew is the only book with notes, and its only chapter: every note.
        assert.deepEqual(exportCalls(app), [{ translationCode: 'web', filter: EVERY }]);
        const file = readFileSync(path, 'utf8');
        assert.ok(file.includes(`> **23** ${storedVerse('web', 'Matthew', 2, 23)}\n`), file);
    }));

test('cancelling the save dialog says nothing; a failure says why', () =>
    withNotes(SAMPLE, {}, async (app) => {
        await app.pick(null);
        await openDialog(app);
        await clickAndWait(app, confirm(app), 'export_notes');
        assert.equal(exportCalls(app).length, 1);
        assert.equal(app.toast().textContent, '');
        assert.ok(app.toast().classList.contains('is-hidden'));

        app.failCommand('export_notes', 'No notes match these filters.');
        await clickAndWait(app, button(app), 'count_notes_export');
        await clickAndWait(app, confirm(app), 'export_notes');
        assert.equal(app.toast().textContent, 'Export failed: No notes match these filters.');
        assert.ok(app.toast().classList.contains('is-error'));

        // If the choices can't be worked out, it says so, and doesn't open.
        app.failCommand('get_notes_export_options', 'Your notes couldn’t be read for export.');
        await clickAndWait(app, button(app), 'get_notes_export_options');
        assert.ok(!dialogOpen(app));
        assert.equal(app.toast().textContent, 'Export failed: Your notes couldn’t be read for export.');
    }));

test('Escape and Cancel close it and give focus back; it keeps the keyboard while open', () =>
    withNotes(SAMPLE, {}, async (app) => {
        await openDialog(app);
        app.click(el(app, 'export-notes-cancel'));
        assert.ok(!dialogOpen(app));
        assert.equal(app.document.activeElement, button(app));

        await clickAndWait(app, button(app), 'count_notes_export');
        // Tab and Shift+Tab wrap: All, None, each book, the chapters, Written, Export, Cancel.
        const tab = (shiftKey) => app.key('Tab', { shiftKey }, app.document.activeElement).defaultPrevented;
        assert.equal(app.document.activeElement, box(app, JOHN));
        app.document.getElementById('export-notes-all').focus();
        assert.ok(tab(true));
        assert.equal(app.document.activeElement, el(app, 'export-notes-cancel'));
        assert.ok(tab(false));
        assert.equal(app.document.activeElement, el(app, 'export-notes-all'));
        app.document.activeElement.blur();
        assert.ok(tab(false));
        assert.equal(app.document.activeElement, el(app, 'export-notes-all'));
        // With nothing matching, Export is skipped: Cancel is still last.
        app.click(el(app, 'export-notes-none'));
        await settle();
        el(app, 'export-notes-cancel').focus();
        assert.ok(tab(false));
        assert.equal(app.document.activeElement, el(app, 'export-notes-all'));

        // None of the reader's shortcuts run.
        const target = box(app, JOHN);
        for (const key of ['c', '/', 'ArrowRight', 'ArrowLeft', 'n']) app.key(key, {}, target);
        app.key('b', { ctrlKey: true }, target);
        app.key('k', { ctrlKey: true }, target);
        await settle();
        assert.equal(app.heading(), 'John 3');
        assert.ok(!app.isComparing());
        assert.ok(app.searchPanel().hidden);
        assert.ok(!app.surface('library').hidden);
        assert.ok(dialogOpen(app));
        // Clicking the dimmed page does nothing.
        assert.ok(app.click(el(app, 'export-notes-overlay')).defaultPrevented);
        assert.ok(dialogOpen(app));

        // Escape closes it, and only it: the Library stays open.
        app.key('Escape', {}, target);
        assert.ok(!dialogOpen(app));
        assert.ok(!app.surface('library').hidden);
        assert.equal(app.document.activeElement, button(app));
        assert.equal(exportCalls(app).length, 0);
    }));

test('every focus it moves is preventScroll', () =>
    withNotes(SAMPLE, {}, async (app) => {
        const calls = [];
        const focus = app.window.HTMLElement.prototype.focus;
        app.window.HTMLElement.prototype.focus = function (options) {
            calls.push({ el: this, options });
            return focus.call(this, options);
        };
        app.openLibrary('notes');
        calls.length = 0;
        await clickAndWait(app, button(app), 'count_notes_export');
        app.key('Tab', { shiftKey: true }, app.document.activeElement);
        app.key('Tab', {}, app.document.activeElement);
        app.key('Escape', {}, app.document.activeElement);
        await clickAndWait(app, button(app), 'count_notes_export');
        app.click(el(app, 'export-notes-cancel'));
        await app.pick(null);
        await clickAndWait(app, button(app), 'count_notes_export');
        await clickAndWait(app, confirm(app), 'export_notes');
        assert.ok(calls.length >= 6, `${calls.length} focus calls`);
        const scrolling = calls.filter((c) => !(c.options && c.options.preventScroll));
        assert.deepEqual(scrolling.map((c) => c.el.id || c.el.className), []);
    }));
