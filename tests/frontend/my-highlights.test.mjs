// Behavior tests for the Highlights list in the Library: every highlighted
// passage, grouped and in Bible order, with its color in words, a color
// filter, opening and removing a passage, and the list keeping up with
// highlighting, translation switches and imports — against the real
// backend, database and files.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { backend, bookId, closeBridge, openApp, storedVerse } from './harness.mjs';

const dir = mkdtempSync(join(tmpdir(), 'gospel-getter-my-highlights-'));
after(async () => {
    await closeBridge();
    rmSync(dir, { recursive: true, force: true });
});
let fileCount = 0;
const newPath = (label) => join(dir, `${label}-${++fileCount}.gospel-getter.json`);

const EN = '–';
const EMPTY = 'No highlights yet. Click a verse, then pick a color.';

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

/** Starts from exactly these highlights: [book, chapter, start, end, color]. */
async function seed(passages) {
    await backend('__reset', {});
    for (const [book, chapter, verseStart, verseEnd, color] of passages) {
        await backend('set_highlight', { bookId: bookId(book), chapter, verseStart, verseEnd, color });
    }
}

const list = (app) => app.document.getElementById('highlight-list');
const filterButtons = (app) => [...app.document.querySelectorAll('#highlight-filter .highlight-filter-btn')];
const filterButton = (app, name) => filterButtons(app).find((b) => b.textContent === name);
const emptyText = (app) => list(app).querySelector('.bookmark-empty')?.textContent ?? null;

function items(app) {
    return [...list(app).querySelectorAll('.bookmark-item')].map((li) => ({
        reference: li.querySelector('.bookmark-ref').textContent,
        color: li.querySelector('.highlight-color').textContent,
        swatch: li.querySelector('.highlight-dot').dataset.color,
        preview: li.querySelector('.bookmark-preview').textContent,
        missing: li.querySelector('.bookmark-preview').classList.contains('is-missing'),
        partial: !!li.querySelector('.bookmark-partial'),
        open: li.querySelector('.bookmark-open'),
        remove: li.querySelector('.bookmark-remove'),
    }));
}
const summary = (app) => items(app).map((i) => `${i.reference} ${i.color}`);

async function clickAndWait(app, el, cmd) {
    const before = app.calls.length;
    app.click(el);
    await app.idle(before, cmd);
}

function select(app, start, end = start) {
    app.click(app.verse(start));
    if (end !== start) app.click(app.verse(end), { shiftKey: true });
}

const swatch = (app, color) => app.actions().querySelector(`.highlight-swatch[data-color="${color}"]`);

async function switchTranslation(app, code) {
    const select = app.document.getElementById('translation-select');
    const before = app.calls.length;
    select.value = code;
    select.dispatchEvent(new app.window.Event('change', { bubbles: true }));
    await app.idle(before, 'list_highlight_passages');
}

test('the Highlights tab sits beside Bookmarks in the Library, with a labelled filter and an empty state', () =>
    withApp({}, async (app) => {
        const bookmarksPanel = app.document.getElementById('bookmark-list').closest('[role="tabpanel"]');
        const panel = list(app).closest('[role="tabpanel"]');
        assert.equal(bookmarksPanel.nextElementSibling, panel, 'the tab after Bookmarks');
        assert.ok(panel.closest('#library-panel'));
        const tab = app.document.getElementById(panel.getAttribute('aria-labelledby'));
        assert.equal(tab.getAttribute('role'), 'tab');
        assert.match(tab.textContent, /^Highlights /);
        assert.equal(list(app).getAttribute('aria-labelledby'), tab.id);
        // The Bookmarks list's look and scrolling.
        assert.ok(list(app).classList.contains('bookmark-list'));

        const group = app.document.getElementById('highlight-filter');
        assert.equal(group.getAttribute('role'), 'group');
        assert.equal(group.getAttribute('aria-label'), 'Show highlights');
        assert.deepEqual(filterButtons(app).map((b) => b.textContent), ['All', 'Yellow', 'Green', 'Blue', 'Pink']);
        for (const b of filterButtons(app)) {
            assert.equal(b.tagName, 'BUTTON');
            assert.equal(b.type, 'button');
            assert.equal(b.getAttribute('aria-pressed'), String(b.textContent === 'All'));
        }

        assert.deepEqual(items(app), []);
        assert.equal(emptyText(app), EMPTY);
        assert.deepEqual(app.consoleErrors, []);
    }));

test('highlights are listed as passages in Bible order, with the color named and a preview', async () => {
    await seed([
        ['John', 3, 16, 18, 'green'],
        ['John', 3, 17, 17, 'pink'], // recolors the middle verse
        ['John', 3, 20, 20, 'green'], // after a gap
        ['John', 4, 1, 1, 'green'], // the next chapter
        ['Psalms', 23, 1, 2, 'yellow'],
        ['Psalms', 23, 3, 4, 'yellow'], // runs on: one passage
    ]);
    await withApp({ book: 'John', chapter: 3, translation: 'web', keepBookmarks: true }, async (app) => {
        assert.deepEqual(summary(app), [
            `Psalms 23:1${EN}4 Yellow`,
            'John 3:16 Green',
            'John 3:17 Pink',
            'John 3:18 Green',
            'John 3:20 Green',
            'John 4:1 Green',
        ]);
        const [psalm, john16] = items(app);
        // The color is a swatch and a word, never the swatch alone.
        assert.equal(psalm.swatch, 'yellow');
        assert.equal(john16.swatch, 'green');
        assert.equal(psalm.open.querySelector('.highlight-dot').getAttribute('aria-hidden'), 'true');
        // The first verse's stored text, unshortened (clipped by CSS only).
        assert.equal(psalm.preview, storedVerse('web', 'Psalms', 23, 1));
        assert.equal(john16.preview, storedVerse('web', 'John', 3, 16));
        assert.ok(!psalm.missing && !psalm.partial);
        assert.equal(psalm.remove.getAttribute('aria-label'), `Remove highlight Psalms 23:1${EN}4`);
    });
});

test('the color filter shows one color at a time, says when it has none, and is not remembered', async () => {
    await seed([
        ['John', 3, 16, 17, 'green'],
        ['John', 3, 18, 18, 'pink'],
        ['Romans', 8, 28, 28, 'green'],
    ]);
    let storage;
    await withApp({ book: 'John', chapter: 3, translation: 'kjv', keepBookmarks: true }, async (app) => {
        app.openLibrary('highlights');
        const before = app.calls.length;
        app.click(filterButton(app, 'Green'));
        assert.equal(app.calls.length, before, 'filtering needs no backend call');
        assert.deepEqual(summary(app), [`John 3:16${EN}17 Green`, 'Romans 8:28 Green']);
        assert.deepEqual(filterButtons(app).filter((b) => b.getAttribute('aria-pressed') === 'true').map((b) => b.textContent), ['Green']);

        app.click(filterButton(app, 'Pink'));
        assert.deepEqual(summary(app), ['John 3:18 Pink']);

        for (const color of ['yellow', 'blue']) {
            app.click(filterButton(app, color[0].toUpperCase() + color.slice(1)));
            assert.deepEqual(items(app), []);
            assert.equal(emptyText(app), `No ${color} highlights.`);
        }

        // The filter holds while the list refreshes.
        app.key('Escape');
        select(app, 1);
        await clickAndWait(app, swatch(app, 'blue'), 'list_highlight_passages');
        assert.deepEqual(summary(app), ['John 3:1 Blue']);

        app.click(filterButton(app, 'All'));
        assert.equal(items(app).length, 4);
        app.click(filterButton(app, 'Pink'));
        storage = app.storage();
    });
    // A restart starts from All again.
    await withApp({ book: 'John', chapter: 3, translation: 'kjv', keepBookmarks: true, storage }, async (app) => {
        assert.equal(filterButton(app, 'All').getAttribute('aria-pressed'), 'true');
        assert.equal(items(app).length, 4);
    });
});

test('opening a passage goes to its chapter and selects it, as a bookmark does', async () => {
    await seed([['John', 3, 16, 18, 'yellow']]);
    await withApp({ book: 'Genesis', chapter: 1, translation: 'kjv', keepBookmarks: true }, async (app) => {
        app.openLibrary('highlights');
        await clickAndWait(app, items(app)[0].open, 'get_reading');
        assert.equal(app.heading(), 'John 3');
        assert.equal(app.surface('library').hidden, true, 'the Library gets out of the way');
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17, 18]);
        assert.equal(app.notice(), '');
    });
});

test('Remove takes the highlight off exactly that passage and keeps focus in the list', async () => {
    await seed([
        ['John', 3, 15, 18, 'green'],
        ['John', 3, 17, 17, 'pink'],
    ]);
    await withApp({ book: 'John', chapter: 3, translation: 'kjv', keepBookmarks: true }, async (app) => {
        app.openLibrary('highlights');
        assert.deepEqual(summary(app), [`John 3:15${EN}16 Green`, 'John 3:17 Pink', 'John 3:18 Green']);
        const pink = items(app)[1];
        assert.equal(pink.remove.getAttribute('aria-label'), 'Remove highlight John 3:17');
        await clickAndWait(app, pink.remove, 'list_highlight_passages');

        const removeCalls = app.calls.filter((c) => c.cmd === 'remove_highlight');
        assert.deepEqual(removeCalls.map((c) => JSON.parse(JSON.stringify(c.args))), [{ bookId: bookId('John'), chapter: 3, verseStart: 17, verseEnd: 17 }]);
        // Only verse 17 lost its highlight, in the list and on the page.
        assert.deepEqual(summary(app), [`John 3:15${EN}16 Green`, 'John 3:18 Green']);
        assert.deepEqual([15, 16, 17, 18].map((n) => app.verse(n).dataset.highlight || null), ['green', 'green', null, 'green']);
        assert.equal(app.surface('library').hidden, false, 'removing keeps the Library open');
        assert.equal(app.document.activeElement, items(app)[1].remove, 'focus moves to the next passage');

        await clickAndWait(app, items(app)[1].remove, 'list_highlight_passages');
        await clickAndWait(app, items(app)[0].remove, 'list_highlight_passages');
        assert.equal(emptyText(app), EMPTY);
        assert.equal(app.document.activeElement, filterButton(app, 'All'), 'then to the filter once the list is empty');
        assert.deepEqual(await backend('list_highlights'), []);
    });
});

test('the list keeps up with highlighting in the reader', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        assert.equal(emptyText(app), EMPTY);
        select(app, 16, 18);
        await clickAndWait(app, swatch(app, 'yellow'), 'list_highlight_passages');
        assert.deepEqual(summary(app), [`John 3:16${EN}18 Yellow`]);

        // Recoloring part of it splits the passage.
        app.key('Escape');
        select(app, 18);
        await clickAndWait(app, swatch(app, 'blue'), 'list_highlight_passages');
        assert.deepEqual(summary(app), [`John 3:16${EN}17 Yellow`, 'John 3:18 Blue']);

        // Removing it in the reader takes it off the list.
        app.key('Escape');
        select(app, 16, 18);
        await clickAndWait(app, app.actions().querySelector('.highlight-remove'), 'list_highlight_passages');
        assert.equal(emptyText(app), EMPTY);
        assert.deepEqual(app.consoleErrors, []);
    }));

test('previews follow the translation, and a verse it does not number says so', async () => {
    // Matthew 2:23 is numbered in the WEB only; Romans 16:25-27 in the KJV only.
    await seed([
        ['Matthew', 2, 23, 23, 'pink'],
        ['Romans', 16, 24, 27, 'green'],
    ]);
    await withApp({ book: 'John', chapter: 3, translation: 'web', keepBookmarks: true }, async (app) => {
        let [matthew, romans] = items(app);
        assert.equal(matthew.preview, storedVerse('web', 'Matthew', 2, 23));
        assert.equal(romans.preview, storedVerse('web', 'Romans', 16, 24));
        assert.ok(romans.partial, 'the WEB numbers only part of Romans 16:24–27');

        await switchTranslation(app, 'kjv');
        [matthew, romans] = items(app);
        assert.equal(matthew.reference, 'Matthew 2:23');
        assert.ok(matthew.missing);
        assert.equal(matthew.preview, 'Not numbered in the KJV');
        assert.equal(romans.preview, storedVerse('kjv', 'Romans', 16, 24));
        assert.ok(!romans.partial);

        // Opening it shows the chapter with the bookmarks' notice.
        app.openLibrary('highlights');
        await clickAndWait(app, matthew.open, 'get_reading');
        assert.equal(app.heading(), 'Matthew 2');
        assert.deepEqual(app.selectedVerseNumbers(), []);
        assert.match(app.notice(), /^Matthew 2:23 isn't numbered in the King James Version/);
    });
});

const v2File = (highlights) => ({
    format: 'gospel-getter-reader-data',
    format_version: 2,
    exported_at: '2026-10-08T12:00:00.000Z',
    app_version: '2.5.0',
    reading_position: null,
    bookmarks: [],
    highlights,
});

async function importFile(app, path, mode) {
    await app.pick(path);
    app.open('menu');
    await clickAndWait(app, app.document.getElementById('import-data-btn'), 'choose_import_file');
    const radio = app.document.querySelector(`input[name="import-mode"][value="${mode}"]`);
    radio.checked = true;
    radio.dispatchEvent(new app.window.Event('change', { bubbles: true }));
    await clickAndWait(app, app.document.getElementById('import-confirm'), 'list_highlight_passages');
}

test('the list refreshes after an import, merged or replaced', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        select(app, 16);
        await clickAndWait(app, swatch(app, 'pink'), 'list_highlight_passages');
        app.key('Escape');

        const path = newPath('import');
        writeFileSync(path, JSON.stringify(v2File([
            { book: 43, chapter: 3, verse: 17, color: 'pink' },
            { book: 1, chapter: 1, verse: 1, color: 'blue' },
        ])));
        await importFile(app, path, 'merge');
        assert.deepEqual(summary(app), ['Genesis 1:1 Blue', `John 3:16${EN}17 Pink`]);

        const other = newPath('import');
        writeFileSync(other, JSON.stringify(v2File([{ book: 19, chapter: 23, verse: 1, color: 'green' }])));
        await importFile(app, other, 'replace');
        assert.deepEqual(summary(app), ['Psalms 23:1 Green']);
    }));

test('a Highlights list that fails to load says so', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        app.failCommand('list_highlight_passages', "Highlights couldn't be loaded.");
        await switchTranslation(app, 'web');
        const error = list(app).querySelector('.bookmark-empty.is-error');
        assert.ok(error);
        assert.equal(error.textContent, "Highlights couldn't be loaded: Highlights couldn't be loaded.");
    }));
