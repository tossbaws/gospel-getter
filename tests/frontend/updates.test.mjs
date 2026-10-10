// Behavior tests for updates: the Updates section in Settings, the check at
// startup (and the setting that turns it off), Check now, the "new version"
// banner with Later, installing with download progress, and the
// download-page variant for builds that can't update themselves. The check
// and the install are scripted in the test bridge (`setUpdate`), which
// answers with the app's own messages.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { closeBridge, openApp, setUpdate } from './harness.mjs';

after(closeBridge);

// The version the app reports about itself, as tauri.conf.json declares it.
const VERSION = JSON.parse(
    readFileSync(new URL('../../src-tauri/tauri.conf.json', import.meta.url), 'utf8'),
).version;

const OFFLINE = 'Couldn’t reach GitHub to check for updates. Check your internet connection and try again.';
const NOT_VERIFIED = 'The update couldn’t be verified as a genuine Gospel Getter release, so it wasn’t installed.';

const UPDATE = {
    version: '2.6.0',
    notes: 'Highlights list in Settings.\nUpdates from inside the app.',
    date: '2026-10-20',
    canSelfUpdate: true,
};

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const $ = (app, id) => app.document.getElementById(id);
const banner = (app) => $(app, 'update-banner');
const checks = (app) => app.calls.filter((c) => c.cmd === 'check_for_update').length;
const checkStatus = (app) => $(app, 'update-check-status');

async function click(app, el, cmd) {
    const before = app.calls.length;
    app.click(el);
    await app.idle(before, cmd);
}

test('Settings has an Updates section: a launch check that is on, Check now, and what it sends', () =>
    withApp({}, async (app) => {
        const toggle = $(app, 'update-check-toggle');
        assert.equal(toggle.type, 'checkbox');
        assert.equal(toggle.checked, true, 'on by default');
        assert.equal(toggle.closest('label').textContent.trim(), 'Check for updates when Gospel Getter starts');
        assert.equal(
            $(app, 'update-note').textContent,
            'Checking downloads a small file from GitHub that names the latest version, so GitHub sees your IP address, as with any download. Nothing about your reading or your data is sent.',
        );
        const button = $(app, 'update-check-btn');
        assert.equal(button.tagName, 'BUTTON');
        assert.equal(button.textContent, 'Check now');
        assert.equal(button.getAttribute('aria-describedby'), 'update-note');
        assert.equal(checkStatus(app).getAttribute('role'), 'status');
        // Next to the version line, at the foot of the menu.
        assert.equal(button.closest('.settings-field').nextElementSibling, $(app, 'app-version'));

        // Checked once at startup; no update, so no banner and nothing said.
        assert.equal(checks(app), 1);
        assert.equal(banner(app).hidden, true);
        assert.equal(checkStatus(app).textContent, '');
        assert.deepEqual(app.consoleErrors, []);
    }));

test('a newer version shows a banner, without taking focus, with its notes and Update and restart', () =>
    withApp({ update: { check: UPDATE } }, async (app) => {
        assert.equal(banner(app).hidden, false);
        assert.equal($(app, 'update-title').textContent, 'Gospel Getter 2.6.0 is available');
        // Announced politely; not a dialog, and focus stays where it was.
        assert.equal($(app, 'update-title').getAttribute('role'), 'status');
        assert.equal(banner(app).getAttribute('role'), null);
        assert.equal(banner(app).getAttribute('aria-labelledby'), 'update-title');
        assert.ok(!banner(app).contains(app.document.activeElement));

        const notes = $(app, 'update-notes');
        assert.equal(notes.tagName, 'DETAILS');
        assert.equal(notes.hidden, false);
        assert.equal(notes.open, false, 'collapsed until asked for');
        assert.equal(notes.querySelector('summary').textContent, 'What’s new');
        assert.equal($(app, 'update-notes-text').textContent, UPDATE.notes, 'shown as plain text');

        assert.equal($(app, 'update-install').hidden, false);
        assert.equal($(app, 'update-install').textContent, 'Update and restart');
        assert.equal($(app, 'update-later').hidden, false);
        assert.equal($(app, 'update-download').hidden, true);
        // Nothing is installed until the reader asks.
        assert.equal(app.calls.filter((c) => c.cmd === 'install_update').length, 0);
        assert.equal(checkStatus(app).textContent, '', 'a launch check says nothing in Settings');
    }));

test('release notes are never treated as markup', () =>
    withApp({ update: { check: { ...UPDATE, notes: '<img src=x onerror="window.pwned=1"> & <b>bold</b>' } } }, async (app) => {
        const text = $(app, 'update-notes-text');
        assert.equal(text.textContent, '<img src=x onerror="window.pwned=1"> & <b>bold</b>');
        assert.equal(text.children.length, 0);
        assert.equal(app.window.pwned, undefined);
    }));

test('an update with no notes has no notes section', () =>
    withApp({ update: { check: { ...UPDATE, notes: null } } }, async (app) => {
        assert.equal(banner(app).hidden, false);
        assert.equal($(app, 'update-notes').hidden, true);
    }));

test('Later hides the banner until the next launch', async () => {
    let storage;
    await withApp({ update: { check: UPDATE } }, async (app) => {
        $(app, 'update-later').focus();
        app.click($(app, 'update-later'));
        assert.equal(banner(app).hidden, true);
        assert.equal(app.document.activeElement, $(app, 'settings-toggle'), 'focus isn’t lost with the banner');
        storage = app.storage();
        assert.equal(Object.keys(storage).filter((k) => k.includes('update')).length, 0, 'Later isn’t remembered');
    });
    await withApp({ update: { check: UPDATE }, storage }, async (app) => {
        assert.equal(banner(app).hidden, false, 'back at the next launch');
    });
});

test('with the setting off there is no check at startup; it is kept, but not exported', async () => {
    let storage;
    await withApp({ update: { check: UPDATE } }, async (app) => {
        const toggle = $(app, 'update-check-toggle');
        toggle.checked = false;
        toggle.dispatchEvent(new app.window.Event('change', { bubbles: true }));
        storage = app.storage();
        assert.equal(storage['gospel-getter-update-check'], 'off');
        // A setting for this computer: data export carries only display settings.
        assert.deepEqual(
            Object.keys(app.window.gospelGetterPreferences.current()).sort(),
            ['compare', 'line_spacing', 'reading_mode', 'text_size', 'theme'],
        );
    });
    await withApp({ update: { check: UPDATE }, storage }, async (app) => {
        assert.equal($(app, 'update-check-toggle').checked, false);
        assert.equal(checks(app), 0, 'no request at all');
        assert.equal(banner(app).hidden, true);

        // Check now still works.
        app.openSettings();
        await click(app, $(app, 'update-check-btn'), 'check_for_update');
        assert.equal(checks(app), 1);
        assert.equal(banner(app).hidden, false);
        assert.equal(checkStatus(app).textContent, 'Gospel Getter 2.6.0 is available.');
    });
});

test('Check now says when the app is up to date', () =>
    withApp({}, async (app) => {
        app.openSettings();
        await click(app, $(app, 'update-check-btn'), 'check_for_update');
        assert.equal(checkStatus(app).textContent, `Gospel Getter ${VERSION} is the latest version.`);
        assert.ok(!checkStatus(app).classList.contains('is-error'));
        assert.equal(banner(app).hidden, true);
        assert.equal($(app, 'update-check-btn').disabled, false);
    }));

test('Check now finds an update after Later, and shows the banner again', () =>
    withApp({ update: { check: UPDATE } }, async (app) => {
        app.click($(app, 'update-later'));
        app.openSettings();
        await click(app, $(app, 'update-check-btn'), 'check_for_update');
        assert.equal(banner(app).hidden, false);
        assert.equal(checkStatus(app).textContent, 'Gospel Getter 2.6.0 is available.');
    }));

test('offline: the check at startup is silent, Check now says what went wrong', () =>
    withApp({ update: { check: { problem: 'offline' } } }, async (app) => {
        assert.equal(checks(app), 1);
        assert.equal(banner(app).hidden, true);
        assert.equal(checkStatus(app).textContent, '');
        assert.deepEqual(app.consoleErrors, [], 'not even logged as an error');
        // The reader is unaffected.
        assert.equal(app.heading(), 'John 3');

        app.openSettings();
        const button = $(app, 'update-check-btn');
        app.click(button);
        assert.equal(checkStatus(app).textContent, 'Checking for updates…');
        assert.equal(button.disabled, true, 'one check at a time');
        await app.idle(0, 'check_for_update');
        assert.equal(checkStatus(app).textContent, OFFLINE);
        assert.ok(checkStatus(app).classList.contains('is-error'));
        assert.equal(button.disabled, false);
    }));

test('a build that can’t update itself offers the download page instead', () =>
    withApp({ update: { check: { ...UPDATE, canSelfUpdate: false } } }, async (app) => {
        assert.equal(banner(app).hidden, false);
        assert.equal($(app, 'update-title').textContent, 'Gospel Getter 2.6.0 is available');
        assert.equal($(app, 'update-install').hidden, true);
        const download = $(app, 'update-download');
        assert.equal(download.hidden, false);
        assert.equal(download.textContent, 'Download page');

        await click(app, download, 'open_download_page');
        // No URL from the page: the backend opens the one releases page.
        const [call] = app.calls.filter((c) => c.cmd === 'open_download_page');
        assert.deepEqual(Object.keys(call.args), []);
        assert.equal($(app, 'update-status').textContent, 'The releases page is open in your browser.');
        assert.equal(app.calls.filter((c) => c.cmd === 'install_update').length, 0);
    }));

test('installing shows download progress, and the reader stays usable', () =>
    withApp({
        update: {
            check: UPDATE,
            // Part way through the download (the install never answers).
            install: {
                events: [
                    { event: 'started', data: { contentLength: 1000 } },
                    { event: 'progress', data: { chunkLength: 250 } },
                ],
            },
        },
    }, async (app) => {
        const install = $(app, 'update-install');
        install.focus();
        const before = app.calls.length;
        app.click(install);
        await app.until(() => app.calls.slice(before).some((c) => c.cmd === 'install_update'), 'install_update');
        await app.until(() => $(app, 'update-status').textContent.endsWith('25%'), 'progress');

        const progress = $(app, 'update-progress');
        assert.equal(progress.hidden, false);
        assert.equal(progress.max, 1000);
        assert.equal(progress.value, 250);
        assert.equal($(app, 'update-status').textContent, 'Downloading the update… 25%');
        assert.equal($(app, 'update-status').getAttribute('role'), 'status');
        // No second install, and no Later halfway through.
        assert.equal(install.hidden, true);
        assert.equal($(app, 'update-later').hidden, true);
        assert.equal(app.document.activeElement, banner(app), 'focus moves to the banner, not lost');

        // Reading goes on while it downloads.
        app.key('ArrowRight');
        await app.waitForHeading('John 4');
        app.click(app.verse(24));
        assert.deepEqual(app.selectedVerseNumbers(), [24]);
        assert.equal(banner(app).hidden, false);
    }));

test('when the download finishes it says it is installing and will restart', () =>
    withApp({
        update: {
            check: UPDATE,
            install: {
                events: [
                    { event: 'started', data: { contentLength: null } },
                    { event: 'progress', data: { chunkLength: 3145728 } },
                    { event: 'finished' },
                ],
            },
        },
    }, async (app) => {
        app.click($(app, 'update-install'));
        await app.until(() => $(app, 'update-status').textContent.startsWith('Installing'), 'installing');
        assert.equal($(app, 'update-status').textContent, 'Installing the update. Gospel Getter will restart when it’s done.');
        // Unknown size: no percentage, an indeterminate bar.
        assert.equal($(app, 'update-progress').hasAttribute('value'), false);
        const install = app.calls.find((c) => c.cmd === 'install_update');
        assert.equal(install.done, false, 'restarting, so it never answers');
    }));

test('an update that fails verification is reported, and can be retried or put off', () =>
    withApp({
        update: {
            check: UPDATE,
            install: { events: [{ event: 'started', data: { contentLength: 10 } }], problem: 'notVerified' },
        },
    }, async (app) => {
        $(app, 'update-install').focus();
        await click(app, $(app, 'update-install'), 'install_update');
        const status = $(app, 'update-status');
        assert.equal(status.textContent, NOT_VERIFIED);
        assert.ok(status.classList.contains('is-error'));
        assert.equal($(app, 'update-progress').hidden, true);
        assert.equal($(app, 'update-install').hidden, false);
        assert.equal($(app, 'update-later').hidden, false);
        assert.equal(app.document.activeElement, $(app, 'update-install'));

        // A retry goes through again.
        await setUpdate({ check: UPDATE, install: { events: [], problem: 'offline' } });
        await click(app, $(app, 'update-install'), 'install_update');
        assert.equal(status.textContent, 'The update couldn’t be downloaded. Check your internet connection and try again.');
    }));
