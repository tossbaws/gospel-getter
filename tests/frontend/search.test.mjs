// Behavior tests for GH-12: the search box — going to references and
// searching the text — against the real backend, parser and FTS5 index.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { closeBridge, openApp, storedVerse } from './harness.mjs';

after(closeBridge);

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

async function pressEnter(app) {
    const before = app.calls.length;
    app.key('Enter', {}, app.searchInput());
    await app.idle(before);
}

async function switchTranslation(app, code) {
    const select = app.document.getElementById('translation-select');
    const before = app.calls.length;
    select.value = code;
    select.dispatchEvent(new app.window.Event('change', { bubbles: true }));
    await app.idle(before, 'get_reading');
}

test('/ and Ctrl+K open search, Escape closes it', () =>
    withApp({}, async (app) => {
        const panel = app.searchPanel();
        const toggle = app.document.getElementById('search-toggle');
        assert.equal(panel.hidden, true);

        const slash = app.key('/');
        assert.ok(slash.defaultPrevented, '/ must not be typed into the box');
        assert.equal(panel.hidden, false);
        assert.equal(app.document.activeElement, app.searchInput());
        assert.equal(toggle.getAttribute('aria-expanded'), 'true');

        app.key('Escape', {}, app.searchInput());
        assert.equal(panel.hidden, true);
        assert.equal(toggle.getAttribute('aria-expanded'), 'false');

        app.key('k', { ctrlKey: true });
        assert.equal(panel.hidden, false);
        app.key('Escape', {}, app.searchInput());
        assert.equal(panel.hidden, true);

        app.click(toggle);
        assert.equal(panel.hidden, false);
        app.click(app.document.querySelector('footer'));
        assert.equal(panel.hidden, true, 'clicking away closes it');
    }));

test('search works in reading mode, and Escape closes it before leaving reading mode', () =>
    withApp({ readerMode: true }, async (app) => {
        const root = app.document.documentElement;
        app.key('/');
        assert.equal(app.searchPanel().hidden, false);
        app.key('Escape', {}, app.searchInput());
        assert.equal(app.searchPanel().hidden, true);
        assert.equal(root.dataset.readerMode, 'on');

        await app.search('ps 23:1');
        await pressEnter(app);
        assert.equal(app.heading(), 'Psalms 23');
        assert.deepEqual(app.selectedVerseNumbers(), [1]);
        assert.equal(root.dataset.readerMode, 'on');

        // The toggle isn't among the things reading mode hides.
        const css = app.document.querySelector('style').textContent;
        const readerModeHides = css.slice(css.indexOf('@media screen'), css.indexOf('header {'));
        assert.ok(!readerModeHides.includes('search'));
    }));

test('a reference goes to its chapter and selects the verses', () =>
    withApp({ book: 'Genesis', chapter: 1, translation: 'web' }, async (app) => {
        await app.search('jn 3:16-18');
        assert.equal(app.searchStatus(), 'Press Enter to go there.');
        assert.equal(app.searchResults()[0].textContent.trim(), 'Go to John 3:16–18');
        await pressEnter(app);

        assert.equal(app.heading(), 'John 3');
        assert.equal(app.searchPanel().hidden, true);
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17, 18]);
        await app.copyButton();
        const text = [16, 17, 18].map((n) => storedVerse('web', 'John', 3, n)).join(' ');
        assert.equal(app.clipboardWrites[0], `${text} — John 3:16–18 (WEB)`);
    }));

test('a chapter reference goes there without selecting anything', () =>
    withApp({}, async (app) => {
        for (const [query, heading] of [
            ['1 Cor 13', '1 Corinthians 13'],
            ['Psalm 23', 'Psalms 23'],
            ['I John 4', '1 John 4'],
            ['gen. 50', 'Genesis 50'],
        ]) {
            await app.search(query);
            await pressEnter(app);
            assert.equal(app.heading(), heading, query);
            assert.deepEqual(app.selectedVerseNumbers(), [], query);
        }
    }));

test('clicking the Go to result works like Enter', () =>
    withApp({}, async (app) => {
        await app.search('rev 22:21');
        const before = app.calls.length;
        app.click(app.searchResults()[0]);
        await app.idle(before, 'get_reading');
        assert.equal(app.heading(), 'Revelation 22');
        assert.deepEqual(app.selectedVerseNumbers(), [21]);
    }));

test('invalid references explain themselves and go nowhere', () =>
    withApp({ book: 'Genesis', chapter: 1, translation: 'kjv' }, async (app) => {
        for (const [query, message] of [
            ['John 22', 'No such chapter or verse: John 22. John has 21 chapters.'],
            ['Jude 2:1', 'No such chapter or verse: Jude 2. Jude has 1 chapter.'],
            ['John 3:99', 'No such chapter or verse: John 3:99. John 3 has 36 verses in the King James Version.'],
            ['Jhon 3:16', 'There’s no book called “Jhon”.'.replace('’', "'")],
        ]) {
            await app.search(query);
            assert.equal(app.searchStatus(), message, query);
            assert.ok(app.document.getElementById('search-status').classList.contains('is-error'));
            assert.equal(app.searchResults().length, 0);
            await pressEnter(app);
            assert.equal(app.heading(), 'Genesis 1', `${query} must not navigate`);
        }
    }));

test('a verse only one translation numbers is checked against the selected one', () =>
    withApp({ book: 'Genesis', chapter: 1, translation: 'kjv' }, async (app) => {
        await app.search('Matthew 2:23');
        assert.equal(app.searchStatus(), 'No such chapter or verse: Matthew 2:23. Matthew 2 has 22 verses in the King James Version.');

        // Switching translation reruns the search: the WEB numbers it.
        await switchTranslation(app, 'web');
        await app.idle(0, 'search');
        assert.equal(app.searchStatus(), 'Press Enter to go there.');
        await pressEnter(app);
        assert.equal(app.heading(), 'Matthew 2');
        assert.deepEqual(app.selectedVerseNumbers(), [23]);
    }));

test('word search lists matching verses in Bible order with the words marked', () =>
    withApp({ translation: 'kjv' }, async (app) => {
        await app.search('faith hope charity');
        const results = app.searchResults();
        assert.ok(results.length > 0);
        assert.match(app.searchStatus(), /^\d+ verses? in the KJV\.$/);

        const refs = results.map((r) => r.querySelector('.search-ref').textContent);
        assert.ok(refs.includes('1 Corinthians 13:13'));
        const keys = results.map((r) => [Number(r.dataset.book), Number(r.dataset.chapter), Number(r.dataset.verseStart)]);
        const sorted = [...keys].sort((a, b) => a[0] - b[0] || a[1] - b[1] || a[2] - b[2]);
        assert.deepEqual(keys, sorted);

        for (const r of results) {
            const [book, chapter, verse] = [r.dataset.book, r.dataset.chapter, r.dataset.verseStart].map(Number);
            const text = r.querySelector('.search-text');
            // The shown text is the stored verse, character for character;
            // the emphasis is markup around whole words, nothing more.
            const bookName = refs[results.indexOf(r)].replace(/ \d+:\d+$/, '');
            assert.equal(text.textContent, storedVerse('kjv', bookName, chapter, verse));
            assert.equal(Number(r.dataset.book), book);
            for (const mark of text.querySelectorAll('mark')) {
                assert.match(mark.textContent.toLowerCase(), /^(faith|hope|charity)$/);
                assert.equal(mark.children.length, 0);
            }
            assert.ok(text.querySelectorAll('mark').length >= 3);
        }
    }));

test('typed markup is searched as words, never rendered', () =>
    withApp({ translation: 'web' }, async (app) => {
        await app.search('<img src=x onerror=alert(1)> shepherd');
        assert.equal(app.searchPanel().querySelector('img'), null);
        assert.equal(app.document.querySelector('#reading-pane img'), null);
        assert.match(app.searchStatus(), /^No verses in the WEB contain all of: img, src, x, onerror, alert, 1, shepherd\.$/);
    }));

test('clicking a result opens and selects that verse, and Copy copies it exactly', () =>
    withApp({ book: 'Genesis', chapter: 1, translation: 'web' }, async (app) => {
        await app.search('brother keeper');
        const result = app.searchResults().find((r) => r.querySelector('.search-ref').textContent === 'Genesis 4:9');
        assert.ok(result, 'Genesis 4:9 is a result');
        const before = app.calls.length;
        app.click(result);
        await app.idle(before, 'get_reading');
        assert.equal(app.heading(), 'Genesis 4');
        assert.equal(app.searchPanel().hidden, true);
        assert.deepEqual(app.selectedVerseNumbers(), [9]);
        await app.copyButton();
        assert.equal(app.clipboardWrites[0], `${storedVerse('web', 'Genesis', 4, 9)} — Genesis 4:9 (WEB)`);
    }));

test('common words are paged, not all rendered at once', () =>
    withApp({ translation: 'web' }, async (app) => {
        await app.search('love');
        assert.equal(app.searchResults().length, 50);
        assert.match(app.searchStatus(), /^\d+ verses in the WEB, showing 50\.$/);
        const more = app.document.getElementById('search-more');
        assert.equal(more.hidden, false);

        const firstPage = app.searchResults().map((r) => r.querySelector('.search-ref').textContent);
        const before = app.calls.length;
        app.click(more);
        await app.idle(before, 'search');
        const both = app.searchResults().map((r) => r.querySelector('.search-ref').textContent);
        assert.equal(both.length, 100);
        assert.deepEqual(both.slice(0, 50), firstPage);
        assert.equal(new Set(both).size, 100, 'no result repeats');
        assert.match(app.searchStatus(), /showing 100\.$/);
    }));

test('switching translation reruns the search in the new translation', () =>
    withApp({ translation: 'kjv' }, async (app) => {
        await app.search('yahweh');
        assert.equal(app.searchResults().length, 0);
        assert.equal(app.searchStatus(), 'No verses in the KJV contain all of: yahweh.');

        await switchTranslation(app, 'web');
        await app.idle(0, 'search');
        assert.equal(app.searchResults().length, 50);
        assert.match(app.searchStatus(), / in the WEB, showing 50\.$/);
        const last = app.calls.filter((c) => c.cmd === 'search').at(-1);
        assert.equal(last.args.translationCode, 'web');
    }));

test('arrow keys move through results without turning the chapter', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await app.search('shepherd');
        const [first, second] = app.searchResults();
        app.key('ArrowDown', {}, app.searchInput());
        assert.equal(app.document.activeElement, first);
        app.key('ArrowDown', {}, first);
        assert.equal(app.document.activeElement, second);
        app.key('ArrowUp', {}, second);
        assert.equal(app.document.activeElement, first);
        app.key('ArrowRight', {}, first);
        app.key('ArrowLeft', {}, first);
        await app.idle(0);
        assert.equal(app.heading(), 'John 3');
        app.key('Escape', {}, first);
        assert.equal(app.searchPanel().hidden, true);
    }));

test('a search that fails says so', () =>
    withApp({}, async (app) => {
        app.failCommand('search', 'Search failed.');
        await app.search('love');
        assert.equal(app.searchStatus(), 'Search failed: Search failed.');
        assert.ok(app.document.getElementById('search-status').classList.contains('is-error'));
    }));
