// Behavior tests for passage history: the places the reader jumped to
// this session, Alt+← and Alt+→ (and the palette's Back and Forward, and a
// mouse's back and forward buttons), where each returns to, and what isn't
// recorded.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, closeBridge, openApp } from './harness.mjs';

after(closeBridge);

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

// jsdom has no layout: the page's scroll position is a number the test
// keeps, which scrollTo and scrollBy set as a browser would.
function fakeScroll(app) {
    const w = app.window;
    let y = 0;
    Object.defineProperty(w, 'scrollY', { configurable: true, get: () => y });
    w.scrollTo = (x, top) => { y = typeof x === 'object' ? x.top || 0 : top; };
    w.scrollBy = (x, dy) => { y += dy; };
    return { get: () => y, set: (v) => { y = v; } };
}

const where = (app) => [app.heading(), app.selectedVerseNumbers()];

async function typeCommand(app, text) {
    if (app.searchPanel().hidden) app.key('/');
    app.type(app.searchInput(), text);
    await app.idle(0);
}

async function waitFor(app, fn, cmd = 'get_reading') {
    const before = app.calls.length;
    fn();
    await app.idle(before, cmd);
}

const back = (app) => waitFor(app, () => app.key('ArrowLeft', { altKey: true }));
const forward = (app) => waitFor(app, () => app.key('ArrowRight', { altKey: true }));

// Alt+← or Alt+→ where history has nothing: says so, reads nothing.
async function nothingThere(app, key, message) {
    const before = app.calls.length;
    const event = app.key(key, { altKey: true });
    assert.ok(event.defaultPrevented);
    await app.idle(before);
    assert.equal(app.calls.slice(before).filter((c) => c.cmd === 'get_reading').length, 0);
    assert.equal(app.toast().textContent, message);
}

const pickerChapter = (app, bookName, chapter) => {
    const book = [...app.document.querySelectorAll('#picker .book-item')].find((b) => b.textContent.trim().startsWith(bookName));
    if (book.getAttribute('aria-expanded') !== 'true') app.click(book);
    return app.document.querySelector(`#picker .chapter-grid[data-book-id="${book.dataset.bookId}"] .chapter-btn[data-chapter="${chapter}"]`);
};

test('a picker jump records; Back and Forward return, and the webview’s own history is never touched', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        const length = app.window.history.length;
        await waitFor(app, () => app.click(pickerChapter(app, 'Psalms', 23)));
        assert.deepEqual(where(app), ['Psalms 23', []]);
        await back(app);
        assert.deepEqual(where(app), ['John 3', []]);
        await forward(app);
        assert.deepEqual(where(app), ['Psalms 23', []]);
        await nothingThere(app, 'ArrowRight', 'Nothing later in this session.');
        assert.equal(app.window.history.length, length, 'no pushState');
        // A popstate (the webview's own back) reads nothing and pushes nothing.
        const before = app.calls.length;
        app.window.dispatchEvent(new app.window.PopStateEvent('popstate', { state: { bookId: 43, chapter: 3 } }));
        await app.idle(before);
        assert.equal(app.calls.length, before);
        assert.equal(app.heading(), 'Psalms 23');
        assert.equal(app.window.history.length, length);
    }));

test('moving on isn’t a jump: ← and →, the end-of-chapter buttons, a neighbor’s heading, [ and ], the translation and compare', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await nothingThere(app, 'ArrowLeft', 'Nothing earlier in this session.');
        await waitFor(app, () => app.key('ArrowRight'));
        await waitFor(app, () => app.key('ArrowLeft'));
        await waitFor(app, () => app.click(app.document.querySelector('.chapter-step-next')));
        await waitFor(app, () => app.click(app.document.querySelector('.chapter-side-prev .chapter-heading-link')));
        await waitFor(app, () => app.key(']'));
        await waitFor(app, () => app.key('['));
        assert.equal(app.heading(), 'John 1');
        app.open('translation');
        await waitFor(app, () => app.click(app.document.querySelector('#translation-choices [data-value="web"]')));
        app.key('Escape', {}, app.document.activeElement);
        await waitFor(app, () => app.key('c'), 'get_compare');
        app.key('c');
        await app.idle(0);
        await nothingThere(app, 'ArrowLeft', 'Nothing earlier in this session.');
        await nothingThere(app, 'ArrowRight', 'Nothing later in this session.');
    }));

test('Back returns to where the reader left: the chapter they’d moved on to, scrolled as it was', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        const scroll = fakeScroll(app);
        await waitFor(app, () => app.key('ArrowRight'));
        scroll.set(640);
        await waitFor(app, () => app.click(pickerChapter(app, 'Psalms', 23)));
        scroll.set(120);
        await back(app);
        assert.deepEqual(where(app), ['John 4', []]);
        assert.equal(scroll.get(), 640);
        await forward(app);
        assert.deepEqual(where(app), ['Psalms 23', []]);
        assert.equal(scroll.get(), 120, 'forward too returns to where Psalm 23 was left');
    }));

test('search: Go to and a verse result record, with the verses they landed on', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        const scroll = fakeScroll(app);
        await app.search('rom 5:8');
        await waitFor(app, () => app.key('Enter', {}, app.searchInput()));
        assert.deepEqual(where(app), ['Romans 5', [8]]);
        scroll.set(900);
        await app.search('brother keeper');
        const result = app.searchResults().find((r) => r.querySelector('.search-ref').textContent === 'Genesis 4:9');
        await waitFor(app, () => app.click(result));
        assert.deepEqual(where(app), ['Genesis 4', [9]]);
        await back(app);
        assert.deepEqual(where(app), ['Romans 5', [8]], 'the landed verse, selected again');
        assert.equal(scroll.get(), 900);
        await back(app);
        assert.deepEqual(where(app), ['John 3', []]);
        await forward(app);
        await forward(app);
        assert.deepEqual(where(app), ['Genesis 4', [9]]);
    }));

test('a cross-reference’s Go to and a chapter-end cross-reference record; Back is what “Back returns” always meant', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await waitFor(app, () => app.click(app.verse(16)), 'list_notes');
        const chip = [...app.document.querySelectorAll('#selection-card .xref-citation')].find((c) => c.textContent === 'Romans 5:8');
        await waitFor(app, () => app.click(chip), 'get_passage');
        await waitFor(app, () => app.click(app.document.querySelector('#xref-preview .xref-preview-go')));
        assert.deepEqual(where(app), ['Romans 5', [8]]);
        await back(app);
        assert.equal(app.heading(), 'John 3');

        const toggle = app.document.querySelector('.xref-summary .disclosure-toggle');
        if (toggle.getAttribute('aria-expanded') !== 'true') app.click(toggle);
        const listed = [...app.document.querySelectorAll('.xref-summary-chip')].find((c) => c.textContent === '1 John 4:9-10');
        await waitFor(app, () => app.click(listed));
        assert.deepEqual(where(app), ['1 John 4', [9, 10]]);
        await back(app);
        assert.equal(app.heading(), 'John 3');
        await forward(app);
        assert.deepEqual(where(app), ['1 John 4', [9, 10]], 'the newer jump replaced Romans 5 ahead');
    }));

test('the Library’s rows record: a bookmark, a highlight and a note', async () => {
    // Saved before the app opens, as a reader's earlier session would.
    await backend('__reset', { bookId: 19, chapter: 23, translationCode: 'kjv' });
    await backend('add_bookmark', { bookId: 43, chapter: 3, verseStart: 16, verseEnd: 16 });
    await backend('set_highlight', { bookId: 45, chapter: 5, verseStart: 8, verseEnd: 8, color: 'yellow' });
    await backend('add_note', { bookId: 1, chapter: 1, verseStart: 1, verseEnd: 1, body: 'In the beginning.' });
    await withApp({ book: 'Psalms', chapter: 23, translation: 'kjv', keepBookmarks: true }, async (app) => {
        for (const [tab, selector, expected] of [
            ['bookmarks', '#bookmark-list .bookmark-open', ['John 3', [16]]],
            ['highlights', '#highlight-list .bookmark-open', ['Romans 5', [8]]],
            ['notes', '#note-list .bookmark-open', ['Genesis 1', [1]]],
        ]) {
            app.openLibrary(tab);
            await app.idle(0);
            const row = app.document.querySelector(selector);
            assert.ok(row, tab);
            await waitFor(app, () => app.click(row));
            assert.deepEqual(where(app), expected, tab);
            await back(app);
            assert.equal(app.heading(), 'Psalms 23', tab);
            app.key('Escape', {}, app.document.activeElement);
        }
    });
});

test('the welcome’s places to start, Random chapter and the palette’s Go to record', () =>
    withApp({ book: 'John', chapter: 3, fresh: true }, async (app) => {
        const row = app.document.querySelector('.welcome-start-row');
        assert.ok(row && !app.document.getElementById('welcome-overlay').hidden, 'a fresh install opens the welcome');
        await waitFor(app, () => app.click(row));
        const first = app.heading();
        await back(app);
        const start = app.heading();
        await forward(app);
        assert.equal(app.heading(), first);

        // Random chapter, from the picker or the palette.
        await waitFor(app, () => app.click(app.document.getElementById('random-chapter-btn')));
        const random = app.heading();
        await back(app);
        assert.equal(app.heading(), first);
        await forward(app);
        assert.equal(app.heading(), random);

        // The palette's Go to.
        await app.search('ps 23');
        await waitFor(app, () => app.key('Enter', {}, app.searchInput()));
        assert.equal(app.heading(), 'Psalms 23');
        await back(app);
        assert.equal(app.heading(), random);
        assert.ok(start);
    }));

test('a jump after going back drops the places ahead', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await waitFor(app, () => app.click(pickerChapter(app, 'Mark', 1)));
        await waitFor(app, () => app.click(pickerChapter(app, 'Luke', 1)));
        await back(app);
        assert.equal(app.heading(), 'Mark 1');
        await waitFor(app, () => app.click(pickerChapter(app, 'Acts', 2)));
        await nothingThere(app, 'ArrowRight', 'Nothing later in this session.');
        await back(app);
        assert.equal(app.heading(), 'Mark 1');
        await back(app);
        assert.equal(app.heading(), 'John 3');
        await nothingThere(app, 'ArrowLeft', 'Nothing earlier in this session.');
    }));

test('history keeps the last 50 places', () =>
    withApp({ book: 'Genesis', chapter: 1 }, async (app) => {
        // 55 jumps, to Psalms 1–55: with Genesis 1, 56 places.
        for (let n = 1; n <= 55; n++) await waitFor(app, () => app.click(pickerChapter(app, 'Psalms', n)));
        let backs = 0;
        for (;;) {
            const before = app.calls.length;
            app.key('ArrowLeft', { altKey: true });
            await app.idle(before);
            if (!app.calls.slice(before).some((c) => c.cmd === 'get_reading')) break;
            backs += 1;
        }
        assert.equal(backs, 49);
        assert.equal(app.heading(), 'Psalms 6', 'the oldest kept place');
        assert.equal(app.toast().textContent, 'Nothing earlier in this session.');
    }));

test('Alt+← and Alt+→ do nothing while typing; a mouse’s back and forward buttons work', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await waitFor(app, () => app.click(pickerChapter(app, 'Psalms', 23)));
        app.key('/');
        const before = app.calls.length;
        const typed = app.key('ArrowLeft', { altKey: true }, app.searchInput());
        assert.ok(!typed.defaultPrevented, 'the box keeps its own Alt+←');
        await app.idle(before);
        assert.equal(app.heading(), 'Psalms 23');
        app.key('Escape', {}, app.searchInput());

        const mouse = (button) => app.window.dispatchEvent(new app.window.MouseEvent('mouseup', { button, bubbles: true, cancelable: true }));
        await waitFor(app, () => mouse(3));
        assert.equal(app.heading(), 'John 3');
        await waitFor(app, () => mouse(4));
        assert.equal(app.heading(), 'Psalms 23');
    }));

test('the palette’s Back and Forward, disabled with a reason at the ends', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        const command = (label) => [...app.document.querySelectorAll('#search-results .search-command')]
            .find((el) => el.querySelector('.palette-label').textContent === label);
        const reason = (label) => {
            const el = command(label);
            return el.getAttribute('aria-disabled') === 'true' ? el.querySelector('.palette-reason').textContent : null;
        };
        await typeCommand(app, '>back');
        assert.equal(reason('Back'), 'Nothing earlier in this session');
        assert.equal(command('Back').querySelector('kbd').textContent, 'Alt+←');
        assert.equal(command('Back').getAttribute('aria-keyshortcuts'), 'Alt+ArrowLeft');
        app.key('Escape', {}, app.searchInput());

        await waitFor(app, () => app.click(pickerChapter(app, 'Psalms', 23)));
        await typeCommand(app, '>back');
        assert.equal(reason('Back'), null);
        // ("back" begins "backup" too, Export data…'s, listed first.)
        await waitFor(app, () => app.click(command('Back')));
        assert.equal(app.heading(), 'John 3');
        await typeCommand(app, '>forward');
        assert.equal(reason('Forward'), null);
        await waitFor(app, () => app.key('Enter', {}, app.searchInput()));
        assert.equal(app.heading(), 'Psalms 23');
        await typeCommand(app, '>forward');
        assert.equal(reason('Forward'), 'Nothing later in this session');
    }));

test('a chapter from the picker or Random chapter starts at its top, as [ and ] do; a verse jump goes to the verse; Back still restores', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        const scroll = fakeScroll(app);
        const centred = [];
        app.window.Element.prototype.scrollIntoView = function () { centred.push(this); };

        // The sidebar's chapter, from the top.
        scroll.set(800);
        await waitFor(app, () => app.click(pickerChapter(app, 'Psalms', 23)));
        assert.deepEqual(where(app), ['Psalms 23', []]);
        assert.equal(scroll.get(), 0);
        // The chapter already on screen stays where the reader is in it.
        scroll.set(500);
        await waitFor(app, () => app.click(pickerChapter(app, 'Psalms', 23)));
        assert.equal(scroll.get(), 500);

        // Random chapter (the last book's last chapter), and [.
        app.window.Math.random = () => 0.999999;
        await waitFor(app, () => app.click(app.document.getElementById('random-chapter-btn')));
        assert.deepEqual(where(app), ['Revelation 22', []]);
        assert.equal(scroll.get(), 0);
        scroll.set(700);
        await waitFor(app, () => app.key('['));
        assert.deepEqual(where(app), ['Jude 1', []]);
        assert.equal(scroll.get(), 0);

        // The popover's chapter, from the top too.
        scroll.set(650);
        app.resize(900);
        app.click(app.document.getElementById('ref-btn'));
        await waitFor(app, () => app.click(pickerChapter(app, 'Romans', 8)));
        assert.deepEqual(where(app), ['Romans 8', []]);
        assert.equal(scroll.get(), 0);

        // A jump to a verse goes to the verse, not the top.
        scroll.set(300);
        await app.search('rom 5:8');
        await waitFor(app, () => app.key('Enter', {}, app.searchInput()));
        assert.deepEqual(where(app), ['Romans 5', [8]]);
        assert.equal(scroll.get(), 300, 'not sent to the top');
        assert.equal(centred.at(-1), app.verse(8));

        // Back still returns to each place as it was left: Romans 8 at
        // 300, then Jude 1 (where [ moved on to from Random's Revelation
        // 22) at 650.
        await back(app);
        assert.deepEqual(where(app), ['Romans 8', []]);
        assert.equal(scroll.get(), 300);
        await back(app);
        assert.deepEqual(where(app), ['Jude 1', []]);
        assert.equal(scroll.get(), 650);
    }));

// GG-20261026-2: a step that can't be read leaves the history as it was.
test('a Back that fails changes nothing: Back again reaches the place it skipped, and Forward returns', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await waitFor(app, () => app.click(pickerChapter(app, 'Psalms', 23)));
        await waitFor(app, () => app.click(pickerChapter(app, 'Romans', 8)));
        assert.equal(app.heading(), 'Romans 8');
        app.failReading();
        await back(app);
        assert.equal(app.heading(), 'Romans 8', 'still on screen');
        assert.match(app.notice(), /simulated get_reading failure/);
        app.failReading(false);
        await back(app);
        assert.deepEqual(where(app), ['Psalms 23', []], 'not John 3');
        await forward(app);
        assert.deepEqual(where(app), ['Romans 8', []]);
        await nothingThere(app, 'ArrowRight', 'Nothing later in this session.');
        await back(app);
        await back(app);
        assert.deepEqual(where(app), ['John 3', []]);
    }));

test('a Forward that fails changes nothing either, and Try again takes the step', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await waitFor(app, () => app.click(pickerChapter(app, 'Psalms', 23)));
        await waitFor(app, () => app.click(pickerChapter(app, 'Romans', 8)));
        await back(app);
        await back(app);
        assert.equal(app.heading(), 'John 3');
        app.failReading();
        await forward(app);
        assert.equal(app.heading(), 'John 3');
        await nothingThere(app, 'ArrowLeft', 'Nothing earlier in this session.');
        app.failReading(false);
        // Try again reads Psalm 23 as the step it was, not as a chapter
        // turn from John 3 (which would put Psalm 23 in John 3's place).
        const retry = app.document.querySelector('#reader-notice .reading-retry');
        assert.equal(retry.textContent, 'Try again');
        await waitFor(app, () => app.click(retry));
        assert.equal(app.heading(), 'Psalms 23');
        await back(app);
        assert.equal(app.heading(), 'John 3');
        await forward(app);
        await forward(app);
        assert.equal(app.heading(), 'Romans 8');
    }));
