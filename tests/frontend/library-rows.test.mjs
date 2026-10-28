// Behavior tests for the Library's rows (Bookmarks, Highlights and Notes):
// each row is one link to its passage, opened by a click anywhere on it,
// with its secondary button (× or Edit) separate beside it, never inside.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, closeBridge, openApp } from './harness.mjs';

after(closeBridge);

const JOHN = bookId('John');

/** One bookmark (John 3:16), one highlight (Psalms 23:1) and one note (John 3:21), then the app, in Romans 8. */
async function withLibrary(body) {
    await backend('__reset', { bookId: JOHN, chapter: 3, translationCode: 'kjv' });
    await backend('add_bookmark', { bookId: JOHN, chapter: 3, verseStart: 16, verseEnd: 16 });
    await backend('set_highlight', { bookId: bookId('Psalms'), chapter: 23, verseStart: 1, verseEnd: 1, color: 'yellow' });
    await backend('add_note', { bookId: JOHN, chapter: 3, verseStart: 21, verseEnd: 21, body: 'Light: coming to it is the whole of it.' });
    const app = await openApp({ book: 'Romans', chapter: 8, keepBookmarks: true });
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const rowsOf = (app, list) => [...app.document.querySelectorAll(`#${list}-list .bookmark-item`)];

// No control holds another, and each row has one way in plus its own
// secondary button.
function assertRowSemantics(row, secondary) {
    const controls = [...row.querySelectorAll('button, a, input, [tabindex], [role="button"], [role="link"]')];
    assert.deepEqual(controls.map((c) => c.className), ['bookmark-open', secondary]);
    for (const control of controls) {
        assert.equal(control.querySelector('button, a, input, [tabindex]'), null, `${control.className} holds no control`);
        assert.equal(control.parentElement, row, 'side by side in the row');
    }
    assert.equal(row.getAttribute('role'), null, 'the row itself is a plain list item');
    assert.equal(row.hasAttribute('tabindex'), false, 'and isn’t a tab stop');
}

async function clickRow(app, row) {
    const before = app.calls.length;
    // The row's own space: not its buttons.
    app.click(row);
    await app.idle(before);
}

test('a click anywhere on a Bookmarks row opens its passage; × stays separate', () =>
    withLibrary(async (app) => {
        app.openLibrary('bookmarks');
        const [row] = rowsOf(app, 'bookmark');
        assertRowSemantics(row, 'bookmark-remove');
        await clickRow(app, row);
        assert.equal(app.heading(), 'John 3');
        assert.deepEqual(app.selectedVerseNumbers(), [16]);
        assert.ok(app.surface('library').hidden);
    }));

test('a click anywhere on a Highlights row opens its passage; × stays separate', () =>
    withLibrary(async (app) => {
        app.openLibrary('highlights');
        const [row] = rowsOf(app, 'highlight');
        assertRowSemantics(row, 'bookmark-remove');
        // On the color's name, inside the row's link: the same.
        await clickRow(app, row.querySelector('.highlight-color'));
        assert.equal(app.heading(), 'Psalms 23');
        assert.deepEqual(app.selectedVerseNumbers(), [1]);
    }));

// A note's row: its link (named by its reference, described by the
// note's start beside it, which is Markdown and may hold links of its own,
// so it can't be inside the row's link), then Edit and Copy as Markdown.
// No control holds another.
function assertNoteRowSemantics(row) {
    const controls = [...row.querySelectorAll('button, a, input, [tabindex], [role="button"], [role="link"]')];
    assert.deepEqual(controls.map((c) => c.className), ['bookmark-open', 'note-list-edit', 'note-list-copy']);
    for (const control of controls) {
        assert.equal(control.querySelector('button, a, input, [tabindex]'), null, `${control.className} holds no control`);
        assert.equal(control.closest('.bookmark-item'), row);
    }
    assert.equal(row.getAttribute('role'), null, 'the row itself is a plain list item');
    assert.equal(row.hasAttribute('tabindex'), false, 'and isn’t a tab stop');
}

test('a click anywhere on a Notes row opens its passage; Edit is a separate, secondary button', () =>
    withLibrary(async (app) => {
        app.openLibrary('notes');
        const [row] = rowsOf(app, 'note');
        assertNoteRowSemantics(row);
        const open = row.querySelector('.bookmark-open');
        // One activatable row, named by its reference, described by its note.
        assert.equal(open.textContent.replace(/\s+/g, ' ').trim(), 'John 3:21');
        const preview = row.querySelector('.note-preview');
        assert.equal(open.getAttribute('aria-describedby'), preview.id);
        assert.equal(preview.textContent, 'Light: coming to it is the whole of it.');
        const edit = row.querySelector('.note-list-edit');
        assert.equal(edit.textContent, 'Edit');
        assert.equal(edit.getAttribute('aria-label'), 'Edit note on John 3:21');

        await clickRow(app, row);
        assert.equal(app.heading(), 'John 3');
        assert.deepEqual(app.selectedVerseNumbers(), [21]);
        assert.ok(app.document.getElementById('note-overlay').hidden, 'opening isn’t editing');
        // On the note's start, beside the link: the same.
        app.openLibrary('notes');
        await clickRow(app, rowsOf(app, 'note')[0].querySelector('.note-preview'));
        assert.deepEqual(app.selectedVerseNumbers(), [21]);
        assert.ok(app.surface('library').hidden);

        // Edit opens the note itself, and the row's click doesn't also fire.
        app.openLibrary('notes');
        const before = app.calls.length;
        app.click(rowsOf(app, 'note')[0].querySelector('.note-list-edit'));
        await app.idle(before);
        assert.equal(app.document.getElementById('note-overlay').hidden, false);
        assert.equal(app.document.getElementById('note-input').value, 'Light: coming to it is the whole of it.');
        assert.ok(app.calls.slice(before).filter((c) => c.cmd === 'get_reading').length <= 1, 'Edit alone, not the row as well');
    }));

test('the keyboard reaches each row’s link and its secondary button, in that order', () =>
    withLibrary(async (app) => {
        app.openLibrary('notes');
        const [row] = rowsOf(app, 'note');
        const focusable = [...app.document.querySelectorAll('#note-list button')];
        assert.deepEqual(focusable, [row.querySelector('.bookmark-open'), row.querySelector('.note-list-edit'), row.querySelector('.note-list-copy')]);
        assert.ok(focusable.every((b) => b.tabIndex === 0 && b.type === 'button'));
    }));
