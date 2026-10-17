// Behavior tests for the Tier 2 reading flow: the chapter ends with a next
// step (the chapters before and after it, as buttons) and its Footnotes
// and Cross references as disclosures that remember being opened.
// Against the real backend and database.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { closeBridge, openApp } from './harness.mjs';

const dir = mkdtempSync(join(tmpdir(), 'gospel-getter-reading-flow-'));
after(async () => {
    await closeBridge();
    rmSync(dir, { recursive: true, force: true });
});

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
        assert.equal(end.previousElementSibling, main(app).querySelector('.chapter-text'));
        assert.equal(end.previousElementSibling.lastElementChild, verses[verses.length - 1]);
        assert.ok(end.nextElementSibling.classList.contains('nav-hint'));
        const hint = end.nextElementSibling;
        assert.equal(hint.textContent.replace(/\s+/g, ' '), 'Use Left arrow and Right arrow to move a chapter at a time, and ? for all keyboard shortcuts.');
        // Three key caps alike; the arrows drawn, their names for screen readers.
        assert.deepEqual([...hint.querySelectorAll('kbd')].map((k) => k.textContent), ['Left arrow', 'Right arrow', '?']);
        assert.equal(hint.querySelectorAll('kbd svg[aria-hidden="true"]').length, 2);
        assert.ok([...hint.querySelectorAll('kbd .visually-hidden')].every((n) => n.nextElementSibling.tagName === 'svg'));
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
        const column = [...main(app).children];
        const text = main(app).querySelector('.chapter-text');
        const before = [...text.children];
        const html = before.map((el) => el.innerHTML);
        await clickAndWait(app, app.verse(16), 'list_notes');
        const after = [...text.children];
        // The only new element is the card, straight after the verse; the
        // column around the verses is as it was.
        assert.deepEqual(after.filter((el) => !before.includes(el)), [card(app)]);
        assert.deepEqual([...main(app).children], column);
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

test('a chip’s Go to reads that passage, selected', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await clickAndWait(app, app.verse(16), 'list_notes');
        const chip = [...card(app).querySelectorAll('.xref-citation')].find((c) => c.textContent === 'Romans 5:8');
        await clickAndWait(app, chip, 'get_passage');
        await clickAndWait(app, card(app).querySelector('.xref-preview-go'), 'get_reading');
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

// ---- Width, and the text size keys

const $ = (app, id) => app.document.getElementById(id);
const widthButton = (app, label) => [...$(app, 'width-choices').querySelectorAll('button')].find((b) => b.textContent === label);

test('Width is a row in Aa: Narrow, Normal and Wide, Normal by default', () =>
    withApp({}, async (app) => {
        app.open('aa');
        const row = $(app, 'width-choices');
        assert.equal(row.getAttribute('role'), 'group');
        assert.equal($(app, row.getAttribute('aria-labelledby')).textContent, 'Width');
        assert.deepEqual([...row.querySelectorAll('button')].map((b) => [b.textContent, b.getAttribute('aria-pressed')]),
            [['Narrow', 'false'], ['Normal', 'true'], ['Wide', 'false']]);
        assert.equal(app.document.documentElement.dataset.width, undefined, 'Normal is no attribute: nothing changes');
    }));

test('choosing a width applies it, and it’s remembered after a restart', async () => {
    let storage;
    await withApp({}, async (app) => {
        app.open('aa');
        app.click(widthButton(app, 'Wide'));
        assert.equal(app.document.documentElement.dataset.width, 'wide');
        assert.equal(widthButton(app, 'Wide').getAttribute('aria-pressed'), 'true');
        assert.equal($(app, 'width-select').value, 'wide');
        storage = app.storage();
    });
    assert.equal(storage['gospel-getter-width'], 'wide');
    await withApp({ storage }, async (app) => {
        assert.equal(app.document.documentElement.dataset.width, 'wide');
        app.click(widthButton(app, 'Narrow'));
        assert.equal(app.document.documentElement.dataset.width, 'narrow');
        app.click(widthButton(app, 'Normal'));
        assert.equal(app.document.documentElement.dataset.width, undefined);
    });
});

test('Width survives a backup: exported, then imported into a fresh window', async () => {
    const path = join(dir, 'width.gospel-getter.json');
    await withApp({ storage: { 'gospel-getter-width': 'narrow' } }, async (app) => {
        await app.pick(path);
        app.open('menu');
        const before = app.calls.length;
        app.click($(app, 'export-data-btn'));
        await app.idle(before);
    });
    assert.equal(JSON.parse(readFileSync(path, 'utf8')).preferences.width, 'narrow');
    await withApp({}, async (app) => {
        assert.equal(app.document.documentElement.dataset.width, undefined);
        await app.pick(path);
        app.open('menu');
        let before = app.calls.length;
        app.click($(app, 'import-data-btn'));
        await app.idle(before);
        assert.match($(app, 'import-overlay').textContent, /narrow width/);
        before = app.calls.length;
        app.click($(app, 'import-confirm'));
        await app.idle(before);
        assert.equal(app.document.documentElement.dataset.width, 'narrow');
        assert.equal(app.storage()['gospel-getter-width'], 'narrow');
        assert.equal($(app, 'width-select').value, 'narrow');
    });
});

const size = (app) => $(app, 'text-size-select').value;
const ctrl = (app, key, opts = {}, target) => app.key(key, { ctrlKey: true, ...opts }, target);

test('Ctrl+= and Ctrl+- step through the text sizes, clamped at the ends, and Ctrl+0 goes back to Medium', () =>
    withApp({}, async (app) => {
        assert.equal(size(app), 'medium');
        const status = $(app, 'size-status');
        assert.equal(status.getAttribute('role'), 'status');
        assert.equal(status.getAttribute('aria-live'), 'polite');
        assert.ok(status.classList.contains('is-hidden'));

        const first = ctrl(app, '=');
        assert.ok(first.defaultPrevented, 'never the page zoom');
        assert.equal(size(app), 'large');
        assert.equal(app.document.documentElement.dataset.textSize, 'large');
        assert.equal(status.textContent, 'Text size: Large');
        assert.ok(!status.classList.contains('is-hidden'));
        // Aa shows the same size.
        assert.equal($(app, 'text-size-value').textContent, 'Large');

        ctrl(app, '+', { shiftKey: true });
        assert.equal(size(app), 'x-large');
        assert.ok(ctrl(app, '+', { code: 'NumpadAdd' }).defaultPrevented);
        assert.equal(size(app), 'x-large', 'clamped at the largest');
        assert.equal(status.textContent, 'Text size: Extra large');

        for (const expected of ['large', 'medium', 'small', 'small']) {
            assert.ok(ctrl(app, '-').defaultPrevented);
            assert.equal(size(app), expected);
        }
        ctrl(app, '-', { code: 'NumpadSubtract' });
        assert.equal(size(app), 'small', 'clamped at the smallest');
        assert.equal(status.textContent, 'Text size: Small');

        ctrl(app, '0');
        assert.equal(size(app), 'medium');
        assert.equal(app.document.documentElement.dataset.textSize, undefined);
        assert.equal(app.storage()['gospel-getter-text-size'], 'medium');
        ctrl(app, '=', { code: 'NumpadAdd', key: '+' });
        assert.equal(size(app), 'large');
        ctrl(app, '0', { code: 'Numpad0' });
        assert.equal(size(app), 'medium');

        // Without Ctrl, or with Alt, they're nothing to do with size.
        app.key('=');
        app.key('-', { ctrlKey: true, altKey: true });
        assert.equal(size(app), 'medium');
    }));

test('the size keys do nothing while typing in a field', () =>
    withApp({}, async (app) => {
        app.key('/');
        const input = app.searchInput();
        const event = ctrl(app, '=', {}, input);
        assert.ok(event.defaultPrevented, 'still no page zoom');
        assert.equal(size(app), 'medium');
        ctrl(app, '-', {}, input);
        ctrl(app, '0', {}, input);
        assert.equal(size(app), 'medium');
        assert.ok($(app, 'size-status').classList.contains('is-hidden'));
    }));

// ---- Genesis 1 and Revelation 22

const edge = (app, which) => app.document.querySelector(`.chapter-side-${which} .edge-card`);

test('Genesis 1 has a quiet start card where the previous chapter would be', () =>
    withApp({ book: 'Genesis', chapter: 1, translation: 'kjv' }, async (app) => {
        const card = edge(app, 'prev');
        assert.ok(card, 'in the previous chapter’s place, so the column stays put');
        assert.deepEqual([...card.children].map((p) => [p.className, p.textContent]), [
            ['edge-card-title', 'The Old Testament'],
            ['edge-card-detail', 'Genesis · 50 chapters'],
        ]);
        // Presentational: no verses, nothing to click.
        assert.equal(card.closest('.chapter-side').querySelectorAll('.verse, button, a').length, 0);
        assert.equal(edge(app, 'next'), null);
        assert.ok(app.document.querySelector('.chapter-side-next .chapter-heading-link'));
    }));

test('Revelation 22 has “The end of the Bible” where the next chapter would be', () =>
    withApp({ book: 'Revelation', chapter: 22, translation: 'web' }, async (app) => {
        const card = edge(app, 'next');
        assert.deepEqual([...card.children].map((p) => p.textContent), ['The end of the Bible']);
        assert.equal(edge(app, 'prev'), null);
    }));

test('other chapters have no edge card, and comparing shows none', async () => {
    await withApp({ book: 'Malachi', chapter: 4, translation: 'kjv' }, async (app) => {
        assert.equal(app.document.querySelector('.edge-card'), null);
    });
    await withApp({ book: 'Genesis', chapter: 1, compare: true }, async (app) => {
        assert.equal(app.document.querySelector('.edge-card, .chapter-side'), null);
    });
});
