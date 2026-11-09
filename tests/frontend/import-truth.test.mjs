// What an import says it will do, and does, with AI findings and "Let AI
// agents save findings" (format 4 files): the preview names the findings
// in the file and the ones replacing would remove, a file of findings alone
// can replace, and the file's switch is set only when the reader ticks the
// option to. Against the real backend and real files, as
// reader-data.test.mjs; the reader's findings are saved as an agent would
// (the bridge's `__save_findings`).

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { backend, bookId, closeBridge, openApp } from './harness.mjs';

const dir = mkdtempSync(join(tmpdir(), 'gospel-getter-import-truth-'));
after(async () => {
    await closeBridge();
    rmSync(dir, { recursive: true, force: true });
});

const JOHN = bookId('John');
let fileCount = 0;

async function withApp(options, body) {
    const app = await openApp({ book: 'John', chapter: 3, translation: 'kjv', ...options });
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const el = (app, id) => app.document.getElementById(id);
const overlayOpen = (app) => !el(app, 'import-overlay').hidden;
const summary = (app) => [...el(app, 'import-summary').children].map((li) => li.textContent);
const permissionShown = (app) => !el(app, 'import-permission-option').hidden;
const permission = (app) => el(app, 'import-permission');

/** Saves findings in John `chapter` as an agent would, whatever the switch says. */
function saveFindings(request, verses, chapter = 3) {
    return backend('__save_findings', {
        request,
        agent: 'Claude',
        references: verses.map((verse) => ({ bookId: JOHN, chapter, verse })),
    });
}

const findingsHere = async (chapter = 3) =>
    (await backend('list_findings', { bookId: JOHN, chapter })).map((f) => `${f.request} ${f.verseStart}`).sort();
const savingOn = () => backend('get_ai_findings_enabled');
const bookmarksHere = async () =>
    (await backend('list_bookmarks', { translationCode: 'kjv' })).map((b) => b.reference).sort();

const finding = (verse, request = 'love') => ({
    book: 43, book_name: 'John', chapter: 3, verse_start: verse, verse_end: verse,
    agent: 'Claude', request, created_at: '2026-11-07T00:00:00.000Z',
});

/** A format 4 file, as 2.28 exports one: two bookmarks, findings on John 3:16 and 17. */
const v4 = (overrides = {}) => ({
    format: 'gospel-getter-reader-data',
    format_version: 4,
    exported_at: '2026-11-08T12:00:00.000Z',
    app_version: '2.28.0',
    reading_position: null,
    bookmarks: [
        { book: 43, book_name: 'John', chapter: 3, verse_start: 16, verse_end: 16, created_at: '2020-01-01T00:00:00.000Z' },
        { book: 19, book_name: 'Psalms', chapter: 23, verse_start: 1, verse_end: 4, created_at: '2021-02-03T04:05:06.000Z' },
    ],
    findings: [finding(16), finding(17)],
    preferences: {},
    ...overrides,
});

function writeFile(data) {
    const path = join(dir, `import-${++fileCount}.gospel-getter.json`);
    writeFileSync(path, JSON.stringify(data, null, 2));
    return path;
}

/** ☰ → Import data…, the open dialog answering with a file of `data`: the preview. */
async function preview(app, data) {
    await app.pick(writeFile(data));
    app.open('menu');
    const before = app.calls.length;
    app.click(el(app, 'import-data-btn'));
    await app.idle(before);
    assert.ok(overlayOpen(app), 'the preview is open');
}

function choose(app, mode) {
    const radio = app.document.querySelector(`input[name="import-mode"][value="${mode}"]`);
    radio.checked = true;
    radio.dispatchEvent(new app.window.Event('change', { bubbles: true }));
}

/** Import, and wait for it and what it reloads. Returns apply_import's arguments. */
async function confirm(app) {
    const before = app.calls.length;
    app.click(el(app, 'import-confirm'));
    await app.idle(before, 'apply_import');
    assert.ok(!overlayOpen(app), 'the preview closed');
    const call = app.calls.slice(before).find((c) => c.cmd === 'apply_import');
    return JSON.parse(JSON.stringify(call.args));
}

test('the preview names the AI findings in the file, merging adds the new ones, and the chapter and the Library show them', () =>
    withApp({}, async (app) => {
        await saveFindings('love', [16]);
        await saveFindings('light', [19]);
        await preview(app, v4());
        assert.deepEqual(summary(app), [
            '2 bookmarks: 2 new, 0 you already have.',
            'Including John 3:16, Psalms 23:1–4',
            'No highlights.',
            'No notes.',
            '2 AI findings: 1 new, 1 you already have.',
            'No reading position.',
            'No display settings.',
        ]);
        assert.equal(el(app, 'import-choice-legend').textContent, 'Bookmarks, highlights, notes and AI findings');
        assert.equal(
            el(app, 'import-merge-label').textContent,
            'Add them to mine (2 new bookmarks, 0 new highlighted verses, 0 new notes, 1 new AI finding; anything I already have stays as it is)',
        );
        assert.ok(!permissionShown(app), 'the file has no switch: nothing to ask');

        const args = await confirm(app);
        assert.equal(args.restoreFindingsPermission, false);
        assert.equal(app.notice(), 'Import complete. Added 2 bookmarks. Added 1 AI finding (1 you already had).');
        assert.deepEqual(await findingsHere(), ['light 19', 'love 16', 'love 17']);
        // The chapter on screen shows the new one without being opened again.
        assert.ok(app.verse(17).classList.contains('has-finding'));
        app.openLibrary('notes');
        await app.idle();
        const love = app.document.querySelector('.finding-group[data-request="love"]');
        assert.deepEqual([...love.querySelectorAll('.finding-open')].map((b) => b.textContent), ['John 3:16', 'John 3:17']);
    }));

test('Replace names the AI findings it removes, even for a file with none, and says it removed them', () =>
    withApp({}, async (app) => {
        await saveFindings('light', [19, 21]);
        await preview(app, v4({ findings: [] }));
        assert.ok(summary(app).includes('No AI findings.'));
        assert.equal(
            el(app, 'import-replace-label').textContent,
            'Replace all my bookmarks, highlights, notes and AI findings with the file’s (removes my 0 current bookmarks, 0 highlighted verses, 0 notes and 2 AI findings)',
        );
        choose(app, 'replace');
        assert.equal(
            el(app, 'import-replace-warning').textContent,
            'This deletes your 0 current bookmarks, 0 highlighted verses, 0 notes and 2 AI findings and keeps only the file’s. If you might want yours back, cancel and export them first.',
        );
        assert.equal(el(app, 'import-confirm').textContent, 'Replace bookmarks, highlights, notes and AI findings and import');

        const args = await confirm(app);
        assert.equal(args.replace, true);
        assert.equal(
            app.notice(),
            'Import complete. Replaced 0 bookmarks with the file’s 2 bookmarks. Replaced 2 AI findings with the file’s 0 AI findings.',
        );
        assert.deepEqual(await findingsHere(), []);
        assert.ok(!app.verse(19).classList.contains('has-finding'), 'the chapter on screen drops them');
    }));

test('Replace leaves AI findings unnamed when there are none to remove', () =>
    withApp({}, async (app) => {
        await preview(app, v4());
        choose(app, 'replace');
        assert.equal(
            el(app, 'import-replace-warning').textContent,
            'This deletes your 0 current bookmarks, 0 highlighted verses and 0 notes and keeps only the file’s. If you might want yours back, cancel and export them first.',
        );
        assert.equal(el(app, 'import-confirm').textContent, 'Replace bookmarks, highlights and notes and import');
        await confirm(app);
        assert.match(app.notice(), /Replaced 0 AI findings with the file’s 2 AI findings\.$/);
        assert.deepEqual(await findingsHere(), ['love 16', 'love 17']);
    }));

test('a file with only AI findings offers Replace, and says what replacing removed', () =>
    withApp({}, async (app) => {
        await backend('add_bookmark', { bookId: JOHN, chapter: 3, verseStart: 5, verseEnd: 5 });
        await saveFindings('light', [19]);
        await preview(app, v4({ bookmarks: [], findings: [finding(16)] }));
        assert.ok(!el(app, 'import-bookmark-choice').hidden, 'the choice is there');
        assert.equal(
            el(app, 'import-replace-label').textContent,
            'Replace all my bookmarks, highlights, notes and AI findings with the file’s (removes my 1 current bookmark, 0 highlighted verses, 0 notes and 1 AI finding)',
        );
        choose(app, 'replace');
        const args = await confirm(app);
        assert.equal(args.replace, true);
        assert.equal(
            app.notice(),
            'Import complete. Replaced 1 bookmark with the file’s 0 bookmarks. Replaced 1 AI finding with the file’s 1 AI finding.',
        );
        assert.deepEqual(await findingsHere(), ['love 16']);
        assert.deepEqual(await bookmarksHere(), []);
    }));

test('a file that would turn AI saving on asks first, unticked, and only turns it on when ticked', () =>
    withApp({}, async (app) => {
        assert.equal(await savingOn(), false);
        const file = v4({ ai_findings_enabled: true });
        for (const mode of ['merge', 'replace']) {
            await preview(app, file);
            assert.ok(permissionShown(app));
            assert.equal(el(app, 'import-permission-label').textContent, 'Also turn on “Let AI agents save findings”, as in the file');
            assert.equal(permission(app).checked, false, 'unticked: turning it on is the reader’s choice');
            choose(app, mode);
            const args = await confirm(app);
            assert.equal(args.restoreFindingsPermission, false);
            assert.equal(await savingOn(), false, `${mode} leaves it off`);
            assert.doesNotMatch(app.notice(), /Let AI agents/);
        }

        await preview(app, file);
        permission(app).checked = true;
        const args = await confirm(app);
        assert.equal(args.restoreFindingsPermission, true);
        assert.equal(await savingOn(), true);
        assert.match(app.notice(), / Turned on “Let AI agents save findings”\.$/);
        assert.equal(el(app, 'ai-findings-toggle').checked, true, 'Aa’s switch shows it');
        app.open('aa');
        await app.idle();
        assert.equal(el(app, 'ai-findings-toggle').checked, true);
    }));

test('a file that would turn AI saving off offers it ticked, and unticked leaves it on', () =>
    withApp({}, async (app) => {
        await backend('set_ai_findings_enabled', { enabled: true });
        app.open('aa');
        await app.idle();
        assert.equal(el(app, 'ai-findings-toggle').checked, true);
        const file = v4({ ai_findings_enabled: false });

        await preview(app, file);
        assert.ok(permissionShown(app));
        assert.equal(el(app, 'import-permission-label').textContent, 'Also turn off “Let AI agents save findings”, as in the file');
        assert.equal(permission(app).checked, true, 'ticked: turning it off is the safe way');
        permission(app).checked = false;
        assert.equal((await confirm(app)).restoreFindingsPermission, false);
        assert.equal(await savingOn(), true, 'unticked: left on');

        await preview(app, file);
        choose(app, 'replace');
        assert.equal((await confirm(app)).restoreFindingsPermission, true);
        assert.equal(await savingOn(), false);
        assert.match(app.notice(), / Turned off “Let AI agents save findings”\.$/);
        app.open('aa');
        await app.idle();
        assert.equal(el(app, 'ai-findings-toggle').checked, false, 'Aa’s switch shows it');
    }));

test('nothing is asked about AI saving when the file agrees with the install or doesn’t say', () =>
    withApp({}, async (app) => {
        const v3 = { ...v4(), format_version: 3 };
        delete v3.findings;
        for (const [here, file] of [[false, v4({ ai_findings_enabled: false })], [true, v4({ ai_findings_enabled: true })], [true, v4()], [false, v4()], [true, v3]]) {
            await backend('set_ai_findings_enabled', { enabled: here });
            await preview(app, file);
            assert.ok(!permissionShown(app), `here ${here}, file ${file.ai_findings_enabled}`);
            choose(app, 'replace');
            assert.equal((await confirm(app)).restoreFindingsPermission, false);
            assert.equal(await savingOn(), here, 'left as it was');
            assert.doesNotMatch(app.notice(), /Let AI agents/);
        }
    }));
