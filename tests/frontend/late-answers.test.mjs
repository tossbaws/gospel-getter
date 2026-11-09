// Late answers never override a newer reader action (the 2026-11-08 review's
// "freshness narrower than intent", GG-01, GG-07, GG-09, GG-10, GG-11): each
// test holds one backend answer, does something newer while it's on its way,
// then lets it go, and checks that what the reader did last is what stays.
// The answers are held by the harness (holdAnswer, holdFailure,
// holdCommand), never by waiting, so each interleaving is the same every
// run.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, closeBridge, openApp, settle } from './harness.mjs';

after(closeBridge);

const JOHN = bookId('John');
const ROMANS = bookId('Romans');
const GENESIS = bookId('Genesis');

const el = (app, id) => app.document.getElementById(id);

/** Resets the reader's data, adds `notes` ([book, chapter, verse, body]), then opens John 3. */
async function withNotes(notes, body, options = {}) {
    await backend('__reset', { bookId: JOHN, chapter: 3, translationCode: 'kjv' });
    const added = [];
    for (const [book, chapter, verse, text] of notes) {
        added.push(await backend('add_note', { bookId: book, chapter, verseStart: verse, verseEnd: verse, body: text }));
    }
    const app = await openApp({ book: 'John', chapter: 3, keepBookmarks: true, ...options });
    try {
        await body(app, added);
    } finally {
        app.close();
    }
}

// ---- GG-01: a note's Edit from the Library, waiting for its jump.

const editorOpen = (app) => !el(app, 'note-overlay').hidden;
const editorTitle = (app) => el(app, 'note-title').textContent;
const toastText = (app) => (app.toast().classList.contains('is-hidden') ? '' : app.toast().textContent);
const STILL_WRITING = 'You’re still writing a note: save it or close it before opening another.';

/** The `get_reading` calls for `book`'s chapters, in order. */
const readings = (app, book) => app.calls.filter((c) => c.cmd === 'get_reading' && c.args.bookId === book);

/** The Library's Notes tab, and the Edit of the note with `id`: clicked, the jump asked for. */
function clickEdit(app, id) {
    app.openLibrary('notes');
    const edit = app.document.querySelector(`#note-list .note-item[data-note-id="${id}"] .note-list-edit`);
    assert.ok(edit, `the Library lists note ${id}`);
    app.click(edit);
    assert.ok(app.surface('library').hidden, 'the Library closes at once');
}

/** Verse `n` of the chapter on screen selected, and its card's + Add note: the editor open on it. */
async function addNoteOn(app, n) {
    const before = app.calls.length;
    app.click(app.verse(n));
    await app.idle(before, 'list_notes');
    app.click(app.actions().querySelector('.verse-note-btn'));
    await settle();
    assert.ok(editorOpen(app), `the editor opens on John 3:${n}`);
}

test('GG-01: a late Edit never replaces a newer note being written, and isn’t opened after it', () =>
    withNotes([[ROMANS, 8, 28, 'All things work together.']], async (app, [romans]) => {
        const release = app.holdAnswer('get_reading');
        clickEdit(app, romans.id);
        const jump = readings(app, ROMANS).at(-1);
        assert.ok(jump && !jump.done, 'Romans 8 is on its way');

        // Newer: a note on John 3:16, still on screen, typed in.
        await addNoteOn(app, 16);
        assert.equal(editorTitle(app), 'John 3:16');
        const typed = 'Typed while the jump was on its way.';
        app.type(el(app, 'note-input'), typed);
        await settle();

        const since = app.calls.length;
        release();
        await app.until(() => jump.done, 'the jump’s answer');
        await app.idle(since);

        assert.ok(editorOpen(app));
        assert.equal(editorTitle(app), 'John 3:16', 'the editor being typed in stays');
        assert.equal(el(app, 'note-input').value, typed, 'with every word typed');
        assert.notEqual(toastText(app), STILL_WRITING, 'the late Edit never even asked');
        // Closed, it was let go: nothing opens after.
        app.click(el(app, 'note-cancel'));
        app.click(el(app, 'note-discard-yes'));
        await app.idle(since);
        assert.ok(!editorOpen(app), 'the Edit isn’t waiting to open');
    }));

test('GG-01: an editor opened and closed while the jump was on its way lets the late Edit go', () =>
    withNotes([[ROMANS, 8, 28, 'All things work together.']], async (app, [romans]) => {
        const release = app.holdAnswer('get_reading');
        clickEdit(app, romans.id);
        const jump = readings(app, ROMANS).at(-1);
        await addNoteOn(app, 16);
        app.click(el(app, 'note-cancel'));
        assert.ok(!editorOpen(app));

        const since = app.calls.length;
        release();
        await app.until(() => jump.done, 'the jump’s answer');
        await app.idle(since);
        assert.equal(app.heading(), 'Romans 8', 'the jump still lands');
        assert.ok(!editorOpen(app), 'but the Edit, older than the editor opened since, doesn’t open');
    }));

test('GG-01: a later navigation lets a waiting Edit go', () =>
    withNotes([[ROMANS, 8, 28, 'All things work together.']], async (app, [romans]) => {
        const release = app.holdAnswer('get_reading');
        clickEdit(app, romans.id);
        const jump = readings(app, ROMANS).at(-1);

        // Newer: → to John 4, which isn't held.
        app.key('ArrowRight');
        await app.waitForHeading('John 4');

        const since = app.calls.length;
        release();
        await app.until(() => jump.done, 'the jump’s answer');
        await app.idle(since);
        assert.equal(app.heading(), 'John 4');
        assert.ok(!editorOpen(app), 'no editor on a note of a chapter the reader left');
    }));

test('GG-01: a jump that fails lets its Edit go', () =>
    withNotes([[ROMANS, 8, 28, 'All things work together.']], async (app, [romans]) => {
        const fail = app.holdFailure('get_reading', 'Romans 8 couldn’t be read.');
        clickEdit(app, romans.id);
        const jump = readings(app, ROMANS).at(-1);

        const since = app.calls.length;
        fail();
        await app.until(() => jump.done, 'the jump’s failure');
        await app.idle(since);
        assert.equal(app.heading(), 'John 3', 'still where the reader was');
        assert.match(app.notice(), /Romans 8 couldn’t be read\./);
        assert.ok(!editorOpen(app), 'no editor on a note whose verses aren’t there');
    }));

test('GG-01: a newer Edit supersedes an older one still on its way', () =>
    withNotes([
        [ROMANS, 8, 28, 'All things work together.'],
        [GENESIS, 1, 1, 'In the beginning.'],
    ], async (app, [romans, genesis]) => {
        const release = app.holdAnswer('get_reading');
        clickEdit(app, romans.id);
        const jump = readings(app, ROMANS).at(-1);

        // Newer: Genesis 1:1's Edit, which isn't held.
        let since = app.calls.length;
        clickEdit(app, genesis.id);
        await app.waitForHeading('Genesis 1');
        await app.until(() => editorOpen(app), 'Genesis 1:1’s editor');
        await app.idle(since);
        assert.equal(editorTitle(app), 'Genesis 1:1');

        since = app.calls.length;
        release();
        await app.until(() => jump.done, 'the older jump’s answer');
        await app.idle(since);
        assert.equal(app.heading(), 'Genesis 1');
        assert.equal(editorTitle(app), 'Genesis 1:1', 'the newer Edit’s note stays open');
        assert.equal(el(app, 'note-input').value, 'In the beginning.');
    }));

test('GG-01: an open editor’s unsaved text survives another note being asked to open', () =>
    withNotes([], async (app) => {
        await addNoteOn(app, 16);
        const typed = 'Not saved yet, and not kept as a draft yet either.';
        app.type(el(app, 'note-input'), typed);
        await settle();

        // Whatever asks (here the card's + Add note, under the editor),
        // the open editor isn't replaced while it holds unsaved text.
        app.click(app.actions().querySelector('.verse-note-btn'));
        await settle();
        assert.ok(editorOpen(app));
        assert.equal(editorTitle(app), 'John 3:16');
        assert.equal(el(app, 'note-input').value, typed);
        assert.equal(toastText(app), STILL_WRITING, 'and the toast says why nothing else opened');
    }));
