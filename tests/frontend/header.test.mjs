// Behavior tests for the top bar and what it opens: Aa (text and display),
// the Library (bookmarks and highlights), the ☰ menu and About. One opens
// at a time; its button, Escape, or (for Aa and the menu) a click outside
// closes it; focus moves in on opening and back to the button on closing.
// Aa's controls are views of the same preferences and translation the
// rest of the app uses. Against the real backend and database.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, closeBridge, openApp, setUpdate, storedVerse } from './harness.mjs';

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
const TOGGLES = { aa: 'aa-toggle', library: 'library-toggle', menu: 'menu-toggle' };
const toggle = (app, name) => $(app, TOGGLES[name]);
const isOpen = (app, name) => !app.surface(name).hidden;
const focusIn = (app, el) => el.contains(app.document.activeElement);
const shownOnScreen = (app, el) => app.window.getComputedStyle(el).display !== 'none';

async function clickAndWait(app, el, cmd) {
    const before = app.calls.length;
    app.click(el);
    await app.idle(before, cmd);
}

const segments = (app, id) => [...$(app, id).querySelectorAll('button')];
const pressed = (app, id) => segments(app, id).filter((b) => b.getAttribute('aria-pressed') === 'true').map((b) => b.dataset.value);

// ---- Opening and closing

test('each surface opens from its button, closes from it or with Escape, and focus returns to the button', () =>
    withApp({}, async (app) => {
        for (const name of ['aa', 'library', 'menu']) {
            const btn = toggle(app, name);
            assert.equal(btn.getAttribute('aria-expanded'), 'false');
            app.click(btn);
            assert.ok(isOpen(app, name), `${name} opens`);
            assert.equal(btn.getAttribute('aria-expanded'), 'true');
            assert.ok(focusIn(app, app.surface(name)), `focus moves into ${name}`);
            app.click(btn);
            assert.ok(!isOpen(app, name), `${name} closes from its button`);
            assert.equal(btn.getAttribute('aria-expanded'), 'false');

            app.click(btn);
            app.key('Escape', {}, app.document.activeElement);
            assert.ok(!isOpen(app, name), `${name} closes with Escape`);
            assert.equal(app.document.activeElement, btn, `focus returns to ${name}'s button`);
        }
        // Where focus lands first.
        app.click(toggle(app, 'aa'));
        assert.equal(app.document.activeElement.dataset.value, 'kjv', 'Aa: the chosen translation');
        app.click(toggle(app, 'library'));
        assert.equal(app.document.activeElement, $(app, 'library-tab-bookmarks'), 'Library: the chosen tab');
        app.click(toggle(app, 'menu'));
        assert.equal(app.document.activeElement, $(app, 'export-data-btn'), 'menu: its first item');
        assert.deepEqual(app.consoleErrors, []);
    }));

test('a click outside closes Aa and the menu, but the Library stays while the reader reads', () =>
    withApp({}, async (app) => {
        for (const name of ['aa', 'menu']) {
            app.open(name);
            // A click inside doesn't close it.
            app.click(app.surface(name).querySelector('button'));
            if (name === 'aa') assert.ok(isOpen(app, name), 'a click inside Aa keeps it open');
            app.open(name);
            app.click(app.document.querySelector('footer'));
            assert.ok(!isOpen(app, name), `${name} closes`);
            assert.equal(app.document.activeElement, toggle(app, name), 'focus returns to the button');
        }
        app.open('library');
        app.click(app.document.querySelector('footer'));
        app.click(app.verse(16));
        assert.ok(isOpen(app, 'library'));
        // Clicking in the Library isn't a click away from the verse selection.
        app.click($(app, 'library-tab-highlights'));
        assert.deepEqual(app.selectedVerseNumbers(), [16]);
        app.click($(app, 'library-close'));
        assert.ok(!isOpen(app, 'library'));
        assert.equal(app.document.activeElement, toggle(app, 'library'));
    }));

test('only one opens at a time, search included', () =>
    withApp({}, async (app) => {
        app.open('aa');
        app.click(toggle(app, 'library'));
        assert.ok(!isOpen(app, 'aa') && isOpen(app, 'library'));
        assert.equal(toggle(app, 'aa').getAttribute('aria-expanded'), 'false');
        app.click(toggle(app, 'menu'));
        assert.ok(!isOpen(app, 'library') && isOpen(app, 'menu'));
        app.click($(app, 'about-btn'));
        assert.ok(!isOpen(app, 'menu') && isOpen(app, 'about'));
        app.key('Escape', {}, app.document.activeElement);
        assert.ok(!isOpen(app, 'about'));

        // Search closes whatever is open, and opening anything closes search.
        app.open('library');
        app.key('/');
        assert.equal(app.searchPanel().hidden, false);
        assert.ok(!isOpen(app, 'library'));
        app.key('b', { ctrlKey: true }, app.document.body);
        assert.ok(isOpen(app, 'library'));
        assert.equal(app.searchPanel().hidden, true);
        app.key('/');
        app.click(toggle(app, 'aa'));
        assert.equal(app.searchPanel().hidden, true);
        assert.ok(isOpen(app, 'aa'));
        const open = ['aa', 'library', 'menu', 'about'].filter((n) => isOpen(app, n));
        assert.deepEqual(open, ['aa']);
    }));

test('the bar sits above the page, which starts below it; nothing new is printed', () =>
    withApp({}, async (app) => {
        const css = app.document.querySelector('style').textContent;
        const screen = css.slice(css.indexOf('@media screen'));
        assert.match(screen, /\.container \{\s*padding-top: calc\(var\(--bar-height\) \+ 1\.5rem\);/);
        assert.match(screen, /scroll-padding-top: var\(--bar-height\);/);
        assert.match(css, /\.library-panel:not\(\[hidden\]\) \{[^}]*top: var\(--bar-height\);/, 'the Library hangs below the bar');
        const print = css.slice(css.indexOf('@media print'));
        const hidden = print.slice(0, print.indexOf('display: none !important'));
        for (const selector of ['.top-bar', '.popover', '.library-panel', '.toast', '.import-overlay', '.reader-exit']) {
            assert.ok(hidden.includes(selector), `print hides ${selector}`);
        }
        // Motion only for those who haven't asked for less.
        const blocks = css.match(/@media \(prefers-reduced-motion: no-preference\) \{([\s\S]*?)\n {8}\}/g);
        const motion = blocks.join('\n');
        assert.match(motion, /\.toast \{\s*transition:/);
        assert.match(motion, /\.library-panel:not\(\[hidden\]\) \{\s*animation:/);
        const rest = blocks.reduce((all, block) => all.replace(block, ''), css);
        assert.ok(!/\.toast[^{]*\{[^}]*transition/.test(rest), 'no fade outside the no-preference block');
        assert.ok(!/\.library-panel[^{]*\{[^}]*animation/.test(rest), 'no slide outside it either');
    }));

// ---- Aa

test('Aa changes the real preferences live, and they persist across a restart', async () => {
    let storage;
    await withApp({}, async (app) => {
        const root = app.document.documentElement;
        app.open('aa');
        const dialog = app.surface('aa');
        assert.equal(dialog.getAttribute('role'), 'dialog');
        assert.equal(dialog.getAttribute('aria-label'), 'Text and display');
        // In this order: translation and compare, text size, line spacing, theme, reading mode.
        const order = [...dialog.querySelectorAll('.section-label')].map((l) => l.textContent);
        assert.deepEqual(order, ['Translation', 'Text size', 'Line spacing', 'Theme']);
        assert.equal(dialog.lastElementChild.querySelector('button').id, 'reader-mode-btn');
        assert.ok($(app, 'compare-btn').closest('.aa-row').contains($(app, 'translation-choices')), 'Compare beside the translations');

        // Text size: A− / name / A+, each disabled at its end.
        const down = $(app, 'text-size-down');
        const up = $(app, 'text-size-up');
        const size = () => $(app, 'text-size-value').textContent;
        assert.equal(size(), 'Medium');
        up.focus();
        app.click(up);
        assert.equal(size(), 'Large');
        assert.equal(root.dataset.textSize, 'large');
        assert.equal(app.window.localStorage.getItem('gospel-getter-text-size'), 'large');
        app.click(up);
        assert.equal(size(), 'Extra large');
        assert.equal(up.disabled, true);
        assert.equal(app.document.activeElement, down, 'focus isn’t left on a disabled button');
        for (let i = 0; i < 4; i++) app.click(down);
        assert.equal(size(), 'Small');
        assert.equal(down.disabled, true);
        assert.equal(up.disabled, false);
        assert.equal(root.dataset.textSize, 'small');

        // Line spacing.
        assert.deepEqual(segments(app, 'line-spacing-choices').map((b) => b.textContent), ['Compact', 'Normal', 'Relaxed']);
        assert.deepEqual(pressed(app, 'line-spacing-choices'), ['normal']);
        app.click(segments(app, 'line-spacing-choices')[2]);
        assert.deepEqual(pressed(app, 'line-spacing-choices'), ['relaxed']);
        assert.equal(root.dataset.lineSpacing, 'relaxed');
        assert.equal($(app, 'line-spacing-select').value, 'relaxed', 'the same record the export reads');

        // Theme: six swatches from the theme list, each previewing its own theme.
        const swatches = segments(app, 'theme-choices');
        assert.deepEqual(swatches.map((s) => s.getAttribute('aria-label')),
            [...$(app, 'theme-select').options].map((o) => o.textContent));
        for (const s of swatches) {
            assert.equal(s.querySelector('[data-theme-preview]').dataset.themePreview, s.dataset.value);
        }
        assert.equal($(app, 'theme-name').textContent, 'Vaporwave');
        app.click(swatches.find((s) => s.dataset.value === 'matrix'));
        assert.equal(root.dataset.theme, 'matrix');
        assert.equal($(app, 'theme-name').textContent, 'Matrix');
        assert.deepEqual(pressed(app, 'theme-choices'), ['matrix']);
        assert.equal(app.window.localStorage.getItem('gospel-getter-theme'), 'matrix');
        assert.ok(isOpen(app, 'aa'), 'choosing doesn’t close Aa');

        // The export sees exactly these.
        assert.deepEqual(JSON.parse(JSON.stringify(app.window.gospelGetterPreferences.current())), {
            theme: 'matrix', text_size: 'small', line_spacing: 'relaxed', reading_mode: false, compare: false,
        });
        storage = app.storage();
        const keys = Object.keys(storage).sort();
        assert.deepEqual(keys, ['gospel-getter-line-spacing', 'gospel-getter-text-size', 'gospel-getter-theme'], 'no new storage keys');
    });
    await withApp({ storage }, async (app) => {
        const root = app.document.documentElement;
        assert.equal(root.dataset.theme, 'matrix');
        assert.equal(root.dataset.textSize, 'small');
        assert.equal(root.dataset.lineSpacing, 'relaxed');
        app.open('aa');
        assert.deepEqual(pressed(app, 'theme-choices'), ['matrix']);
        assert.deepEqual(pressed(app, 'line-spacing-choices'), ['relaxed']);
        assert.equal($(app, 'text-size-value').textContent, 'Small');
        assert.equal($(app, 'text-size-down').disabled, true);
    });
});

test('Aa switches the translation through the existing path, from the list the app loads', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await backend('add_bookmark', { bookId: bookId('Psalms'), chapter: 23, verseStart: 1, verseEnd: 1 });
        app.open('aa');
        const home = await backend('get_home');
        const buttons = segments(app, 'translation-choices');
        assert.deepEqual(buttons.map((b) => b.textContent), home.translations.map((t) => t.code.toUpperCase()));
        assert.deepEqual(buttons.map((b) => b.title), home.translations.map((t) => t.name));
        assert.deepEqual(pressed(app, 'translation-choices'), ['kjv']);

        const before = app.calls.length;
        app.click(buttons.find((b) => b.dataset.value === 'web'));
        await app.idle(before, 'list_bookmarks');
        const cmds = app.calls.slice(before).map((c) => c.cmd);
        for (const cmd of ['get_reading', 'list_bookmarks', 'list_highlight_passages']) assert.ok(cmds.includes(cmd), cmd);
        assert.equal($(app, 'translation-select').value, 'web');
        assert.deepEqual(pressed(app, 'translation-choices'), ['web']);
        assert.equal(app.verse(16, 'web').querySelector('.verse-text').textContent, storedVerse('web', 'John', 3, 16));
        assert.equal(app.bookmarkItems()[0].preview, storedVerse('web', 'Psalms', 23, 1), 'the Library follows');
        assert.ok(isOpen(app, 'aa'));
        // Choosing the one already chosen does nothing.
        const again = app.calls.length;
        app.click(segments(app, 'translation-choices').find((b) => b.dataset.value === 'web'));
        await app.idle(again);
        assert.equal(app.calls.length, again);
    }));

test('Compare sits in Aa and C still toggles it; Reading mode leaves Aa for the exit button', () =>
    withApp({}, async (app) => {
        const compare = $(app, 'compare-btn');
        assert.ok(app.surface('aa').contains(compare));
        let before = app.calls.length;
        app.key('c');
        await app.idle(before);
        assert.ok(app.isComparing());
        assert.equal(compare.getAttribute('aria-pressed'), 'true');
        app.open('aa');
        before = app.calls.length;
        app.click(compare);
        await app.idle(before);
        assert.ok(!app.isComparing());

        app.click($(app, 'reader-mode-btn'));
        assert.equal(app.document.documentElement.dataset.readerMode, 'on');
        assert.ok(!isOpen(app, 'aa'));
        assert.equal(app.document.activeElement, $(app, 'reader-exit'));
        app.click($(app, 'reader-exit'));
        assert.equal(app.document.documentElement.dataset.readerMode, undefined);
        assert.equal(app.document.activeElement, toggle(app, 'aa'), 'back to Aa, which holds the toggle');
    }));

// ---- Reading mode

test('reading mode keeps Aa, search and Exit on screen, and hides the Library, ☰ and the bar', async () => {
    await withApp({}, async (app) => {
        for (const id of ['aa-toggle', 'library-toggle', 'search-toggle', 'menu-toggle']) {
            assert.ok(shownOnScreen(app, $(app, id)), `${id} shows`);
        }
        assert.ok(!shownOnScreen(app, $(app, 'reader-exit')));
    });
    await withApp({ readerMode: true }, async (app) => {
        for (const id of ['aa-toggle', 'search-toggle', 'reader-exit']) {
            assert.ok(shownOnScreen(app, $(app, id)), `${id} shows in reading mode`);
        }
        for (const id of ['library-toggle', 'menu-toggle']) {
            assert.ok(!shownOnScreen(app, $(app, id)), `${id} hides in reading mode`);
        }
        const css = app.document.querySelector('style').textContent;
        assert.match(css, /:root\[data-reader-mode="on"\] \.top-bar \{\s*background: transparent;/);
        // Exit sits in the bar, before Aa, not at a fixed offset.
        assert.equal($(app, 'reader-exit').parentElement, $(app, 'top-bar'));
        assert.equal($(app, 'reader-exit').nextElementSibling, toggle(app, 'aa').parentElement);

        // Aa still works; the Library doesn't open from its shortcut.
        app.open('aa');
        assert.ok(isOpen(app, 'aa'));
        app.key('Escape', {}, app.document.activeElement);
        app.key('b', { ctrlKey: true });
        assert.ok(!isOpen(app, 'library'));
        // Escape then leaves reading mode, as before.
        app.key('Escape');
        assert.equal(app.document.documentElement.dataset.readerMode, undefined);
    });
});

test('Later on the update banner hands focus to ☰, or to Aa in reading mode', async () => {
    const update = { check: { version: '9.9.9', notes: null, date: null, canSelfUpdate: true } };
    for (const [readerMode, id] of [[false, 'menu-toggle'], [true, 'aa-toggle']]) {
        await withApp({ update, readerMode }, async (app) => {
            $(app, 'update-later').focus();
            app.click($(app, 'update-later'));
            assert.equal(app.document.activeElement, $(app, id));
        });
    }
    await setUpdate({});
});

// ---- Library

test('the Library has Bookmarks and Highlights tabs with live counts', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        const count = (id) => $(app, id).textContent;
        assert.equal(count('bookmark-count'), '0');
        assert.equal(count('highlight-count'), '0');

        app.click(app.verse(16));
        await clickAndWait(app, app.bookmarkButton(), 'list_bookmarks');
        assert.equal(count('bookmark-count'), '1');
        await clickAndWait(app, app.actions().querySelector('.highlight-swatch[data-color="pink"]'), 'list_highlight_passages');
        assert.equal(count('highlight-count'), '1');
        app.click(app.verse(18));
        await clickAndWait(app, app.actions().querySelector('.highlight-swatch[data-color="green"]'), 'list_highlight_passages');
        assert.equal(count('highlight-count'), '2');

        app.open('library');
        const panel = app.surface('library');
        assert.equal(panel.tagName, 'ASIDE');
        assert.equal($(app, panel.getAttribute('aria-labelledby')).textContent, 'Library');
        const tabs = [...panel.querySelectorAll('[role="tab"]')];
        assert.deepEqual(tabs.map((t) => t.textContent.trim()), ['Bookmarks 1', 'Highlights 2']);
        assert.equal(panel.querySelector('[role="tablist"]').children.length, 2);
        assert.equal($(app, 'library-bookmarks').hidden, false);
        assert.equal($(app, 'library-highlights').hidden, true);

        // Arrow keys move between tabs (and don't turn the chapter).
        app.key('ArrowRight', {}, tabs[0]);
        assert.equal(app.heading(), 'John 3');
        assert.equal(tabs[1].getAttribute('aria-selected'), 'true');
        assert.equal(tabs[1].tabIndex, 0);
        assert.equal(tabs[0].tabIndex, -1);
        assert.equal(app.document.activeElement, tabs[1]);
        assert.equal($(app, 'library-highlights').hidden, false);
        assert.equal($(app, 'library-bookmarks').hidden, true);
        app.key('Home', {}, tabs[1]);
        assert.equal(tabs[0].getAttribute('aria-selected'), 'true');

        // A translation switch refreshes both lists.
        const select = $(app, 'translation-select');
        const before = app.calls.length;
        select.value = 'web';
        select.dispatchEvent(new app.window.Event('change', { bubbles: true }));
        await app.idle(before, 'list_highlight_passages');
        assert.equal(app.bookmarkItems()[0].preview, storedVerse('web', 'John', 3, 16));
        assert.equal(count('highlight-count'), '2');
    }));

test('the Library keeps its tab and color filter for the session, not across a restart', async () => {
    await backend('__reset', { bookId: bookId('John'), chapter: 3, translationCode: 'kjv' });
    for (const [verse, color] of [[1, 'pink'], [5, 'green']]) {
        await backend('set_highlight', { bookId: bookId('John'), chapter: 3, verseStart: verse, verseEnd: verse, color });
    }
    let storage;
    await withApp({ book: 'John', chapter: 3, keepBookmarks: true }, async (app) => {
        const before = Object.keys(app.storage()).sort();
        app.openLibrary('highlights');
        app.click($(app, 'highlight-filter').querySelector('[data-filter="pink"]'));
        assert.equal(app.document.querySelectorAll('#highlight-list .bookmark-item').length, 1);
        app.key('Escape', {}, app.document.activeElement);
        assert.ok(!isOpen(app, 'library'));

        app.key('b', { ctrlKey: true });
        assert.ok(isOpen(app, 'library'));
        assert.equal($(app, 'library-tab-highlights').getAttribute('aria-selected'), 'true', 'the tab is remembered');
        assert.equal(app.document.activeElement, $(app, 'library-tab-highlights'));
        assert.equal($(app, 'highlight-filter').querySelector('[aria-pressed="true"]').dataset.filter, 'pink', 'and the filter');
        storage = app.storage();
        assert.deepEqual(Object.keys(storage).sort(), before, 'nothing stored');
    });
    await withApp({ book: 'John', chapter: 3, keepBookmarks: true, storage }, async (app) => {
        app.open('library');
        assert.equal($(app, 'library-tab-bookmarks').getAttribute('aria-selected'), 'true');
        assert.equal($(app, 'highlight-filter').querySelector('[aria-pressed="true"]').dataset.filter, 'all');
    });
});

test('Ctrl+B toggles the Library; jump and remove work from it', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        for (const n of [1, 2]) {
            app.click(app.verse(n));
            await clickAndWait(app, app.bookmarkButton(), 'list_bookmarks');
        }
        app.key('Escape');

        const press = app.key('b', { ctrlKey: true });
        assert.ok(press.defaultPrevented);
        assert.ok(isOpen(app, 'library'));
        app.key('b', { ctrlKey: true }, app.document.activeElement);
        assert.ok(!isOpen(app, 'library'));
        assert.equal(app.document.activeElement, toggle(app, 'library'));
        // Not while typing in a box.
        app.key('/');
        app.key('b', { ctrlKey: true }, app.searchInput());
        assert.ok(!isOpen(app, 'library'));
        app.key('Escape', {}, app.searchInput());

        // Remove keeps focus in the list, then on the tab once it's empty.
        app.key('b', { ctrlKey: true });
        await clickAndWait(app, app.bookmarkItems()[0].remove, 'list_bookmarks');
        assert.equal($(app, 'bookmark-count').textContent, '1');
        assert.ok(isOpen(app, 'library'));
        assert.equal(app.document.activeElement, app.bookmarkItems()[0].remove);

        // Jump: the Library gets out of the way; focus returns to its button.
        app.key('ArrowRight', {}, app.document.body);
        await app.waitForHeading('John 4');
        await clickAndWait(app, app.bookmarkItems()[0].open, 'get_reading');
        assert.equal(app.heading(), 'John 3');
        assert.deepEqual(app.selectedVerseNumbers(), [1]);
        assert.ok(!isOpen(app, 'library'));
        assert.equal(app.document.activeElement, toggle(app, 'library'));

        app.open('library');
        await clickAndWait(app, app.bookmarkItems()[0].remove, 'list_bookmarks');
        assert.equal($(app, 'bookmark-count').textContent, '0');
        assert.equal(app.document.activeElement, $(app, 'library-tab-bookmarks'));
    }));

// ---- The ☰ menu

test('☰ is a menu: arrows, Home and End move, Tab and Escape close, and arrows never turn the chapter', () =>
    withApp({}, async (app) => {
        const menu = app.surface('menu');
        assert.equal(menu.getAttribute('role'), 'menu');
        assert.equal(menu.getAttribute('aria-label'), 'Menu');
        assert.equal(toggle(app, 'menu').getAttribute('aria-label'), 'Menu');
        const items = [...menu.querySelectorAll('[role="menuitem"]')];
        assert.deepEqual(items.map((i) => i.textContent), ['Export data…', 'Import data…', 'Show welcome', 'About Gospel Getter']);
        assert.equal(menu.querySelector('[role="separator"]').previousElementSibling, items[1], 'a divider after Import');
        assert.ok(items.every((i) => i.tabIndex === -1), 'one tab stop: arrows move within');

        // ↓ on ☰ opens it on the first item, ↑ on the last.
        assert.ok(app.key('ArrowDown', {}, toggle(app, 'menu')).defaultPrevented);
        assert.equal(app.document.activeElement, items[0]);
        const move = (key) => app.key(key, {}, app.document.activeElement);
        move('ArrowDown');
        assert.equal(app.document.activeElement, items[1]);
        move('ArrowUp');
        move('ArrowUp');
        assert.equal(app.document.activeElement, items[3], 'wraps');
        move('Home');
        assert.equal(app.document.activeElement, items[0]);
        move('End');
        assert.equal(app.document.activeElement, items[3]);
        move('ArrowLeft');
        move('ArrowRight');
        await app.idle(0);
        assert.equal(app.heading(), 'John 3');

        move('Tab');
        assert.ok(!isOpen(app, 'menu'));
        assert.equal(app.document.activeElement, toggle(app, 'menu'));
        app.key('ArrowUp', {}, toggle(app, 'menu'));
        assert.equal(app.document.activeElement, items[3]);
        move('Escape');
        assert.ok(!isOpen(app, 'menu'));
        assert.equal(app.document.activeElement, toggle(app, 'menu'));
    }));

test('the menu items export, import, show the welcome and open About', () =>
    withApp({}, async (app) => {
        const toastText = () => app.toast().textContent;
        const item = (id) => $(app, id);

        await app.pick(null);
        app.open('menu');
        await clickAndWait(app, item('export-data-btn'), 'export_reader_data');
        assert.ok(!isOpen(app, 'menu'));
        assert.equal(toastText(), 'Export cancelled. No file was saved.');

        await app.pick(null);
        app.open('menu');
        await clickAndWait(app, item('import-data-btn'), 'choose_import_file');
        assert.equal(toastText(), 'Import cancelled. Nothing was changed.');
        assert.equal(app.document.activeElement, toggle(app, 'menu'));

        app.open('menu');
        app.click(item('welcome-btn'));
        assert.ok(!isOpen(app, 'menu'));
        assert.equal($(app, 'welcome-overlay').hidden, false);
        assert.equal(app.document.activeElement, $(app, 'welcome-next'), 'the menu doesn’t take focus back');
        app.click($(app, 'welcome-skip'));
        assert.equal(app.document.activeElement, toggle(app, 'menu'));

        app.open('menu');
        app.click(item('about-btn'));
        assert.ok(!isOpen(app, 'menu'));
        assert.ok(isOpen(app, 'about'));
        assert.equal(app.document.activeElement, $(app, 'about-close'));
        app.click($(app, 'about-close'));
        assert.ok(!isOpen(app, 'about'));
        assert.equal(app.document.activeElement, toggle(app, 'menu'));
    }));

// ---- The toast

test('export and import report in a toast that fades, polite to screen readers', () =>
    withApp({ update: { check: { version: '9.9.9', notes: null, date: null, canSelfUpdate: true } } }, async (app) => {
        const toast = app.toast();
        assert.equal(toast.getAttribute('role'), 'status');
        assert.equal(toast.getAttribute('aria-live'), 'polite');
        assert.ok(toast.classList.contains('is-hidden'), 'nothing to say yet');
        assert.ok(!app.surface('library').contains(toast) && !app.surface('menu').contains(toast));

        await app.pick(null);
        app.open('menu');
        await clickAndWait(app, $(app, 'export-data-btn'), 'export_reader_data');
        assert.ok(!toast.classList.contains('is-hidden'));
        assert.equal(toast.textContent, 'Export cancelled. No file was saved.');
        // The update banner is showing: the toast sits above it, not over it.
        assert.equal($(app, 'update-banner').hidden, false);
        assert.notEqual(toast.style.bottom, '');

        await new Promise((r) => setTimeout(r, 6300));
        assert.ok(toast.classList.contains('is-hidden'), 'faded');
        assert.equal(toast.textContent, 'Export cancelled. No file was saved.', 'the words stay for the record');

        app.failCommand('export_reader_data', 'The disk is full.');
        app.open('menu');
        await clickAndWait(app, $(app, 'export-data-btn'), 'export_reader_data');
        assert.equal(toast.textContent, 'Export failed: The disk is full.');
        assert.ok(toast.classList.contains('is-error'));
        assert.ok(!toast.classList.contains('is-hidden'));
    }));
