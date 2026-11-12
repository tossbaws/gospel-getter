// Notes as Markdown (R17): a safe subset rendered wherever a note shows
// (the card here, the Library's Notes list; the margin in
// margin-notes.test.mjs), HTML always shown as typed; references typed in
// a note link to their passages, a step in the history as Go to is; web
// links open through the backend, which takes only http and https; Copy
// as Markdown; and Export notes' "One file per book". Against the real
// backend and database. The stored text and the backups are unchanged.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { backend, bookId, closeBridge, openApp, settle, storedVerse } from './harness.mjs';

const dir = mkdtempSync(join(tmpdir(), 'gospel-getter-markdown-'));
after(async () => {
    await closeBridge();
    rmSync(dir, { recursive: true, force: true });
});

const JOHN = bookId('John');

/** Adds notes (each [book, chapter, start, end, body]) before the app opens. */
async function seed(notes, translationCode = 'kjv') {
    await backend('__reset', { bookId: JOHN, chapter: 3, translationCode });
    const added = [];
    for (const [book, chapter, verseStart, verseEnd, body] of notes) {
        added.push(await backend('add_note', { bookId: book, chapter, verseStart, verseEnd, body }));
    }
    return added;
}

async function withApp(options, body) {
    const app = await openApp({ book: 'John', chapter: 3, keepBookmarks: true, ...options });
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const el = (app, id) => app.document.getElementById(id);
const cardBody = (app) => app.document.querySelector('.verse-notes .note-card-body');

async function select(app, start, end = start) {
    const before = app.calls.length;
    app.click(app.verse(start));
    if (end !== start) app.click(app.verse(end), { shiftKey: true });
    await app.idle(before, 'list_notes');
}

async function clickAndWait(app, target, cmd) {
    const before = app.calls.length;
    app.click(target);
    await app.idle(before, cmd);
}

const MARKDOWN = '**Loved**, *all* of it:\n- the world\n- `whosoever`\n\nSee https://example.com/a and [Gateway](https://www.biblegateway.com/).';

test('a note’s Markdown shows in the card and the Library; its stored text is unchanged', async () => {
    const [note] = await seed([[JOHN, 3, 16, 16, MARKDOWN]]);
    assert.equal(note.body, MARKDOWN, 'stored as typed');
    await withApp({}, async (app) => {
        await select(app, 16);
        const body = cardBody(app);
        assert.equal(body.tagName, 'DIV');
        assert.ok(body.classList.contains('note-text'));
        assert.deepEqual([...body.children].map((c) => c.tagName), ['P', 'UL', 'P']);
        assert.equal(body.querySelector('strong').textContent, 'Loved');
        assert.equal(body.querySelector('em').textContent, 'all');
        assert.deepEqual([...body.querySelectorAll('li')].map((li) => li.textContent), ['the world', 'whosoever']);
        assert.equal(body.querySelector('li code').textContent, 'whosoever');
        assert.deepEqual(
            [...body.querySelectorAll('a.note-web-link')].map((a) => [a.textContent, a.getAttribute('href')]),
            [['https://example.com/a', 'https://example.com/a'], ['Gateway', 'https://www.biblegateway.com/']],
        );
        // The editor still edits the text as typed.
        app.click(app.document.querySelector('.verse-notes .note-edit'));
        await settle();
        assert.equal(el(app, 'note-input').value, MARKDOWN);
        app.click(el(app, 'note-cancel'));
        await settle();

        // The Library: the start of it, run onto one line, beside the row's link.
        app.openLibrary('notes');
        const row = app.document.querySelector('#note-list .note-item');
        const preview = row.querySelector('.note-preview');
        assert.equal(preview.querySelector('strong').textContent, 'Loved');
        assert.ok(preview.textContent.startsWith('Loved, all of it: • the world • whosoever See https://example.com/a'));
        assert.equal(preview.querySelectorAll('p, ul, li, br').length, 0, 'one line');
        const open = row.querySelector('.bookmark-open');
        assert.equal(open.textContent.trim(), 'John 3:16');
        assert.equal(open.getAttribute('aria-describedby'), preview.id);
        assert.equal(open.querySelector('a, button'), null, 'no control in the row’s link');
    });
});

test('HTML in a note is shown as typed, never run, and no link opens anything but the web', async () => {
    const typed = '<img src=x onerror="window.__pwned = true"><b>bold?</b>\n&amp; <script>window.__pwned = true</script>\n'
        + '[click](javascript:window.__pwned=true) [file](file:///etc/passwd) ![image](https://example.com/i.png)';
    await seed([[JOHN, 3, 16, 16, typed]]);
    await withApp({}, async (app) => {
        await select(app, 16);
        app.openLibrary('notes');
        await settle();
        for (const shown of [cardBody(app), app.document.querySelector('#note-list .note-preview')]) {
            assert.equal(shown.querySelectorAll('img, b, script').length, 0);
            assert.ok(shown.textContent.includes('<img src=x onerror="window.__pwned = true"><b>bold?</b>'));
            assert.ok(shown.textContent.includes('&amp; <script>'));
            assert.ok(shown.textContent.includes('[click](javascript:window.__pwned=true)'));
            for (const link of shown.querySelectorAll('a')) {
                assert.match(link.getAttribute('href'), /^https:\/\//);
            }
        }
        await settle();
        assert.equal(app.window.__pwned, undefined);
    });
});

test('a reference typed in a note goes to its passage, a step in the history, from the card or the Library', async () => {
    await seed([[JOHN, 3, 16, 16, 'See Rom 5:8, then Ps 23. Not I Am, Job well done, Acts of kindness or 5:30.']]);
    await withApp({}, async (app) => {
        await select(app, 16);
        const links = [...cardBody(app).querySelectorAll('a.note-ref-link')];
        assert.deepEqual(links.map((a) => a.textContent), ['Rom 5:8', 'Ps 23'], 'no false references');
        assert.deepEqual(links.map((a) => a.title), ['Go to Romans 5:8', 'Go to Psalms 23']);
        assert.equal(links[0].getAttribute('href'), '#');
        await clickAndWait(app, links[0], 'get_reading');
        await app.waitForHeading('Romans 5');
        assert.deepEqual(app.selectedVerseNumbers(), [8]);
        assert.equal(app.window.location.hash, '', 'the page itself followed nothing');
        // Back returns to John 3, as from a Go to.
        const before = app.calls.length;
        app.key('ArrowLeft', { altKey: true });
        await app.idle(before);
        await app.waitForHeading('John 3');

        // From the Library: it closes, and the chapter alone opens at its top.
        app.openLibrary('notes');
        const psalm = [...app.document.querySelectorAll('#note-list .note-preview a.note-ref-link')]
            .find((a) => a.textContent === 'Ps 23');
        await clickAndWait(app, psalm, 'get_reading');
        await app.waitForHeading('Psalms 23');
        assert.ok(app.surface('library').hidden);
        assert.deepEqual(app.selectedVerseNumbers(), []);
        assert.ok(app.document.getElementById('note-overlay').hidden, 'the row’s own click didn’t happen');
    });
});

test('a reference to a verse the translation doesn’t number opens its chapter, saying so', async () => {
    await seed([[JOHN, 3, 16, 16, 'Compare Matt 17:21.']], 'bsb');
    await withApp({ translation: 'bsb' }, async (app) => {
        await select(app, 16);
        const link = cardBody(app).querySelector('a.note-ref-link');
        assert.equal(link.textContent, 'Matt 17:21');
        await clickAndWait(app, link, 'get_reading');
        await app.waitForHeading('Matthew 17');
        assert.deepEqual(app.selectedVerseNumbers(), []);
        assert.match(app.notice(), /isn.t numbered in the/);
    });
});

test('a web link opens through the backend (http and https only), never in the page', async () => {
    await seed([[JOHN, 3, 16, 16, 'Read https://example.com/notes?a=1&b=2 now.']]);
    await withApp({}, async (app) => {
        await select(app, 16);
        const link = cardBody(app).querySelector('a.note-web-link');
        assert.equal(link.getAttribute('href'), 'https://example.com/notes?a=1&b=2');
        const before = app.calls.length;
        const down = app.click(link);
        await app.idle(before, 'open_web_link');
        const opened = app.calls.slice(before).filter((c) => c.cmd === 'open_web_link');
        assert.deepEqual(opened.map((c) => c.args.url), ['https://example.com/notes?a=1&b=2']);
        assert.ok(down, 'clicked');
        assert.equal(app.window.location.href.startsWith('https://example.com'), false);
        // The section is still there: the click went no further.
        assert.ok(cardBody(app));
        // A middle click does nothing.
        const middle = app.calls.length;
        link.dispatchEvent(new app.window.MouseEvent('auxclick', { bubbles: true, cancelable: true, button: 1 }));
        await settle();
        assert.equal(app.calls.slice(middle).filter((c) => c.cmd === 'open_web_link').length, 0);
    });
    // The backend refuses anything else, with a sentence.
    for (const url of ['javascript:alert(1)', 'file:///etc/passwd', 'data:text/html,x', 'mailto:a@b.c', 'https://user:pw@example.com/']) {
        await assert.rejects(backend('open_web_link', { url }), (message) => {
            assert.equal(message, 'Only web links (starting with http:// or https://) open from a note.');
            return true;
        }, url);
    }
});

test('Copy as Markdown copies the note’s reference, its verses in the translation read, then its text', async () => {
    const [note] = await seed([[JOHN, 3, 16, 17, 'See **Rom 5:8**.\n- one']]);
    await withApp({}, async (app) => {
        await select(app, 16);
        const copy = app.document.querySelector('.verse-notes .note-copy');
        assert.equal(copy.textContent, 'Copy as Markdown');
        assert.equal(copy.getAttribute('aria-label'), 'Copy the note on John 3:16–17 as Markdown');
        // Asked for when the card showed it, so the copy is within the click.
        assert.ok(app.calls.some((c) => c.cmd === 'get_note_markdown' && c.args.id === note.id && c.args.translationCode === 'kjv'));
        const expected = await backend('get_note_markdown', { id: note.id, translationCode: 'kjv' });
        assert.ok(expected.startsWith('## John 3:16–17\n\n> **16** '), expected);
        assert.ok(expected.includes('\n>\n> — John 3:16–17 (KJV)\n\nSee **Rom 5:8**.\n- one'), expected);
        await clickAndWait(app, copy, null);
        await app.until(() => app.clipboardWrites.length === 1, 'the copy');
        assert.equal(app.clipboardWrites[0], expected);
        await app.until(() => app.status().textContent === 'Copied the note on John 3:16–17 as Markdown.', 'the status line');

        // From the Library, in the WEB: the toast says so.
        app.openLibrary('notes');
        const row = app.document.querySelector('#note-list .note-item');
        row.dispatchEvent(new app.window.Event('pointerover', { bubbles: true }));
        await app.idle();
        app.click(row.querySelector('.note-list-copy'));
        await app.until(() => app.clipboardWrites.length === 2, 'the second copy');
        assert.equal(app.clipboardWrites[1], expected);
        await app.until(() => app.toast().textContent === 'Copied the note on John 3:16–17 as Markdown.', 'the toast');
    });
    await withApp({ translation: 'web' }, async (app) => {
        await select(app, 16);
        await app.idle();
        app.click(app.document.querySelector('.verse-notes .note-copy'));
        await app.until(() => app.clipboardWrites.length === 1, 'the copy');
        assert.ok(app.clipboardWrites[0].includes('> — John 3:16–17 (WEB)'));
    });
});

test('a copy that fails says so', async () => {
    await seed([[JOHN, 3, 16, 16, 'Loved.']]);
    await withApp({}, async (app) => {
        await select(app, 16);
        await app.idle();
        app.setClipboard('reject');
        app.click(app.document.querySelector('.verse-notes .note-copy'));
        await app.until(() => app.status().textContent.startsWith('Couldn’t copy the note: '), 'the failure');
        assert.ok(app.status().classList.contains('is-error'));
    });
});

/** Opens Export notes (☰ → Export data… → My notes) and waits for its count. */
async function openExport(app) {
    await app.openExport();
    await clickAndWait(app, el(app, 'export-choice-notes'), 'count_notes_export');
}

test('Export notes as one file per book: a folder of books in Bible order, each its own export', async () => {
    await seed([
        [JOHN, 3, 16, 16, 'Loved.'],
        [bookId('Psalms'), 23, 1, 1, 'Shepherd.'],
        [bookId('1 John'), 1, 9, 9, 'Confess.'],
    ]);
    const folder = join(dir, 'vault');
    mkdirSync(folder);
    await withApp({ book: 'Romans', chapter: 8 }, async (app) => {
        await backend('__set_clock', { now: '2026-10-12T18:00:00Z', offsetMinutes: -360 });
        await openExport(app);
        // One file is the default, and as before.
        assert.ok(el(app, 'export-notes-one-file').checked);
        assert.ok(!el(app, 'export-notes-by-book').checked);
        assert.equal(el(app, 'export-notes-by-book').closest('label').textContent.trim(), 'One file per book (a folder)');
        app.click(el(app, 'export-notes-by-book'));
        assert.ok(el(app, 'export-notes-by-book').checked);
        await app.pick(folder);
        await clickAndWait(app, el(app, 'export-notes-confirm'), 'export_notes_by_book');
        await app.until(() => el(app, 'export-notes-overlay').hidden, 'the dialog to close');
        assert.equal(app.toast().textContent, `Saved 3 files to ${folder}.`);
        assert.deepEqual(readdirSync(folder).sort(), ['19 Psalms.md', '43 John.md', '62 1 John.md']);
        const john = readFileSync(join(folder, '43 John.md'), 'utf8');
        assert.ok(john.startsWith('# Notes on John\n\nExported Oct 12, 2026 · Verse text: King James Version · 1 note\n\n## John 3:16\n'), john);
        // Each file is that book's single-file export.
        const single = join(dir, 'john.md');
        await app.pick(single);
        await openExport(app);
        app.click(el(app, 'export-notes-none'));
        await settle();
        const tick = app.calls.length;
        app.click(app.document.querySelector(`#export-notes-books input[value="${JOHN}"]`));
        await app.idle(tick, 'count_notes_export');
        assert.equal(el(app, 'export-notes-count').textContent, '1 note matches');
        assert.ok(el(app, 'export-notes-one-file').checked, 'it opens fresh');
        await clickAndWait(app, el(app, 'export-notes-confirm'), 'export_notes');
        assert.equal(readFileSync(single, 'utf8'), john);
    });
});

test('files already in the folder are replaced only when the reader says so', async () => {
    await seed([[JOHN, 3, 16, 16, 'Loved.'], [bookId('Psalms'), 23, 1, 1, 'Shepherd.']]);
    const folder = join(dir, 'existing');
    mkdirSync(folder);
    writeFileSync(join(folder, '43 John.md'), 'my own edits');
    await withApp({ book: 'Romans', chapter: 8 }, async (app) => {
        const ask = async () => {
            await openExport(app);
            app.click(el(app, 'export-notes-by-book'));
            await app.pick(folder);
            await clickAndWait(app, el(app, 'export-notes-confirm'), 'export_notes_by_book');
            await app.until(() => !el(app, 'export-notes-replace').hidden, 'the question');
        };
        await ask();
        assert.equal(el(app, 'export-notes-replace-question').textContent.trim(), `43 John.md is already in ${folder}. Replace it?`);
        assert.equal(el(app, 'export-notes-replace-question').getAttribute('role'), 'alert');
        assert.equal(app.document.activeElement, el(app, 'export-notes-replace-no'), 'focus on Cancel');
        assert.ok(el(app, 'export-notes-confirm').closest('[hidden]'), 'Export… out of the way');
        assert.deepEqual(readdirSync(folder), ['43 John.md'], 'nothing written yet');
        // Cancel: nothing written, said nothing.
        await clickAndWait(app, el(app, 'export-notes-replace-no'), 'confirm_notes_export_by_book');
        assert.ok(el(app, 'export-notes-overlay').hidden);
        assert.equal(readFileSync(join(folder, '43 John.md'), 'utf8'), 'my own edits');
        assert.deepEqual(readdirSync(folder), ['43 John.md']);
        // Escape is Cancel too.
        await ask();
        const before = app.calls.length;
        app.key('Escape', {}, el(app, 'export-notes-replace-no'));
        await app.idle(before, 'confirm_notes_export_by_book');
        assert.ok(el(app, 'export-notes-overlay').hidden);
        assert.equal(readFileSync(join(folder, '43 John.md'), 'utf8'), 'my own edits');
        // Replace: every file written.
        await ask();
        await clickAndWait(app, el(app, 'export-notes-replace-yes'), 'confirm_notes_export_by_book');
        assert.ok(el(app, 'export-notes-overlay').hidden);
        assert.equal(app.toast().textContent, `Saved 2 files to ${folder}.`);
        assert.ok(readFileSync(join(folder, '43 John.md'), 'utf8').startsWith('# Notes on John\n'));
        assert.deepEqual(readdirSync(folder).sort(), ['19 Psalms.md', '43 John.md']);
    });
});

// AUD-08: the notes' Markdown asked for ahead of Copy as Markdown is kept
// for the notes pointed at most recently, not for every note ever pointed
// at: one pointed at long ago is asked for again when it's pointed at
// again, and its Copy still copies within the click.
test('AUD-08: only the notes pointed at most recently keep their Markdown; one pointed at long ago asks again, and copies', async () => {
    const PSALMS = bookId('Psalms');
    const notes = await seed(Array.from({ length: 60 }, (_, i) => [PSALMS, i + 1, 1, 1, `Psalm ${i + 1}, noted.`]));
    await withApp({}, async (app) => {
        app.openLibrary('notes');
        const rows = () => [...app.document.querySelectorAll('#note-list .note-item')];
        await app.until(() => rows().length === notes.length, 'every note listed');
        const asked = (id) => app.calls.filter((c) => c.cmd === 'get_note_markdown' && c.args.id === id).length;
        const point = async (row) => {
            row.dispatchEvent(new app.window.Event('pointerover', { bubbles: true }));
            await app.idle();
        };
        const first = rows()[0];
        const firstId = Number(first.dataset.noteId);
        for (const row of rows()) await point(row);
        assert.equal(asked(firstId), 1);
        // Pointed at again just now: still kept.
        const last = rows().at(-1);
        await point(last);
        assert.equal(asked(Number(last.dataset.noteId)), 1, 'the most recent is kept');

        // Long ago: asked for again, before the click, which copies it.
        await point(first);
        assert.equal(asked(firstId), 2, 'asked again');
        app.click(first.querySelector('.note-list-copy'));
        await app.until(() => app.clipboardWrites.length === 1, 'the copy');
        assert.equal(app.clipboardWrites[0], await backend('get_note_markdown', { id: firstId, translationCode: 'kjv' }));
    });
});

// N-U-01: SQLite gives a deleted note's id to the next note added. The
// Markdown asked for ahead of Copy as Markdown is for the note as it was
// shown (its passage, and when it was written), so a new note under a
// deleted note's id (here the same words, on John 3:17) copies its own
// heading and verse, never the deleted one's: in the card, in the Library,
// and after an import replaces every note. In the WEB, whose Markdown
// quotes each verse exactly as stored. The clipboard takes writes only
// within the click, as WebKitGTK's does.
const AMEN = 'Amen.';
const VERSE_16 = storedVerse('web', 'John', 3, 16);
const VERSE_17 = storedVerse('web', 'John', 3, 17);

/** The note "Amen." on John 3:17 as Markdown, its verse exactly as stored. */
function assertTheNewPassage(copied) {
    assert.equal(copied, `## John 3:17\n\n> **17** ${VERSE_17}\n>\n> — John 3:17 (WEB)\n\n${AMEN}`);
    assert.ok(!copied.includes(VERSE_16), 'nothing of the deleted note’s verse');
}

const markdownAsked = (app, id) => app.calls.filter((c) => c.cmd === 'get_note_markdown' && c.args.id === id);

/** Points at the Library row of note `id` (its Markdown asked for), and waits. */
async function pointAtRow(app, id) {
    const row = app.document.querySelector(`#note-list .note-item[data-note-id="${id}"]`);
    assert.ok(row, `a row for note ${id}`);
    row.dispatchEvent(new app.window.Event('pointerover', { bubbles: true }));
    await app.idle();
    return row;
}

// What's said once a copy is done, or has failed.
const COPIED = 'Copied the note on John 3:17 as Markdown.';
const sayingDone = (line) => line.textContent === COPIED || line.textContent.startsWith('Couldn’t copy the note');

/** Copy as Markdown on the card's only note: what the clipboard got. */
async function copyFromCard(app) {
    const writes = app.clipboardWrites.length;
    const copy = app.document.querySelectorAll('.verse-notes .note-copy');
    assert.equal(copy.length, 1);
    app.click(copy[0]);
    await app.until(() => sayingDone(app.status()), 'the copy');
    assert.equal(app.status().textContent, COPIED);
    assert.equal(app.clipboardWrites.length, writes + 1);
    return app.clipboardWrites.at(-1);
}

/** Copy as Markdown on note `id`'s Library row: what the clipboard got. */
async function copyFromRow(app, id) {
    const writes = app.clipboardWrites.length;
    const row = await pointAtRow(app, id);
    assert.ok(!sayingDone(app.toast()), 'nothing said about a copy yet');
    app.click(row.querySelector('.note-list-copy'));
    await app.until(() => sayingDone(app.toast()), 'the copy');
    assert.equal(app.toast().textContent, COPIED);
    assert.equal(app.clipboardWrites.length, writes + 1);
    return app.clipboardWrites.at(-1);
}

test('N-U-01: deleted in the editor and written again on another verse under its id, a note copies its own passage (the card)', async () => {
    const [old] = await seed([[JOHN, 3, 16, 16, AMEN]], 'web');
    await withApp({ translation: 'web', clipboard: 'gesture' }, async (app) => {
        await select(app, 16);
        await app.idle();
        assert.equal(markdownAsked(app, old.id).length, 1, 'asked for as the card showed it');
        // Deleted in the editor.
        await clickAndWait(app, app.document.querySelector('.verse-notes .note-edit'), null);
        await clickAndWait(app, el(app, 'note-delete'), 'list_all_notes');
        assert.deepEqual(await backend('list_all_notes'), []);
        // The same words on John 3:17.
        await select(app, 17);
        app.click(app.actions().querySelector('.verse-note-btn'));
        await settle();
        app.type(el(app, 'note-input'), AMEN);
        await settle();
        await clickAndWait(app, el(app, 'note-save'), 'list_all_notes');
        const [added] = await backend('list_all_notes');
        assert.equal(added.id, old.id, 'fixture: SQLite gave it the deleted note’s id');
        assert.equal(added.verseStart, 17);
        assertTheNewPassage(await copyFromCard(app));
    });
});

test('N-U-01: a note under a deleted note’s id copies its own passage from the Library, whatever the page was told', async () => {
    const [old] = await seed([[JOHN, 3, 16, 16, AMEN]], 'web');
    await withApp({ translation: 'web', clipboard: 'gesture' }, async (app) => {
        app.openLibrary('notes');
        await app.idle();
        await pointAtRow(app, old.id);
        assert.equal(markdownAsked(app, old.id).length, 1);
        // Deleted and written again on John 3:17 behind the page's back,
        // so only the note itself can tell the two apart.
        await backend('delete_note', { id: old.id });
        const added = await backend('add_note', { bookId: JOHN, chapter: 3, verseStart: 17, verseEnd: 17, body: AMEN });
        assert.equal(added.id, old.id, 'fixture: SQLite gave it the deleted note’s id');
        // A note added in the card reloads the Library's list.
        app.key('Escape', {}, app.document.activeElement);
        await select(app, 18);
        app.click(app.actions().querySelector('.verse-note-btn'));
        await settle();
        app.type(el(app, 'note-input'), 'Another.');
        await settle();
        await clickAndWait(app, el(app, 'note-save'), 'list_all_notes');
        app.openLibrary('notes');
        await app.idle();
        assertTheNewPassage(await copyFromRow(app, added.id));
    });
});

test('N-U-01: after an import replaces every note, one under an old note’s id copies its own passage (the card and the Library)', async () => {
    const [old] = await seed([[JOHN, 3, 16, 16, AMEN]], 'web');
    await withApp({ translation: 'web', clipboard: 'gesture' }, async (app) => {
        await select(app, 16);
        app.openLibrary('notes');
        await app.idle();
        await pointAtRow(app, old.id);
        assert.equal(markdownAsked(app, old.id).length, 1, 'asked for once, by the card and the row');
        app.key('Escape', {}, app.document.activeElement);

        const path = join(dir, 'replace.gospel-getter.json');
        writeFileSync(path, JSON.stringify({
            format: 'gospel-getter-reader-data',
            format_version: 3,
            exported_at: '2026-10-10T00:00:00.000Z',
            app_version: '2.8.0',
            reading_position: null,
            bookmarks: [],
            notes: [{ book: JOHN, book_name: 'John', chapter: 3, verse_start: 17, verse_end: 17, body: AMEN }],
        }));
        await app.pick(path);
        app.open('menu');
        let before = app.calls.length;
        app.click(el(app, 'import-data-btn'));
        await app.idle(before);
        const replace = app.document.querySelector('input[name="import-mode"][value="replace"]');
        replace.checked = true;
        replace.dispatchEvent(new app.window.Event('change', { bubbles: true }));
        before = app.calls.length;
        app.click(el(app, 'import-confirm'));
        await app.idle(before, 'apply_import');
        const [added] = await backend('list_all_notes');
        assert.equal(added.id, old.id, 'fixture: the imported note has the replaced note’s id');
        assert.equal(added.verseStart, 17);

        await select(app, 17);
        await app.idle();
        assertTheNewPassage(await copyFromCard(app));
        app.openLibrary('notes');
        await app.idle();
        assertTheNewPassage(await copyFromRow(app, added.id));
    });
});

// N-U-03: the card shows the first notes on a passage and has their
// Markdown ready for Copy as Markdown, however many other notes' Markdown
// is asked for (more notes on the passage than are kept, the Library's
// rows pointed at); what's kept besides is still bounded.
test('N-U-03: with more notes on a passage than are kept, and the Library pointed at, the card’s first Copy as Markdown copies within its click', async () => {
    const PSALMS = bookId('Psalms');
    await seed([
        ...Array.from({ length: 35 }, (_, i) => [JOHN, 3, 16, 16, `Note ${i + 1} on 3:16.`]),
        ...Array.from({ length: 40 }, (_, i) => [PSALMS, i + 1, 1, 1, `Psalm ${i + 1}, noted.`]),
    ], 'web');
    await withApp({ translation: 'web', clipboard: 'gesture' }, async (app) => {
        await select(app, 16);
        await app.idle();
        const firstCopy = () => app.document.querySelector('.verse-notes .note-copy');
        const firstId = Number(firstCopy().dataset.noteId);
        app.openLibrary('notes');
        const rows = () => [...app.document.querySelectorAll('#note-list .note-item')];
        await app.until(() => rows().length === 75, 'every note listed');
        const psalmRows = rows().filter((row) => row.textContent.includes('Psalm'));
        for (const row of psalmRows) await pointAtRow(app, Number(row.dataset.noteId));

        app.click(firstCopy());
        await app.until(() => /^(Copied|Couldn’t copy) the note/.test(app.status().textContent), 'the copy');
        assert.equal(app.status().textContent, 'Copied the note on John 3:16 as Markdown.');
        assert.deepEqual(app.clipboardWrites, [await backend('get_note_markdown', { id: firstId, translationCode: 'web' })]);
        assert.ok(app.clipboardWrites[0].includes(`> **16** ${VERSE_16}\n`), 'the verse exactly as stored');

        // Still bounded: the Psalm pointed at first is asked for again.
        const longAgo = Number(psalmRows[0].dataset.noteId);
        assert.equal(markdownAsked(app, longAgo).length, 1);
        await pointAtRow(app, longAgo);
        assert.equal(markdownAsked(app, longAgo).length, 2, 'asked again');
    });
});
