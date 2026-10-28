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
import { backend, bookId, closeBridge, openApp, settle } from './harness.mjs';

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

/** Opens Export notes and waits for its count. */
async function openExport(app) {
    app.openLibrary('notes');
    await clickAndWait(app, el(app, 'export-notes-btn'), 'count_notes_export');
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
