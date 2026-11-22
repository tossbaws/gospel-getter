// Notes beside the text (B1-B8, M1-M4): one setting, Aa's "Notes beside the
// text" (Off · In reading mode · Always), says where the reader's notes sit
// in the margin. "Always" shows the margin outside reading mode too, the
// chapter's notes beside their verses, following every change to them; a
// chapter without notes says so at the margin's top. Compare compares
// translations only. M shows or hides the notes where the reader is, and
// says what it chose; S opens this passage beside, or closes the second
// pane, leaving reading mode first. A saved Compare → Notes becomes
// "Always" before the first frame. Against the real backend; jsdom has no
// layout, so `layOut` gives the page one (the stylesheet's shape for
// "Always" is frontend_markup.rs's, and the real app's in the GUI checks).

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, clickAndWaitForFrames as clickAndWait, closeBridge, el, frame, notePanelOpen, openApp, select, settle, shown, shownText } from './harness.mjs';

after(closeBridge);

const JOHN = bookId('John');
const EMPTY = 'No notes on this chapter yet. Select verses and choose Add note.';

/**
 * A laid-out page, set up before the page's scripts run: the window
 * `width` px wide (as the page's root says), each pane's reading column
 * from 420px to 1180px and 100px down, verse n 60n px down it (50px tall,
 * its number 2px lower), each margin note 40px tall, and nothing scrolled.
 */
const layOut = (width) => (window) => {
    const html = () => window.document.documentElement;
    Object.defineProperty(window.Element.prototype, 'clientWidth', {
        configurable: true,
        get() { return this === html() ? width : 0; },
    });
    Object.defineProperty(window.HTMLElement.prototype, 'offsetHeight', {
        configurable: true,
        get() { return this.classList.contains('margin-note') ? 40 : 0; },
    });
    const measured = window.HTMLElement.prototype.getBoundingClientRect;
    window.HTMLElement.prototype.getBoundingClientRect = function () {
        const box = (top, height, left = 420, right = 1180) => ({ top, bottom: top + height, left, right, width: right - left, height, x: left, y: top });
        if (this.id === 'top-bar') return box(0, 54, 0, width);
        if (this.matches('#reading-pane-inner .chapter-main, #reading-pane-2-inner .chapter-main')) return box(100, 5000);
        if (this.matches('.chapter-main .verse[data-verse]')) return box(100 + 60 * Number(this.dataset.verse), 50);
        if (this.matches('.chapter-main .verse-num')) {
            return box(102 + 60 * Number(this.closest('.verse').dataset.verse), 12);
        }
        return measured.call(this);
    };
};

/** Adds notes (each [start, end, body], in John 3) before the app opens. */
async function seed(notes) {
    await backend('__reset', { bookId: JOHN, chapter: 3, translationCode: 'kjv' });
    for (const [verseStart, verseEnd, body] of notes) {
        await backend('add_note', { bookId: JOHN, chapter: 3, verseStart, verseEnd, body });
    }
}

async function withNotes(notes, options, body) {
    await seed(notes);
    const app = await openApp({
        book: 'John', chapter: 3, keepBookmarks: true, setUp: layOut(options.width ?? 1600), ...options,
    });
    try {
        await frame(app);
        await body(app);
    } finally {
        app.close();
    }
}

const ALWAYS = { 'gospel-getter-margin-notes': 'always' };
const margin = (app) => app.document.querySelector('.chapter-main .margin-notes');
const marginNotes = (app) => [...app.document.querySelectorAll('.margin-notes .margin-note')].map((note) => ({
    reference: note.querySelector('.margin-note-ref').textContent,
    body: shownText(note.querySelector('.margin-note-body')),
    top: note.style.top,
}));
const choices = (app) => [...el(app, 'notes-beside-choices').querySelectorAll('button')];
const choice = (app, label) => choices(app).find((b) => b.textContent === label);
const pressedChoice = (app) => choices(app).filter((b) => b.getAttribute('aria-pressed') === 'true').map((b) => b.textContent);
const setting = (app) => app.document.documentElement.dataset.marginNotes;
const toast = (app) => el(app, 'data-status').textContent;
const panes = (app) => app.window.gospelGetterPanes;
const readingMode = (app) => app.document.documentElement.dataset.readerMode === 'on';

/** Chooses `label` in Aa's "Notes beside the text". */
async function choose(app, label) {
    app.open('aa');
    app.click(choice(app, label));
    await app.idle(0);
    await frame(app);
}

/** Presses `key` and waits for every call it made and the next frame. */
async function press(app, key, opts = {}, target) {
    const before = app.calls.length;
    const event = await app.key(key, opts, target);
    await app.idle(before);
    await settle();
    await frame(app);
    return event;
}

const NOTES = [[3, 5, 'Born again.'], [4, 4, 'He takes it literally.'], [16, 16, '**Love**: see Rom 5:8']];

test('Aa has one control, Off · In reading mode · Always, there in or out of reading mode, M its key', () =>
    withNotes(NOTES, {}, async (app) => {
        app.open('aa');
        assert.deepEqual(choices(app).map((b) => b.textContent), ['Off', 'In reading mode', 'Always']);
        assert.deepEqual(pressedChoice(app), ['In reading mode'], 'the default: as it always was');
        assert.equal(setting(app), undefined, 'no attribute for the default');
        const group = el(app, 'notes-beside-choices');
        assert.equal(group.getAttribute('role'), 'group');
        assert.equal(app.document.getElementById(group.getAttribute('aria-labelledby')).textContent, 'Notes beside the text');
        assert.equal(group.getAttribute('aria-keyshortcuts'), 'M');
        assert.match(group.title, /\(M\)$/);
        assert.doesNotMatch(group.closest('.aa-section').textContent, /\(M\)/, 'no visible (M)');
        assert.equal(el(app, 'margin-notes-btn'), null, 'the old switch is gone');
        assert.equal(shown(app), 'nothing', 'out of reading mode, by default');
    }));

test('choosing Always shows the margin outside reading mode, with the chapter’s notes beside their verses', () =>
    withNotes(NOTES, {}, async (app) => {
        const text = app.document.querySelector('.chapter-main .chapter-text').outerHTML;
        await choose(app, 'Always');
        assert.equal(setting(app), 'always');
        assert.equal(app.storage()['gospel-getter-margin-notes'], 'always');
        assert.deepEqual(pressedChoice(app), ['Always']);
        assert.ok(!readingMode(app));
        assert.equal(shown(app), 'margin');
        assert.deepEqual(marginNotes(app), [
            { reference: 'John 3:3–5', body: 'Born again.', top: '180px' },
            { reference: 'John 3:4', body: 'He takes it literally.', top: '240px' },
            { reference: 'John 3:16', body: 'Love: see Rom 5:8', top: '960px' },
        ]);
        assert.equal(margin(app).getAttribute('aria-label'), 'Your notes');
        // Ten lines before "more" (B5), and as wide as the room allows.
        assert.equal(margin(app).style.getPropertyValue('--margin-fold-lines'), '10');
        assert.equal(margin(app).style.getPropertyValue('--margin-width'), '320px');
        // The text is untouched, and the page is the app's, not reading mode's.
        assert.equal(app.document.querySelector('.chapter-main .chapter-text').outerHTML, text);
        assert.ok(!el(app, 'library-toggle').hidden);
        assert.equal(app.document.querySelector('.compare-grid'), null);
        assert.deepEqual(app.consoleErrors, []);
    }));

test('under Always the margin follows the notes as they are added, edited, deleted and restored', () =>
    withNotes([], { storage: ALWAYS }, async (app) => {
        assert.equal(shown(app), 'empty');
        assert.equal(margin(app).textContent, EMPTY, 'one quiet line');
        assert.equal(margin(app).querySelector('.margin-notes-empty').textContent, EMPTY);
        await clickAndWait(app, app.verse(20), 'list_notes');
        app.click(app.actions().querySelector('.verse-note-btn'));
        app.type(el(app, 'note-input'), 'Truth comes to the light.');
        await settle();
        await clickAndWait(app, el(app, 'note-save'), 'list_all_notes');
        assert.equal(shown(app), 'margin');
        assert.deepEqual(marginNotes(app).map((n) => [n.reference, n.body, n.top]), [['John 3:20', 'Truth comes to the light.', '1200px']]);
        assert.equal(margin(app).querySelector('.margin-notes-empty'), null);
        // Edited.
        app.click(app.document.querySelector('.verse-notes .note-edit'));
        app.type(el(app, 'note-input'), 'Edited.');
        await settle();
        await clickAndWait(app, el(app, 'note-save'), 'list_all_notes');
        assert.deepEqual(marginNotes(app).map((n) => n.body), ['Edited.']);
        // Deleted, then restored with Undo.
        app.click(app.document.querySelector('.verse-notes .note-edit'));
        await clickAndWait(app, el(app, 'note-delete'), 'list_all_notes');
        assert.equal(shown(app), 'empty');
        assert.equal(margin(app).textContent, EMPTY);
        await clickAndWait(app, app.status().querySelector('.status-undo'), 'list_all_notes');
        assert.deepEqual(marginNotes(app).map((n) => n.body), ['Edited.']);
    }));

test('a chapter without notes says so at the margin’s top only once that is known, and only out of reading mode', () =>
    withNotes(NOTES, { storage: ALWAYS }, async (app) => {
        assert.equal(marginNotes(app).length, 3);
        const release = app.holdAnswer('list_notes');
        const before = app.calls.length;
        app.key('ArrowRight');
        await app.waitForHeading('John 4');
        await frame(app);
        assert.equal(shown(app), 'nothing', 'not while they are on their way');
        assert.equal(app.document.querySelectorAll('.margin-note').length, 0, 'nor John 3’s');
        release();
        await app.idle(before, 'list_notes');
        await frame(app);
        assert.equal(shown(app), 'empty');
        assert.equal(margin(app).textContent, EMPTY);
        // Reading mode keeps its empty space.
        app.click(el(app, 'reader-mode-btn'));
        await app.idle(0);
        await frame(app);
        assert.equal(shown(app), 'nothing');
        assert.equal(margin(app), null);
    }));

test('Off hides the notes everywhere; In reading mode shows them only in reading mode; Always in both', () =>
    withNotes(NOTES, {}, async (app) => {
        const where = async () => {
            const out = [shown(app)];
            app.click(el(app, 'reader-mode-btn'));
            await app.idle(0);
            await frame(app);
            out.push(shown(app));
            app.click(el(app, 'reader-exit'));
            await app.idle(0);
            await frame(app);
            return out;
        };
        assert.deepEqual(await where(), ['nothing', 'margin'], 'In reading mode');
        await choose(app, 'Off');
        assert.equal(setting(app), 'off');
        assert.deepEqual(await where(), ['nothing', 'nothing'], 'Off');
        await choose(app, 'Always');
        assert.deepEqual(await where(), ['margin', 'margin'], 'Always');
        // Not a display setting a backup carries.
        const exported = JSON.parse(JSON.stringify(app.window.gospelGetterPreferences.current()));
        assert.deepEqual(Object.keys(exported).sort(), ['compare', 'layout', 'line_spacing', 'reading_mode', 'text_size', 'theme', 'width']);
    }));

test('too narrow for the margin, Always puts a marker after each noted verse', () =>
    withNotes(NOTES, { storage: ALWAYS, width: 1200 }, async (app) => {
        assert.equal(shown(app), 'markers');
        assert.deepEqual([...app.document.querySelectorAll('.chapter-main .verse .margin-marker')].map((m) => Number(m.closest('.verse').dataset.verse)), [3, 4, 16]);
        assert.equal(margin(app), null);
        // No notes, no room: no quiet line either.
        app.key('ArrowRight');
        await app.waitForHeading('John 4');
        await app.idle(0, 'list_notes');
        await frame(app);
        assert.equal(shown(app), 'nothing');
    }));

test('Compare compares translations only: its “with” list has no Notes, and it leaves no room for the margin', () =>
    withNotes(NOTES, { storage: ALWAYS, compare: true }, async (app) => {
        app.open('translation');
        assert.deepEqual([...app.document.querySelectorAll('#compare-with-choices button')].map((b) => b.textContent), ['BSB', 'WEB']);
        assert.equal(app.document.querySelector('[data-beside]'), null);
        assert.ok(app.document.querySelector('.compare-grid'));
        assert.equal(shown(app), 'nothing');
        app.click(el(app, 'compare-btn'));
        await app.idle(0);
        await frame(app);
        assert.equal(shown(app), 'margin');
    }));

test('a saved Compare → Notes becomes Always before the first frame: Compare off, its key gone, no notes column', async () => {
    let storage;
    await withNotes(NOTES, { storage: { 'gospel-getter-compare-notes': 'on', 'gospel-getter-compare': 'on', 'gospel-getter-compare-with': 'bsb' } }, async (app) => {
        const data = app.document.documentElement.dataset;
        assert.equal(data.marginNotes, 'always');
        assert.equal(data.compare, undefined, 'Compare, which showed no translation, off');
        assert.equal(data.compareNotes, undefined);
        storage = app.storage();
        assert.equal(storage['gospel-getter-margin-notes'], 'always');
        assert.equal(storage['gospel-getter-compare'], 'off');
        assert.equal(storage['gospel-getter-compare-with'], 'bsb', 'the translation chosen stays chosen');
        assert.ok(!('gospel-getter-compare-notes' in storage));
        assert.ok(!app.calls.some((c) => c.cmd === 'get_compare'), 'no comparison fetched');
        assert.equal(app.document.querySelector('.compare-grid, .notes-column, .notes-grid'), null);
        assert.equal(shown(app), 'margin');
        assert.equal(marginNotes(app).length, 3);
    });
    // And it stays so.
    await withNotes(NOTES, { storage }, async (app) => {
        assert.equal(setting(app), 'always');
        assert.equal(shown(app), 'margin');
    });
    // Compare → Notes off: only the old key goes. Compare with a
    // translation stays as it was.
    await withNotes(NOTES, { storage: { 'gospel-getter-compare-notes': 'off', 'gospel-getter-compare': 'on' } }, async (app) => {
        assert.equal(setting(app), undefined);
        assert.equal(app.document.documentElement.dataset.compare, 'on');
        const after = app.storage();
        assert.ok(!('gospel-getter-compare-notes' in after));
        assert.ok(!('gospel-getter-margin-notes' in after));
        assert.ok(app.document.querySelector('.compare-grid'));
    });
    // A margin switch saved "on" (before "Always") is In reading mode.
    await withNotes(NOTES, { readerMode: true, storage: { 'gospel-getter-margin-notes': 'on' } }, async (app) => {
        assert.equal(setting(app), undefined, 'the default');
        app.open('aa');
        assert.deepEqual(pressedChoice(app), ['In reading mode']);
        assert.equal(shown(app), 'margin');
    });
});

test('M shows or hides the notes where the reader is, and says what it chose', () =>
    withNotes(NOTES, {}, async (app) => {
        // Out of reading mode: In reading mode → Always → In reading mode.
        let event = await press(app, 'm');
        assert.ok(event.defaultPrevented);
        assert.equal(setting(app), 'always');
        assert.equal(shown(app), 'margin');
        assert.equal(toast(app), 'Notes beside the text: Always');
        await press(app, 'M');
        assert.equal(setting(app), undefined);
        assert.equal(shown(app), 'nothing');
        assert.equal(toast(app), 'Notes beside the text: In reading mode');
        // Off, out of reading mode: Always.
        await choose(app, 'Off');
        app.document.activeElement?.blur();
        await press(app, 'm');
        assert.equal(setting(app), 'always');
        // In reading mode: anything but Off → Off; Off → In reading mode.
        app.click(el(app, 'reader-mode-btn'));
        await app.idle(0);
        await frame(app);
        assert.equal(shown(app), 'margin');
        app.document.activeElement?.blur();
        await press(app, 'm');
        assert.equal(setting(app), 'off');
        assert.equal(shown(app), 'nothing');
        assert.equal(toast(app), 'Notes beside the text: Off');
        await press(app, 'm');
        assert.equal(setting(app), undefined);
        assert.equal(shown(app), 'margin');
        assert.equal(toast(app), 'Notes beside the text: In reading mode');
        app.open('aa');
        assert.deepEqual(pressedChoice(app), ['In reading mode'], 'Aa follows');
        // Ctrl+M and Alt+M are nothing of the app's.
        for (const opts of [{ ctrlKey: true }, { altKey: true }, { metaKey: true }]) {
            event = await press(app, 'm', opts);
            assert.equal(event.defaultPrevented, false, JSON.stringify(opts));
            assert.equal(setting(app), undefined);
        }
    }));

test('S opens this passage beside, and S again closes the second pane', () =>
    withNotes(NOTES, { storage: ALWAYS }, async (app) => {
        assert.deepEqual([...panes(app).shown()], ['first']);
        let event = await press(app, 's');
        assert.ok(event.defaultPrevented);
        assert.deepEqual([...panes(app).shown()], ['first', 'second']);
        assert.equal(panes(app).active(), 'second');
        const headings = [...app.document.querySelectorAll('.chapter-heading')].map((h) => h.textContent);
        assert.deepEqual(headings, ['John 3', 'John 3']);
        // Two panes side by side: markers in each, no margin.
        assert.equal(shown(app), 'markers');
        for (const id of ['reading-pane', 'reading-pane-2']) {
            const root = el(app, id);
            assert.equal(root.querySelector('.margin-notes'), null, id);
            assert.deepEqual([...root.querySelectorAll('.verse .margin-marker')].map((m) => Number(m.closest('.verse').dataset.verse)), [3, 4, 16], id);
        }
        // Caps Lock's S closes it, as X does.
        event = await press(app, 'S', {}, app.document.activeElement ?? app.document.body);
        assert.ok(event.defaultPrevented);
        assert.deepEqual([...panes(app).shown()], ['first']);
        assert.equal(shown(app), 'margin', 'one pane: the margin again');
        // Ctrl+S stays the webview's, and Alt+S is nothing.
        for (const opts of [{ ctrlKey: true }, { altKey: true }, { metaKey: true }]) {
            event = await press(app, 's', opts);
            assert.equal(event.defaultPrevented, false, JSON.stringify(opts));
            assert.deepEqual([...panes(app).shown()], ['first']);
        }
    }));

test('S in reading mode leaves it first and opens both panes, as the palette’s Open this passage beside does', async () => {
    await withNotes(NOTES, { readerMode: true }, async (app) => {
        assert.ok(readingMode(app));
        const event = await press(app, 's');
        assert.ok(event.defaultPrevented);
        assert.ok(!readingMode(app), 'reading mode left');
        assert.deepEqual([...panes(app).shown()], ['first', 'second']);
        assert.equal(app.document.querySelectorAll('.pane:not([hidden])').length, 2, 'both shown');
    });
    await withNotes(NOTES, { readerMode: true }, async (app) => {
        app.key('/');
        app.type(app.searchInput(), '>open this passage');
        await app.idle(0);
        const option = [...app.document.querySelectorAll('#search-results .search-command')].find((o) => o.textContent.includes('Open this passage beside'));
        assert.equal(option.querySelector('kbd').textContent, 'S', 'its key shown, as Close’s X is');
        assert.equal(option.getAttribute('aria-keyshortcuts'), 'S');
        const before = app.calls.length;
        app.click(option);
        await app.idle(before);
        await settle();
        assert.ok(!readingMode(app));
        assert.deepEqual([...panes(app).shown()], ['first', 'second']);
    });
});

test('m and s typed in the search box or the note editor are letters', () =>
    withNotes(NOTES, {}, async (app) => {
        await select(app, 16);
        const keys = ['m', 's', 'M', 'S'];
        await app.key('/');
        for (const key of keys) {
            const event = await app.key(key, {}, app.searchInput());
            assert.equal(event.defaultPrevented, false, `${key} types`);
        }
        await app.key('Escape', {}, app.searchInput());
        await app.key('n');
        assert.ok(notePanelOpen(app));
        const input = el(app, 'note-input');
        for (const key of keys) {
            const event = await app.key(key, {}, input);
            assert.equal(event.defaultPrevented, false, `${key} types`);
        }
        await app.key('Escape', {}, input);
        await app.idle();
        assert.equal(setting(app), undefined, 'M changed nothing');
        assert.deepEqual([...panes(app).shown()], ['first'], 'S opened nothing');
    }));

test('with Single-key shortcuts off, M and S do nothing', () =>
    withNotes(NOTES, {}, async (app) => {
        app.open('aa');
        await app.click(el(app, 'single-keys-toggle'));
        assert.equal(app.document.documentElement.dataset.singleKeyShortcuts, 'off');
        await app.key('Escape', {}, app.document.activeElement);
        app.document.activeElement?.blur();
        for (const key of ['m', 's', 'M', 'S']) {
            const event = await press(app, key);
            assert.equal(event.defaultPrevented, false, key);
        }
        assert.equal(setting(app), undefined);
        assert.deepEqual([...panes(app).shown()], ['first']);
    }));

test('the shortcuts sheet lists S in Reading, before Switch pane, and M in Text & display', () =>
    withNotes([], {}, async (app) => {
        await app.key('?', { shiftKey: true });
        const row = (id) => app.document.querySelector(`.shortcut-row[data-shortcut="${id}"]`);
        const text = (id) => row(id).textContent.replace(/\s+/g, ' ').trim();
        const group = (id) => row(id).closest('.shortcut-group').querySelector('h3').textContent;
        assert.equal(text('split'), 'S Open this passage beside, or close the second pane');
        assert.equal(group('split'), 'Reading');
        assert.equal(row('split').nextElementSibling, row('switch-pane'));
        assert.equal(text('notes-beside'), 'M Notes beside the text, on or off where you are');
        assert.equal(group('notes-beside'), 'Text & display');
    }));
