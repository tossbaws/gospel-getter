// Behavior tests for GH-13: comparing translations side by side, aligned
// by verse number — against the real backend — and its interplay with
// selection (GH-10), bookmarks (GH-11) and search (GH-12).

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { closeBridge, openApp, storedVerse, verseCount } from './harness.mjs';

after(closeBridge);

const EN = '–';

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

async function toggleCompare(app, how = 'button') {
    const before = app.calls.length;
    if (how === 'key') {
        app.key('c');
    } else {
        app.click(app.document.getElementById('compare-btn'));
    }
    await app.idle(before);
}

/** The verse text of a rendered verse element, without its number or popup. */
function shownText(el) {
    const clone = el.cloneNode(true);
    clone.querySelectorAll('.verse-num, .xref-popup, .verse-actions').forEach((n) => n.remove());
    return clone.textContent.trim();
}

test('the Compare toggle swaps the faded neighbors for labeled side-by-side columns and back', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        const btn = app.document.getElementById('compare-btn');
        assert.equal(btn.getAttribute('aria-pressed'), 'false');
        assert.equal(app.document.querySelectorAll('.chapter-side').length, 2);

        await toggleCompare(app);
        assert.equal(btn.getAttribute('aria-pressed'), 'true');
        assert.ok(app.isComparing());
        assert.deepEqual(app.compareColumns(), ['King James Version (KJV)', 'World English Bible (WEB)']);
        assert.equal(app.document.querySelectorAll('.chapter-side').length, 0, 'columns replace the neighbors');
        assert.equal(app.heading(), 'John 3');
        assert.equal(app.window.localStorage.getItem('gospel-getter-compare'), 'on');

        await toggleCompare(app);
        assert.ok(!app.isComparing());
        assert.equal(app.document.querySelectorAll('.chapter-side').length, 2, 'normal layout restored');
        assert.equal(app.sideVerse('prev', 1).textContent.includes(storedVerse('kjv', 'John', 2, 1)), true);
        assert.equal(app.window.localStorage.getItem('gospel-getter-compare'), 'off');
    }));

test('C toggles compare, and the mode is remembered across restarts', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await toggleCompare(app, 'key');
        assert.ok(app.isComparing());
        // Typing C into a form control isn't the shortcut.
        app.key('c', {}, app.document.getElementById('translation-select'));
        await app.idle(0);
        assert.ok(app.isComparing());
    }).then(() =>
        withApp({ book: 'John', chapter: 3, compare: true }, async (app) => {
            assert.ok(app.isComparing(), 'starts in compare mode');
            assert.equal(app.document.getElementById('compare-btn').getAttribute('aria-pressed'), 'true');
            await toggleCompare(app, 'key');
            assert.ok(!app.isComparing());
        })));

for (const [book, chapter] of [['Romans', 14], ['Romans', 16], ['3 John', 1], ['Matthew', 2]]) {
    test(`${book} ${chapter}: rows align by verse number and gaps are shown, not filled`, () =>
        withApp({ book, chapter, compare: true }, async (app) => {
            const counts = { kjv: verseCount('kjv', book, chapter), web: verseCount('web', book, chapter) };
            assert.notEqual(counts.kjv, counts.web, 'fixture: the translations number this chapter differently');
            const rows = Math.max(counts.kjv, counts.web);

            const cells = [...app.document.querySelector('.compare-grid').children].filter((el) => !el.classList.contains('compare-head'));
            assert.equal(cells.length, rows * 2, 'one cell per translation per verse number');
            for (let n = 1; n <= rows; n++) {
                const [kjv, web] = [cells[(n - 1) * 2], cells[(n - 1) * 2 + 1]];
                for (const [cell, code] of [[kjv, 'kjv'], [web, 'web']]) {
                    if (n <= counts[code]) {
                        assert.equal(cell.dataset.translation, code);
                        assert.equal(Number(cell.dataset.verse), n, 'same row, same number');
                        assert.equal(cell.querySelector('.verse-num').textContent, String(n));
                        assert.equal(shownText(cell), storedVerse(code, book, chapter, n).trim());
                    } else {
                        assert.ok(cell.classList.contains('compare-gap'), `${code} ${n} is a gap`);
                        assert.equal(Number(cell.dataset.gapVerse), n);
                        assert.equal(cell.querySelector('.verse-num').textContent, String(n));
                        assert.equal(cell.querySelector('.compare-gap-note').textContent, `Not numbered in the ${code.toUpperCase()}`);
                        assert.equal(cell.dataset.translation, undefined, 'a gap is not a selectable verse');
                    }
                }
            }
        }));
}

test('selecting in either column copies that column’s text and translation code', () =>
    withApp({ book: 'Romans', chapter: 14, translation: 'kjv', compare: true }, async (app) => {
        app.click(app.verse(23, 'web'));
        app.click(app.verse(26, 'web'), { shiftKey: true });
        assert.deepEqual(app.selectedVerseNumbers('web'), [23, 24, 25, 26]);
        assert.deepEqual(app.selectedVerseNumbers('kjv'), []);
        await app.copyButton();
        const web = [23, 24, 25, 26].map((n) => storedVerse('web', 'Romans', 14, n)).join(' ');
        assert.equal(app.clipboardWrites.at(-1), `${web} — Romans 14:23${EN}26 (WEB)`);

        // Shift-click in the other column doesn't drag the range across.
        app.click(app.verse(20, 'kjv'), { shiftKey: true });
        assert.deepEqual(app.selectedVerseNumbers('web'), [23, 24, 25, 26]);
        assert.deepEqual(app.selectedVerseNumbers('kjv'), []);

        app.click(app.verse(23, 'kjv'));
        assert.deepEqual(app.selectedVerseNumbers('kjv'), [23]);
        assert.deepEqual(app.selectedVerseNumbers('web'), []);
        app.key('c', { ctrlKey: true });
        await app.idle(0);
        assert.equal(app.clipboardWrites.at(-1), `${storedVerse('kjv', 'Romans', 14, 23)} — Romans 14:23 (KJV)`);

        // Clicking a gap is a click away.
        app.click(app.document.querySelector('.compare-gap'));
        assert.deepEqual(app.selectedVerseNumbers(), []);
    }));

test('cross-references open in either column and expand in that column’s translation', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv', compare: true }, async (app) => {
        const web = app.verse(16, 'web');
        app.click(web);
        const popup = web.querySelector('.xref-popup');
        assert.equal(popup.hidden, false);
        assert.equal(app.verse(16, 'kjv').querySelector('.xref-popup').hidden, true);
        const citation = popup.querySelector('.xref-citation');
        const before = app.calls.length;
        app.click(citation);
        await app.idle(before, 'get_xref_text');
        const call = app.calls.filter((c) => c.cmd === 'get_xref_text').at(-1);
        assert.equal(call.args.translationCode, 'web');
        assert.equal(citation.parentElement.querySelector('.xref-expanded').hidden, false);
    }));

test('bookmarks stay translation-independent while comparing', () =>
    withApp({ book: 'Romans', chapter: 14, translation: 'kjv', compare: true }, async (app) => {
        // Romans 14:26 exists only in the WEB.
        app.click(app.verse(26, 'web'));
        let before = app.calls.length;
        app.click(app.bookmarkButton());
        await app.idle(before, 'list_bookmarks');
        assert.equal(app.status().textContent, 'Bookmarked');
        assert.deepEqual(app.bookmarkedVerseNumbers('web'), [26]);
        assert.deepEqual(app.bookmarkedVerseNumbers('kjv'), []);
        const [item] = app.bookmarkItems();
        assert.equal(item.reference, 'Romans 14:26');
        assert.ok(item.missing, 'previewed in the selected translation (KJV), which lacks it');

        // A verse both number is marked in both columns.
        app.click(app.verse(8, 'kjv'));
        before = app.calls.length;
        app.click(app.bookmarkButton());
        await app.idle(before, 'list_bookmarks');
        assert.deepEqual(app.bookmarkedVerseNumbers('kjv'), [8]);
        assert.deepEqual(app.bookmarkedVerseNumbers('web'), [8, 26]);

        // The same passage selected in the other column is the same bookmark.
        app.click(app.verse(8, 'web'));
        assert.equal(app.bookmarkButton().textContent, '★ Remove bookmark');

        // Opening a bookmark selects it in the selected translation's column.
        app.key('ArrowRight');
        await app.waitForHeading('Romans 15');
        app.openLibrary();
        before = app.calls.length;
        app.click(app.bookmarkItems().find((i) => i.reference === 'Romans 14:8').open);
        await app.idle(before, 'get_reading');
        assert.ok(app.isComparing());
        assert.deepEqual(app.selectedVerseNumbers('kjv'), [8]);
        assert.deepEqual(app.selectedVerseNumbers('web'), []);
    }));

test('arrow keys turn chapters and cross books while comparing', () =>
    withApp({ book: 'Romans', chapter: 16, compare: true }, async (app) => {
        app.key('ArrowRight');
        await app.waitForHeading('1 Corinthians 1');
        assert.ok(app.isComparing());
        assert.equal(app.verse(1, 'web').querySelector('.verse-num').textContent, '1');
        app.key('ArrowLeft');
        await app.waitForHeading('Romans 16');
        assert.ok(app.isComparing());
        // The comparison is for the chapter on screen, not a stale one.
        assert.equal(app.document.querySelectorAll('.chapter-main .verse[data-translation="kjv"]').length, verseCount('kjv', 'Romans', 16));
    }));

test('search results open into the compare grid, selected in the searched translation', () =>
    withApp({ book: 'Genesis', chapter: 1, translation: 'web', compare: true }, async (app) => {
        await app.search('Rom 14:25');
        const before = app.calls.length;
        app.key('Enter', {}, app.searchInput());
        await app.idle(before, 'get_reading');
        assert.equal(app.heading(), 'Romans 14');
        assert.ok(app.isComparing());
        assert.deepEqual(app.selectedVerseNumbers('web'), [25]);
        assert.deepEqual(app.selectedVerseNumbers('kjv'), []);
    }));

test('switching translation keeps compare and re-renders it', () =>
    withApp({ book: 'Romans', chapter: 14, translation: 'kjv', compare: true }, async (app) => {
        app.click(app.verse(3, 'kjv'));
        const select = app.document.getElementById('translation-select');
        const before = app.calls.length;
        select.value = 'web';
        select.dispatchEvent(new app.window.Event('change', { bubbles: true }));
        await app.idle(before, 'get_reading');
        assert.ok(app.isComparing());
        assert.deepEqual(app.selectedVerseNumbers(), [], 'selection cleared on switch');
        assert.deepEqual(app.compareColumns(), ['King James Version (KJV)', 'World English Bible (WEB)']);
    }));

test('compare works in reading mode', () =>
    withApp({ book: 'Romans', chapter: 14, readerMode: true, compare: true }, async (app) => {
        assert.equal(app.document.documentElement.dataset.readerMode, 'on');
        assert.ok(app.isComparing());
        app.click(app.verse(24, 'web'));
        assert.deepEqual(app.selectedVerseNumbers('web'), [24]);
        app.key('Escape');
        assert.deepEqual(app.selectedVerseNumbers(), []);
        assert.equal(app.document.documentElement.dataset.readerMode, 'on');
    }));

test('if the comparison cannot load, the chapter still shows, with a notice', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        app.failCommand('get_compare', 'The comparison couldn’t be loaded.');
        await toggleCompare(app);
        assert.ok(!app.isComparing());
        assert.equal(app.document.querySelectorAll('.chapter-main .verse[data-translation]').length, 36);
        assert.match(app.notice(), /couldn't be compared/);
    }));

test('compare styles use only theme variables and stack on narrow windows', () =>
    withApp({}, async (app) => {
        const css = app.document.querySelector('style').textContent;
        const compareRules = [...css.matchAll(/([^{}]*\.compare-[^{]*)\{([^}]*)\}/g)];
        assert.ok(compareRules.length >= 4);
        for (const [, selector, body] of compareRules) {
            assert.ok(!/#[0-9a-f]{3,8}\b|rgb\(/i.test(body), `${selector.trim()} must use theme variables, not fixed colors`);
        }
        const narrow = css.slice(css.indexOf('@media (max-width: 640px) {\n            .compare-grid'));
        assert.match(narrow, /\.compare-grid \{\s*grid-template-columns: minmax\(0, 1fr\);/);
        assert.match(narrow, /\.compare-grid > \[data-code\]::before \{\s*content: attr\(data-code\);/);
        // Text size and spacing reach the gap cells too, so rows stay even.
        assert.match(css, /\.compare-gap \{[^}]*var\(--reading-scale\)[^}]*var\(--reading-leading\)/);
    }));
