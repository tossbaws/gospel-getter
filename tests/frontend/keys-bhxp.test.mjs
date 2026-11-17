// B, H, X and P (H1-H7): B turns the selected verses' bookmark on or off,
// as ★ Bookmark does; H steps their highlight through the colors and off
// (mixed colors go to yellow); X closes the second pane, as its × does; P
// switches the active pane, as F6 does, and never swaps the passages. B and
// H do nothing without a selection, X and P nothing with one pane, and the
// key goes on unprevented then. Typed in a field they're letters, and with
// Single-key shortcuts off they're nothing (single-keys.test.mjs tries
// every character key in the table; the split panes are tried here).

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, closeBridge, el, notePanelOpen, select, settle, swatch, withApp } from './harness.mjs';

after(closeBridge);

const COLORS = ['yellow', 'green', 'blue', 'pink'];
// Psalm 23 has no verse 16, so John 3:16 is the only one on the page.
const PSALMS = bookId('Psalms');

const panes = (app) => app.window.gospelGetterPanes;
const pressed = (app) => COLORS.filter((c) => swatch(app, c).getAttribute('aria-pressed') === 'true');
const marker = (app, n) => app.verse(n).dataset.highlight || null;

/** Presses `key` and waits for every call it made. */
async function press(app, key, opts = {}, target) {
    const before = app.calls.length;
    const event = await app.key(key, opts, target);
    await app.idle(before);
    return event;
}

/** The second pane opened beside, the first active again. */
async function split(app) {
    await panes(app).openBeside({ bookId: PSALMS, chapter: 23, translation: 'kjv' });
    await settle();
    await app.key('F6');
    assert.deepEqual([...panes(app).shown()], ['first', 'second']);
    assert.equal(panes(app).active(), 'first');
}

test('B bookmarks the selected verse, and B again removes it, as ★ Bookmark does', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await select(app, 16);
        let event = await press(app, 'b');
        assert.ok(event.defaultPrevented);
        assert.deepEqual(app.bookmarkedVerseNumbers(), [16]);
        assert.equal(app.bookmarkButton().textContent, '★ Remove bookmark');
        assert.equal((await backend('list_bookmarks')).length, 1);

        // Caps Lock's B.
        event = await press(app, 'B');
        assert.ok(event.defaultPrevented);
        assert.deepEqual(app.bookmarkedVerseNumbers(), []);
        assert.equal(app.bookmarkButton().textContent, '★ Bookmark');
        assert.deepEqual(await backend('list_bookmarks'), []);
    }));

test('H steps the highlight yellow, green, blue, pink, then off, the swatches following', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await select(app, 16);
        for (const color of COLORS) {
            const event = await press(app, 'h');
            assert.ok(event.defaultPrevented, color);
            assert.equal(marker(app, 16), color);
            assert.deepEqual(pressed(app), [color]);
        }
        const event = await press(app, 'H');
        assert.ok(event.defaultPrevented);
        assert.equal(marker(app, 16), null);
        assert.deepEqual(pressed(app), []);
        assert.deepEqual(await backend('list_highlights'), []);
    }));

test('H on verses in mixed colors highlights them all yellow', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await select(app, 16);
        await press(app, 'h');
        await press(app, 'h');
        assert.equal(marker(app, 16), 'green');
        // 16 green, 17 none: mixed.
        await app.key('Escape');
        assert.deepEqual(app.selectedVerseNumbers(), []);
        await select(app, 16, 17);
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17]);
        await press(app, 'h');
        assert.deepEqual([marker(app, 16), marker(app, 17)], ['yellow', 'yellow']);
        assert.deepEqual(pressed(app), ['yellow']);
    }));

test('with no selection, B and H change nothing and the key goes on', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        assert.deepEqual(app.selectedVerseNumbers(), []);
        const before = app.calls.length;
        for (const key of ['b', 'B', 'h', 'H']) {
            const event = await app.key(key);
            assert.equal(event.defaultPrevented, false, key);
        }
        await app.idle(before);
        assert.equal(app.calls.length, before, 'nothing asked of the backend');
        assert.deepEqual(app.bookmarkedVerseNumbers(), []);
        assert.equal(app.document.querySelector('.chapter-main .verse[data-highlight]'), null);
    }));

test('X closes the second pane while split, and does nothing with one', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        let event = await app.key('x');
        assert.equal(event.defaultPrevented, false, 'one pane: the key goes on');
        assert.deepEqual([...panes(app).shown()], ['first']);

        await split(app);
        // From the second pane, active, as × closes it from either.
        await app.key('F6');
        assert.equal(panes(app).active(), 'second');
        event = await app.key('X', {}, app.document.activeElement ?? app.document.body);
        await settle();
        assert.ok(event.defaultPrevented);
        assert.deepEqual([...panes(app).shown()], ['first']);
        assert.equal(panes(app).active(), 'first');
        assert.equal(app.heading(), 'John 3');
        assert.equal(app.document.activeElement, el(app, 'ref-btn'), 'focus as × leaves it');
    }));

test('P switches the active pane as F6 does, never the passages; with one pane it does nothing', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        let event = await app.key('p');
        assert.equal(event.defaultPrevented, false, 'one pane: the key goes on');

        await split(app);
        const column = (name) => app.document.querySelector(`.pane[data-pane="${name}"]`);
        const reference = (name) => column(name).querySelector('.pane-bar .ref-btn');
        const headings = () => [...app.document.querySelectorAll('.chapter-heading')].map((h) => h.textContent);
        const passages = headings();
        for (const [key, to] of [['p', 'second'], ['P', 'first'], ['p', 'second']]) {
            event = await app.key(key, {}, app.document.activeElement ?? app.document.body);
            assert.ok(event.defaultPrevented, key);
            assert.equal(panes(app).active(), to, key);
            assert.equal(app.document.activeElement, reference(to), `${key}: focus as F6 leaves it`);
            assert.deepEqual(headings(), passages, 'the passages stay where they are');
        }
        // Ctrl+P is the webview's print, not P.
        event = await app.key('p', { ctrlKey: true });
        assert.equal(event.defaultPrevented, false);
        assert.equal(panes(app).active(), 'second');
    }));

test('b, h, x and p typed in the search box or the note editor are letters', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await split(app);
        await select(app, 16);
        const keys = ['b', 'h', 'x', 'p', 'B', 'H', 'X', 'P'];

        await app.key('/');
        for (const key of keys) {
            const event = await app.key(key, {}, app.searchInput());
            assert.equal(event.defaultPrevented, false, `${key} types`);
        }
        await app.key('Escape', {}, app.searchInput());
        assert.equal(app.searchPanel().hidden, true);

        await app.key('n');
        assert.ok(notePanelOpen(app));
        const input = el(app, 'note-input');
        for (const key of keys) {
            const event = await app.key(key, {}, input);
            assert.equal(event.defaultPrevented, false, `${key} types`);
        }
        assert.ok(notePanelOpen(app));
        await app.key('Escape', {}, input);
        await app.idle();

        assert.deepEqual([...panes(app).shown()], ['first', 'second'], 'X closed nothing');
        assert.equal(panes(app).active(), 'first', 'P switched nothing');
        assert.deepEqual(app.bookmarkedVerseNumbers(), []);
        assert.equal(marker(app, 16), null);
    }));

test('with Single-key shortcuts off, B, H, X and P do nothing, split or not', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        app.open('aa');
        await app.click(el(app, 'single-keys-toggle'));
        assert.equal(app.document.documentElement.dataset.singleKeyShortcuts, 'off');
        await app.key('Escape', {}, app.document.activeElement);
        app.document.activeElement?.blur();
        await split(app);
        await select(app, 16);
        for (const key of ['b', 'h', 'x', 'p', 'B', 'H', 'X', 'P']) {
            const event = await press(app, key);
            assert.equal(event.defaultPrevented, false, key);
        }
        assert.deepEqual([...panes(app).shown()], ['first', 'second']);
        assert.equal(panes(app).active(), 'first');
        assert.deepEqual(app.bookmarkedVerseNumbers(), []);
        assert.equal(marker(app, 16), null);
        // F6 still switches: it's no character.
        await app.key('F6');
        assert.equal(panes(app).active(), 'second');
    }));

test('★ Bookmark and the swatches name their keys as + Add note does, with no key shown on the card', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await select(app, 16);
        const bookmark = app.bookmarkButton();
        assert.equal(bookmark.title, 'Bookmark (B)');
        assert.equal(bookmark.getAttribute('aria-keyshortcuts'), 'B');
        assert.equal(bookmark.textContent, '★ Bookmark');
        const note = app.actions().querySelector('.verse-note-btn');
        assert.equal(note.title, 'Add a note (N)');
        // H picks no one color: the group has the key, each swatch says
        // it's the next color.
        const group = app.actions().querySelector('.verse-highlight');
        assert.equal(group.getAttribute('aria-keyshortcuts'), 'H');
        for (const color of COLORS) {
            const b = swatch(app, color);
            assert.equal(b.title, `Highlight ${color} (H: the next color)`);
            assert.equal(b.getAttribute('aria-label'), `Highlight ${color}`);
            assert.equal(b.getAttribute('aria-keyshortcuts'), null);
        }
        assert.doesNotMatch(app.actions().textContent, /\((B|H)\)/);
        // Bookmarked, the title is the same.
        await press(app, 'b');
        assert.equal(bookmark.title, 'Bookmark (B)');
    }));
