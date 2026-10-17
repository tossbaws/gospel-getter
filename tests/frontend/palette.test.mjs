// Behavior tests for the command palette in the search box: its groups
// (Go to, Commands, Verses), ">" for commands only, word-prefix matching,
// the combobox's keys and ARIA, commands that can't run now, and each
// command running its own control's code.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, closeBridge, openApp, settle } from './harness.mjs';

after(closeBridge);

const EM = '—';

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const input = (app) => app.searchInput();
const groups = (app) => [...app.document.querySelectorAll('#search-results [role="group"]')].map((g) => ({
    label: app.document.getElementById(g.getAttribute('aria-labelledby')).textContent,
    options: [...g.querySelectorAll('[role="option"]')],
}));
const groupLabels = (app) => groups(app).map((g) => g.label);
const commandLabels = (app) => [...app.document.querySelectorAll('#search-results .search-command .palette-label')].map((el) => el.textContent);
const option = (app, label) => [...app.document.querySelectorAll('#search-results .search-command')]
    .find((el) => el.querySelector('.palette-label').textContent === label);
const active = (app) => {
    const id = input(app).getAttribute('aria-activedescendant');
    return id ? app.document.getElementById(id) : null;
};

/** Opens the box (if it's closed) and types `text`, waiting for any search. */
async function type(app, text) {
    if (app.searchPanel().hidden) app.key('k', { ctrlKey: true });
    const before = app.calls.length;
    app.type(input(app), text);
    await new Promise((r) => setTimeout(r, 200));
    await app.idle(before);
}

async function enter(app) {
    const before = app.calls.length;
    const event = app.key('Enter', {}, input(app));
    await app.idle(Math.max(0, before - 1));
    return event;
}

/** Counts clicks on `el` (the control a command should click). */
function spyClicks(el) {
    const spy = { count: 0 };
    el.addEventListener('click', () => { spy.count += 1; });
    return spy;
}

test('the empty box suggests a few commands, and says what it takes', () =>
    withApp({}, async (app) => {
        app.key('k', { ctrlKey: true });
        assert.equal(app.searchPanel().hidden, false);
        assert.deepEqual(groupLabels(app), ['Commands']);
        assert.deepEqual(commandLabels(app), ['Compare on', 'Reading mode', 'Random chapter', 'Copy chapter', 'Export notes…']);
        assert.equal(app.searchStatus(), 'Type words to search, a reference to go there, or > for a command.');
        assert.equal(active(app), null, 'nothing runs on a bare Enter');
        await enter(app);
        assert.equal(app.searchPanel().hidden, false);
        assert.equal(app.isComparing(), false);
        // `/` opens the same box.
        app.key('Escape', {}, input(app));
        app.key('/');
        assert.deepEqual(groupLabels(app), ['Commands']);
    }));

test('the box is a combobox over a listbox of named groups', () =>
    withApp({}, async (app) => {
        await type(app, 'mark');
        const box = input(app);
        const list = app.document.getElementById('search-results');
        assert.equal(box.getAttribute('role'), 'combobox');
        assert.equal(box.getAttribute('aria-controls'), 'search-results');
        assert.equal(box.getAttribute('aria-autocomplete'), 'list');
        assert.equal(box.getAttribute('aria-expanded'), 'true');
        assert.equal(list.getAttribute('role'), 'listbox');
        for (const g of groups(app)) {
            for (const o of g.options) {
                assert.equal(o.getAttribute('role'), 'option');
                assert.ok(o.id, 'every option has an id');
                // Its group is announced with it.
                assert.equal(app.document.getElementById(o.getAttribute('aria-describedby')).textContent, g.label);
            }
        }
        assert.equal(app.document.activeElement, box);
        assert.equal(active(app), groups(app)[0].options[0]);
        assert.equal(active(app).getAttribute('aria-selected'), 'true');

        await type(app, 'zzzz qqqq');
        assert.equal(box.getAttribute('aria-expanded'), 'false');
        assert.equal(box.getAttribute('aria-activedescendant'), null);
    }));

test('groups come in order: Go to, Commands, Verses; ↓ and ↑ move through all of them', () =>
    withApp({
        translation: 'kjv',
        // No real reference is also a word a command begins with (a book
        // alone is searched as a word), so for the Go to group's place
        // the search answers "mark" as it would "Mark 1".
        alter: {
            search: (result) => (result.kind === 'text' && result.terms.join(' ') === 'mark'
                ? { kind: 'passage', bookId: 41, chapter: 1, verseStart: null, verseEnd: null, reference: 'Mark 1' }
                : result),
        },
    }, async (app) => {
        await type(app, 'mark');
        assert.deepEqual(groupLabels(app), ['Go to', 'Commands']);
        assert.equal(groups(app)[0].options[0].textContent.trim(), 'Go to Mark 1');
        assert.deepEqual(commandLabels(app), ['Export notes…', 'Copy chapter as…'], 'their other words include Markdown');
        assert.equal(active(app), groups(app)[0].options[0], 'Go to is active');

        // Words some commands and some verses have.
        await type(app, 'compare');
        assert.deepEqual(groupLabels(app), ['Commands', 'Verses']);
        assert.deepEqual(commandLabels(app), ['Compare on', 'Compare with: BSB', 'Compare with: WEB']);
        const all = groups(app).flatMap((g) => g.options);
        assert.ok(all.length > 3);
        assert.equal(active(app), all[0], 'the first command is active');
        for (let i = 1; i < all.length; i++) {
            app.key('ArrowDown', {}, input(app));
            assert.equal(active(app), all[i]);
        }
        app.key('ArrowDown', {}, input(app));
        assert.equal(active(app), all.at(-1), 'stops at the last');
        for (let i = all.length - 2; i >= 0; i--) app.key('ArrowUp', {}, input(app));
        assert.equal(active(app), all[0]);
        app.key('ArrowUp', {}, input(app));
        assert.equal(active(app), null, 'above the first is the box itself');

        // "john 3" is a reference: Go to, as before.
        await type(app, 'john 3');
        assert.deepEqual(groupLabels(app), ['Go to']);
        assert.equal(app.searchStatus(), 'Press Enter to go there.');
    }));

test('">" lists commands only, without searching', () =>
    withApp({}, async (app) => {
        await type(app, '>theme');
        const searches = app.calls.filter((c) => c.cmd === 'search').length;
        assert.deepEqual(groupLabels(app), ['Commands']);
        assert.deepEqual(commandLabels(app), [
            'Theme: Chi Rho', 'Theme: Vellum', 'Theme: System', 'Theme: Vaporwave', 'Theme: Classic Dark',
            'Theme: Classic Light', 'Theme: Matrix', 'Theme: Beast Slayer', 'Theme: Hot Pink',
        ]);
        await type(app, '> theme ma');
        assert.deepEqual(commandLabels(app), ['Theme: Matrix']);
        await type(app, '>');
        assert.equal(commandLabels(app).length, 39);
        assert.equal(commandLabels(app)[0], 'Compare on');
        assert.equal(commandLabels(app).at(-1), 'Previous chapter');
        await type(app, '>nothing like this');
        assert.deepEqual(groupLabels(app), []);
        assert.equal(app.searchStatus(), 'No commands match.');
        assert.equal(app.calls.filter((c) => c.cmd === 'search').length, searches, 'no search for commands');
        // Words alone would have searched.
        await type(app, 'theme');
        assert.ok(app.calls.filter((c) => c.cmd === 'search').length > searches);
    }));

test('commands match word prefixes, in any order, ignoring case', () =>
    withApp({}, async (app) => {
        for (const [query, labels] of [
            ['rea mo', ['Reading mode']],
            ['mo rea', ['Reading mode']],
            ['READING', ['Reading mode']],
            ['theme ma', ['Theme: Matrix']],
            ['Theme: classic d', ['Theme: Classic Dark']],
            ['text size lar', ['Text size: Larger']],
            ['open lib no', ['Open Library: Notes']],
            ['export', ['Export notes…', 'Export data…']],
            ['copy ch', ['Copy chapter', 'Copy chapter as…']],
            ['next', ['Next chapter']],
            ['translation bs', ['Translation: BSB']],
            // The translations' names are words too.
            ['translation berean', ['Translation: BSB']],
        ]) {
            await type(app, query);
            assert.deepEqual(commandLabels(app), labels, query);
        }
        // A prefix, not any part of a word.
        await type(app, 'eading');
        assert.deepEqual(commandLabels(app), []);
    }));

test('commands show their keys', () =>
    withApp({}, async (app) => {
        await type(app, '>');
        for (const [label, key, keys] of [
            ['Compare on', 'C', 'C'],
            ['Text size: Larger', 'Ctrl+=', 'Control+='],
            ['Text size: Medium', 'Ctrl+0', 'Control+0'],
            ['Open Library: Bookmarks', 'Ctrl+B', 'Control+B'],
            ['Next chapter', '→', 'ArrowRight'],
            ['Previous chapter', '←', 'ArrowLeft'],
        ]) {
            const o = option(app, label);
            assert.equal(o.querySelector('kbd').textContent, key, label);
            assert.equal(o.getAttribute('aria-keyshortcuts'), keys, label);
        }
        assert.equal(option(app, 'Random chapter').querySelector('kbd'), null);
    }));

test('Enter runs a command through its own control, and the box closes', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        const byId = (id) => app.document.getElementById(id);
        const root = app.document.documentElement;

        // Compare: Aa's toggle, as C presses it.
        let spy = spyClicks(byId('compare-btn'));
        await type(app, 'compare on');
        await enter(app);
        assert.equal(spy.count, 1);
        assert.equal(app.searchPanel().hidden, true);
        await app.idle(0);
        assert.ok(app.isComparing());
        await type(app, 'compare');
        assert.equal(commandLabels(app)[0], 'Compare off', 'named for what it would do');
        await enter(app);
        assert.equal(spy.count, 2);
        await app.idle(0);
        assert.ok(!app.isComparing());

        // A theme: Aa's swatch.
        spy = spyClicks(app.document.querySelector('#theme-choices [data-value="matrix"]'));
        await type(app, 'theme ma');
        await enter(app);
        assert.equal(spy.count, 1);
        assert.equal(root.dataset.theme, 'matrix');
        assert.equal(byId('theme-select').value, 'matrix');
        assert.equal(app.document.querySelector('#theme-choices [data-value="matrix"]').getAttribute('aria-pressed'), 'true');

        // Width and layout: Aa's segments.
        spy = spyClicks(app.document.querySelector('#width-choices [data-value="wide"]'));
        await type(app, 'width wide');
        await enter(app);
        assert.equal(spy.count, 1);
        assert.equal(byId('width-select').value, 'wide');
        spy = spyClicks(app.document.querySelector('#layout-choices [data-value="flowing"]'));
        await type(app, 'layout flow');
        await enter(app);
        assert.equal(spy.count, 1);
        assert.equal(root.dataset.layout, 'flowing');

        // Text size: Ctrl+='s own step, with its toast.
        await type(app, 'text size larger');
        await enter(app);
        assert.equal(byId('text-size-select').value, 'large');
        assert.equal(byId('size-status').textContent, 'Text size: Large');
        await type(app, 'text size medium');
        await enter(app);
        assert.equal(byId('text-size-select').value, 'medium');

        // A translation: the <select>'s change, as Aa's rows make it.
        let changes = 0;
        byId('translation-select').addEventListener('change', () => { changes += 1; });
        await type(app, 'translation bsb');
        const before = app.calls.length;
        await enter(app);
        await app.idle(before, 'get_reading');
        assert.equal(changes, 1);
        assert.equal(byId('translation-select').value, 'bsb');
        assert.equal(app.calls.filter((c) => c.cmd === 'get_reading').at(-1).args.translationCode, 'bsb');

        // Random chapter: the picker's button.
        spy = spyClicks(byId('random-chapter-btn'));
        await type(app, 'random');
        await enter(app);
        assert.equal(spy.count, 1);

        // The Library, on a tab.
        await type(app, 'open library notes');
        await enter(app);
        assert.equal(app.surface('library').hidden, false);
        assert.equal(byId('library-tab-notes').getAttribute('aria-selected'), 'true');
        assert.equal(app.document.activeElement, byId('library-tab-notes'));
        app.key('Escape', {}, app.document.activeElement);

        // The ☰ menu's items.
        for (const [query, id] of [
            ['export data', 'export-data-btn'],
            ['import data', 'import-data-btn'],
            ['show welcome', 'welcome-btn'],
            ['about gospel', 'about-btn'],
            // About, then its Check now.
            ['check for updates', 'update-check-btn'],
        ]) {
            spy = spyClicks(byId(id));
            await type(app, query);
            await enter(app);
            assert.equal(spy.count, 1, query);
            app.key('Escape', {}, app.document.activeElement);
            await app.idle(0);
        }
    }));

test('next and previous chapter turn as the arrow keys do; reading mode and the sidebar toggle', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        const root = app.document.documentElement;
        await type(app, 'next chapter');
        let before = app.calls.length;
        await enter(app);
        await app.idle(before, 'get_reading');
        assert.equal(app.heading(), 'John 4');
        await type(app, 'previous chapter');
        before = app.calls.length;
        await enter(app);
        await app.idle(before, 'get_reading');
        assert.equal(app.heading(), 'John 3');

        const sidebar = spyClicks(app.document.getElementById('sidebar-toggle'));
        await type(app, 'hide sidebar');
        await enter(app);
        assert.equal(sidebar.count, 1);
        assert.equal(root.dataset.sidebar, 'closed');
        await type(app, 'sidebar');
        assert.deepEqual(commandLabels(app), ['Show sidebar']);

        await type(app, 'reading mode');
        await enter(app);
        assert.equal(root.dataset.readerMode, 'on');
        await type(app, 'reading mode');
        assert.deepEqual(commandLabels(app), ['Exit reading mode']);
        assert.equal(option(app, 'Exit reading mode').querySelector('kbd').textContent, 'Esc');
        await enter(app);
        assert.notEqual(root.dataset.readerMode, 'on');
    }));

test('a command that can’t run now is listed disabled, with the reason, and Enter on it explains', () =>
    withApp({ book: 'Revelation', chapter: 22 }, async (app) => {
        await type(app, '>');
        const reason = (label) => {
            const o = option(app, label);
            assert.ok(o, label);
            return o.getAttribute('aria-disabled') === 'true' ? o.querySelector('.palette-reason').textContent : null;
        };
        assert.equal(reason('Compare with: WEB'), 'Only while comparing');
        assert.equal(reason('Theme: Chi Rho'), 'In use');
        assert.equal(reason('Theme: Vellum'), null);
        assert.equal(reason('Translation: KJV'), 'In use');
        assert.equal(reason('Text size: Medium'), 'In use');
        assert.equal(reason('Next chapter'), 'This is the Bible’s last chapter');
        assert.equal(reason('Previous chapter'), null);
        assert.equal(reason('Export notes…'), 'There are no notes yet');
        // The reason is part of the option's name.
        assert.match(option(app, 'Next chapter').textContent, /Next chapter, This is the Bible’s last chapter/);

        await type(app, 'compare with web');
        assert.equal(active(app), option(app, 'Compare with: WEB'));
        const spy = spyClicks(app.document.getElementById('compare-btn'));
        await enter(app);
        assert.equal(app.searchPanel().hidden, false, 'the box stays');
        assert.equal(app.searchStatus(), 'Compare with: WEB: Only while comparing.');
        assert.equal(spy.count, 0);
        assert.equal(app.document.documentElement.dataset.compareWith, undefined);
        // A click is the same.
        app.click(option(app, 'Compare with: WEB'));
        assert.equal(app.searchPanel().hidden, false);
    }));

test('commands follow the state: Compare with while comparing, Layout disabled then, Larger at the largest', () =>
    withApp({ compare: true, storage: { 'gospel-getter-text-size': 'x-large' } }, async (app) => {
        await type(app, '>');
        const disabled = (label) => option(app, label).getAttribute('aria-disabled') === 'true';
        assert.ok(!disabled('Compare with: BSB'));
        assert.ok(disabled('Compare with: WEB'), 'the one beside it is in use');
        assert.ok(disabled('Layout: Flowing'));
        assert.equal(option(app, 'Layout: Flowing').querySelector('.palette-reason').textContent, 'Compare always shows a verse to a row');
        assert.ok(disabled('Text size: Larger'));
        assert.ok(!disabled('Text size: Smaller'));

        await type(app, 'compare with bsb');
        const before = app.calls.length;
        await enter(app);
        await app.idle(before, 'get_compare');
        assert.deepEqual(app.compareColumns().map((c) => c.match(/\((\w+)\)$/)[1]), ['KJV', 'BSB']);
        assert.equal(app.document.documentElement.dataset.compareWith, 'bsb');
    }));

test('a word search keeps Enter as it was: nothing until a verse is chosen', () =>
    withApp({ translation: 'web' }, async (app) => {
        await type(app, 'shepherd');
        assert.deepEqual(groupLabels(app), ['Verses']);
        assert.equal(active(app), null);
        await enter(app);
        assert.equal(app.searchPanel().hidden, false);
        app.key('ArrowDown', {}, input(app));
        const first = active(app);
        const before = app.calls.length;
        await enter(app);
        await app.idle(before, 'get_reading');
        assert.equal(app.searchPanel().hidden, true);
        assert.equal(`${app.heading()}`, first.querySelector('.search-ref').textContent.replace(/:\d+$/, ''));
    }));

test('Enter straight after typing a reference waits for it, as before', () =>
    withApp({}, async (app) => {
        app.key('k', { ctrlKey: true });
        app.type(input(app), 'rev 22:21');
        const before = app.calls.length;
        app.key('Enter', {}, input(app));
        await app.idle(before, 'get_reading');
        assert.equal(app.heading(), 'Revelation 22');
        assert.deepEqual(app.selectedVerseNumbers(), [21]);
    }));

test('Escape closes the box first, then the rest as before', () =>
    withApp({ book: 'John', chapter: 3, readerMode: true }, async (app) => {
        app.click(app.verse(16));
        await app.idle(0);
        await type(app, 'theme');
        app.key('Escape', {}, input(app));
        assert.equal(app.searchPanel().hidden, true);
        assert.deepEqual(app.selectedVerseNumbers(), [16], 'the selection stays');
        assert.equal(app.document.documentElement.dataset.readerMode, 'on');
        assert.equal(app.document.activeElement, app.document.getElementById('search-toggle'));
        app.key('Escape');
        assert.deepEqual(app.selectedVerseNumbers(), []);
        assert.equal(app.document.documentElement.dataset.readerMode, 'on');
        app.key('Escape');
        assert.notEqual(app.document.documentElement.dataset.readerMode, 'on');
    }));

test('Copy chapter and Copy chapter as… from the palette', () =>
    withApp({ book: 'Psalms', chapter: 23, translation: 'kjv' }, async (app) => {
        const expected = await backend('get_copy_text', { translationCode: 'kjv', bookId: bookId('Psalms'), chapter: 23 });
        await type(app, 'copy chapter');
        assert.equal(active(app), option(app, 'Copy chapter'));
        await enter(app);
        assert.equal(app.searchPanel().hidden, true);
        assert.equal(app.clipboardWrites.at(-1), expected.formats.reference);
        assert.ok(app.clipboardWrites.at(-1).endsWith(`${EM} Psalms 23 (KJV)`));
        assert.equal(app.toast().textContent, 'Copied Psalms 23');

        await type(app, 'copy chapter as');
        assert.equal(active(app), option(app, 'Copy chapter as…'));
        await enter(app);
        const menu = app.document.getElementById('copy-as-menu');
        assert.ok(menu.isConnected && !menu.hidden);
        assert.equal(menu.previousElementSibling, app.document.querySelector('.chapter-copy'));
        assert.equal(app.document.activeElement, menu.querySelector('[role="menuitem"]'));
        const markdown = [...menu.querySelectorAll('[role="menuitem"]')].find((i) => i.textContent === 'Markdown quote');
        const before = app.calls.length;
        app.click(markdown);
        await app.idle(Math.max(0, before - 1));
        assert.equal(app.clipboardWrites.at(-1), expected.formats.markdown);
        assert.equal(app.toast().textContent, 'Copied Psalms 23 as a Markdown quote');
        assert.equal(app.document.activeElement, app.document.querySelector('.chapter-copy-btn'));

        // Escape from the menu goes back to Copy chapter…, not further.
        app.click(app.document.querySelector('.chapter-copy-btn'));
        app.key('Escape', {}, app.document.activeElement);
        assert.ok(menu.hidden);
        assert.equal(app.document.activeElement, app.document.querySelector('.chapter-copy-btn'));
        await settle();
    }));
