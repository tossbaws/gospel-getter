// Behavior tests for j and k (select the next or previous verse, Shift to
// extend) and [ and ] (the previous or next book, at chapter 1).

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

async function press(app, key, opts = {}, target) {
    const before = app.calls.length;
    const event = app.key(key, opts, target);
    await app.idle(before);
    return event;
}

const card = (app) => app.document.getElementById('selection-card');

// jsdom has no layout: lays out the column's verses 100px apart, from
// `first` at the top of the window (the top bar's bottom is 0 here), so
// verses `first` to `first + 7` are in view in its 768px (a line or more
// of each).
function layOut(app, code, first) {
    for (const el of app.document.querySelectorAll(`.chapter-main .verse[data-translation="${code}"]`)) {
        const top = (Number(el.dataset.verse) - first) * 100 + 10;
        el.getBoundingClientRect = () => ({ top, bottom: top + 90, left: 0, right: 600, width: 600, height: 90 });
    }
}

test('j and k with nothing selected: the first verse in view, or the last', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        layOut(app, 'kjv', 5);
        const event = await press(app, 'j');
        assert.ok(event.defaultPrevented);
        assert.deepEqual(app.selectedVerseNumbers(), [5]);
        assert.equal(card(app).previousElementSibling, app.verse(5), 'the card follows');
        app.key('Escape');
        layOut(app, 'kjv', 5);
        await press(app, 'k');
        assert.deepEqual(app.selectedVerseNumbers(), [12]);
        // A verse counts only once a line of it shows: verse 4's last
        // 20px under the bar don't.
        app.key('Escape');
        for (const el of app.document.querySelectorAll('.chapter-main .verse[data-translation="kjv"]')) {
            const top = (Number(el.dataset.verse) - 5) * 100 + 30;
            el.getBoundingClientRect = () => ({ top, bottom: top + 90, left: 0, right: 600, width: 600, height: 90 });
        }
        await press(app, 'j');
        assert.deepEqual(app.selectedVerseNumbers(), [5]);
    }));

test('j and k move the selection a verse, the card following; never past the chapter', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        app.click(app.verse(16));
        await press(app, 'j');
        assert.deepEqual(app.selectedVerseNumbers(), [17]);
        assert.equal(card(app).previousElementSibling, app.verse(17));
        await press(app, 'k');
        await press(app, 'k');
        assert.deepEqual(app.selectedVerseNumbers(), [15]);

        // A range moves from its ends: j after the last, k before the first.
        app.click(app.verse(20));
        app.click(app.verse(22), { shiftKey: true });
        await press(app, 'j');
        assert.deepEqual(app.selectedVerseNumbers(), [23]);
        app.click(app.verse(20));
        app.click(app.verse(22), { shiftKey: true });
        await press(app, 'k');
        assert.deepEqual(app.selectedVerseNumbers(), [19]);

        // The chapter's last and first verses: nowhere further.
        app.click(app.verse(36));
        await press(app, 'j');
        assert.deepEqual(app.selectedVerseNumbers(), [36]);
        assert.equal(app.heading(), 'John 3', 'j never turns the chapter');
        app.click(app.verse(1));
        await press(app, 'k');
        assert.deepEqual(app.selectedVerseNumbers(), [1]);
    }));

test('Shift+j and Shift+k extend the selection from where it started', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(16));
        await press(app, 'J', { shiftKey: true });
        await press(app, 'J', { shiftKey: true });
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17, 18]);
        assert.equal(app.document.querySelector('.verse-actions-ref').textContent, 'John 3:16–18 (WEB)');
        assert.equal(card(app).previousElementSibling, app.verse(18));
        await press(app, 'K', { shiftKey: true });
        await press(app, 'K', { shiftKey: true });
        await press(app, 'K', { shiftKey: true });
        assert.deepEqual(app.selectedVerseNumbers(), [15, 16]);
        // Within the chapter only.
        app.click(app.verse(35));
        for (let i = 0; i < 3; i++) await press(app, 'J', { shiftKey: true });
        assert.deepEqual(app.selectedVerseNumbers(), [35, 36]);
    }));

test('j skips a verse the translation doesn’t number', () =>
    withApp({ book: 'Matthew', chapter: 17, translation: 'bsb' }, async (app) => {
        app.click(app.verse(20, 'bsb'));
        await press(app, 'j');
        assert.deepEqual(app.selectedVerseNumbers('bsb'), [22]);
        await press(app, 'k');
        assert.deepEqual(app.selectedVerseNumbers('bsb'), [20]);
    }));

test('comparing, j and k work in the left column, the translation being read', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv', compare: true }, async (app) => {
        assert.ok(app.isComparing());
        await press(app, 'j');
        assert.deepEqual(app.selectedVerseNumbers('kjv'), [1]);
        assert.deepEqual(app.selectedVerseNumbers('web'), []);
        await press(app, 'j');
        assert.deepEqual(app.selectedVerseNumbers('kjv'), [2]);
        // A selection in the right column: j starts again in the left.
        app.click(app.verse(10, 'web'));
        await press(app, 'j');
        assert.deepEqual(app.selectedVerseNumbers('web'), []);
        assert.deepEqual(app.selectedVerseNumbers('kjv'), [1]);
    }));

test('j and k work in the Flowing layout', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv', storage: { 'gospel-getter-layout': 'flowing' } }, async (app) => {
        assert.equal(app.document.documentElement.dataset.layout, 'flowing');
        app.click(app.verse(16));
        await press(app, 'j');
        await press(app, 'J', { shiftKey: true });
        assert.deepEqual(app.selectedVerseNumbers(), [17, 18]);
        assert.equal(card(app).previousElementSibling, app.verse(18));
    }));

test('j and k do nothing while typing, or with a dialog or the palette open', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        app.key('/');
        await press(app, 'j', {}, app.searchInput());
        assert.deepEqual(app.selectedVerseNumbers(), []);
        app.key('Escape', {}, app.searchInput());
        app.open('about');
        await press(app, 'j', {}, app.document.activeElement);
        assert.deepEqual(app.selectedVerseNumbers(), []);
    }));

test('[ and ] go to the book before and after, at chapter 1, from the top', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        let event = await press(app, ']');
        assert.ok(event.defaultPrevented);
        assert.equal(app.heading(), 'Acts 1');
        await press(app, '[');
        await press(app, '[');
        assert.equal(app.heading(), 'Luke 1');
    }));

test('[ in Genesis and ] in Revelation say there’s no book there', () =>
    withApp({ book: 'Genesis', chapter: 5 }, async (app) => {
        let before = app.calls.length;
        await press(app, '[');
        assert.equal(app.calls.slice(before).filter((c) => c.cmd === 'get_reading').length, 0);
        assert.equal(app.heading(), 'Genesis 5');
        assert.equal(app.toast().textContent, 'Genesis is the Bible’s first book.');

        app.key('/');
        app.type(app.searchInput(), 'rev 3');
        await app.idle(0, 'search');
        before = app.calls.length;
        app.key('Enter', {}, app.searchInput());
        await app.idle(before, 'get_reading');
        before = app.calls.length;
        await press(app, ']');
        assert.equal(app.calls.slice(before).filter((c) => c.cmd === 'get_reading').length, 0);
        assert.equal(app.toast().textContent, 'Revelation is the Bible’s last book.');
        await press(app, '[');
        assert.equal(app.heading(), 'Jude 1');
    }));
