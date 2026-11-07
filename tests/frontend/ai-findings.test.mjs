// AI-flagged findings (the governing doc's C1-C10): markers an AI agent
// saved, at the reader's request, through gospel-getter-cli (here, the
// bridge's `__save_findings`, as the CLI's save would). In reading mode each
// verse's findings sit in the margin after the reader's own notes there,
// collapsed as the violet "AI" badge and the verse until clicked (C7); a
// flagged verse is underlined, dotted, beside its highlight and pencil
// (C8, C9); "Keep as mine" opens the note editor with the finding's words,
// all selected (C10); the Library groups them by request, apart from My
// notes, with Remove all (C4); Aa's switch is off until turned on (C6).
// Against the real backend. jsdom has no layout: `layOut` gives the page one.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { backend, bookId, closeBridge, openApp, settle } from './harness.mjs';

const dir = mkdtempSync(join(tmpdir(), 'gospel-getter-findings-'));
after(async () => {
    await closeBridge();
    rmSync(dir, { recursive: true, force: true });
});

const JOHN = bookId('John');
const QUOTE = (request) => `Found by Claude for your request “${request}”.`;

/** A laid-out page, as margin-notes.test.mjs lays one out (see there). */
function layOut(width) {
    return (window) => {
        const html = () => window.document.documentElement;
        Object.defineProperty(window.Element.prototype, 'clientWidth', {
            configurable: true,
            get() { return this === html() ? width : 0; },
        });
        Object.defineProperty(window.HTMLElement.prototype, 'offsetHeight', {
            configurable: true,
            get() {
                if (this.classList.contains('margin-note')) return 40;
                if (this.classList.contains('margin-finding')) return this.classList.contains('is-open') ? 60 : 20;
                return 0;
            },
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

/** Saves findings as an agent would: `{ request, verses: [n…], chapter }` in John. */
async function save({ request, verses, chapter = 11, agent = 'Claude' }) {
    return backend('__save_findings', {
        request,
        agent,
        references: verses.map((verse) => ({ bookId: JOHN, chapter, verse })),
    });
}

/**
 * Opens John `chapter` in reading mode, `width` px wide, after `seed` (an
 * async function given the backend) has set things up.
 */
async function withFindings({ chapter = 11, width = 1600, readerMode = true, seed = async () => {}, ...options }, body) {
    await backend('__reset', { bookId: JOHN, chapter, translationCode: 'kjv' });
    await seed();
    const app = await openApp({ book: 'John', chapter, readerMode, keepBookmarks: true, setUp: layOut(width), ...options });
    try {
        await frame(app);
        await body(app);
    } finally {
        app.close();
    }
}

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

const $ = (app, selector) => app.document.querySelector(selector);
const $$ = (app, selector) => [...app.document.querySelectorAll(selector)];
const entry = (app, verse) => $(app, `.margin-notes .margin-finding[data-finding-key="${verse}-${verse}"]`);
const head = (app, verse) => entry(app, verse).querySelector('.margin-finding-head');
const list = (app, verse) => entry(app, verse).querySelector('.finding-list');
const isOpen = (app, verse) => head(app, verse).getAttribute('aria-expanded') === 'true' && !list(app, verse).hidden;
const texts = (root) => [...root.querySelectorAll('.finding-text')].map((t) => t.textContent);

test('a finding opens collapsed as the AI badge and its verse; a click, Enter or Space opens and folds it', () =>
    withFindings({ seed: () => save({ request: 'grief', verses: [33, 35] }) }, async (app) => {
        assert.equal(app.window.gospelGetterMargin.shown(), 'margin', 'findings alone fill the margin');
        const aside = $(app, '.chapter-main .margin-notes');
        assert.deepEqual($$(app, '.margin-notes .margin-finding').map((e) => e.querySelector('.margin-finding-head').textContent), [
            'AI John 11:33',
            'AI John 11:35',
        ]);
        assert.equal(head(app, 35).querySelector('.ai-badge').textContent, 'AI');
        assert.equal(entry(app, 35).getAttribute('aria-label'), 'AI finding on John 11:35');
        assert.equal(head(app, 35).type, 'button');
        assert.equal(head(app, 35).getAttribute('aria-controls'), list(app, 35).id);
        assert.ok(!isOpen(app, 35), 'collapsed');
        assert.ok(aside.contains(entry(app, 35)));

        // Open: who found it and for what, never a word about the verse.
        app.click(head(app, 35));
        await frame(app);
        assert.ok(isOpen(app, 35));
        assert.ok(entry(app, 35).classList.contains('is-open'));
        assert.deepEqual(texts(entry(app, 35)), [QUOTE('grief')]);
        assert.deepEqual([...entry(app, 35).querySelectorAll('.finding-actions button')].map((b) => b.textContent), ['Keep as mine', 'Remove']);
        assert.deepEqual(app.selectedVerseNumbers(), [], 'no verse is selected by it');
        assert.ok(!isOpen(app, 33), 'each opens alone');
        // Again: folded.
        app.click(head(app, 35));
        await frame(app);
        assert.ok(!isOpen(app, 35));

        // The keyboard: Enter and Space, on the focused toggle.
        head(app, 33).focus();
        const enter = app.key('Enter', {}, head(app, 33));
        assert.ok(enter.defaultPrevented);
        assert.ok(isOpen(app, 33));
        app.key(' ', {}, head(app, 33));
        assert.ok(!isOpen(app, 33));
        assert.deepEqual(app.consoleErrors, []);
    }));

test('opened, a finding opens collapsed again next time the chapter is shown', () =>
    withFindings({ seed: () => save({ request: 'grief', verses: [35] }) }, async (app) => {
        app.click(head(app, 35));
        await frame(app);
        assert.ok(isOpen(app, 35));
        await clickAndWait(app, $(app, '#chapter-next'), 'list_findings');
        await app.waitForHeading('John 12');
        await clickAndWait(app, $(app, '#chapter-prev'), 'list_findings');
        await app.waitForHeading('John 11');
        assert.ok(!isOpen(app, 35), 'nothing remembers it was open');
    }));

test('several findings on one verse share one badge, AI ×2, that lists each request', () =>
    withFindings({
        seed: async () => {
            await save({ request: 'grief', verses: [37] });
            await save({ request: 'death', verses: [37], agent: 'Codex' });
        },
    }, async (app) => {
        assert.equal($$(app, '.margin-notes .margin-finding').length, 1);
        assert.equal(head(app, 37).querySelector('.ai-badge').textContent, 'AI ×2');
        assert.equal(entry(app, 37).getAttribute('aria-label'), '2 AI findings on John 11:37');
        app.click(head(app, 37));
        await frame(app);
        assert.deepEqual(texts(entry(app, 37)), [QUOTE('grief'), 'Found by Codex for your request “death”.']);
        assert.equal(entry(app, 37).querySelectorAll('.finding-keep').length, 2, 'no bulk keep: one each');
    }));

test('a verse with a note, a highlight and a finding keeps all three marks, the reader’s note first', () =>
    withFindings({
        seed: async () => {
            await backend('add_note', { bookId: JOHN, chapter: 11, verseStart: 35, verseEnd: 35, body: 'He wept anyway.' });
            await backend('set_highlight', { bookId: JOHN, chapter: 11, verseStart: 35, verseEnd: 35, color: 'yellow' });
            await save({ request: 'grief', verses: [35] });
        },
    }, async (app) => {
        const verse = app.verse(35);
        assert.equal(verse.dataset.highlight, 'yellow');
        assert.ok(verse.classList.contains('has-notes'), 'the pencil');
        assert.ok(verse.classList.contains('has-finding'), 'the dotted violet underline');
        assert.equal(verse.querySelector('.finding-cue').textContent, '(AI finding) ');
        assert.ok(!app.verse(34).classList.contains('has-finding'));
        // In the margin: the note, then the badge below it (C9).
        const items = $$(app, '.margin-notes .margin-note, .margin-notes .margin-finding');
        assert.deepEqual(items.map((el) => el.className.split(' ')[0]), ['margin-note', 'margin-finding']);
        const note = $(app, '.margin-notes .margin-note');
        assert.ok(parseFloat(entry(app, 35).style.top) > parseFloat(note.style.top), 'stacked after the note');
        // Removing the finding leaves the note, the highlight and the pencil.
        app.click(head(app, 35));
        await clickAndWait(app, entry(app, 35).querySelector('.finding-remove'), 'list_findings');
        assert.equal(entry(app, 35), null);
        assert.ok(!app.verse(35).classList.contains('has-finding'));
        assert.ok(app.verse(35).classList.contains('has-notes'));
        assert.equal(app.verse(35).dataset.highlight, 'yellow');
        assert.equal(app.toast().textContent.startsWith('Removed the AI finding on John 11:35 for “grief”.'), true);
        // Undo brings it back.
        await clickAndWait(app, app.toast().querySelector('button'), 'restore_findings');
        await app.idle();
        await frame(app);
        assert.ok(entry(app, 35));
        assert.equal((await backend('list_findings', { bookId: JOHN, chapter: 11 })).length, 1);
    }));

test('a narrow window shows both markers after the verse, the pencil first, and the AI marker opens the findings there', () =>
    withFindings({
        width: 1400,
        seed: async () => {
            await backend('add_note', { bookId: JOHN, chapter: 11, verseStart: 35, verseEnd: 35, body: 'He wept anyway.' });
            await save({ request: 'grief', verses: [35] });
        },
    }, async (app) => {
        assert.equal(app.window.gospelGetterMargin.shown(), 'markers');
        const markers = [...app.verse(35).querySelectorAll('.margin-marker')];
        assert.deepEqual(markers.map((m) => m.classList.contains('finding-marker')), [false, true], 'the pencil, then the AI');
        const ai = markers[1];
        assert.equal(ai.textContent, 'AI');
        assert.equal(ai.getAttribute('aria-label'), 'AI finding on John 11:35');
        const inline = app.document.getElementById(ai.getAttribute('aria-controls'));
        assert.ok(inline.hidden);
        app.click(ai);
        await frame(app);
        assert.equal(ai.getAttribute('aria-expanded'), 'true');
        assert.ok(!inline.hidden);
        assert.deepEqual(texts(inline), [QUOTE('grief')]);
        assert.deepEqual(app.selectedVerseNumbers(), [], 'the click selects nothing');
    }));

test('Compare’s My notes column shows the findings the same way', () =>
    withFindings({
        readerMode: false,
        compare: true,
        storage: { 'gospel-getter-compare-notes': 'on' },
        seed: () => save({ request: 'grief', verses: [35] }),
    }, async (app) => {
        assert.equal(app.window.gospelGetterMargin.shown(), 'column');
        const found = $(app, '.notes-column .margin-finding');
        assert.ok(found, 'in the column');
        assert.equal(found.querySelector('.margin-finding-head').textContent, 'AI 35');
        assert.equal($(app, '.notes-column .notes-column-empty'), null, 'not "no notes" when there are findings');
        app.click(found.querySelector('.margin-finding-head'));
        await frame(app);
        assert.deepEqual(texts(found), [QUOTE('grief')]);
    }));

test('Keep as mine opens the note editor with the finding’s words, all selected; saving makes it the reader’s note', () =>
    withFindings({
        seed: async () => {
            await backend('add_note', { bookId: JOHN, chapter: 11, verseStart: 35, verseEnd: 35, body: 'He wept anyway.' });
            await save({ request: 'grief', verses: [35] });
            await save({ request: 'tears', verses: [35] });
        },
    }, async (app) => {
        const before = await backend('list_all_notes');
        app.click(head(app, 35));
        await frame(app);
        const keep = entry(app, 35).querySelector('.finding-item .finding-keep');
        app.click(keep);
        await settle();
        const overlay = app.document.getElementById('note-overlay');
        const input = app.document.getElementById('note-input');
        assert.ok(!overlay.hidden, 'the same editor as Add note');
        assert.equal(app.document.getElementById('note-title').textContent, 'John 11:35');
        assert.equal(input.value, QUOTE('grief'));
        assert.equal(app.document.activeElement, input, 'focused');
        assert.deepEqual([input.selectionStart, input.selectionEnd], [0, input.value.length], 'all of it selected');
        assert.ok(app.document.getElementById('note-delete').hidden, 'nothing to delete');
        assert.match(overlay.textContent, /He wept anyway\./, 'the note already there is listed');

        // Cancel: nothing changes.
        app.click(app.document.getElementById('note-cancel'));
        await settle();
        assert.ok(overlay.hidden);
        assert.equal((await backend('list_findings', { bookId: JOHN, chapter: 11 })).length, 2);
        assert.deepEqual(await backend('list_all_notes'), before);

        // Emptied, it can't be saved.
        app.click(head(app, 35));
        await frame(app);
        app.click(entry(app, 35).querySelector('.finding-item .finding-keep'));
        await settle();
        app.type(input, '   ');
        await settle();
        assert.ok(app.document.getElementById('note-save').disabled);

        // Typed over and saved: the reader's own note, beside the one there
        // already (untouched), and that one finding gone.
        app.type(input, 'Grief, read for myself.');
        await settle();
        await clickAndWait(app, app.document.getElementById('note-save'), 'list_all_notes');
        await app.idle();
        await frame(app);
        assert.ok(overlay.hidden);
        const after = await backend('list_all_notes');
        assert.equal(after.length, 2);
        assert.deepEqual(after.find((n) => n.id === before[0].id), before[0], 'the existing note is untouched');
        const kept = after.find((n) => n.id !== before[0].id);
        assert.equal(kept.body, 'Grief, read for myself.');
        assert.equal(kept.reference, 'John 11:35');
        const left = await backend('list_findings', { bookId: JOHN, chapter: 11 });
        assert.deepEqual(left.map((f) => f.request), ['tears']);
        assert.equal(head(app, 35).querySelector('.ai-badge').textContent, 'AI', 'the badge counts one now');
        assert.equal($$(app, '.margin-notes .margin-note').length, 2);
    }));

test('Keep as mine saved unchanged keeps who found it, as the reader’s note', () =>
    withFindings({ seed: () => save({ request: 'grief', verses: [33] }) }, async (app) => {
        app.click(head(app, 33));
        await frame(app);
        app.click(entry(app, 33).querySelector('.finding-keep'));
        await settle();
        await clickAndWait(app, app.document.getElementById('note-save'), 'list_all_notes');
        const notes = await backend('list_all_notes');
        assert.deepEqual(notes.map((n) => n.body), [QUOTE('grief')]);
        assert.deepEqual(await backend('list_findings', { bookId: JOHN, chapter: 11 }), []);
    }));

test('the Library lists the findings by request, apart from My notes, and Remove all removes a request’s with Undo', () =>
    withFindings({
        readerMode: false,
        seed: async () => {
            await backend('add_note', { bookId: JOHN, chapter: 3, verseStart: 16, verseEnd: 16, body: 'Loved.' });
            await save({ request: 'grief', verses: [33, 35] });
            await save({ request: 'love', chapter: 3, verses: [16] });
        },
    }, async (app) => {
        app.openLibrary('notes');
        await app.idle();
        const groups = () => $$(app, '#finding-group-list .finding-group').map((g) => ({
            name: g.querySelector('.finding-group-name').textContent,
            meta: g.querySelector('.finding-group-meta').textContent,
            verses: [...g.querySelectorAll('.finding-open')].map((b) => b.textContent),
        }));
        const found = groups();
        assert.deepEqual(found.map((g) => [g.name, g.verses]).sort(), [
            ['AI “grief”', ['John 11:33', 'John 11:35']],
            ['AI “love”', ['John 3:16']],
        ]);
        const grief = found.find((g) => g.name.includes('grief'));
        assert.match(grief.meta, /^Found by Claude · 2 verses · \w{3} \d{1,2}, \d{4}$/);
        assert.equal($(app, '#finding-groups-title').textContent, 'Found by AI agents');
        assert.equal($(app, '.my-notes-title').hidden, false, 'My notes, apart');
        assert.deepEqual($$(app, '#note-list .note-item').map((li) => li.dataset.noteId).length, 1, 'only the reader’s notes are notes');

        // Remove all: that request's, with Undo.
        const removeAll = $(app, '.finding-group[data-request="grief"] .finding-remove-all');
        assert.equal(removeAll.getAttribute('aria-label'), 'Remove all 2 AI findings for “grief”');
        await clickAndWait(app, removeAll, 'list_finding_requests');
        assert.deepEqual(groups().map((g) => g.name), ['AI “love”']);
        assert.ok(app.toast().textContent.startsWith('Removed 2 AI findings for “grief”.'));
        await clickAndWait(app, app.toast().querySelector('button'), 'restore_findings');
        await app.idle();
        assert.equal(groups().length, 2);

        // A verse goes to its passage.
        await clickAndWait(app, [...$$(app, '.finding-open')].find((b) => b.textContent === 'John 11:35'), 'get_reading');
        await app.waitForHeading('John 11');
        assert.deepEqual(app.selectedVerseNumbers(), [35]);
    }));

test('what an agent saves while the app is open shows when the Library opens and on the next chapter shown', () =>
    withFindings({ readerMode: false, chapter: 3 }, async (app) => {
        app.openLibrary('notes');
        await app.idle();
        assert.equal($(app, '#finding-group-list'), null, 'none yet');
        app.open('menu');
        await save({ request: 'light', chapter: 3, verses: [19, 21] });
        app.openLibrary('notes');
        await app.idle();
        assert.equal($$(app, '#finding-group-list .finding-group').length, 1);
        assert.ok(!app.verse(19).classList.contains('has-finding'), 'the chapter on screen waits for its next showing');
        app.open('menu');
        await clickAndWait(app, $(app, '#chapter-next'), 'list_findings');
        await clickAndWait(app, $(app, '#chapter-prev'), 'list_findings');
        await app.waitForHeading('John 3');
        assert.ok(app.verse(19).classList.contains('has-finding'));
    }));

test('Aa’s switch, "Let AI agents save findings", is off until the reader turns it on', async () => {
    await withFindings({ readerMode: false }, async (app) => {
        app.open('aa');
        await app.idle();
        const toggle = app.document.getElementById('ai-findings-toggle');
        assert.equal(toggle.type, 'checkbox');
        assert.equal(toggle.getAttribute('role'), 'switch');
        assert.equal(toggle.checked, false, 'off by default (C6)');
        assert.equal(toggle.closest('label').textContent.trim(), 'Let AI agents save findings');
        const note = app.document.getElementById(toggle.getAttribute('aria-describedby'));
        assert.match(note.textContent, /can’t change your notes, bookmarks or highlights/);
        assert.equal(await backend('get_ai_findings_enabled'), false);
        const before = app.calls.length;
        toggle.click();
        await app.idle(before, 'set_ai_findings_enabled');
        assert.equal(await backend('get_ai_findings_enabled'), true);
    });
    // Kept: the next session shows it on.
    const app = await openApp({ book: 'John', chapter: 11, keepBookmarks: true, keepPosition: true });
    try {
        app.open('aa');
        await app.idle();
        assert.equal(app.document.getElementById('ai-findings-toggle').checked, true);
    } finally {
        app.close();
    }
});

test('Export notes leaves the findings out', () =>
    withFindings({
        readerMode: false,
        seed: async () => {
            await backend('add_note', { bookId: JOHN, chapter: 11, verseStart: 35, verseEnd: 35, body: 'Mine.' });
            await save({ request: 'grief', verses: [33, 35] });
        },
    }, async (app) => {
        const path = join(dir, 'notes.md');
        await app.pick(path);
        app.openLibrary('notes');
        await clickAndWait(app, app.document.getElementById('export-notes-btn'), 'count_notes_export');
        await clickAndWait(app, app.document.getElementById('export-notes-confirm'), 'export_notes');
        const file = readFileSync(path, 'utf8');
        assert.ok(file.includes('Mine.'), file);
        assert.ok(!file.includes('Found by') && !file.includes('grief') && !file.includes('11:33'), file);
    }));
