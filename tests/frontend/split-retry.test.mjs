// Try again in the second pane (GG32-03; ui-leptos/src/pane.rs's `make`,
// reader.rs's `listen_to_notice`): a chapter the second pane couldn't read
// says so with Try again, and one press asks for it once, and shows it when
// it comes; after the pane is closed and opened again too, and when the
// panes were split at launch. The first pane's chapter and the place the
// backend keeps don't change. The failure is injected the way the audit did,
// a get_reading that rejects.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, closeBridge, openApp, withApp } from './harness.mjs';

after(closeBridge);

const ROMANS = bookId('Romans');
const JOHN = bookId('John');

const panes = (app) => app.window.gospelGetterPanes;
const root = (app, n) => app.document.getElementById(n === 1 ? 'reading-pane' : 'reading-pane-2');
const heading = (app, n) => root(app, n)?.querySelector('.chapter-heading')?.textContent;
const notice = (app) => app.document.getElementById('reader-notice-2');
const readings = (app, since = 0) => app.calls.slice(since).filter((c) => c.cmd === 'get_reading')
    .map((c) => JSON.parse(JSON.stringify(c.args)));
const place = async () => {
    const home = await backend('get_home');
    return [home.currentBookId, home.currentChapter];
};

/** The second pane's Try again: shown, then pressed. Returns the get_reading calls it made. */
async function tryAgain(app) {
    assert.ok(!notice(app).hidden, 'the second pane says it couldn’t read the chapter');
    const retry = notice(app).querySelector('.reading-retry');
    assert.ok(retry, 'with Try again');
    assert.equal(retry.textContent, 'Try again');
    const before = app.calls.length;
    await app.click(retry);
    await app.idle(before);
    return readings(app, before);
}

/** Opens Romans 5 beside while every get_reading fails, then lets them through. */
async function openBesideFailing(app) {
    app.failCommand('get_reading');
    const before = app.calls.length;
    await panes(app).openBeside({ bookId: ROMANS, chapter: 5, translation: 'bsb' });
    await app.idle(before);
    app.failCommand('get_reading', null);
}

test('Try again in the second pane asks once and shows the chapter, after a close and reopen too, the first pane untouched', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        const where = await place();
        assert.deepEqual(where, [JOHN, 3]);
        for (const round of ['first open', 'reopened']) {
            await openBesideFailing(app);
            assert.deepEqual([...panes(app).shown()], ['first', 'second'], round);
            assert.notEqual(heading(app, 2), 'Romans 5', round);
            assert.deepEqual(
                await tryAgain(app),
                [{ bookId: ROMANS, chapter: 5, translationCode: 'bsb' }],
                `${round}: one press, one request`,
            );
            assert.equal(heading(app, 2), 'Romans 5', round);
            assert.ok(notice(app).hidden, `${round}: the notice goes`);
            assert.equal(heading(app, 1), 'John 3', `${round}: the first pane’s chapter`);
            assert.deepEqual(await place(), where, `${round}: the backend’s place`);
            const close = app.document.querySelector('.pane[data-pane="second"] .pane-close');
            await app.click(close);
            await app.idle();
            assert.deepEqual([...panes(app).shown()], ['first'], round);
        }
    }));

test('Try again in a second pane opened at launch asks once', async () => {
    let failRomans = true;
    const app = await openApp({
        book: 'John',
        chapter: 3,
        translation: 'kjv',
        storage: {
            'gospel-getter-split': 'on',
            'gospel-getter-second-pane': JSON.stringify({ bookId: ROMANS, chapter: 5, translation: 'bsb' }),
        },
        alterReading: (dto) => {
            if (failRomans && dto.currentBookId === ROMANS) throw 'simulated get_reading failure';
            return dto;
        },
    });
    try {
        await app.idle();
        assert.deepEqual([...panes(app).shown()], ['first', 'second']);
        assert.equal(heading(app, 1), 'John 3');
        failRomans = false;
        assert.deepEqual(await tryAgain(app), [{ bookId: ROMANS, chapter: 5, translationCode: 'bsb' }]);
        assert.equal(heading(app, 2), 'Romans 5');
        assert.equal(heading(app, 1), 'John 3');
        assert.deepEqual(await place(), [JOHN, 3]);
    } finally {
        app.close();
    }
});
