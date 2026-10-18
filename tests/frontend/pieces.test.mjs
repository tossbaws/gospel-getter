// The page's Rust pieces (ui-leptos) arriving late or not at all: a fresh
// install whose pieces load after boot still gets its welcome, and without
// them each piece says it isn't available while the rest of the app keeps
// working — against the real backend and database.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { closeBridge, openApp, settle } from './harness.mjs';

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
