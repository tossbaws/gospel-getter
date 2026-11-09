// Aa's "Single-key shortcuts" (GG-16, WCAG 2.1.4 Character Key Shortcuts):
// on by default; off, every shortcut that's a character key pressed on its
// own does nothing, while those with Ctrl, Cmd or Alt, the arrows and
// Escape still work, and the shortcuts sheet and the palette are still
// reached (☰, Ctrl+K). Kept on this computer only, like the margin's
// switch: it survives a restart, and no backup carries it.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { closeBridge, openApp } from './harness.mjs';

const dir = mkdtempSync(join(tmpdir(), 'gospel-getter-single-keys-'));
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

const KEY = 'gospel-getter-single-key-shortcuts';
const OFF_TEXT = 'Single-key shortcuts are off, so the keys here that are pressed on their own, such as G, J, / and ?, do nothing. The ones with Ctrl or Alt, the arrow keys and Esc still work. To turn them back on, open Aa and switch on Single-key shortcuts.';

const el = (app, id) => app.document.getElementById(id);
const toggle = (app) => el(app, 'single-keys-toggle');
const sheetOpen = (app) => !el(app, 'shortcuts-overlay').hidden;

/** Aa → Single-key shortcuts, switched to `on`, then Aa closed. */
async function setSingleKeys(app, on) {
    app.open('aa');
    if (toggle(app).checked !== on) toggle(app).click();
    assert.equal(toggle(app).checked, on);
    await app.key('Escape', {}, app.document.activeElement);
    assert.ok(app.surface('aa').hidden);
    app.document.activeElement?.blur();
}

/**
 * The key presses one way of pressing a shortcut (a combination of the
 * table's key caps) makes: a letter in either case (with Shift, its
 * capital), an arrow or Esc by its key's name.
 */
function pressesOf(caps) {
    const ctrlKey = caps.includes('Ctrl');
    const altKey = caps.includes('Alt');
    const shiftKey = caps.includes('Shift');
    const cap = caps.find((c) => !['Ctrl', 'Alt', 'Shift'].includes(c));
    const named = { '←': 'ArrowLeft', '→': 'ArrowRight', '↑': 'ArrowUp', '↓': 'ArrowDown', Esc: 'Escape' }[cap];
    const keys = named ? [named] : shiftKey ? [cap.toUpperCase()] : [...new Set([cap.toLowerCase(), cap.toUpperCase()])];
    return keys.map((key) => ({ key, ctrlKey, altKey, shiftKey }));
}

const characterOnly = (press) => !press.ctrlKey && !press.altKey && [...press.key].length === 1;

/** What a shortcut could change: where the reader is, what's open, focus, the backend's calls. */
const seen = (app) => ({
    heading: app.heading(),
    selected: app.selectedVerseNumbers(),
    comparing: app.isComparing(),
    surface: app.window.gospelGetterSurfaces.current(),
    search: !app.searchPanel().hidden,
    sheet: sheetOpen(app),
    editor: !el(app, 'note-overlay').hidden,
    cardMenu: !!app.document.querySelector('#copy-as-menu:not([hidden])'),
    focus: app.document.activeElement,
    scrollY: app.window.scrollY,
    calls: app.calls.length,
});

test('Aa’s Single-key shortcuts: a switch, on by default, saved as it changes', () =>
    withApp({}, async (app) => {
        app.open('aa');
        const sw = toggle(app);
        assert.equal(sw.type, 'checkbox');
        assert.equal(sw.getAttribute('role'), 'switch');
        assert.equal(sw.checked, true, 'on by default');
        assert.equal(sw.closest('label').textContent.trim(), 'Single-key shortcuts');
        const note = el(app, sw.getAttribute('aria-describedby'));
        assert.match(note.textContent, /G, \/ and \? work pressed on their own/);
        assert.match(note.textContent, /speech input or a screen reader/);
        assert.match(note.textContent, /Ctrl\+K/);
        assert.equal(sw.closest('.aa-section').querySelector('.section-label').textContent, 'Keyboard');

        sw.click();
        assert.equal(app.storage()[KEY], 'off');
        assert.equal(app.document.documentElement.dataset.singleKeyShortcuts, 'off');
        sw.click();
        assert.equal(app.storage()[KEY], 'on');
        assert.equal(app.document.documentElement.dataset.singleKeyShortcuts, undefined, 'no attribute for the default');
    }));

test('off, no shortcut in the table that’s a character key pressed on its own does anything', () =>
    withApp({}, async (app) => {
        // A verse selected, so the selection's keys (j, k, N, C) would act.
        const before = app.calls.length;
        app.click(app.verse(16));
        await app.idle(before, 'list_notes');
        await setSingleKeys(app, false);
        await app.idle();

        const ignored = [];
        for (const row of app.window.gospelGetterShortcuts) {
            for (const caps of row.keys) {
                for (const press of pressesOf(caps).filter(characterOnly)) {
                    const was = seen(app);
                    const event = await app.key(press.key, press);
                    await app.idle();
                    assert.equal(event.defaultPrevented, false, `${row.id}: ${press.key} isn’t stopped, so it types where typing is possible`);
                    assert.deepEqual(seen(app), was, `${row.id}: ${press.key} does nothing`);
                    ignored.push(press.key);
                }
            }
        }
        for (const key of ['g', 'G', 't', 'c', 'n', 'j', 'k', 'J', 'K', '[', ']', '/', '?']) {
            assert.ok(ignored.includes(key), `the table's ways to press include ${key}`);
        }

        // On again, they're shortcuts again.
        await setSingleKeys(app, true);
        const event = await app.key('?', { shiftKey: true });
        assert.ok(event.defaultPrevented);
        assert.ok(sheetOpen(app));
    }));

test('off, the shortcuts with Ctrl or Alt, the arrows and Escape still work', () =>
    withApp({}, async (app) => {
        await setSingleKeys(app, false);
        // Ctrl+K: the palette (the search box), and Escape closes it.
        let event = await app.key('k', { ctrlKey: true });
        assert.ok(event.defaultPrevented);
        assert.equal(app.searchPanel().hidden, false);
        assert.equal(app.document.activeElement, app.searchInput());
        // A character typed in it is typed, not a shortcut.
        app.searchInput().value = '';
        event = await app.key('g', {}, app.searchInput());
        assert.equal(event.defaultPrevented, false);
        await app.key('Escape', {}, app.searchInput());
        assert.equal(app.searchPanel().hidden, true);
        // Cmd+K too.
        await app.key('k', { metaKey: true });
        assert.equal(app.searchPanel().hidden, false);
        await app.key('Escape', {}, app.searchInput());

        // Ctrl+B, the Library.
        await app.key('b', { ctrlKey: true });
        assert.equal(app.window.gospelGetterSurfaces.current(), 'library');
        await app.key('Escape', {}, app.document.activeElement);
        assert.equal(app.window.gospelGetterSurfaces.current(), null);
        app.document.activeElement?.blur();

        // → turns the chapter, Alt+← goes back to where a jump came from.
        await app.key('ArrowRight');
        await app.waitForHeading('John 4');
        await app.key('k', { ctrlKey: true });
        await app.search('Romans 8');
        app.key('Enter', {}, app.searchInput());
        await app.waitForHeading('Romans 8');
        await app.idle();
        await app.key('ArrowLeft', { altKey: true });
        await app.waitForHeading('John 4');
    }));

test('off, the sheet says so and how to turn them back on, reached from ☰ and from the palette', () =>
    withApp({}, async (app) => {
        await setSingleKeys(app, false);
        const notice = () => el(app, 'shortcuts-off');

        app.open('menu');
        await app.click(el(app, 'shortcuts-btn'));
        assert.ok(sheetOpen(app), '☰ → Keyboard shortcuts opens it');
        assert.equal(notice().hidden, false);
        assert.equal(notice().textContent.trim(), OFF_TEXT);
        const body = el(app, 'shortcuts-body');
        assert.equal(body.firstElementChild, notice(), 'said first');
        await app.key('Escape', {}, app.document.activeElement);
        assert.ok(!sheetOpen(app));

        // The palette's command, by Ctrl+K.
        await app.key('k', { ctrlKey: true });
        app.type(app.searchInput(), 'keyboard');
        await app.idle(0);
        app.key('Enter', {}, app.searchInput());
        await app.idle(0);
        assert.ok(sheetOpen(app), 'the palette’s Keyboard shortcuts opens it');
        assert.equal(notice().hidden, false);
        await app.key('Escape', {}, app.document.activeElement);

        // On, the sheet says nothing of it.
        await setSingleKeys(app, true);
        await app.key('?', { shiftKey: true });
        assert.ok(sheetOpen(app));
        assert.equal(notice().hidden, true);
    }));

test('the switch is this computer’s: it survives a restart, and no backup carries it', async () => {
    let storage;
    const path = join(dir, 'backup.gospel-getter.json');
    await withApp({}, async (app) => {
        await setSingleKeys(app, false);
        storage = app.storage();
        assert.equal(storage[KEY], 'off');
        assert.ok(!('single_key_shortcuts' in app.window.gospelGetterPreferences.current()));
        await app.pick(path);
        await app.exportEverything();
    });
    const json = JSON.parse(readFileSync(path, 'utf8'));
    assert.ok(json.preferences, 'the backup has display settings');
    assert.deepEqual(Object.keys(json.preferences).filter((k) => /single|key/i.test(k)), [], JSON.stringify(json.preferences));
    assert.ok(!readFileSync(path, 'utf8').includes(KEY));

    await withApp({ storage }, async (app) => {
        assert.equal(app.document.documentElement.dataset.singleKeyShortcuts, 'off', 'applied before the first frame');
        app.open('aa');
        assert.equal(toggle(app).checked, false, 'still off');
        await app.key('Escape', {}, app.document.activeElement);
        app.document.activeElement?.blur();
        const event = await app.key('?', { shiftKey: true });
        assert.equal(event.defaultPrevented, false);
        assert.ok(!sheetOpen(app), '? still does nothing');
    });
});
