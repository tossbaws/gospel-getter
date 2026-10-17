// Behavior tests for cross-references: a chip in the selection card
// previews its passage in the card, and Go to opens it; a citation in the
// chapter's Cross references list opens its passage. Against the real
// backend and database.

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

const card = (app) => app.document.getElementById('selection-card');
const preview = (app) => app.document.getElementById('xref-preview');
const chips = (app) => [...card(app).querySelectorAll('.xref-citation')];
const chip = (app, citation, row = 0) => {
    const found = [...card(app).querySelectorAll('.selection-xref-row')][row]
        .querySelectorAll('.xref-citation');
    const el = [...found].find((c) => c.textContent === citation);
    assert.ok(el, `fixture: a ${citation} chip in row ${row}`);
    return el;
};
const shown = (app) => (preview(app).hidden ? null : {
    title: preview(app).querySelector('.xref-preview-title').textContent,
    numbers: [...preview(app).querySelectorAll('.xref-preview-num')].map((n) => Number(n.textContent)),
    notice: preview(app).querySelector('.xref-preview-notice')?.textContent ?? null,
    go: preview(app).querySelector('.xref-preview-go').textContent,
});
const pressed = (app) => chips(app).filter((c) => c.getAttribute('aria-expanded') === 'true').map((c) => c.textContent);
const where = (app) => [app.heading(), app.selectedVerseNumbers()];

test('a chip shows its passage in the card, under its row, and stays in the chapter', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await clickAndWait(app, app.verse(16), 'list_notes');
        const romans = chip(app, 'Romans 5:8');
        assert.equal(romans.tagName, 'BUTTON');
        assert.equal(romans.getAttribute('aria-expanded'), 'false');
        assert.equal(romans.getAttribute('aria-controls'), 'xref-preview');
        await clickAndWait(app, romans, 'get_passage');

        assert.deepEqual(where(app), ['John 3', [16]], 'nothing navigated');
        assert.equal(app.calls.filter((c) => c.cmd === 'get_reading').length, 1, 'only the first reading');
        assert.deepEqual(shown(app), {
            title: 'Romans 5:8 (KJV)',
            numbers: [8],
            notice: null,
            go: 'Go to Romans 5:8 →',
        });
        assert.deepEqual(pressed(app), ['Romans 5:8']);
        // Under the chips of its row, in the card.
        assert.equal(preview(app).parentElement, romans.closest('.selection-xref-row'));
        assert.equal(preview(app).previousElementSibling, romans.closest('.xref-chips'));
        assert.equal(preview(app).getAttribute('role'), 'region');
        assert.equal(preview(app).getAttribute('aria-labelledby'), 'xref-preview-title');
        // The text as the reader shows it: Romans 5:8 has no markup.
        const verse = await shownVerse('kjv', 'Romans', 5, 8);
        assert.equal(preview(app).querySelector('.xref-preview-text').textContent, `8${verse.text}`);
        // The arrow isn't read out.
        assert.equal(preview(app).querySelector('.xref-preview-go [aria-hidden="true"]').textContent, '→');
    }));

test('a range previews every verse in it, each with its number, as prose', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        await clickAndWait(app, app.verse(16), 'list_notes');
        await clickAndWait(app, chip(app, '1 John 4:9-10'), 'get_passage');
        assert.deepEqual(shown(app), {
            title: '1 John 4:9–10 (WEB)',
            numbers: [9, 10],
            notice: null,
            go: 'Go to 1 John 4:9–10 →',
        });
        const [nine, ten] = await Promise.all([9, 10].map((n) => shownVerse('web', '1 John', 4, n)));
        assert.equal(preview(app).querySelector('.xref-preview-text').textContent, `9${nine.text} 10${ten.text}`);
        // One paragraph: the verses run on.
        assert.equal(preview(app).querySelectorAll('.xref-preview-text').length, 1);
        assert.equal(preview(app).querySelector('.xref-preview-text').tagName, 'P');
    }));

test('one preview at a time, across rows, and the same chip closes it', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        app.click(app.verse(16));
        app.click(app.verse(17), { shiftKey: true });
        await app.idle(0, 'list_notes');
        assert.equal(card(app).querySelectorAll('.selection-xref-row').length, 2);
        const first = chip(app, 'Romans 5:8', 0);
        await clickAndWait(app, first, 'get_passage');
        assert.equal(preview(app).parentElement, first.closest('.selection-xref-row'));

        // A chip in the other row replaces it.
        const other = card(app).querySelectorAll('.selection-xref-row')[1].querySelector('.xref-citation');
        await clickAndWait(app, other, 'get_passage');
        assert.equal(app.document.querySelectorAll('.xref-preview').length, 1);
        assert.equal(preview(app).parentElement, other.closest('.selection-xref-row'));
        assert.deepEqual(pressed(app), [other.textContent]);
        assert.equal(first.getAttribute('aria-expanded'), 'false');

        // The same chip again closes it.
        app.click(other);
        await app.idle(0);
        assert.equal(shown(app), null);
        assert.deepEqual(pressed(app), []);
        assert.deepEqual(where(app), ['John 3', [16, 17]]);
    }));

test('Go to opens the passage, selected, and Back returns', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        await clickAndWait(app, app.verse(16), 'list_notes');
        await clickAndWait(app, chip(app, '1 John 4:9-10'), 'get_passage');
        await clickAndWait(app, preview(app).querySelector('.xref-preview-go'), 'get_reading');
        assert.deepEqual(where(app), ['1 John 4', [9, 10]]);
        assert.equal(card(app).previousElementSibling, app.verse(10));
        // The new card's chips start closed.
        assert.deepEqual(pressed(app), []);
        assert.equal(shown(app), null);
        // Back (Alt+←): John 3 again, as it was left.
        const before = app.calls.length;
        app.key('ArrowLeft', { altKey: true });
        await app.idle(before, 'get_reading');
        assert.equal(app.heading(), 'John 3');
    }));

test('Esc closes the preview first, then the card', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv', readerMode: true }, async (app) => {
        await clickAndWait(app, app.verse(16), 'list_notes');
        const romans = chip(app, 'Romans 5:8');
        await clickAndWait(app, romans, 'get_passage');
        preview(app).querySelector('.xref-preview-go').focus();

        let event = app.key('Escape', {}, app.document.activeElement);
        assert.ok(event.defaultPrevented);
        assert.equal(shown(app), null, 'the preview closed');
        assert.ok(card(app), 'the card stayed');
        assert.equal(app.document.activeElement, romans, 'focus back on its chip');
        assert.equal(romans.getAttribute('aria-expanded'), 'false');

        event = app.key('Escape', {}, app.document.activeElement);
        assert.equal(card(app), null, 'then the card');
        assert.equal(app.document.activeElement, app.verse(16));
        assert.equal(app.document.documentElement.dataset.readerMode, 'on', 'reading mode is last');
        app.key('Escape');
        assert.notEqual(app.document.documentElement.dataset.readerMode, 'on');
    }));

test('while comparing, the preview is the first column’s translation, labelled', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web', compare: true }, async (app) => {
        // The first column is the translation being read, the WEB.
        assert.deepEqual(app.compareColumns(), ['World English Bible (WEB)', 'King James Version (KJV)']);
        // Selected in the KJV column, previewed from the WEB, the left one.
        await clickAndWait(app, app.verse(16, 'kjv'), 'list_notes');
        await clickAndWait(app, chip(app, 'Romans 5:8'), 'get_passage');
        assert.equal(shown(app).title, 'Romans 5:8 (WEB)');
        const web = await shownVerse('web', 'Romans', 5, 8);
        assert.equal(preview(app).querySelector('.xref-preview-text').textContent, `8${web.text}`);
        assert.ok(app.isComparing());
        await clickAndWait(app, preview(app).querySelector('.xref-preview-go'), 'get_compare');
        assert.ok(app.isComparing(), 'still comparing');
        assert.equal(app.heading(), 'Romans 5');
        assert.ok(app.selectedVerseNumbers().includes(8));
    }));

test('a verse the translation doesn’t number is said so, and Go to is still offered', () =>
    withApp({ book: 'Leviticus', chapter: 13, translation: 'kjv' }, async (app) => {
        // Leviticus 13:23 cites Matthew 26:75; the KJV ends Matthew 26 at 74.
        await clickAndWait(app, app.verse(23), 'list_notes');
        await clickAndWait(app, chip(app, 'Matthew 26:75'), 'get_passage');
        assert.deepEqual(shown(app), {
            title: 'Matthew 26:75 (KJV)',
            numbers: [],
            notice: 'Matthew 26:75 isn’t numbered in the King James Version — Matthew 26 ends at verse 74 there.'.replace('’', "'"),
            go: 'Go to Matthew 26:75 →',
        });
        assert.equal(preview(app).querySelector('.xref-preview-text'), null);
        await clickAndWait(app, preview(app).querySelector('.xref-preview-go'), 'get_reading');
        assert.equal(app.heading(), 'Matthew 26');
        assert.match(app.notice(), /isn't numbered in the King James Version/);
    }));

test('a KJV preview has the italics for supplied words and no footnote markers', () =>
    withApp({ book: 'John', chapter: 10, translation: 'kjv' }, async (app) => {
        // John 10:11 cites Psalms 23:1 ("is" supplied); Psalm 23:2's notes
        // aren't in it, and a range with notes shows none of their markers.
        await clickAndWait(app, app.verse(11), 'list_notes');
        await clickAndWait(app, chip(app, 'Psalms 23:1'), 'get_passage');
        const text = preview(app).querySelector('.xref-preview-text');
        assert.deepEqual([...text.querySelectorAll('em.supplied')].map((e) => e.textContent), ['is']);
        assert.equal(text.querySelectorAll('.fn-marker, sup').length, 0);
        assert.doesNotMatch(text.textContent, /[{}]/);
    }));

test('a failed read says so in the preview and still offers Go to', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv', failing: { get_passage: 'no database' } }, async (app) => {
        await clickAndWait(app, app.verse(16), 'list_notes');
        await clickAndWait(app, chip(app, '1 John 4:9-10'), 'get_passage');
        assert.deepEqual(shown(app), {
            title: '1 John 4:9–10 (KJV)',
            numbers: [],
            notice: 'Couldn\'t read 1 John 4:9–10: no database',
            go: 'Go to 1 John 4:9–10 →',
        });
        assert.deepEqual(where(app), ['John 3', [16]]);
    }));

test('a new selection or closing the card closes the preview', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await clickAndWait(app, app.verse(16), 'list_notes');
        await clickAndWait(app, chip(app, 'Romans 5:8'), 'get_passage');
        await clickAndWait(app, app.verse(3), 'list_notes');
        assert.equal(shown(app), null);
        assert.deepEqual(pressed(app), []);
        await clickAndWait(app, app.verse(16), 'list_notes');
        await clickAndWait(app, chip(app, 'Romans 5:8'), 'get_passage');
        app.click(app.verse(16));
        assert.equal(card(app), null);
        // Esc now goes on to reading mode or nothing, not a hidden preview.
        const event = app.key('Escape');
        assert.equal(event.defaultPrevented, false);
    }));

test('a preview keeps the card on its side: it grows or scrolls there', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await clickAndWait(app, app.verse(16), 'list_notes');
        const { window, document } = app;
        const rect = (top, bottom) => ({ top, bottom, left: 300, right: 800, width: 500, height: bottom - top, x: 300, y: top });
        Object.defineProperty(window, 'innerHeight', { configurable: true, value: 800 });
        document.getElementById('top-bar').getBoundingClientRect = () => rect(0, 56);
        document.querySelector('.chapter-main').getBoundingClientRect = () => rect(-2000, 3000);
        app.verse(16).getBoundingClientRect = () => rect(300, 340);
        let height = 200;
        for (const prop of ['scrollHeight', 'offsetHeight']) {
            Object.defineProperty(card(app), prop, { configurable: true, get: () => height });
        }
        window.dispatchEvent(new window.Event('resize'));
        assert.ok(!card(app).classList.contains('is-above'), 'below, with room');
        const top = card(app).style.top;

        // The preview makes it taller than the room below (800 - 340 - 16
        // = 444) and than the room above would take: it stays below and
        // scrolls inside.
        height = 600;
        await clickAndWait(app, chip(app, 'Romans 5:8'), 'get_passage');
        window.dispatchEvent(new window.Event('resize'));
        assert.ok(!card(app).classList.contains('is-above'), 'still below');
        assert.ok(!card(app).classList.contains('is-docked'));
        assert.equal(card(app).style.top, top, 'not moved');
        assert.equal(card(app).style.maxHeight, '444px');

        // Without a preview the same height would have flipped it above.
        app.click(chip(app, 'Romans 5:8'));
        await app.idle(0);
        app.verse(16).getBoundingClientRect = () => rect(700, 740);
        height = 300;
        window.dispatchEvent(new window.Event('resize'));
        assert.ok(card(app).classList.contains('is-above'));
        const above = card(app).style.top;
        height = 700;
        await clickAndWait(app, chip(app, 'Romans 5:8'), 'get_passage');
        window.dispatchEvent(new window.Event('resize'));
        assert.ok(card(app).classList.contains('is-above'), 'still above');
        assert.equal(card(app).style.maxHeight, `${700 - 56 - 16}px`);
        assert.notEqual(card(app).style.top, above, 'grown upwards, its bottom kept');
    }));

test('opening a preview puts nothing into the chapter’s flow', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv', storage: { 'gospel-getter-layout': 'flowing' } }, async (app) => {
        const text = app.document.querySelector('.chapter-main .chapter-text');
        await clickAndWait(app, app.verse(16), 'list_notes');
        const children = [...text.children];
        const verses = () => [...text.children].filter((el) => el !== card(app)).map((el) => el.outerHTML);
        const html = verses();
        await clickAndWait(app, chip(app, '1 John 4:9-10'), 'get_passage');
        assert.deepEqual([...text.children], children, 'nothing added or moved: the card was already there');
        assert.deepEqual(verses(), html, 'the verses are untouched');
        assert.ok(card(app).contains(preview(app)));
        assert.equal(app.window.getComputedStyle(card(app)).position, 'absolute', 'out of the flow');
    }));

test('the keyboard: Enter and Space reach the chips and Go to, which act on click', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        // A button's Enter and Space are the browser's: nothing in the page
        // may take them. Each control is a real button, focusable, whose
        // keyboard activation is a click.
        await clickAndWait(app, app.verse(16), 'list_notes');
        const romans = chip(app, 'Romans 5:8');
        romans.focus();
        for (const key of ['Enter', ' ']) {
            assert.equal(app.key(key, {}, romans).defaultPrevented, false, `${JSON.stringify(key)} on a chip`);
        }
        let before = app.calls.length;
        romans.click();
        await app.idle(before, 'get_passage');
        assert.deepEqual(pressed(app), ['Romans 5:8']);
        const go = preview(app).querySelector('.xref-preview-go');
        go.focus();
        assert.equal(app.key('Enter', {}, go).defaultPrevented, false);
        before = app.calls.length;
        go.click();
        await app.idle(before, 'get_reading');
        assert.deepEqual(where(app), ['Romans 5', [8]]);
    }));

// ---- The chapter's Cross references list

const listed = (app) => [...app.document.querySelectorAll('#xref-summary-list .xref-summary-chip')];
const openList = (app) => app.click(app.document.querySelector('.disclosure-toggle[data-disclosure="xrefs"]'));

test('a citation in the chapter’s Cross references opens its passage, selected', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        openList(app);
        const range = listed(app).find((c) => c.textContent === '1 John 4:9-10');
        assert.ok(range, 'fixture: John 3 lists 1 John 4:9-10');
        assert.equal(range.tagName, 'BUTTON');
        assert.equal(range.type, 'button');
        assert.equal(range.tabIndex, 0);
        await clickAndWait(app, range, 'get_reading');
        assert.deepEqual(where(app), ['1 John 4', [9, 10]]);
        assert.equal(card(app).previousElementSibling, app.verse(10));
        assert.equal(shown(app), null, 'no preview there');

        const before = app.calls.length;
        app.key('ArrowLeft', { altKey: true });
        await app.idle(before, 'get_reading');
        assert.equal(app.heading(), 'John 3');

        openList(app);
        const single = listed(app).find((c) => c.textContent === 'Romans 5:8');
        await clickAndWait(app, single, 'get_reading');
        assert.deepEqual(where(app), ['Romans 5', [8]]);
    }));

test('every citation in the list is a button named by its reference', () =>
    withApp({ book: 'Psalms', chapter: 23, translation: 'kjv' }, async (app) => {
        const all = listed(app);
        const count = Number(/\((\d+)\)/.exec(app.document.querySelector('.disclosure-toggle[data-disclosure="xrefs"]').textContent)[1]);
        assert.equal(all.length, count);
        for (const el of all) {
            assert.equal(el.tagName, 'BUTTON');
            assert.match(el.textContent, /^(\d )?[A-Z][A-Za-z ]+ \d+:\d+(-\d+)?$/);
            assert.equal(el.getAttribute('aria-label'), null, 'its name is its text');
            assert.ok(el.dataset.book && el.dataset.chapter && el.dataset.verse);
        }
    }));

test('the keyboard: Enter and Space reach the list’s citations, which act on click', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        openList(app);
        const link = listed(app)[0];
        link.focus();
        assert.equal(app.document.activeElement, link);
        for (const key of ['Enter', ' ']) {
            assert.equal(app.key(key, {}, link).defaultPrevented, false, `${JSON.stringify(key)} on a citation`);
        }
        const verse = Number(link.dataset.verse);
        const before = app.calls.length;
        link.click();
        await app.idle(before, 'get_reading');
        assert.ok(app.selectedVerseNumbers().includes(verse));
    }));
