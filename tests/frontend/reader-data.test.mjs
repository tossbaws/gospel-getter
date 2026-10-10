// Behavior tests for exporting and importing the reader's data (bookmarks,
// reading position, display settings), against the real backend and real
// files. The native file dialogs are the one thing scripted: each test
// says what the next dialog answers (a path, or cancel) via the bridge.
// Export and Import are in the ☰ menu; how they went is said in a toast,
// and a file that can't be imported is explained in the import dialog.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { backend, closeBridge, openApp, storedVerse } from './harness.mjs';

const dir = mkdtempSync(join(tmpdir(), 'gospel-getter-reader-data-'));
after(async () => {
    await closeBridge();
    rmSync(dir, { recursive: true, force: true });
});

let fileCount = 0;
const newPath = (label) => join(dir, `${label}-${++fileCount}.gospel-getter.json`);

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const el = (app, id) => app.document.getElementById(id);
const dataStatus = (app) => el(app, 'data-status').textContent;
const toastShown = (app) => !el(app, 'data-status').classList.contains('is-hidden');
const overlayOpen = (app) => !el(app, 'import-overlay').hidden;

async function bookmarkVerse(app, n) {
    app.click(app.verse(n));
    const before = app.calls.length;
    app.click(app.bookmarkButton());
    await app.idle(before, 'list_bookmarks');
    app.key('Escape');
}

async function setTheme(app, value) {
    const select = el(app, 'theme-select');
    select.value = value;
    select.dispatchEvent(new app.window.Event('change', { bubbles: true }));
}

async function clickData(app, id, path) {
    await app.pick(path);
    app.open('menu');
    const before = app.calls.length;
    app.click(el(app, id));
    await app.idle(before);
}

async function bookmarksIn(code = 'kjv') {
    return (await backend('list_bookmarks', { translationCode: code })).map((b) => b.reference).sort();
}

function writeFile(contents) {
    const path = newPath('import');
    writeFileSync(path, typeof contents === 'string' ? contents : JSON.stringify(contents, null, 2));
    return path;
}

const fileData = (overrides = {}) => ({
    format: 'gospel-getter-reader-data',
    format_version: 1,
    exported_at: '2026-10-04T12:00:00.000Z',
    app_version: '2.2.0',
    reading_position: { translation: 'web', book: 45, book_name: 'Romans', chapter: 8 },
    bookmarks: [
        { book: 43, book_name: 'John', chapter: 3, verse_start: 16, verse_end: 16, created_at: '2020-01-01T00:00:00.000Z' },
        { book: 19, book_name: 'Psalms', chapter: 23, verse_start: 1, verse_end: 4, created_at: '2021-02-03T04:05:06.000Z' },
    ],
    preferences: { theme: 'matrix', text_size: 'large', line_spacing: 'relaxed', reading_mode: false, compare: true },
    ...overrides,
});

test('Export saves bookmarks, reading position and display settings to the chosen file', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await bookmarkVerse(app, 16);
        await setTheme(app, 'matrix');
        const path = newPath('export');
        await clickData(app, 'export-data-btn', path);

        assert.equal(dataStatus(app), `Saved 1 bookmark, your reading position, your display settings to ${path}.`);
        assert.ok(toastShown(app));
        assert.ok(!el(app, 'data-status').classList.contains('is-error'));
        assert.ok(app.surface('menu').hidden, 'choosing Export closed the menu');
        assert.equal(app.document.activeElement, el(app, 'menu-toggle'), 'focus is back on ☰');
        const text = readFileSync(path, 'utf8');
        const json = JSON.parse(text);
        assert.equal(json.format, 'gospel-getter-reader-data');
        assert.equal(json.format_version, 2);
        assert.deepEqual(
            { ...json.bookmarks[0], created_at: typeof json.bookmarks[0].created_at },
            { book: 43, book_name: 'John', chapter: 3, verse_start: 16, verse_end: 16, created_at: 'string' },
        );
        assert.deepEqual(json.reading_position, { translation: 'kjv', book: 43, book_name: 'John', chapter: 3 });
        assert.deepEqual(json.preferences, {
            theme: 'matrix', text_size: 'medium', line_spacing: 'normal', reading_mode: false, compare: false,
        });
        assert.ok(!text.includes(storedVerse('kjv', 'John', 3, 16)), 'no Bible text');
    }));

test('the ☰ menu items read exactly "Export data…" and "Import data…"', () =>
    withApp({}, async (app) => {
        const exportBtn = el(app, 'export-data-btn');
        const importBtn = el(app, 'import-data-btn');
        assert.equal(exportBtn.textContent, 'Export data…');
        assert.equal(importBtn.textContent, 'Import data…');
        for (const btn of [exportBtn, importBtn]) {
            assert.equal(btn.getAttribute('type'), 'button');
            assert.equal(btn.getAttribute('role'), 'menuitem');
            assert.ok(btn.closest('#app-menu'), 'in the ☰ menu');
        }
    }));

test('cancelling the save dialog writes nothing', () =>
    withApp({}, async (app) => {
        const before = readdirSync(dir).length;
        await clickData(app, 'export-data-btn', null);
        assert.equal(dataStatus(app), 'Export cancelled. No file was saved.');
        assert.equal(readdirSync(dir).length, before);
        assert.equal(el(app, 'export-data-btn').disabled, false, 'the buttons are usable again');
    }));

test('cancelling the open dialog changes nothing', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await bookmarkVerse(app, 16);
        await clickData(app, 'import-data-btn', null);
        assert.equal(dataStatus(app), 'Import cancelled. Nothing was changed.');
        assert.ok(!overlayOpen(app));
        assert.deepEqual(await bookmarksIn(), ['John 3:16']);
    }));

test('an invalid file is explained and changes nothing', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await bookmarkVerse(app, 16);
        const path = writeFile(fileData({
            bookmarks: [
                { book: 43, chapter: 3, verse_start: 1, verse_end: 1 },
                { book: 99, chapter: 1, verse_start: 1, verse_end: 1 },
            ],
        }));
        await clickData(app, 'import-data-btn', path);
        // The import dialog explains it, with each problem; nothing to confirm.
        assert.ok(overlayOpen(app));
        assert.equal(el(app, 'import-title').textContent, 'This file can’t be imported');
        assert.equal(el(app, 'import-file').textContent, path.split('/').pop());
        assert.equal(el(app, 'import-invalid').textContent, "The file can't be imported: 1 problem was found. Nothing was changed.");
        assert.deepEqual([...el(app, 'data-problems').children].map((li) => li.textContent), ['Bookmark 2: there’s no book number 99.'.replace('’', "'")]);
        assert.ok(el(app, 'data-problems').closest('#import-panel'), 'the problems are in the dialog');
        assert.ok(el(app, 'import-confirm').hidden);
        assert.ok(el(app, 'import-bookmark-choice').hidden);
        assert.ok(el(app, 'import-summary').hidden);
        assert.equal(el(app, 'import-cancel').textContent, 'Close');
        assert.equal(app.document.activeElement, el(app, 'import-cancel'));
        assert.match(el(app, 'import-panel').getAttribute('aria-describedby'), /data-problems/);
        // None of it goes to the toast.
        assert.equal(dataStatus(app), '');
        assert.ok(!toastShown(app));

        const before = app.calls.length;
        app.click(el(app, 'import-cancel'));
        await app.idle(before);
        assert.ok(!overlayOpen(app));
        assert.ok(!app.calls.slice(before).some((c) => c.cmd === 'cancel_import'), 'nothing to discard');
        assert.equal(app.document.activeElement, el(app, 'menu-toggle'));
        assert.ok(!toastShown(app));
        assert.deepEqual(await bookmarksIn(), ['John 3:16']);

        // A good file afterwards gets the full preview back.
        await clickData(app, 'import-data-btn', writeFile(fileData()));
        assert.equal(el(app, 'import-title').textContent, 'Import bookmarks, highlights and settings');
        assert.ok(el(app, 'import-invalid').hidden);
        assert.deepEqual([...el(app, 'data-problems').children], []);
        assert.ok(!el(app, 'import-confirm').hidden);
        assert.equal(el(app, 'import-cancel').textContent, 'Cancel');
    }));

test('a file from a newer version is refused clearly', () =>
    withApp({}, async (app) => {
        const path = writeFile({ format: 'gospel-getter-reader-data', format_version: 3, exported_at: 'x', app_version: '9.0.0' });
        await clickData(app, 'import-data-btn', path);
        const invalid = () => el(app, 'import-invalid').textContent;
        assert.match(invalid(), /made by a newer version of Gospel Getter \(data format 3\)\. This version reads format 2; update Gospel Getter to import it\. Nothing was changed\.$/);
        app.key('Escape', {}, el(app, 'import-cancel'));
        assert.ok(!overlayOpen(app), 'Escape closes it');
        const garbage = writeFile('this is not json');
        await clickData(app, 'import-data-btn', garbage);
        assert.match(invalid(), /isn't valid JSON \(line 1, column 2\)/);
        assert.equal(dataStatus(app), '');
    }));

test('the preview shows what will happen before anything changes, and Cancel keeps it that way', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await bookmarkVerse(app, 16);
        const path = writeFile(fileData());
        await clickData(app, 'import-data-btn', path);

        assert.ok(overlayOpen(app));
        assert.equal(app.document.activeElement, el(app, 'import-confirm'));
        const panel = el(app, 'import-panel');
        assert.equal(panel.getAttribute('role'), 'dialog');
        assert.equal(panel.getAttribute('aria-modal'), 'true');
        assert.match(el(app, 'import-file').textContent, /exported 2026-10-04 by Gospel Getter 2\.2\.0$/);
        const summary = [...el(app, 'import-summary').children].map((li) => li.textContent);
        assert.deepEqual(summary, ['2 bookmarks: 1 new, 1 you already have.', 'Including John 3:16, Psalms 23:1–4']);
        assert.equal(el(app, 'import-merge-label').textContent, 'Add them to my bookmarks (1 new; any I already have stay as they are)');
        assert.ok(app.document.querySelector('input[name="import-mode"][value="merge"]').checked, 'merge is the default');
        assert.equal(el(app, 'import-position-label').textContent, 'Go to the file’s reading position: Romans 8 (WEB)');
        assert.equal(
            el(app, 'import-prefs-label').textContent,
            'Use the file’s display settings: Matrix theme, large text, relaxed line spacing, compare on, reading mode off',
        );
        assert.equal(el(app, 'import-confirm').textContent, 'Import');
        assert.ok(el(app, 'import-replace-warning').hidden);

        // Nothing has changed yet.
        assert.deepEqual(await bookmarksIn(), ['John 3:16']);
        assert.equal(app.document.documentElement.dataset.theme, undefined);
        assert.equal(app.heading(), 'John 3');

        const before = app.calls.length;
        app.click(el(app, 'import-cancel'));
        await app.idle(before, 'cancel_import');
        assert.ok(!overlayOpen(app));
        assert.equal(dataStatus(app), 'Import cancelled. Nothing was changed.');
        assert.ok(toastShown(app));
        assert.equal(app.document.activeElement, el(app, 'menu-toggle'), 'focus goes back to ☰');
        assert.deepEqual(await bookmarksIn(), ['John 3:16']);
        assert.equal(app.document.documentElement.dataset.theme, undefined);

        // Escape cancels too, before it does anything else.
        await clickData(app, 'import-data-btn', path);
        app.key('Escape', {}, el(app, 'import-confirm'));
        await app.idle(0);
        assert.ok(!overlayOpen(app));
        assert.equal(dataStatus(app), 'Import cancelled. Nothing was changed.');
        assert.deepEqual(await bookmarksIn(), ['John 3:16']);
    }));

test('confirming merges bookmarks, goes to the position and applies the settings; importing again changes nothing', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await bookmarkVerse(app, 16);
        const path = writeFile(fileData());
        await clickData(app, 'import-data-btn', path);
        let before = app.calls.length;
        app.click(el(app, 'import-confirm'));
        await app.idle(before, 'apply_import');

        assert.ok(!overlayOpen(app));
        const root = app.document.documentElement;
        assert.equal(root.dataset.theme, 'matrix');
        assert.equal(root.dataset.textSize, 'large');
        assert.equal(root.dataset.lineSpacing, 'relaxed');
        assert.equal(el(app, 'theme-select').value, 'matrix');
        assert.equal(el(app, 'compare-btn').getAttribute('aria-pressed'), 'true');
        assert.equal(app.window.localStorage.getItem('gospel-getter-theme'), 'matrix');
        assert.equal(el(app, 'translation-select').value, 'web');
        assert.equal(app.heading(), 'Romans 8');
        assert.ok(app.isComparing(), 'compare turned on by the imported settings');
        const expected = 'Import complete. Added 1 bookmark (1 you already had). Went to Romans 8 (WEB). Display settings applied.';
        assert.equal(app.notice(), expected);
        assert.equal(dataStatus(app), expected);
        assert.deepEqual(await bookmarksIn('web'), ['John 3:16', 'Psalms 23:1–4']);
        // Newest first, by each bookmark's own date: John 3:16 was made
        // just now (and merging kept it); Psalms 23 keeps its 2021 date.
        assert.deepEqual(app.bookmarkItems().map((b) => b.reference), ['John 3:16', 'Psalms 23:1–4']);

        // The same file again: nothing new.
        await clickData(app, 'import-data-btn', path);
        assert.deepEqual([...el(app, 'import-summary').children][0].textContent, '2 bookmarks: 0 new, 2 you already have.');
        before = app.calls.length;
        app.click(el(app, 'import-confirm'));
        await app.idle(before, 'apply_import');
        assert.match(app.notice(), /^Import complete\. Added 0 bookmarks \(2 you already had\)\./);
        assert.deepEqual(await bookmarksIn('web'), ['John 3:16', 'Psalms 23:1–4']);
    }));

test('replacing bookmarks is an explicit choice with a clear warning', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        for (const n of [1, 2, 3]) await bookmarkVerse(app, n);
        const path = writeFile(fileData({ reading_position: null, preferences: {} }));
        await clickData(app, 'import-data-btn', path);
        assert.ok(el(app, 'import-position-option').hidden, 'no position in the file, no option');
        assert.ok(el(app, 'import-prefs-option').hidden, 'no settings in the file, no option');
        assert.equal(el(app, 'import-replace-label').textContent, 'Replace all my bookmarks with the file’s (removes my 3 current bookmarks)');

        const replace = app.document.querySelector('input[name="import-mode"][value="replace"]');
        replace.checked = true;
        replace.dispatchEvent(new app.window.Event('change', { bubbles: true }));
        assert.equal(el(app, 'import-replace-warning').hidden, false);
        assert.match(el(app, 'import-replace-warning').textContent, /^This deletes your 3 current bookmarks and keeps only the file’s\. If you might want yours back, cancel and export them first\.$/);
        assert.equal(el(app, 'import-confirm').textContent, 'Replace bookmarks and import');

        const before = app.calls.length;
        app.click(el(app, 'import-confirm'));
        await app.idle(before, 'apply_import');
        assert.equal(app.notice(), 'Import complete. Replaced 3 bookmarks with the file’s 2 bookmarks.');
        assert.deepEqual(await bookmarksIn(), ['John 3:16', 'Psalms 23:1–4']);
        const call = app.calls.find((c) => c.cmd === 'apply_import');
        assert.deepEqual(JSON.parse(JSON.stringify(call.args)), { token: call.args.token, replace: true, restorePosition: false });
    }));

test('unticking the position and settings imports only bookmarks', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        const path = writeFile(fileData());
        await clickData(app, 'import-data-btn', path);
        el(app, 'import-position').checked = false;
        el(app, 'import-prefs').checked = false;
        const before = app.calls.length;
        app.click(el(app, 'import-confirm'));
        await app.idle(before, 'apply_import');
        assert.equal(app.notice(), 'Import complete. Added 2 bookmarks.');
        assert.equal(app.heading(), 'John 3');
        assert.equal(el(app, 'translation-select').value, 'kjv');
        assert.equal(app.document.documentElement.dataset.theme, undefined);
        assert.equal(app.window.localStorage.getItem('gospel-getter-theme'), null);
        assert.deepEqual(await bookmarksIn(), ['John 3:16', 'Psalms 23:1–4']);
    }));

test('a failed import keeps the preview open and changes nothing', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await bookmarkVerse(app, 16);
        const path = writeFile(fileData());
        await clickData(app, 'import-data-btn', path);
        app.failCommand('apply_import', 'The import failed, so nothing was changed.');
        let before = app.calls.length;
        app.click(el(app, 'import-confirm'));
        await app.idle(before, 'apply_import');
        assert.ok(overlayOpen(app));
        assert.equal(el(app, 'import-error').textContent, 'The import failed, so nothing was changed. You can try again, or cancel.');
        assert.equal(el(app, 'import-confirm').disabled, false);
        assert.deepEqual(await bookmarksIn(), ['John 3:16']);
        assert.equal(app.document.documentElement.dataset.theme, undefined, 'settings not applied either');

        app.failCommand('apply_import', null);
        before = app.calls.length;
        app.click(el(app, 'import-confirm'));
        await app.idle(before, 'apply_import');
        assert.ok(!overlayOpen(app));
        assert.deepEqual(await bookmarksIn(), ['John 3:16', 'Psalms 23:1–4']);
    }));

test('display settings that can’t be saved are applied and the problem reported', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        const path = writeFile(fileData({ reading_position: null, bookmarks: [] }));
        await clickData(app, 'import-data-btn', path);
        assert.ok(el(app, 'import-bookmark-choice').hidden, 'no bookmarks in the file, no bookmark choice');
        app.window.Storage.prototype.setItem = () => {
            throw new app.window.DOMException('quota', 'QuotaExceededError');
        };
        const before = app.calls.length;
        app.click(el(app, 'import-confirm'));
        await app.idle(before, 'apply_import');
        assert.equal(app.document.documentElement.dataset.theme, 'matrix');
        assert.equal(
            app.notice(),
            'Import complete. Display settings applied, but they couldn’t be saved here, so they’ll reset when the app restarts.',
        );
    }));

test('while the preview is open, Tab stays in it and shortcuts do nothing', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        const path = writeFile(fileData());
        await clickData(app, 'import-data-btn', path);
        const cancel = el(app, 'import-cancel');
        cancel.focus();
        const tab = app.key('Tab', {}, cancel);
        assert.ok(tab.defaultPrevented);
        assert.equal(app.document.activeElement, app.document.querySelector('input[name="import-mode"][value="merge"]'));
        app.key('ArrowRight');
        app.key('c');
        app.key('/');
        await app.idle(0);
        assert.equal(app.heading(), 'John 3');
        assert.ok(!app.isComparing());
        assert.ok(app.searchPanel().hidden);
        // Clicking the page behind does nothing either.
        app.click(app.document.querySelector('footer'));
        assert.ok(overlayOpen(app));
    }));

test('data moves to a fresh install through a real file', async () => {
    const path = newPath('move');
    await withApp({ book: 'Psalms', chapter: 23, translation: 'web' }, async (app) => {
        app.click(app.verse(1));
        app.click(app.verse(4), { shiftKey: true });
        let before = app.calls.length;
        app.click(app.bookmarkButton());
        await app.idle(before, 'list_bookmarks');
        app.key('Escape');
        await bookmarkVerse(app, 6);
        await setTheme(app, 'hot-pink');
        await clickData(app, 'export-data-btn', path);
        assert.match(dataStatus(app), /^Saved 2 bookmarks, your reading position, your display settings to /);
    });

    // A different install: new database, empty storage.
    await closeBridge();
    assert.ok(existsSync(path));
    await withApp({ book: 'Genesis', chapter: 1, translation: 'kjv' }, async (app) => {
        assert.deepEqual(await bookmarksIn(), []);
        await clickData(app, 'import-data-btn', path);
        const before = app.calls.length;
        app.click(el(app, 'import-confirm'));
        await app.idle(before, 'apply_import');
        assert.equal(app.notice(), 'Import complete. Added 2 bookmarks. Went to Psalms 23 (WEB). Display settings applied.');
        assert.equal(app.heading(), 'Psalms 23');
        assert.equal(app.document.documentElement.dataset.theme, 'hot-pink');
        assert.deepEqual(app.bookmarkedVerseNumbers(), [1, 2, 3, 4, 6]);
        assert.deepEqual(await bookmarksIn('web'), ['Psalms 23:1–4', 'Psalms 23:6']);
    });
});
