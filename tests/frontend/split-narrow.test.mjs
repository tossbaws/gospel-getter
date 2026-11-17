// Behavior tests for two passages on a window too narrow for them side by
// side (ui-leptos/src/pane.rs, S7), reached for now only through
// `window.gospelGetterPanes`: the panes as tabs, as the WAI-ARIA tabs
// pattern has them, only the active one shown, the top bar's own controls
// acting on it, and each pane's place kept as it goes out of view and put
// back as it comes into view (the window crossing 900px, or a tab chosen),
// whatever the engine does with a hidden column's scroll position.
// jsdom has no layout: where places matter, the page is laid out by
// `layOut`.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { bookId, closeBridge, frame, withApp } from './harness.mjs';

after(closeBridge);

const ROMANS = bookId('Romans');
const JOHN = bookId('John');

const panes = (app) => app.window.gospelGetterPanes;
const $ = (app, id) => app.document.getElementById(id);
const root = (app, n) => $(app, n === 1 ? 'reading-pane' : 'reading-pane-2');
const heading = (app, n) => root(app, n).querySelector('.chapter-heading').textContent;
const column = (app, name) => app.document.querySelector(`.pane[data-pane="${name}"]`);
const tabs = (app) => app.document.querySelector('.pane-tabs');
const tab = (app, name) => $(app, `pane-tab-${name}`);
const tabText = (app, name) => [...tab(app, name).children].map((part) => part.textContent);
const readings = (app, since = 0) => app.calls.slice(since).filter((c) => c.cmd === 'get_reading')
    .map((c) => JSON.parse(JSON.stringify(c.args)));

/** Opens `args` beside, through the test global, and waits for the backend. */
async function openBeside(app, args) {
    const before = app.calls.length;
    const loaded = await panes(app).openBeside(args);
    await app.idle(before);
    return loaded;
}

/** The tabs as the reader meets them: which shows, and what each says. */
function tabsSeen(app) {
    return {
        tabs: app.document.documentElement.hasAttribute('data-pane-tabs'),
        shown: tabs(app) ? !tabs(app).hidden : false,
        selected: ['first', 'second'].filter((name) => tab(app, name).getAttribute('aria-selected') === 'true'),
        tabindex: ['first', 'second'].map((name) => tab(app, name).getAttribute('tabindex')),
        hidden: ['first', 'second'].filter((name) => column(app, name).hidden),
        active: panes(app).active(),
    };
}

test('below 900px the panes are tabs: a tab list naming each pane, only the active pane shown, and back side by side above it', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv', width: 1280 }, async (app) => {
        await openBeside(app, { bookId: ROMANS, chapter: 5, translation: 'bsb' });
        assert.deepEqual(tabsSeen(app), {
            tabs: false, shown: false, selected: ['second'], tabindex: ['-1', '0'], hidden: [], active: 'second',
        });

        app.resize(899);
        assert.deepEqual(tabsSeen(app), {
            tabs: true, shown: true, selected: ['second'], tabindex: ['-1', '0'], hidden: ['first'], active: 'second',
        });
        const list = tabs(app).querySelector('[role="tablist"]');
        assert.equal(list.getAttribute('aria-label'), 'Passages');
        assert.deepEqual([...list.children].map((t) => t.getAttribute('role')), ['tab', 'tab'], 'only tabs in the list');
        for (const name of ['first', 'second']) {
            const controlled = $(app, tab(app, name).getAttribute('aria-controls'));
            assert.equal(controlled, column(app, name));
            assert.equal(controlled.getAttribute('role'), 'tabpanel');
            assert.equal(controlled.getAttribute('aria-labelledby'), tab(app, name).id);
        }
        assert.deepEqual(tabText(app, 'first'), ['John 3', 'KJV']);
        assert.deepEqual(tabText(app, 'second'), ['Romans 5', 'BSB']);
        // × follows the list, outside it.
        assert.equal(list.querySelector('.pane-close'), null);
        assert.ok(tabs(app).querySelector('.pane-close'));
        const ids = [...app.document.querySelectorAll('[id]')].map((el) => el.id);
        assert.equal(new Set(ids).size, ids.length, 'every id once');

        // A tab chosen: its pane shows instead, and is the active one.
        await app.click(tab(app, 'first'));
        assert.deepEqual(tabsSeen(app), {
            tabs: true, shown: true, selected: ['first'], tabindex: ['0', '-1'], hidden: ['second'], active: 'first',
        });
        assert.match(app.document.title, /^John 3/);
        assert.equal($(app, 'translation-select').value, 'kjv');

        // 900px: side by side again, each with its bar; the tabs go.
        app.resize(900);
        assert.deepEqual(tabsSeen(app), {
            tabs: false, shown: false, selected: ['first'], tabindex: ['0', '-1'], hidden: [], active: 'first',
        });
        assert.equal(column(app, 'first').getAttribute('role'), null);
        assert.equal(column(app, 'first').getAttribute('aria-labelledby'), null);
    }));

test('the top bar’s reference, arrows and translation button act on the active pane while tabbed', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv', width: 800 }, async (app) => {
        await openBeside(app, { bookId: ROMANS, chapter: 5, translation: 'bsb' });
        assert.equal(tabsSeen(app).tabs, true);
        assert.match($(app, 'ref-text').textContent, /^Romans 5/);
        assert.equal($(app, 'translation-toggle').textContent, 'BSB');

        // ›: the active pane turns, the other stays.
        let before = app.calls.length;
        app.click($(app, 'chapter-next'));
        await app.idle(before, 'get_reading');
        assert.deepEqual(readings(app, before), [{ bookId: ROMANS, chapter: 6, translationCode: 'bsb' }]);
        assert.equal(heading(app, 2), 'Romans 6');
        assert.equal(heading(app, 1), 'John 3');
        assert.deepEqual(tabText(app, 'second'), ['Romans 6', 'BSB']);

        // The translation button: the popover from the top bar, reading the
        // active pane in the choice.
        await app.click($(app, 'translation-toggle'));
        const popover = app.surface('translation');
        assert.equal(popover.hidden, false);
        assert.equal(popover.parentElement, $(app, 'translation-toggle').parentElement);
        before = app.calls.length;
        app.click(popover.querySelector('#translation-choices [data-value="web"]'));
        await app.idle(before, 'get_reading');
        assert.deepEqual(readings(app, before), [{ bookId: ROMANS, chapter: 6, translationCode: 'web' }]);
        assert.deepEqual(tabText(app, 'second'), ['Romans 6', 'WEB']);
        assert.deepEqual(tabText(app, 'first'), ['John 3', 'KJV']);

        // The reference button: the picker, marking the active pane's book.
        await app.click($(app, 'ref-btn'));
        assert.equal(app.window.gospelGetterSurfaces.current(), 'picker');
        assert.match(app.document.querySelector('#picker .book-item[aria-current]').textContent, /Romans/);
        await app.key('Escape');

        // The first tab chosen: the same controls act on it.
        await app.click(tab(app, 'first'));
        before = app.calls.length;
        app.click($(app, 'chapter-prev'));
        await app.idle(before, 'get_reading');
        assert.deepEqual(readings(app, before), [{ bookId: JOHN, chapter: 2, translationCode: 'kjv' }]);
        assert.equal(heading(app, 1), 'John 2');
        assert.equal(heading(app, 2), 'Romans 6');
        assert.deepEqual(tabText(app, 'first'), ['John 2', 'KJV']);
    }));

test('← and → on a tab move between the tabs, choosing as they go, and never turn the chapter', () =>
    withApp({ book: 'John', chapter: 3, width: 640 }, async (app) => {
        await openBeside(app, { bookId: ROMANS, chapter: 5 });
        tab(app, 'second').focus();
        const before = app.calls.length;
        for (const [key, to] of [['ArrowRight', 'first'], ['ArrowRight', 'second'], ['ArrowLeft', 'first'],
            ['End', 'second'], ['Home', 'first']]) {
            const event = await app.key(key, {}, app.document.activeElement);
            assert.equal(event.defaultPrevented, true, key);
            assert.equal(app.document.activeElement, tab(app, to), `${key}: focus`);
            assert.equal(panes(app).active(), to, `${key}: active`);
            assert.deepEqual(tabsSeen(app).hidden, [to === 'first' ? 'second' : 'first'], `${key}: shown`);
        }
        await app.idle(before);
        assert.deepEqual(readings(app, before), [], 'no chapter turned');
        assert.equal(heading(app, 1), 'John 3');
        assert.equal(heading(app, 2), 'Romans 5');

        // Away from the tabs, → turns the active pane's chapter again.
        app.document.activeElement.blur();
        app.key('ArrowRight');
        await app.idle(before, 'get_reading');
        assert.deepEqual(readings(app, before), [{ bookId: JOHN, chapter: 4, translationCode: 'kjv' }]);
    }));

test('× on the tabs closes the second pane: one pane, the page as it was, focus on the reference button', () =>
    withApp({ book: 'John', chapter: 3, width: 640 }, async (app) => {
        const html = app.document.documentElement;
        const attributes = () => [...html.attributes].map((a) => `${a.name}=${a.value}`).sort();
        const was = { attributes: attributes(), main: app.document.querySelector('main').outerHTML };
        await openBeside(app, { bookId: ROMANS, chapter: 5 });
        assert.equal(tabsSeen(app).tabs, true);
        const before = app.calls.length;
        await app.click(tabs(app).querySelector('.pane-close'));
        await app.idle(before);
        assert.deepEqual([...panes(app).shown()], ['first']);
        assert.equal(panes(app).active(), 'first');
        assert.equal(app.document.activeElement, $(app, 'ref-btn'));
        assert.equal(tabs(app), null, 'the tabs are gone with the second pane');
        assert.deepEqual(attributes(), was.attributes);
        assert.equal(app.document.querySelector('main').outerHTML, was.main);
    }));

/**
 * A laid-out page, as two columns (or tabs) lay it out: each column 120px
 * down the window and 500px tall, its bar (while it shows: side by side)
 * to 160px; verse n of a column 60n + 120px down it, 50px tall. Each
 * column's scroll position is kept here; a hidden column has no layout.
 */
function layOut(app) {
    const { window } = app;
    const scrolled = { first: 0, second: 0 };
    const nameOf = (el) => el.closest('.pane')?.dataset.pane;
    const box = (top, height) => ({ top, bottom: top + height, left: 0, right: 600, width: 600, height, x: 0, y: top });
    const none = box(0, 0);
    const measured = window.HTMLElement.prototype.getBoundingClientRect;
    window.HTMLElement.prototype.getBoundingClientRect = function () {
        const name = nameOf(this);
        if (!name) return measured.call(this);
        const col = column(app, name);
        if (col.hidden) return none;
        if (this === col) return box(120, 500);
        if (this.matches('.pane-bar')) return window.document.documentElement.hasAttribute('data-pane-tabs') ? none : box(120, 40);
        if (this.matches('.chapter-main .chapter-heading')) return box(130 - scrolled[name], 30);
        if (this.matches('.chapter-main .verse[data-verse]')) {
            return box(60 * Number(this.dataset.verse) + 120 - scrolled[name], 50);
        }
        return measured.call(this);
    };
    for (const name of ['first', 'second']) {
        const col = column(app, name);
        Object.defineProperty(col, 'clientHeight', { configurable: true, get: () => (col.hidden ? 0 : 500) });
        Object.defineProperty(col, 'scrollTop', {
            configurable: true,
            get: () => (col.hidden ? 0 : scrolled[name]),
            set: (y) => { if (!col.hidden) scrolled[name] = Math.max(0, y); },
        });
    }
    return scrolled;
}

/** The reader scrolls a column to `y` and stops; the page notes its layout. */
async function scrollColumn(app, scrolled, name, y) {
    scrolled[name] = y;
    column(app, name).dispatchEvent(new app.window.Event('scroll'));
    await frame(app);
}

test('each pane keeps its place across the tabs and the 900px crossing, not trusting a hidden column’s scroll position', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv', width: 1280 }, async (app) => {
        await openBeside(app, { bookId: ROMANS, chapter: 5, translation: 'bsb' });
        const scrolled = layOut(app);
        // Side by side, below the bars (160px): verse 5 of the first pane
        // 40px above the bar's foot, verse 10 of the second 30px above it.
        await scrollColumn(app, scrolled, 'first', 300);
        await scrollColumn(app, scrolled, 'second', 590);

        // Across 900px: the second pane (active) alone, its verse the same
        // distance from the top of its view, which has no bar now (120px);
        // the first's place kept for it.
        app.resize(800);
        assert.deepEqual(tabsSeen(app).hidden, ['first']);
        assert.equal(scrolled.second, 630, 'verse 10 30px above the top of the second pane’s view');
        // Out of view, the first pane's place is still the one saved.
        app.window.dispatchEvent(new app.window.Event('blur'));
        await app.idle();
        assert.equal(app.window.gospelGetterPlace.saved().bookId, JOHN);
        assert.equal(app.window.gospelGetterPlace.saved().verse, 5);
        assert.equal(app.window.gospelGetterPlace.saved().offset, -40);

        // The first tab: an engine may have dropped the hidden column's
        // scroll position; its place is put back all the same, 40px above
        // the view's top (the tabs' column has no bar).
        scrolled.first = 0;
        await app.click(tab(app, 'first'));
        assert.equal(scrolled.first, 340, 'verse 5 40px above the top of the first pane’s view');
        await frame(app);
        // And back: the second pane's place as it went.
        scrolled.second = 0;
        await app.click(tab(app, 'second'));
        assert.equal(scrolled.second, 630, 'verse 10 30px above the top of the second pane’s view');
        await frame(app);

        // Back across 900px: both side by side, each at its place below its
        // bar again, as it was before the tabs.
        scrolled.first = 0;
        app.resize(1000);
        assert.deepEqual(tabsSeen(app).hidden, []);
        assert.equal(scrolled.second, 590, 'verse 10 30px above the second pane’s bar again');
        assert.equal(scrolled.first, 300, 'verse 5 40px above the first pane’s bar again');

        // Across both lines at once (1200px too): one change, both places
        // kept. (A window paints between resizes.)
        await frame(app);
        app.resize(1280);
        assert.deepEqual([scrolled.first, scrolled.second], [300, 590]);
        await frame(app);
        app.resize(700);
        assert.deepEqual(tabsSeen(app).hidden, ['first']);
        assert.equal(scrolled.second, 630);
        await frame(app);
        scrolled.first = 0;
        app.resize(1280);
        assert.deepEqual([scrolled.first, scrolled.second], [300, 590]);
    }));
