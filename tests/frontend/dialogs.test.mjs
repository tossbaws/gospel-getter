// Behavior tests for the overlays' one list (ui-leptos/src/overlays.rs):
// every overlay the page has is on it; while any modal on it is open the
// reader behind it takes no key, mouse button or text-size key; and one
// Escape closes each modal and does nothing else.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { backend, closeBridge, el, select, waitFor, withApp } from './harness.mjs';

const dir = mkdtempSync(join(tmpdir(), 'gospel-getter-dialogs-'));
after(async () => {
    rmSync(dir, { recursive: true, force: true });
    await closeBridge();
});

// Copied out of the page's realm, so they compare as this one's arrays.
const overlays = (app) => JSON.parse(JSON.stringify(app.window.gospelGetterOverlays));
const modals = (app) => overlays(app).filter((o) => o.modal).map((o) => o.id);
const isOpen = (app, id) => !el(app, id).hidden;
const mouseButton = (app, button) => {
    const event = new app.window.MouseEvent('mouseup', { button, bubbles: true, cancelable: true });
    app.window.dispatchEvent(event);
    return event;
};
const readings = (app, since) => app.calls.slice(since).filter((c) => c.cmd === 'get_reading').length;

test('every overlay the page has is on the one list, and every one listed is on the page', () =>
    withApp({}, async (app) => {
        const listed = new Set(overlays(app).map((o) => o.id));
        assert.ok(listed.size >= 8, JSON.stringify(overlays(app)));
        for (const id of listed) assert.ok(el(app, id), `#${id} is on the page`);
        const inListed = (node) => [...listed].some((id) => el(app, id).contains(node));
        // An overlay is anything named one, and any dialog that isn't a
        // header popover (those are `surfaces`', not overlays).
        const found = [
            ...app.document.querySelectorAll('[id$="-overlay"], [class*="overlay"], [aria-modal="true"], [role="dialog"]:not(.popover)'),
        ];
        assert.ok(found.length >= 8);
        for (const node of found) {
            assert.ok(inListed(node), `${node.id ? `#${node.id}` : node.className} is in an overlay on the list`);
        }
        assert.deepEqual(overlays(app).filter((o) => !o.modal).map((o) => o.id), ['search-panel'], 'only the search box isn’t modal');
    }));

// Each modal, opened the way a reader would: what it's opened from (where
// focus goes back to) and how.
const OPENERS = {
    'about-overlay': async (app) => {
        app.open('about');
        return el(app, 'menu-toggle');
    },
    'shortcuts-overlay': async (app) => {
        const from = el(app, 'ref-btn');
        from.focus();
        await app.key('?', { shiftKey: true }, from);
        return from;
    },
    'welcome-overlay': async (app) => {
        app.open('menu');
        await app.click(el(app, 'welcome-btn'));
        return el(app, 'menu-toggle');
    },
    'export-choice-overlay': async (app) => {
        await app.openExport();
        return el(app, 'menu-toggle');
    },
    'export-notes-overlay': async (app) => {
        await app.openExport();
        const before = app.calls.length;
        app.click(el(app, 'export-choice-notes'));
        await app.idle(before, 'count_notes_export');
        return el(app, 'menu-toggle');
    },
    'import-overlay': async (app) => {
        const path = join(dir, 'backup.gospel-getter.json');
        await app.pick(path);
        await app.exportEverything();
        await app.pick(path);
        app.open('menu');
        const before = app.calls.length;
        app.click(el(app, 'import-data-btn'));
        await app.idle(before);
        return el(app, 'menu-toggle');
    },
    'note-overlay': async (app) => {
        const from = el(app, 'ref-btn');
        from.focus();
        await app.key('n', {}, from);
        return from;
    },
};

test('one Escape closes each modal and nothing else: focus goes back, the selection and the chapter stay', async () => {
    await backend('__reset', { bookId: 43, chapter: 3, translationCode: 'kjv' });
    await backend('add_note', { bookId: 43, chapter: 3, verseStart: 17, verseEnd: 17, body: 'Not to condemn.' });
    await withApp({ book: 'John', chapter: 3, translation: 'kjv', keepBookmarks: true }, async (app) => {
        await select(app, 16);
        for (const id of Object.keys(OPENERS)) {
            const heading = app.heading();
            const opening = app.calls.length;
            const opened = await OPENERS[id](app);
            await app.until(() => isOpen(app, id), `#${id} to open`);
            await app.idle(opening);
            const before = app.calls.length;
            const esc = await app.key('Escape', {}, app.document.activeElement);
            await app.idle(before);
            assert.ok(esc.defaultPrevented, `${id}: Escape is taken`);
            assert.ok(!isOpen(app, id), `${id}: one Escape closes it`);
            assert.equal(app.document.activeElement, opened, `${id}: focus goes back to ${opened.id}`);
            assert.deepEqual(app.selectedVerseNumbers(), [16], `${id}: the selection stays`);
            assert.equal(app.heading(), heading, `${id}: the chapter stays`);
            assert.equal(readings(app, before), 0, `${id}: nothing is read`);
            assert.equal(app.window.gospelGetterSurfaces.current(), null, `${id}: nothing else is open`);
        }
        assert.deepEqual(Object.keys(OPENERS).sort(), modals(app).sort(), 'every modal on the list was walked');
    });
});

test('E-05: one Escape on About closes About and keeps the verse selection', () =>
    withApp({}, async (app) => {
        await select(app, 16);
        app.open('about');
        assert.ok(isOpen(app, 'about-overlay'));
        await app.key('Escape', {}, el(app, 'about-close'));
        assert.ok(!isOpen(app, 'about-overlay'));
        assert.deepEqual(app.selectedVerseNumbers(), [16]);
    }));

test('E-05: Ctrl+= and Ctrl+- don’t change the text size behind About', () =>
    withApp({}, async (app) => {
        const root = app.document.documentElement;
        const size = root.dataset.textSize;
        app.open('about');
        for (const key of ['=', '-', '0']) {
            await app.key(key, { ctrlKey: true }, el(app, 'about-close'));
            assert.equal(root.dataset.textSize, size, `Ctrl+${key}`);
        }
        assert.ok(isOpen(app, 'about-overlay'));
        await app.key('Escape', {}, el(app, 'about-close'));
        // Closed, the keys work again.
        await app.key('=', { ctrlKey: true }, app.document.body);
        assert.notEqual(root.dataset.textSize, size);
    }));

test('E-06: with the welcome open, a mouse’s Back and Forward leave the chapter behind it alone', async () => {
    await backend('__reset', { bookId: 19, chapter: 23, translationCode: 'kjv' });
    await backend('add_note', { bookId: 43, chapter: 3, verseStart: 16, verseEnd: 16, body: 'Loved.' });
    await withApp({ book: 'Psalms', chapter: 23, translation: 'kjv', keepBookmarks: true }, async (app) => {
        // John 3:16 from its note's Library row (a jump), so there's
        // somewhere to go back to.
        app.openLibrary('notes');
        await app.idle(0);
        const row = [...app.document.querySelectorAll('#note-list .note-item')]
            .find((r) => r.textContent.includes('John 3:16'));
        await waitFor(app, () => app.click(row.querySelector('.bookmark-open')));
        assert.equal(app.heading(), 'John 3');
        app.open('menu');
        await app.click(el(app, 'welcome-btn'));
        assert.ok(isOpen(app, 'welcome-overlay'));
        for (const button of [3, 4]) {
            const before = app.calls.length;
            const event = mouseButton(app, button);
            await app.idle(before);
            assert.ok(event.defaultPrevented, 'the webview doesn’t go back either');
            assert.equal(readings(app, before), 0, `button ${button} reads nothing`);
            assert.equal(app.heading(), 'John 3');
            assert.ok(isOpen(app, 'welcome-overlay'));
        }
        // And no reader shortcut reaches past it.
        for (const key of ['ArrowLeft', 'ArrowRight']) {
            const before = app.calls.length;
            await app.key(key, {}, app.document.activeElement);
            await app.idle(before);
            assert.equal(readings(app, before), 0, key);
        }
        assert.equal(app.heading(), 'John 3');
        // Closed, Back goes back again.
        await app.key('Escape', {}, app.document.activeElement);
        assert.ok(!isOpen(app, 'welcome-overlay'));
        await waitFor(app, () => mouseButton(app, 3));
        assert.equal(app.heading(), 'Psalms 23');
    });
});
