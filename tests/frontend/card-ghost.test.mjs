// Holding Shift turns the selection card into a ghost (faint, and clicks
// go through it), so a Shift-click reaches a verse the card covers and
// extends the selection. Shift is followed from the keyboard, and the
// ghost ends on keyup, blur and a hidden window. It never shows while
// Shift is for something else: a field, the palette, a dialog, Copy as…,
// or focus in the card.

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

const card = (app) => app.document.getElementById('selection-card');
const ghost = (app) => card(app).classList.contains('is-ghost');

function shiftDown(app, target = app.document.body) {
    return app.key('Shift', { shiftKey: true }, target);
}

function shiftUp(app, target = app.document.body) {
    target.dispatchEvent(new app.window.KeyboardEvent('keyup', { key: 'Shift', shiftKey: false, bubbles: true }));
}

test('Shift makes the card a ghost that clicks go through; letting go makes it solid', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(16));
        assert.ok(card(app).isConnected, 'the card is open');
        assert.ok(!ghost(app), 'solid to start with');

        shiftDown(app);
        assert.ok(ghost(app), 'Shift ghosts it');
        const style = app.window.getComputedStyle(card(app));
        assert.equal(style.pointerEvents, 'none', 'clicks go through the card');
        assert.equal(Number(style.opacity), 0.18);
        const button = card(app).querySelector('.verse-copy-btn');
        assert.equal(app.window.getComputedStyle(button).pointerEvents, 'none', 'and through everything in it');
        // Only visual: still in the accessibility tree.
        assert.ok(!card(app).hidden);
        assert.equal(card(app).getAttribute('aria-hidden'), null);

        shiftUp(app);
        assert.ok(!ghost(app), 'letting go of Shift makes it solid');
        assert.notEqual(app.window.getComputedStyle(card(app)).pointerEvents, 'none');
    }));

test('a Shift-click on a verse under the ghost extends the selection, and the card stays a ghost until Shift is let go', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(16));
        shiftDown(app);
        assert.equal(app.window.getComputedStyle(card(app)).pointerEvents, 'none');
        // In the real webview the click lands on the verse under the card
        // because of pointer-events: none; here it's sent to that verse.
        app.click(app.verse(18), { shiftKey: true });
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17, 18]);
        assert.equal(app.document.querySelector('.verse-actions-ref').textContent, 'John 3:16–18 (WEB)');
        assert.equal(card(app).previousElementSibling, app.verse(18), 'the card moved under the new range');
        assert.ok(ghost(app), 'still a ghost while Shift is held');
        shiftUp(app);
        assert.ok(!ghost(app));
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17, 18]);
    }));

test('the ghost ends when the window loses focus or is hidden, even with no keyup', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(16));
        shiftDown(app);
        assert.ok(ghost(app));
        app.window.dispatchEvent(new app.window.Event('blur'));
        assert.ok(!ghost(app), 'blur makes it solid');

        shiftDown(app);
        assert.ok(ghost(app));
        Object.defineProperty(app.document, 'hidden', { value: true, configurable: true });
        app.document.dispatchEvent(new app.window.Event('visibilitychange'));
        assert.ok(!ghost(app), 'a hidden window makes it solid');
        Object.defineProperty(app.document, 'hidden', { value: false, configurable: true });

        // A keyup of anything without Shift held also ends it (the Shift
        // keyup itself went to another window).
        shiftDown(app);
        app.document.body.dispatchEvent(new app.window.KeyboardEvent('keyup', { key: 'a', shiftKey: false, bubbles: true }));
        assert.ok(!ghost(app));
    }));

test('no ghost while typing in the note editor', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(16));
        app.key('n');
        const editor = app.document.querySelector('#note-overlay textarea');
        assert.ok(!app.document.getElementById('note-overlay').hidden, 'the note editor is open');
        editor.focus();
        shiftDown(app, editor);
        assert.ok(!ghost(app));
    }));

test('no ghost while the palette or search is open', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(16));
        app.key('/');
        assert.ok(!app.searchPanel().hidden);
        app.searchInput().focus();
        shiftDown(app, app.searchInput());
        assert.ok(!ghost(app));
    }));

test('no ghost with Copy as… open, from its button or from Ctrl+Shift+C', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(16));
        app.click(card(app).querySelector('.verse-copy-as-btn'));
        const menu = app.document.getElementById('copy-as-menu');
        assert.ok(!menu.hidden, 'Copy as… is open');
        shiftDown(app, app.document.activeElement);
        assert.ok(!ghost(app));
        app.key('Escape', {}, app.document.activeElement);
        assert.ok(menu.hidden);
        shiftUp(app);

        // Ctrl+Shift+C: Shift is down as it opens, and the ghost goes as
        // focus moves into the menu.
        app.key('Shift', { shiftKey: true });
        app.key('C', { ctrlKey: true, shiftKey: true });
        assert.ok(!menu.hidden);
        assert.ok(!ghost(app));
    }));

test('no ghost with focus in the card, so Shift+Tab moves through it', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(16));
        const bookmark = card(app).querySelector('.verse-bookmark-btn');
        bookmark.focus();
        shiftDown(app, bookmark);
        app.key('Tab', { shiftKey: true }, bookmark);
        assert.ok(!ghost(app));
        assert.notEqual(app.window.getComputedStyle(bookmark).pointerEvents, 'none');
    }));

test('no ghost once Shift+? opens the shortcuts sheet', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(16));
        shiftDown(app);
        assert.ok(ghost(app));
        await app.key('?', { shiftKey: true });
        assert.ok(!app.document.getElementById('shortcuts-overlay').hidden);
        assert.ok(!ghost(app), 'the sheet took focus, so the card is solid again');
    }));

test('Shift+j still extends the selection (the card is a ghost while Shift is down)', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(16));
        shiftDown(app);
        app.key('J', { shiftKey: true });
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17]);
        assert.ok(ghost(app));
        shiftUp(app);
        assert.ok(!ghost(app));
    }));

test('in compare, either column; and in Flowing', async () => {
    await withApp({ book: 'John', chapter: 3, compare: true }, async (app) => {
        for (const code of ['kjv', 'web']) {
            app.click(app.verse(16, code));
            shiftDown(app);
            assert.ok(ghost(app), `the ${code} column's card`);
            app.click(app.verse(18, code), { shiftKey: true });
            assert.deepEqual(app.selectedVerseNumbers(code), [16, 17, 18]);
            shiftUp(app);
            assert.ok(!ghost(app));
            app.key('Escape');
        }
    });
    await withApp({ book: 'John', chapter: 3, translation: 'web', storage: { 'gospel-getter-layout': 'flowing' } }, async (app) => {
        assert.equal(app.document.documentElement.dataset.layout, 'flowing');
        app.click(app.verse(16));
        shiftDown(app);
        app.click(app.verse(18), { shiftKey: true });
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17, 18]);
        assert.ok(ghost(app));
    });
});

test('in every theme the ghost changes only opacity and pointer-events, never the layout', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        const root = app.document.documentElement;
        const themes = [...app.document.querySelectorAll('#theme-select option')].map((o) => o.value);
        assert.ok(themes.length >= 9, `${themes.length} themes`);
        app.click(app.verse(16));
        const layout = ['position', 'top', 'left', 'right', 'bottom', 'width', 'height', 'max-height', 'margin',
            'padding', 'border-width', 'transform', 'display', 'visibility', 'inset', 'z-index'];
        for (const theme of themes) {
            root.dataset.theme = theme;
            const box = (s) => Object.fromEntries(layout.map((p) => [p, s.getPropertyValue(p)]));
            const solid = box(app.window.getComputedStyle(card(app)));
            shiftDown(app);
            assert.ok(ghost(app), theme);
            assert.deepEqual(box(app.window.getComputedStyle(card(app))), solid, `${theme}: no layout change`);
            assert.equal(Number(app.window.getComputedStyle(card(app)).opacity), 0.18, theme);
            shiftUp(app);
        }
        // The stylesheet's ghost rules set nothing else, in any theme.
        const css = app.document.querySelector('style').textContent;
        const rules = [...css.matchAll(/([^{}]*\.is-ghost[^{}]*)\{([^}]*)\}/g)];
        assert.ok(rules.length >= 2);
        for (const [, , body] of rules) {
            const props = body.split(';').map((d) => d.split(':')[0].trim()).filter(Boolean);
            for (const p of props) assert.ok(['opacity', 'pointer-events'].includes(p), `the ghost sets only opacity and pointer-events, not ${p}`);
        }
    }));

test('the fade is quick, and only for those who haven\'t asked for less motion', () =>
    withApp({}, async (app) => {
        const css = app.document.querySelector('style').textContent;
        const blocks = css.match(/@media \(prefers-reduced-motion: no-preference\) \{([\s\S]*?)\n {8}\}/g);
        const motion = blocks.join('\n');
        assert.match(motion, /\.selection-card \{\s*transition: opacity 0\.1s ease;/);
        const rest = blocks.reduce((all, block) => all.replace(block, ''), css);
        assert.ok(!/\.selection-card[^{]*\{[^}]*transition/.test(rest), 'no fade outside the no-preference block');
    }));
