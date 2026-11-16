// R, reading mode on and off: the same as Aa's Reading mode button, focus
// and all; Esc still leaves it; typed in a field it's only a letter. With
// Single-key shortcuts off it does nothing (single-keys.test.mjs, which
// tries every character key in the table).

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { closeBridge, el, notePanelOpen, withApp } from './harness.mjs';

after(closeBridge);

const readerMode = (app) => app.document.documentElement.dataset.readerMode;
const button = (app) => el(app, 'reader-mode-btn');

test('R turns reading mode on and R again turns it off, focus going as the button sends it', () =>
    withApp({}, async (app) => {
        assert.equal(readerMode(app), undefined);
        let event = await app.key('r');
        assert.ok(event.defaultPrevented);
        assert.equal(readerMode(app), 'on');
        assert.equal(button(app).getAttribute('aria-pressed'), 'true');
        assert.equal(app.document.activeElement, el(app, 'reader-exit'));
        assert.equal(app.storage()['gospel-getter-reader-mode'], 'on');

        // Caps Lock's R, from where focus now is.
        event = await app.key('R', {}, app.document.activeElement);
        assert.ok(event.defaultPrevented);
        assert.equal(readerMode(app), undefined);
        assert.equal(button(app).getAttribute('aria-pressed'), 'false');
        assert.equal(app.document.activeElement, el(app, 'aa-toggle'), 'back to Aa, which holds the toggle');
    }));

test('Esc still leaves reading mode that R turned on; Ctrl+R and Alt+R are no shortcut', () =>
    withApp({}, async (app) => {
        await app.key('r');
        assert.equal(readerMode(app), 'on');
        await app.key('Escape', {}, app.document.activeElement);
        assert.equal(readerMode(app), undefined);
        assert.equal(app.document.activeElement, el(app, 'aa-toggle'));

        for (const opts of [{ ctrlKey: true }, { metaKey: true }, { altKey: true }]) {
            const event = await app.key('r', opts);
            assert.equal(event.defaultPrevented, false, JSON.stringify(opts));
            assert.equal(readerMode(app), undefined, JSON.stringify(opts));
        }
    }));

test('R typed in the search box or the note editor is a letter, not reading mode', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await app.key('/');
        let event = await app.key('r', {}, app.searchInput());
        assert.equal(event.defaultPrevented, false, 'it types');
        assert.equal(readerMode(app), undefined);
        await app.key('Escape', {}, app.searchInput());
        assert.equal(app.searchPanel().hidden, true);

        const before = app.calls.length;
        app.click(app.verse(16));
        await app.idle(before, 'list_notes');
        await app.key('n');
        assert.ok(notePanelOpen(app));
        const input = el(app, 'note-input');
        assert.equal(app.document.activeElement, input);
        event = await app.key('r', {}, input);
        assert.equal(event.defaultPrevented, false, 'it types');
        assert.equal(readerMode(app), undefined);
        assert.ok(notePanelOpen(app));
        await app.key('Escape', {}, input);
        assert.ok(!notePanelOpen(app));
    }));

test('the Reading mode button shows its key as Compare does, its name still “Reading mode”', () =>
    withApp({}, async (app) => {
        const btn = button(app);
        assert.equal(btn.textContent, 'Reading mode (R)');
        assert.equal(btn.querySelector('span[aria-hidden="true"]').textContent, '(R)');
        assert.match(btn.title, /\(R\)$/);
        const compare = el(app, 'compare-btn');
        assert.equal(compare.querySelector('span[aria-hidden="true"]').textContent, '(C)');
        assert.match(compare.title, /\(C\)$/);
        // The exit keeps Esc.
        assert.equal(el(app, 'reader-exit').textContent, 'Exit reading mode (Esc)');
    }));
