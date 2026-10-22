// Behavior tests for the book and chapter picker and the reference button
// in the bar. The page is the chapter; books and chapters are in the
// picker, a sidebar down the left on a wide window (1200px and up; the
// reader can hide it, and that's remembered) and a popover from the
// reference button on a narrower one or in reading mode. The reference
// button always says which chapter is being read, and in which
// translation. Against the real backend and database.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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
const picker = (app) => $(app, 'picker');
const refBtn = (app) => $(app, 'ref-btn');
const refText = (app) => $(app, 'ref-text').textContent;
const book = (app, name) => [...picker(app).querySelectorAll('.book-item')].find((b) => b.textContent === name);
const chapters = (app) => [...picker(app).querySelectorAll('.chapter-btn')];
const chapter = (app, n) => chapters(app).find((b) => b.textContent === String(n));
const shownOnScreen = (app, el) => app.window.getComputedStyle(el).display !== 'none';
const marked = (app) => ({
    book: [...picker(app).querySelectorAll('.book-item[aria-current="true"]')].map((b) => b.textContent),
    chapter: [...picker(app).querySelectorAll('.chapter-btn[aria-current="page"]')].map((b) => b.textContent),
});

async function clickAndWait(app, el, cmd = null) {
    const before = app.calls.length;
    app.click(el);
    await app.idle(before, cmd);
}

async function keyAndWait(app, key, target) {
    const before = app.calls.length;
    app.key(key, {}, target);
    await app.idle(before);
}

// ---- The page and the reference button

test('the page is the chapter: no masthead, and the reference button says where the reader is', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        const { document } = app;
        assert.equal(document.querySelector('header'), null, 'no masthead');
        assert.equal(document.querySelector('.book-list, #chapter-list'), null, 'no book grid on the page');
        // Before the chapter, only a heading for screen readers and the
        // (hidden) notice line.
        const before = [];
        for (let el = $(app, 'reading-pane').previousElementSibling; el; el = el.previousElementSibling) before.push(el);
        assert.deepEqual(before.map((el) => el.tagName), ['P', 'H1']);
        assert.ok(before[0].hidden);
        assert.equal(before[1].className, 'visually-hidden');

        assert.equal(refText(app), 'John 3 · KJV');
        assert.equal(refBtn(app).getAttribute('aria-label'), 'John 3, King James Version. Choose a book and chapter');
        assert.equal(refBtn(app).getAttribute('aria-keyshortcuts'), 'G');
        assert.equal(refBtn(app).parentElement, $(app, 'top-bar'));
        assert.deepEqual(app.consoleErrors, []);
    }));

test('the reference follows navigation, the translation and compare', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await keyAndWait(app, 'ArrowRight');
        assert.equal(refText(app), 'John 4 · KJV');
        await keyAndWait(app, 'ArrowLeft');
        await keyAndWait(app, 'ArrowLeft');
        assert.equal(refText(app), 'John 2 · KJV');

        // Across a book boundary, with the picker following.
        app.click(book(app, 'Mark'));
        await clickAndWait(app, chapter(app, 16), 'get_reading');
        await keyAndWait(app, 'ArrowRight');
        assert.equal(app.heading(), 'Luke 1');
        assert.equal(refText(app), 'Luke 1 · KJV');
        assert.deepEqual(marked(app), { book: ['Luke'], chapter: ['1'] });

        // The translation, chosen with the translation button.
        app.open('translation');
        await clickAndWait(app, $(app, 'translation-choices').querySelector('[data-value="web"]'), 'get_reading');
        assert.equal(refText(app), 'Luke 1 · WEB');
        assert.equal(refBtn(app).getAttribute('aria-label'), 'Luke 1, World English Bible. Choose a book and chapter');

        // Comparing: both translations on screen, the one being read first.
        app.key('Escape', {}, app.document.activeElement);
        await keyAndWait(app, 'c');
        assert.ok(app.isComparing());
        assert.equal(refText(app), 'Luke 1 · WEB + KJV');
        assert.equal(refBtn(app).getAttribute('aria-label'), 'Luke 1, World English Bible and King James Version. Choose a book and chapter');
        await keyAndWait(app, 'c');
        assert.equal(refText(app), 'Luke 1 · WEB');

        // Back (Alt+←): to John 2, where the reader left for Mark 16 (the
        // picker's jump), in the translation now chosen.
        const before = app.calls.length;
        app.key('ArrowLeft', { altKey: true });
        await app.idle(before, 'get_reading');
        assert.equal(refText(app), 'John 2 · WEB');
        assert.deepEqual(app.consoleErrors, []);
    }));

// The backend copies the page's title to the native window (window.rs),
// so the taskbar and Alt-Tab say where the reader is.
test('the window title follows the reference: navigation, the translation and compare', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        const title = () => app.document.title;
        assert.equal(title(), 'John 3 · KJV — Gospel Getter');
        await keyAndWait(app, 'ArrowRight');
        assert.equal(title(), 'John 4 · KJV — Gospel Getter');

        app.click(book(app, 'Mark'));
        await clickAndWait(app, chapter(app, 16), 'get_reading');
        assert.equal(title(), 'Mark 16 · KJV — Gospel Getter');

        app.open('translation');
        await clickAndWait(app, $(app, 'translation-choices').querySelector('[data-value="web"]'), 'get_reading');
        assert.equal(title(), 'Mark 16 · WEB — Gospel Getter');

        // Comparing: what the reference button shows.
        app.key('Escape', {}, app.document.activeElement);
        await keyAndWait(app, 'c');
        assert.equal(refText(app), 'Mark 16 · WEB + KJV');
        assert.equal(title(), 'Mark 16 · WEB + KJV — Gospel Getter');
        await keyAndWait(app, 'ArrowLeft');
        assert.equal(title(), 'Mark 15 · WEB + KJV — Gospel Getter');
        await keyAndWait(app, 'c');
        assert.equal(title(), 'Mark 15 · WEB — Gospel Getter');

        // Back (Alt+←): to John 4, where the reader left for Mark 16.
        const before = app.calls.length;
        app.key('ArrowLeft', { altKey: true });
        await app.idle(before, 'get_reading');
        assert.equal(title(), 'John 4 · WEB — Gospel Getter');
    }));

test('before a chapter is read the window title is the app’s name', () => {
    const html = readFileSync(new URL('../../ui/index.html', import.meta.url), 'utf8');
    assert.match(html, /<title>Gospel Getter<\/title>/);
});

test('scrolling never changes the reference: it is the chapter in the center column', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        // The neighbors sit beside the chapter, not above or below it, so
        // nothing a scroll brings into view is another chapter's text in
        // the center.
        app.window.dispatchEvent(new app.window.Event('scroll'));
        await settle();
        assert.equal(refText(app), 'John 3 · KJV');
        assert.equal(app.document.querySelector('.chapter-main .chapter-heading').textContent, 'John 3');
    }));

// ---- The sidebar

test('on a wide window the picker is a sidebar: a navigation landmark with every book, the one being read open', () =>
    withApp({ book: 'John', chapter: 3, width: 1280 }, async (app) => {
        const root = app.document.documentElement;
        assert.equal(picker(app).hidden, false);
        assert.equal(picker(app).tagName, 'NAV');
        assert.equal(picker(app).getAttribute('role'), null, 'a navigation landmark, not a dialog');
        assert.equal(picker(app).getAttribute('aria-label'), 'Books and chapters');
        assert.equal(root.dataset.picker, 'sidebar');
        assert.equal(root.dataset.wide, '');
        assert.equal(refBtn(app).getAttribute('aria-haspopup'), null);
        assert.equal($(app, 'sidebar-toggle').getAttribute('aria-expanded'), 'true');
        assert.ok(shownOnScreen(app, $(app, 'sidebar-toggle')));

        assert.equal($(app, 'ot-book-list').querySelectorAll('li > .book-item').length, 39);
        assert.equal($(app, 'nt-book-list').querySelectorAll('li > .book-item').length, 27);
        assert.equal(picker(app).firstElementChild, $(app, 'random-chapter-btn'));
        assert.deepEqual(marked(app), { book: ['John'], chapter: ['3'] });
        const john = book(app, 'John');
        assert.equal(john.getAttribute('aria-expanded'), 'true');
        assert.equal(john.getAttribute('aria-controls'), 'picker-chapters');
        const grid = $(app, 'picker-chapters');
        assert.equal(john.nextElementSibling, grid);
        assert.equal(grid.getAttribute('role'), 'group');
        assert.equal(grid.getAttribute('aria-label'), 'John chapters');
        assert.equal(chapters(app).length, 21);
        assert.ok(!grid.classList.contains('is-compact'));
        assert.equal(picker(app).querySelectorAll('.book-item[aria-expanded="true"]').length, 1);
    }));

test('choosing a book shows its chapters; choosing a chapter reads it, and the sidebar stays', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        const before = app.calls.length;
        app.click(book(app, 'Genesis'));
        assert.equal(app.calls.length, before, 'choosing a book reads nothing yet');
        assert.equal(app.heading(), 'John 3');
        assert.equal(book(app, 'Genesis').getAttribute('aria-expanded'), 'true');
        assert.equal(book(app, 'John').getAttribute('aria-expanded'), 'false');
        assert.equal(book(app, 'John').getAttribute('aria-controls'), null);
        assert.equal(chapters(app).length, 50);
        assert.deepEqual(marked(app), { book: ['John'], chapter: [] }, 'still marking where the reader is');

        // (A real click focuses the button; jsdom's doesn't.)
        const two = chapter(app, 2);
        two.focus();
        await clickAndWait(app, two, 'get_reading');
        assert.equal(app.heading(), 'Genesis 2');
        assert.equal(picker(app).hidden, false, 'the sidebar stays');
        assert.deepEqual(marked(app), { book: ['Genesis'], chapter: ['2'] });
        assert.equal(app.document.activeElement, two, 'the chapters stay put, and focus with them');

        // Choosing the open book again hides its chapters.
        app.click(book(app, 'Genesis'));
        assert.equal(chapters(app).length, 0);
        assert.equal(book(app, 'Genesis').getAttribute('aria-expanded'), 'false');
        assert.deepEqual(app.consoleErrors, []);
    }));

test('Psalms, and every book over 50 chapters, gets a compact grid ten to a row', () =>
    withApp({ book: 'Psalms', chapter: 119 }, async (app) => {
        const grid = () => $(app, 'picker-chapters');
        assert.ok(grid().classList.contains('is-compact'));
        assert.equal(grid().dataset.columns, '10');
        assert.equal(chapters(app).length, 150);
        assert.deepEqual(marked(app), { book: ['Psalms'], chapter: ['119'] });
        for (const [name, compact] of [['Isaiah', true], ['Jeremiah', true], ['Genesis', false], ['Obadiah', false]]) {
            app.click(book(app, name));
            assert.equal(grid().classList.contains('is-compact'), compact, name);
            assert.equal(grid().dataset.columns, compact ? '10' : '6', name);
        }
        assert.equal(chapters(app).length, 1, 'Obadiah has one chapter');
    }));

// jsdom lays nothing out, so the cell widths are measured in WebKit and
// checked as arithmetic in frontend_markup.rs; here, the rules each grid
// gets, in the sidebar and in the popover.
test('the compact grid is roomier: it reaches into the picker’s padding, a hairline apart, in the sidebar and the popover', async () => {
    for (const width of [1280, 1000]) {
        await withApp({ book: 'Psalms', chapter: 119, width }, async (app) => {
            if (width < 1200) {
                app.click($(app, 'ref-btn'));
                assert.equal($(app, 'picker').getAttribute('role'), 'dialog', 'the popover');
            } else {
                assert.equal($(app, 'picker').getAttribute('role'), null, 'the sidebar');
            }
            const grid = $(app, 'picker-chapters');
            const style = app.window.getComputedStyle(grid);
            assert.equal(style.gridTemplateColumns, 'repeat(10, minmax(0, 1fr))');
            assert.equal(style.marginLeft, '-5.6px');
            assert.equal(style.marginRight, '-5.6px');
            assert.equal(style.getPropertyValue('gap'), '0.15rem 1px');
            assert.equal(grid.querySelectorAll('.chapter-btn').length, 150);
            assert.equal(app.window.getComputedStyle(grid.querySelector('.chapter-btn')).height, '27.2px', 'at least 24px tall');
        });
    }
});

test('the sidebar can be hidden and shown, is remembered, and keeps the reading place', async () => {
    let storage;
    await withApp({ book: 'John', chapter: 3 }, async (app) => {
        const { window, document } = app;
        const toggle = $(app, 'sidebar-toggle');
        // jsdom has no layout, so it's stubbed: the bar ends at 54px, and
        // verse 2 is the first thing below it, 100px down with the sidebar
        // and 160px down without it (the text above reflows). Hiding the
        // sidebar must scroll by the difference, so verse 2 stays put.
        $(app, 'top-bar').getBoundingClientRect = () => ({ top: 0, bottom: 54 });
        const verse2 = app.verse(2);
        verse2.getBoundingClientRect = () => {
            const top = document.documentElement.dataset.picker === 'sidebar' ? 100 : 160;
            return { top, bottom: top + 40 };
        };
        const scrolls = [];
        window.scrollBy = (x, y) => scrolls.push([x, y]);

        app.click(toggle);
        assert.equal(picker(app).hidden, true);
        assert.equal(document.documentElement.dataset.picker, undefined);
        assert.equal(toggle.getAttribute('aria-expanded'), 'false');
        assert.equal(refBtn(app).getAttribute('aria-expanded'), 'false');
        assert.equal(app.storage()['gospel-getter-sidebar'], 'closed');
        assert.deepEqual(scrolls, [[0, 60]], 'verse 2 stays 100px down');
        storage = app.storage();
    });
    await withApp({ book: 'John', chapter: 3, storage }, async (app) => {
        assert.equal(picker(app).hidden, true, 'still hidden after a restart');
        assert.equal(app.document.documentElement.dataset.sidebar, 'closed');
        // The reference button (or G) brings it back, into the chapter being read.
        app.click(refBtn(app));
        assert.equal(picker(app).hidden, false);
        assert.equal(app.storage()['gospel-getter-sidebar'], 'open');
        assert.equal(app.document.activeElement, chapter(app, 3));
        assert.equal(app.surface('picker').getAttribute('role'), null, 'still the sidebar, not a popover');
        assert.equal(app.window.gospelGetterSurfaces.current(), null);
        // Clicking it again doesn't hide it: it only ever opens the picker.
        app.click(refBtn(app));
        assert.equal(picker(app).hidden, false);
        app.click($(app, 'sidebar-toggle'));
        assert.equal(picker(app).hidden, true);
        app.click($(app, 'sidebar-toggle'));
        assert.equal(picker(app).hidden, false);
    });
});

// ---- The popover

test('on a narrow window the reference button opens a popover dialog; choosing a chapter reads it and closes it', () =>
    withApp({ book: 'John', chapter: 3, width: 900 }, async (app) => {
        const root = app.document.documentElement;
        assert.equal(root.dataset.picker, undefined);
        assert.equal(root.dataset.wide, undefined);
        assert.ok(!shownOnScreen(app, $(app, 'sidebar-toggle')), 'no sidebar button without room for the sidebar');
        assert.equal(picker(app).hidden, true);
        assert.equal(refBtn(app).getAttribute('aria-haspopup'), 'dialog');
        assert.equal(refBtn(app).getAttribute('aria-expanded'), 'false');

        app.click(refBtn(app));
        assert.equal(picker(app).hidden, false);
        assert.equal(picker(app).getAttribute('role'), 'dialog');
        assert.equal(refBtn(app).getAttribute('aria-expanded'), 'true');
        assert.equal(app.document.activeElement, chapter(app, 3), 'focus goes to the chapter being read');

        app.click(book(app, 'Romans'));
        assert.equal(picker(app).hidden, false, 'choosing a book keeps it open');
        await clickAndWait(app, chapter(app, 8), 'get_reading');
        assert.equal(app.heading(), 'Romans 8');
        assert.equal(refText(app), 'Romans 8 · KJV');
        assert.equal(picker(app).hidden, true, 'choosing a chapter closes it');
        assert.equal(refBtn(app).getAttribute('aria-expanded'), 'false');
        assert.equal(app.document.activeElement, refBtn(app), 'focus returns to the reference button');

        // The button closes it too, and so does a click outside.
        app.click(refBtn(app));
        app.click(refBtn(app));
        assert.equal(picker(app).hidden, true);
        app.click(refBtn(app));
        app.click(app.document.querySelector('.chapter-main .chapter-heading'));
        assert.equal(picker(app).hidden, true);
        assert.equal(app.document.activeElement, refBtn(app));
        assert.deepEqual(app.consoleErrors, []);
    }));

test('Escape closes the popover first, then the reader backs out as before', () =>
    withApp({ book: 'John', chapter: 3, width: 900 }, async (app) => {
        app.click(app.verse(16));
        assert.deepEqual(app.selectedVerseNumbers(), [16]);
        app.click(refBtn(app));
        app.key('Escape', {}, app.document.activeElement);
        assert.equal(picker(app).hidden, true);
        assert.equal(app.document.activeElement, refBtn(app));
        assert.deepEqual(app.selectedVerseNumbers(), [16], 'the selection waits for the next Escape');
        app.key('Escape', {}, app.document.activeElement);
        assert.deepEqual(app.selectedVerseNumbers(), []);

        // One surface at a time: Aa closes the picker, and the picker Aa.
        app.click(refBtn(app));
        app.click($(app, 'aa-toggle'));
        assert.equal(picker(app).hidden, true);
        assert.equal(app.surface('aa').hidden, false);
        app.click(refBtn(app));
        assert.equal(app.surface('aa').hidden, true);
        assert.equal(picker(app).hidden, false);
        app.key('/');
        assert.equal(picker(app).hidden, true, 'search closes it too');
    }));

test('resizing across 1200px swaps the sidebar for the popover and back', () =>
    withApp({ book: 'John', chapter: 3, width: 1280 }, async (app) => {
        const root = app.document.documentElement;
        chapter(app, 5).focus();
        app.resize(1199);
        assert.equal(root.dataset.picker, undefined);
        assert.equal(root.dataset.wide, undefined);
        assert.equal(picker(app).hidden, true);
        assert.equal(app.document.activeElement, refBtn(app), 'focus in the sidebar moves to the reference button');
        app.click(refBtn(app));
        assert.equal(picker(app).getAttribute('role'), 'dialog');
        app.resize(1200);
        assert.equal(app.window.gospelGetterSurfaces.current(), null, 'the popover closed');
        assert.equal(root.dataset.picker, 'sidebar');
        assert.equal(picker(app).hidden, false);
        assert.equal(picker(app).getAttribute('role'), null);
    }));

test('resizing across 1200px keeps the reading place, as the sidebar button does', () =>
    withApp({ book: 'John', chapter: 3, width: 1280 }, async (app) => {
        const { window, document } = app;
        // Stubbed layout, as for the sidebar button: the bar ends at 54px,
        // and verse 2 is the first thing below it, 100px down with the
        // sidebar and 160px down without it.
        $(app, 'top-bar').getBoundingClientRect = () => ({ top: 0, bottom: 54 });
        app.verse(2).getBoundingClientRect = () => {
            const top = document.documentElement.dataset.picker === 'sidebar' ? 100 : 160;
            return { top, bottom: top + 40 };
        };
        const scrolls = [];
        window.scrollBy = (x, y) => scrolls.push([x, y]);

        app.resize(1199);
        assert.equal(document.documentElement.dataset.picker, undefined);
        assert.deepEqual(scrolls, [[0, 60]], 'verse 2 stays 100px down');
        // Widening with the popover open: it closes, and the place stays.
        app.click(refBtn(app));
        assert.equal(app.window.gospelGetterSurfaces.current(), 'picker');
        app.resize(1200);
        assert.equal(app.window.gospelGetterSurfaces.current(), null);
        assert.equal(document.documentElement.dataset.picker, 'sidebar');
        assert.deepEqual(scrolls, [[0, 60], [0, -60]], 'verse 2 stays 160px down');
        // Narrowing again keeps it again; a resize that stays on one side
        // of 1200px is left to the browser.
        app.resize(1000);
        assert.deepEqual(scrolls, [[0, 60], [0, -60], [0, 60]]);
        app.resize(900);
        assert.deepEqual(scrolls, [[0, 60], [0, -60], [0, 60]], 'nothing more');
        assert.deepEqual(app.consoleErrors, []);
    }));

// Waits for the page's next animation frame, after the ones it asked for.
const frame = (app) => new Promise((resolve) => app.window.requestAnimationFrame(() => resolve()));

// Stubbed layout for a window resized across 1200px, as WebKitGTK lays it
// out: the bar ends at 54px, and the reading column is as wide with the
// sidebar at 1280px as without it at 1000px. When the 1200px media query
// tells the page, it's already laid out at the new width in the old shape
// (`width` is that new width): a layout the reader never sees, with
// another verse first under the bar.
function crossingLayout(app) {
    const layout = { width: 1280 };
    const tops = {
        2: { 'wide-sidebar': -202, 'narrow-sidebar': 55, 'narrow-popover': -202, 'wide-popover': -400 },
        6: { 'wide-sidebar': 30, 'narrow-sidebar': 300, 'narrow-popover': 30, 'wide-popover': -150 },
        8: { 'wide-sidebar': 500, 'narrow-sidebar': 500, 'narrow-popover': 500, 'wide-popover': 60 },
    };
    $(app, 'top-bar').getBoundingClientRect = () => ({ top: 0, bottom: 54 });
    for (const [n, at] of Object.entries(tops)) {
        app.verse(Number(n)).getBoundingClientRect = () => {
            const shape = app.document.documentElement.dataset.picker === 'sidebar' ? 'sidebar' : 'popover';
            const top = at[`${layout.width >= 1200 ? 'wide' : 'narrow'}-${shape}`];
            return { top, bottom: top + 40 };
        };
    }
    return layout;
}

test('resizing across 1200px keeps the place the reader saw, not one laid out on the way', () =>
    withApp({ book: 'John', chapter: 3, width: 1280 }, async (app) => {
        const { window } = app;
        const layout = crossingLayout(app);
        const scrolls = [];
        window.scrollBy = (x, y) => scrolls.push([x, y]);
        // The reader sees verse 6, 30px down.
        window.dispatchEvent(new window.Event('scroll'));
        await frame(app);

        // On the way to 1000px, verse 2 was first under the bar, at 55px,
        // and the sidebar's going takes it to -202px; measured there, the
        // page would jump 257px back to it. Verse 6 is still 30px down.
        layout.width = 1000;
        app.resize(1000);
        assert.equal(app.document.documentElement.dataset.picker, undefined);
        assert.deepEqual(scrolls, [], 'verse 6 stays 30px down');
        await frame(app);

        // Widening with the popover open: on the way, verse 8 was first.
        app.click(refBtn(app));
        assert.equal(window.gospelGetterSurfaces.current(), 'picker');
        layout.width = 1280;
        app.resize(1280);
        assert.equal(window.gospelGetterSurfaces.current(), null);
        assert.equal(app.document.documentElement.dataset.picker, 'sidebar');
        assert.deepEqual(scrolls, [], 'verse 6 stays 30px down again');
        assert.deepEqual(app.consoleErrors, []);
    }));

test('resizing across 1200px with no place seen yet, or one no longer on the page, keeps the place found then', () =>
    withApp({ book: 'John', chapter: 3, width: 1280 }, async (app) => {
        const { window } = app;
        const layout = crossingLayout(app);
        const scrolls = [];
        window.scrollBy = (x, y) => scrolls.push([x, y]);
        // No frame has seen the stubbed layout: verse 2, first at 55px on
        // the way, stays there.
        layout.width = 1000;
        app.resize(1000);
        assert.deepEqual(scrolls, [[0, -257]]);

        // Verse 6 was seen, then left the page (a new chapter took it).
        layout.width = 1280;
        app.resize(1280);
        window.dispatchEvent(new window.Event('scroll'));
        await frame(app);
        app.verse(6).remove();
        scrolls.length = 0;
        layout.width = 1000;
        app.resize(1000);
        assert.deepEqual(scrolls, [[0, -257]]);
        assert.deepEqual(app.consoleErrors, []);
    }));

// ---- The keyboard

test('G opens the picker, but not while typing', async () => {
    await withApp({ book: 'John', chapter: 3, width: 900 }, async (app) => {
        app.key('g');
        assert.equal(picker(app).hidden, false);
        assert.equal(app.document.activeElement, chapter(app, 3));
        app.key('g', {}, app.document.activeElement);
        assert.equal(picker(app).hidden, false, 'G again leaves it open');
        app.key('Escape', {}, app.document.activeElement);
        assert.equal(picker(app).hidden, true);
        app.key('G');
        assert.equal(picker(app).hidden, false, 'Caps Lock G');
        app.key('Escape', {}, app.document.activeElement);

        // Not with a modifier, and not in a text field.
        app.key('g', { ctrlKey: true });
        app.key('g', { altKey: true });
        assert.equal(picker(app).hidden, true);
        app.key('/');
        const input = app.searchInput();
        const event = app.key('g', {}, input);
        assert.equal(event.defaultPrevented, false, 'the g is typed');
        assert.equal(picker(app).hidden, true);
        app.key('Escape', {}, input);

        // Not in a note either.
        app.click(app.verse(16));
        app.key('n');
        const note = $(app, 'note-input');
        assert.equal($(app, 'note-overlay').hidden, false);
        app.key('g', {}, note);
        assert.equal(picker(app).hidden, true);
    });
    await withApp({ book: 'John', chapter: 3, width: 1440, storage: { 'gospel-getter-sidebar': 'closed' } }, async (app) => {
        app.key('g');
        assert.equal(picker(app).hidden, false, 'G shows a hidden sidebar');
        assert.equal(app.storage()['gospel-getter-sidebar'], 'open');
        assert.equal(app.document.activeElement, chapter(app, 3));
    });
});

test('arrow keys move around the picker and never turn the chapter', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        const press = (key) => {
            const event = app.key(key, {}, app.document.activeElement);
            assert.ok(event.defaultPrevented, key);
            return app.document.activeElement;
        };
        chapter(app, 3).focus();
        assert.equal(press('ArrowRight'), chapter(app, 4));
        assert.equal(press('ArrowDown'), chapter(app, 10), 'six to a row');
        assert.equal(press('ArrowDown'), chapter(app, 16));
        assert.equal(press('ArrowDown'), chapter(app, 21), 'into the short last row');
        assert.equal(press('ArrowDown'), book(app, 'Acts'), 'then the next book');
        assert.equal(press('ArrowUp'), chapter(app, 21), 'back into the open book');
        assert.equal(press('Home'), chapter(app, 1));
        assert.equal(press('ArrowLeft'), chapter(app, 1), 'nothing before chapter 1');
        assert.equal(press('End'), chapter(app, 21));
        chapter(app, 2).focus();
        assert.equal(press('ArrowUp'), book(app, 'John'), 'up from the first row is the book');
        assert.equal(press('ArrowUp'), book(app, 'Luke'));
        assert.equal(press('ArrowRight'), book(app, 'Luke'), '→ shows its chapters');
        assert.equal(book(app, 'Luke').getAttribute('aria-expanded'), 'true');
        assert.equal(press('ArrowRight'), chapter(app, 1), '→ again goes into them');
        assert.equal(app.document.getElementById('picker-chapters').dataset.bookId, '42');
        assert.equal(press('ArrowUp'), book(app, 'Luke'));
        assert.equal(press('ArrowLeft'), book(app, 'Luke'), '← hides them');
        assert.equal(book(app, 'Luke').getAttribute('aria-expanded'), 'false');
        assert.equal(press('ArrowDown'), book(app, 'John'));
        assert.equal(press('Home'), $(app, 'random-chapter-btn'));
        assert.equal(press('ArrowDown'), book(app, 'Genesis'));
        assert.equal(press('End'), book(app, 'Revelation'));

        // Psalms: ten to a row.
        app.click(book(app, 'Psalms'));
        chapter(app, 1).focus();
        assert.equal(press('ArrowDown'), chapter(app, 11));
        await settle();
        assert.equal(app.heading(), 'John 3', 'no arrow turned the chapter');
        assert.equal(app.calls.filter((c) => c.cmd === 'get_reading').length, 1);

        // Enter (a click, on a button) reads the chapter.
        await clickAndWait(app, chapter(app, 11), 'get_reading');
        assert.equal(app.heading(), 'Psalms 11');
    }));

// ---- Random chapter

test('Random chapter is the first row of the picker, and reads a chapter it picks', () =>
    withApp({ book: 'John', chapter: 3, width: 900 }, async (app) => {
        app.click(refBtn(app));
        const random = $(app, 'random-chapter-btn');
        assert.equal(picker(app).firstElementChild, random);
        assert.equal(random.textContent.trim(), 'Random chapter');
        // The last book, its last chapter.
        app.window.Math.random = () => 0.999999;
        await clickAndWait(app, random, 'get_reading');
        assert.equal(app.heading(), 'Revelation 22');
        assert.equal(refText(app), 'Revelation 22 · KJV');
        assert.equal(picker(app).hidden, true, 'the popover closes');
        assert.equal(app.document.activeElement, refBtn(app));
        assert.deepEqual(marked(app), { book: ['Revelation'], chapter: ['22'] });
    }));

// ---- Reading mode

test('reading mode hides the sidebar; the reference button opens the popover instead', () =>
    withApp({ book: 'Psalms', chapter: 23, readerMode: true, width: 1440 }, async (app) => {
        const root = app.document.documentElement;
        assert.equal(picker(app).hidden, true);
        assert.equal(root.dataset.picker, undefined);
        assert.ok(!shownOnScreen(app, $(app, 'sidebar-toggle')));
        assert.ok(shownOnScreen(app, refBtn(app)));
        assert.equal(refText(app), 'Psalms 23 · KJV');

        app.key('g');
        assert.equal(picker(app).hidden, false);
        assert.equal(picker(app).getAttribute('role'), 'dialog');
        assert.equal(app.document.activeElement, chapter(app, 23));
        await clickAndWait(app, chapter(app, 24), 'get_reading');
        assert.equal(app.heading(), 'Psalms 24');
        assert.equal(picker(app).hidden, true);
        assert.equal(root.dataset.readerMode, 'on');

        // Escape: the popover, then reading mode, which brings the sidebar back.
        app.click(refBtn(app));
        app.key('Escape', {}, app.document.activeElement);
        assert.equal(picker(app).hidden, true);
        assert.equal(root.dataset.readerMode, 'on');
        app.key('Escape', {}, app.document.activeElement);
        assert.equal(root.dataset.readerMode, undefined);
        assert.equal(root.dataset.picker, 'sidebar');
        assert.equal(picker(app).hidden, false);
        assert.equal(picker(app).getAttribute('role'), null);
    }));
