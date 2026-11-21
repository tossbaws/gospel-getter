// Behavior tests for two passages remembered on restart (S9; ui-leptos/src/
// pane.rs's `reopen`, reader.rs's `reopen_beside`): `gospel-getter-split`
// (a saved setting of this computer's, pre-painted as html[data-split]) and
// the second pane's place, `gospel-getter-second-pane`, kept in local
// storage and in no backup. A restart with both opens the second pane where
// it was, the first at the backend's place and active (D8); with neither,
// nothing changes (the upgrader's case, down to the pre-paint script's
// attributes); with a place that can't be one, the page is one pane again
// and both keys go. × forgets both; the second pane's place never reaches
// save_reading_position (S15), an export, or an import's settings. Two
// panes are reached, for now, only through `window.gospelGetterPanes`.
// jsdom has no layout: each column is laid out by `layOut`.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { JSDOM } from 'jsdom';
import { backend, bookId, closeBridge, el, exportData, openApp, withApp } from './harness.mjs';

const dir = mkdtempSync(join(tmpdir(), 'gospel-getter-split-remembered-'));
after(async () => {
    await closeBridge();
    rmSync(dir, { recursive: true, force: true });
});

const ROMANS = bookId('Romans');
const JOHN = bookId('John');
const SPLIT_KEY = 'gospel-getter-split';
const PLACE_KEY = 'gospel-getter-second-pane';

const panes = (app) => app.window.gospelGetterPanes;
const column = (app, name) => app.document.querySelector(`.pane[data-pane="${name}"]`);
const root = (app, n) => app.document.getElementById(n === 1 ? 'reading-pane' : 'reading-pane-2');
const heading = (app, n) => root(app, n).querySelector('.chapter-heading').textContent;
const codesIn = (app, n) => [...new Set([...root(app, n).querySelectorAll('.verse[data-translation]')].map((v) => v.dataset.translation))];
const stored = (app, key) => app.window.localStorage.getItem(key);
const place = (app) => JSON.parse(stored(app, PLACE_KEY));
const readings = (app) => app.calls.filter((c) => c.cmd === 'get_reading').map((c) => JSON.parse(JSON.stringify(c.args)));
const savedBooks = (app) => app.calls.filter((c) => c.cmd === 'save_reading_position').map((c) => c.args.bookId);
const htmlAttributes = (document) => [...document.documentElement.attributes].map((a) => `${a.name}=${a.value}`).sort();

/**
 * The columns laid out, set up before the page's scripts run (a restart
 * opens the second pane during boot): each column 120px down the window
 * and 500px tall, its bar to 160px; verse n of a column 60n + 120px down
 * it, 50px tall, and its heading 130px. Each column's scroll position is
 * kept in `scrolled`.
 */
function layOut(scrolled) {
    return (window) => {
        const proto = window.HTMLElement.prototype;
        const box = (top, height) => ({ top, bottom: top + height, left: 0, right: 600, width: 600, height, x: 0, y: top });
        Object.defineProperty(proto, 'scrollTop', {
            configurable: true,
            get() { return this.matches('.pane') ? scrolled[this.dataset.pane] : 0; },
            set(y) { if (this.matches('.pane')) scrolled[this.dataset.pane] = Math.max(0, y); },
        });
        Object.defineProperty(proto, 'clientHeight', {
            configurable: true,
            get() { return this.matches('.pane') ? 500 : 0; },
        });
        const measured = proto.getBoundingClientRect;
        proto.getBoundingClientRect = function () {
            const col = this.closest('.pane');
            if (!col) return measured.call(this);
            const name = col.dataset.pane;
            if (this === col) return box(120, 500);
            if (this.matches('.pane-bar')) return box(120, 40);
            if (this.matches('.chapter-main .chapter-heading')) return box(130 - scrolled[name], 30);
            if (this.matches('.chapter-main .verse[data-verse]')) return box(60 * Number(this.dataset.verse) + 120 - scrolled[name], 50);
            return measured.call(this);
        };
    };
}

/** The reader scrolls a column to `y`; the page notes it. */
function scrollColumn(app, scrolled, name, y) {
    scrolled[name] = y;
    column(app, name).dispatchEvent(new app.window.Event('scroll'));
}

/** Waits out the reader's place-saving debounce (400 ms) and what follows. */
async function stillFor(app) {
    await new Promise((r) => setTimeout(r, 500));
    await app.idle();
}

/**
 * A session that splits, scrolls both panes and loses focus, then closes:
 * the first pane at John 3:5, 40px above its bar's foot (the backend's
 * place); the second at Romans 5:10 (BSB), 30px above its own. Returns the
 * local storage it leaves.
 */
async function splitSession() {
    const scrolled = { first: 0, second: 0 };
    const app = await openApp({ book: 'John', chapter: 3, translation: 'kjv', setUp: layOut(scrolled) });
    try {
        await panes(app).openBeside({ bookId: ROMANS, chapter: 5, translation: 'bsb' });
        await app.idle();
        scrollColumn(app, scrolled, 'first', 300);
        scrollColumn(app, scrolled, 'second', 590);
        app.window.dispatchEvent(new app.window.Event('blur'));
        await app.idle();
        assert.equal(stored(app, SPLIT_KEY), 'on');
        assert.deepEqual(place(app), { bookId: ROMANS, chapter: 5, translation: 'bsb', verse: 10, offset: -30 });
        const saved = app.window.gospelGetterPlace.saved();
        assert.deepEqual([saved.bookId, saved.chapter, saved.verse, saved.offset], [JOHN, 3, 5, -40]);
        return app.storage();
    } finally {
        app.close();
    }
}

test('a restart with the panes split opens the second where it was, the first at the backend’s place and active', async () => {
    const storage = await splitSession();
    const scrolled = { first: 0, second: 0 };
    const app = await openApp({ book: 'John', chapter: 3, translation: 'kjv', keepPosition: true, storage, setUp: layOut(scrolled) });
    try {
        assert.equal(app.document.documentElement.dataset.split, 'on');
        assert.deepEqual([...panes(app).shown()], ['first', 'second']);
        assert.equal(panes(app).active(), 'first', 'the first pane is active at launch (D8)');
        assert.match(el(app, 'ref-btn').getAttribute('aria-label'), /^John 3,/);
        assert.equal(app.document.getElementById('translation-select').value, 'kjv');
        // The first pane's chapter, then the second's, each read once.
        assert.deepEqual(readings(app), [
            { bookId: JOHN, chapter: 3, translationCode: 'kjv' },
            { bookId: ROMANS, chapter: 5, translationCode: 'bsb' },
        ]);
        assert.equal(heading(app, 1), 'John 3');
        assert.deepEqual(codesIn(app, 1), ['kjv']);
        assert.equal(heading(app, 2), 'Romans 5');
        assert.deepEqual(codesIn(app, 2), ['bsb']);
        // Each verse back where it was: John 3:5 40px above the first bar's
        // foot (160px), Romans 5:10 30px above the second's.
        assert.equal(scrolled.first, 300, 'verse 5 at 120px');
        assert.equal(scrolled.second, 590, 'verse 10 at 130px');

        // Once still, what's kept is as it was, and the second pane's place
        // never went to the backend (S15).
        await stillFor(app);
        assert.deepEqual(place(app), { bookId: ROMANS, chapter: 5, translation: 'bsb', verse: 10, offset: -30 });
        assert.equal(stored(app, SPLIT_KEY), 'on');
        scrollColumn(app, scrolled, 'second', 650);
        app.window.dispatchEvent(new app.window.Event('blur'));
        await app.idle();
        assert.deepEqual(place(app), { bookId: ROMANS, chapter: 5, translation: 'bsb', verse: 11, offset: -30 });
        assert.ok(!savedBooks(app).includes(ROMANS), `saved: ${savedBooks(app)}`);
        const home = await backend('get_home');
        assert.deepEqual([home.currentBookId, home.currentChapter, home.currentVerse, home.currentVerseOffset], [JOHN, 3, 5, -40]);
    } finally {
        app.close();
    }
});

test('the second pane’s place follows its chapter and translation, and × forgets both keys', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        assert.equal(stored(app, SPLIT_KEY), null);
        assert.equal(stored(app, PLACE_KEY), null);
        await panes(app).openBeside({ bookId: ROMANS, chapter: 5, translation: 'bsb' });
        await app.idle();
        assert.equal(stored(app, SPLIT_KEY), 'on');
        assert.deepEqual(place(app), { bookId: ROMANS, chapter: 5, translation: 'bsb' });
        // → in the second pane (active): the next chapter.
        let before = app.calls.length;
        app.key('ArrowRight');
        await app.idle(before, 'get_reading');
        assert.deepEqual(place(app), { bookId: ROMANS, chapter: 6, translation: 'bsb' });
        // Another translation.
        before = app.calls.length;
        await app.key('t');
        app.click(app.surface('translation').querySelector('#translation-choices [data-value="web"]'));
        await app.idle(before, 'get_reading');
        assert.deepEqual(place(app), { bookId: ROMANS, chapter: 6, translation: 'web' });
        assert.ok(!savedBooks(app).includes(ROMANS), `saved: ${savedBooks(app)}`);

        before = app.calls.length;
        await app.click(column(app, 'second').querySelector('.pane-close'));
        await app.idle(before);
        assert.equal(stored(app, SPLIT_KEY), null);
        assert.equal(stored(app, PLACE_KEY), null);
        assert.equal(app.document.documentElement.dataset.split, undefined);
        // Nothing the second pane had waiting writes it back.
        await stillFor(app);
        app.window.dispatchEvent(new app.window.Event('blur'));
        await app.idle();
        assert.equal(stored(app, PLACE_KEY), null);
        assert.equal(stored(app, SPLIT_KEY), null);
    }));

test('with neither key (an upgrader), a restart is one pane, as it always was', async () => {
    const app = await openApp({ book: 'John', chapter: 3, keepPosition: true });
    try {
        assert.equal(app.document.documentElement.dataset.split, undefined);
        assert.deepEqual([...panes(app).shown()], ['first']);
        assert.equal(app.document.querySelector('.pane'), null);
        assert.equal(app.document.getElementById('reading-pane-2'), null);
        assert.ok(app.document.querySelector('#reading-pane .chapter-side'), 'the faded neighbors, as one pane has them');
        assert.deepEqual(readings(app), [{ bookId: JOHN, chapter: 3, translationCode: 'kjv' }]);
        assert.equal(stored(app, SPLIT_KEY), null);
        assert.equal(stored(app, PLACE_KEY), null);
    } finally {
        app.close();
    }
});

test('split with no place, or one that can’t be, is one pane again, and both keys go', async () => {
    const good = JSON.stringify({ bookId: ROMANS, chapter: 5, translation: 'bsb', verse: 3 });
    for (const [why, kept] of [
        ['no place', null],
        ['an unknown book', JSON.stringify({ bookId: 999, chapter: 1, translation: 'kjv' })],
        ['a chapter the book doesn’t have', JSON.stringify({ bookId: ROMANS, chapter: 17, translation: 'kjv' })],
        ['a translation that isn’t bundled', JSON.stringify({ bookId: ROMANS, chapter: 5, translation: 'esv' })],
        ['a verse past any chapter’s', JSON.stringify({ bookId: ROMANS, chapter: 5, translation: 'kjv', verse: 177 })],
        ['no chapter', JSON.stringify({ bookId: ROMANS, translation: 'kjv' })],
        ['not JSON', '{"bookId":'],
        ['far too long to be one', `${good.slice(0, -1)},"x":"${'x'.repeat(600)}"}`],
    ]) {
        const storage = { [SPLIT_KEY]: 'on' };
        if (kept !== null) storage[PLACE_KEY] = kept;
        const app = await openApp({ book: 'John', chapter: 3, storage });
        try {
            assert.equal(app.document.documentElement.dataset.split, undefined, why);
            assert.deepEqual([...panes(app).shown()], ['first'], why);
            assert.equal(app.document.querySelector('.pane'), null, why);
            assert.equal(stored(app, SPLIT_KEY), null, why);
            assert.equal(stored(app, PLACE_KEY), null, why);
            assert.deepEqual(readings(app), [{ bookId: JOHN, chapter: 3, translationCode: 'kjv' }], why);
            assert.ok(app.document.querySelector('#reading-pane .chapter-side'), `${why}: one pane, with its faded neighbors`);
        } finally {
            app.close();
        }
    }
    // The same place, good, opens the second pane.
    const app = await openApp({ book: 'John', chapter: 3, storage: { [SPLIT_KEY]: 'on', [PLACE_KEY]: good } });
    try {
        assert.deepEqual([...panes(app).shown()], ['first', 'second']);
        assert.equal(heading(app, 2), 'Romans 5');
    } finally {
        app.close();
    }
});

test('a place with no split is left alone, and opens nothing', () =>
    withApp({ book: 'John', chapter: 3, storage: { [PLACE_KEY]: JSON.stringify({ bookId: ROMANS, chapter: 5, translation: 'bsb' }) } }, async (app) => {
        assert.deepEqual([...panes(app).shown()], ['first']);
        assert.equal(app.document.documentElement.dataset.split, undefined);
        assert.deepEqual(readings(app), [{ bookId: JOHN, chapter: 3, translationCode: 'kjv' }]);
    }));

test('neither key reaches a backup, and importing one leaves the panes as they are', async () => {
    const scrolled = { first: 0, second: 0 };
    await withApp({ book: 'John', chapter: 3, translation: 'kjv', setUp: layOut(scrolled) }, async (app) => {
        await panes(app).openBeside({ bookId: ROMANS, chapter: 5, translation: 'bsb' });
        await app.idle();
        scrollColumn(app, scrolled, 'second', 590);
        app.window.dispatchEvent(new app.window.Event('blur'));
        await app.idle();
        const kept = [stored(app, SPLIT_KEY), stored(app, PLACE_KEY)];
        assert.equal(kept[0], 'on');

        const path = join(dir, 'everything.gospel-getter.json');
        await exportData(app, path);
        const text = readFileSync(path, 'utf8');
        const json = JSON.parse(text);
        assert.equal(json.format, 'gospel-getter-reader-data');
        assert.ok(!/split|second|pane/i.test(JSON.stringify(json.preferences)), JSON.stringify(json.preferences));
        assert.ok(!text.includes(SPLIT_KEY) && !text.includes(PLACE_KEY));
        assert.ok(!/"bookId"|"book": ?45/.test(text), 'the second pane’s place isn’t in it');
        assert.deepEqual(json.reading_position, { translation: 'kjv', book: JOHN, book_name: 'John', chapter: 3 });

        // The first pane moves on, and the second is active again.
        const press = (el) => el.dispatchEvent(new app.window.MouseEvent('pointerdown', { bubbles: true, cancelable: true, button: 0 }));
        press(root(app, 1).querySelector('.chapter-heading'));
        let before = app.calls.length;
        app.key('ArrowRight');
        await app.idle(before, 'get_reading');
        assert.equal(heading(app, 1), 'John 4');
        press(root(app, 2).querySelector('.chapter-heading'));
        await app.idle();
        assert.equal(panes(app).active(), 'second');

        // The whole file back, every setting in it applied, and its reading
        // position: the first pane's (S15), whichever is active.
        await app.pick(path);
        app.open('menu');
        before = app.calls.length;
        app.click(el(app, 'import-data-btn'));
        await app.idle(before);
        assert.ok(el(app, 'import-position').checked, 'the file’s reading position, chosen');
        before = app.calls.length;
        app.click(el(app, 'import-confirm'));
        await app.idle(before, 'apply_import');
        assert.deepEqual([stored(app, SPLIT_KEY), stored(app, PLACE_KEY)], kept);
        assert.equal(app.document.documentElement.dataset.split, 'on');
        assert.deepEqual([...panes(app).shown()], ['first', 'second']);
        assert.equal(heading(app, 1), 'John 3');
        assert.equal(heading(app, 2), 'Romans 5');
        assert.deepEqual(codesIn(app, 2), ['bsb']);
        assert.ok(!savedBooks(app).includes(ROMANS), `saved: ${savedBooks(app)}`);
    });
});

// ---- The pre-paint script, on its own.

/** ui/index.html's pre-paint script: the first script in <head>. */
const PREPAINT = (() => {
    const html = readFileSync(new URL('../../ui/index.html', import.meta.url), 'utf8');
    const head = html.slice(0, html.indexOf('</head>'));
    const at = head.indexOf('<script>');
    return head.slice(at + '<script>'.length, head.indexOf('</script>', at));
})();

const SPLIT_ROW = "                ['gospel-getter-split', 'split', 'off', ['off', 'on'], {}],\n";

/** `<html>`'s attributes once `script` has run with `storage` saved, `width` px wide. */
function prePainted(script, storage, width = 1280) {
    const { window } = new JSDOM('<!doctype html><html><head></head><body></body></html>', {
        url: 'http://tauri.localhost/',
        runScripts: 'outside-only',
    });
    for (const [key, value] of Object.entries(storage)) window.localStorage.setItem(key, value);
    window.matchMedia = (query) => ({ matches: Number(/(\d+)px/.exec(query)[1]) <= width });
    window.eval(script);
    return htmlAttributes(window.document);
}

test('with no split saved, the pre-paint script sets exactly what it did before the split row', () => {
    assert.ok(PREPAINT.includes(SPLIT_ROW), 'the generated row');
    const before = PREPAINT.replace(SPLIT_ROW, '');
    for (const storage of [
        {},
        { [SPLIT_KEY]: 'off' },
        { [SPLIT_KEY]: 'yes' },
        { [PLACE_KEY]: '{"bookId":45,"chapter":5,"translation":"bsb"}' },
        { 'gospel-getter-theme': 'matrix', 'gospel-getter-reader-mode': 'on', 'gospel-getter-sidebar': 'closed', 'gospel-getter-compare-with': 'bsb' },
    ]) {
        for (const width of [1280, 800]) {
            assert.deepEqual(prePainted(PREPAINT, storage, width), prePainted(before, storage, width), JSON.stringify(storage));
        }
    }
    // Saved on, it's laid out for two panes from the first frame: the only
    // difference.
    const on = { [SPLIT_KEY]: 'on', 'gospel-getter-theme': 'vellum' };
    assert.deepEqual(prePainted(PREPAINT, on), [...prePainted(before, on), 'data-split=on'].sort());
});
