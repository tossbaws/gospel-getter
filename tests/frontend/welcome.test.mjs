// The welcome: one card over the open chapter, with a look to pick (Chi
// Rho or Vellum) and places to start reading. It opens by itself only on
// a fresh install (never for anyone who saw the earlier tour), closes for
// good by any route (Skip, ×, Escape, a place to start), keeps the
// keyboard to itself while open, and can be reopened from the ☰ menu —
// against the real backend and database.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, closeBridge, openApp, settle } from './harness.mjs';

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
const overlay = (app) => $(app, 'welcome-overlay');
const panel = (app) => $(app, 'welcome-panel');
const isOpen = (app) => !overlay(app).hidden;
const looks = (app) => [...app.document.querySelectorAll('#welcome-looks [role="radio"]')];
const checked = (app) => looks(app).filter((r) => r.getAttribute('aria-checked') === 'true').map((r) => r.dataset.value);
const rows = (app) => [...app.document.querySelectorAll('#welcome-start .welcome-start-row')];

const START_HERE = [
    ['Mark', 'The shortest Gospel, 16 chapters. A good first read.', 'Mark 1'],
    ['John', 'Who Jesus is, in his own words.', 'John 1'],
    ['Psalms', 'Prayers for every mood. Read one a day.', 'Psalms 1'],
    ['Genesis 1', 'Start at the beginning.', 'Genesis 1'],
];

// Boot has finished (the harness waits for the bookmark list); give the
// last step, which decides on the welcome, a moment.
async function booted(app) {
    await app.idle();
    await settle();
}

/** Opens the app as a fresh install and waits for the welcome. */
async function freshApp(options = {}) {
    const app = await openApp({ fresh: true, ...options });
    await app.until(() => isOpen(app), 'the welcome');
    return app;
}

async function withFreshApp(options, body) {
    const app = await freshApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

test('a fresh install opens the welcome card over the first chapter', () =>
    withFreshApp({}, async (app) => {
        assert.equal(app.heading(), 'Genesis 1', 'the chapter is open behind it');
        assert.equal(panel(app).getAttribute('role'), 'dialog');
        assert.equal(panel(app).getAttribute('aria-modal'), 'true');
        assert.equal(panel(app).getAttribute('aria-labelledby'), 'welcome-title');
        assert.equal(panel(app).getAttribute('aria-describedby'), 'welcome-intro');
        assert.equal($(app, 'welcome-title').textContent, 'Welcome to Gospel Getter');
        assert.equal($(app, 'welcome-intro').textContent, 'A quiet place to read the Bible on your computer, offline and with no account.');
        assert.equal(app.document.activeElement, panel(app), 'focus moves into the card');

        // Pick a look: Chi Rho (the default, checked) and Vellum.
        const group = $(app, 'welcome-looks');
        assert.equal(group.getAttribute('role'), 'radiogroup');
        assert.equal($(app, group.getAttribute('aria-labelledby')).textContent, 'Pick a look');
        // Named by their theme (the preview is hidden from assistive tech).
        assert.deepEqual(looks(app).map((r) => r.querySelector('.welcome-look-name').textContent), ['Chi Rho', 'Vellum']);
        assert.ok(looks(app).every((r) => r.querySelector('.welcome-look-preview').getAttribute('aria-hidden') === 'true'));
        assert.deepEqual(looks(app).map((r) => r.children.length), [2, 2]);
        assert.deepEqual(checked(app), ['chi-rho']);
        assert.deepEqual(looks(app).map((r) => r.tabIndex), [0, -1]);
        for (const radio of looks(app)) {
            assert.equal(radio.querySelector('[data-theme-preview]').dataset.themePreview, radio.dataset.value);
        }
        assert.equal(app.document.querySelector('.welcome-hint').textContent, 'More themes in Aa.');

        // Where to start: the backend's list, word for word.
        const list = $(app, 'welcome-start');
        assert.equal($(app, list.getAttribute('aria-labelledby')).textContent, 'Where to start');
        assert.deepEqual(rows(app).map((r) => [
            r.querySelector('.welcome-start-title').textContent,
            r.querySelector('.welcome-start-line').textContent,
        ]), START_HERE.map(([title, line]) => [title, line]));
        assert.equal(rows(app)[0].textContent, 'Mark The shortest Gospel, 16 chapters. A good first read.');
        assert.equal($(app, 'welcome-skip').textContent, 'Skip');
        assert.equal($(app, 'welcome-close').getAttribute('aria-label'), 'Close');

        // No tour left: no pages, no Next or Back, no key lists.
        assert.equal(app.document.querySelector('.welcome-page, #welcome-next, #welcome-back, #welcome-progress'), null);
        assert.equal(panel(app).querySelector('kbd'), null);
        assert.deepEqual(app.consoleErrors, []);
    }));

test('an existing reading position means no welcome', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await booted(app);
        assert.equal(isOpen(app), false);
        assert.equal(app.heading(), 'John 3');
    }));

test('anyone who saw the earlier tour never gets the card, even with nothing else saved', () =>
    withApp({ fresh: true, storage: { 'gospel-getter-welcome': 'seen' } }, async (app) => {
        await booted(app);
        assert.equal(isOpen(app), false);
    }));

test('existing bookmarks mean no welcome, even with no reading position', async () => {
    await backend('__reset', {});
    await backend('add_bookmark', { bookId: bookId('John'), chapter: 3, verseStart: 16, verseEnd: 16 });
    await withApp({ fresh: true, keepBookmarks: true }, async (app) => {
        await booted(app);
        assert.equal(app.bookmarkItems().length, 1);
        assert.equal(isOpen(app), false);
    });
});

test('saved display settings mean no welcome, even with nothing else saved', async () => {
    for (const [key, value] of [
        ['gospel-getter-theme', 'matrix'],
        ['gospel-getter-text-size', 'large'],
        ['gospel-getter-compare', 'off'],
        ['gospel-getter-layout', 'flowing'],
    ]) {
        await withApp({ fresh: true, storage: { [key]: value } }, async (app) => {
            await booted(app);
            assert.equal(isOpen(app), false, `with ${key} saved`);
        });
    }
});

test('if the bookmarks can\'t be loaded, the welcome doesn\'t open', () =>
    withApp({ fresh: true, failing: { list_bookmarks: 'simulated failure' } }, async (app) => {
        await app.until(() => app.document.querySelector('#bookmark-list li'), 'bookmark list');
        await booted(app);
        assert.equal(isOpen(app), false);
    }));

for (const [route, dismiss] of [
    ['Skip', (app) => app.click($(app, 'welcome-skip'))],
    ['×', (app) => app.click($(app, 'welcome-close'))],
    ['Escape', (app) => app.key('Escape', {}, app.document.activeElement)],
]) {
    test(`${route} closes the welcome where the reader is, and it stays closed after a restart`, async () => {
        let storage;
        await withFreshApp({}, async (app) => {
            const before = app.calls.length;
            dismiss(app);
            await settle();
            assert.equal(isOpen(app), false);
            assert.ok(!overlay(app).contains(app.document.activeElement), 'focus left inside the closed welcome');
            assert.equal(app.heading(), 'Genesis 1', 'the reader stays where they are');
            assert.deepEqual(app.calls.slice(before).map((c) => c.cmd), []);
            storage = app.storage();
            assert.equal(storage['gospel-getter-welcome'], 'seen');
        });
        // A restart on the same (still otherwise empty) install.
        await withApp({ fresh: true, storage }, async (app) => {
            await booted(app);
            assert.equal(isOpen(app), false);
        });
    });
}

for (const [title, , heading] of START_HERE) {
    test(`“${title}” opens ${heading} and closes the card`, () =>
        withFreshApp({}, async (app) => {
            const row = rows(app).find((r) => r.querySelector('.welcome-start-title').textContent === title);
            const before = app.calls.length;
            await app.click(row);
            assert.equal(isOpen(app), false);
            await app.idle(before, 'get_reading');
            assert.equal(app.heading(), heading);
            assert.equal(app.storage()['gospel-getter-welcome'], 'seen');
            assert.ok(!overlay(app).contains(app.document.activeElement));
        }));
}

test('a look applies its theme live, as Aa does, and the arrow keys move between the two', () =>
    withFreshApp({}, async (app) => {
        const root = app.document.documentElement;
        const [chiRho, vellum] = looks(app);
        await app.click(vellum);
        assert.equal(root.dataset.theme, 'vellum');
        assert.equal(app.storage()['gospel-getter-theme'], 'vellum');
        assert.equal($(app, 'theme-select').value, 'vellum', 'the record Aa and the export read');
        assert.deepEqual(checked(app), ['vellum']);
        assert.deepEqual(looks(app).map((r) => r.tabIndex), [-1, 0]);
        assert.equal(app.document.activeElement, vellum);
        assert.ok(isOpen(app), 'choosing a look keeps the card open');

        // ← and → (↑ and ↓ too) move and choose, wrapping.
        await app.key('ArrowRight', {}, vellum);
        assert.equal(app.document.activeElement, chiRho);
        assert.deepEqual(checked(app), ['chi-rho']);
        assert.equal(root.dataset.theme, undefined, 'Chi Rho is the default: no attribute');
        await app.key('ArrowUp', {}, chiRho);
        assert.deepEqual(checked(app), ['vellum']);
        await app.key('ArrowDown', {}, vellum);
        assert.deepEqual(checked(app), ['chi-rho']);

        // Aa agrees.
        app.click($(app, 'welcome-skip'));
        app.open('aa');
        const pressed = [...$(app, 'theme-choices').querySelectorAll('[aria-pressed="true"]')].map((b) => b.dataset.value);
        assert.deepEqual(pressed, ['chi-rho']);
    }));

test('with another theme in use, neither look is checked, and the first is the tab stop', () =>
    withApp({ storage: { 'gospel-getter-welcome': 'seen', 'gospel-getter-theme': 'matrix' } }, async (app) => {
        await booted(app);
        app.open('menu');
        app.click($(app, 'welcome-btn'));
        assert.deepEqual(checked(app), []);
        assert.deepEqual(looks(app).map((r) => r.tabIndex), [0, -1]);
        assert.equal(app.document.documentElement.dataset.theme, 'matrix', 'opening it changes nothing');
    }));

test('Tab stays inside the card, through ×, the look, the places to start and Skip', () =>
    withFreshApp({}, async (app) => {
        const tab = (shiftKey = false) => app.key('Tab', { shiftKey }, app.document.activeElement);
        // From the card itself, Tab's default reaches × first; Shift+Tab
        // wraps to Skip.
        tab(true);
        assert.equal(app.document.activeElement, $(app, 'welcome-skip'));
        const stops = [$(app, 'welcome-close'), looks(app)[0], ...rows(app), $(app, 'welcome-skip')];
        // Forward from Skip wraps to ×; backward from × wraps to Skip.
        tab();
        assert.equal(app.document.activeElement, stops[0]);
        tab(true);
        assert.equal(app.document.activeElement, stops.at(-1));
        // Only the checked look is a stop.
        assert.deepEqual(stops.map((el) => el.tabIndex >= 0), stops.map(() => true));
        assert.equal(looks(app)[1].tabIndex, -1);

        // Focus that somehow got out (say, onto the page) comes back in.
        app.document.activeElement.blur();
        tab();
        assert.equal(app.document.activeElement, stops[0]);
        app.document.activeElement.blur();
        tab(true);
        assert.equal(app.document.activeElement, stops.at(-1));
    }));

test('the reader\'s shortcuts don\'t run while the welcome is open', () =>
    withFreshApp({}, async (app) => {
        const before = app.calls.length;
        for (const [key, opts] of [['/', {}], ['k', { ctrlKey: true }], ['c', {}], ['C', {}], ['g', {}], ['ArrowRight', {}], ['ArrowLeft', {}]]) {
            app.key(key, opts, app.document.activeElement);
            app.key(key, opts, app.document.body);
        }
        await settle();
        assert.equal(app.searchPanel().hidden, true);
        assert.equal(app.isComparing(), false);
        assert.equal(app.storage()['gospel-getter-compare'], undefined);
        assert.equal(app.heading(), 'Genesis 1');
        assert.deepEqual(app.calls.slice(before).map((c) => c.cmd), []);
        assert.ok(panel(app).contains(app.document.activeElement));

        // Escape closes only the welcome; then the shortcuts work again.
        await app.key('Escape', {}, app.document.body);
        assert.equal(isOpen(app), false);
        app.key('/');
        assert.equal(app.searchPanel().hidden, false);
    }));

test('Show welcome in the ☰ menu opens the card, and focus returns to ☰', () =>
    withApp({ storage: { 'gospel-getter-welcome': 'seen' } }, async (app) => {
        await booted(app);
        assert.equal(isOpen(app), false);
        app.open('menu');
        const show = $(app, 'welcome-btn');
        assert.equal(show.textContent, 'Show welcome');
        await app.click(show);
        assert.equal(isOpen(app), true);
        assert.equal(app.surface('menu').hidden, true);
        assert.equal(app.document.activeElement, panel(app));

        await app.click($(app, 'welcome-close'));
        assert.equal(isOpen(app), false);
        assert.equal(app.document.activeElement, $(app, 'menu-toggle'));

        // Escape returns focus to ☰ as well, and the reader stays put.
        app.open('menu');
        await app.click(show);
        await app.key('Escape', {}, app.document.activeElement);
        assert.equal(isOpen(app), false);
        assert.equal(app.document.activeElement, $(app, 'menu-toggle'));
        assert.equal(app.surface('menu').hidden, true);
        assert.equal(app.heading(), 'John 3');

        // A place to start, from the menu, reads it.
        app.open('menu');
        await app.click(show);
        const before = app.calls.length;
        await app.click(rows(app)[2]);
        await app.idle(before, 'get_reading');
        assert.equal(app.heading(), 'Psalms 1');
        assert.equal(isOpen(app), false);
    }));

test('with no places to start, the card leaves out that section', () =>
    withApp({ fresh: true, alter: { get_home: (home) => ({ ...home, startHere: [] }) } }, async (app) => {
        await app.until(() => isOpen(app), 'the welcome');
        assert.equal($(app, 'welcome-start').hidden, true);
        assert.equal($(app, 'welcome-start-label').hidden, true);
        assert.equal(looks(app).length, 2, 'the looks are still there');
    }));

test('the places to start are text, never markup', () =>
    withApp({
        fresh: true,
        alter: { get_home: (home) => ({ ...home, startHere: [{ bookId: 41, chapter: 1, title: '<b>Mark</b>', line: '<img src=x onerror=alert(1)>' }] }) },
    }, async (app) => {
        await app.until(() => isOpen(app), 'the welcome');
        const row = rows(app)[0];
        assert.equal(row.querySelector('.welcome-start-title').textContent, '<b>Mark</b>');
        assert.equal(row.querySelector('img, b'), null);
    }));
