// One reading session across GH-10 to GH-13 together — search, select,
// copy, bookmark, switch translation, compare, reopen a bookmark, turn
// chapters, reading mode — against the real backend and database.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { closeBridge, openApp, storedVerse } from './harness.mjs';

after(closeBridge);

const EM = '—';
const EN = '–';

test('search, select, copy, bookmark, switch, compare, reopen and read on', async () => {
    const app = await openApp({ book: 'John', chapter: 3, translation: 'kjv' });
    try {
        const select = app.document.getElementById('translation-select');
        const switchTo = async (code) => {
            const before = app.calls.length;
            select.value = code;
            select.dispatchEvent(new app.window.Event('change', { bubbles: true }));
            await app.idle(before, 'get_reading');
        };

        // Search (KJV) and open a result.
        await app.search('brother keeper');
        const result = app.searchResults().find((r) => r.querySelector('.search-ref').textContent === 'Genesis 4:9');
        assert.ok(result);
        let before = app.calls.length;
        app.click(result);
        await app.idle(before, 'get_reading');
        assert.equal(app.heading(), 'Genesis 4');
        assert.deepEqual(app.selectedVerseNumbers(), [9]);

        // Extend to a range, copy it, bookmark it.
        app.click(app.verse(11), { shiftKey: true });
        assert.deepEqual(app.selectedVerseNumbers(), [9, 10, 11]);
        app.key('c', { ctrlKey: true });
        await app.idle(0);
        const kjv = [9, 10, 11].map((n) => storedVerse('kjv', 'Genesis', 4, n)).join(' ');
        assert.equal(app.clipboardWrites.at(-1), `${kjv} ${EM} Genesis 4:9${EN}11 (KJV)`);
        before = app.calls.length;
        app.click(app.bookmarkButton());
        await app.idle(before, 'list_bookmarks');
        assert.deepEqual(app.bookmarkedVerseNumbers(), [9, 10, 11]);

        // Switch to the WEB: the selection goes, the bookmark stays and
        // previews in the WEB, and the open search reruns in the WEB.
        await switchTo('web');
        await app.idle(0);
        assert.deepEqual(app.selectedVerseNumbers(), []);
        assert.deepEqual(app.bookmarkedVerseNumbers(), [9, 10, 11]);
        assert.equal(app.bookmarkItems()[0].preview, storedVerse('web', 'Genesis', 4, 9));
        assert.equal(app.calls.filter((c) => c.cmd === 'search').at(-1).args.translationCode, 'web');

        // Compare: both columns carry the bookmark; copy from the KJV column.
        before = app.calls.length;
        app.key('c');
        await app.idle(before, 'get_compare');
        assert.ok(app.isComparing());
        assert.deepEqual(app.bookmarkedVerseNumbers('kjv'), [9, 10, 11]);
        assert.deepEqual(app.bookmarkedVerseNumbers('web'), [9, 10, 11]);
        app.click(app.verse(10, 'kjv'));
        await app.copyButton();
        assert.equal(app.clipboardWrites.at(-1), `${storedVerse('kjv', 'Genesis', 4, 10)} ${EM} Genesis 4:10 (KJV)`);

        // Read on, then reopen the bookmark: it opens in compare, selected
        // in the selected translation's (WEB) column.
        app.key('Escape');
        app.key('ArrowRight');
        await app.waitForHeading('Genesis 5');
        assert.ok(app.isComparing());
        app.openLibrary();
        before = app.calls.length;
        app.click(app.bookmarkItems()[0].open);
        await app.idle(before, 'get_reading');
        assert.equal(app.heading(), 'Genesis 4');
        assert.deepEqual(app.selectedVerseNumbers('web'), [9, 10, 11]);
        await app.copyButton();
        const web = [9, 10, 11].map((n) => storedVerse('web', 'Genesis', 4, n)).join(' ');
        assert.equal(app.clipboardWrites.at(-1), `${web} ${EM} Genesis 4:9${EN}11 (WEB)`);

        // Remove it from the popup; leave compare; the neighbors return.
        before = app.calls.length;
        app.click(app.bookmarkButton());
        await app.idle(before, 'list_bookmarks');
        assert.deepEqual(app.bookmarkedVerseNumbers(), []);
        before = app.calls.length;
        app.key('c');
        await app.idle(before);
        assert.ok(!app.isComparing());
        assert.equal(app.document.querySelectorAll('.chapter-side').length, 2);

        // Reading mode: search and Escape still behave, one step at a time.
        app.click(app.document.getElementById('reader-mode-btn'));
        const root = app.document.documentElement;
        assert.equal(root.dataset.readerMode, 'on');
        await app.search('jude 3');
        before = app.calls.length;
        app.key('Enter', {}, app.searchInput());
        await app.idle(before, 'get_reading');
        assert.equal(app.heading(), 'Jude 1');
        assert.deepEqual(app.selectedVerseNumbers(), [3]);
        app.key('Escape');
        assert.deepEqual(app.selectedVerseNumbers(), []);
        assert.equal(root.dataset.readerMode, 'on');
        app.key('Escape');
        assert.equal(root.dataset.readerMode, undefined);

        assert.deepEqual(app.consoleErrors, [], 'nothing failed along the way');
    } finally {
        app.close();
    }
});

test('the print stylesheet hides every new control but keeps the reading content', async () => {
    const app = await openApp({});
    try {
        const css = app.document.querySelector('style').textContent;
        const print = css.slice(css.indexOf('@media print'));
        const hidden = print.slice(0, print.indexOf('display: none !important'));
        for (const selector of ['.verse-actions', '.search-toggle', '.search-panel', '.reader-notice', '.xref-popup']) {
            assert.ok(hidden.includes(selector), `print should hide ${selector}`);
        }
        assert.match(print, /\.is-bookmarked \.verse-num::after \{\s*content: none !important;/);
        for (const kept of ['.compare-grid', '.chapter-main {', '.verse {']) {
            assert.ok(!hidden.includes(kept), `print must keep ${kept}`);
        }
    } finally {
        app.close();
    }
});
