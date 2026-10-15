// Verse notes: the reader's own words on a passage, shown by a marker on
// every verse they cover, in a Notes section under a selection's actions,
// and in the Library's Notes tab; written and edited in a modal panel —
// against the real backend and database. Notes belong to no translation.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { backend, bookId, closeBridge, openApp, settle, storedVerse } from './harness.mjs';

const dir = mkdtempSync(join(tmpdir(), 'gospel-getter-notes-'));
after(async () => {
    await closeBridge();
    rmSync(dir, { recursive: true, force: true });
});

const JOHN = bookId('John');

/** Adds notes (each [start, end, body], in John 3 unless `book`/`chapter` given) before the app opens. */
async function seed(notes) {
    await backend('__reset', { bookId: JOHN, chapter: 3, translationCode: 'kjv' });
    const added = [];
    for (const [verseStart, verseEnd, body, book = JOHN, chapter = 3] of notes) {
        added.push(await backend('add_note', { bookId: book, chapter, verseStart, verseEnd, body }));
    }
    return added;
}

async function withNotes(notes, options, body) {
    await seed(notes);
    const app = await openApp({ book: 'John', chapter: 3, keepBookmarks: true, ...options });
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const el = (app, id) => app.document.getElementById(id);
const panelOpen = (app) => !el(app, 'note-overlay').hidden;
const section = (app) => app.document.querySelector('.verse-notes');
const cards = (app) => [...app.document.querySelectorAll('.verse-notes .note-card')].map((card) => ({
    reference: card.querySelector('.note-card-ref').textContent,
    body: card.querySelector('.note-card-body').textContent,
}));
const notedVerses = (app, code) => [...app.document.querySelectorAll('.chapter-main .verse.has-notes')]
    .filter((v) => !code || v.dataset.translation === code)
    .map((v) => Number(v.dataset.verse));
const addNoteButton = (app) => app.actions().querySelector('.verse-note-btn');
const statusText = (app) => app.status().textContent;
const allNotes = () => backend('list_all_notes');

async function select(app, start, end = start, code) {
    const before = app.calls.length;
    app.click(app.verse(start, code));
    if (end !== start) app.click(app.verse(end, code), { shiftKey: true });
    await app.idle(before, 'list_notes');
}

async function clickAndWait(app, target, cmd) {
    const before = app.calls.length;
    app.click(target);
    await app.idle(before, cmd);
}

async function typeNote(app, text) {
    app.type(el(app, 'note-input'), text);
    await settle();
}

test('every verse a note covers is marked, in the chapter being read only, once per verse', () =>
    withNotes([[16, 18, 'Range.'], [17, 17, 'Inside it.'], [21, 21, 'Light.']], {}, async (app) => {
        assert.deepEqual(notedVerses(app), [16, 17, 18, 21]);
        // The faded neighbors show none.
        assert.equal(app.document.querySelectorAll('.chapter-side .has-notes, .chapter-side .note-cue').length, 0);
        // Screen readers hear "has notes" once per verse, however many notes.
        const cues = app.verse(17).querySelectorAll('.note-cue');
        assert.equal(cues.length, 1);
        assert.equal(cues[0].textContent.trim(), '(has notes)');
        assert.ok(cues[0].classList.contains('visually-hidden'));
        assert.equal(app.verse(19).querySelectorAll('.note-cue').length, 0);
        // The marker is a pseudo-element: no new tab stops, nothing to click.
        assert.equal(app.verse(17).querySelectorAll('button, a, [tabindex]').length, 0);
        // The verse text itself is untouched.
        assert.equal(app.verse(17).querySelector('.verse-text').textContent, storedVerse('kjv', 'John', 3, 17));
        // Clicking a marked verse still selects it.
        await select(app, 17);
        assert.deepEqual(app.selectedVerseNumbers(), [17]);
    }));

test('the Notes section lists every note overlapping the selection, in order', () =>
    withNotes([[21, 21, 'Light.'], [16, 18, 'Range.'], [16, 16, 'Loved.'], [19, 19, 'Darkness.']], {}, async (app) => {
        await select(app, 16, 21);
        const region = section(app);
        assert.equal(region.getAttribute('role'), 'region');
        assert.equal(region.getAttribute('aria-label'), 'Notes for John 3:16–21');
        assert.equal(region.querySelector('.verse-notes-title').textContent, 'Notes (4)');
        // The first three, then a way to see them all.
        assert.deepEqual(cards(app).map((c) => c.reference), ['John 3:16', 'John 3:16–18', 'John 3:19']);
        const showAll = region.querySelector('.note-show-all');
        assert.equal(showAll.textContent, 'Show all 4 notes');
        app.click(showAll);
        assert.deepEqual(cards(app).map((c) => c.reference), ['John 3:16', 'John 3:16–18', 'John 3:19', 'John 3:21']);
        assert.equal(app.document.activeElement, region.querySelectorAll('.note-edit')[3]);
        // It's in the selection card: after the actions and the selection's
        // footnotes, before its cross-references.
        assert.equal(region.parentElement, app.document.getElementById('selection-card'));
        assert.ok(region.previousElementSibling.classList.contains('selection-footnotes'));
        assert.ok(region.nextElementSibling.classList.contains('selection-xrefs'));
        for (const card of region.querySelectorAll('.note-card')) {
            assert.ok(card.querySelector('.note-card-date').textContent.match(/^[A-Z][a-z]{2} \d{1,2}, \d{4}$/));
            assert.equal(card.querySelector('.note-edit').textContent, 'Edit');
        }

        // A note on 3:16–18 shows for 3:17 alone.
        await select(app, 17);
        assert.deepEqual(cards(app), [{ reference: 'John 3:16–18', body: 'Range.' }]);
        assert.equal(section(app).getAttribute('aria-label'), 'Notes for John 3:17');

        // No note overlaps 3:20: no section, only + Add note.
        await select(app, 20);
        assert.equal(section(app), null);
        assert.equal(addNoteButton(app).textContent.trim(), '+ Add note');

        // Clearing the selection takes the section with it.
        await select(app, 17);
        app.key('Escape');
        assert.equal(section(app), null);
    }));

test('a note is added, edited, deleted and restored, and every view keeps up', () =>
    withNotes([], {}, async (app) => {
        app.openLibrary('notes');
        assert.equal(el(app, 'note-count').textContent, '0');
        app.key('Escape');

        await select(app, 16, 18);
        const add = addNoteButton(app);
        app.click(add);
        assert.ok(panelOpen(app));
        const panel = el(app, 'note-panel');
        assert.equal(panel.getAttribute('role'), 'dialog');
        assert.equal(panel.getAttribute('aria-modal'), 'true');
        assert.equal(el(app, panel.getAttribute('aria-labelledby')).textContent, 'John 3:16–18');
        assert.equal(el(app, 'note-text-shown').textContent, 'Text shown: KJV');
        const quoted = el(app, 'note-passage').textContent;
        for (const verse of [16, 17, 18]) assert.ok(quoted.includes(storedVerse('kjv', 'John', 3, verse)), `quotes 3:${verse}`);
        const input = el(app, 'note-input');
        assert.equal(app.document.activeElement, input);
        assert.equal(el(app, 'note-label').textContent, 'Your note on John 3:16–18');
        assert.equal(input.labels[0], el(app, 'note-label'));
        assert.ok(el(app, 'note-save').disabled, 'nothing to save yet');
        assert.ok(el(app, 'note-delete').hidden, 'no Delete when adding');
        await typeNote(app, '   ');
        assert.ok(el(app, 'note-save').disabled, 'whitespace is nothing to save');

        await typeNote(app, '  Belief and judgment.\nLine two.  ');
        assert.ok(!el(app, 'note-save').disabled);
        await clickAndWait(app, el(app, 'note-save'), 'list_all_notes');
        assert.ok(!panelOpen(app));
        assert.equal(app.document.activeElement, add, 'focus is back on + Add note');
        assert.equal(statusText(app), 'Note added');
        assert.deepEqual(cards(app), [{ reference: 'John 3:16–18', body: 'Belief and judgment.\nLine two.' }]);
        assert.deepEqual(notedVerses(app), [16, 17, 18]);
        assert.equal(el(app, 'note-count').textContent, '1');
        const [saved] = await allNotes();
        assert.equal(saved.body, 'Belief and judgment.\nLine two.');

        // Edit: the panel holds the note, with Delete.
        const edit = section(app).querySelector('.note-edit');
        app.click(edit);
        assert.equal(input.value, 'Belief and judgment.\nLine two.');
        assert.ok(!el(app, 'note-delete').hidden);
        assert.ok(el(app, 'note-others').hidden, 'only adding lists the other notes');
        await typeNote(app, 'Belief, judgment and light.');
        await clickAndWait(app, el(app, 'note-save'), 'list_all_notes');
        assert.ok(!panelOpen(app));
        assert.equal(statusText(app), 'Note saved');
        assert.deepEqual(cards(app).map((c) => c.body), ['Belief, judgment and light.']);
        assert.equal(app.document.activeElement, section(app).querySelector('.note-edit'), 'focus is back on its Edit');
        assert.equal(app.document.querySelector('#note-list .note-preview').textContent, 'Belief, judgment and light.');
        const [edited] = await allNotes();
        assert.equal(edited.createdAt, saved.createdAt);
        assert.ok(edited.updatedAt >= saved.updatedAt);

        // Delete: gone everywhere, with an Undo that lasts longer than the
        // usual status.
        app.click(section(app).querySelector('.note-edit'));
        await clickAndWait(app, el(app, 'note-delete'), 'list_all_notes');
        assert.ok(!panelOpen(app));
        assert.equal(section(app), null);
        assert.deepEqual(notedVerses(app), []);
        assert.equal(el(app, 'note-count').textContent, '0');
        assert.deepEqual(await allNotes(), []);
        assert.equal(statusText(app), 'Note deleted · Undo');
        assert.equal(app.document.activeElement, addNoteButton(app), 'its Edit is gone, so focus goes to the actions');
        await new Promise((resolve) => setTimeout(resolve, 3000));
        assert.equal(statusText(app), 'Note deleted · Undo', 'still there after 3 s');

        await clickAndWait(app, app.status().querySelector('.status-undo'), 'list_all_notes');
        assert.equal(statusText(app), 'Note restored');
        const [restored] = await allNotes();
        assert.deepEqual(
            [restored.reference, restored.body, restored.createdAt, restored.updatedAt],
            [edited.reference, edited.body, edited.createdAt, edited.updatedAt],
        );
        assert.deepEqual(cards(app).map((c) => c.body), ['Belief, judgment and light.']);
        assert.deepEqual(notedVerses(app), [16, 17, 18]);
        assert.equal(el(app, 'note-count').textContent, '1');
    }));

test('Escape and Cancel close at once when nothing changed, and ask first when it did', () =>
    withNotes([[16, 16, 'Kept as is.']], {}, async (app) => {
        await select(app, 16);
        const add = addNoteButton(app);
        // Nothing typed: Escape closes at once.
        app.click(add);
        app.key('Escape', {}, el(app, 'note-input'));
        assert.ok(!panelOpen(app));
        assert.equal(app.document.activeElement, add);
        assert.deepEqual(app.selectedVerseNumbers(), [16], 'that Escape didn’t also clear the selection');
        // Nor Cancel.
        app.click(add);
        app.click(el(app, 'note-cancel'));
        assert.ok(!panelOpen(app));

        // Typed: Escape asks, and Escape again (or Keep editing) goes back.
        app.click(add);
        await typeNote(app, 'A draft.');
        app.key('Escape', {}, el(app, 'note-input'));
        assert.ok(panelOpen(app));
        assert.ok(!el(app, 'note-discard').hidden);
        assert.ok(el(app, 'note-buttons').hidden);
        assert.equal(el(app, 'note-discard-question').textContent, 'Discard this note?');
        assert.equal(app.document.activeElement, el(app, 'note-keep'));
        app.key('Escape', {}, el(app, 'note-keep'));
        assert.ok(el(app, 'note-discard').hidden);
        assert.equal(app.document.activeElement, el(app, 'note-input'));
        assert.equal(el(app, 'note-input').value, 'A draft.');
        app.click(el(app, 'note-cancel'));
        app.click(el(app, 'note-keep'));
        assert.ok(panelOpen(app));
        assert.equal(el(app, 'note-input').value, 'A draft.');
        // Discard: closed, nothing saved.
        app.click(el(app, 'note-cancel'));
        app.click(el(app, 'note-discard-yes'));
        assert.ok(!panelOpen(app));
        assert.ok(!app.calls.some((c) => c.cmd === 'add_note'));

        // Editing: an unchanged note closes at once; a changed one asks.
        app.click(section(app).querySelector('.note-edit'));
        app.key('Escape', {}, el(app, 'note-input'));
        assert.ok(!panelOpen(app));
        app.click(section(app).querySelector('.note-edit'));
        await typeNote(app, 'Changed.');
        app.click(el(app, 'note-cancel'));
        assert.ok(!el(app, 'note-discard').hidden);
        app.click(el(app, 'note-discard-yes'));
        assert.deepEqual((await allNotes()).map((n) => n.body), ['Kept as is.']);
        assert.ok(!app.calls.some((c) => c.cmd === 'update_note'));
    }));

test('Ctrl+Enter (or Cmd+Enter) saves', () =>
    withNotes([], {}, async (app) => {
        await select(app, 3);
        app.click(addNoteButton(app));
        await typeNote(app, 'Born again.');
        const before = app.calls.length;
        const event = app.key('Enter', { ctrlKey: true }, el(app, 'note-input'));
        assert.ok(event.defaultPrevented);
        await app.idle(before, 'list_all_notes');
        assert.ok(!panelOpen(app));
        assert.deepEqual((await allNotes()).map((n) => [n.reference, n.body]), [['John 3:3', 'Born again.']]);

        await select(app, 5);
        app.click(addNoteButton(app));
        await typeNote(app, 'Water and spirit.');
        const later = app.calls.length;
        app.key('Enter', { metaKey: true }, el(app, 'note-input'));
        await app.idle(later, 'list_all_notes');
        assert.equal((await allNotes()).length, 2);
        // A plain Enter is a new line, not a save.
        await select(app, 7);
        app.click(addNoteButton(app));
        const plain = app.key('Enter', {}, el(app, 'note-input'));
        assert.ok(!plain.defaultPrevented);
        assert.ok(panelOpen(app));
    }));

test('N opens the panel only with a selection, and never while typing or with a modifier', () =>
    withNotes([], {}, async (app) => {
        app.key('n');
        assert.ok(!panelOpen(app), 'no selection, no panel');

        await select(app, 16);
        for (const modifier of ['ctrlKey', 'altKey', 'metaKey', 'shiftKey']) {
            app.key(modifier === 'shiftKey' ? 'N' : 'n', { [modifier]: true });
            assert.ok(!panelOpen(app), `not with ${modifier}`);
        }
        // Typing in the search box is typing.
        app.key('/');
        const search = app.searchInput();
        assert.equal(app.document.activeElement, search);
        app.key('n', {}, search);
        assert.ok(!panelOpen(app));
        app.key('Escape', {}, search);

        const event = app.key('n');
        assert.ok(event.defaultPrevented);
        assert.ok(panelOpen(app));
        assert.equal(el(app, 'note-title').textContent, 'John 3:16');
        assert.equal(app.document.activeElement, el(app, 'note-input'));
        // Its own N is just a letter.
        app.key('n', {}, el(app, 'note-input'));
        assert.ok(panelOpen(app));
        app.key('Escape', {}, el(app, 'note-input'));
        assert.ok(!panelOpen(app));
    }));

test('the panel keeps focus and the keyboard to itself, and gives focus back', () =>
    withNotes([[16, 16, 'First.']], {}, async (app) => {
        await select(app, 16);
        app.click(section(app).querySelector('.note-edit'));
        const input = el(app, 'note-input');
        // Tab and Shift+Tab wrap around inside: the textarea, Save, Cancel, Delete.
        const tab = (shiftKey) => {
            const target = app.document.activeElement;
            const event = app.key('Tab', { shiftKey }, target);
            return event.defaultPrevented;
        };
        assert.equal(app.document.activeElement, input);
        assert.ok(tab(true));
        assert.equal(app.document.activeElement, el(app, 'note-delete'));
        assert.ok(tab(false));
        assert.equal(app.document.activeElement, input);
        // Focus that escaped is brought back in.
        input.blur();
        assert.equal(app.document.activeElement, app.document.body);
        assert.ok(tab(false));
        assert.equal(app.document.activeElement, input);

        // None of the reader's shortcuts run.
        for (const key of ['c', '/', 'ArrowRight', 'ArrowLeft', 'n']) app.key(key, {}, input);
        app.key('b', { ctrlKey: true }, input);
        app.key('k', { ctrlKey: true }, input);
        await settle();
        assert.equal(app.heading(), 'John 3');
        assert.ok(!app.isComparing());
        assert.ok(app.searchPanel().hidden);
        assert.ok(app.surface('library').hidden);
        // Clicking the dimmed page does nothing.
        const down = app.click(el(app, 'note-overlay'));
        assert.ok(down.defaultPrevented);
        assert.ok(panelOpen(app));

        // Back to the Edit that opened it.
        const edit = section(app).querySelector('.note-edit');
        app.click(el(app, 'note-cancel'));
        assert.equal(app.document.activeElement, edit);
    }));

test('a note that can’t be saved stays in the panel, with why', () =>
    withNotes([], {}, async (app) => {
        await select(app, 16);
        app.click(addNoteButton(app));
        await typeNote(app, 'Precious words.');
        app.failCommand('add_note', 'The note couldn’t be saved.');
        await clickAndWait(app, el(app, 'note-save'), 'add_note');
        assert.ok(panelOpen(app));
        assert.equal(el(app, 'note-input').value, 'Precious words.');
        assert.equal(el(app, 'note-error').textContent, 'The note wasn’t saved: The note couldn’t be saved. Your text is still here.');
        assert.equal(el(app, 'note-error').getAttribute('role'), 'alert');
        assert.ok(!el(app, 'note-save').disabled, 'it can be tried again');

        app.failCommand('add_note', null);
        await clickAndWait(app, el(app, 'note-save'), 'list_all_notes');
        assert.ok(!panelOpen(app));
        assert.deepEqual((await allNotes()).map((n) => n.body), ['Precious words.']);

        // Too long: Save waits, and says why.
        app.click(addNoteButton(app));
        await typeNote(app, 'x'.repeat(10001));
        assert.ok(el(app, 'note-save').disabled);
        assert.equal(el(app, 'note-length').textContent, '10,001 characters: a note can hold up to 10,000.');
    }));

test('a note’s text is shown as text, never as markup', () => {
    const body = '<img src=x onerror="window.__pwned = true"><b>bold?</b>\n&amp; <script>window.__pwned = true</script>';
    return withNotes([[16, 16, body]], {}, async (app) => {
        await select(app, 16);
        const card = section(app).querySelector('.note-card-body');
        assert.equal(card.textContent, body.trim());
        assert.equal(card.children.length, 0);
        assert.equal(app.document.querySelector('#note-list .note-preview').textContent, body.trim().replace(/\s+/g, ' '));
        assert.equal(app.document.querySelector('#note-list .note-preview').children.length, 0);
        app.click(section(app).querySelector('.note-edit'));
        assert.equal(el(app, 'note-input').value, body.trim());
        await settle();
        assert.equal(app.window.__pwned, undefined);
    });
});

test('notes are translation-independent: one section in compare mode, markers in every column', () =>
    withNotes([[16, 17, 'Both translations.']], { compare: true }, async (app) => {
        assert.ok(app.isComparing());
        assert.deepEqual(notedVerses(app, 'kjv'), [16, 17]);
        assert.deepEqual(notedVerses(app, 'web'), [16, 17]);
        await select(app, 16, 17, 'web');
        assert.equal(app.document.querySelectorAll('.verse-notes').length, 1);
        assert.deepEqual(cards(app), [{ reference: 'John 3:16–17', body: 'Both translations.' }]);
        app.click(addNoteButton(app));
        assert.equal(el(app, 'note-text-shown').textContent, 'Text shown: WEB');
        assert.ok(el(app, 'note-passage').textContent.includes(storedVerse('web', 'John', 3, 16)));
        // Adding lists the notes already on the passage, folded away.
        assert.ok(!el(app, 'note-others').hidden);
        assert.ok(!el(app, 'note-others').open);
        assert.equal(el(app, 'note-others-summary').textContent, 'Other notes on these verses (1)');
        assert.ok(el(app, 'note-others-list').textContent.includes('Both translations.'));
    }));

test('copying the selection copies the verses only, never their notes', () =>
    withNotes([[16, 16, 'Private thought.']], {}, async (app) => {
        await select(app, 16);
        await app.copyButton();
        assert.deepEqual(app.clipboardWrites, [`${storedVerse('kjv', 'John', 3, 16)} — John 3:16 (KJV)`]);
    }));

test('the Library’s Notes tab lists every note in Bible order and opens each one', () =>
    withNotes([[21, 21, 'Light.'], [1, 1, 'Beginning.', bookId('Genesis'), 1], [16, 18, 'Range.\nMore.']], { book: 'Romans', chapter: 8 }, async (app) => {
        app.openLibrary('notes');
        const tab = el(app, 'library-tab-notes');
        assert.equal(tab.getAttribute('aria-selected'), 'true');
        assert.equal(tab.textContent.trim(), 'Notes 3');
        assert.equal(el(app, 'library-notes').hidden, false);
        const rows = () => [...app.document.querySelectorAll('#note-list .bookmark-item')].map((li) => [
            li.querySelector('.bookmark-ref').textContent,
            li.querySelector('.note-preview').textContent,
        ]);
        assert.deepEqual(rows(), [['Genesis 1:1', 'Beginning.'], ['John 3:16–18', 'Range. More.'], ['John 3:21', 'Light.']]);
        // Keyboard: the tabs move with the arrow keys.
        app.key('ArrowLeft', {}, tab);
        assert.equal(el(app, 'library-tab-highlights').getAttribute('aria-selected'), 'true');
        app.key('End', {}, el(app, 'library-tab-highlights'));
        assert.equal(app.document.activeElement, tab);

        // Opening one goes to its chapter with its passage selected.
        await clickAndWait(app, app.document.querySelectorAll('#note-list .bookmark-open')[1], 'list_notes');
        await app.idle();
        assert.equal(app.heading(), 'John 3');
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17, 18]);
        assert.deepEqual(cards(app).map((c) => c.reference), ['John 3:16–18']);
        assert.ok(app.surface('library').hidden);

        // Edit opens it in the panel there.
        app.openLibrary('notes');
        await clickAndWait(app, app.document.querySelectorAll('#note-list .note-list-edit')[2], 'list_notes');
        await app.idle();
        assert.ok(panelOpen(app));
        assert.equal(el(app, 'note-title').textContent, 'John 3:21');
        assert.equal(el(app, 'note-input').value, 'Light.');
        assert.deepEqual(app.selectedVerseNumbers(), [21]);
        app.click(el(app, 'note-cancel'));
        assert.equal(app.document.activeElement, section(app).querySelector('.note-edit'));
    }));

test('the Library says when there are no notes, or they can’t be loaded', () =>
    withNotes([], { failing: { list_all_notes: 'Notes couldn’t be loaded.' } }, async (app) => {
        app.openLibrary('notes');
        assert.equal(app.document.querySelector('#note-list .is-error').textContent, 'Notes couldn’t be loaded: Notes couldn’t be loaded.');
        assert.equal(el(app, 'note-count').textContent, '');
        app.failCommand('list_all_notes', null);
        await select(app, 16);
        app.click(addNoteButton(app));
        await typeNote(app, 'Now it loads.');
        await clickAndWait(app, el(app, 'note-save'), 'list_all_notes');
        assert.equal(el(app, 'note-count').textContent, '1');
    }).then(() => withNotes([], {}, async (app) => {
        app.openLibrary('notes');
        assert.equal(app.document.querySelector('#note-list .bookmark-empty').textContent, 'No notes yet. Click a verse, then + Add note (or press N).');
    })));

test('importing notes refreshes the markers, the section and the Library', () =>
    withNotes([[16, 16, 'Mine.']], {}, async (app) => {
        await select(app, 16);
        assert.equal(cards(app).length, 1);
        const path = join(dir, 'notes.gospel-getter.json');
        writeFileSync(path, JSON.stringify({
            format: 'gospel-getter-reader-data',
            format_version: 3,
            exported_at: '2026-10-10T00:00:00.000Z',
            app_version: '2.8.0',
            reading_position: null,
            bookmarks: [],
            notes: [
                { book: 43, book_name: 'John', chapter: 3, verse_start: 16, verse_end: 16, body: 'Mine.' },
                { book: 43, chapter: 3, verse_start: 14, verse_end: 16, body: 'Lifted up.' },
                { book: 43, chapter: 3, verse_start: 30, verse_end: 30, body: 'Decrease.' },
            ],
        }));
        await app.pick(path);
        app.open('menu');
        let before = app.calls.length;
        app.click(el(app, 'import-data-btn'));
        await app.idle(before);
        const summary = el(app, 'import-summary').textContent;
        assert.ok(summary.includes('3 notes: 2 new, 1 you already have.'), summary);
        assert.equal(el(app, 'import-choice-legend').textContent, 'Bookmarks, highlights and notes');
        assert.equal(el(app, 'import-merge-label').textContent, 'Add them to mine (0 new bookmarks, 0 new highlighted verses, 2 new notes; anything I already have stays as it is)');
        assert.equal(el(app, 'import-replace-label').textContent, 'Replace all my bookmarks, highlights and notes with the file’s (removes my 0 current bookmarks, 0 highlighted verses and 1 note)');
        app.click(el(app, 'import-data-btn').ownerDocument.querySelector('input[name="import-mode"][value="replace"]'));
        app.document.querySelector('input[name="import-mode"][value="replace"]').dispatchEvent(new app.window.Event('change', { bubbles: true }));
        assert.equal(el(app, 'import-replace-warning').textContent, 'This deletes your 0 current bookmarks, 0 highlighted verses and 1 note and keeps only the file’s. If you might want yours back, cancel and export them first.');
        assert.equal(el(app, 'import-confirm').textContent, 'Replace bookmarks, highlights and notes and import');
        app.document.querySelector('input[name="import-mode"][value="merge"]').checked = true;
        app.document.querySelector('input[name="import-mode"][value="merge"]').dispatchEvent(new app.window.Event('change', { bubbles: true }));

        before = app.calls.length;
        app.click(el(app, 'import-confirm'));
        await app.idle(before, 'apply_import');
        assert.match(el(app, 'data-status').textContent, /Added 2 notes \(1 you already had\)\./);
        assert.deepEqual(notedVerses(app), [14, 15, 16, 30]);
        assert.equal(el(app, 'note-count').textContent, '3');
        assert.equal(app.document.querySelectorAll('#note-list .bookmark-item').length, 3);
        // 3:16 is still selected, and its section shows the new note too.
        assert.deepEqual(app.selectedVerseNumbers(), [16]);
        assert.deepEqual(cards(app).map((c) => c.body), ['Lifted up.', 'Mine.']);
    }));

test('export counts notes in its toast, and the file holds them', () =>
    withNotes([[16, 16, 'Exported.']], {}, async (app) => {
        const path = join(dir, 'export.gospel-getter.json');
        await app.pick(path);
        app.open('menu');
        const before = app.calls.length;
        app.click(el(app, 'export-data-btn'));
        await app.idle(before, 'export_reader_data');
        assert.match(el(app, 'data-status').textContent, /^Saved 0 bookmarks, 1 note, your reading position, your display settings to .*\. Bible text isn’t included\.$/);
        const json = JSON.parse((await import('node:fs')).readFileSync(path, 'utf8'));
        assert.equal(json.format_version, 3);
        assert.deepEqual(json.notes.map((n) => [n.book, n.chapter, n.verse_start, n.verse_end, n.body]), [[43, 3, 16, 16, 'Exported.']]);
    }));

test('the note panel never scrolls the page: every focus it moves is preventScroll', () =>
    withNotes([[16, 16, 'First.']], {}, async (app) => {
        const calls = [];
        const focus = app.window.HTMLElement.prototype.focus;
        app.window.HTMLElement.prototype.focus = function (options) {
            calls.push({ el: this, options });
            return focus.call(this, options);
        };
        await select(app, 16);
        // Open, save.
        app.click(addNoteButton(app));
        await typeNote(app, 'Second.');
        await clickAndWait(app, el(app, 'note-save'), 'list_all_notes');
        // Edit, cancel; edit, change, cancel, keep editing, discard.
        app.click(section(app).querySelector('.note-edit'));
        app.click(el(app, 'note-cancel'));
        app.click(section(app).querySelector('.note-edit'));
        await typeNote(app, 'Changed.');
        app.click(el(app, 'note-cancel'));
        app.click(el(app, 'note-keep'));
        app.click(el(app, 'note-cancel'));
        app.click(el(app, 'note-discard-yes'));
        // Delete, undo.
        app.click(section(app).querySelector('.note-edit'));
        await clickAndWait(app, el(app, 'note-delete'), 'list_all_notes');
        await clickAndWait(app, app.status().querySelector('.status-undo'), 'list_all_notes');
        // N, Escape.
        app.key('n');
        app.key('Escape', {}, el(app, 'note-input'));
        assert.ok(calls.length >= 12, `${calls.length} focus calls`);
        const scrolling = calls.filter((c) => !(c.options && c.options.preventScroll));
        assert.deepEqual(scrolling.map((c) => c.el.id || c.el.className), []);
    }));
