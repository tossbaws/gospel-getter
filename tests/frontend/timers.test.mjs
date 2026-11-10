// AUD-06: the page's timers that are started again and again, and
// cancelled (the note draft's pause, the reading position's, the Library's
// note search, the toast's fading and the selection card's status line),
// each call one callback made once, so a timer cancelled before it fires
// leaves nothing behind; the timer that's left still fires, once, and the
// ones cancelled never do. Against the real backend and database.
//
// The page's `setTimeout` and `clearTimeout` are watched: each timer is
// kept with its callback, so the callbacks still waiting (never called)
// can be counted. A callback made for each timer, one-shot, is freed only
// when it's called: every cancelled timer would leave one behind.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, closeBridge, openApp, settle } from './harness.mjs';

after(closeBridge);

const JOHN = bookId('John');
/** How long each timer waits (`DRAFT_DELAY_MS`, `PLACE_SAVE_MS`, `SEARCH_DELAY_MS`, the toast's, `STATUS_MS`). */
const DRAFT_MS = 500;
const PLACE_MS = 400;
const NOTE_SEARCH_MS = 150;
const TOAST_MS = 6000;
const STATUS_MS = 2500;
const RESETS = 20;

/** Watches the window's timers (before the page's scripts run). */
function watchTimers(window) {
    const timers = [];
    const byHandle = new Map();
    const set = window.setTimeout;
    const clear = window.clearTimeout;
    window.setTimeout = function (callback, ms, ...args) {
        const timer = { callback, ms, ran: false, cleared: false };
        timers.push(timer);
        const handle = set.call(window, () => {
            timer.ran = true;
            if (typeof callback === 'function') callback(...args);
        }, ms);
        byHandle.set(handle, timer);
        return handle;
    };
    window.clearTimeout = function (handle) {
        const timer = byHandle.get(handle);
        if (timer && !timer.ran) timer.cleared = true;
        return clear.call(window, handle);
    };
    window.__timers = timers;
}

/** The timers of `ms` started after the first `since`: how many, the callbacks still waiting to be called, and how many ran. */
function timersOf(app, ms, since = 0) {
    const timers = app.window.__timers.slice(since).filter((t) => t.ms === ms);
    const called = new Set(app.window.__timers.filter((t) => t.ran).map((t) => t.callback));
    const waiting = new Set(timers.map((t) => t.callback).filter((c) => !called.has(c)));
    return { started: timers.length, waiting: waiting.size, ran: timers.filter((t) => t.ran).length, cancelledRan: timers.filter((t) => t.cleared && t.ran).length };
}

const started = (app) => app.window.__timers.length;
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function withApp(options, body) {
    const app = await openApp({ book: 'John', chapter: 3, keepBookmarks: true, setUp: watchTimers, ...options });
    try {
        await body(app);
    } finally {
        app.close();
    }
}

/** Checks `RESETS` resets of the timers of `ms`: each replaced the last, none of the ones cancelled ran, and at most one callback waits. */
function assertBounded(app, ms, since, what) {
    const t = timersOf(app, ms, since);
    assert.ok(t.started >= RESETS, `${what}: started each time (${t.started})`);
    assert.equal(t.cancelledRan, 0, `${what}: none cancelled ran`);
    assert.ok(t.waiting <= 1, `${what}: ${t.waiting} callbacks of ${t.started} timers still wait, never to be called`);
}

test('AUD-06: the note draft’s pause, reset as each key is typed, leaves no callbacks behind; the last fires once', () =>
    withApp({}, async (app) => {
        app.click(app.verse(16));
        await app.idle();
        app.click(app.actions().querySelector('.verse-note-btn'));
        await app.until(() => !app.document.getElementById('note-overlay').hidden, 'the note editor');
        const input = app.document.getElementById('note-input');
        const since = started(app);
        for (let i = 1; i <= RESETS; i++) app.type(input, 'Typed'.padEnd(5 + i, '.'));
        await settle();
        assertBounded(app, DRAFT_MS, since, 'the draft');
        await wait(DRAFT_MS + 150);
        assert.equal(timersOf(app, DRAFT_MS, since).ran, 1, 'the last pause fired, once');
        const drafts = JSON.parse(app.window.localStorage.getItem('gospel-getter-note-drafts')).drafts;
        assert.deepEqual(drafts.map((d) => d.text), ['Typed'.padEnd(5 + RESETS, '.')]);
    }));

test('AUD-06: the reading position’s pause, reset by each scroll, leaves no callbacks behind; the last fires once', () =>
    withApp({}, async (app) => {
        const since = started(app);
        for (let i = 0; i < RESETS; i++) app.window.dispatchEvent(new app.window.Event('scroll'));
        await settle();
        assertBounded(app, PLACE_MS, since, 'the reading position');
        await wait(PLACE_MS + 150);
        assert.equal(timersOf(app, PLACE_MS, since).ran, 1, 'the last pause fired, once');
    }));

test('AUD-06: the Library’s note search, reset as each key is typed, leaves no callbacks behind; the last fires once', async () => {
    await backend('__reset', { bookId: JOHN, chapter: 3, translationCode: 'kjv' });
    await backend('add_note', { bookId: JOHN, chapter: 3, verseStart: 16, verseEnd: 16, body: 'Loved.' });
    await withApp({}, async (app) => {
        app.openLibrary('notes');
        const box = app.document.getElementById('note-search');
        const since = started(app);
        for (let i = 1; i <= RESETS; i++) app.type(box, 'Loved'.slice(0, 1 + (i % 5)));
        await settle();
        assertBounded(app, NOTE_SEARCH_MS, since, 'the note search');
        await wait(NOTE_SEARCH_MS + 150);
        assert.equal(timersOf(app, NOTE_SEARCH_MS, since).ran, 1, 'the last pause fired, once');
    });
});

test('AUD-06: a toast said again and again leaves no fading callbacks behind', () =>
    withApp({ book: 'Genesis', chapter: 1 }, async (app) => {
        const since = started(app);
        for (let i = 0; i < RESETS; i++) await app.key('[');
        assert.equal(app.toast().textContent, 'Genesis is the Bible’s first book.');
        assertBounded(app, TOAST_MS, since, 'the toast');
    }));

test('AUD-06: the card’s status line said again and again leaves no callbacks behind', () =>
    withApp({}, async (app) => {
        app.click(app.verse(16));
        await app.idle();
        const since = started(app);
        for (let i = 0; i < RESETS; i++) await app.copyButton();
        await app.until(() => app.status().textContent.startsWith('Copied John 3:16'), 'the status line');
        assertBounded(app, STATUS_MS, since, 'the status line');
    }));
