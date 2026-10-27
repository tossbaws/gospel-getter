// About Gospel Getter, from the ☰ menu: the app's name, the version it
// reports about itself (which must be the one tauri.conf.json declares),
// Updates, a Release notes link to that version's release page, and a
// Support Gospel Getter link to the Ko-fi page, both of which the backend
// opens. The version line stays quietly empty, and the Release notes link
// hidden, if the version can't be read.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { closeBridge, openApp } from './harness.mjs';

after(closeBridge);

const VERSION = JSON.parse(
    readFileSync(new URL('../../src-tauri/tauri.conf.json', import.meta.url), 'utf8'),
).version;

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const $ = (app, id) => app.document.getElementById(id);
const versionLine = (app) => $(app, 'app-version');
const link = (app) => $(app, 'release-notes-link');
const supportLink = (app) => $(app, 'support-link');

test(`About shows the icon, the name and "Version ${VERSION}", as a modal like the import dialog`, () =>
    withApp({}, async (app) => {
        await app.idle(0, 'get_app_version');
        app.open('about');
        const panel = $(app, 'about-panel');
        assert.equal(app.surface('about').hidden, false);
        assert.ok(app.surface('menu').hidden, 'the menu closed behind it');
        assert.equal(panel.getAttribute('role'), 'dialog');
        assert.equal(panel.getAttribute('aria-modal'), 'true');
        assert.ok(panel.classList.contains('import-panel'), 'the import dialog’s look');
        assert.ok($(app, 'about-overlay').classList.contains('import-overlay'));
        assert.equal($(app, panel.getAttribute('aria-labelledby')).textContent, 'Gospel Getter');
        assert.equal(panel.querySelector('img').getAttribute('src'), 'favicon.svg');
        assert.equal(versionLine(app).textContent, `Version ${VERSION}`);
        assert.ok(panel.contains(versionLine(app)));
        assert.equal(app.document.activeElement, $(app, 'about-close'), 'focus moves in');
    }));

test('the Release notes link is this version’s release page, opened by the backend', () =>
    withApp({}, async (app) => {
        await app.idle(0, 'get_app_version');
        app.open('about');
        assert.equal(link(app).hidden, false);
        assert.equal(link(app).textContent, 'Release notes');
        assert.equal(link(app).getAttribute('href'), `https://github.com/tossbaws/gospel-getter/releases/tag/v${VERSION}`);
        assert.ok(link(app).closest('#about-panel'));

        const before = app.calls.length;
        const click = new app.window.MouseEvent('click', { bubbles: true, cancelable: true });
        link(app).dispatchEvent(click);
        assert.ok(click.defaultPrevented, 'the page doesn’t navigate itself');
        await app.idle(before, 'open_release_notes');
        // No URL from the page: the backend builds it from its own version.
        const [call] = app.calls.filter((c) => c.cmd === 'open_release_notes');
        assert.deepEqual(Object.keys(call.args), []);
        assert.equal($(app, 'release-notes-status').textContent, '');
        assert.equal(app.surface('about').hidden, false, 'About stays open');
    }));

test('a release page that can’t be opened says so', () =>
    withApp({ failing: { open_release_notes: 'Couldn’t open the release notes in your browser.' } }, async (app) => {
        await app.idle(0, 'get_app_version');
        app.open('about');
        const before = app.calls.length;
        app.click(link(app));
        await app.idle(before, 'open_release_notes');
        assert.equal($(app, 'release-notes-status').textContent, 'Couldn’t open the release notes in your browser.');
    }));

test('the Support Gospel Getter link is the Ko-fi page, opened by the backend', () =>
    withApp({}, async (app) => {
        await app.idle(0, 'get_app_version');
        app.open('about');
        const support = supportLink(app);
        assert.equal(support.hidden, false);
        assert.equal(support.textContent, 'Support Gospel Getter');
        assert.equal(support.getAttribute('href'), 'https://ko-fi.com/crcllc');
        assert.ok(support.closest('#about-panel .about-links'), 'among About’s links');

        const before = app.calls.length;
        const click = new app.window.MouseEvent('click', { bubbles: true, cancelable: true });
        support.dispatchEvent(click);
        assert.ok(click.defaultPrevented, 'the page doesn’t navigate itself');
        await app.idle(before, 'open_support_page');
        // Called once, with no URL from the page: the backend opens its own.
        const calls = app.calls.slice(before).filter((c) => c.cmd === 'open_support_page');
        assert.equal(calls.length, 1);
        assert.deepEqual(Object.keys(calls[0].args), []);
        assert.equal($(app, 'support-status').textContent, '');
        assert.equal(app.surface('about').hidden, false, 'About stays open');
    }));

test('a Support Gospel Getter page that can’t be opened says so', () =>
    withApp({ failing: { open_support_page: 'Couldn’t open the link in your browser.' } }, async (app) => {
        await app.idle(0, 'get_app_version');
        app.open('about');
        const before = app.calls.length;
        app.click(supportLink(app));
        await app.idle(before, 'open_support_page');
        assert.equal($(app, 'support-status').textContent, 'Couldn’t open the link in your browser.');
        assert.equal($(app, 'support-status').hidden, false);
    }));

test('Check now in About works, and About keeps the keyboard while open', () =>
    withApp({}, async (app) => {
        await app.idle(0, 'get_app_version');
        app.open('about');
        const before = app.calls.length;
        app.click($(app, 'update-check-btn'));
        await app.idle(before, 'check_for_update');
        assert.equal($(app, 'update-check-status').textContent, `Gospel Getter ${VERSION} is the latest version.`);

        // Modal: Tab wraps, shortcuts wait, the page behind can't be clicked away to.
        // Support Gospel Getter is About's last control.
        const last = supportLink(app);
        last.focus();
        assert.ok(app.key('Tab', {}, last).defaultPrevented);
        assert.equal(app.document.activeElement, $(app, 'about-close'));
        assert.ok(app.key('Tab', { shiftKey: true }, $(app, 'about-close')).defaultPrevented);
        assert.equal(app.document.activeElement, last);
        app.key('ArrowRight', {}, last);
        app.key('/', {}, last);
        app.key('b', { ctrlKey: true }, last);
        await app.idle(0);
        assert.equal(app.heading(), 'John 3');
        assert.ok(app.searchPanel().hidden);
        assert.ok(app.surface('library').hidden);
        app.click(app.document.querySelector('.chapter-main .chapter-heading'));
        assert.equal(app.surface('about').hidden, false);

        // Escape closes it, and focus goes back to ☰, where it was opened.
        app.key('Escape', {}, last);
        assert.ok(app.surface('about').hidden);
        assert.equal(app.document.activeElement, $(app, 'menu-toggle'));
    }));

test('if the version can\'t be read, the line stays empty, the link hidden, and nothing else breaks', () =>
    withApp({ failing: { get_app_version: 'simulated failure' } }, async (app) => {
        await app.idle(0, 'get_app_version');
        app.open('about');
        assert.equal(versionLine(app).textContent, '');
        assert.equal(link(app).hidden, true);
        assert.deepEqual(app.consoleErrors, []);
        assert.equal(app.heading(), 'John 3');
    }));
