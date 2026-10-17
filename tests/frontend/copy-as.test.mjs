// Behavior tests for Copy as… and Copy chapter: the selection or the
// chapter in each of the backend's formats (get_copy_text, the one
// formatter Copy, Copy as…, Copy chapter and the notes export share), the
// menu by the card and at the chapter's end, and what the status says.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, closeBridge, openApp, settle, storedVerse } from './harness.mjs';

after(closeBridge);

const EM = '—';
const EN = '–';

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const FORMATS = [
    ['Text with reference', 'reference', ''],
    ['With verse numbers', 'numbered', ' with verse numbers'],
    ['Markdown quote', 'markdown', ' as a Markdown quote'],
    ['Plain text', 'plain', ' as plain text'],
];

const menu = (app) => app.document.getElementById('copy-as-menu');
const isOpen = (app) => !!menu(app) && menu(app).isConnected && !menu(app).hidden;
const items = (app) => [...menu(app).querySelectorAll('[role="menuitem"]')];
const copyAsButton = (app) => app.document.querySelector('.verse-actions .verse-copy-as-btn');
const chapterCopyButton = (app) => app.document.querySelector('.chapter-copy-btn');

async function select(app, start, end, code) {
    const before = app.calls.length;
    app.click(app.verse(start, code));
    if (end !== start) app.click(app.verse(end, code), { shiftKey: true });
    await app.idle(before);
}

async function choose(app, label) {
    const item = items(app).find((i) => i.textContent === label);
    assert.ok(item, label);
    const before = app.calls.length;
    app.click(item);
    await app.idle(Math.max(0, before - 1));
}

test('Copy as… sits beside Copy, and Ctrl+Shift+C opens its menu by the card', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        // Nothing selected: Ctrl+Shift+C is left alone.
        const idle = app.key('C', { ctrlKey: true, shiftKey: true });
        assert.ok(!idle.defaultPrevented);
        assert.ok(!isOpen(app));

        await select(app, 16, 17);
        const button = copyAsButton(app);
        assert.equal(button.previousElementSibling, app.document.querySelector('.verse-actions .verse-copy-btn'));
        assert.equal(button.textContent, 'Copy as…');
        assert.equal(button.getAttribute('aria-haspopup'), 'menu');
        assert.equal(button.getAttribute('aria-expanded'), 'false');
        assert.equal(button.getAttribute('aria-keyshortcuts'), 'Control+Shift+C');

        const event = app.key('C', { ctrlKey: true, shiftKey: true });
        assert.ok(event.defaultPrevented);
        assert.ok(isOpen(app));
        const card = app.document.getElementById('selection-card');
        assert.equal(menu(app).parentElement, card, 'in the card');
        assert.equal(menu(app).previousElementSibling, app.actions(), 'under its actions');
        assert.equal(menu(app).getAttribute('role'), 'menu');
        assert.equal(menu(app).getAttribute('aria-label'), `Copy John 3:16${EN}17 (WEB) as`);
        assert.equal(button.getAttribute('aria-controls'), 'copy-as-menu');
        assert.equal(button.getAttribute('aria-expanded'), 'true');
        assert.deepEqual(items(app).map((i) => i.textContent), FORMATS.map(([label]) => label));
        assert.equal(app.document.activeElement, items(app)[0]);
        // Nothing is copied until a format is chosen; Copy itself still is.
        assert.equal(app.clipboardWrites.length, 0);
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17]);

        // The button opens and closes it too.
        app.click(button);
        assert.ok(!isOpen(app));
        assert.equal(button.getAttribute('aria-expanded'), 'false');
        app.click(button);
        assert.ok(isOpen(app));
    }));

test('each format copies exactly what the backend formats, and says so', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        const expected = await backend('get_copy_text', { translationCode: 'web', bookId: bookId('John'), chapter: 3, start: 16, end: 17 });
        const [v16, v17] = [16, 17].map((n) => storedVerse('web', 'John', 3, n));
        // The formats themselves (the backend's tests pin them in detail).
        assert.equal(expected.formats.reference, `${v16} ${v17} ${EM} John 3:16${EN}17 (WEB)`);
        assert.equal(expected.formats.numbered, `16 ${v16}\n17 ${v17}\n${EM} John 3:16${EN}17 (WEB)`);
        assert.equal(expected.formats.markdown, `> **16** ${v16} **17** ${v17}\n>\n> ${EM} John 3:16${EN}17 (WEB)`);
        assert.equal(expected.formats.plain, `${v16} ${v17}`);

        await select(app, 16, 17);
        for (const [label, format, said] of FORMATS) {
            app.click(copyAsButton(app));
            await choose(app, label);
            assert.equal(app.clipboardWrites.at(-1), expected.formats[format], label);
            assert.equal(app.status().textContent, `Copied John 3:16${EN}17${said}`, label);
            assert.ok(!isOpen(app), 'choosing closes the menu');
            assert.equal(app.document.activeElement, copyAsButton(app), 'focus goes back to Copy as…');
            assert.deepEqual(app.selectedVerseNumbers(), [16, 17], 'the selection stays');
        }
        assert.equal(app.clipboardWrites.length, 4);

        // Ctrl+C is Copy as before: text with its reference.
        app.key('c', { ctrlKey: true });
        await settle();
        assert.equal(app.clipboardWrites.at(-1), expected.formats.reference);
        assert.equal(app.status().textContent, `Copied John 3:16${EN}17`);
    }));

test('Psalm 23:1 in the KJV: supplied words as words, or italics in Markdown; no margin notes', () =>
    withApp({ book: 'Psalms', chapter: 23, translation: 'kjv' }, async (app) => {
        await select(app, 1, 1);
        const copied = {};
        for (const [label, format] of FORMATS) {
            app.click(copyAsButton(app));
            await choose(app, label);
            copied[format] = app.clipboardWrites.at(-1);
        }
        const one = '[A Psalm of David.] The LORD is my shepherd; I shall not want.';
        assert.deepEqual(copied, {
            reference: `${one} ${EM} Psalms 23:1 (KJV)`,
            numbered: `1 ${one}\n${EM} Psalms 23:1 (KJV)`,
            markdown: `> **1** \\[A Psalm of David.\\] The LORD *is* my shepherd; I shall not want.\n>\n> ${EM} Psalms 23:1 (KJV)`,
            plain: one,
        });
    }));

test('a BSB range across a verse it doesn’t number skips it, in every format', () =>
    withApp({ book: 'Matthew', chapter: 17, translation: 'bsb' }, async (app) => {
        await select(app, 20, 22, 'bsb');
        app.click(copyAsButton(app));
        await choose(app, 'With verse numbers');
        assert.equal(
            app.clipboardWrites.at(-1),
            `20 ${storedVerse('bsb', 'Matthew', 17, 20)}\n22 ${storedVerse('bsb', 'Matthew', 17, 22)}\n${EM} Matthew 17:20${EN}22 (BSB)`,
        );
        assert.ok(!app.clipboardWrites.at(-1).includes('fasting'), 'no footnote');
    }));

test('the menu’s keys: arrows, Home and End move; Escape and Tab close it; no chapter turns', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        await select(app, 16, 16);
        app.key('C', { ctrlKey: true, shiftKey: true });
        const all = items(app);
        const at = () => all.indexOf(app.document.activeElement);
        const press = (key) => app.key(key, {}, app.document.activeElement);
        press('ArrowDown');
        assert.equal(at(), 1);
        press('End');
        assert.equal(at(), 3);
        press('ArrowDown');
        assert.equal(at(), 0, 'wraps');
        press('ArrowUp');
        assert.equal(at(), 3);
        press('Home');
        assert.equal(at(), 0);
        press('ArrowRight');
        press('ArrowLeft');
        await app.idle(0);
        assert.equal(app.heading(), 'John 3');

        // Escape closes the menu, and only the menu.
        const esc = press('Escape');
        assert.ok(esc.defaultPrevented);
        assert.ok(!isOpen(app));
        assert.deepEqual(app.selectedVerseNumbers(), [16]);
        assert.equal(app.document.activeElement, copyAsButton(app));
        app.key('Escape', {}, app.document.activeElement);
        assert.deepEqual(app.selectedVerseNumbers(), [], 'then the selection');

        // Tab closes it too.
        await select(app, 16, 16);
        app.key('C', { ctrlKey: true, shiftKey: true });
        press('Tab');
        assert.ok(!isOpen(app));
        assert.equal(app.document.activeElement, copyAsButton(app));
    }));

test('the menu closes on a click elsewhere, a new selection or another chapter', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        await select(app, 16, 16);
        app.click(copyAsButton(app));
        assert.ok(isOpen(app));
        await select(app, 18, 18);
        assert.ok(!isOpen(app));
        app.click(copyAsButton(app));
        app.click(app.document.querySelector('.chapter-main .chapter-heading'));
        assert.ok(!isOpen(app));

        app.click(chapterCopyButton(app));
        assert.ok(isOpen(app));
        const before = app.calls.length;
        app.key('ArrowRight');
        await app.idle(before, 'get_reading');
        assert.equal(app.heading(), 'John 4');
        assert.ok(!isOpen(app));
    }));

test('Copy chapter at the chapter’s end copies the whole chapter in the format chosen', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        const button = chapterCopyButton(app);
        assert.equal(button.textContent, 'Copy chapter…');
        assert.equal(button.getAttribute('aria-haspopup'), 'menu');
        // Under the keys' hint, after the next-chapter buttons, not among them.
        assert.equal(button.closest('.chapter-end'), null);
        assert.equal(button.parentElement.previousElementSibling.className, 'nav-hint');

        app.click(button);
        assert.ok(isOpen(app));
        assert.equal(menu(app).previousElementSibling, button.parentElement);
        assert.equal(menu(app).getAttribute('aria-label'), 'Copy John 3 as');
        assert.equal(button.getAttribute('aria-expanded'), 'true');
        assert.equal(app.document.activeElement, items(app)[0]);
        await choose(app, 'With verse numbers');

        const verses = app.document.querySelectorAll('.chapter-main .verse[data-translation="web"]').length;
        const lines = [];
        for (let n = 1; n <= verses; n++) lines.push(`${n} ${storedVerse('web', 'John', 3, n)}`);
        assert.equal(app.clipboardWrites.at(-1), `${lines.join('\n')}\n${EM} John 3 (WEB)`);
        // The status shows in the toast (there's no card).
        assert.equal(app.toast().textContent, 'Copied John 3 with verse numbers');
        assert.ok(!app.toast().classList.contains('is-hidden'));
        assert.equal(app.toast().getAttribute('role'), 'status');
        assert.equal(app.document.activeElement, button);

        const expected = await backend('get_copy_text', { translationCode: 'web', bookId: bookId('John'), chapter: 3 });
        for (const [label, format, said] of FORMATS) {
            app.click(chapterCopyButton(app));
            await choose(app, label);
            assert.equal(app.clipboardWrites.at(-1), expected.formats[format], label);
            assert.equal(app.toast().textContent, `Copied John 3${said}`);
        }
    }));

test('Copy chapter copies the translation being read, and skips verses it doesn’t number', () =>
    withApp({ book: 'Matthew', chapter: 17, translation: 'bsb', compare: true }, async (app) => {
        assert.ok(app.isComparing());
        app.click(chapterCopyButton(app));
        await choose(app, 'With verse numbers');
        const copied = app.clipboardWrites.at(-1);
        assert.ok(copied.endsWith(`\n${EM} Matthew 17 (BSB)`), copied.slice(-40));
        const numbers = copied.split('\n').slice(0, -1).map((line) => Number(line.split(' ')[0]));
        assert.deepEqual(numbers, [...Array(27).keys()].map((i) => i + 1).filter((n) => n !== 21));
    }));

test('a failed copy says so, in the toast for a chapter', () =>
    withApp({ book: 'John', chapter: 3, clipboard: 'reject' }, async (app) => {
        app.click(chapterCopyButton(app));
        await choose(app, 'Plain text');
        assert.match(app.toast().textContent, /^Couldn't copy: The request is not allowed/);
        assert.ok(app.toast().classList.contains('is-error'));
    }));
