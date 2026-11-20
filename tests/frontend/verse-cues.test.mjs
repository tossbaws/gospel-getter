// What a screen reader hears on a verse for each mark drawn on it (D-04):
// a highlight's color and a bookmark, as notes, findings and footnotes
// already are: visually hidden words after the verse's number, kept in
// step as the marks change, and never part of the verse's own text.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, closeBridge, select, withApp } from './harness.mjs';

after(closeBridge);

const JOHN = bookId('John');

/** The verse's text as the accessibility tree has it: its text, hidden cues included. */
const heard = (app, n) => app.verse(n).textContent.replace(/\s+/g, ' ').trim();
const cueEls = (app, n) => [...app.verse(n).querySelectorAll(':scope > .bookmark-cue, :scope > .highlight-cue')];
const cues = (app, n) => cueEls(app, n).map((c) => c.textContent);

/** Presses `key` on the page and waits for every call it made. */
async function press(app, key) {
    const before = app.calls.length;
    await app.key(key);
    await app.idle(before);
}

test('a highlighted, bookmarked verse says so to a screen reader, and stops saying it once it isn’t', async () => {
    await backend('__reset', { bookId: JOHN, chapter: 3, translationCode: 'kjv' });
    await backend('add_bookmark', { bookId: JOHN, chapter: 3, verseStart: 16, verseEnd: 16 });
    await backend('set_highlight', { bookId: JOHN, chapter: 3, verseStart: 16, verseEnd: 16, color: 'yellow' });
    await withApp({ book: 'John', chapter: 3, translation: 'kjv', keepBookmarks: true }, async (app) => {
        const text = app.verse(16).querySelector('.verse-text').textContent;
        // Its number, both cues (in the order they came), then its words.
        assert.match(heard(app, 16), /^16(\((highlighted yellow|bookmarked)\) ){2}For God so loved/);
        assert.deepEqual(cues(app, 16).sort(), ['(bookmarked) ', '(highlighted yellow) ']);
        for (const cue of cueEls(app, 16)) {
            assert.ok(cue.classList.contains('visually-hidden'), 'heard, not seen');
            assert.ok(cue.previousElementSibling.classList.contains('verse-num')
                || cue.previousElementSibling.classList.contains('visually-hidden'), 'after the number');
        }
        // A verse with neither hears neither.
        assert.deepEqual(cues(app, 15), []);
        // The verse's own text is untouched.
        assert.equal(app.verse(16).querySelector('.verse-text').textContent, text);

        // H steps the color on: the cue follows it. B takes the bookmark off.
        await select(app, 16);
        await press(app, 'h');
        assert.deepEqual(cues(app, 16).sort(), ['(bookmarked) ', '(highlighted green) ']);
        await press(app, 'b');
        assert.deepEqual(cues(app, 16), ['(highlighted green) ']);
        for (const _ of ['blue', 'pink', 'off']) await press(app, 'h');
        assert.deepEqual(cues(app, 16), []);
        assert.equal(app.verse(16).dataset.highlight, undefined);
        assert.ok(!app.verse(16).classList.contains('is-bookmarked'));
    });
});
