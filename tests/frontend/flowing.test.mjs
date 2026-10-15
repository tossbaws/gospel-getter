// The Flowing layout (Aa → Layout): the chapter's verses run on as prose,
// in the chapter being read and its faded neighbors, while every
// verse-level feature keeps working; compare stays a verse to a row.
// Against the real backend and database.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { closeBridge, openApp, shownVerse } from './harness.mjs';

after(closeBridge);

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

async function clickAndWait(app, el, cmd = null) {
    const before = app.calls.length;
    app.click(el);
    await app.idle(before, cmd);
}

const FLOWING = { 'gospel-getter-layout': 'flowing' };
const $ = (app, id) => app.document.getElementById(id);
const main = (app) => app.document.querySelector('.chapter-main');
const card = (app) => $(app, 'selection-card');
const segments = (app) => [...$(app, 'layout-choices').querySelectorAll('button')];
const pressed = (app) => segments(app).filter((b) => b.getAttribute('aria-pressed') === 'true').map((b) => b.dataset.value);
const layoutOf = (app) => app.document.documentElement.dataset.layout || 'verses';

test('Aa → Layout switches between Verses and Flowing, live, and remembers it', async () => {
    let storage;
    await withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        // Verses is the default: nothing set, nothing stored.
        assert.equal(layoutOf(app), 'verses');
        assert.equal(app.storage()['gospel-getter-layout'], undefined);
        const texts = [...main(app).querySelectorAll('.chapter-text > .verse')].map((v) => v.textContent);

        app.open('aa');
        assert.deepEqual(segments(app).map((b) => b.textContent), ['Verses', 'Flowing']);
        assert.deepEqual(pressed(app), ['verses']);
        app.click(segments(app)[1]);
        assert.equal(layoutOf(app), 'flowing');
        assert.deepEqual(pressed(app), ['flowing']);
        assert.equal($(app, 'layout-select').value, 'flowing', 'the record the export reads');
        assert.equal(app.window.gospelGetterPreferences.current().layout, 'flowing');

        // The same verses, with the same text, in the same elements: only
        // how they're set changes. The neighbors flow too.
        assert.deepEqual([...main(app).querySelectorAll('.chapter-text > .verse')].map((v) => v.textContent), texts);
        for (const which of ['prev', 'next']) {
            assert.ok(app.document.querySelector(`.chapter-side-${which} .chapter-text > .verse`), which);
        }
        storage = app.storage();
        assert.equal(storage['gospel-getter-layout'], 'flowing');

        app.click(segments(app)[0]);
        assert.equal(layoutOf(app), 'verses');
        app.click(segments(app)[1]);
    });
    await withApp({ book: 'John', chapter: 3, storage }, async (app) => {
        assert.equal(layoutOf(app), 'flowing');
        app.open('aa');
        assert.deepEqual(pressed(app), ['flowing']);
    });
});

test('in Flowing the verses are one run of prose, a space between them', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv', storage: FLOWING }, async (app) => {
        const text = main(app).querySelector('.chapter-text');
        const verses = [...text.children];
        assert.ok(verses.length > 30 && verses.every((v) => v.classList.contains('verse')));
        // Between each verse and the next, whitespace (a space when set as
        // prose), so lines break between verses as between words.
        for (let i = 1; i < verses.length; i++) {
            const between = verses[i].previousSibling;
            assert.equal(between.nodeType, 3, `before verse ${i + 1}`);
            assert.match(between.textContent, /^\s+$/);
        }
        // Each verse is its number and its text, nothing added.
        const sixteen = app.verse(16);
        assert.equal(sixteen.querySelector('.verse-num').textContent, '16');
        assert.equal(sixteen.querySelector('.verse-text').textContent, (await shownVerse('kjv', 'John', 3, 16)).text);
        assert.equal(app.window.getComputedStyle(sixteen).display, 'inline');
    }));

test('selecting in Flowing tints the verses, puts nothing into the flow, and opens the card under them', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv', storage: FLOWING }, async (app) => {
        const text = main(app).querySelector('.chapter-text');
        const column = [...main(app).children];
        const before = [...text.childNodes];
        const html = [...text.children].map((el) => el.innerHTML);

        await clickAndWait(app, app.verse(16), 'list_notes');
        app.click(app.verse(18), { shiftKey: true });
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17, 18]);
        // The only new node is the card, straight after the last selected
        // verse, out of the flow; every verse's content is as it was.
        const after = [...text.childNodes];
        assert.deepEqual(after.filter((n) => !before.includes(n)), [card(app)]);
        assert.equal(card(app).previousElementSibling, app.verse(18));
        assert.equal(app.window.getComputedStyle(card(app)).position, 'absolute');
        assert.deepEqual([...text.children].filter((el) => el !== card(app)).map((el) => el.innerHTML), html);
        assert.deepEqual([...main(app).children], column);

        // Copy copies the verses, as in Verses.
        app.key('c', { ctrlKey: true });
        await app.idle(0);
        const kjv = (await Promise.all([16, 17, 18].map((n) => shownVerse('kjv', 'John', 3, n)))).map((v) => v.text).join(' ');
        assert.equal(app.clipboardWrites.at(-1), `${kjv} — John 3:16–18 (KJV)`);
    }));

// Lays out three selected verses as runs of lines, which jsdom can't: each
// verse's line boxes (getClientRects) and its whole box, `down` px lower
// than drawn here. Verse 18 ends mid-line on its last line.
function flowingLayout(app, { cardHeight = 200, down = 0 } = {}) {
    const { window, document } = app;
    const box = (top, bottom, left, right) => ({
        top, bottom, left, right, width: right - left, height: bottom - top, x: left, y: top,
    });
    const rect = (top, bottom, left, right) => box(top + down, bottom + down, left, right);
    Object.defineProperty(window, 'innerHeight', { configurable: true, value: 800 });
    document.getElementById('top-bar').getBoundingClientRect = () => box(0, 56, 0, 1280);
    main(app).getBoundingClientRect = () => box(-2000, 3000, 300, 800);
    const lines = {
        16: [rect(200, 230, 520, 800), rect(230, 260, 300, 800), rect(260, 290, 300, 420)],
        17: [rect(260, 290, 424, 800), rect(290, 320, 300, 610)],
        18: [rect(290, 320, 614, 800), rect(320, 350, 300, 800), rect(350, 380, 300, 470)],
    };
    for (const [n, boxes] of Object.entries(lines)) {
        const verse = app.verse(Number(n));
        verse.getClientRects = () => boxes;
        verse.getBoundingClientRect = () => box(boxes[0].top, boxes.at(-1).bottom, 300, 800);
    }
    for (const prop of ['scrollHeight', 'offsetHeight']) {
        Object.defineProperty(card(app), prop, { configurable: true, get: () => cardHeight });
    }
    window.dispatchEvent(new window.Event('resize'));
}

test('in Flowing the card sits under the last selected verse’s last line, from the column’s edge', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv', storage: FLOWING }, async (app) => {
        await clickAndWait(app, app.verse(16), 'list_notes');
        app.click(app.verse(18), { shiftKey: true });
        flowingLayout(app);
        assert.ok(!card(app).classList.contains('is-above'));
        // Under verse 18's last line (380), not its first (320).
        assert.equal(card(app).style.top, `${380 + 2000 + 8}px`);
        assert.equal(card(app).style.left, '0px');

        // Near the bottom: above verse 16's first line (600), never over
        // the selection.
        flowingLayout(app, { down: 400 });
        assert.ok(card(app).classList.contains('is-above'));
        assert.equal(card(app).style.top, `${600 + 2000 - 8 - 200}px`);
        assert.equal(card(app).style.left, '0px');
    }));

test('switching layout keeps the reader’s place and moves the card with its verses', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await clickAndWait(app, app.verse(16), 'list_notes');
        const { window, document } = app;
        // Verse 10 is the first below the bar: at 100px before, 340px after.
        const flowing = () => document.documentElement.dataset.layout === 'flowing';
        document.getElementById('top-bar').getBoundingClientRect = () => ({ top: 0, bottom: 56 });
        for (const verse of main(app).querySelectorAll('.chapter-text > .verse')) {
            const n = Number(verse.dataset.verse);
            verse.getBoundingClientRect = () => {
                const top = n < 10 ? -200 : (flowing() ? 340 : 100) + (n - 10) * 30;
                return { top, bottom: top + 20, left: 300, right: 800, width: 500, height: 20 };
            };
        }
        const scrolls = [];
        window.scrollBy = (x, y) => scrolls.push([x, y]);
        let placed = 0;
        document.addEventListener('gospel-getter:layout-change', () => { placed++; });

        app.open('aa');
        app.click(segments(app)[1]);
        assert.deepEqual(scrolls, [[0, 240]], 'verse 10 stays where it was on screen');
        assert.equal(placed, 1);
        assert.ok(card(app).isConnected, 'the selection and its card stay');
        assert.deepEqual(app.selectedVerseNumbers(), [16]);
    }));

test('compare stays a verse to a row in Flowing, and Aa says so', () =>
    withApp({ book: 'Psalms', chapter: 23, translation: 'web', storage: FLOWING }, async (app) => {
        assert.ok(main(app).querySelector('.chapter-text'));
        app.open('aa');
        assert.equal($(app, 'layout-note').hidden, true);
        assert.ok(segments(app).every((b) => !b.disabled));

        const before = app.calls.length;
        app.click($(app, 'compare-btn'));
        await app.idle(before, 'get_compare');
        assert.ok(app.isComparing());
        // The grid: no .chapter-text, so nothing flows; the layout is kept.
        assert.equal(main(app).querySelector('.chapter-text'), null);
        assert.ok(main(app).querySelector('.compare-grid > .verse[data-translation="kjv"]'));
        assert.equal(layoutOf(app), 'flowing');
        assert.equal($(app, 'layout-note').hidden, false);
        assert.equal($(app, 'layout-note').textContent, 'Compare always shows a verse to a row.');
        assert.ok(segments(app).every((b) => b.disabled));
        assert.deepEqual(pressed(app), ['flowing']);
        assert.equal($(app, 'layout-choices').getAttribute('aria-describedby'), 'layout-note');

        // Compare off: flowing again, and the row is live.
        const off = app.calls.length;
        app.click($(app, 'compare-btn'));
        await app.idle(off);
        assert.ok(main(app).querySelector('.chapter-text > .verse'));
        assert.equal($(app, 'layout-note').hidden, true);
        assert.ok(segments(app).every((b) => !b.disabled));
    }));

test('opening compare with C, with Aa closed, still updates the Layout row', () =>
    withApp({ book: 'Psalms', chapter: 23, translation: 'web', storage: FLOWING }, async (app) => {
        const before = app.calls.length;
        app.key('c');
        await app.idle(before, 'get_compare');
        app.open('aa');
        assert.ok(segments(app).every((b) => b.disabled));
        assert.equal($(app, 'layout-note').hidden, false);
    }));

test('the reading position comes back in Flowing, and a search result lands on its verse', async () => {
    await withApp({ book: 'Romans', chapter: 8, translation: 'web', storage: FLOWING }, async (app) => {
        // The saved chapter opens, set as prose.
        assert.equal(app.heading(), 'Romans 8');
        assert.equal(layoutOf(app), 'flowing');
        assert.ok(main(app).querySelector('.chapter-text > .verse[data-verse="28"]'));

        // A result scrolls its verse to the middle and selects it.
        const scrolled = [];
        app.window.Element.prototype.scrollIntoView = function (opts) { scrolled.push([this, opts]); };
        await app.search('called according to his purpose');
        const result = app.searchResults().find((r) => r.querySelector('.search-ref').textContent === 'Romans 8:28');
        assert.ok(result);
        const before = app.calls.length;
        app.click(result);
        await app.idle(before);
        assert.deepEqual(app.selectedVerseNumbers(), [28]);
        assert.equal(scrolled.length, 1);
        assert.equal(scrolled[0][0].dataset.verse, '28');
        assert.ok(scrolled[0][0].parentElement.classList.contains('chapter-text'));
        assert.equal(scrolled[0][1].block, 'center');
        assert.ok(card(app).isConnected);
    });
    // Reopened on the same install: the same chapter, still flowing.
    await withApp({ book: 'Romans', chapter: 8, translation: 'web', storage: FLOWING }, async (app) => {
        assert.equal(app.heading(), 'Romans 8');
        assert.equal(layoutOf(app), 'flowing');
    });
});

test('bookmarks, notes, highlights and KJV footnotes all show in Flowing', () =>
    withApp({ book: 'Psalms', chapter: 23, translation: 'kjv', storage: FLOWING }, async (app) => {
        // Footnote markers and supplied words are inline in the prose.
        assert.ok(app.verse(2).querySelector('.fn-marker'));
        assert.ok(app.verse(4).querySelector('em.supplied'));

        await clickAndWait(app, app.verse(1), 'list_notes');
        await clickAndWait(app, app.bookmarkButton(), 'list_bookmarks');
        assert.deepEqual(app.bookmarkedVerseNumbers(), [1]);

        const swatch = app.actions().querySelector('.highlight-swatch[data-color="green"]');
        await clickAndWait(app, swatch);
        assert.equal(app.verse(1).dataset.highlight, 'green');

        // A footnote marker selects its verse; the card has its footnotes.
        app.click(app.verse(5).querySelector('.fn-marker'));
        assert.deepEqual(app.selectedVerseNumbers(), [5]);
        assert.match(card(app).querySelector('.selection-footnotes').textContent, /Footnotes/);
    }));

test('a backup carries the layout, and an unknown one is refused before anything changes', () =>
    withApp({}, async (app) => {
        const prefs = app.window.gospelGetterPreferences;
        assert.equal(prefs.current().layout, 'verses');
        prefs.apply({ layout: 'flowing' });
        assert.equal(layoutOf(app), 'flowing');
        assert.equal(app.storage()['gospel-getter-layout'], 'flowing');
        assert.throws(() => prefs.apply({ layout: 'columns', theme: 'matrix' }), /isn’t a valid layout/);
        assert.equal(layoutOf(app), 'flowing');
        assert.notEqual(app.document.documentElement.dataset.theme, 'matrix');
    }));
