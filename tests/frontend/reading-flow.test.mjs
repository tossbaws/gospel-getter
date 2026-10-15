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
