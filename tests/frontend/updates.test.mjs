// Behavior tests for updates: the Updates section in About, the check at
// startup (and the setting that turns it off), Check now, the "new version"
// banner with Later, the update shown inside About when it's found there,
// installing with download progress (from either), and the download-page
// variant for builds that can't update themselves. The check
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

test('About has an Updates section: a launch check that is on, Check now, and what it sends', () =>
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
        // In About, under the name and version.
        const section = button.closest('section');
        assert.ok(section.closest('#about-panel'));
        assert.equal($(app, section.getAttribute('aria-labelledby')).textContent, 'Updates');
        assert.ok(toggle.closest('section') === section && checkStatus(app).closest('section') === section);

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
        assert.equal(checkStatus(app).textContent, '', 'a launch check says nothing in About');
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
        assert.equal(app.document.activeElement, $(app, 'menu-toggle'), 'focus isn’t lost with the banner');
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
            ['compare', 'layout', 'line_spacing', 'reading_mode', 'text_size', 'theme', 'width'],
        );
    });
    await withApp({ update: { check: UPDATE }, storage }, async (app) => {
        assert.equal($(app, 'update-check-toggle').checked, false);
        assert.equal(checks(app), 0, 'no request at all');
        assert.equal(banner(app).hidden, true);

        // Check now still works, and About shows what it finds.
        app.open('about');
        await click(app, $(app, 'update-check-btn'), 'check_for_update');
        assert.equal(checks(app), 1);
        assert.equal($(app, 'about-update').hidden, false);
        assert.equal(banner(app).hidden, true);
        assert.equal(checkStatus(app).textContent, 'Gospel Getter 2.6.0 is available.');
    });
});

test('Check now says when the app is up to date', () =>
    withApp({}, async (app) => {
        app.open('about');
        await click(app, $(app, 'update-check-btn'), 'check_for_update');
        assert.equal(checkStatus(app).textContent, `Gospel Getter ${VERSION} is the latest version.`);
        assert.ok(!checkStatus(app).classList.contains('is-error'));
        assert.equal(banner(app).hidden, true);
        assert.equal($(app, 'update-check-btn').disabled, false);
    }));

test('Check now finds an update after Later, and shows it in About', () =>
    withApp({ update: { check: UPDATE } }, async (app) => {
        app.click($(app, 'update-later'));
        app.open('about');
        await click(app, $(app, 'update-check-btn'), 'check_for_update');
        assert.equal($(app, 'about-update').hidden, false);
        assert.equal($(app, 'about-update-title').textContent, 'Gospel Getter 2.6.0 is available');
        assert.equal(banner(app).hidden, true);
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

        app.open('about');
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

// ---- An update found in About shows in About

const NOTES_MD = [
    'Gospel Getter 2.13.1 fixes two things.',
    '',
    '## Fixes',
    '- **Search** finds words again.',
    '  - Even `G`.',
    '- The [About](https://example.com) box.',
    '',
    '| File | Platform |',
    '|---|---|',
    '| `a.deb` | Debian |',
].join('\n');
const FOUND = { ...UPDATE, version: '2.13.1', notes: NOTES_MD };
const aboutUpdate = (app) => $(app, 'about-update');
const notesBox = (app) => $(app, 'about-update-notes');

async function checkInAbout(app) {
    app.open('about');
    await click(app, $(app, 'update-check-btn'), 'check_for_update');
}

test('About’s check finds an update: About shows it, its notes, Update now and Close, and no banner', async () => {
    let storage;
    // Checks at startup are off, so the banner can't have come first.
    await withApp({ update: { check: FOUND }, storage: { 'gospel-getter-update-check': 'off' } }, async (app) => {
        await checkInAbout(app);
        assert.equal(banner(app).hidden, true, 'no banner as well');
        assert.equal(aboutUpdate(app).hidden, false);
        assert.ok(app.surface('about').contains(aboutUpdate(app)), 'in the About panel');
        assert.equal(aboutUpdate(app).getAttribute('role'), 'group');
        const title = $(app, aboutUpdate(app).getAttribute('aria-labelledby'));
        assert.equal(title.textContent, 'Gospel Getter 2.13.1 is available');
        // Said once, by the check's status line (the heading shows it).
        assert.equal(checkStatus(app).textContent, 'Gospel Getter 2.13.1 is available.');
        assert.ok(checkStatus(app).classList.contains('visually-hidden'));

        // The notes, read as Markdown by the backend, built as text.
        const box = notesBox(app);
        assert.equal(box.hidden, false);
        assert.equal(box.getAttribute('role'), 'region');
        assert.equal(box.getAttribute('aria-label'), 'What’s new in 2.13.1');
        assert.equal(box.tabIndex, 0, 'scrollable from the keyboard');
        assert.deepEqual([...box.children].map((el) => el.tagName), ['P', 'H5', 'UL', 'P', 'P']);
        assert.equal(box.querySelector('h5').textContent, 'Fixes');
        const items = [...box.querySelectorAll(':scope > ul > li')];
        assert.equal(items.length, 2);
        assert.equal(items[0].querySelector('strong').textContent, 'Search');
        assert.equal(items[0].querySelector('ul > li code').textContent, 'G', 'nested under its item');
        assert.equal(items[1].textContent, 'The About box.');
        assert.equal(box.querySelector('a'), null, 'links are their text');
        assert.equal(box.children[4].textContent, 'a.deb — Debian');

        const install = $(app, 'about-update-install');
        assert.equal(install.textContent, 'Update now');
        assert.equal(install.hidden, false);
        assert.equal($(app, 'about-update-download').hidden, true);
        assert.equal($(app, 'about-update-close').textContent, 'Close');
        assert.equal(app.calls.filter((c) => c.cmd === 'install_update').length, 0, 'nothing installed unasked');

        // Close: About closes, focus goes back to ☰, and no banner follows.
        $(app, 'about-update-close').focus();
        app.click($(app, 'about-update-close'));
        assert.equal(app.surface('about').hidden, true);
        assert.equal(app.document.activeElement, $(app, 'menu-toggle'));
        assert.equal(banner(app).hidden, true, 'the banner stays away, as after Later');
        // Reopened, About still offers it.
        app.open('about');
        assert.equal(aboutUpdate(app).hidden, false);
        // Esc closes About as it does any panel.
        app.key('Escape', {}, app.document.activeElement);
        assert.equal(app.surface('about').hidden, true);
        assert.equal(app.document.activeElement, $(app, 'menu-toggle'));
        assert.equal(banner(app).hidden, true);
        storage = app.storage();
        assert.deepEqual(app.consoleErrors, []);
    });
    // Nothing is remembered: the next launch's check shows the banner.
    delete storage['gospel-getter-update-check'];
    await withApp({ update: { check: FOUND }, storage }, async (app) => {
        assert.equal(banner(app).hidden, false);
        assert.equal(aboutUpdate(app).hidden, true);
    });
});

test('release notes in About are never treated as markup', () =>
    withApp({ update: { check: { ...FOUND, notes: '<img src=x onerror="window.pwned=1"> **<b>bold</b>**' } } }, async (app) => {
        await checkInAbout(app);
        const box = notesBox(app);
        assert.equal(box.textContent, '<img src=x onerror="window.pwned=1"> <b>bold</b>');
        assert.equal(box.querySelector('img, b'), null);
        assert.deepEqual([...box.querySelectorAll('*')].map((el) => el.tagName), ['P', 'STRONG']);
        assert.equal(app.window.pwned, undefined);
    }));

test('an update with no notes shows no notes box in About', () =>
    withApp({ update: { check: { ...FOUND, notes: null } } }, async (app) => {
        await checkInAbout(app);
        assert.equal(aboutUpdate(app).hidden, false);
        assert.equal(notesBox(app).hidden, true);
    }));

test('About’s check finding nothing, or failing, shows no update', async () => {
    await withApp({}, async (app) => {
        await checkInAbout(app);
        assert.equal(checkStatus(app).textContent, `Gospel Getter ${VERSION} is the latest version.`);
        assert.ok(!checkStatus(app).classList.contains('visually-hidden'));
        assert.equal(aboutUpdate(app).hidden, true);
        assert.equal(banner(app).hidden, true);
    });
    await withApp({ update: { check: { problem: 'offline' } } }, async (app) => {
        await checkInAbout(app);
        assert.equal(checkStatus(app).textContent, OFFLINE);
        assert.ok(checkStatus(app).classList.contains('is-error'));
        assert.equal(aboutUpdate(app).hidden, true);
        assert.equal(banner(app).hidden, true);
    });
});

test('the launch check finishing while About is open shows the update in About, not the banner', () =>
    withApp({ update: { check: FOUND }, hold: ['check_for_update'] }, async (app) => {
        // The reader opens About before the check at startup answers.
        app.open('about');
        assert.equal(aboutUpdate(app).hidden, true);
        app.release('check_for_update');
        await app.idle(0, 'check_for_update');
        assert.equal(checks(app), 1, 'the startup check, not Check now');
        assert.equal(aboutUpdate(app).hidden, false);
        assert.equal($(app, 'about-update-title').textContent, 'Gospel Getter 2.13.1 is available');
        assert.equal(banner(app).hidden, true);
        assert.equal(app.document.activeElement, $(app, 'about-close'), 'focus stays where it was');
    }));

test('Update now installs from About, with its progress there; closing About hands it to the banner', () =>
    withApp({
        update: {
            check: FOUND,
            install: {
                events: [
                    { event: 'started', data: { contentLength: 1000 } },
                    { event: 'progress', data: { chunkLength: 400 } },
                ],
            },
        },
        storage: { 'gospel-getter-update-check': 'off' },
    }, async (app) => {
        await checkInAbout(app);
        const install = $(app, 'about-update-install');
        install.focus();
        const before = app.calls.length;
        app.click(install);
        await app.until(() => app.calls.slice(before).some((c) => c.cmd === 'install_update'), 'install_update');
        const status = $(app, 'about-update-status');
        await app.until(() => status.textContent.endsWith('40%'), 'progress');
        assert.equal(status.textContent, 'Downloading the update… 40%');
        assert.equal(status.getAttribute('role'), 'status');
        const progress = $(app, 'about-update-progress');
        assert.equal(progress.hidden, false);
        assert.equal(progress.max, 1000);
        assert.equal(progress.value, 400);
        assert.equal(install.hidden, true, 'no second install');
        assert.equal($(app, 'about-update-close').hidden, true);
        assert.equal(app.document.activeElement, aboutUpdate(app), 'focus moves to the update, not lost');
        assert.equal(banner(app).hidden, true, 'About shows it; no banner');

        // Esc still closes About; the banner carries on with the download.
        app.key('Escape', {}, app.document.activeElement);
        assert.equal(app.surface('about').hidden, true);
        assert.equal(banner(app).hidden, false);
        assert.equal($(app, 'update-title').textContent, 'Gospel Getter 2.13.1 is available');
        assert.equal($(app, 'update-status').textContent, 'Downloading the update… 40%');
        assert.equal($(app, 'update-progress').value, 400);
        assert.equal($(app, 'update-install').hidden, true);
        assert.equal($(app, 'update-later').hidden, true);
        assert.equal(app.calls.filter((c) => c.cmd === 'install_update').length, 1);
    }));

test('when the download finishes, About says it is installing and will restart', () =>
    withApp({
        update: {
            check: FOUND,
            install: {
                events: [
                    { event: 'started', data: { contentLength: null } },
                    { event: 'progress', data: { chunkLength: 3145728 } },
                    { event: 'finished' },
                ],
            },
        },
        storage: { 'gospel-getter-update-check': 'off' },
    }, async (app) => {
        await checkInAbout(app);
        app.click($(app, 'about-update-install'));
        const status = $(app, 'about-update-status');
        await app.until(() => status.textContent.startsWith('Installing'), 'installing');
        assert.equal(status.textContent, 'Installing the update. Gospel Getter will restart when it’s done.');
        // Unknown size: no percentage, an indeterminate bar.
        assert.equal($(app, 'about-update-progress').hasAttribute('value'), false);
        const install = app.calls.find((c) => c.cmd === 'install_update');
        assert.equal(install.done, false, 'restarting, so it never answers');
    }));

test('an install that fails in About says why there, and can be retried', () =>
    withApp({
        update: {
            check: FOUND,
            install: { events: [{ event: 'started', data: { contentLength: 10 } }], problem: 'notVerified' },
        },
        storage: { 'gospel-getter-update-check': 'off' },
    }, async (app) => {
        await checkInAbout(app);
        const install = $(app, 'about-update-install');
        install.focus();
        await click(app, install, 'install_update');
        const status = $(app, 'about-update-status');
        assert.equal(status.textContent, NOT_VERIFIED);
        assert.ok(status.classList.contains('is-error'));
        assert.equal($(app, 'about-update-progress').hidden, true);
        assert.equal(install.hidden, false);
        assert.equal($(app, 'about-update-close').hidden, false);
        assert.equal(app.document.activeElement, install);
        assert.equal(app.surface('about').hidden, false, 'About stays open');
        assert.equal(banner(app).hidden, true);

        await setUpdate({ check: FOUND, install: { events: [], problem: 'offline' } });
        await click(app, install, 'install_update');
        assert.equal(status.textContent, 'The update couldn’t be downloaded. Check your internet connection and try again.');
    }));

test('a build that can’t update itself offers the download page in About', () =>
    withApp({ update: { check: { ...FOUND, canSelfUpdate: false } }, storage: { 'gospel-getter-update-check': 'off' } }, async (app) => {
        await checkInAbout(app);
        assert.equal($(app, 'about-update-install').hidden, true);
        const download = $(app, 'about-update-download');
        assert.equal(download.hidden, false);
        await click(app, download, 'open_download_page');
        assert.equal($(app, 'about-update-status').textContent, 'The releases page is open in your browser.');
        assert.equal(app.calls.filter((c) => c.cmd === 'install_update').length, 0);
    }));

// Fake timers for the update check's 20-second limit: the page's timers of
// 20 s or more wait until `fire` runs them; shorter ones run as usual.
function longTimers() {
    const waiting = [];
    return {
        waiting,
        setUp(window) {
            const { setTimeout: set, clearTimeout: clear } = window;
            window.setTimeout = (fn, ms, ...args) => {
                if (ms < 20000) return set.call(window, fn, ms, ...args);
                waiting.push({ fn, ms, args });
                return -waiting.length;
            };
            window.clearTimeout = (id) => {
                if (id < 0) waiting[-id - 1].cleared = true;
                else clear.call(window, id);
            };
        },
        fire() {
            for (const timer of waiting) {
                if (timer.cleared || timer.fired) continue;
                timer.fired = true;
                timer.fn(...timer.args);
            }
        },
    };
}

test('Check now gives up after 20 seconds with no answer, and says GitHub couldn’t be reached', () => {
    const timers = longTimers();
    return withApp({ update: { check: FOUND }, storage: { 'gospel-getter-update-check': 'off' }, setUp: timers.setUp }, async (app) => {
        app.open('about');
        const release = app.holdCommand('check_for_update');
        const button = $(app, 'update-check-btn');
        app.click(button);
        assert.equal(checkStatus(app).textContent, 'Checking for updates…');
        assert.deepEqual(timers.waiting.map((t) => t.ms), [20000]);
        timers.fire();
        await app.until(() => checkStatus(app).textContent === OFFLINE, 'the timeout');
        assert.ok(checkStatus(app).classList.contains('is-error'));
        assert.equal(button.disabled, false, 'Check now can be tried again');

        // The answer coming after all changes nothing.
        release();
        await app.idle(0, 'check_for_update');
        assert.equal(checkStatus(app).textContent, OFFLINE);
        assert.equal(aboutUpdate(app).hidden, true);
        assert.equal(banner(app).hidden, true);

        // A check that answers in time clears its timer.
        await click(app, button, 'check_for_update');
        assert.equal(aboutUpdate(app).hidden, false);
        assert.ok(timers.waiting.every((t) => t.cleared || t.fired));
    });
});

test('the check at startup gives up after 20 seconds too, saying nothing', () => {
    const timers = longTimers();
    return withApp({ update: { check: FOUND }, hold: ['check_for_update'], setUp: timers.setUp }, async (app) => {
        await app.until(() => timers.waiting.length === 1, 'the startup check');
        timers.fire();
        await app.idle();
        app.release('check_for_update');
        await app.idle(0, 'check_for_update');
        assert.equal(banner(app).hidden, true, 'no banner from a check given up on');
        assert.equal(checkStatus(app).textContent, '');
        assert.deepEqual(app.consoleErrors, []);
    });
});
