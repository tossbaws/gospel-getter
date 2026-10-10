// Behavior tests for GH-11: bookmarking the selected verse or range, the
// markers in the reading pane, and the Bookmarks list in the Library — against
// the real backend and database.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { closeBridge, openApp, storedVerse } from './harness.mjs';

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

async function bookmarkSelection(app) {
    const before = app.calls.length;
    app.click(app.bookmarkButton());
    await app.idle(before, 'list_bookmarks');
}

test('the empty Bookmarks list explains how to add one', () =>
    withApp({}, async (app) => {
        assert.deepEqual(app.bookmarkItems(), []);
        assert.equal(
            app.document.querySelector('#bookmark-list .bookmark-empty').textContent,
            'No bookmarks yet. Click a verse, then ★ Bookmark.',
        );
    }));

test('bookmarking a verse marks it, lists it and turns the action into Remove', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        app.click(app.verse(16));
        // John 3:16 has cross-references, so the action sits in its popup.
        assert.equal(app.actions().parentElement, app.verse(16).querySelector('.xref-popup'));
        assert.equal(app.bookmarkButton().textContent, '★ Bookmark');

        await bookmarkSelection(app);
        assert.equal(app.status().textContent, 'Bookmarked');
        assert.equal(app.bookmarkButton().textContent, '★ Remove bookmark');
        assert.deepEqual(app.bookmarkedVerseNumbers(), [16]);

        const [item] = app.bookmarkItems();
        assert.equal(item.reference, 'John 3:16');
        assert.equal(item.preview, storedVerse('kjv', 'John', 3, 16));
        assert.ok(!item.missing && !item.partial);

        // The marker is a pseudo-element: copying is still exactly the verse.
        await app.copyButton();
        assert.equal(app.clipboardWrites[0], `${storedVerse('kjv', 'John', 3, 16)} — John 3:16 (KJV)`);
    }));

test('a range is bookmarked as one passage, newest first in the list', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(1));
        await bookmarkSelection(app);

        app.click(app.verse(16));
        app.click(app.verse(18), { shiftKey: true });
        await bookmarkSelection(app);

        assert.deepEqual(app.bookmarkedVerseNumbers(), [1, 16, 17, 18]);
        const items = app.bookmarkItems();
        assert.deepEqual(items.map((i) => i.reference), [`John 3:16${EN}18`, 'John 3:1']);
        assert.equal(items[0].preview, storedVerse('web', 'John', 3, 16), 'preview is the first verse, unshortened');

        // Selecting part of a bookmarked range is a different passage.
        app.click(app.verse(17));
        assert.equal(app.bookmarkButton().textContent, '★ Bookmark');
        app.click(app.verse(16));
        app.click(app.verse(18), { shiftKey: true });
        assert.equal(app.bookmarkButton().textContent, '★ Remove bookmark');
    }));

test('Remove bookmark in the verse actions removes exactly that passage', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        app.click(app.verse(16));
        app.click(app.verse(17), { shiftKey: true });
        await bookmarkSelection(app);
        app.click(app.verse(16));
        await bookmarkSelection(app);
        assert.deepEqual(app.bookmarkedVerseNumbers(), [16, 17]);

        // Remove the range; the single-verse bookmark stays. (Clicking the
        // selected verse again would just deselect it, so start afresh.)
        app.key('Escape');
        app.click(app.verse(16));
        app.click(app.verse(17), { shiftKey: true });
        await bookmarkSelection(app);
        assert.equal(app.status().textContent, 'Bookmark removed');
        assert.equal(app.bookmarkButton().textContent, '★ Bookmark');
        assert.deepEqual(app.bookmarkedVerseNumbers(), [16]);
        assert.deepEqual(app.bookmarkItems().map((i) => i.reference), ['John 3:16']);
    }));

test('bookmarks can be removed from the list, keeping focus in it', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        for (const n of [1, 2, 3]) {
            app.click(app.verse(n));
            await bookmarkSelection(app);
        }
        app.openLibrary();
        const before = app.calls.length;
        const [, middle] = app.bookmarkItems();
        assert.equal(middle.remove.getAttribute('aria-label'), 'Remove bookmark John 3:2');
        app.click(middle.remove);
        await app.idle(before, 'list_bookmarks');

        assert.deepEqual(app.bookmarkItems().map((i) => i.reference), ['John 3:3', 'John 3:1']);
        assert.deepEqual(app.bookmarkedVerseNumbers(), [1, 3]);
        assert.equal(app.surface('library').hidden, false, 'removing keeps the Library open');
        assert.ok(app.document.activeElement.classList.contains('bookmark-remove'));
    }));

test('opening a bookmark goes to its chapter and selects the passage', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        app.click(app.verse(16));
        app.click(app.verse(18), { shiftKey: true });
        await bookmarkSelection(app);

        app.key('ArrowLeft');
        await app.waitForHeading('John 2');
        app.openLibrary();
        const before = app.calls.length;
        app.click(app.bookmarkItems()[0].open);
        await app.idle(before, 'get_reading');

        assert.equal(app.heading(), 'John 3');
        assert.equal(app.surface('library').hidden, true, 'the Library gets out of the way');
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17, 18]);
        assert.equal(app.notice(), '');
        await app.copyButton();
        const text = [16, 17, 18].map((n) => storedVerse('kjv', 'John', 3, n)).join(' ');
        assert.equal(app.clipboardWrites[0], `${text} — John 3:16${EN}18 (KJV)`);
    }));

test('bookmarks survive a restart and a translation switch', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        app.click(app.verse(16));
        await bookmarkSelection(app);
    }).then(() =>
        withApp({ book: 'Genesis', chapter: 1, translation: 'kjv', keepBookmarks: true }, async (app) => {
            assert.deepEqual(app.bookmarkItems().map((i) => i.preview), [storedVerse('kjv', 'John', 3, 16)]);

            const select = app.document.getElementById('translation-select');
            const before = app.calls.length;
            select.value = 'web';
            select.dispatchEvent(new app.window.Event('change', { bubbles: true }));
            await app.idle(before, 'list_bookmarks');
            const [item] = app.bookmarkItems();
            assert.equal(item.reference, 'John 3:16');
            assert.equal(item.preview, storedVerse('web', 'John', 3, 16), 'preview follows the translation');
        })));

test('a bookmark whose verse the translation does not number opens its chapter with a notice', () =>
    withApp({ book: 'Matthew', chapter: 2, translation: 'web' }, async (app) => {
        // Matthew 2 has 23 verses in the WEB, 22 in the KJV.
        app.click(app.verse(23));
        await bookmarkSelection(app);

        const select = app.document.getElementById('translation-select');
        let before = app.calls.length;
        select.value = 'kjv';
        select.dispatchEvent(new app.window.Event('change', { bubbles: true }));
        await app.idle(before, 'list_bookmarks');
        const [item] = app.bookmarkItems();
        assert.equal(item.reference, 'Matthew 2:23');
        assert.ok(item.missing);
        assert.equal(item.preview, 'Not numbered in the KJV');
        assert.deepEqual(app.bookmarkedVerseNumbers(), [], 'no KJV verse is marked in its place');

        app.key('ArrowRight');
        await app.waitForHeading('Matthew 3');
        app.openLibrary();
        before = app.calls.length;
        app.click(item.open);
        await app.idle(before, 'get_reading');
        assert.equal(app.heading(), 'Matthew 2');
        assert.deepEqual(app.selectedVerseNumbers(), [], 'never selects a different verse');
        assert.equal(
            app.notice(),
            "Matthew 2:23 isn't numbered in the King James Version — Matthew 2 ends at verse 22 there. Showing the whole chapter.",
        );

        // Moving on clears the notice.
        app.key('ArrowRight');
        await app.waitForHeading('Matthew 3');
        assert.equal(app.notice(), '');
    }));

test('a bookmark only partly numbered in the translation says so and selects nothing', () =>
    withApp({ book: 'Romans', chapter: 16, translation: 'kjv' }, async (app) => {
        // Romans 16 has 27 verses in the KJV and 25 in the WEB.
        app.click(app.verse(24));
        app.click(app.verse(27), { shiftKey: true });
        await bookmarkSelection(app);

        const select = app.document.getElementById('translation-select');
        const before = app.calls.length;
        select.value = 'web';
        select.dispatchEvent(new app.window.Event('change', { bubbles: true }));
        await app.idle(before, 'list_bookmarks');
        const [item] = app.bookmarkItems();
        assert.equal(item.preview, storedVerse('web', 'Romans', 16, 24));
        assert.ok(item.partial);
        assert.deepEqual(app.bookmarkedVerseNumbers(), [24, 25], 'only the verses the WEB numbers are marked');

        app.openLibrary();
        app.click(item.open);
        await app.idle(0, 'get_reading');
        assert.deepEqual(app.selectedVerseNumbers(), []);
        assert.match(app.notice(), /^Romans 16:24–27 isn't numbered in the World English Bible/);
    }));

test('a failed bookmark change is reported, not swallowed', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        app.failCommand('add_bookmark', 'The bookmark couldn’t be saved.');
        app.click(app.verse(16));
        app.click(app.bookmarkButton());
        await app.until(() => app.status().textContent !== '', 'status');
        assert.equal(app.status().textContent, "Couldn't save bookmark: The bookmark couldn\u2019t be saved.");
        assert.ok(app.status().classList.contains('is-error'));
        assert.deepEqual(app.bookmarkItems(), []);
    }));

test('a bookmark list that fails to load says so', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        app.failCommand('list_bookmarks', "Bookmarks couldn't be loaded.");
        const select = app.document.getElementById('translation-select');
        const before = app.calls.length;
        select.value = 'web';
        select.dispatchEvent(new app.window.Event('change', { bubbles: true }));
        await app.idle(before, 'list_bookmarks');
        const error = app.document.querySelector('#bookmark-list .bookmark-empty.is-error');
        assert.ok(error);
        assert.match(error.textContent, /Bookmarks couldn't be loaded/);
    }));

test('markers show in reading mode and use the theme accent', () =>
    withApp({ book: 'John', chapter: 3, readerMode: true }, async (app) => {
        app.click(app.verse(16));
        await bookmarkSelection(app);
        assert.equal(app.document.documentElement.dataset.readerMode, 'on');
        assert.deepEqual(app.bookmarkedVerseNumbers(), [16]);

        const css = app.document.querySelector('style').textContent;
        const rule = css.match(/\.chapter-main \.verse\.is-bookmarked \.verse-num::after \{([^}]*)\}/);
        assert.ok(rule, 'marker rule exists');
        assert.match(rule[1], /color: var\(--accent\)/, 'every theme defines --accent');
        const readerModeRules = css.slice(css.indexOf('@media screen'), css.indexOf('header {'));
        assert.ok(!readerModeRules.includes('is-bookmarked'), 'reading mode does not hide markers');
    }));
