// The reading position the app reopens at is the chapter the page put on
// screen last, in the translation it showed it in: never one it asked for
// and then dropped, however the answers arrive. Asked through get_home,
// as the next launch asks. (Which chapter is shown when answers arrive
// out of order is in races.test.mjs and reading-flow.test.mjs.)

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, closeBridge, openApp } from './harness.mjs';

after(closeBridge);

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

/** Where the next launch opens: [translation, book, chapter]. */
async function saved() {
    const home = await backend('get_home');
    return [home.currentTranslationCode, home.currentBookId, home.currentChapter];
}

/** What's on screen: [translation, book, chapter]. */
function shown(app) {
    const inner = app.document.getElementById('reading-pane-inner');
    return [app.verse(1).dataset.translation, Number(inner.dataset.currentBook), Number(inner.dataset.currentChapter)];
}

/** Waits until every call so far has finished, then for the page to be idle. */
async function allDone(app) {
    await app.until(() => app.calls.every((c) => c.done), 'every call');
    await app.idle(0);
}

function chooseTranslation(app, code) {
    const select = app.document.getElementById('translation-select');
    select.value = code;
    select.dispatchEvent(new app.window.Event('change', { bubbles: true }));
}

const JOHN = bookId('John');
const ACTS = bookId('Acts');
const FAILED = "Acts 1 couldn't be loaded.";

test('a translation switch overtaken by another: the place saved is the one shown', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        // The WEB's chapter is held; let go, it reaches the backend only
        // after the BSB's, asked for at once.
        const release = app.holdCommand('get_reading');
        chooseTranslation(app, 'web');
        await app.until(() => app.calls.filter((c) => c.cmd === 'get_reading').length === 2, 'the WEB chapter');
        release();
        chooseTranslation(app, 'bsb');
        await allDone(app);
        assert.equal(app.document.getElementById('translation-select').value, 'bsb');
        assert.deepEqual(shown(app), ['bsb', JOHN, 3]);
        assert.deepEqual(await saved(), ['bsb', JOHN, 3]);
    }));

test('a chapter turn overtaken by a jump: the place saved is the one shown', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        const release = app.holdCommand('get_reading');
        app.key('ArrowRight');
        await app.until(() => app.calls.filter((c) => c.cmd === 'get_reading').length === 2, 'John 4');
        release();
        app.key(']');
        await allDone(app);
        assert.equal(app.heading(), 'Acts 1');
        assert.deepEqual(shown(app), ['kjv', ACTS, 1]);
        assert.deepEqual(await saved(), ['kjv', ACTS, 1]);
    }));

test('an older chapter that arrives after a newer one failed moves nothing; Try again then saves the newer one', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        const release = app.holdCommand('get_reading');
        app.key('ArrowRight');
        await app.until(() => app.calls.filter((c) => c.cmd === 'get_reading').length === 2, 'John 4');
        // The jump fails at once; then John 4, asked for first, arrives.
        app.failCommand('get_reading', FAILED);
        app.key(']');
        await app.until(() => app.notice() === `${FAILED} Try again`, 'the failure');
        app.failCommand('get_reading', null);
        release();
        await allDone(app);
        assert.equal(app.heading(), 'John 3', 'the dropped answer isn’t shown');
        assert.equal(app.notice(), `${FAILED} Try again`);
        assert.deepEqual(await saved(), ['kjv', JOHN, 3], 'nor saved');

        const before = app.calls.length;
        app.click(app.document.querySelector('#reader-notice .reading-retry'));
        await app.idle(before, 'get_reading');
        assert.equal(app.heading(), 'Acts 1');
        assert.deepEqual(await saved(), ['kjv', ACTS, 1]);
    }));

test('a chapter that fails late, after a newer one was shown, changes neither', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        const fail = app.holdFailure('get_reading', "John 4 couldn't be loaded.");
        app.key('ArrowRight');
        app.key(']');
        await app.waitForHeading('Acts 1');
        fail();
        await allDone(app);
        assert.equal(app.heading(), 'Acts 1');
        assert.equal(app.notice(), '');
        assert.deepEqual(await saved(), ['kjv', ACTS, 1]);
    }));

test('every way of reading a chapter saves it once it’s shown: startup, turns, Back and Forward, compare', () =>
    withApp({ fresh: true, translation: 'kjv', storage: { 'gospel-getter-welcome': 'seen' } }, async (app) => {
        // A new install opens at Genesis 1, which is then the saved place
        // (the welcome, seen already, stays shut).
        assert.equal(app.heading(), 'Genesis 1');
        assert.deepEqual(await saved(), ['kjv', 1, 1]);
        assert.equal((await backend('get_home')).hasSavedPosition, true);

        let before = app.calls.length;
        app.key('ArrowRight');
        await app.idle(before, 'get_reading');
        assert.deepEqual(await saved(), ['kjv', 1, 2]);

        await app.search('John 3');
        before = app.calls.length;
        app.key('Enter', {}, app.searchInput());
        await app.idle(before, 'get_reading');
        assert.deepEqual(await saved(), ['kjv', JOHN, 3]);
        before = app.calls.length;
        app.key('ArrowLeft', { altKey: true });
        await app.idle(before, 'get_reading');
        assert.equal(app.heading(), 'Genesis 2');
        assert.deepEqual(await saved(), ['kjv', 1, 2]);
        before = app.calls.length;
        app.key('ArrowRight', { altKey: true });
        await app.idle(before, 'get_reading');
        assert.deepEqual(await saved(), ['kjv', JOHN, 3]);

        // Compare reads a second translation beside the first, but the
        // place saved stays the reader's own translation.
        before = app.calls.length;
        app.key('c');
        await app.idle(before);
        assert.ok(app.isComparing());
        assert.deepEqual(await saved(), ['kjv', JOHN, 3]);
        before = app.calls.length;
        app.key('ArrowRight');
        await app.idle(before, 'get_reading');
        assert.equal(app.heading(), 'John 4');
        assert.deepEqual(await saved(), ['kjv', JOHN, 4]);
        assert.ok(app.calls.slice(before).every((c) => c.cmd !== 'save_reading_position' || c.args.translationCode === 'kjv'));
    }));

test('the saves are sent one at a time, in the order the chapters were shown', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        const saves = () => app.calls.filter((c) => c.cmd === 'save_reading_position');
        const already = saves().length;
        const release = app.holdCommand('save_reading_position');
        for (const _ of [4, 5, 6]) {
            const before = app.calls.length;
            app.key('ArrowRight');
            await app.until(() => app.calls.slice(before).some((c) => c.cmd === 'get_reading' && c.done), 'the chapter');
        }
        await app.waitForHeading('John 6');
        // John 4's save is on its way; John 5's was overtaken by John 6's
        // before it went, so only John 6's waits.
        assert.deepEqual(saves().slice(already).map((c) => c.args.chapter), [4]);
        release();
        await allDone(app);
        assert.deepEqual(saves().slice(already).map((c) => c.args.chapter), [4, 6]);
        assert.deepEqual(await saved(), ['kjv', JOHN, 6]);
    }));
