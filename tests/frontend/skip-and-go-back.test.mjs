// Behavior tests for Skip this version (the banner) and Go back to the
// previous version (About): the skipped version kept on this computer, quiet
// at launch and shown by Check now; Go back offered from this release's
// previous.json only when the app's own rules allow it (a lower final
// release, a build that installs itself, a release that can use this
// database), its confirm dialog, the backup saved beside the database
// before anything is installed, and a failed backup installing nothing.
// previous.json, the install and the backup's folder are scripted in the
// test bridge (`setUpdate`); the rules and the backup are the app's.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { backend, clickAndWaitForCalls as click, closeBridge, el as $, withApp } from './harness.mjs';

const kept = [];
after(async () => {
    for (const path of kept) rmSync(path, { force: true });
    await closeBridge();
});

// The version the app reports about itself, as tauri.conf.json declares it.
const VERSION = JSON.parse(
    readFileSync(new URL('../../src-tauri/tauri.conf.json', import.meta.url), 'utf8'),
).version;

const SKIPPED_KEY = 'gospel-getter-update-skipped';
const SKIPPED_NOTE = 'You skipped this version, so it isn’t offered when Gospel Getter starts.';
const BACKUP_FAILED = 'Your notes, bookmarks, highlights and settings couldn’t be backed up, so Gospel Getter didn’t go back. Nothing was changed.';
const NOT_VERIFIED = 'The update couldn’t be verified as a genuine Gospel Getter release, so it wasn’t installed.';

const UPDATE = { version: '99.1.0', notes: 'Newer.', date: '2026-12-01', canSelfUpdate: true };
// The schema this build makes: an earlier release built for it can use
// this database.
const PREVIOUS = { version: '1.0.0', schemaVersion: 2 };

const banner = (app) => $(app, 'update-banner');
const calls = (app, cmd) => app.calls.filter((c) => c.cmd === cmd);

async function checkNow(app) {
    if ($(app, 'about-overlay').hidden) app.open('about');
    await click(app, $(app, 'update-check-btn'), 'check_for_update');
}

test('Skip this version sits beside Later; it keeps that version, hides the banner, and the next launch stays quiet', async () => {
    let storage;
    await withApp({ update: { check: UPDATE } }, async (app) => {
        const skip = $(app, 'update-skip');
        assert.equal(skip.hidden, false);
        assert.equal(skip.textContent, 'Skip this version');
        assert.equal(skip.nextElementSibling, $(app, 'update-later'), 'beside Later');
        skip.focus();
        app.click(skip);
        assert.equal(banner(app).hidden, true);
        assert.equal(app.document.activeElement, $(app, 'menu-toggle'), 'focus isn’t lost with the banner');
        storage = app.storage();
        assert.equal(storage[SKIPPED_KEY], '99.1.0', 'that exact version');
        // This computer's: a backup carries only the display settings.
        assert.ok(!Object.keys(app.window.gospelGetterPreferences.current()).some((k) => k.includes('skip')));
    });
    await withApp({ update: { check: UPDATE }, storage }, async (app) => {
        assert.equal(calls(app, 'check_for_update').length, 1, 'the launch check still runs');
        assert.equal(banner(app).hidden, true, 'and stays quiet for the skipped version');
        assert.equal($(app, 'update-check-status').textContent, '');
    });
    // A newer version shows again.
    await withApp({ update: { check: { ...UPDATE, version: '99.2.0' } }, storage }, async (app) => {
        assert.equal(banner(app).hidden, false);
        assert.equal($(app, 'update-title').textContent, 'Gospel Getter 99.2.0 is available');
    });
});

test('Check now still shows a skipped version in About, noting it, with Update now', () =>
    withApp({ update: { check: UPDATE }, storage: { [SKIPPED_KEY]: '99.1.0' } }, async (app) => {
        assert.equal(banner(app).hidden, true);
        await checkNow(app);
        assert.equal($(app, 'about-update').hidden, false);
        assert.equal($(app, 'about-update-title').textContent, 'Gospel Getter 99.1.0 is available');
        assert.equal($(app, 'about-update-skipped').hidden, false);
        assert.equal($(app, 'about-update-skipped').textContent, SKIPPED_NOTE);
        assert.equal($(app, 'about-update-install').hidden, false, 'skipping is never a trap');
        assert.equal(banner(app).hidden, true);
    }));

test('an update that wasn’t skipped has no note in About', () =>
    withApp({ update: { check: UPDATE }, storage: { [SKIPPED_KEY]: '99.0.0' } }, async (app) => {
        await checkNow(app);
        assert.equal($(app, 'about-update').hidden, false);
        assert.equal($(app, 'about-update-skipped').hidden, true);
    }));

test('Go back shows only for a lower final release, on a build that installs itself', async () => {
    for (const [what, update] of [
        ['no previous.json', {}],
        ['a newer version', { previous: { version: '99.0.0', schemaVersion: 2 } }],
        ['this version', { previous: { version: VERSION, schemaVersion: 2 } }],
        ['a pre-release', { previous: { version: '1.0.0-rc.1', schemaVersion: 2 } }],
        ['a build that can’t update itself', { previous: PREVIOUS, mode: 'link' }],
    ]) {
        await withApp({ update }, async (app) => {
            assert.equal(calls(app, 'check_previous_version').length, 1, `${what}: asked with the launch check`);
            app.open('about');
            assert.equal($(app, 'go-back').hidden, true, what);
            assert.equal($(app, 'go-back-btn').hidden, true, what);
            assert.equal($(app, 'go-back-unavailable').hidden, true, what);
            assert.deepEqual(app.consoleErrors, [], `${what}: quietly`);
        });
    }
    await withApp({ update: { previous: PREVIOUS } }, async (app) => {
        app.open('about');
        const button = $(app, 'go-back-btn');
        assert.equal($(app, 'go-back').hidden, false);
        assert.equal(button.hidden, false);
        assert.equal(button.textContent, 'Go back to 1.0.0');
        assert.equal($(app, 'go-back-unavailable').hidden, true);
        // Under Check now, in About's Updates.
        assert.ok(button.closest('section') === $(app, 'update-check-btn').closest('section'));
        // No URL from the page: the backend reads its own release's file.
        for (const call of calls(app, 'check_previous_version')) assert.deepEqual(Object.keys(call.args), []);
    });
});

test('with the launch check off, Go back is asked for by Check now', () =>
    withApp({ update: { previous: PREVIOUS }, storage: { 'gospel-getter-update-check': 'off' } }, async (app) => {
        assert.equal(calls(app, 'check_previous_version').length, 0);
        await checkNow(app);
        assert.equal(calls(app, 'check_previous_version').length, 1);
        assert.equal($(app, 'go-back-btn').hidden, false);
    }));

test('B8: a release that can’t read this data gets one line, no button', () =>
    withApp({ update: { previous: { version: '1.0.0', schemaVersion: 0 } } }, async (app) => {
        app.open('about');
        assert.equal($(app, 'go-back').hidden, false);
        assert.equal($(app, 'go-back-btn').hidden, true);
        const line = $(app, 'go-back-unavailable');
        assert.equal(line.hidden, false);
        assert.equal(line.textContent, 'Going back to 1.0.0 isn’t offered: it can’t read the data this version has updated.');
        assert.equal(calls(app, 'save_backup_before_going_back').length, 0);
    }));

test('Go back asks first: its dialog over About, Escape, Cancel and the dimmed page close it alone', () =>
    withApp({ update: { previous: PREVIOUS } }, async (app) => {
        app.open('about');
        const button = $(app, 'go-back-btn');
        const overlay = $(app, 'go-back-overlay');
        const open = () => {
            button.focus();
            app.click(button);
            assert.equal(overlay.hidden, false);
        };
        open();
        const panel = $(app, 'go-back-panel');
        assert.equal(panel.getAttribute('role'), 'dialog');
        assert.equal(panel.getAttribute('aria-modal'), 'true');
        assert.equal($(app, 'go-back-title').textContent, 'Go back to 1.0.0?');
        assert.equal(
            $(app, 'go-back-body').textContent,
            'Gospel Getter will save a full backup of your notes, bookmarks, highlights and settings next to its database, then install 1.0.0 and restart.',
        );
        assert.equal($(app, 'go-back-confirm').textContent, 'Go back');
        assert.equal(app.document.activeElement, $(app, 'go-back-cancel'), 'focus on Cancel');
        // Tab stays inside it.
        $(app, 'go-back-cancel').focus();
        const tab = await app.key('Tab', {}, $(app, 'go-back-cancel'));
        assert.ok(tab.defaultPrevented);
        assert.equal(app.document.activeElement, $(app, 'go-back-confirm'));

        const esc = await app.key('Escape', {}, app.document.activeElement);
        assert.ok(esc.defaultPrevented);
        assert.equal(overlay.hidden, true, 'Escape closes the dialog');
        assert.equal($(app, 'about-overlay').hidden, false, 'and only the dialog');
        assert.equal(app.document.activeElement, button, 'focus back on Go back');

        open();
        app.click($(app, 'go-back-cancel'));
        assert.equal(overlay.hidden, true);
        open();
        app.click(overlay);
        assert.equal(overlay.hidden, true, 'a click on the dimmed page');
        assert.equal($(app, 'about-overlay').hidden, false);
        assert.equal(calls(app, 'save_backup_before_going_back').length, 0, 'nothing done');
        assert.equal(app.storage()[SKIPPED_KEY], undefined);
    }));

test('Go back: the backup is saved beside the database first, this version skipped, then the install, with its progress', () =>
    withApp({
        update: {
            previous: PREVIOUS,
            install: {
                events: [
                    { event: 'started', data: { contentLength: 1000 } },
                    { event: 'progress', data: { chunkLength: 250 } },
                ],
                problem: 'notVerified',
            },
        },
    }, async (app) => {
        await backend('add_note', { bookId: 43, chapter: 3, verseStart: 16, verseEnd: 16, body: 'Kept before going back.' });
        app.open('about');
        app.click($(app, 'go-back-btn'));
        const before = app.calls.length;
        app.click($(app, 'go-back-confirm'));
        assert.equal($(app, 'go-back-overlay').hidden, true);
        assert.equal($(app, 'go-back-btn').hidden, true, 'no second go');
        assert.equal($(app, 'update-check-btn').disabled, true, 'no check meanwhile');
        assert.equal($(app, 'go-back-status').textContent, 'Saving a backup of your notes, bookmarks, highlights and settings…');
        await app.idle(before, 'install_previous_version');

        const made = app.calls.slice(before).map((c) => c.cmd).filter((c) => c !== 'get_app_version');
        assert.deepEqual(made, ['save_backup_before_going_back', 'install_previous_version'], 'the backup first');
        const [backup] = calls(app, 'save_backup_before_going_back');
        assert.ok(backup.args.preferences && typeof backup.args.preferences === 'object', 'with the display settings');

        const saved = $(app, 'go-back-saved').textContent;
        assert.match(saved, /^Backup saved: /);
        const path = saved.slice('Backup saved: '.length);
        kept.push(path);
        assert.ok(existsSync(path), path);
        assert.match(path, new RegExp(`gospel-getter-backup-before-1\\.0\\.0-\\d{4}-\\d{2}-\\d{2}\\.gospel-getter\\.json$`));
        assert.ok(existsSync(join(path, '..', 'gospel_getter.db')), 'beside the database');
        const file = JSON.parse(readFileSync(path, 'utf8'));
        assert.equal(file.format, 'gospel-getter-reader-data');
        assert.ok(file.notes.some((n) => n.body === 'Kept before going back.'), 'the whole of the reader’s data');

        assert.equal(app.storage()[SKIPPED_KEY], VERSION, 'the version left behind is skipped');

        // Progress, as Update now's, then the problem, and Go back offered again.
        assert.equal($(app, 'go-back-status').textContent, NOT_VERIFIED);
        assert.ok($(app, 'go-back-status').classList.contains('is-error'));
        assert.equal($(app, 'go-back-progress').hidden, true);
        assert.equal($(app, 'go-back-btn').hidden, false);
        assert.equal($(app, 'update-check-btn').disabled, false);
    }));

test('Go back’s download shows its progress, and closing About hands it to the banner', () =>
    withApp({
        update: {
            previous: PREVIOUS,
            // Part way through (the install never answers).
            install: {
                events: [
                    { event: 'started', data: { contentLength: 1000 } },
                    { event: 'progress', data: { chunkLength: 500 } },
                ],
            },
        },
    }, async (app) => {
        app.open('about');
        app.click($(app, 'go-back-btn'));
        const before = app.calls.length;
        app.click($(app, 'go-back-confirm'));
        await app.until(() => app.calls.slice(before).some((c) => c.cmd === 'install_previous_version'), 'install_previous_version');
        await app.until(() => $(app, 'go-back-status').textContent.endsWith('50%'), 'progress');
        assert.equal($(app, 'go-back-status').textContent, 'Downloading 1.0.0… 50%');
        assert.equal($(app, 'go-back-progress').hidden, false);
        assert.equal($(app, 'go-back-progress').value, 500);
        const [path] = $(app, 'go-back-saved').textContent.split(': ').slice(1);
        kept.push(path);

        await app.key('Escape', {}, app.document.activeElement);
        assert.equal($(app, 'about-overlay').hidden, true);
        assert.equal(banner(app).hidden, false, 'the banner carries on');
        assert.equal($(app, 'update-title').textContent, 'Going back to 1.0.0');
        assert.equal($(app, 'update-later').hidden, true);
        assert.equal($(app, 'update-skip').hidden, true);
        assert.equal($(app, 'update-progress').hidden, false);
    }));

test('a backup that can’t be saved stops Go back there: nothing is installed or skipped', () =>
    withApp({
        update: { previous: PREVIOUS, backupFolder: join(tmpdir(), 'gospel-getter-no-such-folder', 'deeper') },
    }, async (app) => {
        app.open('about');
        app.click($(app, 'go-back-btn'));
        const before = app.calls.length;
        app.click($(app, 'go-back-confirm'));
        await app.idle(before, 'save_backup_before_going_back');
        assert.equal(calls(app, 'install_previous_version').length, 0, 'nothing installed');
        assert.equal($(app, 'go-back-status').textContent, BACKUP_FAILED);
        assert.ok($(app, 'go-back-status').classList.contains('is-error'));
        assert.equal($(app, 'go-back-saved').hidden, true);
        assert.equal(app.storage()[SKIPPED_KEY], undefined, 'nothing skipped');
        assert.equal($(app, 'go-back-btn').hidden, false, 'and it can be tried again');
        assert.equal($(app, 'update-check-btn').disabled, false);
        // The next check's answer stands: what Go back said is cleared.
        await checkNow(app);
        assert.equal($(app, 'go-back-status').textContent, '');
        assert.ok(!$(app, 'go-back-status').classList.contains('is-error'));
        assert.equal($(app, 'go-back-btn').hidden, false);
    }));
