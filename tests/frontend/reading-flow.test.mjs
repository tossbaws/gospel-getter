// Behavior tests for the Tier 2 reading flow: the chapter ends with a next
// step (the chapters before and after it, as buttons) and its Footnotes
// and Cross references as disclosures that remember being opened.
// Against the real backend and database.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { closeBridge, openApp } from './harness.mjs';

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

const main = (app) => app.document.querySelector('.chapter-main');
const steps = (app) => [...main(app).querySelectorAll('.chapter-end .chapter-step')].map((b) => ({
    which: b.classList.contains('chapter-step-prev') ? 'prev' : 'next',
    text: b.textContent.replace(/\s+/g, ' ').trim(),
    label: b.getAttribute('aria-label'),
}));
const toggles = (app, kind) => [...app.document.querySelectorAll(`.disclosure-toggle[data-disclosure="${kind}"]`)];
const bodyOf = (app, toggle) => app.document.getElementById(toggle.getAttribute('aria-controls'));

// ---- The chapter ends with a next step

test('after the last verse come the chapters before and after, as buttons, then the hint', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        assert.deepEqual(steps(app), [
            { which: 'prev', text: '← John 2', label: 'John 2, previous chapter' },
            { which: 'next', text: 'John 4 →', label: 'John 4, next chapter' },
        ]);
        const end = main(app).querySelector('.chapter-end');
        assert.equal(end.tagName, 'NAV');
        // Straight after the verses, before the chapter's lists.
        const verses = main(app).querySelectorAll('.verse[data-translation]');
        assert.equal(end.previousElementSibling, verses[verses.length - 1]);
        assert.ok(end.nextElementSibling.classList.contains('nav-hint'));
        assert.match(end.nextElementSibling.textContent, /move a chapter at a time/);
        assert.equal(app.document.querySelectorAll('.nav-hint').length, 1, 'one hint, in the chapter');

        await clickAndWait(app, main(app).querySelector('.chapter-step-next'), 'get_reading');
        assert.equal(app.heading(), 'John 4');
        await clickAndWait(app, main(app).querySelector('.chapter-step-prev'), 'get_reading');
        assert.equal(app.heading(), 'John 3');
    }));

test('the buttons cross books, and the first and last chapters have only one', async () => {
    await withApp({ book: 'Genesis', chapter: 50, translation: 'kjv' }, async (app) => {
        assert.deepEqual(steps(app).map((s) => s.text), ['← Genesis 49', 'Exodus 1 →']);
        await clickAndWait(app, main(app).querySelector('.chapter-step-next'), 'get_reading');
        assert.equal(app.heading(), 'Exodus 1');
    });
    await withApp({ book: 'Genesis', chapter: 1, translation: 'web' }, async (app) => {
        assert.deepEqual(steps(app).map((s) => s.which), ['next']);
        assert.equal(steps(app)[0].text, 'Genesis 2 →');
    });
    await withApp({ book: 'Revelation', chapter: 22, translation: 'kjv' }, async (app) => {
        assert.deepEqual(steps(app).map((s) => s.which), ['prev']);
        assert.equal(steps(app)[0].text, '← Revelation 21');
    });
});

test('the buttons are there in reading mode and while comparing', async () => {
    await withApp({ book: 'John', chapter: 3, readerMode: true }, async (app) => {
        assert.equal(app.document.documentElement.dataset.readerMode, 'on');
        assert.deepEqual(steps(app).map((s) => s.text), ['← John 2', 'John 4 →']);
        await clickAndWait(app, main(app).querySelector('.chapter-step-next'), 'get_reading');
        assert.equal(app.heading(), 'John 4');
    });
    await withApp({ book: 'Psalms', chapter: 23, compare: true }, async (app) => {
        assert.ok(app.isComparing());
        assert.deepEqual(steps(app).map((s) => s.text), ['← Psalms 22', 'Psalms 24 →']);
        // After the passage being compared.
        assert.ok(main(app).querySelector('.chapter-end').previousElementSibling.classList.contains('compare-grid'));
        await clickAndWait(app, main(app).querySelector('.chapter-step-next'), 'get_compare');
        assert.equal(app.heading(), 'Psalms 24');
        assert.ok(app.isComparing());
    });
});

test('there is no tagline footer', () =>
    withApp({}, async (app) => {
        assert.equal(app.document.querySelector('footer'), null);
        assert.doesNotMatch(app.document.body.textContent, /whole counsel of God/);
    }));

// ---- Footnotes and Cross references are disclosures

test('Footnotes and Cross references start closed, with their counts', () =>
    withApp({ book: 'Psalms', chapter: 23, translation: 'kjv' }, async (app) => {
        const [footnotes] = toggles(app, 'footnotes');
        const [xrefs] = toggles(app, 'xrefs');
        assert.equal(footnotes.textContent, 'Footnotes (4)');
        assert.match(xrefs.textContent, /^Cross references \(\d+\)$/);
        const count = Number(/\((\d+)\)/.exec(xrefs.textContent)[1]);
        assert.equal(count, bodyOf(app, xrefs).querySelectorAll('.xref-summary-chip').length);
        for (const toggle of [footnotes, xrefs]) {
            assert.equal(toggle.tagName, 'BUTTON');
            assert.equal(toggle.getAttribute('aria-expanded'), 'false');
            assert.equal(bodyOf(app, toggle).hidden, true);
            // Not shown, though the lists are grids.
            assert.equal(app.window.getComputedStyle(bodyOf(app, toggle)).display, 'none');
        }
        // The cross-references' credit stays in view.
        assert.equal(app.document.querySelector('.xref-credit').closest('[hidden]'), null);
    }));

test('opening or closing one remembers it, for every chapter and after a restart', async () => {
    let storage;
    await withApp({ book: 'Psalms', chapter: 23, translation: 'kjv' }, async (app) => {
        app.click(toggles(app, 'footnotes')[0]);
        assert.equal(toggles(app, 'footnotes')[0].getAttribute('aria-expanded'), 'true');
        assert.equal(bodyOf(app, toggles(app, 'footnotes')[0]).hidden, false);
        assert.equal(bodyOf(app, toggles(app, 'xrefs')[0]).hidden, true, 'each kind on its own');

        await clickAndWait(app, main(app).querySelector('.chapter-step-next'), 'get_reading');
        assert.equal(app.heading(), 'Psalms 24');
        assert.equal(toggles(app, 'footnotes')[0].getAttribute('aria-expanded'), 'true', 'every chapter');
        assert.equal(bodyOf(app, toggles(app, 'footnotes')[0]).hidden, false);

        app.click(toggles(app, 'xrefs')[0]);
        app.click(toggles(app, 'footnotes')[0]);
        assert.equal(bodyOf(app, toggles(app, 'footnotes')[0]).hidden, true);
        storage = app.storage();
    });
    assert.equal(storage['gospel-getter-footnotes'], 'closed');
    assert.equal(storage['gospel-getter-cross-references'], 'open');
    await withApp({ book: 'Psalms', chapter: 23, translation: 'kjv', storage }, async (app) => {
        assert.equal(toggles(app, 'xrefs')[0].getAttribute('aria-expanded'), 'true');
        assert.equal(bodyOf(app, toggles(app, 'xrefs')[0]).hidden, false);
        assert.equal(bodyOf(app, toggles(app, 'footnotes')[0]).hidden, true);
    });
});

test('while comparing, both columns’ footnotes open and close together', () =>
    withApp({ book: 'Psalms', chapter: 23, compare: true, storage: { 'gospel-getter-footnotes': 'open' } }, async (app) => {
        const footnotes = toggles(app, 'footnotes');
        assert.deepEqual(footnotes.map((t) => t.textContent), ['KJV footnotes (4)']);
        assert.equal(bodyOf(app, footnotes[0]).hidden, false);
        app.click(footnotes[0]);
        assert.ok(toggles(app, 'footnotes').every((t) => bodyOf(app, t).hidden));
    }));

test('a chapter with neither shows neither', () =>
    withApp({ book: 'Psalms', chapter: 23, translation: 'web', alterReading: (dto) => ({ ...dto, hasChapterXrefs: false, currentVerses: dto.currentVerses.map((v) => ({ ...v, xrefs: [] })) }) }, async (app) => {
        assert.equal(app.document.querySelector('.disclosure-toggle'), null);
        assert.equal(app.document.querySelector('.footnote-summary, .xref-summary'), null);
        assert.ok(main(app).querySelector('.chapter-end'), 'the buttons still show');
    }));

test('printing opens both lists', () =>
    withApp({}, async (app) => {
        const css = [...app.document.querySelectorAll('style')].map((s) => s.textContent).join('\n');
        const print = css.slice(css.indexOf('@media print'));
        assert.match(print, /\.disclosure-body\[hidden\]\s*\{\s*display: block !important;/);
    }));

// ---- The selection card floats over the text

const card = (app) => app.document.getElementById('selection-card');

// Lays the page out for placeCard, which jsdom can't: the window, the top
// bar, the reading column, the selected verses and the card's height.
function layout(app, { innerHeight = 800, bar = 56, verses, cardHeight }) {
    const { window, document } = app;
    const rect = (top, bottom, left = 300, right = 800) => ({
        top, bottom, left, right, width: right - left, height: bottom - top, x: left, y: top,
    });
    Object.defineProperty(window, 'innerHeight', { configurable: true, value: innerHeight });
    document.getElementById('top-bar').getBoundingClientRect = () => rect(0, bar, 0, 1280);
    main(app).getBoundingClientRect = () => rect(-2000, 3000);
    for (const [n, [top, bottom]] of Object.entries(verses)) {
        app.verse(Number(n)).getBoundingClientRect = () => rect(top, bottom);
    }
    for (const prop of ['scrollHeight', 'offsetHeight']) {
        Object.defineProperty(card(app), prop, { configurable: true, get: () => cardHeight });
    }
    window.dispatchEvent(new window.Event('resize'));
}

test('selecting puts nothing into the column’s flow: the card floats, and the verses are untouched', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        const before = [...main(app).children];
        const html = before.map((el) => el.innerHTML);
        await clickAndWait(app, app.verse(16), 'list_notes');
        const after = [...main(app).children];
        // The only new element is the card, straight after the verse.
        assert.deepEqual(after.filter((el) => !before.includes(el)), [card(app)]);
        assert.equal(card(app).previousElementSibling, app.verse(16));
        assert.equal(app.window.getComputedStyle(card(app)).position, 'absolute');
        // Every verse's own content is exactly as it was.
        assert.deepEqual(before.map((el) => el.innerHTML), html);
        assert.equal(app.verse(16).querySelector('.verse-actions, #selection-card'), null);
        // Out of the flow, in every verse's offsetTop as far as jsdom can tell.
        assert.ok(before.every((el) => el.offsetTop === 0));
    }));

test('the card is a labelled group, next in Tab order after the selection', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(16));
        app.click(app.verse(18), { shiftKey: true });
        assert.equal(card(app).getAttribute('role'), 'group');
        assert.equal(card(app).getAttribute('aria-label'), 'Selection: John 3:16–18 (WEB)');
        // In the DOM right after the last selected verse, so Tab from the
        // selection lands on Copy.
        assert.equal(card(app).previousElementSibling, app.verse(18));
        assert.equal(card(app).querySelector('button'), card(app).querySelector('.verse-copy-btn'));
        assert.equal(card(app).firstElementChild, app.actions());
    }));

test('the card goes under the selection, flips above it near the bottom, and docks when neither fits', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        app.click(app.verse(16));
        app.click(app.verse(17), { shiftKey: true });
        // Room below: under the last selected verse.
        layout(app, { verses: { 16: [200, 260], 17: [268, 300] }, cardHeight: 240 });
        assert.ok(!card(app).classList.contains('is-above'));
        assert.equal(card(app).style.top, `${300 + 2000 + 8}px`);
        assert.equal(card(app).style.maxHeight, '');

        // Near the bottom of the window: above the first selected verse.
        layout(app, { verses: { 16: [600, 660], 17: [668, 720] }, cardHeight: 240 });
        assert.ok(card(app).classList.contains('is-above'));
        assert.equal(card(app).style.top, `${600 + 2000 - 8 - 240}px`);
        const bottom = Number.parseFloat(card(app).style.top) + 240 - 2000;
        assert.ok(bottom <= 600, 'it never covers the selection');

        // Too tall for either side: the roomier side, scrolling inside.
        layout(app, { verses: { 16: [380, 400], 17: [408, 420] }, cardHeight: 500 });
        assert.ok(!card(app).classList.contains('is-docked'));
        assert.equal(card(app).style.maxHeight, `${800 - 420 - 16}px`);

        // A selection filling the window: docked at its bottom.
        layout(app, { verses: { 16: [60, 400], 17: [408, 790] }, cardHeight: 300 });
        assert.ok(card(app).classList.contains('is-docked'));
    }));

test('Esc or a click away closes the card and returns focus to the verse', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await clickAndWait(app, app.verse(16), 'list_notes');
        const chip = card(app).querySelector('.xref-citation');
        chip.focus();
        assert.equal(app.document.activeElement, chip);
        app.key('Escape', {}, chip);
        assert.equal(card(app), null);
        assert.deepEqual(app.selectedVerseNumbers(), []);
        assert.equal(app.document.activeElement, app.verse(16));
        assert.equal(app.verse(16).getAttribute('tabindex'), '-1');

        await clickAndWait(app, app.verse(3), 'list_notes');
        card(app).querySelector('.verse-copy-btn').focus();
        app.click(app.document.querySelector('.chapter-main .chapter-heading'));
        assert.equal(card(app), null);
        assert.equal(app.document.activeElement, app.verse(3));

        // Deselecting by clicking the verse again closes it too.
        await clickAndWait(app, app.verse(5), 'list_notes');
        app.click(app.verse(5));
        assert.equal(card(app), null);
    }));

test('a chip reads that passage, selected', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await clickAndWait(app, app.verse(16), 'list_notes');
        const chip = [...card(app).querySelectorAll('.xref-citation')].find((c) => c.textContent === 'Romans 5:8');
        await clickAndWait(app, chip, 'get_reading');
        assert.equal(app.heading(), 'Romans 5');
        assert.deepEqual(app.selectedVerseNumbers(), [8]);
        assert.equal(card(app).previousElementSibling, app.verse(8));
    }));

test('the card works in reading mode', () =>
    withApp({ book: 'John', chapter: 3, readerMode: true }, async (app) => {
        await clickAndWait(app, app.verse(16), 'list_notes');
        assert.ok(card(app));
        assert.equal(card(app).previousElementSibling, app.verse(16));
        app.key('Escape');
        assert.equal(card(app), null);
        assert.equal(app.document.documentElement.dataset.readerMode, 'on', 'Esc closed the card first');
    }));
