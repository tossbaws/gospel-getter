// The first-run welcome: a four-page tour that opens by itself only on a
// fresh install, closes for good by any route (Skip, Start reading,
// Escape), keeps the keyboard to itself while open, and can be reopened
// from Settings — against the real backend and database.

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

const overlay = (app) => app.document.getElementById('welcome-overlay');
const panel = (app) => app.document.getElementById('welcome-panel');
const button = (app, name) => app.document.getElementById(`welcome-${name}`);
const isOpen = (app) => !overlay(app).hidden;
const progress = (app) => app.document.getElementById('welcome-progress').textContent;
const pageTitle = (app) => [...app.document.querySelectorAll('.welcome-page')]
    .find((page) => !page.hidden).querySelector('h2').textContent;

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

test('a fresh install opens the welcome on its first page, over the reader', () =>
    withFreshApp({}, async (app) => {
        assert.equal(app.heading(), 'Genesis 1');
        assert.equal(panel(app).getAttribute('role'), 'dialog');
        assert.equal(panel(app).getAttribute('aria-modal'), 'true');
        assert.equal(panel(app).getAttribute('aria-labelledby'), 'welcome-title-1');
        assert.equal(panel(app).getAttribute('aria-describedby'), 'welcome-text-1');
        assert.equal(app.document.getElementById('welcome-title-1').textContent, 'Welcome to Gospel Getter');
        assert.equal(progress(app), '1 of 4');
        assert.equal(button(app, 'next').textContent, 'Next');
        assert.equal(button(app, 'back').disabled, true);
        assert.equal(button(app, 'skip').textContent, 'Skip');
        assert.equal(app.document.activeElement, button(app, 'next'));
        assert.deepEqual(app.consoleErrors, []);
    }));

test('an existing reading position means no welcome', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await booted(app);
        assert.equal(isOpen(app), false);
        assert.equal(app.heading(), 'John 3');
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
    ['Skip', (app) => app.click(button(app, 'skip'))],
    ['Start reading', (app) => {
        for (let i = 0; i < 3; i++) app.click(button(app, 'next'));
        assert.equal(button(app, 'next').textContent, 'Start reading');
        app.click(button(app, 'next'));
    }],
    ['Escape', (app) => app.key('Escape', {}, app.document.activeElement)],
]) {
    test(`${route} closes the welcome, and it stays closed after a restart`, async () => {
        let storage;
        await withFreshApp({}, async (app) => {
            dismiss(app);
            assert.equal(isOpen(app), false);
            assert.ok(!overlay(app).contains(app.document.activeElement), 'focus left inside the closed welcome');
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

test('Next, Back and the arrow keys move through the four pages', () =>
    withFreshApp({}, async (app) => {
        const titles = ['Welcome to Gospel Getter', 'Read in context', 'Find and keep', 'Make it yours'];
        app.click(button(app, 'next'));
        assert.equal(progress(app), '2 of 4');
        assert.equal(pageTitle(app), titles[1]);
        assert.equal(panel(app).getAttribute('aria-labelledby'), 'welcome-title-2');
        assert.equal(panel(app).getAttribute('aria-describedby'), 'welcome-text-2');
        assert.equal(button(app, 'back').disabled, false);

        app.click(button(app, 'back'));
        assert.equal(progress(app), '1 of 4');
        assert.equal(pageTitle(app), titles[0]);
        assert.equal(button(app, 'back').disabled, true);

        for (let i = 2; i <= 4; i++) {
            app.key('ArrowRight', {}, app.document.activeElement);
            assert.equal(progress(app), `${i} of 4`);
            assert.equal(pageTitle(app), titles[i - 1]);
        }
        assert.equal(button(app, 'next').textContent, 'Start reading');
        // Past the last page, → does nothing (it doesn't close the tour).
        app.key('ArrowRight', {}, app.document.activeElement);
        assert.equal(progress(app), '4 of 4');
        assert.equal(isOpen(app), true);

        app.key('ArrowLeft', {}, app.document.activeElement);
        assert.equal(progress(app), '3 of 4');
        assert.equal(button(app, 'next').textContent, 'Next');
        // The reader underneath never moved.
        assert.equal(app.heading(), 'Genesis 1');
    }));

test('Tab stays inside the welcome', () =>
    withFreshApp({}, async (app) => {
        const tab = (shiftKey = false) => app.key('Tab', { shiftKey }, app.document.activeElement);
        // Page 1: Back is hidden, so Next and Skip are the only stops.
        button(app, 'skip').focus();
        tab();
        assert.equal(app.document.activeElement, button(app, 'next'));
        tab(true);
        assert.equal(app.document.activeElement, button(app, 'skip'));

        app.click(button(app, 'next'));
        button(app, 'skip').focus();
        tab();
        assert.equal(app.document.activeElement, button(app, 'back'));
        tab(true);
        assert.equal(app.document.activeElement, button(app, 'skip'));

        // Focus that somehow got out (say, onto the page) comes back in.
        app.document.activeElement.blur();
        tab();
        assert.equal(app.document.activeElement, button(app, 'back'));

        // Going back to page 1 from Back moves focus off the hidden button.
        app.click(button(app, 'back'));
        button(app, 'back').focus();
        app.key('ArrowLeft', {}, app.document.activeElement);
        assert.notEqual(app.document.activeElement, button(app, 'back'));
    }));

test('the reader\'s shortcuts don\'t run while the welcome is open', () =>
    withFreshApp({}, async (app) => {
        const before = app.calls.length;
        for (const [key, opts] of [['/', {}], ['k', { ctrlKey: true }], ['c', {}], ['C', {}], ['ArrowRight', {}], ['ArrowLeft', {}]]) {
            app.key(key, opts, app.document.activeElement);
            app.key(key, opts, app.document.body);
        }
        await settle();
        assert.equal(app.searchPanel().hidden, true);
        assert.equal(app.isComparing(), false);
        assert.equal(app.storage()['gospel-getter-compare'], undefined);
        assert.equal(app.heading(), 'Genesis 1');
        assert.deepEqual(app.calls.slice(before).map((c) => c.cmd), []);

        // Escape closes only the welcome; then the shortcuts work again.
        app.key('Escape', {}, app.document.body);
        assert.equal(isOpen(app), false);
        app.key('/');
        assert.equal(app.searchPanel().hidden, false);
    }));

test('Show welcome in Settings reopens it on page 1, and focus returns to Settings', () =>
    withApp({ storage: { 'gospel-getter-welcome': 'seen' } }, async (app) => {
        await booted(app);
        assert.equal(isOpen(app), false);
        app.openSettings();
        const show = app.document.getElementById('welcome-btn');
        assert.equal(show.textContent, 'Show welcome');
        app.click(show);
        assert.equal(isOpen(app), true);
        assert.equal(app.settingsMenu().hidden, true);
        assert.equal(progress(app), '1 of 4');
        assert.equal(app.document.activeElement, button(app, 'next'));

        app.click(button(app, 'next'));
        app.click(button(app, 'skip'));
        assert.equal(isOpen(app), false);
        assert.equal(app.document.activeElement, app.document.getElementById('settings-toggle'));

        // Opening it again starts from the first page.
        app.openSettings();
        app.click(show);
        assert.equal(progress(app), '1 of 4');
        app.key('Escape', {}, app.document.activeElement);
        assert.equal(isOpen(app), false);
        assert.equal(app.settingsMenu().hidden, true);
    }));
