// Behavior tests for verse highlighting: the four color swatches beside
// Copy and ★ Bookmark, applying, recoloring and removing over a selection,
// how highlights show (the chapter, its faded neighbors and both compare
// columns), persistence, and highlights in export and import — against the
// real backend, database and files.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { backend, bookId, closeBridge, openApp, settle } from './harness.mjs';

const dir = mkdtempSync(join(tmpdir(), 'gospel-getter-highlights-'));
after(async () => {
    await closeBridge();
    rmSync(dir, { recursive: true, force: true });
});
let fileCount = 0;
const newPath = (label) => join(dir, `${label}-${++fileCount}.gospel-getter.json`);

const COLORS = ['yellow', 'green', 'blue', 'pink'];

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const swatch = (app, color) => app.actions().querySelector(`.highlight-swatch[data-color="${color}"]`);
const removeButton = (app) => app.actions().querySelector('.highlight-remove');
const pressed = (app) => COLORS.filter((c) => swatch(app, c).getAttribute('aria-pressed') === 'true');
const statusText = (app) => app.status().textContent;

/** The highlight color shown on verses `from`..`to` (in column `code`). */
const shown = (app, from, to, code) => {
    const out = [];
    for (let n = from; n <= to; n++) out.push(app.verse(n, code).dataset.highlight || null);
    return out;
};

function select(app, start, end = start, code) {
    app.click(app.verse(start, code));
    if (end !== start) app.click(app.verse(end, code), { shiftKey: true });
}

async function choose(app, button) {
    const before = app.calls.length;
    app.click(button);
    await app.idle(before);
}

async function storedHighlights() {
    return (await backend('list_highlights')).map((h) => `${h.bookId} ${h.chapter}:${h.verse} ${h.color}`);
}

test('the swatches are four labelled buttons, with Remove highlight hidden at first', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        select(app, 16);
        const group = app.actions().querySelector('.verse-highlight');
        assert.equal(group.getAttribute('role'), 'group');
        assert.equal(group.getAttribute('aria-label'), 'Highlight');
        for (const color of COLORS) {
            const b = swatch(app, color);
            // Native buttons, so Tab, Enter and Space work as for Copy.
            assert.equal(b.tagName, 'BUTTON');
            assert.equal(b.type, 'button');
            assert.equal(b.disabled, false);
            assert.equal(b.getAttribute('tabindex'), null);
            assert.equal(b.getAttribute('aria-label'), `Highlight ${color}`);
            assert.equal(b.getAttribute('aria-pressed'), 'false');
        }
        assert.equal(removeButton(app).hidden, true);
        assert.equal(removeButton(app).textContent, 'Remove highlight');
    }));

test('each color highlights one verse, and the chosen color shows as pressed', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        for (const color of COLORS) {
            select(app, 16);
            await choose(app, swatch(app, color));
            assert.deepEqual(shown(app, 15, 17), [null, color, null]);
            assert.deepEqual(pressed(app), [color]);
            assert.equal(removeButton(app).hidden, false);
            assert.equal(statusText(app), `Highlighted ${color}`);
            assert.deepEqual(await storedHighlights(), [`${bookId('John')} 3:16 ${color}`]);
            app.key('Escape');
        }
        assert.deepEqual(app.consoleErrors, []);
    }));

test('a range is highlighted, partly recolored and removed, verse by verse', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        select(app, 16, 18);
        await choose(app, swatch(app, 'green'));
        assert.deepEqual(shown(app, 15, 19), [null, 'green', 'green', 'green', null]);
        assert.deepEqual(pressed(app), ['green']);

        // Recolor the middle verse: it replaces just that verse's color.
        app.key('Escape');
        select(app, 17);
        await choose(app, swatch(app, 'pink'));
        assert.deepEqual(shown(app, 16, 18), ['green', 'pink', 'green']);

        // A mixed selection has no pressed color, but can be removed.
        app.key('Escape');
        select(app, 15, 18);
        assert.deepEqual(pressed(app), []);
        assert.equal(removeButton(app).hidden, false);
        // Choosing a color replaces every selected verse's color.
        await choose(app, swatch(app, 'blue'));
        assert.deepEqual(shown(app, 15, 18), ['blue', 'blue', 'blue', 'blue']);

        // Remove highlight, reached from the keyboard: focus moves to the
        // first swatch once the button disappears.
        removeButton(app).focus();
        await choose(app, removeButton(app));
        assert.deepEqual(shown(app, 15, 18), [null, null, null, null]);
        assert.equal(removeButton(app).hidden, true);
        assert.equal(app.document.activeElement, swatch(app, 'yellow'));
        assert.equal(statusText(app), 'Highlight removed');
        assert.deepEqual(await storedHighlights(), []);
    }));

test('highlights survive a restart, and highlighting doesn\'t reload the chapter', async () => {
    await withApp({ book: 'Psalms', chapter: 23, translation: 'kjv' }, async (app) => {
        select(app, 1, 2);
        const before = app.calls.length;
        await choose(app, swatch(app, 'yellow'));
        const calls = app.calls.slice(before).map((c) => c.cmd);
        assert.deepEqual(calls, ['set_highlight']);
    });
    await withApp({ book: 'Psalms', chapter: 23, translation: 'kjv', keepBookmarks: true }, async (app) => {
        assert.deepEqual(shown(app, 1, 3), ['yellow', 'yellow', null]);
    });
});

test('highlights show in both compare columns and in the faded neighbors', async () => {
    await backend('__reset', {});
    await backend('set_highlight', { bookId: bookId('John'), chapter: 3, verseStart: 16, verseEnd: 16, color: 'yellow' });
    await backend('set_highlight', { bookId: bookId('John'), chapter: 2, verseStart: 1, verseEnd: 1, color: 'pink' });
    await backend('set_highlight', { bookId: bookId('John'), chapter: 4, verseStart: 2, verseEnd: 2, color: 'green' });

    await withApp({ book: 'John', chapter: 3, translation: 'kjv', keepBookmarks: true }, async (app) => {
        assert.equal(app.verse(16).dataset.highlight, 'yellow');
        assert.equal(app.sideVerse('prev', 1).dataset.highlight, 'pink');
        assert.equal(app.sideVerse('prev', 2).dataset.highlight, undefined);
        assert.equal(app.sideVerse('next', 2).dataset.highlight, 'green');
    });

    await withApp({ book: 'John', chapter: 3, translation: 'kjv', compare: true, keepBookmarks: true }, async (app) => {
        assert.ok(app.isComparing());
        assert.equal(app.verse(16, 'kjv').dataset.highlight, 'yellow');
        assert.equal(app.verse(16, 'web').dataset.highlight, 'yellow');
        // Highlighting from either column marks the verse in both.
        select(app, 17, 17, 'web');
        await choose(app, swatch(app, 'blue'));
        assert.equal(app.verse(17, 'kjv').dataset.highlight, 'blue');
        assert.equal(app.verse(17, 'web').dataset.highlight, 'blue');
    });
});

test('a highlight on a verse one translation doesn\'t number shows nothing there', async () => {
    await backend('__reset', {});
    // Matthew 2:23 is numbered in the WEB, not the KJV.
    await backend('set_highlight', { bookId: bookId('Matthew'), chapter: 2, verseStart: 23, verseEnd: 23, color: 'green' });
    await withApp({ book: 'Matthew', chapter: 2, translation: 'web', keepBookmarks: true }, async (app) => {
        assert.equal(app.verse(23).dataset.highlight, 'green');
    });
    await withApp({ book: 'Matthew', chapter: 2, translation: 'kjv', keepBookmarks: true }, async (app) => {
        assert.equal(app.document.querySelectorAll('.chapter-main .verse[data-highlight]').length, 0);
        assert.deepEqual(app.consoleErrors, []);
    });
});

test('a failed highlight is reported and changes nothing', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.failCommand('set_highlight', 'The highlight couldn\'t be saved.');
        select(app, 16);
        await choose(app, swatch(app, 'yellow'));
        assert.equal(app.verse(16).dataset.highlight, undefined);
        assert.match(statusText(app), /^Couldn't highlight: The highlight couldn't be saved\.$/);
        assert.ok(app.status().classList.contains('is-error'));
    }));

test('existing highlights mean no first-run welcome', async () => {
    await backend('__reset', {});
    await backend('set_highlight', { bookId: bookId('John'), chapter: 3, verseStart: 16, verseEnd: 16, color: 'yellow' });
    await withApp({ fresh: true, keepBookmarks: true }, async (app) => {
        await app.idle();
        await settle();
        assert.equal(app.document.getElementById('welcome-overlay').hidden, true);
    });
});

// ---- Export and import

async function clickData(app, id, path) {
    await app.pick(path);
    app.openSettings();
    const before = app.calls.length;
    app.click(app.document.getElementById(id));
    await app.idle(before);
}

const dataStatus = (app) => app.document.getElementById('data-status').textContent;

test('export saves highlights, and says so', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        select(app, 16, 17);
        await choose(app, swatch(app, 'yellow'));
        app.key('Escape');
        const path = newPath('export');
        await clickData(app, 'export-data-btn', path);
        assert.equal(dataStatus(app), `Saved 0 bookmarks, 2 highlighted verses, your reading position, your display settings to ${path}.`);
        const json = JSON.parse(readFileSync(path, 'utf8'));
        assert.equal(json.format_version, 2);
        assert.deepEqual(
            json.highlights.map((h) => [h.book, h.book_name, h.chapter, h.verse, h.color]),
            [[43, 'John', 3, 16, 'yellow'], [43, 'John', 3, 17, 'yellow']],
        );
    }));

const v2File = (highlights) => ({
    format: 'gospel-getter-reader-data',
    format_version: 2,
    exported_at: '2026-10-08T12:00:00.000Z',
    app_version: '2.5.0',
    reading_position: null,
    bookmarks: [],
    highlights,
});

test('the import preview counts a format 2 file\'s highlights, and merging keeps the reader\'s colors', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        select(app, 16);
        await choose(app, swatch(app, 'pink'));
        app.key('Escape');

        const path = newPath('import');
        writeFileSync(path, JSON.stringify(v2File([
            { book: 43, chapter: 3, verse: 16, color: 'yellow' },
            { book: 43, chapter: 3, verse: 17, color: 'yellow' },
        ])));
        await clickData(app, 'import-data-btn', path);
        assert.equal(app.document.getElementById('import-overlay').hidden, false);
        assert.equal(app.document.getElementById('import-title').textContent, 'Import bookmarks, highlights and settings');
        const summary = [...app.document.querySelectorAll('#import-summary li')].map((li) => li.textContent);
        assert.ok(summary.includes('No bookmarks.'), summary.join(' | '));
        assert.ok(
            summary.includes('2 highlighted verses: 1 new, 1 you’ve already highlighted (those keep your colors).'),
            summary.join(' | '),
        );
        assert.equal(app.document.getElementById('import-choice-legend').textContent, 'Bookmarks and highlights');
        assert.equal(
            app.document.getElementById('import-merge-label').textContent,
            'Add them to mine (0 new bookmarks, 1 new highlighted verse; anything I already have stays as it is)',
        );
        assert.equal(
            app.document.getElementById('import-replace-label').textContent,
            'Replace all my bookmarks and highlights with the file’s (removes my 0 current bookmarks and 1 highlighted verse)',
        );

        const before = app.calls.length;
        app.click(app.document.getElementById('import-confirm'));
        await app.idle(before);
        assert.match(dataStatus(app), /Added 1 highlighted verse \(1 you’d already highlighted kept your colors\)\./);
        // On screen straight away, the reader's own color kept.
        assert.deepEqual(shown(app, 16, 17), ['pink', 'yellow']);
    }));

test('replacing from a format 2 file replaces highlights; a format 1 file leaves them alone', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        select(app, 16, 18);
        await choose(app, swatch(app, 'green'));
        app.key('Escape');

        // Format 1: the preview says nothing about highlights, and replacing
        // swaps bookmarks only.
        const v1 = newPath('v1');
        writeFileSync(v1, JSON.stringify({
            format: 'gospel-getter-reader-data', format_version: 1, exported_at: 'x', app_version: '2.4.0',
            reading_position: null, bookmarks: [{ book: 1, chapter: 1, verse_start: 1, verse_end: 1 }],
        }));
        await clickData(app, 'import-data-btn', v1);
        const v1Summary = app.document.getElementById('import-summary').textContent;
        assert.ok(!/highlight/i.test(v1Summary), v1Summary);
        assert.equal(app.document.getElementById('import-choice-legend').textContent, 'Bookmarks');
        app.document.querySelector('input[name="import-mode"][value="replace"]').checked = true;
        app.document.querySelector('input[name="import-mode"][value="replace"]').dispatchEvent(new app.window.Event('change', { bubbles: true }));
        assert.equal(app.document.getElementById('import-confirm').textContent, 'Replace bookmarks and import');
        let before = app.calls.length;
        app.click(app.document.getElementById('import-confirm'));
        await app.idle(before);
        assert.deepEqual(shown(app, 16, 18), ['green', 'green', 'green']);

        // Format 2: replacing swaps the highlights too, with a warning first.
        const v2 = newPath('v2');
        writeFileSync(v2, JSON.stringify(v2File([{ book: 43, chapter: 3, verse: 20, color: 'blue' }])));
        await clickData(app, 'import-data-btn', v2);
        const replace = app.document.querySelector('input[name="import-mode"][value="replace"]');
        replace.checked = true;
        replace.dispatchEvent(new app.window.Event('change', { bubbles: true }));
        assert.match(
            app.document.getElementById('import-replace-warning').textContent,
            /^This deletes your 1 current bookmark and 3 highlighted verses and keeps only the file’s\./,
        );
        assert.equal(app.document.getElementById('import-confirm').textContent, 'Replace bookmarks and highlights and import');
        before = app.calls.length;
        app.click(app.document.getElementById('import-confirm'));
        await app.idle(before);
        assert.match(dataStatus(app), /Replaced 3 highlighted verses with the file’s 1 highlighted verse\./);
        assert.deepEqual(shown(app, 16, 20), [null, null, null, null, 'blue']);
    }));
