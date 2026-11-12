// ☰ → Export data…: one place to export (D20). It opens a small chooser:
// Everything (backup), the backup Export data… has always saved, or My
// notes, the Export notes dialog, opened with what the Library's old
// Export notes… button gave it. My notes is greyed out, saying why, while
// there are no notes; Everything while an export or import runs. The
// command palette's Export notes… and Export data… are as they were, and
// go straight to the dialog and the backup, never by way of the chooser.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { backend, bookId, closeBridge, openApp, settle } from './harness.mjs';

const dir = mkdtempSync(join(tmpdir(), 'gospel-getter-export-choice-'));
after(async () => {
    await closeBridge();
    rmSync(dir, { recursive: true, force: true });
});

const JOHN = bookId('John');
const PSALMS = bookId('Psalms');
const NOTES = [
    [JOHN, 3, 16, 16, 'Loved.'],
    [PSALMS, 23, 1, 1, 'The shepherd.'],
];

/** Opens the app at John 3 in the KJV with `notes` ([book, chapter, start, end, body]) already written. */
async function withApp({ notes = NOTES, ...options }, body) {
    await backend('__reset', { bookId: JOHN, chapter: 3, translationCode: 'kjv' });
    for (const [id, chapter, verseStart, verseEnd, text] of notes) {
        await backend('add_note', { bookId: id, chapter, verseStart, verseEnd, body: text });
    }
    const app = await openApp({ book: 'John', chapter: 3, keepBookmarks: true, ...options });
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const el = (app, id) => app.document.getElementById(id);
const chooserOpen = (app) => !el(app, 'export-choice-overlay').hidden;
const notesDialogOpen = (app) => !el(app, 'export-notes-overlay').hidden;
const everything = (app) => el(app, 'export-choice-everything');
const myNotes = (app) => el(app, 'export-choice-notes');
/** A choice's name and its second line (its hint, or why it can't be chosen). */
const said = (app, choice) => [el(app, `${choice.id}-name`).textContent, el(app, `${choice.id}-line`).textContent];
const callsOf = (app, cmd) => app.calls.filter((c) => c.cmd === cmd);

const EVERYTHING_HINT = 'Bookmarks, highlights, notes, findings and settings, to restore later';
const NOTES_HINT = 'Choose notes and a layout, then save or print';

/** Counts the requests the page makes for the chooser and for Export notes, and keeps the last one's detail. */
function watchRequests(app) {
    const seen = { choice: 0, notes: 0, notesDetail: null };
    app.document.addEventListener('gospel-getter:export-choice', () => { seen.choice += 1; }, true);
    app.document.addEventListener('gospel-getter:export-notes', (event) => {
        seen.notes += 1;
        seen.notesDetail = event.detail;
    }, true);
    return seen;
}

// ---- The palette's helpers, as palette.test.mjs has them.

const option = (app, label) => [...app.document.querySelectorAll('#search-results .search-command')]
    .find((o) => o.querySelector('.palette-label').textContent === label);
const commandLabels = (app) => [...app.document.querySelectorAll('#search-results .search-command .palette-label')].map((o) => o.textContent);
const reason = (app, label) => {
    const o = option(app, label);
    assert.ok(o, label);
    return o.getAttribute('aria-disabled') === 'true' ? o.querySelector('.palette-reason').textContent : null;
};

async function type(app, text) {
    if (app.searchPanel().hidden) app.key('k', { ctrlKey: true });
    const before = app.calls.length;
    app.type(app.searchInput(), text);
    await new Promise((r) => setTimeout(r, 200));
    await app.idle(before);
}

/** Enter in the box; waits for `cmd` if given. */
async function enter(app, cmd = null) {
    const before = app.calls.length;
    app.key('Enter', {}, app.searchInput());
    await app.idle(Math.max(0, before - 1), cmd);
}

test('☰ → Export data… opens a chooser: Export, Everything (backup) or My notes, and Cancel', () =>
    withApp({}, async (app) => {
        const item = el(app, 'export-data-btn');
        assert.equal(item.textContent, 'Export data…');
        assert.equal(item.title, 'Save a backup, or export your notes');
        assert.ok(!chooserOpen(app));

        await app.openExport();
        assert.ok(chooserOpen(app));
        assert.ok(app.surface('menu').hidden, 'the menu closed for it');
        assert.equal(callsOf(app, 'export_reader_data').length, 0, 'nothing is saved yet');
        const panel = el(app, 'export-choice-panel');
        assert.equal(panel.getAttribute('role'), 'dialog');
        assert.equal(panel.getAttribute('aria-modal'), 'true');
        assert.equal(el(app, panel.getAttribute('aria-labelledby')).textContent, 'Export');
        assert.deepEqual(said(app, everything(app)), ['Everything (backup)', EVERYTHING_HINT]);
        assert.deepEqual(said(app, myNotes(app)), ['My notes', NOTES_HINT]);
        for (const choice of [everything(app), myNotes(app)]) {
            assert.equal(choice.getAttribute('type'), 'button');
            assert.equal(choice.getAttribute('aria-labelledby'), `${choice.id}-name`);
            assert.equal(choice.getAttribute('aria-describedby'), `${choice.id}-line`);
            assert.ok(!choice.disabled);
        }
        // In this order, then Cancel.
        assert.deepEqual([...panel.querySelectorAll('button')].map((b) => b.id),
            ['export-choice-everything', 'export-choice-notes', 'export-choice-cancel']);
        assert.equal(el(app, 'export-choice-cancel').textContent, 'Cancel');
        assert.equal(app.document.activeElement, everything(app), 'focus starts on Everything');
        assert.deepEqual(app.consoleErrors, []);
    }));

test('Everything (backup) is the backup as it always was: the same command, file and toast', () =>
    withApp({}, async (app) => {
        const path = join(dir, 'everything.gospel-getter.json');
        await app.pick(path);
        await app.openExport();
        const before = app.calls.length;
        await app.click(everything(app));
        assert.ok(!chooserOpen(app), 'it closes first');
        await app.idle(before, 'export_reader_data');

        const exports = callsOf(app, 'export_reader_data');
        assert.equal(exports.length, 1);
        assert.deepEqual(Object.keys(exports[0].args), ['preferences']);
        assert.deepEqual(JSON.parse(JSON.stringify(exports[0].args.preferences)),
            JSON.parse(JSON.stringify(app.window.gospelGetterPreferences.current())));
        assert.equal(app.toast().textContent,
            `Saved 0 bookmarks, 2 notes, your reading position, your display settings to ${path}. Bible text isn’t included.`);
        const json = JSON.parse(readFileSync(path, 'utf8'));
        assert.equal(json.format, 'gospel-getter-reader-data');
        assert.deepEqual(json.notes.map((n) => n.body).sort(), ['Loved.', 'The shepherd.']);
        assert.equal(app.document.activeElement, el(app, 'menu-toggle'), 'focus is back on ☰');
        assert.equal(callsOf(app, 'get_notes_export_options').length, 0);
    }));

test('My notes opens Export notes with what the Library’s button gave it; closing it, focus is back on ☰', () =>
    withApp({}, async (app) => {
        const seen = watchRequests(app);
        await app.openExport();
        const before = app.calls.length;
        await app.click(myNotes(app));
        assert.ok(!chooserOpen(app));
        await app.idle(before, 'count_notes_export');
        assert.ok(notesDialogOpen(app));
        assert.equal(seen.notes, 1);
        const { returnFocus, ...asked } = seen.notesDetail;
        assert.deepEqual(JSON.parse(JSON.stringify(asked)), {
            noteCount: 2,
            bookId: JOHN,
            translationCode: 'kjv',
            translationName: 'King James Version',
        });
        assert.equal(returnFocus, el(app, 'menu-toggle'));
        // The dialog as it always opened: on the book being read.
        const ticked = [...el(app, 'export-notes-books').querySelectorAll('input:checked')].map((b) => Number(b.value));
        assert.deepEqual(ticked, [JOHN]);
        assert.equal(callsOf(app, 'export_reader_data').length, 0);

        await app.key('Escape', {}, app.document.activeElement);
        assert.ok(!notesDialogOpen(app));
        assert.equal(app.document.activeElement, el(app, 'menu-toggle'));
    }));

test('without notes My notes is greyed out, saying why, and does nothing', () =>
    withApp({ notes: [] }, async (app) => {
        const seen = watchRequests(app);
        await app.openExport();
        assert.ok(myNotes(app).disabled);
        assert.deepEqual(said(app, myNotes(app)), ['My notes', 'There are no notes yet']);
        assert.ok(!everything(app).disabled);
        app.click(myNotes(app));
        await settle();
        assert.ok(chooserOpen(app));
        assert.equal(seen.notes, 0);
        assert.ok(!notesDialogOpen(app));
        // The arrows skip it.
        assert.equal(app.document.activeElement, everything(app));
        await app.key('ArrowDown', {}, everything(app));
        assert.equal(app.document.activeElement, everything(app));
    }));

test('while an export runs, Everything waits, saying why, and My notes doesn’t; it follows live', () =>
    withApp({}, async (app) => {
        await app.pick(null);
        const release = app.holdCommand('export_reader_data');
        await app.openExport();
        await app.click(everything(app));
        await settle();
        assert.ok(!chooserOpen(app));
        assert.ok(el(app, 'import-data-btn').disabled, 'Import data… waits too');
        assert.ok(!el(app, 'export-data-btn').disabled, 'but ☰ → Export data… still opens, for My notes');

        // The palette says the same, word for word.
        await type(app, 'export data');
        assert.equal(reason(app, 'Export data…'), 'Busy with an export or import');
        app.key('Escape', {}, app.searchInput());
        await settle();

        await app.openExport();
        assert.ok(everything(app).disabled);
        assert.deepEqual(said(app, everything(app)), ['Everything (backup)', 'Busy with an export or import']);
        assert.ok(!myNotes(app).disabled);
        assert.equal(app.document.activeElement, myNotes(app), 'focus starts on the first that can be chosen');
        app.click(everything(app));
        await settle();
        assert.equal(callsOf(app, 'export_reader_data').length, 1, 'no second backup');

        // It finishes while the chooser is open: Everything can be chosen again.
        release();
        await app.idle(0, 'export_reader_data');
        await settle();
        assert.ok(chooserOpen(app));
        assert.ok(!everything(app).disabled);
        assert.deepEqual(said(app, everything(app)), ['Everything (backup)', EVERYTHING_HINT]);
        assert.equal(app.toast().textContent, 'Export cancelled. No file was saved.');
        assert.ok(!el(app, 'import-data-btn').disabled);
    }));

test('Escape, Cancel and the dimmed page close it, focus back on ☰; Tab and the arrows stay inside; no shortcut runs', () =>
    withApp({}, async (app) => {
        const menu = el(app, 'menu-toggle');
        const cancel = el(app, 'export-choice-cancel');
        await app.openExport();
        const tab = (shiftKey) => app.key('Tab', { shiftKey }, app.document.activeElement).defaultPrevented;
        // ↓ and ↑ move between the two choices, round and round.
        assert.ok(app.key('ArrowDown', {}, everything(app)).defaultPrevented);
        assert.equal(app.document.activeElement, myNotes(app));
        app.key('ArrowDown', {}, myNotes(app));
        assert.equal(app.document.activeElement, everything(app));
        app.key('ArrowUp', {}, everything(app));
        assert.equal(app.document.activeElement, myNotes(app));
        // Tab and Shift+Tab wrap: Everything, My notes, Cancel.
        cancel.focus();
        assert.ok(tab(false));
        assert.equal(app.document.activeElement, everything(app));
        assert.ok(tab(true));
        assert.equal(app.document.activeElement, cancel);
        app.document.activeElement.blur();
        assert.ok(tab(false));
        assert.equal(app.document.activeElement, everything(app));

        // None of the reader's shortcuts run.
        for (const key of ['c', '/', 'ArrowRight', 'n', '?']) app.key(key, {}, everything(app));
        app.key('b', { ctrlKey: true }, everything(app));
        app.key('k', { ctrlKey: true }, everything(app));
        await settle();
        assert.equal(app.heading(), 'John 3');
        assert.ok(!app.isComparing());
        assert.ok(app.searchPanel().hidden);
        assert.ok(app.surface('library').hidden);
        assert.ok(el(app, 'shortcuts-overlay').hidden);
        assert.ok(chooserOpen(app));
        // A click inside the panel leaves it open.
        await app.click(el(app, 'export-choice-panel'));
        assert.ok(chooserOpen(app));

        await app.key('Escape', {}, everything(app));
        assert.ok(!chooserOpen(app));
        assert.equal(app.document.activeElement, menu);
        assert.equal(app.window.gospelGetterSurfaces.current(), null, 'and nothing else opens');

        await app.openExport();
        await app.click(cancel);
        assert.ok(!chooserOpen(app));
        assert.equal(app.document.activeElement, menu);

        await app.openExport();
        await app.click(el(app, 'export-choice-overlay'));
        assert.ok(!chooserOpen(app));
        assert.equal(app.document.activeElement, menu);
        assert.equal(callsOf(app, 'export_reader_data').length, 0);
        assert.equal(callsOf(app, 'get_notes_export_options').length, 0);
    }));

test('the palette’s Export notes… and Export data… are as they were, and skip the chooser', () =>
    withApp({}, async (app) => {
        const seen = watchRequests(app);
        // Listed as before: the same labels, in the same order, found by
        // the same words.
        await type(app, 'export');
        assert.deepEqual(commandLabels(app), ['Export notes…', 'Export data…']);
        assert.equal(reason(app, 'Export notes…'), null);
        assert.equal(reason(app, 'Export data…'), null);
        await type(app, 'markdown');
        assert.ok(commandLabels(app).includes('Export notes…'));
        await type(app, 'backup save');
        assert.deepEqual(commandLabels(app), ['Export data…']);

        // Export data…: the backup, at once.
        await app.pick(null);
        await type(app, 'export data');
        await enter(app, 'export_reader_data');
        assert.equal(seen.choice, 0);
        assert.ok(!chooserOpen(app));
        assert.equal(callsOf(app, 'export_reader_data').length, 1);
        assert.equal(app.toast().textContent, 'Export cancelled. No file was saved.');

        // Export notes…: the dialog, at once, with what it always had.
        await type(app, 'export notes');
        await enter(app, 'count_notes_export');
        assert.equal(seen.choice, 0);
        assert.ok(!chooserOpen(app));
        assert.ok(notesDialogOpen(app));
        const { returnFocus, ...asked } = seen.notesDetail;
        assert.deepEqual(JSON.parse(JSON.stringify(asked)), {
            noteCount: 2,
            bookId: JOHN,
            translationCode: 'kjv',
            translationName: 'King James Version',
        });
        assert.equal(returnFocus, null);
        // Closed, focus goes home, as it did.
        await app.key('Escape', {}, app.document.activeElement);
        assert.ok(!notesDialogOpen(app));
        assert.equal(app.document.activeElement, el(app, 'menu-toggle'));
    }).then(() => withApp({ notes: [] }, async (app) => {
        // Without notes, the same reason as before.
        await type(app, 'export');
        assert.equal(reason(app, 'Export notes…'), 'There are no notes yet');
        assert.equal(reason(app, 'Export data…'), null);
    })));

// AUD-05: My notes' dialog opens once its choices are in, unless another
// modal opened meanwhile: a note editor opened (and typed in) while they
// were on their way keeps the keyboard and focus, and Export notes neither
// opens nor says anything, whether its choices came or failed. Nothing
// typed is touched.
const notePanelOpen = (app) => !el(app, 'note-overlay').hidden;

/** My notes, its choices held (to `succeed` or fail), then a note editor opened and typed in. */
async function editorOpenedWhileMyNotesWaits(app, succeed) {
    const release = succeed
        ? app.holdCommand('get_notes_export_options')
        : app.holdFailure('get_notes_export_options', 'simulated get_notes_export_options failure');
    await app.openExport();
    const before = app.calls.length;
    await app.click(myNotes(app));
    assert.ok(!chooserOpen(app));
    await app.until(() => app.calls.slice(before).some((c) => c.cmd === 'get_notes_export_options'), 'the choices to be asked for');
    assert.ok(!notesDialogOpen(app), 'not yet');

    // Waiting from here: the choices are still held.
    const meanwhile = app.calls.length;
    app.click(app.verse(17));
    await app.idle(meanwhile, 'list_notes');
    app.click(app.actions().querySelector('.verse-note-btn'));
    await app.until(() => notePanelOpen(app), 'the note editor');
    const input = el(app, 'note-input');
    app.type(input, 'Typed while the export got ready.');
    await settle();
    assert.equal(app.document.activeElement, input);

    const toastBefore = app.toast().textContent;
    release();
    await app.idle(before);
    await settle();
    return { input, toastBefore };
}

for (const succeed of [true, false]) {
    test(`AUD-05: a note editor opened while My notes’ choices are on its way keeps the keyboard (the choices ${succeed ? 'come' : 'fail'})`, () =>
        withApp({}, async (app) => {
            const { input, toastBefore } = await editorOpenedWhileMyNotesWaits(app, succeed);
            assert.ok(!notesDialogOpen(app), 'Export notes doesn’t open over it');
            assert.ok(notePanelOpen(app), 'the editor is still open');
            assert.equal(app.document.activeElement, input, 'focus stays in it');
            assert.equal(input.value, 'Typed while the export got ready.', 'what was typed is as it was');
            assert.equal(app.toast().textContent, toastBefore, 'nothing is said about an export left behind');
            assert.equal(callsOf(app, 'count_notes_export').length, 0);
            // The keyboard is the editor's: Escape asks about the text typed.
            await app.key('Escape', {}, input);
            assert.ok(!el(app, 'note-discard').hidden, 'the editor asks first');
            assert.ok(!notesDialogOpen(app));

            // Asked for again afterwards, My notes opens as always.
            app.click(el(app, 'note-keep-later'));
            await settle();
            assert.ok(!notePanelOpen(app));
            await app.openExport();
            const before = app.calls.length;
            await app.click(myNotes(app));
            await app.idle(before, 'count_notes_export');
            assert.ok(notesDialogOpen(app));
        }));
}

// D2-01: the same for the keyboard shortcuts sheet and About, opened while
// My notes' choices are on their way: the newer sheet keeps the keyboard
// and focus, and Export notes neither opens over it nor says anything,
// whether its choices came or failed. One Escape closes the sheet, and
// nothing is open behind it.
const SHEETS = [
    {
        name: 'the keyboard shortcuts sheet',
        open: (app) => app.key('?', { shiftKey: true }),
        overlay: 'shortcuts-overlay',
        panel: 'shortcuts-panel',
    },
    {
        name: 'About',
        open: (app) => app.open('about'),
        overlay: 'about-overlay',
        panel: 'about-panel',
    },
];

for (const sheet of SHEETS) {
    for (const succeed of [true, false]) {
        test(`D2-01: ${sheet.name} opened while My notes’ choices are on their way keeps the keyboard (the choices ${succeed ? 'come' : 'fail'})`, () =>
            withApp({}, async (app) => {
                const sheetOpen = () => !el(app, sheet.overlay).hidden;
                const release = succeed
                    ? app.holdCommand('get_notes_export_options')
                    : app.holdFailure('get_notes_export_options', 'simulated get_notes_export_options failure');
                await app.openExport();
                const before = app.calls.length;
                await app.click(myNotes(app));
                await app.until(() => app.calls.slice(before).some((c) => c.cmd === 'get_notes_export_options'), 'the choices to be asked for');
                assert.ok(!notesDialogOpen(app), 'not yet');

                // Waiting from here: the choices are still held.
                await sheet.open(app);
                await settle();
                assert.ok(sheetOpen(), `${sheet.name} is open`);
                const focused = app.document.activeElement;
                assert.ok(el(app, sheet.panel).contains(focused), `focus is in ${sheet.name}`);
                const toastBefore = app.toast().textContent;
                release();
                await app.idle(before);
                await settle();

                assert.ok(!notesDialogOpen(app), 'Export notes doesn’t open over it');
                assert.ok(sheetOpen(), `${sheet.name} is still open`);
                assert.equal(app.document.activeElement, focused, 'focus stays where it was');
                assert.equal(app.toast().textContent, toastBefore, 'nothing is said about an export left behind');
                assert.equal(callsOf(app, 'count_notes_export').length, 0);
                const shownDialogs = [...app.document.querySelectorAll('[aria-modal="true"]')].filter((d) => !d.closest('[hidden]'));
                assert.deepEqual(shownDialogs.map((d) => d.id), [sheet.panel], 'one dialog');
                await app.key('Escape', {}, app.document.activeElement);
                assert.ok(!sheetOpen(), 'Escape closes it');
                assert.ok(!notesDialogOpen(app), 'and nothing is open behind it');
            }));
    }
}
