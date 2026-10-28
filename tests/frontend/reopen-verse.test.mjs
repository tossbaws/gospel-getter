// Reopening at the verse the reader was at: as they stop scrolling (and as
// the window is hidden or loses focus), the verse at the top of the reading
// pane is saved with the chapter, with how far its top is below the bar;
// the next launch puts that verse back there, as the reading place logic
// puts a place back. A translation that doesn't number the verse opens the
// chapter at its top, and a chapter turn after the launch is as it was.
// jsdom has no layout: the page is laid out by `layOut`.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, closeBridge, openApp } from './harness.mjs';

after(closeBridge);

const JOHN = bookId('John');
const MATTHEW = bookId('Matthew');
const BAR = 54;

/**
 * A laid-out page, set up before the page's scripts run (so boot sees it):
 * the bar ends 54px down, the chapter's heading is 60px down the page (just
 * below the bar, as the page sets it) and verse n 60n + 40px down, 50px
 * tall; the window is scrolled to `layout.y`,
 * which scrollBy and scrollTo move, firing the scroll event a browser fires.
 */
function layOut(layout) {
    return (window) => {
        Object.defineProperty(window, 'scrollY', { configurable: true, get: () => layout.y });
        const scrolled = () => window.dispatchEvent(new window.Event('scroll'));
        window.scrollBy = (x, dy) => {
            layout.y += dy;
            layout.scrolls.push(dy);
            scrolled();
        };
        window.scrollTo = (x, top) => {
            layout.y = typeof x === 'object' ? x.top || 0 : top;
            scrolled();
        };
        const measured = window.HTMLElement.prototype.getBoundingClientRect;
        window.HTMLElement.prototype.getBoundingClientRect = function () {
            const box = (top, height) => ({ top, bottom: top + height, left: 0, right: 600, width: 600, height, x: 0, y: top });
            if (this.id === 'top-bar') return box(0, BAR);
            if (this.matches('.chapter-main .chapter-heading')) return box(60 - layout.y, 30);
            if (this.matches('.chapter-main .verse[data-verse]')) {
                return box(60 * Number(this.dataset.verse) + 40 - layout.y, 50);
            }
            return measured.call(this);
        };
    };
}

const layout = () => ({ y: 0, scrolls: [] });

/** Where the next launch opens: translation, book, chapter, verse, offset. */
async function saved() {
    const home = await backend('get_home');
    return [home.currentTranslationCode, home.currentBookId, home.currentChapter, home.currentVerse, home.currentVerseOffset];
}

async function savedBecomes(app, want) {
    for (let i = 0; i < 100; i++) {
        if (JSON.stringify(await saved()) === JSON.stringify(want)) return;
        await new Promise((r) => setTimeout(r, 20));
    }
    assert.deepEqual(await saved(), want);
}

/** The reader scrolls the page to `y` and stops. */
function scrollTo(app, at, y) {
    at.y = y;
    app.window.dispatchEvent(new app.window.Event('scroll'));
}

test('the verse read last is where the next launch opens, at the same place under the bar', async () => {
    const first = layout();
    const app = await openApp({ book: 'John', chapter: 3, translation: 'kjv', setUp: layOut(first) });
    try {
        assert.deepEqual(first.scrolls, [], 'a chapter saved without a verse opens at its top');
        assert.deepEqual(await saved(), ['kjv', JOHN, 3, null, null]);
        // Scrolled 900px: verse 15 (940px down the page) is the first to
        // reach below the bar, its top 40px down the window, 14px above
        // the bar's bottom.
        scrollTo(app, first, 900);
        await savedBecomes(app, ['kjv', JOHN, 3, 15, -14]);
        assert.deepEqual(JSON.parse(JSON.stringify(app.window.gospelGetterPlace.saved())), {
            bookId: JOHN, chapter: 3, translationCode: 'kjv', verse: 15, offset: -14,
        });
        const sent = app.calls.filter((c) => c.cmd === 'save_reading_position');
        assert.deepEqual(JSON.parse(JSON.stringify(sent.map((c) => c.args))), [
            { bookId: JOHN, chapter: 3, translationCode: 'kjv' },
            { bookId: JOHN, chapter: 3, translationCode: 'kjv', verse: 15, offset: -14 },
        ]);
    } finally {
        app.close();
    }

    const second = layout();
    const again = await openApp({ book: 'John', chapter: 3, translation: 'kjv', keepPosition: true, setUp: layOut(second) });
    try {
        assert.equal(again.heading(), 'John 3');
        assert.equal(second.y, 900, 'scrolled back to where the reader was');
        assert.equal(again.verse(15).getBoundingClientRect().top, BAR - 14, 'verse 15 14px above the bar, as it was');
        // Putting it back saved nothing new: the same place.
        assert.deepEqual(await saved(), ['kjv', JOHN, 3, 15, -14]);
        assert.deepEqual(again.consoleErrors, []);
    } finally {
        again.close();
    }
});

test('a verse the translation does not number opens the chapter at its top', async () => {
    // Matthew 17:21, saved for the BSB (which leaves it out): the backend
    // keeps it (the KJV numbers it), but there's no such verse on screen.
    await backend('__reset', { bookId: MATTHEW, chapter: 17, translationCode: 'bsb' });
    await backend('save_reading_position', { bookId: MATTHEW, chapter: 17, translationCode: 'bsb', verse: 21, offset: -10 });
    assert.deepEqual(await saved(), ['bsb', MATTHEW, 17, 21, -10]);
    const at = layout();
    const app = await openApp({ book: 'Matthew', chapter: 17, translation: 'bsb', keepPosition: true, setUp: layOut(at) });
    try {
        assert.equal(app.heading(), 'Matthew 17');
        assert.equal(app.document.querySelector('.chapter-main .verse[data-verse="21"]'), null);
        assert.equal(at.y, 0, 'the chapter at its top');
        assert.deepEqual(at.scrolls, []);
        assert.deepEqual(app.consoleErrors, []);
    } finally {
        app.close();
    }
});

test('after reopening at a verse, the chapter turns are as they were', async () => {
    await backend('__reset', { bookId: JOHN, chapter: 3, translationCode: 'web' });
    await backend('save_reading_position', { bookId: JOHN, chapter: 3, translationCode: 'web', verse: 20, offset: 0 });
    const at = layout();
    const app = await openApp({ book: 'John', chapter: 3, translation: 'web', keepPosition: true, setUp: layOut(at) });
    try {
        assert.equal(app.verse(20).getBoundingClientRect().top, BAR, 'verse 20 at the bar');
        assert.equal(at.y, 1186);
        // → leaves the page where it is, as it always has: there, John
        // 4:20 is at the bar, and that's saved once the page is still.
        let before = app.calls.length;
        app.key('ArrowRight');
        await app.idle(before, 'get_reading');
        await app.waitForHeading('John 4');
        assert.equal(at.y, 1186);
        await savedBecomes(app, ['web', JOHN, 4, 20, 0]);
        // The chapter's end goes to the next chapter from its top.
        before = app.calls.length;
        app.click(app.document.querySelector('.chapter-step-next'));
        await app.idle(before, 'get_reading');
        await app.waitForHeading('John 5');
        assert.equal(at.y, 0, 'John 5 from its top');
        await savedBecomes(app, ['web', JOHN, 5, null, null]);
    } finally {
        app.close();
    }
});

test('scrolled back to the heading, the chapter reopens at its top; losing focus saves at once', async () => {
    const at = layout();
    const app = await openApp({ book: 'John', chapter: 3, translation: 'kjv', setUp: layOut(at) });
    try {
        scrollTo(app, at, 300);
        // Verse 5 (340px down the page) is first, 14px above the bar.
        app.window.dispatchEvent(new app.window.Event('blur'));
        await app.idle();
        assert.deepEqual(await saved(), ['kjv', JOHN, 3, 5, -14]);
        scrollTo(app, at, 0);
        await savedBecomes(app, ['kjv', JOHN, 3, null, null]);
    } finally {
        app.close();
    }
});
