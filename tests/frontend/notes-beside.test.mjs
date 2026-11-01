// Compare with Notes: the "with" choices' Notes puts the reader's notes on
// the chapter in a "My notes" column beside the text, outside reading
// mode. The text is drawn as normal reading draws it (Verses or Flowing);
// each note sits beside its first verse, labelled with its verses, placed
// by the margin's rules (stacked where they'd overlap, a rule beside a
// range), and the column follows every change to the notes. Too narrow for
// columns, each goes under its first verse. Reading mode keeps its margin.
// The choice is this computer's: remembered, never in a backup. Against
// the real backend; jsdom has no layout, so `layOut` gives the page one.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { backend, bookId, closeBridge, openApp, settle } from './harness.mjs';

const dir = mkdtempSync(join(tmpdir(), 'gg-notes-beside-'));
after(() => {
    closeBridge();
    rmSync(dir, { recursive: true, force: true });
});

const JOHN = bookId('John');
const EMPTY = 'No notes on this chapter yet. Select verses and choose Add note.';

/**
 * A laid-out page, set up before the page's scripts run: the window
 * `width` px wide (as the page's root says), the reading
 * column from 420px to 1180px and 100px down, verse n 60n px down it (50px
 * tall, its number 2px lower), the notes column from 800px and 140px down,
 * each note 80px tall, and nothing scrolled.
 */
const layOut = (width) => (window) => {
    const html = () => window.document.documentElement;
    Object.defineProperty(window.Element.prototype, 'clientWidth', {
        configurable: true,
        get() { return this === html() ? width : 0; },
    });
    Object.defineProperty(window.HTMLElement.prototype, 'offsetHeight', {
        configurable: true,
        get() { return this.classList.contains('margin-note') ? 80 : 0; },
    });
    const measured = window.HTMLElement.prototype.getBoundingClientRect;
    window.HTMLElement.prototype.getBoundingClientRect = function () {
        const box = (top, height, left = 420, right = 1180) => ({ top, bottom: top + height, left, right, width: right - left, height, x: left, y: top });
        if (this.id === 'top-bar') return box(0, 54, 0, 1280);
        if (this.matches('#reading-pane-inner .chapter-main')) return box(100, 5000);
        if (this.matches('.chapter-main .notes-column')) return box(140, 2000, 800, 1180);
        if (this.matches('.chapter-main .verse[data-verse]')) return box(100 + 60 * Number(this.dataset.verse), 50, 420, 780);
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
        book: 'John', chapter: 3, keepBookmarks: true, setUp: layOut(options.width ?? 1280), ...options,
    });
    try {
        await frame(app);
        await body(app);
    } finally {
        app.close();
    }
}

/** The page's next animation frames have run (the notes are placed in one). */
async function frame(app) {
    for (let i = 0; i < 2; i++) await new Promise((resolve) => app.window.requestAnimationFrame(resolve));
    await settle();
}

async function clickAndWait(app, target, cmd) {
    const before = app.calls.length;
    app.click(target);
    await app.idle(before, cmd);
    await frame(app);
}

/** A note's text as shown, its line breaks as new lines. */
function shownText(el) {
    const copy = el.cloneNode(true);
    for (const br of copy.querySelectorAll('br')) br.replaceWith('\n');
    return copy.textContent;
}

const shown = (app) => app.window.gospelGetterMargin.shown();
const column = (app) => app.document.querySelector('.chapter-main .notes-column');
const notesOf = (root) => [...root.querySelectorAll('.margin-note')].map((note) => ({
    label: note.querySelector('.margin-note-ref').textContent,
    name: note.querySelector('.margin-note-ref').getAttribute('aria-label'),
    body: shownText(note.querySelector('.margin-note-body')),
    top: note.style.top,
}));
const columnNotes = (app) => notesOf(column(app));
const withButtons = (app) => [...app.document.querySelectorAll('#compare-with-choices button')];
const notesChoice = (app) => app.document.querySelector('#compare-with-choices [data-beside="notes"]');
const pressed = (app) => withButtons(app).filter((b) => b.getAttribute('aria-pressed') === 'true').map((b) => b.textContent);
const cardNotes = (app) => [...app.document.querySelectorAll('.verse-notes .note-card .note-card-body')].map((b) => b.textContent);
const chapterText = (app) => app.document.querySelector('.chapter-main .chapter-text');
/** The chapter's verses as drawn: each one's number and words, in order. */
const verses = (app) => [...chapterText(app).querySelectorAll(':scope > .verse')]
    .map((v) => `${v.dataset.verse}|${v.querySelector('.verse-text').innerHTML}`);

/** Chooses Notes in the translation popover's "with" choices. */
async function chooseNotes(app) {
    app.open('translation');
    app.click(notesChoice(app));
    await app.idle(0);
    await frame(app);
}

const NOTES = [[3, 5, 'Born again.'], [4, 4, 'He takes it literally.'], [16, 16, '**Love**: see Rom 5:8']];

test('choosing Notes shows the chapter’s notes in a “My notes” column beside the text, drawn as normal reading draws it', () =>
    withNotes(NOTES, { compare: true }, async (app) => {
        assert.ok(app.document.querySelector('.compare-grid:not(.notes-grid)'), 'Compare with the WEB, at first');
        // The text as normal reading draws it, to compare with.
        app.click(app.document.getElementById('compare-btn'));
        await app.idle(0);
        await frame(app);
        const normal = chapterText(app).outerHTML;
        app.click(app.document.getElementById('compare-btn'));
        await app.idle(0, 'get_compare');

        // Notes, after the translations.
        app.open('translation');
        assert.deepEqual(withButtons(app).map((b) => b.textContent), ['BSB', 'WEB', 'Notes']);
        assert.equal(notesChoice(app).dataset.value, undefined, 'not a translation');
        const before = app.calls.length;
        await chooseNotes(app);
        assert.ok(!app.calls.slice(before).some((c) => c.cmd === 'get_compare'), 'no second translation is fetched');
        assert.deepEqual(pressed(app), ['Notes']);
        assert.equal(app.document.documentElement.dataset.compareNotes, 'on');
        assert.equal(app.storage()['gospel-getter-compare-notes'], 'on');

        // The text as normal reading has it, in a column headed with its
        // translation's name, beside "My notes".
        assert.equal(shown(app), 'column');
        assert.equal(chapterText(app).outerHTML, normal);
        assert.ok(app.document.querySelector('.reading-row.is-comparing.with-notes'));
        assert.deepEqual(app.compareColumns(), ['King James Version (KJV)', 'My notes']);
        assert.equal(app.document.querySelectorAll('.chapter-side').length, 0);
        assert.equal(column(app).tagName, 'SECTION');
        assert.equal(column(app).getAttribute('aria-label'), 'My notes', 'a labelled region');
        // The reference and the title name the translation alone.
        assert.equal(app.document.getElementById('ref-text').textContent, 'John 3 · KJV');
        assert.equal(app.document.title, 'John 3 · KJV — Gospel Getter');

        // Each note beside its first verse (verse 3 at 280px, the column at
        // 140px), labelled with its verses; 4's, which would overlap 3–5's,
        // 8px below it.
        assert.deepEqual(columnNotes(app), [
            { label: '3–5', name: 'John 3:3–5', body: 'Born again.', top: '140px' },
            { label: '4', name: 'John 3:4', body: 'He takes it literally.', top: '228px' },
            { label: '16', name: 'John 3:16', body: 'Love: see Rom 5:8', top: '920px' },
        ]);
        const rules = [...column(app).querySelectorAll('.margin-rule')];
        assert.equal(rules.length, 1);
        assert.deepEqual([rules[0].style.top, rules[0].style.height], ['140px', '170px']);
        // Ten lines before "more"; the column reaches down to its last note.
        assert.equal(column(app).style.getPropertyValue('--margin-fold-lines'), '10');
        assert.equal(column(app).style.minHeight, '1000px');
        // Its Markdown, as the card shows it.
        const love = column(app).querySelectorAll('.margin-note-body')[2];
        assert.equal(love.querySelector('strong').textContent, 'Love');
        assert.equal(love.querySelector('a.note-ref-link').textContent, 'Rom 5:8');
        // The same tab stops as the margin's: the label and the links.
        for (const button of column(app).querySelectorAll('.margin-note-ref')) assert.equal(button.type, 'button');
        assert.equal(column(app).querySelectorAll('[tabindex]').length, 0);
        assert.deepEqual(app.consoleErrors, []);
    }));

test('in Flowing the text flows as ever, a note beside its verse number, and Layout stays to hand', () =>
    withNotes([[16, 16, 'On the number.']], { compare: true, storage: { 'gospel-getter-layout': 'flowing', 'gospel-getter-compare-notes': 'on' } }, async (app) => {
        assert.equal(app.document.documentElement.dataset.layout, 'flowing');
        assert.equal(shown(app), 'column');
        assert.deepEqual(columnNotes(app).map((n) => n.top), ['922px']);
        // Compare's translations disable Layout; its notes don't.
        app.open('aa');
        const layouts = [...app.document.querySelectorAll('#layout-choices button')];
        assert.ok(layouts.length > 0 && layouts.every((b) => !b.disabled));
        assert.ok(app.document.getElementById('layout-note').hidden);
    }));

test('a note in the column, its label or its more opens the card with its verses selected and the note in it', () =>
    withNotes([[3, 5, 'Born again.'], [16, 16, 'Love.']], { compare: true, storage: { 'gospel-getter-compare-notes': 'on' } }, async (app) => {
        await clickAndWait(app, column(app).querySelector('.margin-note-ref'), 'list_notes');
        assert.deepEqual(app.selectedVerseNumbers(), [3, 4, 5]);
        assert.deepEqual(cardNotes(app), ['Born again.']);
        const love = [...column(app).querySelectorAll('.margin-note')].find((n) => n.textContent.includes('Love.'));
        await clickAndWait(app, love.querySelector('.margin-note-body'), 'list_notes');
        assert.deepEqual(app.selectedVerseNumbers(), [16]);
        assert.deepEqual(cardNotes(app), ['Love.']);
        assert.ok(!app.document.getElementById('note-overlay') || app.document.getElementById('note-overlay').hidden, 'edited in the card, not the column');
    }));

test('the column follows the notes as they are added, edited, deleted, restored and imported', () =>
    withNotes([], { compare: true, storage: { 'gospel-getter-compare-notes': 'on' } }, async (app) => {
        assert.equal(shown(app), 'column');
        assert.equal(column(app).textContent, EMPTY, 'one quiet line');
        assert.equal(column(app).querySelector('.notes-column-empty').textContent, EMPTY);
        const text = verses(app);
        // Added from the card.
        await clickAndWait(app, app.verse(20), 'list_notes');
        app.click(app.actions().querySelector('.verse-note-btn'));
        app.type(app.document.getElementById('note-input'), 'Truth comes to the light.');
        await settle();
        await clickAndWait(app, app.document.getElementById('note-save'), 'list_all_notes');
        assert.deepEqual(columnNotes(app).map((n) => [n.label, n.body, n.top]), [['20', 'Truth comes to the light.', '1160px']]);
        // Edited.
        app.click(app.document.querySelector('.verse-notes .note-edit'));
        app.type(app.document.getElementById('note-input'), 'Edited.');
        await settle();
        await clickAndWait(app, app.document.getElementById('note-save'), 'list_all_notes');
        assert.deepEqual(columnNotes(app).map((n) => n.body), ['Edited.']);
        // Deleted, then restored with Undo.
        app.click(app.document.querySelector('.verse-notes .note-edit'));
        await clickAndWait(app, app.document.getElementById('note-delete'), 'list_all_notes');
        assert.equal(column(app).textContent, EMPTY);
        await clickAndWait(app, app.status().querySelector('.status-undo'), 'list_all_notes');
        assert.deepEqual(columnNotes(app).map((n) => n.body), ['Edited.']);

        // Imported, merged in, then replacing them.
        const file = (body, start) => {
            const path = join(dir, `import-${start}.gospel-getter.json`);
            writeFileSync(path, JSON.stringify({
                format: 'gospel-getter-reader-data', format_version: 3, exported_at: '2026-10-31T00:00:00.000Z',
                app_version: '2.22.1', reading_position: null, bookmarks: [],
                notes: [{ book: JOHN, chapter: 3, verse_start: start, verse_end: start, body }],
            }));
            return path;
        };
        await app.pick(file('Imported.', 2));
        app.open('menu');
        await clickAndWait(app, app.document.getElementById('import-data-btn'), 'choose_import_file');
        await clickAndWait(app, app.document.getElementById('import-confirm'), 'list_all_notes');
        assert.deepEqual(columnNotes(app).map((n) => [n.label, n.body]), [['2', 'Imported.'], ['20', 'Edited.']]);
        await app.pick(file('Replaced.', 7));
        app.open('menu');
        await clickAndWait(app, app.document.getElementById('import-data-btn'), 'choose_import_file');
        app.click(app.document.querySelector('input[name="import-mode"][value="replace"]'));
        await clickAndWait(app, app.document.getElementById('import-confirm'), 'list_all_notes');
        assert.deepEqual(columnNotes(app).map((n) => [n.label, n.body]), [['7', 'Replaced.']]);
        // Through all of it, the text is as it was.
        assert.deepEqual(verses(app), text);
    }));

test('the column follows the chapter, and a chapter without notes says so only once it is known', () =>
    withNotes(NOTES, { compare: true, storage: { 'gospel-getter-compare-notes': 'on' } }, async (app) => {
        assert.equal(columnNotes(app).length, 3);
        // John 4: none. The column never says so before its notes are read.
        const release = app.holdAnswer('list_notes');
        const before = app.calls.length;
        app.key('ArrowRight');
        await app.waitForHeading('John 4');
        await frame(app);
        assert.equal(column(app).textContent, '', 'not while they are on their way');
        assert.equal(column(app).querySelectorAll('.margin-note').length, 0, 'nor John 3’s');
        release();
        await app.idle(before, 'list_notes');
        await frame(app);
        assert.equal(column(app).textContent, EMPTY);
        app.key('ArrowLeft');
        await app.waitForHeading('John 3');
        await app.idle(0, 'list_notes');
        await frame(app);
        assert.deepEqual(columnNotes(app).map((n) => n.label), ['3–5', '4', '16']);
    }));

test('choosing a translation again restores Compare’s columns, with the translation chosen before', async () => {
    let storage;
    await withNotes(NOTES, { compare: true, storage: { 'gospel-getter-compare-with': 'bsb' } }, async (app) => {
        assert.deepEqual(app.compareColumns(), ['King James Version (KJV)', 'Berean Standard Bible (BSB)']);
        await chooseNotes(app);
        assert.equal(app.storage()['gospel-getter-compare-with'], 'bsb', 'kept for later');
        assert.deepEqual(app.compareColumns(), ['King James Version (KJV)', 'My notes']);
        const bsb = withButtons(app).find((b) => b.dataset.value === 'bsb');
        const before = app.calls.length;
        app.click(bsb);
        await app.idle(before);
        await frame(app);
        // The comparison already fetched for this chapter, drawn again.
        assert.ok(!app.calls.slice(before).some((c) => c.cmd === 'get_compare'));
        assert.deepEqual(app.compareColumns(), ['King James Version (KJV)', 'Berean Standard Bible (BSB)']);
        assert.equal(column(app), null);
        assert.equal(app.document.querySelector('.notes-grid'), null);
        assert.equal(shown(app), 'nothing');
        assert.deepEqual(pressed(app), ['BSB']);
        assert.equal(app.document.documentElement.dataset.compareNotes, undefined);
        assert.equal(app.storage()['gospel-getter-compare-notes'], 'off');
        assert.equal(app.document.getElementById('ref-text').textContent, 'John 3 · KJV + BSB');
        // Compare off and on again keeps Notes, once chosen.
        await chooseNotes(app);
        app.click(app.document.getElementById('compare-btn'));
        await app.idle(0);
        await frame(app);
        assert.equal(shown(app), 'nothing');
        assert.equal(app.document.querySelectorAll('.chapter-side').length, 2, 'normal reading');
        app.click(app.document.getElementById('compare-btn'));
        await app.idle(0);
        await frame(app);
        assert.equal(shown(app), 'column');
        storage = app.storage();
    });
    // Remembered across a restart, and never in a backup.
    await withNotes(NOTES, { storage }, async (app) => {
        assert.equal(app.document.documentElement.dataset.compareNotes, 'on', 'applied before the first frame');
        assert.equal(shown(app), 'column');
        assert.deepEqual(columnNotes(app).map((n) => n.label), ['3–5', '4', '16']);
        app.open('translation');
        assert.deepEqual(pressed(app), ['Notes']);
        const exported = JSON.parse(JSON.stringify(app.window.gospelGetterPreferences.current()));
        assert.deepEqual(exported, {
            theme: 'chi-rho', text_size: 'medium', line_spacing: 'normal', width: 'normal', layout: 'verses',
            reading_mode: false, compare: true, compare_with: 'bsb',
        });
    });
});

test('reading mode keeps its margin notes, never the column, and leaves Compare’s Notes as it was', () =>
    withNotes(NOTES, { compare: true, readerMode: true, width: 1600, storage: { 'gospel-getter-compare-notes': 'on' } }, async (app) => {
        assert.equal(shown(app), 'margin');
        assert.equal(column(app), null);
        assert.equal(app.document.querySelector('.compare-grid'), null);
        assert.equal(app.document.getElementById('ref-text').textContent, 'John 3 · KJV');
        assert.deepEqual([...app.document.querySelectorAll('.margin-notes .margin-note-ref')].map((b) => b.textContent),
            ['John 3:3–5', 'John 3:4', 'John 3:16']);
        // Out of reading mode: the column.
        app.click(app.document.getElementById('reader-exit'));
        await app.idle(0);
        await frame(app);
        assert.equal(shown(app), 'column');
        assert.equal(app.document.querySelector('.margin-notes'), null);
        // And back.
        app.click(app.document.getElementById('reader-mode-btn'));
        await app.idle(0);
        await frame(app);
        assert.equal(shown(app), 'margin');
        assert.equal(column(app), null);
    }));

test('too narrow for columns, each note goes under its first verse, labelled, and a chapter without notes says so above the text', async () => {
    await withNotes(NOTES, { compare: true, width: 600, storage: { 'gospel-getter-compare-notes': 'on' } }, async (app) => {
        assert.equal(shown(app), 'under');
        assert.equal(column(app).childElementCount, 0, 'the column, hidden here, is empty');
        const under = [...app.document.querySelectorAll('.chapter-text > .note-under')];
        assert.equal(under.length, 3);
        // In the text: 3–5 after verse 3, 4 after verse 4, 16 after 16.
        const after = (n) => app.verse(n).nextElementSibling;
        assert.equal(notesOf(after(3))[0].label, '3–5');
        assert.equal(notesOf(after(4))[0].label, '4');
        assert.equal(notesOf(after(16))[0].label, '16');
        assert.equal(under[0].querySelector('.margin-note').style.top, '', 'in the flow here');
        // A click opens the card as ever.
        await clickAndWait(app, after(4).querySelector('.margin-note-ref'), 'list_notes');
        assert.deepEqual(app.selectedVerseNumbers(), [4]);
        // Wider: the column again.
        app.resize(1280, { events: ['resize'] });
        await frame(app);
        assert.equal(shown(app), 'column');
        assert.equal(app.document.querySelectorAll('.note-under').length, 0);
        assert.equal(columnNotes(app).length, 3);
    });
    await withNotes([], { compare: true, width: 600, storage: { 'gospel-getter-compare-notes': 'on' } }, async (app) => {
        assert.equal(shown(app), 'under');
        const first = chapterText(app).firstElementChild;
        assert.ok(first.classList.contains('note-under'));
        assert.equal(first.textContent, EMPTY);
    });
});
