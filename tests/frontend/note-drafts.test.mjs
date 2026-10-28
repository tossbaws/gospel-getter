// Note drafts (R16): what's typed in the note editor is kept on this
// computer (local storage), by the note's id for an edit and by its verses
// for a new note, once typing pauses and at once when the text box loses
// focus, the window is hidden or the editor is closed with "Keep for
// later"; it's offered back, never put in the text box unasked, when that
// note (or a new note on those verses) is opened again — after a restart
// too. Saving, Discard and Delete forget it; it's in no backup and no
// notes export. Against the real backend and database.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { backend, bookId, closeBridge, openApp, settle } from './harness.mjs';

const dir = mkdtempSync(join(tmpdir(), 'gospel-getter-drafts-'));
after(async () => {
    await closeBridge();
    rmSync(dir, { recursive: true, force: true });
});

const JOHN = bookId('John');
const DRAFTS_KEY = 'gospel-getter-note-drafts';
/** How long typing pauses before the text is kept (`note_text::DRAFT_DELAY_MS`). */
const DRAFT_DELAY_MS = 500;
const DAY_MS = 24 * 60 * 60 * 1000;

/** Adds notes (each [start, end, body], in John 3) before the app opens. */
async function seed(notes) {
    await backend('__reset', { bookId: JOHN, chapter: 3, translationCode: 'kjv' });
    const added = [];
    for (const [verseStart, verseEnd, body] of notes) {
        added.push(await backend('add_note', { bookId: JOHN, chapter: 3, verseStart, verseEnd, body }));
    }
    return added;
}

const open = (options = {}) => openApp({ book: 'John', chapter: 3, keepBookmarks: true, ...options });

async function withApp(options, body) {
    const app = await open(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const el = (app, id) => app.document.getElementById(id);
const panelOpen = (app) => !el(app, 'note-overlay').hidden;
const offerShown = (app) => !el(app, 'note-draft').hidden;
const offerText = (app) => el(app, 'note-draft-text').textContent;
const input = (app) => el(app, 'note-input');
/** The drafts local storage keeps, as the page wrote them. */
const kept = (app) => JSON.parse(app.window.localStorage.getItem(DRAFTS_KEY) || '{"drafts":[]}').drafts;
const keptTexts = (app) => kept(app).map((d) => [d.key, d.text]);

async function select(app, start, end = start) {
    const before = app.calls.length;
    app.click(app.verse(start));
    if (end !== start) app.click(app.verse(end), { shiftKey: true });
    await app.idle(before, 'list_notes');
}

/** Opens the editor on the selection's first note (Edit), or + Add note. */
async function openEditor(app, how) {
    const button = how === 'add'
        ? app.actions().querySelector('.verse-note-btn')
        : app.document.querySelector('.verse-notes .note-edit');
    app.click(button);
    await app.until(() => panelOpen(app), 'the note editor to open');
    await settle();
}

async function type(app, text) {
    app.type(input(app), text);
    await settle();
}

const pause = () => new Promise((resolve) => setTimeout(resolve, DRAFT_DELAY_MS + 100));

/** Cancel, then the discard prompt's `choice` (`note-discard-yes`, `note-keep`, `note-keep-later`). */
async function cancelWith(app, choice) {
    app.click(el(app, 'note-cancel'));
    assert.ok(!el(app, 'note-discard').hidden, 'Cancel asks first');
    app.click(el(app, choice));
    await settle();
}

test('typed text is kept once typing pauses, and offered (not restored) when the note is opened again', async () => {
    const [note] = await seed([[16, 16, 'Loved.']]);
    await withApp({}, async (app) => {
        await select(app, 16);
        await openEditor(app, 'edit');
        await type(app, 'Loved, all of it.');
        assert.deepEqual(keptTexts(app), [], 'not at once: once typing pauses');
        await pause();
        assert.deepEqual(keptTexts(app), [[`note:${note.id}`, 'Loved, all of it.']]);
        // Closed keeping it: the discard prompt's third choice.
        await cancelWith(app, 'note-keep-later');
        assert.ok(!panelOpen(app));
        assert.deepEqual(keptTexts(app), [[`note:${note.id}`, 'Loved, all of it.']]);

        await openEditor(app, 'edit');
        assert.ok(offerShown(app));
        assert.equal(offerText(app), 'You have unsaved text from a moment ago.');
        assert.equal(input(app).value, 'Loved.', 'never put in the text box unasked');
        assert.equal(app.document.activeElement, input(app), 'focus where it always goes');
        // Restore, then Save: the draft is the note, and no draft is left.
        app.click(el(app, 'note-draft-restore'));
        await settle();
        assert.ok(!offerShown(app));
        assert.equal(input(app).value, 'Loved, all of it.');
        assert.equal(app.document.activeElement, input(app));
        assert.equal(el(app, 'note-save').disabled, false);
        const before = app.calls.length;
        app.click(el(app, 'note-save'));
        await app.until(() => !panelOpen(app), 'the editor to close');
        await app.idle(before);
        await pause();
        assert.deepEqual((await backend('list_all_notes')).map((n) => n.body), ['Loved, all of it.']);
        assert.deepEqual(keptTexts(app), []);
        await openEditor(app, 'edit');
        assert.ok(!offerShown(app));
    });
});

test('a new note’s draft is kept by its verses, whatever the translation, and offered on + Add note there', async () => {
    await seed([]);
    let storage;
    await withApp({}, async (app) => {
        await select(app, 16, 18);
        await openEditor(app, 'add');
        await type(app, 'A first thought on the range.');
        await pause();
        assert.deepEqual(keptTexts(app), [[`new:${JOHN}:3:16-18`, 'A first thought on the range.']]);
        await cancelWith(app, 'note-keep-later');
        storage = app.storage();
    });
    await withApp({ storage, translation: 'web' }, async (app) => {
        // Other verses: nothing to offer.
        await select(app, 16, 17);
        await openEditor(app, 'add');
        assert.ok(!offerShown(app));
        app.click(el(app, 'note-cancel'));
        await settle();
        await select(app, 16, 18);
        await openEditor(app, 'add');
        assert.ok(offerShown(app));
        assert.equal(input(app).value, '');
        app.click(el(app, 'note-draft-restore'));
        await settle();
        assert.equal(input(app).value, 'A first thought on the range.');
    });
});

test('quit and relaunch: the text typed is offered with the note', async () => {
    const [note] = await seed([[16, 16, 'Loved.']]);
    let storage;
    await withApp({}, async (app) => {
        await select(app, 16);
        await openEditor(app, 'edit');
        await type(app, 'Typed, then the app quit.');
        await pause();
        storage = app.storage();
    });
    await withApp({ storage }, async (app) => {
        await select(app, 16);
        await openEditor(app, 'edit');
        assert.ok(offerShown(app));
        assert.match(offerText(app), /^You have unsaved text from (a moment|a minute) ago\.$/);
        assert.equal(input(app).value, 'Loved.');
        app.click(el(app, 'note-draft-restore'));
        await settle();
        assert.equal(input(app).value, 'Typed, then the app quit.');
        assert.equal((await backend('list_all_notes')).find((n) => n.id === note.id).body, 'Loved.', 'nothing saved without Save');
    });
});

test('the text is kept at once when the text box loses focus or the window is hidden or left', async () => {
    const [note] = await seed([[16, 16, 'Loved.']]);
    await withApp({}, async (app) => {
        await select(app, 16);
        await openEditor(app, 'edit');
        await type(app, 'On blur.');
        el(app, 'note-cancel').focus();
        assert.deepEqual(keptTexts(app), [[`note:${note.id}`, 'On blur.']]);
        input(app).focus();
        await type(app, 'On pagehide.');
        app.window.dispatchEvent(new app.window.Event('pagehide'));
        assert.deepEqual(keptTexts(app), [[`note:${note.id}`, 'On pagehide.']]);
        await type(app, 'On visibilitychange.');
        Object.defineProperty(app.document, 'hidden', { configurable: true, get: () => true });
        app.document.dispatchEvent(new app.window.Event('visibilitychange'));
        assert.deepEqual(keptTexts(app), [[`note:${note.id}`, 'On visibilitychange.']]);
        Object.defineProperty(app.document, 'hidden', { configurable: true, get: () => false });
        await type(app, 'On the pointer leaving.');
        app.document.dispatchEvent(new app.window.MouseEvent('mouseleave'));
        assert.deepEqual(keptTexts(app), [[`note:${note.id}`, 'On the pointer leaving.']]);
        // Typed back to what's saved: nothing worth keeping.
        await type(app, 'Loved.');
        await pause();
        assert.deepEqual(keptTexts(app), []);
    });
});

test('Discard forgets the draft offered and keeps what’s saved; so does Cancel with Discard', async () => {
    const [note] = await seed([[16, 16, 'Loved.']]);
    await withApp({}, async (app) => {
        await select(app, 16);
        await openEditor(app, 'edit');
        await type(app, 'Second thoughts.');
        await cancelWith(app, 'note-keep-later');
        await openEditor(app, 'edit');
        assert.ok(offerShown(app));
        app.click(el(app, 'note-draft-discard'));
        await settle();
        assert.ok(!offerShown(app));
        assert.equal(input(app).value, 'Loved.');
        assert.equal(app.document.activeElement, input(app));
        assert.deepEqual(keptTexts(app), []);
        // Nothing changed: Cancel closes at once, and nothing comes back.
        app.click(el(app, 'note-cancel'));
        await settle();
        assert.ok(!panelOpen(app));
        await openEditor(app, 'edit');
        assert.ok(!offerShown(app));

        // Typed, then Cancel and Discard: no draft.
        await type(app, 'Thrown away.');
        await pause();
        assert.deepEqual(keptTexts(app), [[`note:${note.id}`, 'Thrown away.']]);
        await cancelWith(app, 'note-discard-yes');
        assert.ok(!panelOpen(app));
        assert.deepEqual(keptTexts(app), []);
        await openEditor(app, 'edit');
        assert.ok(!offerShown(app));
        assert.equal(input(app).value, 'Loved.');
    });
});

test('a draft offered stays until it’s answered; Keep editing keeps editing', async () => {
    const [note] = await seed([[16, 16, 'Loved.']]);
    await withApp({}, async (app) => {
        await select(app, 16);
        await openEditor(app, 'edit');
        await type(app, 'Kept for later.');
        await cancelWith(app, 'note-keep-later');
        // Opened and closed unanswered (Escape, nothing typed): still kept.
        await openEditor(app, 'edit');
        input(app).blur();
        app.key('Escape', {}, input(app));
        await settle();
        assert.ok(!panelOpen(app));
        assert.deepEqual(keptTexts(app), [[`note:${note.id}`, 'Kept for later.']]);
        // Keep editing: the prompt goes, the editor stays.
        await openEditor(app, 'edit');
        await type(app, 'Something else.');
        await cancelWith(app, 'note-keep');
        assert.ok(panelOpen(app));
        assert.equal(input(app).value, 'Something else.');
    });
});

test('the offer is announced, and its buttons are reached with the keyboard', async () => {
    await seed([[16, 16, 'Loved.']]);
    await withApp({}, async (app) => {
        await select(app, 16);
        await openEditor(app, 'edit');
        await type(app, 'Draft.');
        await cancelWith(app, 'note-keep-later');
        await openEditor(app, 'edit');
        assert.equal(el(app, 'note-draft-text').getAttribute('role'), 'alert');
        for (const id of ['note-draft-restore', 'note-draft-discard']) {
            const button = el(app, id);
            assert.equal(button.type, 'button');
            assert.equal(button.tabIndex, 0);
            assert.equal(button.getAttribute('aria-describedby'), 'note-draft-text');
        }
        assert.equal(el(app, 'note-draft-restore').textContent.trim(), 'Restore');
        assert.equal(el(app, 'note-draft-discard').textContent.trim(), 'Discard');
        // Shift+Tab from the text box: Discard, then Restore, inside the editor.
        const stops = [...el(app, 'note-panel').querySelectorAll('textarea, button, summary')]
            .filter((b) => !b.closest('[hidden]'));
        const at = stops.indexOf(input(app));
        assert.deepEqual(stops.slice(at - 2, at).map((b) => b.id), ['note-draft-restore', 'note-draft-discard']);
    });
});

test('a draft whose note was deleted is dropped, here or anywhere', async () => {
    const [first, second] = await seed([[16, 16, 'One.'], [17, 17, 'Two.']]);
    let storage;
    await withApp({}, async (app) => {
        await select(app, 16);
        await openEditor(app, 'edit');
        await type(app, 'One, changed.');
        await pause();
        // Deleted from the editor: its draft goes with it.
        const before = app.calls.length;
        app.click(el(app, 'note-delete'));
        await app.until(() => !panelOpen(app), 'the editor to close');
        await app.idle(before);
        assert.deepEqual(keptTexts(app), []);

        await select(app, 17);
        await openEditor(app, 'edit');
        await type(app, 'Two, changed.');
        await cancelWith(app, 'note-keep-later');
        assert.deepEqual(keptTexts(app), [[`note:${second.id}`, 'Two, changed.']]);
        storage = app.storage();
    });
    // Deleted elsewhere (here, by the backend before a restart): the
    // Library's list no longer has it, and its draft goes.
    await backend('delete_note', { id: second.id });
    await withApp({ storage }, async (app) => {
        assert.deepEqual(keptTexts(app), []);
    });
    assert.ok(first.id !== second.id);
});

test('drafts older than 30 days are dropped, never offered', async () => {
    const [note] = await seed([[16, 16, 'Loved.']]);
    const passage = { book: JOHN, chapter: 3, start: 16, end: 16 };
    const storage = {
        [DRAFTS_KEY]: JSON.stringify({
            drafts: [
                { key: `note:${note.id}`, passage, text: 'Ancient.', at: Date.now() - 31 * DAY_MS },
                { key: `new:${JOHN}:3:17-17`, passage: { ...passage, start: 17, end: 17 }, text: 'Recent.', at: Date.now() - 2 * DAY_MS },
            ],
        }),
    };
    await withApp({ storage }, async (app) => {
        await select(app, 16);
        await openEditor(app, 'edit');
        assert.ok(!offerShown(app));
        assert.deepEqual(keptTexts(app), [[`new:${JOHN}:3:17-17`, 'Recent.']]);
        app.click(el(app, 'note-cancel'));
        await settle();
        await select(app, 17);
        await openEditor(app, 'add');
        assert.ok(offerShown(app));
        assert.equal(offerText(app), 'You have unsaved text from 2 days ago.');
    });
});

test('drafts are in no backup and no notes export: both are as they were', async () => {
    await seed([[16, 16, 'Loved.']]);
    await backend('__set_clock', { now: '2026-10-12T18:00:00Z', offsetMinutes: -360 });
    await withApp({}, async (app) => {
        const exportBoth = async (name) => {
            const data = join(dir, `${name}.gospel-getter.json`);
            await app.pick(data);
            app.open('menu');
            let before = app.calls.length;
            app.click(el(app, 'export-data-btn'));
            await app.idle(before, 'export_reader_data');
            const notes = join(dir, `${name}.md`);
            await app.pick(notes);
            app.openLibrary('notes');
            before = app.calls.length;
            app.click(el(app, 'export-notes-btn'));
            await app.idle(before, 'count_notes_export');
            before = app.calls.length;
            app.click(el(app, 'export-notes-confirm'));
            await app.idle(before, 'export_notes');
            const backup = JSON.parse(readFileSync(data, 'utf8'));
            delete backup.exported_at;
            return { backup, notes: readFileSync(notes, 'utf8') };
        };
        const without = await exportBoth('without');
        await select(app, 16);
        await openEditor(app, 'edit');
        await type(app, 'A PRIVATE DRAFT');
        await cancelWith(app, 'note-keep-later');
        await select(app, 17);
        await openEditor(app, 'add');
        await type(app, 'ANOTHER PRIVATE DRAFT');
        await cancelWith(app, 'note-keep-later');
        assert.equal(kept(app).length, 2);
        const withDrafts = await exportBoth('with');
        assert.deepEqual(withDrafts.backup, without.backup);
        assert.equal(withDrafts.notes, without.notes);
        assert.ok(!JSON.stringify(withDrafts.backup).includes('PRIVATE DRAFT'));
        assert.ok(!withDrafts.notes.includes('PRIVATE DRAFT'));
    });
});
