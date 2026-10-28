// Margin notes in reading mode: the reader's notes on the chapter in the
// open space to the right of the text, each beside its first verse (on the
// verse number in Flowing), a thin rule beside a range's verses, notes that
// would overlap stacked downward in verse order; a marker after each noted
// verse where the window is too narrow; none while comparing, out of
// reading mode, or with Aa's "Notes in the margin" off, a setting kept on
// this computer and never in a backup. Read-only: a note opens the
// selection card with its verses selected. Against the real backend.
// jsdom has no layout: `layOut` gives the page one.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, closeBridge, openApp, settle } from './harness.mjs';

after(closeBridge);

const JOHN = bookId('John');
const MATTHEW = bookId('Matthew');

/**
 * A laid-out page, set up before the page's scripts run: the window
 * `width` px wide (as the page's root says), the reading column from 420px
 * to 1180px and 100px down, verse n 60n px down the column (50px tall, its
 * number 2px lower), each margin note 40px tall, and nothing scrolled.
 */
function layOut(width) {
    return (window) => {
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
            if (this.matches('#reading-pane-inner .chapter-main')) return box(100, 5000);
            if (this.matches('.chapter-main .verse[data-verse]')) return box(100 + 60 * Number(this.dataset.verse), 50);
            if (this.matches('.chapter-main .verse-num')) {
                return box(102 + 60 * Number(this.closest('.verse').dataset.verse), 12);
            }
            return measured.call(this);
        };
    };
}

/** Adds notes (each [start, end, body], in `book` `chapter`) before the app opens. */
async function seed(notes, book = JOHN, chapter = 3, translationCode = 'kjv') {
    await backend('__reset', { bookId: book, chapter, translationCode });
    const added = [];
    for (const [verseStart, verseEnd, body] of notes) {
        added.push(await backend('add_note', { bookId: book, chapter, verseStart, verseEnd, body }));
    }
    return added;
}

async function withMargin(notes, options, body) {
    await seed(notes, options.bookId ?? JOHN, options.chapter ?? 3, options.translation ?? 'kjv');
    const app = await openApp({
        book: 'John', chapter: 3, readerMode: true, keepBookmarks: true, setUp: layOut(options.width ?? 1600), ...options,
    });
    try {
        await frame(app);
        await body(app);
    } finally {
        app.close();
    }
}

/** The page's next animation frames have run (the margin is placed in one). */
async function frame(app) {
    for (let i = 0; i < 2; i++) await new Promise((resolve) => app.window.requestAnimationFrame(resolve));
    await settle();
}

/** A note's text as shown, its line breaks (`<br>`, the note as Markdown) as new lines. */
function shownText(el) {
    const copy = el.cloneNode(true);
    for (const br of copy.querySelectorAll('br')) br.replaceWith('\n');
    return copy.textContent;
}

const margin = (app) => app.document.querySelector('.chapter-main .margin-notes');
const shown = (app) => app.window.gospelGetterMargin.shown();
const marginNotes = (app) => [...app.document.querySelectorAll('.margin-notes .margin-note')].map((note) => ({
    label: note.getAttribute('aria-label'),
    reference: note.querySelector('.margin-note-ref').textContent,
    body: shownText(note.querySelector('.margin-note-body')),
    top: note.style.top,
}));
const markers = (app) => [...app.document.querySelectorAll('.chapter-main .verse .margin-marker')].map((m) => ({
    verse: Number(m.closest('.verse').dataset.verse),
    label: m.getAttribute('aria-label'),
}));
const cardNotes = (app) => [...app.document.querySelectorAll('.verse-notes .note-card .note-card-body')].map((b) => b.textContent);

async function clickAndWait(app, target, cmd) {
    const before = app.calls.length;
    app.click(target);
    await app.idle(before, cmd);
    await frame(app);
}

test('in reading mode the notes sit in the margin beside their verses, in verse order, as a labelled landmark', () =>
    withMargin([[21, 21, 'Light.'], [16, 18, 'For God so loved.\nThe world.'], [16, 16, 'Again on 16.']], {}, async (app) => {
        assert.equal(shown(app), 'margin');
        const aside = margin(app);
        assert.equal(aside.getAttribute('role'), 'complementary');
        assert.equal(aside.getAttribute('aria-label'), 'Your notes');
        // After the chapter's text in reading order.
        const text = app.document.querySelector('.chapter-main .chapter-text');
        assert.ok(text.compareDocumentPosition(aside) & app.window.Node.DOCUMENT_POSITION_FOLLOWING);
        // In verse order, as the card lists them (16 before 16–18).
        assert.deepEqual(marginNotes(app), [
            { label: 'Note on John 3:16', reference: 'John 3:16', body: 'Again on 16.', top: '960px' },
            // Would overlap the one above: stacked below it, 8px apart.
            { label: 'Note on John 3:16–18', reference: 'John 3:16–18', body: 'For God so loved.\nThe world.', top: '1008px' },
            { label: 'Note on John 3:21', reference: 'John 3:21', body: 'Light.', top: '1260px' },
        ]);
        // The range's rule runs beside verses 16 to 18.
        const rules = [...aside.querySelectorAll('.margin-rule')];
        assert.equal(rules.length, 1);
        assert.equal(rules[0].getAttribute('aria-hidden'), 'true');
        assert.deepEqual([rules[0].style.top, rules[0].style.height], ['960px', '170px']);
        // 1600px wide, the column ending at 1180px: 380px beside it, at
        // most 20rem of it.
        assert.equal(aside.style.getPropertyValue('--margin-width'), '320px');
        // The notes are buttons a keyboard reaches; nothing traps focus.
        for (const button of aside.querySelectorAll('.margin-note-ref')) assert.equal(button.type, 'button');
        assert.deepEqual(app.consoleErrors, []);
    }));

test('in Flowing a note sits beside its verse number', () =>
    withMargin([[16, 16, 'On the number.']], { storage: { 'gospel-getter-layout': 'flowing' } }, async (app) => {
        assert.equal(app.document.documentElement.dataset.layout, 'flowing');
        assert.deepEqual(marginNotes(app).map((n) => n.top), ['962px']);
    }));

test('a margin note, its reference or its more opens the card with its verses selected and the note in it', () =>
    withMargin([[16, 18, 'For God so loved.'], [21, 21, 'Light.']], {}, async (app) => {
        await clickAndWait(app, margin(app).querySelector('.margin-note[data-note-id] .margin-note-ref'), 'list_notes');
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17, 18]);
        assert.deepEqual(cardNotes(app), ['For God so loved.']);
        // A click anywhere on another note does the same; the click goes no
        // further (the selection isn't cleared as a click away would).
        const light = [...margin(app).querySelectorAll('.margin-note')].find((n) => n.textContent.includes('Light.'));
        await clickAndWait(app, light.querySelector('.margin-note-body'), 'list_notes');
        assert.deepEqual(app.selectedVerseNumbers(), [21]);
        assert.deepEqual(cardNotes(app), ['Light.']);
        assert.ok(!app.document.getElementById('note-overlay') || app.document.getElementById('note-overlay').hidden, 'no editor');
    }));

test('a margin note shows its Markdown, and a reference in it goes to its passage rather than opening the card', () =>
    withMargin([[16, 16, '**Loved**: see Rom 5:8\n- the world\n- whosoever']], {}, async (app) => {
        const body = margin(app).querySelector('.margin-note-body');
        assert.ok(body.classList.contains('note-text'));
        assert.equal(body.querySelector('strong').textContent, 'Loved');
        assert.deepEqual([...body.querySelectorAll('li')].map((li) => li.textContent), ['the world', 'whosoever']);
        const link = body.querySelector('a.note-ref-link');
        assert.equal(link.textContent, 'Rom 5:8');
        await clickAndWait(app, link, 'get_reading');
        await app.waitForHeading('Romans 5');
        assert.deepEqual(app.selectedVerseNumbers(), [8]);
    }));

test('the margin follows the notes as they are added, edited, deleted and restored', () =>
    withMargin([], {}, async (app) => {
        assert.equal(shown(app), 'nothing', 'no notes, no margin');
        assert.equal(margin(app), null);
        // Added from the card.
        await clickAndWait(app, app.verse(20), 'list_notes');
        app.click(app.actions().querySelector('.verse-note-btn'));
        app.type(app.document.getElementById('note-input'), 'Truth comes to the light.');
        await settle();
        await clickAndWait(app, app.document.getElementById('note-save'), 'list_all_notes');
        assert.equal(shown(app), 'margin');
        assert.deepEqual(marginNotes(app).map((n) => [n.reference, n.body, n.top]), [['John 3:20', 'Truth comes to the light.', '1200px']]);
        // Edited.
        app.click(app.document.querySelector('.verse-notes .note-edit'));
        app.type(app.document.getElementById('note-input'), 'Edited.');
        await settle();
        await clickAndWait(app, app.document.getElementById('note-save'), 'list_all_notes');
        assert.deepEqual(marginNotes(app).map((n) => n.body), ['Edited.']);
        // Deleted, then restored with Undo.
        app.click(app.document.querySelector('.verse-notes .note-edit'));
        await clickAndWait(app, app.document.getElementById('note-delete'), 'list_all_notes');
        assert.equal(margin(app), null);
        assert.equal(shown(app), 'nothing');
        await clickAndWait(app, app.status().querySelector('.status-undo'), 'list_all_notes');
        assert.deepEqual(marginNotes(app).map((n) => n.body), ['Edited.']);
    }));

test('too narrow for the margin: a marker after each noted verse opens the card with its note', () =>
    withMargin([[16, 18, 'For God so loved.'], [21, 21, 'Light.']], { width: 1400 }, async (app) => {
        // 220px beside the column: under 14rem once the gap and the edge
        // are taken.
        assert.equal(shown(app), 'markers');
        assert.equal(margin(app), null);
        assert.deepEqual(markers(app), [
            { verse: 16, label: 'Note on John 3:16–18' },
            { verse: 21, label: 'Note on John 3:21' },
        ]);
        const marker = app.verse(21).querySelector('.margin-marker');
        assert.equal(marker.type, 'button');
        await clickAndWait(app, marker, 'list_notes');
        assert.deepEqual(app.selectedVerseNumbers(), [21]);
        assert.deepEqual(cardNotes(app), ['Light.']);
    }));

test('a note on a verse the translation does not number sits by the verse before it, marked, and opens across the gap', () =>
    withMargin([[21, 21, 'The verse the BSB leaves out.']], {
        book: 'Matthew', chapter: 17, translation: 'bsb', bookId: MATTHEW,
    }, async (app) => {
        assert.equal(app.document.querySelector('.chapter-main .verse[data-verse="21"]'), null);
        const [note] = marginNotes(app);
        assert.equal(note.label, 'Note on Matthew 17:21');
        assert.equal(note.reference, 'Matthew 17:21 (not numbered in this translation)');
        assert.equal(note.top, '1200px', 'beside verse 20');
        assert.ok(margin(app).querySelector('.margin-note').classList.contains('is-elsewhere'));
        await clickAndWait(app, margin(app).querySelector('.margin-note-ref'), 'list_notes');
        assert.deepEqual(app.selectedVerseNumbers('bsb'), [20, 22]);
        assert.deepEqual(cardNotes(app), ['The verse the BSB leaves out.']);
    }));

test('none while comparing, out of reading mode, or with the switch off; the switch is remembered and never backed up', async () => {
    let storage;
    await withMargin([[16, 16, 'For God so loved.']], {}, async (app) => {
        assert.equal(shown(app), 'margin');
        // Aa's switch, with reading mode's: on by default.
        const toggle = app.document.getElementById('margin-notes-btn');
        assert.equal(toggle.textContent, 'Notes in the margin');
        assert.equal(toggle.getAttribute('aria-pressed'), 'true');
        app.click(toggle);
        await frame(app);
        assert.equal(toggle.getAttribute('aria-pressed'), 'false');
        assert.equal(app.document.documentElement.dataset.marginNotes, 'off');
        assert.equal(shown(app), 'nothing');
        assert.equal(margin(app), null);
        // Not a display setting a backup carries.
        const exported = JSON.parse(JSON.stringify(app.window.gospelGetterPreferences.current()));
        assert.deepEqual(Object.keys(exported).sort(), ['compare', 'layout', 'line_spacing', 'reading_mode', 'text_size', 'theme', 'width']);
        app.click(toggle);
        await frame(app);
        assert.equal(shown(app), 'margin');
        // Compare: no room, no margin.
        app.click(app.document.getElementById('compare-btn'));
        await app.idle(0, 'get_compare');
        await frame(app);
        assert.equal(shown(app), 'nothing');
        assert.equal(margin(app), null);
        app.click(app.document.getElementById('compare-btn'));
        await app.idle();
        await frame(app);
        assert.equal(shown(app), 'margin');
        // Out of reading mode: none.
        app.click(app.document.getElementById('reader-exit'));
        await frame(app);
        assert.equal(shown(app), 'nothing');
        assert.equal(margin(app), null);
        assert.equal(app.document.querySelectorAll('.margin-marker').length, 0);
        // Off again, for the restart.
        app.click(app.document.getElementById('reader-mode-btn'));
        app.click(toggle);
        await frame(app);
        assert.equal(shown(app), 'nothing');
        storage = app.storage();
    });
    assert.equal(storage['gospel-getter-margin-notes'], 'off');
    await withMargin([[16, 16, 'For God so loved.']], { storage }, async (app) => {
        assert.equal(app.document.documentElement.dataset.marginNotes, 'off', 'applied before the first frame');
        assert.equal(app.document.getElementById('margin-notes-btn').getAttribute('aria-pressed'), 'false');
        assert.equal(shown(app), 'nothing');
    });
});
