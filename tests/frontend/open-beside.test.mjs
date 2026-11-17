// Behavior tests for Open beside (S8), the reader's way into two passages
// side by side: from the selection card's cross-reference preview, a
// Library row, a search result (its own button, or Alt+Enter), a note's
// link (Shift- or Alt-click, or the card's note row), and the palette.
// Each opens the passage in the second pane in the translation of the
// pane it was asked from (D9), the second pane then active (S13); while
// split, it replaces only the second pane's passage, whose Back returns
// (S12). F6 and Shift+F6 switch panes (D10). The panes themselves are
// split.test.mjs's and split-narrow.test.mjs's.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, card, clickAndWaitForCalls as clickAndWait, closeBridge, openApp, settle, withApp } from './harness.mjs';

after(closeBridge);

const JOHN = bookId('John');
const ROMANS = bookId('Romans');
const PSALMS = bookId('Psalms');

const $ = (app, id) => app.document.getElementById(id);
const panes = (app) => app.window.gospelGetterPanes;
const root = (app, n) => $(app, n === 1 ? 'reading-pane' : 'reading-pane-2');
const heading = (app, n) => root(app, n)?.querySelector('.chapter-heading')?.textContent ?? null;
const column = (app, name) => app.document.querySelector(`.pane[data-pane="${name}"]`);
const readings = (app, since = 0) => app.calls.slice(since).filter((c) => c.cmd === 'get_reading')
    .map((c) => JSON.parse(JSON.stringify(c.args)));
const selectedIn = (app, n) => [...root(app, n).querySelectorAll('.chapter-main .verse[data-translation].verse-selected')]
    .map((v) => Number(v.dataset.verse));
const codeIn = (app, n) => root(app, n).querySelector('.chapter-main .verse[data-translation]').dataset.translation;

/** The panes as the reader meets them after an Open beside. */
const seen = (app) => ({
    shown: [...panes(app).shown()],
    active: panes(app).active(),
    first: heading(app, 1),
    second: heading(app, 2),
});

/** Clicks `el` (an Open beside) and waits for the chapter it reads. */
async function clickBeside(app, el, opts = {}) {
    const before = app.calls.length;
    app.click(el, opts);
    await app.idle(before, 'get_reading');
    return readings(app, before);
}

/** Selects verse `verse` of pane `n`'s chapter, as a press and a click on it, and waits for its notes. */
async function selectIn(app, n, verse) {
    const el = root(app, n).querySelector(`.chapter-main .verse[data-translation][data-verse="${verse}"]`);
    el.dispatchEvent(new app.window.MouseEvent('pointerdown', { bubbles: true }));
    const before = app.calls.length;
    app.click(el);
    await app.idle(before, 'list_notes');
}

/** Back in the active pane, and waits for its chapter. */
async function back(app) {
    const before = app.calls.length;
    app.key('ArrowLeft', { altKey: true });
    await app.idle(before, 'get_reading');
}

/** Opens the search box (or the palette) and types `text`, waiting for any search. */
async function type(app, text) {
    if (app.searchPanel().hidden) app.key('k', { ctrlKey: true });
    const before = app.calls.length;
    app.type(app.searchInput(), text);
    await new Promise((r) => setTimeout(r, 200));
    await app.idle(before);
}

const commandLabels = (app) => [...app.document.querySelectorAll('#search-results .search-command .palette-label')]
    .map((el) => el.textContent);
const command = (app, label) => [...app.document.querySelectorAll('#search-results .search-command')]
    .find((el) => el.querySelector('.palette-label').textContent === label);

test('the cross-reference preview’s Open beside opens the passage in the second pane, in this pane’s translation, active', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        await clickAndWait(app, app.verse(16), 'list_notes');
        const chip = [...card(app).querySelectorAll('.xref-citation')].find((c) => c.textContent === 'Romans 5:8');
        assert.ok(chip, 'fixture: John 3:16 cites Romans 5:8');
        await clickAndWait(app, chip, 'get_passage');
        const preview = $(app, 'xref-preview');
        const go = preview.querySelector('.xref-preview-go');
        const beside = preview.querySelector('.xref-preview-beside');
        // After Go to, a real button, named for where it goes.
        assert.equal(beside.previousElementSibling, go);
        assert.equal(beside.tagName, 'BUTTON');
        assert.equal(beside.type, 'button');
        assert.equal(beside.textContent, 'Open beside');
        assert.equal(beside.getAttribute('aria-label'), 'Open Romans 5:8 beside');
        assert.equal(beside.querySelector('svg').getAttribute('aria-hidden'), 'true');
        // Enter or Space on it is its own (a click), as on Go to.
        beside.focus();
        assert.equal(app.key('Enter', {}, beside).defaultPrevented, false);

        assert.deepEqual(await clickBeside(app, beside), [{ bookId: ROMANS, chapter: 5, translationCode: 'web' }]);
        assert.deepEqual(seen(app), { shown: ['first', 'second'], active: 'second', first: 'John 3', second: 'Romans 5' });
        assert.equal(codeIn(app, 2), 'web');
        assert.deepEqual(selectedIn(app, 2), [8], 'its verse selected, as Go to selects it');
        assert.equal(app.document.documentElement.dataset.split, 'on');
    }));

test('a Library row’s Open beside closes the Library and opens it beside; while split, it replaces only the second pane, whose Back returns (S12)', async () => {
    await backend('__reset', { bookId: JOHN, chapter: 3, translationCode: 'kjv' });
    await backend('add_bookmark', { bookId: JOHN, chapter: 3, verseStart: 16, verseEnd: 16 });
    await backend('set_highlight', { bookId: PSALMS, chapter: 23, verseStart: 1, verseEnd: 1, color: 'yellow' });
    await backend('add_note', { bookId: ROMANS, chapter: 5, verseStart: 8, verseEnd: 8, body: 'Love first.' });
    const app = await openApp({ book: 'Romans', chapter: 8, translation: 'bsb', keepBookmarks: true });
    try {
        const besideIn = (list) => {
            const row = app.document.querySelector(`#${list}-list .bookmark-item`);
            return row.querySelector('.library-beside');
        };
        // Bookmarks: one pane, so it opens the second.
        app.openLibrary('bookmarks');
        let beside = besideIn('bookmark');
        assert.equal(beside.textContent.trim(), 'Open beside');
        assert.equal(beside.getAttribute('aria-label'), 'Open John 3:16 beside');
        assert.equal(beside.title, 'Open John 3:16 beside');
        assert.deepEqual(await clickBeside(app, beside), [{ bookId: JOHN, chapter: 3, translationCode: 'bsb' }]);
        assert.ok(app.surface('library').hidden, 'the Library closed');
        assert.deepEqual(seen(app), { shown: ['first', 'second'], active: 'second', first: 'Romans 8', second: 'John 3' });
        assert.deepEqual(selectedIn(app, 2), [16]);
        assert.equal(codeIn(app, 2), 'bsb');

        // Highlights, while split: the second pane's passage is replaced,
        // the first untouched, even from the first pane.
        root(app, 1).querySelector('.chapter-main .verse[data-translation]')
            .dispatchEvent(new app.window.MouseEvent('pointerdown', { bubbles: true }));
        assert.equal(panes(app).active(), 'first');
        app.openLibrary('highlights');
        beside = besideIn('highlight');
        assert.equal(beside.getAttribute('aria-label'), 'Open Psalms 23:1 beside');
        assert.deepEqual(await clickBeside(app, beside), [{ bookId: PSALMS, chapter: 23, translationCode: 'bsb' }]);
        assert.deepEqual(seen(app), { shown: ['first', 'second'], active: 'second', first: 'Romans 8', second: 'Psalms 23' });
        await back(app);
        assert.deepEqual(seen(app), { shown: ['first', 'second'], active: 'second', first: 'Romans 8', second: 'John 3' });

        // Notes.
        app.openLibrary('notes');
        beside = besideIn('note');
        assert.equal(beside.getAttribute('aria-label'), 'Open Romans 5:8 beside');
        assert.deepEqual(await clickBeside(app, beside), [{ bookId: ROMANS, chapter: 5, translationCode: 'bsb' }]);
        assert.ok(app.surface('library').hidden);
        assert.ok($(app, 'note-overlay').hidden, 'opened beside, not edited');
        assert.deepEqual(seen(app), { shown: ['first', 'second'], active: 'second', first: 'Romans 8', second: 'Romans 5' });
        assert.deepEqual(selectedIn(app, 2), [8]);
    } finally {
        app.close();
    }
});

test('a verse found by search opens beside from its own button, and the active result with Alt+Enter', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        await app.search('shepherd');
        const result = app.searchResults().find((r) => !r.classList.contains('search-go'));
        const beside = result.querySelector('.search-beside');
        const reference = result.querySelector('.search-ref').textContent;
        assert.equal(beside.textContent.trim(), 'Open beside');
        assert.equal(beside.getAttribute('aria-label'), `Open ${reference} beside`);
        assert.equal(beside.title, 'Open beside (Alt+Enter)', 'its tooltip names its key');
        assert.equal(beside.tabIndex, -1, 'the box keeps focus');
        const [{ bookId: book, chapter, translationCode }] = await clickBeside(app, beside);
        assert.equal(translationCode, 'web');
        assert.ok(app.searchPanel().hidden, 'the box closed');
        assert.equal(panes(app).active(), 'second');
        assert.equal(heading(app, 1), 'John 3');
        assert.equal(`${heading(app, 2)}:${selectedIn(app, 2)[0]}`, reference);
        assert.equal(Number(root(app, 2).querySelector('[data-current-book]').dataset.currentBook), book);
        assert.equal(Number(root(app, 2).querySelector('[data-current-chapter]').dataset.currentChapter), chapter);

        // Alt+Enter on the active option, here Go to: beside, replacing the
        // second pane's passage.
        await type(app, 'Ps 23:1');
        assert.equal(app.searchResults()[0].textContent.trim(), 'Go to Psalms 23:1');
        let before = app.calls.length;
        const event = app.key('Enter', { altKey: true }, app.searchInput());
        await app.idle(before, 'get_reading');
        assert.ok(event.defaultPrevented);
        assert.deepEqual(readings(app, before), [{ bookId: PSALMS, chapter: 23, translationCode: 'web' }]);
        assert.deepEqual(seen(app), { shown: ['first', 'second'], active: 'second', first: 'John 3', second: 'Psalms 23' });
        assert.deepEqual(selectedIn(app, 2), [1]);

        // And on a verse: ↓ to it first.
        await type(app, 'shepherd');
        const options = () => [...app.document.querySelectorAll('#search-results [role="option"]')];
        const verse = options().findIndex((o) => o.classList.contains('search-result') && !o.classList.contains('search-go'));
        for (let i = 0; i <= verse; i++) app.key('ArrowDown', {}, app.searchInput());
        const chosen = options()[verse].querySelector('.search-ref').textContent;
        before = app.calls.length;
        app.key('Enter', { altKey: true }, app.searchInput());
        await app.idle(before, 'get_reading');
        assert.equal(`${heading(app, 2)}:${selectedIn(app, 2)[0]}`, chosen);
        assert.equal(heading(app, 1), 'John 3');

        // Plain Enter still goes there, in the active pane.
        await type(app, 'Ps 23:1');
        before = app.calls.length;
        app.key('Enter', {}, app.searchInput());
        await app.idle(before, 'get_reading');
        assert.deepEqual(seen(app), { shown: ['first', 'second'], active: 'second', first: 'John 3', second: 'Psalms 23' });
    }));

test('a note’s link opens beside with Shift-click or Alt-click, or from the card’s note row; a plain click is still Go to', async () => {
    await backend('__reset', { bookId: JOHN, chapter: 3, translationCode: 'kjv' });
    await backend('add_note', { bookId: JOHN, chapter: 3, verseStart: 16, verseEnd: 16, body: 'See Rom 5:8, then Ps 23. And `Rom 8:1` is code.' });
    const app = await openApp({ book: 'John', chapter: 3, translation: 'web', keepBookmarks: true });
    try {
        const cardNote = () => app.document.querySelector('.verse-notes .note-card');
        const link = (text) => [...cardNote().querySelectorAll('a.note-ref-link')].find((a) => a.textContent === text);
        await selectIn(app, 1, 16);
        // The note row: one Open beside per passage the note links to.
        const row = [...cardNote().querySelectorAll('.note-beside')];
        assert.deepEqual(row.map((b) => [b.textContent, b.getAttribute('aria-label')]),
            [['Open beside', 'Open Romans 5:8 beside'], ['Open beside', 'Open Psalms 23 beside']]);
        assert.ok(row.every((b) => b.tagName === 'BUTTON' && b.closest('.note-card-actions')));

        // Shift-click: beside, in this pane's translation, its verse selected.
        assert.deepEqual(await clickBeside(app, link('Rom 5:8'), { shiftKey: true }),
            [{ bookId: ROMANS, chapter: 5, translationCode: 'web' }]);
        assert.deepEqual(seen(app), { shown: ['first', 'second'], active: 'second', first: 'John 3', second: 'Romans 5' });
        assert.deepEqual(selectedIn(app, 2), [8]);
        assert.equal(app.window.location.hash, '', 'the page itself followed nothing');

        // Alt-click, from the first pane's card again: a chapter alone,
        // replacing the second pane's passage.
        await selectIn(app, 1, 16);
        assert.equal(panes(app).active(), 'first');
        assert.deepEqual(await clickBeside(app, link('Ps 23'), { altKey: true }),
            [{ bookId: PSALMS, chapter: 23, translationCode: 'web' }]);
        assert.deepEqual(seen(app), { shown: ['first', 'second'], active: 'second', first: 'John 3', second: 'Psalms 23' });
        assert.deepEqual(selectedIn(app, 2), []);

        // The note row's Open beside, without a modifier.
        await selectIn(app, 1, 16);
        const romans = [...cardNote().querySelectorAll('.note-beside')][0];
        assert.deepEqual(await clickBeside(app, romans), [{ bookId: ROMANS, chapter: 5, translationCode: 'web' }]);
        assert.deepEqual(seen(app), { shown: ['first', 'second'], active: 'second', first: 'John 3', second: 'Romans 5' });

        // A plain click is Go to, in the pane it's in.
        await selectIn(app, 1, 16);
        await clickAndWait(app, link('Rom 5:8'), 'get_reading');
        assert.deepEqual(seen(app), { shown: ['first', 'second'], active: 'first', first: 'Romans 5', second: 'Romans 5' });
    } finally {
        app.close();
    }
});

test('F6 and Shift+F6 switch panes, focus on the reference button acting on it; with one pane they do nothing', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        // One pane: nothing, and the webview keeps the key.
        for (const shiftKey of [false, true]) {
            const event = await app.key('F6', { shiftKey });
            assert.equal(event.defaultPrevented, false);
            assert.deepEqual([...panes(app).shown()], ['first']);
        }
        await panes(app).openBeside({ bookId: ROMANS, chapter: 5, translation: 'bsb' });
        await settle();
        const reference = (name) => column(app, name).querySelector('.pane-bar .ref-btn');
        for (const [shiftKey, to] of [[false, 'first'], [false, 'second'], [true, 'first'], [true, 'second']]) {
            const event = await app.key('F6', { shiftKey }, app.document.activeElement ?? app.document.body);
            assert.equal(event.defaultPrevented, true);
            assert.equal(panes(app).active(), to);
            assert.equal(app.document.activeElement, reference(to), `${shiftKey ? 'Shift+' : ''}F6: focus`);
            assert.equal(column(app, to).dataset.paneActive, 'true');
        }
        assert.equal($(app, 'translation-select').value, 'bsb', 'the second pane’s translation, as it’s active');

        // F6 isn't a character: it switches with Single-key shortcuts off.
        app.open('aa');
        await app.click($(app, 'single-keys-toggle'));
        assert.equal(app.document.documentElement.dataset.singleKeyShortcuts, 'off');
        app.key('Escape');
        const event = await app.key('F6');
        assert.equal(event.defaultPrevented, true);
        assert.equal(panes(app).active(), 'first');

        // Tabbed, the top bar's reference button acts on the active pane.
        app.resize(800);
        await app.key('F6');
        assert.equal(panes(app).active(), 'second');
        assert.equal(app.document.activeElement, $(app, 'ref-btn'));
        assert.ok(column(app, 'first').hidden);
    }));

test('the palette: Open this passage beside with one pane; Switch pane (F6) and Close the second pane only while split', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        await type(app, '>pane');
        assert.deepEqual(commandLabels(app), ['Open this passage beside'], 'neither Switch pane nor Close the second pane with one pane');
        await type(app, '>open this');
        assert.deepEqual(commandLabels(app), ['Open this passage beside']);
        let before = app.calls.length;
        app.key('Enter', {}, app.searchInput());
        await app.idle(before, 'get_reading');
        assert.deepEqual(readings(app, before), [{ bookId: JOHN, chapter: 3, translationCode: 'web' }]);
        assert.deepEqual(seen(app), { shown: ['first', 'second'], active: 'second', first: 'John 3', second: 'John 3' });

        await type(app, '>pane');
        assert.deepEqual(commandLabels(app), ['Open this passage beside', 'Switch pane (F6)', 'Close the second pane']);
        const switching = command(app, 'Switch pane (F6)');
        assert.equal(switching.getAttribute('aria-keyshortcuts'), 'F6 Shift+F6');
        assert.equal(switching.querySelector('kbd'), null, 'its label names its key already');
        await type(app, '>switch');
        assert.deepEqual(commandLabels(app), ['Switch pane (F6)']);
        app.key('Enter', {}, app.searchInput());
        await settle();
        assert.equal(panes(app).active(), 'first');
        assert.ok(app.searchPanel().hidden);

        await type(app, '>close the second');
        assert.deepEqual(commandLabels(app), ['Close the second pane']);
        app.key('Enter', {}, app.searchInput());
        await settle();
        assert.deepEqual([...panes(app).shown()], ['first']);
        assert.equal(app.document.activeElement, $(app, 'ref-btn'));
        await type(app, '>pane');
        assert.deepEqual(commandLabels(app), ['Open this passage beside']);
    }));

test('opened beside while tabbed, the second pane’s tab shows; in reading mode, the second pane shows', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv', width: 800 }, async (app) => {
        await panes(app).openBeside({ bookId: ROMANS, chapter: 5 });
        await settle();
        // The first tab chosen, then a Library row opened beside.
        await app.click($(app, 'pane-tab-first'));
        assert.ok(column(app, 'second').hidden);
        await backend('add_bookmark', { bookId: PSALMS, chapter: 23, verseStart: 1, verseEnd: 1 });
        await app.window.gospelGetterLibrary.refresh(['bookmarks']);
        app.openLibrary('bookmarks');
        const row = [...app.document.querySelectorAll('#bookmark-list .bookmark-item')]
            .find((r) => r.querySelector('.bookmark-ref').textContent === 'Psalms 23:1');
        await clickBeside(app, row.querySelector('.library-beside'));
        assert.equal($(app, 'pane-tab-second').getAttribute('aria-selected'), 'true');
        assert.ok(!column(app, 'second').hidden && column(app, 'first').hidden);
        assert.equal(heading(app, 2), 'Psalms 23');

        // Reading mode, the first pane shown: a note's link Alt-clicked
        // there shows the second.
        app.resize(1280);
        await app.click($(app, 'pane-tab-first'));
        root(app, 1).querySelector('.chapter-main .verse[data-translation]')
            .dispatchEvent(new app.window.MouseEvent('pointerdown', { bubbles: true }));
        app.key('r');
        await settle();
        assert.equal(app.document.documentElement.dataset.readerMode, 'on');
        assert.ok(column(app, 'second').hidden);
        await clickAndWait(app, app.verse(16), 'list_notes');
        const chip = [...card(app).querySelectorAll('.xref-citation')].find((c) => c.textContent === 'Romans 5:8');
        await clickAndWait(app, chip, 'get_passage');
        await clickBeside(app, $(app, 'xref-preview').querySelector('.xref-preview-beside'));
        assert.equal(app.document.documentElement.dataset.readerMode, 'on');
        assert.equal(panes(app).active(), 'second');
        assert.ok(!column(app, 'second').hidden && column(app, 'first').hidden);
        assert.equal(heading(app, 2), 'Romans 5');
    }));
