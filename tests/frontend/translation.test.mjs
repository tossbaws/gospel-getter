// Behavior tests for the translation button in the top bar: it shows the
// translation being read by its code and opens the translations and
// Compare, which left Aa (Aa keeps the display settings, and says
// "Display" where there's room). T opens it as the button does. Plus the
// ☰ menu's hover text on Export data… and Import data…. Against the real
// backend and database.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { closeBridge, openApp } from './harness.mjs';

const dir = mkdtempSync(join(tmpdir(), 'gospel-getter-translation-'));
after(async () => {
    await closeBridge();
    rmSync(dir, { recursive: true, force: true });
});

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const $ = (app, id) => app.document.getElementById(id);
const button = (app) => $(app, 'translation-toggle');
const isOpen = (app, name) => !app.surface(name).hidden;
const radio = (app, code) => app.document.querySelector(`#translation-choices [data-value="${code}"]`);
const shownOnScreen = (app, el) => app.window.getComputedStyle(el).display !== 'none';

async function clickAndWait(app, el, cmd) {
    const before = app.calls.length;
    app.click(el);
    await app.idle(before, cmd);
}

test('the translation button shows the code being read and its name, just before Aa, and follows a switch', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        const btn = button(app);
        assert.equal(btn.textContent, 'KJV');
        assert.equal(btn.getAttribute('aria-label'), 'Translation: King James Version');
        assert.equal(btn.title, 'Translation and compare (T)');
        assert.equal(btn.getAttribute('aria-keyshortcuts'), 'T');
        assert.equal(btn.getAttribute('aria-haspopup'), 'dialog');
        assert.equal(btn.getAttribute('aria-controls'), 'translation-popover');
        assert.equal(btn.getAttribute('aria-expanded'), 'false');
        assert.ok(btn.classList.contains('bar-btn'));
        assert.equal(btn.parentElement.nextElementSibling, $(app, 'aa-toggle').parentElement, 'just before Aa');
        const popover = app.surface('translation');
        assert.equal(popover.getAttribute('role'), 'dialog');
        assert.equal(popover.getAttribute('aria-label'), 'Translation and compare');

        app.open('translation');
        await clickAndWait(app, radio(app, 'web'), 'get_reading');
        assert.equal(btn.textContent, 'WEB');
        assert.equal(btn.getAttribute('aria-label'), 'Translation: World English Bible');

        // Comparing, and changing what to compare with: still the one being read.
        app.key('Escape', {}, app.document.activeElement);
        let before = app.calls.length;
        app.key('c');
        await app.idle(before, 'get_compare');
        app.open('translation');
        await clickAndWait(app, app.document.querySelector('#compare-with-choices [data-value="bsb"]'), 'get_compare');
        assert.deepEqual(app.compareColumns(), ['World English Bible (WEB)', 'Berean Standard Bible (BSB)']);
        assert.equal(btn.textContent, 'WEB');
        assert.deepEqual(app.consoleErrors, []);
    }));

test('the translation button shows the remembered translation after a restart, and the one an import goes to', async () => {
    await withApp({ book: 'John', chapter: 3, translation: 'bsb' }, async (app) => {
        assert.equal(button(app).textContent, 'BSB');
        assert.equal(button(app).getAttribute('aria-label'), 'Translation: Berean Standard Bible');

        const path = join(dir, 'position.gospel-getter.json');
        writeFileSync(path, JSON.stringify({
            format: 'gospel-getter-reader-data',
            format_version: 1,
            exported_at: '2026-10-04T12:00:00.000Z',
            app_version: '2.16.3',
            reading_position: { translation: 'web', book: 45, book_name: 'Romans', chapter: 8 },
            bookmarks: [],
            preferences: {},
        }));
        await app.pick(path);
        app.open('menu');
        let before = app.calls.length;
        app.click($(app, 'import-data-btn'));
        await app.idle(before);
        before = app.calls.length;
        app.click($(app, 'import-confirm'));
        await app.idle(before, 'apply_import');
        assert.equal(app.heading(), 'Romans 8');
        assert.equal(button(app).textContent, 'WEB');
        assert.equal(button(app).getAttribute('aria-label'), 'Translation: World English Bible');
    });
});

test('T opens and closes it, focus going to the chosen translation and back to the button', () =>
    withApp({ book: 'John', chapter: 3, translation: 'bsb' }, async (app) => {
        const event = app.key('t');
        assert.ok(event.defaultPrevented);
        assert.ok(isOpen(app, 'translation'));
        assert.equal(button(app).getAttribute('aria-expanded'), 'true');
        assert.equal(app.document.activeElement, radio(app, 'bsb'));
        assert.equal(app.document.activeElement.getAttribute('aria-checked'), 'true');
        app.key('T', {}, app.document.activeElement);
        assert.ok(!isOpen(app, 'translation'));
        assert.equal(app.document.activeElement, button(app));

        // It's in the one table of shortcuts, so the sheet lists it.
        const row = app.window.gospelGetterShortcuts.find((s) => s.id === 'translation');
        assert.deepEqual([...row.handles], ['t', 'T']);
        assert.equal(row.aria, button(app).getAttribute('aria-keyshortcuts'));
        await app.key('?', { shiftKey: true });
        const sheetRow = app.document.querySelector('#shortcuts-panel .shortcut-row[data-shortcut="translation"]');
        assert.equal(sheetRow.textContent.replace(/\s+/g, ' ').trim(), 'T Open or close the translations and Compare');
        app.key('Escape', {}, app.document.activeElement);
    }));

test('T does nothing in search, in a field, or with a dialog open; in reading mode it works as the button shows', async () => {
    await withApp({}, async (app) => {
        app.key('/');
        app.key('t', {}, app.searchInput());
        assert.ok(!isOpen(app, 'translation'), 'typed into search');
        app.key('Escape', {}, app.searchInput());

        app.open('about');
        app.key('t', {}, app.document.activeElement);
        assert.ok(!isOpen(app, 'translation'), 'About is open');
        assert.ok(isOpen(app, 'about'));
        app.key('Escape', {}, app.document.activeElement);

        app.key('t', {}, $(app, 'update-check-toggle'));
        assert.ok(!isOpen(app, 'translation'), 'a checkbox');
    });
    await withApp({ readerMode: true }, async (app) => {
        assert.ok(shownOnScreen(app, button(app)), 'shown in reading mode, as Aa is');
        assert.ok(shownOnScreen(app, $(app, 'aa-toggle')));
        app.key('t');
        assert.ok(isOpen(app, 'translation'));
        app.key('Escape', {}, app.document.activeElement);
        assert.ok(!isOpen(app, 'translation'));
        assert.equal(app.document.documentElement.dataset.readerMode, 'on', 'the first Escape only closes it');
    });
});

test('Escape and a click outside close it, focus back to its button; it is one surface among the others', () =>
    withApp({}, async (app) => {
        app.click(button(app));
        assert.ok(isOpen(app, 'translation'));
        app.key('Escape', {}, app.document.activeElement);
        assert.ok(!isOpen(app, 'translation'));
        assert.equal(app.document.activeElement, button(app));

        app.click(button(app));
        app.click($(app, 'translation-label'));
        assert.ok(isOpen(app, 'translation'), 'a click inside keeps it open');
        app.click(app.document.querySelector('.chapter-main .chapter-heading'));
        assert.ok(!isOpen(app, 'translation'));
        assert.equal(app.document.activeElement, button(app));

        app.click(button(app));
        app.click($(app, 'aa-toggle'));
        assert.ok(!isOpen(app, 'translation') && isOpen(app, 'aa'));
        assert.equal(button(app).getAttribute('aria-expanded'), 'false');
        app.click(button(app));
        assert.ok(isOpen(app, 'translation') && !isOpen(app, 'aa'));
    }));

test('Aa keeps the display settings, named Display; the translations and Compare left it', async () => {
    await withApp({}, async (app) => {
        const aa = app.surface('aa');
        const popover = app.surface('translation');
        for (const id of ['translation-choices', 'translation-select', 'compare-btn', 'compare-with-choices']) {
            assert.ok(!aa.contains($(app, id)), `${id} isn't in Aa`);
            assert.ok(popover.contains($(app, id)), `${id} is in the translation popover`);
        }
        const toggle = $(app, 'aa-toggle');
        assert.equal(toggle.getAttribute('aria-label'), 'Display');
        assert.equal(toggle.title, 'Text and display');
        assert.equal(aa.getAttribute('aria-label'), 'Display');
        assert.equal(toggle.querySelector('.aa-label').textContent, 'Display');
        // The label shows only where the bar has room for it.
        const css = app.document.querySelector('style').textContent;
        assert.match(css, /@media \(max-width: 847px\) \{[^@]*\.aa-label \{\s*display: none;/);

        // Focus goes to Aa's first control.
        app.click(toggle);
        assert.equal(app.document.activeElement, $(app, 'text-size-down'));
    });
    // Or the next, when that one is disabled.
    await withApp({ storage: { 'gospel-getter-text-size': 'small' } }, async (app) => {
        app.click($(app, 'aa-toggle'));
        assert.equal(app.document.activeElement, $(app, 'text-size-up'));
    });
});

test('Export data… and Import data… say what they do on hover, and nothing more on screen', () =>
    withApp({}, async (app) => {
        const exportBtn = $(app, 'export-data-btn');
        const importBtn = $(app, 'import-data-btn');
        assert.equal(exportBtn.title, 'Save a backup of your bookmarks, highlights, notes and settings');
        assert.equal(importBtn.title, 'Load a Gospel Getter backup from this or another computer');
        assert.equal(exportBtn.textContent, 'Export data…');
        assert.equal(importBtn.textContent, 'Import data…');
    }));
