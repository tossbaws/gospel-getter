// The page's Rust pieces (ui-leptos) arriving late or not at all: a fresh
// install whose pieces load after boot still gets its welcome, and the
// launch check for updates and About's credits once they come; without
// them each piece says it isn't available (and no update is looked for)
// while the rest of the app keeps working — against the real backend and
// database.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, closeBridge, openApp, settle } from './harness.mjs';
import { readFileSync } from 'node:fs';

after(closeBridge);

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const $ = (app, id) => app.document.getElementById(id);
const toast = (app) => ({ text: app.toast().textContent, error: app.toast().classList.contains('is-error') });

test('a fresh install whose pieces load after boot still opens the welcome', () =>
    withApp({ fresh: true, leptos: 'late' }, async (app) => {
        await app.idle();
        await settle();
        assert.equal($(app, 'welcome-overlay'), null, 'not loaded yet');
        await app.loadPieces();
        await app.until(() => $(app, 'welcome-overlay') && !$(app, 'welcome-overlay').hidden, 'the welcome');
        assert.equal(app.document.activeElement, $(app, 'welcome-panel'));
        assert.equal(app.heading(), 'Genesis 1');
        assert.deepEqual(app.consoleErrors, []);
    }));

test('pieces that load after boot on an install that isn’t fresh open nothing', () =>
    withApp({ leptos: 'late' }, async (app) => {
        await app.idle();
        await app.loadPieces();
        assert.equal($(app, 'welcome-overlay').hidden, true);
        assert.deepEqual(app.consoleErrors, []);
    }));

test('without the pieces, each says it isn’t available, and reading goes on', () =>
    withApp({ book: 'John', chapter: 3, leptos: false }, async (app) => {
        await app.idle();
        for (const id of ['shortcuts-overlay', 'welcome-overlay', 'import-overlay', 'note-overlay', 'export-notes-overlay']) {
            assert.equal($(app, id), null, id);
        }

        app.key('?', { shiftKey: true });
        assert.deepEqual(toast(app), { text: 'Keyboard shortcuts aren’t available.', error: true });
        assert.equal(app.searchPanel().hidden, true, '? still never opens search');

        app.open('menu');
        app.click($(app, 'welcome-btn'));
        assert.deepEqual(toast(app), { text: 'The welcome isn’t available.', error: true });

        const before = app.calls.length;
        app.open('menu');
        app.click($(app, 'import-data-btn'));
        assert.deepEqual(toast(app), { text: 'Import isn’t available.', error: true });
        assert.deepEqual(app.calls.slice(before).map((c) => c.cmd), [], 'no file dialog');

        app.click(app.verse(16));
        app.key('n');
        assert.deepEqual(toast(app), { text: 'The note editor isn’t available.', error: true });

        // The reader's own keys still work.
        const turn = app.calls.length;
        app.key('ArrowRight');
        await app.idle(turn, 'get_reading');
        assert.equal(app.heading(), 'John 4');
        assert.deepEqual(app.consoleErrors, []);
    }));

const VERSION = JSON.parse(
    readFileSync(new URL('../../src-tauri/tauri.conf.json', import.meta.url), 'utf8'),
).version;
const UPDATE = { version: '2.6.0', notes: 'Notes.', date: '2026-10-20', canSelfUpdate: true };
const checks = (app) => app.calls.filter((c) => c.cmd === 'check_for_update').length;

test('pieces that load after boot run the launch check then, and fill About', () =>
    withApp({ leptos: 'late', update: { check: UPDATE } }, async (app) => {
        await app.idle();
        assert.equal(checks(app), 0, 'nothing to check with yet');
        assert.equal($(app, 'update-banner'), null);
        await app.loadPieces();
        await app.idle();
        assert.equal(checks(app), 1, 'the launch check, once');
        assert.equal($(app, 'update-banner').hidden, false);
        assert.equal($(app, 'update-title').textContent, 'Gospel Getter 2.6.0 is available');

        app.open('about');
        assert.equal(app.surface('about').hidden, false);
        assert.equal($(app, 'app-version').textContent, `Version ${VERSION}`);
        const credits = [...$(app, 'about-credits').querySelectorAll('li')].map((li) => li.textContent);
        assert.ok(credits.length > 1, 'the bundled texts, from boot');
        assert.equal(credits.at(-1), 'Cross-references from OpenBible.info, CC BY 4.0');
        assert.deepEqual(app.consoleErrors, []);
    }));

test('without the pieces, About says it isn’t available, no update is looked for, and reading goes on', () =>
    withApp({ book: 'John', chapter: 3, leptos: false, update: { check: UPDATE } }, async (app) => {
        await app.idle();
        assert.equal($(app, 'about-overlay'), null);
        assert.equal($(app, 'update-banner'), null);
        assert.equal(checks(app), 0);

        app.open('menu');
        app.click($(app, 'about-btn'));
        assert.deepEqual(toast(app), { text: 'About isn’t available.', error: true });
        assert.equal(app.surface('menu').hidden, true, 'the menu closes as after any item');

        const turn = app.calls.length;
        app.key('ArrowRight');
        await app.idle(turn, 'get_reading');
        assert.equal(app.heading(), 'John 4');
        assert.deepEqual(app.consoleErrors, []);
    }));

const JOHN = bookId('John');

/** One bookmark (John 3:16) and one highlight (John 3:3), kept for the app. */
async function seedLibrary() {
    await backend('__reset', { bookId: JOHN, chapter: 3, translationCode: 'kjv' });
    await backend('add_bookmark', { bookId: JOHN, chapter: 3, verseStart: 16, verseEnd: 16 });
    await backend('set_highlight', { bookId: JOHN, chapter: 3, verseStart: 3, verseEnd: 3, color: 'green' });
}

const listCalls = (app) => app.calls.filter((c) => ['list_bookmarks', 'list_highlight_passages', 'list_all_notes'].includes(c.cmd));

test('pieces that load after boot fill the Library then, and mark the bookmarks', async () => {
    await seedLibrary();
    await withApp({ book: 'John', chapter: 3, keepBookmarks: true, leptos: 'late' }, async (app) => {
        await app.idle();
        assert.equal($(app, 'bookmark-list'), null, 'no Library yet');
        assert.deepEqual(listCalls(app), [], 'its lists wait for it');
        assert.deepEqual(app.bookmarkedVerseNumbers(), []);
        assert.equal(app.verse(3).dataset.highlight, 'green', 'the reader’s own highlights don’t');

        await app.loadPieces();
        await app.idle();
        assert.deepEqual(listCalls(app).map((c) => c.cmd).sort(), ['list_all_notes', 'list_bookmarks', 'list_highlight_passages']);
        assert.deepEqual(app.bookmarkItems().map((i) => i.reference), ['John 3:16']);
        assert.deepEqual(app.bookmarkedVerseNumbers(), [16]);
        assert.equal($(app, 'highlight-count').textContent, '1');
        assert.deepEqual(app.consoleErrors, []);
    });
});

test('without the pieces, the Library says it isn’t available, and the reader’s highlights still show', async () => {
    await seedLibrary();
    await withApp({ book: 'John', chapter: 3, keepBookmarks: true, leptos: false }, async (app) => {
        await app.idle();
        assert.deepEqual(listCalls(app), []);
        assert.equal(app.verse(3).dataset.highlight, 'green');

        app.open('library');
        assert.equal(app.surface('library').hidden, true, 'nothing opens without the pieces (E1)');
        assert.deepEqual(toast(app), { text: 'The library isn’t available.', error: true });

        const turn = app.calls.length;
        app.key('Escape');
        app.key('ArrowRight');
        await app.idle(turn, 'get_reading');
        assert.equal(app.heading(), 'John 4');
        assert.deepEqual(app.consoleErrors, []);
    });
});

test('without the pieces, search says it isn’t available, and reading goes on', () =>
    withApp({ book: 'John', chapter: 3, leptos: false }, async (app) => {
        await app.idle();
        assert.equal(app.searchInput(), null, 'the box is the pieces’');
        assert.equal(app.window.gospelGetterShortcuts, undefined, 'so is the table of shortcuts');

        app.key('/');
        assert.deepEqual(toast(app), { text: 'Search isn’t available.', error: true });
        assert.equal(app.searchPanel().hidden, true);
        app.click($(app, 'search-toggle'));
        assert.deepEqual(toast(app), { text: 'Search isn’t available.', error: true });
        assert.equal(app.searchPanel().hidden, true);

        const turn = app.calls.length;
        app.key('ArrowRight');
        await app.idle(turn, 'get_reading');
        assert.equal(app.heading(), 'John 4');
        assert.deepEqual(app.consoleErrors, []);
    }));

test('pieces that load after boot bring search, and the table of shortcuts, with them', () =>
    withApp({ book: 'John', chapter: 3, leptos: 'late' }, async (app) => {
        await app.idle();
        app.key('/');
        assert.equal(app.searchPanel().hidden, true, 'not loaded yet');

        await app.loadPieces();
        assert.ok(app.window.gospelGetterShortcuts.some((s) => s.id === 'search'));
        await app.search('rev 22:21');
        assert.equal(app.searchStatus(), 'Press Enter to go there.');
        const before = app.calls.length;
        app.key('Enter', {}, app.searchInput());
        await app.idle(before, 'get_reading');
        assert.equal(app.heading(), 'Revelation 22');
        assert.equal(app.searchPanel().hidden, true);
        assert.deepEqual(app.consoleErrors, []);
    }));

const SAVED_LOOK = {
    'gospel-getter-theme': 'vellum',
    'gospel-getter-text-size': 'large',
    'gospel-getter-layout': 'flowing',
    'gospel-getter-compare': 'on',
    'gospel-getter-compare-with': 'bsb',
};

test('without the pieces, Aa, the translations and the picker say they aren’t available; the Bible text shows in the saved look', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv', leptos: false, storage: SAVED_LOOK }, async (app) => {
        await app.idle();
        const root = app.document.documentElement.dataset;
        assert.deepEqual([root.theme, root.textSize, root.layout, root.compare, root.compareWith], ['vellum', 'large', 'flowing', 'on', 'bsb']);
        assert.deepEqual(app.compareColumns(), ['King James Version (KJV)', 'Berean Standard Bible (BSB)'], 'compared as saved');
        assert.equal(app.window.gospelGetterPreferences, undefined);

        app.click($(app, 'aa-toggle'));
        assert.deepEqual(toast(app), { text: 'Display settings aren’t available.', error: true });
        app.key('Escape', {}, app.document.activeElement);
        app.key('t');
        assert.deepEqual(toast(app), { text: 'Translations aren’t available.', error: true });
        app.key('Escape', {}, app.document.activeElement);
        assert.equal(app.surface('translation').hidden, true);

        // The picker's books are the pieces' too: on a wide window G goes
        // into the sidebar, on a narrow one it opens the popover.
        app.key('g');
        assert.deepEqual(toast(app), { text: 'The book list isn’t available.', error: true });
        assert.equal(app.document.querySelectorAll('#picker .book-item').length, 0);
        app.resize(900);
        app.document.body.focus();
        app.key('g');
        assert.equal(app.surface('picker').hidden, true, 'nothing opens without the pieces (E1)');
        assert.deepEqual(toast(app), { text: 'The book list isn’t available.', error: true });
        app.key('Escape', {}, app.document.activeElement);

        // Nothing was saved, and reading goes on.
        assert.deepEqual(Object.keys(app.storage()).sort(), Object.keys(SAVED_LOOK).sort());
        const turn = app.calls.length;
        app.key('ArrowRight');
        await app.idle(turn, 'get_compare');
        assert.equal(app.heading(), 'John 4');
        assert.deepEqual(app.consoleErrors, []);
    }));

test('pieces that load after boot bring Aa and the translations, with the saved settings', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv', leptos: 'late', storage: { 'gospel-getter-theme': 'monster-hunter', 'gospel-getter-text-size': 'small' } }, async (app) => {
        await app.idle();
        const root = app.document.documentElement.dataset;
        assert.equal(root.theme, 'beast-slayer', 'the renamed theme, from the first frame');
        assert.equal(root.textSize, 'small');
        app.click($(app, 'aa-toggle'));
        assert.deepEqual(toast(app), { text: 'Display settings aren’t available.', error: true });
        app.key('Escape', {}, app.document.activeElement);

        await app.loadPieces();
        assert.equal(app.storage()['gospel-getter-theme'], 'beast-slayer', 'saved under its new name');
        app.click($(app, 'aa-toggle'));
        assert.equal(app.surface('aa').hidden, false);
        const pressed = (id) => [...$(app, id).querySelectorAll('[aria-pressed="true"]')].map((b) => b.dataset.value);
        assert.deepEqual(pressed('theme-choices'), ['beast-slayer']);
        assert.equal($(app, 'text-size-value').textContent, 'Small');
        assert.equal(app.document.activeElement, $(app, 'text-size-up'), 'A− is disabled at Small');
        app.key('Escape', {}, app.document.activeElement);
        app.click($(app, 'translation-toggle'));
        assert.equal(app.document.activeElement, app.document.querySelector('#translation-choices [data-value="kjv"]'));
        assert.deepEqual(app.window.gospelGetterPreferences.current().theme, 'beast-slayer');
        assert.deepEqual(app.consoleErrors, []);
    }));

test('without the pieces, ☰, the sidebar’s button and reading mode’s exit say they aren’t available', () =>
    withApp({ book: 'John', chapter: 3, leptos: false, readerMode: true }, async (app) => {
        await app.idle();
        assert.equal(app.window.gospelGetterSurfaces, undefined);
        const root = app.document.documentElement.dataset;
        assert.equal(root.readerMode, 'on', 'the saved look, from the first frame');

        app.click($(app, 'reader-exit'));
        assert.deepEqual(toast(app), { text: 'Display settings aren’t available.', error: true });
        app.toast().textContent = '';
        app.document.body.focus();
        app.key('Escape');
        assert.deepEqual(toast(app), { text: 'Display settings aren’t available.', error: true });
        assert.equal(root.readerMode, 'on', 'still reading');

        app.click($(app, 'menu-toggle'));
        assert.deepEqual(toast(app), { text: 'The menu isn’t available.', error: true });
        assert.equal(app.surface('menu').hidden, true);
        assert.equal($(app, 'menu-toggle').getAttribute('aria-expanded'), 'false');
        app.click($(app, 'sidebar-toggle'));
        assert.deepEqual(toast(app), { text: 'The book list isn’t available.', error: true });

        // Nothing was saved, and reading goes on.
        assert.deepEqual(Object.keys(app.storage()), ['gospel-getter-reader-mode']);
        const turn = app.calls.length;
        app.key('ArrowRight');
        await app.idle(turn, 'get_reading');
        assert.equal(app.heading(), 'John 4');
        assert.deepEqual(app.consoleErrors, []);
    }));

test('pieces that load after boot open the ☰ menu and the picker from then on', () =>
    withApp({ book: 'John', chapter: 3, leptos: 'late', width: 900 }, async (app) => {
        await app.idle();
        app.click($(app, 'menu-toggle'));
        assert.deepEqual(toast(app), { text: 'The menu isn’t available.', error: true });
        assert.equal(app.surface('picker').hidden, true);

        await app.loadPieces();
        assert.equal(app.window.gospelGetterSurfaces.current(), null);
        app.click($(app, 'menu-toggle'));
        assert.equal(app.surface('menu').hidden, false);
        assert.equal(app.window.gospelGetterSurfaces.current(), 'menu');
        assert.equal(app.document.activeElement, $(app, 'export-data-btn'));
        app.document.body.focus();
        app.key('g');
        assert.equal(app.surface('menu').hidden, true, 'one at a time');
        assert.equal(app.surface('picker').hidden, false);
        assert.equal($(app, 'picker').getAttribute('role'), 'dialog');
        assert.equal(app.document.activeElement, app.document.querySelector('#picker .chapter-btn[aria-current="page"]'));
        assert.deepEqual(app.consoleErrors, []);
    }));
