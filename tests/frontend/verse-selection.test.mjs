// Behavior tests for GH-10: selecting verses in the chapter being read and
// copying them, with their reference, to the clipboard.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { closeBridge, openApp, settle, storedVerse } from './harness.mjs';

after(closeBridge);

const EM = '—';
const EN = '–';

// Compares as UTF-8 bytes, not just as JS strings, so a normalized quote or
// dash (or any other code point change) can't slip through.
function assertSameBytes(actual, expected) {
    assert.deepEqual(Buffer.from(actual, 'utf8'), Buffer.from(expected, 'utf8'), `\n${actual}\n!==\n${expected}`);
}

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

test('copying a single KJV verse gives its stored text and reference', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        app.click(app.verse(16));
        assert.deepEqual(app.selectedVerseNumbers(), [16]);
        await app.copyButton();
        assert.equal(app.clipboardWrites.length, 1);
        assertSameBytes(app.clipboardWrites[0], `${storedVerse('kjv', 'John', 3, 16)} ${EM} John 3:16 (KJV)`);
    }));

test('copying a single WEB verse matches the format in the issue', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(16));
        await app.copyButton();
        assertSameBytes(
            app.clipboardWrites[0],
            'For God so loved the world, that he gave his one and only Son, that whoever believes in him ' +
                'should not perish, but have eternal life. — John 3:16 (WEB)',
        );
    }));

test('Shift-click selects a visible contiguous range and copies it in order', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(16));
        const down = app.click(app.verse(18), { shiftKey: true });
        assert.ok(down.defaultPrevented, 'Shift+mousedown must not extend the native text selection');
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17, 18]);
        assert.equal(app.document.querySelector('.verse-actions-ref').textContent, `John 3:16${EN}18 (WEB)`);
        // The actions sit under the last verse of the range.
        assert.equal(app.actions().closest('.verse'), app.verse(18));

        await app.copyButton();
        const text = [16, 17, 18].map((n) => storedVerse('web', 'John', 3, n)).join(' ');
        assertSameBytes(app.clipboardWrites[0], `${text} ${EM} John 3:16${EN}18 (WEB)`);
    }));

test('a range selected bottom-up still copies in Bible order', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        app.click(app.verse(18));
        app.click(app.verse(16), { shiftKey: true });
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17, 18]);
        await app.copyButton();
        const text = [16, 17, 18].map((n) => storedVerse('kjv', 'John', 3, n)).join(' ');
        assertSameBytes(app.clipboardWrites[0], `${text} ${EM} John 3:16${EN}18 (KJV)`);
    }));

test('Shift-click re-extends from the first verse clicked', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        app.click(app.verse(5));
        app.click(app.verse(9), { shiftKey: true });
        app.click(app.verse(3), { shiftKey: true });
        assert.deepEqual(app.selectedVerseNumbers(), [3, 4, 5]);
    }));

test('verse numbers order numerically, not as strings', () =>
    withApp({ book: 'Genesis', chapter: 4, translation: 'kjv' }, async (app) => {
        app.click(app.verse(9));
        app.click(app.verse(12), { shiftKey: true });
        assert.deepEqual(app.selectedVerseNumbers(), [9, 10, 11, 12]);
        await app.copyButton();
        const text = [9, 10, 11, 12].map((n) => storedVerse('kjv', 'Genesis', 4, n)).join(' ');
        assertSameBytes(app.clipboardWrites[0], `${text} ${EM} Genesis 4:9${EN}12 (KJV)`);
    }));

test('three-digit verse ranges order numerically', () =>
    withApp({ book: 'Psalms', chapter: 119, translation: 'web' }, async (app) => {
        app.click(app.verse(105));
        app.click(app.verse(99), { shiftKey: true });
        await app.copyButton();
        const nums = [99, 100, 101, 102, 103, 104, 105];
        const text = nums.map((n) => storedVerse('web', 'Psalms', 119, n)).join(' ');
        assertSameBytes(app.clipboardWrites[0], `${text} ${EM} Psalms 119:99${EN}105 (WEB)`);
    }));

for (const [code, book, chapter, verse, note] of [
    ['kjv', 'Genesis', 4, 9, "apostrophe and {} translator markup"],
    ['kjv', 'Mark', 13, 8, 'straight double quotes inside a {} note'],
    ['web', 'Genesis', 4, 9, 'curly double quotes and apostrophes'],
    ['web', 'Genesis', 1, 3, 'curly quotes around a comma'],
]) {
    test(`${code.toUpperCase()} ${book} ${chapter}:${verse} copies byte for byte (${note})`, () =>
        withApp({ book, chapter, translation: code }, async (app) => {
            const stored = storedVerse(code, book, chapter, verse);
            // The rendered verse carries its number and possibly
            // cross-reference labels — none of which may be copied.
            assert.notEqual(app.verse(verse).textContent.trim(), stored);
            app.click(app.verse(verse));
            await app.copyButton();
            assertSameBytes(app.clipboardWrites[0], `${stored} ${EM} ${book} ${chapter}:${verse} (${code.toUpperCase()})`);
        }));
}

test('markup-like text is rendered as text and copied raw (synthetic DTO)', () =>
    withApp(
        {
            book: 'John',
            chapter: 3,
            alterReading: (dto) => ({
                ...dto,
                currentVerses: dto.currentVerses.map((v) =>
                    v.number === 1 ? { ...v, text: 'SYNTHETIC <b>not bold</b> & "q" \'a\'' } : v,
                ),
            }),
        },
        async (app) => {
            assert.equal(app.verse(1).querySelector('b'), null, 'verse text must be escaped, not parsed');
            app.click(app.verse(1));
            await app.copyButton();
            assertSameBytes(app.clipboardWrites[0], `SYNTHETIC <b>not bold</b> & "q" 'a' ${EM} John 3:1 (KJV)`);
        },
    ));

test('Shift-click in a faded neighboring chapter does not extend the selection', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        app.click(app.verse(16));
        for (const which of ['prev', 'next']) {
            app.click(app.sideVerse(which, 5), { shiftKey: true });
            assert.deepEqual(app.selectedVerseNumbers(), [16]);
        }
        await app.copyButton();
        assertSameBytes(app.clipboardWrites[0], `${storedVerse('kjv', 'John', 3, 16)} ${EM} John 3:16 (KJV)`);
        // Side-chapter verses are never themselves selectable.
        assert.equal(app.document.querySelectorAll('.chapter-side .verse-selected').length, 0);
    }));

test('a plain click in a faded neighboring chapter clears the selection', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        app.click(app.verse(16));
        app.click(app.sideVerse('next', 2));
        assert.deepEqual(app.selectedVerseNumbers(), []);
        assert.equal(app.actions(), null);
        assert.equal(app.heading(), 'John 3', 'clicking side text is not navigation');
    }));

test('successful copy shows a brief confirmation', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        app.click(app.verse(16));
        await app.copyButton();
        assert.equal(app.status().textContent, 'Copied');
        assert.equal(app.status().getAttribute('role'), 'status');
        assert.ok(!app.status().classList.contains('is-error'));
        await new Promise((r) => setTimeout(r, 2600));
        assert.equal(app.status().textContent, '', 'the confirmation fades');
    }));

test('a rejected clipboard write shows an error instead of failing silently', () =>
    withApp({ book: 'John', chapter: 3, clipboard: 'reject' }, async (app) => {
        app.click(app.verse(16));
        await app.copyButton();
        assert.match(app.status().textContent, /^Couldn't copy: The request is not allowed/);
        assert.ok(app.status().classList.contains('is-error'));
        assert.equal(app.consoleErrors.length, 1);

        // And recovers once the clipboard works again.
        app.setClipboard('ok');
        await app.copyButton();
        assert.equal(app.status().textContent, 'Copied');
        assert.ok(!app.status().classList.contains('is-error'));
    }));

test('a missing clipboard API shows an error', () =>
    withApp({ book: 'John', chapter: 3, clipboard: 'missing' }, async (app) => {
        app.click(app.verse(16));
        app.key('c', { ctrlKey: true });
        await settle();
        assert.equal(app.status().textContent, "Couldn't copy: clipboard access is not available");
        assert.ok(app.status().classList.contains('is-error'));
    }));

test('Ctrl+C copies the selection; without one it is left alone', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        const idle = app.key('c', { ctrlKey: true });
        assert.ok(!idle.defaultPrevented, 'no selection: Ctrl+C keeps its normal behavior');
        await settle();
        assert.equal(app.clipboardWrites.length, 0);

        app.click(app.verse(16));
        app.click(app.verse(17), { shiftKey: true });
        const copy = app.key('C', { ctrlKey: true });
        assert.ok(copy.defaultPrevented);
        await settle();
        const text = [16, 17].map((n) => storedVerse('web', 'John', 3, n)).join(' ');
        assertSameBytes(app.clipboardWrites[0], `${text} ${EM} John 3:16${EN}17 (WEB)`);
        assert.equal(app.status().textContent, 'Copied');

        // Other modifier combinations aren't a copy.
        app.key('c', { ctrlKey: true, shiftKey: true });
        app.key('c', { ctrlKey: true, altKey: true });
        app.key('c');
        await settle();
        assert.equal(app.clipboardWrites.length, 1);
    }));

test('Ctrl+C with highlighted text copies the highlight natively', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        app.click(app.verse(16));
        const range = app.document.createRange();
        range.selectNodeContents(app.verse(2));
        app.window.getSelection().addRange(range);
        const event = app.key('c', { ctrlKey: true });
        assert.ok(!event.defaultPrevented);
        await settle();
        assert.equal(app.clipboardWrites.length, 0);
    }));

test('Ctrl+C in a form control is not intercepted', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        app.click(app.verse(16));
        const event = app.key('c', { ctrlKey: true }, app.document.getElementById('translation-select'));
        assert.ok(!event.defaultPrevented);
        await settle();
        assert.equal(app.clipboardWrites.length, 0);
    }));

test('a click that ends a text highlight does not select a verse', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        const range = app.document.createRange();
        range.selectNodeContents(app.verse(4));
        app.window.getSelection().addRange(range);
        app.click(app.verse(4));
        assert.deepEqual(app.selectedVerseNumbers(), []);
    }));

test('Escape clears the selection before leaving reading mode', () =>
    withApp({ book: 'John', chapter: 3, readerMode: true }, async (app) => {
        const root = app.document.documentElement;
        assert.equal(root.dataset.readerMode, 'on');
        app.click(app.verse(16));
        app.key('Escape');
        assert.deepEqual(app.selectedVerseNumbers(), []);
        assert.equal(app.actions(), null);
        assert.equal(root.dataset.readerMode, 'on', 'first Escape only clears the selection');
        app.key('Escape');
        assert.equal(root.dataset.readerMode, undefined, 'next Escape leaves reading mode as before');
    }));

test('Escape closes an open settings menu before clearing the selection', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        const menu = app.document.getElementById('settings-menu');
        app.click(app.verse(16));
        app.click(app.document.getElementById('settings-toggle'));
        assert.equal(menu.hidden, false);
        assert.deepEqual(app.selectedVerseNumbers(), [16], 'opening settings is not a click away');
        app.key('Escape');
        assert.equal(menu.hidden, true);
        assert.deepEqual(app.selectedVerseNumbers(), [16]);
        app.key('Escape');
        assert.deepEqual(app.selectedVerseNumbers(), []);
    }));

test('clicking elsewhere or the selected verse again clears the selection', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        app.click(app.verse(16));
        app.click(app.verse(16));
        assert.deepEqual(app.selectedVerseNumbers(), []);

        app.click(app.verse(16));
        app.click(app.document.querySelector('footer'));
        assert.deepEqual(app.selectedVerseNumbers(), []);
        assert.equal(app.actions(), null);

        // A plain click inside a range starts a new single selection.
        app.click(app.verse(1));
        app.click(app.verse(4), { shiftKey: true });
        app.click(app.verse(2));
        assert.deepEqual(app.selectedVerseNumbers(), [2]);
    }));

test('cross-reference popups still open, expand and sit alongside Copy', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        const verse = app.verse(16);
        assert.ok(verse.classList.contains('has-xref'), 'fixture: John 3:16 has cross-references');
        const popup = verse.querySelector('.xref-popup');
        assert.equal(popup.hidden, true);

        app.click(verse);
        assert.equal(popup.hidden, false);
        assert.equal(app.actions().parentElement, popup, 'Copy lives in the verse popup');

        const citation = popup.querySelector('.xref-citation');
        app.click(citation);
        const expanded = citation.parentElement.querySelector('.xref-expanded');
        await app.until(() => !expanded.hidden, 'cross-reference text');
        assert.ok(expanded.querySelector('.xref-verse-text'));
        assert.deepEqual(app.selectedVerseNumbers(), [16], 'expanding a citation keeps the selection');
        assert.ok(app.calls.some((c) => c.cmd === 'get_xref_text' && c.args.translationCode === 'kjv'));

        // Copy still copies only the selected verse, not the popup content.
        await app.copyButton();
        assertSameBytes(app.clipboardWrites[0], `${storedVerse('kjv', 'John', 3, 16)} ${EM} John 3:16 (KJV)`);
        assert.equal(popup.hidden, false, 'Copy keeps the popup open');

        // Shift-click into the expanded text of a citation doesn't count as a
        // verse of its own.
        app.click(expanded.querySelector('.xref-verse-text'), { shiftKey: true });
        assert.deepEqual(app.selectedVerseNumbers(), [16]);
    }));

test('a range hides cross-reference popups', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        app.click(app.verse(16));
        app.click(app.verse(17), { shiftKey: true });
        assert.ok(app.verse(17).classList.contains('has-xref'), 'fixture: John 3:17 has cross-references');
        for (const popup of app.document.querySelectorAll('.chapter-main .xref-popup')) {
            assert.equal(popup.hidden, true);
        }
        assert.equal(app.actions().parentElement, app.verse(17));
        assert.equal(app.document.querySelectorAll('.verse-actions').length, 1);
    }));

test('verses without cross-references are selectable and get Copy', () =>
    withApp({ book: 'Genesis', chapter: 1, translation: 'web' }, async (app) => {
        const plain = app.verse(13);
        assert.ok(!plain.classList.contains('has-xref'), 'fixture: Genesis 1:13 has no cross-references');
        app.click(plain);
        assert.deepEqual(app.selectedVerseNumbers(), [13]);
        assert.equal(app.actions().parentElement, plain);
        await app.copyButton();
        assertSameBytes(app.clipboardWrites[0], `${storedVerse('web', 'Genesis', 1, 13)} ${EM} Genesis 1:13 (WEB)`);
    }));

test('arrow-key navigation clears the selection and still turns chapters', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        app.click(app.verse(16));
        app.key('ArrowRight');
        await app.waitForHeading('John 4');
        assert.deepEqual(app.selectedVerseNumbers(), []);
        assert.equal(app.actions(), null);
        const event = app.key('c', { ctrlKey: true });
        assert.ok(!event.defaultPrevented);

        app.click(app.verse(1));
        await app.copyButton();
        assert.match(app.clipboardWrites[0], / — John 4:1 \(KJV\)$/);

        app.key('ArrowLeft');
        await app.waitForHeading('John 3');
        assert.deepEqual(app.selectedVerseNumbers(), []);
    }));

test('switching translation clears the selection and copies in the new one', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        app.click(app.verse(16));
        const select = app.document.getElementById('translation-select');
        const before = app.calls.length;
        select.value = 'web';
        select.dispatchEvent(new app.window.Event('change', { bubbles: true }));
        // The heading is John 3 either way, so wait for the reload itself.
        await app.idle(before, 'get_reading');
        assert.deepEqual(app.selectedVerseNumbers(), []);

        app.click(app.verse(16));
        await app.copyButton();
        assertSameBytes(app.clipboardWrites[0], `${storedVerse('web', 'John', 3, 16)} ${EM} John 3:16 (WEB)`);
    }));

test('switching translation drops the selection before the new text arrives', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        app.click(app.verse(16));
        const release = app.holdCommand('get_reading');
        const select = app.document.getElementById('translation-select');
        select.value = 'web';
        select.dispatchEvent(new app.window.Event('change', { bubbles: true }));
        await settle();

        // The KJV chapter is still on screen, but no longer selected or
        // copyable as if it were the reader's choice.
        assert.ok(app.calls.some((c) => c.cmd === 'get_reading' && c.args.translationCode === 'web' && !c.done));
        assert.deepEqual(app.selectedVerseNumbers(), []);
        assert.equal(app.actions(), null);
        assert.ok(!app.key('c', { ctrlKey: true }).defaultPrevented);

        release();
        await app.idle(0, 'get_reading');
        assert.equal(app.verse(16).dataset.translation, 'web');
        assert.deepEqual(app.selectedVerseNumbers(), []);
        assert.equal(app.clipboardWrites.length, 0);
    }));

test('arrow navigation drops the selection before the next chapter arrives', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        app.click(app.verse(16));
        const release = app.holdCommand('get_reading');
        app.key('ArrowRight');
        await settle();
        assert.equal(app.heading(), 'John 3');
        assert.deepEqual(app.selectedVerseNumbers(), []);
        release();
        await app.waitForHeading('John 4');
        assert.deepEqual(app.selectedVerseNumbers(), []);
    }));

test('the citation names the translation that is on screen, even if a switch failed', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        app.failReading();
        const select = app.document.getElementById('translation-select');
        select.value = 'web';
        select.dispatchEvent(new app.window.Event('change', { bubbles: true }));
        await settle();
        app.click(app.verse(16));
        await app.copyButton();
        assertSameBytes(app.clipboardWrites[0], `${storedVerse('kjv', 'John', 3, 16)} ${EM} John 3:16 (KJV)`);
    }));

test('print hides the verse actions', () =>
    withApp({}, async (app) => {
        const css = app.document.querySelector('style').textContent;
        const print = css.slice(css.indexOf('@media print'));
        assert.match(print, /\.verse-actions,[^{]*\{\s*display: none !important;/);
    }));
