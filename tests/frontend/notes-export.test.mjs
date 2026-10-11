// Export notes…: every note, or one book's, as a readable Markdown file
// with each note's verses quoted in the translation being read. A button
// at the top of the Library's Notes tab opens a small modal; the real
// backend builds and writes the file (the save dialog is scripted).

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
const bookOptions = (app) => [...el(app, 'export-notes-book').options].map((o) => [Number(o.value), o.textContent]);
// The page's own objects, as plain ones (to compare with deepEqual).
const exportCalls = (app) => app.calls.filter((c) => c.cmd === 'export_notes').map((c) => JSON.parse(JSON.stringify(c.args)));

async function clickAndWait(app, target, cmd) {
    const before = app.calls.length;
    app.click(target);
    await app.idle(before, cmd);
}

function openDialog(app) {
    app.openLibrary('notes');
    app.click(button(app));
    assert.ok(dialogOpen(app));
}

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

test('the dialog lists only books with notes, with counts, starting on the book being read', () =>
    withNotes(SAMPLE, {}, async (app) => {
        openDialog(app);
        const panel = el(app, 'export-notes-panel');
        assert.equal(panel.getAttribute('role'), 'dialog');
        assert.equal(panel.getAttribute('aria-modal'), 'true');
        assert.equal(el(app, panel.getAttribute('aria-labelledby')).textContent, 'Export notes');
        assert.equal(el(app, panel.getAttribute('aria-describedby')).textContent,
            'A Markdown file of your notes, each with its verses quoted in the King James Version.');
        assert.equal(el(app, 'export-notes-all-label').textContent, 'All notes (5)');
        assert.deepEqual(bookOptions(app), [[PSALMS, 'Psalms (1)'], [JOHN, 'John (3)'], [FIRST_JOHN, '1 John (1)']]);
        assert.ok(el(app, 'export-notes-one').checked);
        assert.ok(!el(app, 'export-notes-all').checked);
        assert.equal(Number(el(app, 'export-notes-book').value), JOHN);
        assert.equal(app.document.activeElement, el(app, 'export-notes-one'));
    }));

test('reading a book without notes, the dialog starts on All notes', () =>
    withNotes(SAMPLE, { book: 'Genesis', chapter: 1 }, async (app) => {
        openDialog(app);
        assert.ok(el(app, 'export-notes-all').checked);
        assert.ok(!el(app, 'export-notes-one').checked);
        assert.equal(Number(el(app, 'export-notes-book').value), PSALMS);
        assert.equal(app.document.activeElement, el(app, 'export-notes-all'));
        // Choosing a book chooses One book.
        el(app, 'export-notes-book').value = String(FIRST_JOHN);
        el(app, 'export-notes-book').dispatchEvent(new app.window.Event('change', { bubbles: true }));
        assert.ok(el(app, 'export-notes-one').checked);
    }));

test('Export sends the translation being read and the book, and reports the file', () =>
    withNotes(SAMPLE, {}, async (app) => {
        const path = join(dir, 'john.md');
        await app.pick(path);
        openDialog(app);
        await clickAndWait(app, el(app, 'export-notes-confirm'), 'export_notes');
        assert.deepEqual(exportCalls(app), [{ translationCode: 'kjv', bookId: JOHN }]);
        assert.ok(!dialogOpen(app));
        assert.equal(app.toast().textContent, `Exported 3 notes on John to ${path}.`);
        assert.ok(!app.toast().classList.contains('is-error'));
        const file = readFileSync(path, 'utf8');
        assert.ok(file.startsWith('# Notes on John\n\nExported '), file);
        assert.ok(file.includes(' · Verse text: King James Version · 3 notes\n'));
        assert.ok(file.includes(`## John 3:16\n\n> **16** ${storedVerse('kjv', 'John', 3, 16)}\n\nLoved.\n`));
        assert.ok(!file.includes('Confess.'));

        // All notes: no book.
        const all = join(dir, 'all.md');
        await app.pick(all);
        app.click(button(app));
        app.click(el(app, 'export-notes-all'));
        await clickAndWait(app, el(app, 'export-notes-confirm'), 'export_notes');
        assert.deepEqual(exportCalls(app)[1], { translationCode: 'kjv', bookId: null });
        assert.equal(app.toast().textContent, `Exported 5 notes to ${all}.`);
        assert.ok(readFileSync(all, 'utf8').startsWith('# Gospel Getter notes\n'));
    }));

test('in compare mode the verses come from the translation chosen in Aa, not the other column', () =>
    withNotes([[bookId('Matthew'), 2, 23, 23, 'Nazareth.']], { book: 'Matthew', chapter: 2, translation: 'web', compare: true }, async (app) => {
        assert.ok(app.isComparing());
        // The columns keep their own order; the WEB, chosen, is the right one.
        assert.deepEqual(app.compareColumns(), ['King James Version (KJV)', 'World English Bible (WEB)']);
        assert.equal(app.document.getElementById('translation-select').value, 'web');
        const path = join(dir, 'web.md');
        await app.pick(path);
        openDialog(app);
        assert.ok(el(app, 'export-notes-text').textContent.endsWith('quoted in the World English Bible.'));
        await clickAndWait(app, el(app, 'export-notes-confirm'), 'export_notes');
        assert.deepEqual(exportCalls(app), [{ translationCode: 'web', bookId: bookId('Matthew') }]);
        const file = readFileSync(path, 'utf8');
        assert.ok(file.includes(`> **23** ${storedVerse('web', 'Matthew', 2, 23)}\n`), file);
    }));

test('cancelling the save dialog says nothing; a failure says why', () =>
    withNotes(SAMPLE, {}, async (app) => {
        await app.pick(null);
        openDialog(app);
        await clickAndWait(app, el(app, 'export-notes-confirm'), 'export_notes');
        assert.equal(exportCalls(app).length, 1);
        assert.equal(app.toast().textContent, '');
        assert.ok(app.toast().classList.contains('is-hidden'));

        app.failCommand('export_notes', 'There are no notes to export.');
        app.click(button(app));
        await clickAndWait(app, el(app, 'export-notes-confirm'), 'export_notes');
        assert.equal(app.toast().textContent, 'Export failed: There are no notes to export.');
        assert.ok(app.toast().classList.contains('is-error'));
    }));

test('Escape and Cancel close it and give focus back; it keeps the keyboard while open', () =>
    withNotes(SAMPLE, {}, async (app) => {
        openDialog(app);
        app.click(el(app, 'export-notes-cancel'));
        assert.ok(!dialogOpen(app));
        assert.equal(app.document.activeElement, button(app));

        app.click(button(app));
        // Tab and Shift+Tab wrap: the checked radio, the book, Export, Cancel.
        const tab = (shiftKey) => app.key('Tab', { shiftKey }, app.document.activeElement).defaultPrevented;
        assert.equal(app.document.activeElement, el(app, 'export-notes-one'));
        assert.ok(tab(true));
        assert.equal(app.document.activeElement, el(app, 'export-notes-cancel'));
        assert.ok(tab(false));
        assert.equal(app.document.activeElement, el(app, 'export-notes-one'));
        app.document.activeElement.blur();
        assert.ok(tab(false));
        assert.equal(app.document.activeElement, el(app, 'export-notes-one'));

        // None of the reader's shortcuts run.
        const target = el(app, 'export-notes-one');
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
        app.click(button(app));
        app.key('Tab', { shiftKey: true }, app.document.activeElement);
        app.key('Tab', {}, app.document.activeElement);
        app.key('Escape', {}, app.document.activeElement);
        app.click(button(app));
        app.click(el(app, 'export-notes-cancel'));
        await app.pick(null);
        app.click(button(app));
        await clickAndWait(app, el(app, 'export-notes-confirm'), 'export_notes');
        assert.ok(calls.length >= 7, `${calls.length} focus calls`);
        const scrolling = calls.filter((c) => !(c.options && c.options.preventScroll));
        assert.deepEqual(scrolling.map((c) => c.el.id || c.el.className), []);
    }));
