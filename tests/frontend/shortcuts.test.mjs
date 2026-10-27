// Behavior tests for the keyboard shortcuts sheet: ? (never search), the
// ☰ menu's and the palette's way in, Escape, the focus trap, and that it
// lists the one table of shortcuts the palette's key hints come from.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { closeBridge, openApp } from './harness.mjs';

after(closeBridge);

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const overlay = (app) => app.document.getElementById('shortcuts-overlay');
const isOpen = (app) => !overlay(app).hidden;
const panel = (app) => app.document.getElementById('shortcuts-panel');

test('? opens the sheet, not search; Escape closes it, focus going back', () =>
    withApp({}, async (app) => {
        const toggle = app.document.getElementById('search-toggle');
        toggle.focus();
        const event = await app.key('?', { shiftKey: true }, toggle);
        assert.ok(event.defaultPrevented, '? is never typed anywhere');
        assert.ok(isOpen(app));
        assert.equal(app.searchPanel().hidden, true, 'not search');
        assert.equal(panel(app).getAttribute('role'), 'dialog');
        assert.equal(panel(app).getAttribute('aria-modal'), 'true');
        assert.equal(app.document.getElementById(panel(app).getAttribute('aria-labelledby')).textContent, 'Keyboard shortcuts');
        assert.equal(app.document.activeElement, app.document.getElementById('shortcuts-close'));

        const esc = await app.key('Escape', {}, app.document.activeElement);
        assert.ok(esc.defaultPrevented);
        assert.ok(!isOpen(app));
        assert.equal(app.document.activeElement, toggle);

        // / still opens search.
        app.key('/');
        assert.equal(app.searchPanel().hidden, false);
        assert.ok(!isOpen(app));
    }));

test('the sheet lists every shortcut in its groups, with key caps', () =>
    withApp({}, async (app) => {
        await app.key('?', { shiftKey: true });
        const groups = [...panel(app).querySelectorAll('.shortcut-group')];
        assert.deepEqual(groups.map((g) => g.querySelector('h3').textContent), ['Reading', 'Selection', 'Search & palette', 'Panels', 'Text & display', 'History']);
        for (const g of groups) assert.equal(app.document.getElementById(g.getAttribute('aria-labelledby')), g.querySelector('h3'));
        const table = app.window.gospelGetterShortcuts;
        const rows = [...panel(app).querySelectorAll('.shortcut-row')];
        assert.deepEqual(rows.map((r) => r.dataset.shortcut), Array.from(table, (s) => s.id));
        const row = (id) => rows.find((r) => r.dataset.shortcut === id);
        const text = (id) => row(id).textContent.replace(/\s+/g, ' ').trim();
        assert.equal(text('next-verse'), 'j Select the next verse');
        assert.equal(text('extend-down'), 'Shift+j Extend the selection down a verse');
        assert.equal(text('search'), '/ or Ctrl+K Search, go to a reference, or run a command');
        assert.equal(text('back'), 'Alt+Left arrow Back to the passage you jumped from');
        assert.equal(text('previous-book'), '[ Previous book, at chapter 1');
        assert.equal(text('shortcuts'), '? These keyboard shortcuts');
        // Key caps, the hint's: drawn arrows named for screen readers.
        assert.deepEqual([...row('copy-as').querySelectorAll('kbd')].map((k) => k.textContent), ['Ctrl', 'Shift', 'C']);
        assert.equal(row('next-chapter').querySelector('kbd svg').getAttribute('aria-hidden'), 'true');
        assert.equal(row('next-chapter').querySelector('kbd .visually-hidden').textContent, 'Right arrow');
        assert.ok(rows.every((r) => r.querySelector('dt kbd') && r.querySelector('dd').textContent));
    }));

test('the palette’s key hints come from the same table', () =>
    withApp({}, async (app) => {
        app.key('/');
        app.type(app.searchInput(), '>');
        await app.idle(0);
        const table = app.window.gospelGetterShortcuts;
        const options = [...app.document.querySelectorAll('#search-results .search-command[aria-keyshortcuts]')];
        assert.ok(options.length >= 12);
        for (const o of options) {
            const row = table.find((s) => s.aria === o.getAttribute('aria-keyshortcuts'));
            assert.ok(row, o.textContent);
            assert.equal(o.querySelector('kbd').textContent, row.keys[0].join('+'));
        }
    }));

test('Tab stays in the sheet, and the reader’s keys wait while it’s open', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await app.key('?', { shiftKey: true });
        const close = app.document.getElementById('shortcuts-close');
        const body = app.document.getElementById('shortcuts-body');
        assert.equal(body.tabIndex, 0, 'the list scrolls by keyboard');
        app.key('Tab', {}, close);
        assert.equal(app.document.activeElement, body);
        app.key('Tab', {}, body);
        assert.equal(app.document.activeElement, close);
        app.key('Tab', { shiftKey: true }, close);
        assert.equal(app.document.activeElement, body);

        for (const key of ['j', 'c', '/', ']', 'ArrowRight']) app.key(key, {}, body);
        await app.idle(0);
        assert.deepEqual(app.selectedVerseNumbers(), []);
        assert.ok(!app.isComparing());
        assert.equal(app.searchPanel().hidden, true);
        assert.equal(app.heading(), 'John 3');
        assert.ok(isOpen(app));

        await app.click(close);
        assert.ok(!isOpen(app));
    }));

test('☰ → Keyboard shortcuts opens it, and closing returns to ☰; so does the palette’s command', () =>
    withApp({}, async (app) => {
        app.open('menu');
        const item = app.document.getElementById('shortcuts-btn');
        assert.equal(item.getAttribute('role'), 'menuitem');
        assert.equal(item.getAttribute('aria-keyshortcuts'), '?');
        await app.click(item);
        assert.ok(isOpen(app));
        assert.ok(app.surface('menu').hidden, 'the menu closes');
        app.key('Escape', {}, app.document.activeElement);
        assert.equal(app.document.activeElement, app.document.getElementById('menu-toggle'));

        app.key('/');
        app.type(app.searchInput(), 'keyboard');
        await app.idle(0);
        app.key('Enter', {}, app.searchInput());
        await app.idle(0);
        assert.ok(isOpen(app));
        assert.equal(app.searchPanel().hidden, true);
        // A click on the dimmed page closes it.
        await app.click(overlay(app));
        assert.ok(!isOpen(app));
    }));
