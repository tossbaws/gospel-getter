// Behavior tests for the top bar's Back, Forward and Recent places (▾):
// each button runs what its key runs (Alt+← and Alt+→, the history's one
// path), says where it goes or that it can't, and Recent places lists the
// places jumped to and left, newest first, kept on this computer across a
// restart, checked as it's read back, and in no backup.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { closeBridge, openApp } from './harness.mjs';

const dir = mkdtempSync(join(tmpdir(), 'gospel-getter-nav-'));
after(async () => {
    await closeBridge();
    rmSync(dir, { recursive: true, force: true });
});

const KEY = 'gospel-getter-recent-places';

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const el = (app, id) => app.document.getElementById(id);
const back = (app) => el(app, 'history-back');
const forward = (app) => el(app, 'history-forward');
const toggle = (app) => el(app, 'recent-places-toggle');
const menu = (app) => el(app, 'recent-places-menu');
const items = (app) => [...menu(app).querySelectorAll('[role="menuitem"]')];
const rows = (app) => [...menu(app).querySelectorAll('.recent-item')].map((r) => r.textContent);
const disabled = (button) => button.getAttribute('aria-disabled') === 'true';
const readings = (app, since) => app.calls.slice(since).filter((c) => c.cmd === 'get_reading').map((c) => [c.args.bookId, c.args.chapter]);

async function waitFor(app, fn, cmd = 'get_reading') {
    const before = app.calls.length;
    fn();
    await app.idle(before, cmd);
}

const pickerChapter = (app, bookName, chapter) => {
    const book = [...app.document.querySelectorAll('#picker .book-item')].find((b) => b.textContent.trim().startsWith(bookName));
    if (book.getAttribute('aria-expanded') !== 'true') app.click(book);
    return app.document.querySelector(`#picker .chapter-grid[data-book-id="${book.dataset.bookId}"] .chapter-btn[data-chapter="${chapter}"]`);
};

// A jump by the picker (the sidebar, at 1280).
const jump = (app, book, chapter) => waitFor(app, () => app.click(pickerChapter(app, book, chapter)));

// A jump to a verse by search's Go to.
async function goTo(app, reference) {
    await app.search(reference);
    await waitFor(app, () => app.key('Enter', {}, app.searchInput()));
}

function openMenu(app) {
    if (menu(app).hidden) app.click(toggle(app));
    assert.ok(!menu(app).hidden, 'the menu is open');
}

// Spies on every focus() call, to check none scrolls the page.
function focusCalls(app) {
    const calls = [];
    const focus = app.window.HTMLElement.prototype.focus;
    app.window.HTMLElement.prototype.focus = function (options) {
        calls.push({ el: this, options });
        return focus.call(this, options);
    };
    return calls;
}

test('Back and Forward are buttons that name where they go, and are unavailable at the history’s ends', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        for (const [button, keys, title] of [
            [back(app), 'Alt+ArrowLeft', 'Nothing earlier in this session (Alt+←)'],
            [forward(app), 'Alt+ArrowRight', 'Nothing later in this session (Alt+→)'],
        ]) {
            assert.equal(button.tagName, 'BUTTON');
            assert.equal(button.getAttribute('type'), 'button');
            assert.ok(button.classList.contains('bar-btn'));
            assert.equal(button.getAttribute('aria-keyshortcuts'), keys);
            assert.ok(disabled(button), 'nowhere to go yet');
            assert.ok(!button.disabled, 'still in the tab order, saying it’s unavailable');
            assert.equal(button.title, title);
            assert.ok(button.querySelector('svg[aria-hidden="true"]'), 'an inline SVG icon');
            assert.equal(button.textContent.trim(), '', 'no glyph');
        }
        assert.equal(back(app).getAttribute('aria-label'), 'Back');
        // A click on an unavailable button does nothing at all.
        const before = app.calls.length;
        app.click(back(app));
        await app.idle(before);
        assert.equal(app.calls.length, before);
        assert.equal(app.heading(), 'John 3');

        await jump(app, 'Psalms', 23);
        assert.ok(!disabled(back(app)));
        assert.equal(back(app).getAttribute('aria-label'), 'Back to John 3');
        assert.equal(back(app).title, 'Back to John 3 (Alt+←)');
        assert.ok(disabled(forward(app)));
        await goTo(app, 'rom 8:28');
        assert.equal(back(app).title, 'Back to Psalms 23 (Alt+←)');

        await waitFor(app, () => app.click(back(app)));
        assert.equal(app.heading(), 'Psalms 23');
        assert.equal(forward(app).title, 'Forward to Romans 8:28 (Alt+→)');
        assert.equal(forward(app).getAttribute('aria-label'), 'Forward to Romans 8:28');
        assert.equal(back(app).title, 'Back to John 3 (Alt+←)');
        await waitFor(app, () => app.click(forward(app)));
        assert.equal(app.heading(), 'Romans 8');
        assert.deepEqual(app.selectedVerseNumbers(), [28], 'the verse it landed on, selected again');
        assert.ok(disabled(forward(app)));
    }));

test('the buttons run exactly what Alt+← and Alt+→ run', async () => {
    // The same walk twice, once by the keys and once by the buttons: the
    // same chapters read, in the same order, landing in the same places.
    const walk = async (byButton) => {
        let trace;
        await withApp({ book: 'John', chapter: 3 }, async (app) => {
            await jump(app, 'Psalms', 23);
            await jump(app, 'Genesis', 1);
            const step = (dir) => (byButton
                ? app.click(dir < 0 ? back(app) : forward(app))
                : app.key(dir < 0 ? 'ArrowLeft' : 'ArrowRight', { altKey: true }));
            const since = app.calls.length;
            const where = [];
            for (const dir of [-1, -1, 1, -1, 1, 1]) {
                await waitFor(app, () => step(dir));
                where.push(app.heading());
            }
            trace = { reads: readings(app, since), where };
        });
        return trace;
    };
    const byKeys = await walk(false);
    const byButtons = await walk(true);
    assert.deepEqual(byButtons, byKeys);
    assert.deepEqual(byKeys.where, ['Psalms 23', 'John 3', 'Psalms 23', 'John 3', 'Psalms 23', 'Genesis 1']);
});

test('Recent places lists the places jumped to and left, newest first, the reader’s place marked', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        assert.equal(toggle(app).getAttribute('aria-haspopup'), 'menu');
        assert.equal(toggle(app).getAttribute('aria-controls'), 'recent-places-menu');
        assert.equal(toggle(app).getAttribute('aria-label'), 'Recent places');
        assert.equal(menu(app).getAttribute('role'), 'menu');
        openMenu(app);
        assert.equal(toggle(app).getAttribute('aria-expanded'), 'true');
        assert.equal(app.window.gospelGetterSurfaces.current(), 'recent');
        assert.equal(menu(app).querySelector('.recent-label').textContent, 'Recent places');
        assert.equal(menu(app).querySelector('.recent-empty').textContent, 'No recent places yet.');
        assert.deepEqual(items(app), []);
        app.click(toggle(app));
        assert.ok(menu(app).hidden);
        assert.equal(toggle(app).getAttribute('aria-expanded'), 'false');

        await goTo(app, 'rom 8:28');
        await jump(app, 'Psalms', 23);
        // A chapter step isn't a jump: Psalm 24 isn't listed.
        await waitFor(app, () => app.key('ArrowRight'));
        assert.equal(app.heading(), 'Psalms 24');
        openMenu(app);
        assert.deepEqual(rows(app), ['Psalms 23 · KJV', 'Romans 8:28 · KJV', 'John 3 · KJV']);
        assert.equal(menu(app).querySelector('.is-here'), null, 'Psalm 24 is in none');
        for (const item of items(app)) {
            assert.equal(item.getAttribute('role'), 'menuitem');
            assert.equal(item.getAttribute('tabindex'), '-1');
        }
        app.key('Escape', {}, app.document.activeElement);
        await waitFor(app, () => app.key('ArrowLeft'));
        openMenu(app);
        const here = menu(app).querySelectorAll('.is-here');
        assert.equal(here.length, 1);
        assert.equal(here[0].getAttribute('aria-current'), 'location');
        assert.equal(here[0].textContent, 'Psalms 23 · KJVyou are here');
        // Up to ten, one per chapter, the newest of each.
        app.key('Escape', {}, app.document.activeElement);
        for (let chapter = 1; chapter <= 12; chapter++) await jump(app, 'Genesis', chapter);
        await goTo(app, 'gen 3:15');
        openMenu(app);
        assert.equal(rows(app).length, 10);
        assert.deepEqual(rows(app).slice(0, 3), ['Genesis 3:15 · KJVyou are here', 'Genesis 12 · KJV', 'Genesis 11 · KJV']);
        assert.equal(rows(app).filter((r) => r.startsWith('Genesis 3')).length, 1);
    }));

test('the menu by keyboard: ↓ opens it at its first place, ↑ and ↓ move, Enter jumps, Escape goes back to ▾, and no focus scrolls', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await jump(app, 'Psalms', 23);
        await jump(app, 'Romans', 8);
        const calls = focusCalls(app);
        toggle(app).focus({ preventScroll: true });
        app.key('ArrowDown', {}, toggle(app));
        assert.ok(!menu(app).hidden);
        assert.equal(app.document.activeElement, items(app)[0]);
        app.key('ArrowDown', {}, app.document.activeElement);
        assert.equal(app.document.activeElement, items(app)[1]);
        app.key('ArrowUp', {}, app.document.activeElement);
        assert.equal(app.document.activeElement, items(app)[0]);
        // ← and → in the menu aren't chapter turns.
        const before = app.calls.length;
        app.key('ArrowRight', {}, app.document.activeElement);
        await app.idle(before);
        assert.equal(app.heading(), 'Romans 8');
        app.key('Escape', {}, app.document.activeElement);
        assert.ok(menu(app).hidden);
        assert.equal(app.document.activeElement, toggle(app), 'focus back on ▾');
        assert.equal(toggle(app).getAttribute('aria-expanded'), 'false');

        // Enter on a place (a button: Enter clicks it) jumps there.
        app.key('ArrowDown', {}, toggle(app));
        app.key('ArrowDown', {}, app.document.activeElement);
        assert.equal(app.document.activeElement.textContent, 'Psalms 23 · KJV');
        await waitFor(app, () => app.click(app.document.activeElement));
        assert.equal(app.heading(), 'Psalms 23');
        assert.ok(menu(app).hidden);
        assert.equal(app.document.activeElement, toggle(app));
        // A jump like any other: Back returns to Romans 8.
        assert.equal(back(app).title, 'Back to Romans 8 (Alt+←)');
        await waitFor(app, () => app.click(back(app)));
        assert.equal(app.heading(), 'Romans 8');

        const scrolling = calls.filter((c) => !(c.options && c.options.preventScroll));
        assert.ok(calls.length >= 6, `${calls.length} focus calls`);
        assert.deepEqual(scrolling.map((c) => c.el.id || c.el.className), []);
    }));

test('a recent place opens in the translation it was read in', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await jump(app, 'Romans', 8);
        app.open('translation');
        await waitFor(app, () => app.click(app.document.querySelector('#translation-choices [data-value="web"]')));
        app.key('Escape', {}, app.document.activeElement);
        await jump(app, 'Psalms', 23);
        openMenu(app);
        assert.deepEqual(rows(app), ['Psalms 23 · WEByou are here', 'Romans 8 · WEB', 'John 3 · KJV']);
        await waitFor(app, () => app.click(items(app)[2]));
        assert.equal(app.heading(), 'John 3');
        assert.equal(el(app, 'translation-select').value, 'kjv');
        assert.equal(el(app, 'translation-toggle').textContent, 'KJV');
        assert.equal(el(app, 'ref-text').textContent, 'John 3 · KJV');
    }));

test('recent places last across a restart, on this computer only: not a display setting, and in no backup', async () => {
    let storage;
    await withApp({ book: 'John', chapter: 3 }, async (app) => {
        await goTo(app, 'rom 8:28');
        await jump(app, 'Psalms', 23);
        storage = app.storage();
        assert.deepEqual(JSON.parse(storage[KEY]), [
            { bookId: 19, chapter: 23, translation: 'kjv' },
            { bookId: 45, chapter: 8, start: 28, end: 28, translation: 'kjv' },
            { bookId: 43, chapter: 3, translation: 'kjv' },
        ]);
        assert.ok(!(KEY.replace('gospel-getter-', '').replace('-', '_') in app.window.gospelGetterPreferences.current()));
        const path = join(dir, 'backup.gospel-getter.json');
        await app.pick(path);
        app.open('menu');
        const before = app.calls.length;
        app.click(el(app, 'export-data-btn'));
        await app.idle(before);
        const text = readFileSync(path, 'utf8');
        assert.ok(!/recent/i.test(text), 'Recent places aren’t in the backup');
        assert.ok(!text.includes('"start":28') && !text.includes('Romans'), 'nor any of its places');
    });
    // The Back and Forward history is this session's only; Recent places come back.
    await withApp({ book: 'Psalms', chapter: 23, storage }, async (app) => {
        assert.ok(disabled(back(app)) && disabled(forward(app)));
        openMenu(app);
        assert.deepEqual(rows(app), ['Psalms 23 · KJVyou are here', 'Romans 8:28 · KJV', 'John 3 · KJV']);
        await waitFor(app, () => app.click(items(app)[1]));
        assert.equal(app.heading(), 'Romans 8');
        assert.deepEqual(app.selectedVerseNumbers(), [28]);
    });
});

test('what’s kept is checked as it’s read back, and a value that isn’t a list of places never stops the app starting', async () => {
    const good = { bookId: 43, chapter: 3, translation: 'kjv' };
    const values = [
        ['not JSON', 'not json', []],
        ['an object', JSON.stringify(good), []],
        ['a string', '"John 3"', []],
        ['numbers', '[1, 2, 3]', []],
        ['null', 'null', []],
        ['too long to be ours', JSON.stringify([good, ...Array(80).fill({ ...good, chapter: 1, padding: 'x'.repeat(40) })]), []],
        ['entries that aren’t places', JSON.stringify([
            { bookId: 999, chapter: 1, translation: 'kjv' },
            { bookId: 43, chapter: 99, translation: 'kjv' },
            { bookId: 43, chapter: 1, translation: 'xyz' },
            { bookId: 43, chapter: 2, start: 9, end: 3, translation: 'kjv' },
            { bookId: '43', chapter: 4, translation: 'kjv' },
            { bookId: 43, chapter: 5, translation: '<img src=x onerror=alert(1)>' },
            'Romans 8',
            good,
            { bookId: 43, chapter: 3, translation: 'web' },
            { bookId: 19, chapter: 23, start: 1, end: 6, translation: 'web' },
        ]), ['John 3 · KJV', 'Psalms 23:1–6 · WEB']],
    ];
    for (const [what, value, expected] of values) {
        await withApp({ book: 'Genesis', chapter: 1, storage: { [KEY]: value } }, async (app) => {
            assert.equal(app.heading(), 'Genesis 1', what);
            assert.equal(app.consoleErrors.length, 0, `${what}: ${app.consoleErrors.join('\n')}`);
            openMenu(app);
            assert.deepEqual(rows(app), expected, what);
            assert.equal(menu(app).querySelector('img'), null);
            app.key('Escape', {}, app.document.activeElement);
            // The next jump keeps only what's valid.
            await jump(app, 'Exodus', 2);
            const kept = JSON.parse(app.storage()[KEY]);
            assert.equal(kept.length, expected.length + 2, what);
            assert.deepEqual(kept.slice(0, 2).map((p) => p.chapter), [2, 1]);
        });
    }
});

test('a narrow window folds Forward into the menu, first, and leaves the translation off the reference', () =>
    withApp({ book: 'John', chapter: 3, width: 700 }, async (app) => {
        const ref = el(app, 'ref-text');
        assert.equal(ref.textContent, 'John 3 · KJV');
        assert.equal(ref.querySelector('.ref-codes').textContent, ' · KJV', 'the part a narrow window hides');
        assert.equal(ref.querySelector('.ref-place').textContent, 'John 3');
        await goTo(app, 'ps 23');
        openMenu(app);
        const fold = items(app)[0];
        assert.equal(fold.id, 'recent-forward');
        assert.ok(fold.disabled, 'nothing later yet');
        assert.equal(fold.getAttribute('aria-keyshortcuts'), 'Alt+ArrowRight');
        assert.equal(fold.textContent, 'ForwardAlt+→');
        assert.ok(fold.nextElementSibling.matches('[role="separator"]'));
        assert.equal(app.document.activeElement, items(app)[1], 'focus skips it to the first place');
        app.key('Escape', {}, app.document.activeElement);
        await waitFor(app, () => app.click(back(app)));
        assert.equal(app.heading(), 'John 3');
        openMenu(app);
        assert.ok(!items(app)[0].disabled);
        assert.equal(app.document.activeElement, items(app)[0]);
        // Forward from the menu: Alt+→'s path, landing where it would.
        await waitFor(app, () => app.click(items(app)[0]));
        assert.equal(app.heading(), 'Psalms 23');
        assert.ok(menu(app).hidden);
        // Wide again: Forward is the bar's, not the menu's.
        app.resize(1280);
        openMenu(app);
        assert.equal(el(app, 'recent-forward'), null);
    }));

test('reading mode is bare: Recent places doesn’t open there', () =>
    withApp({ book: 'John', chapter: 3, readerMode: true }, async (app) => {
        app.click(toggle(app));
        assert.ok(menu(app).hidden);
        assert.equal(app.window.gospelGetterSurfaces.current(), null);
    }));
