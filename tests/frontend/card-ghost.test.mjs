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

// A pointer event as the webview sends it, with the modifier state it carries.
function pointer(app, type, target, opts = {}) {
    const Pointer = app.window.PointerEvent || app.window.MouseEvent;
    target.dispatchEvent(new Pointer(type, { bubbles: true, cancelable: true, button: 0, ...opts }));
}

// A Shift-click as the webview sends it: pointerdown, mousedown, pointerup,
// mouseup and click, each saying Shift is held. `detail` 1: a click with
// the mouse. Resolves once the click is done (the page's tasks after it
// have run), as it is before the hand moves on.
async function shiftClick(app, target) {
    const init = { bubbles: true, cancelable: true, button: 0, detail: 1, shiftKey: true };
    pointer(app, 'pointerdown', target, init);
    target.dispatchEvent(new app.window.MouseEvent('mousedown', init));
    pointer(app, 'pointerup', target, init);
    target.dispatchEvent(new app.window.MouseEvent('mouseup', init));
    target.dispatchEvent(new app.window.MouseEvent('click', init));
    await later(app);
}

const solid = (app) => {
    const style = app.window.getComputedStyle(card(app));
    return !ghost(app) && style.pointerEvents !== 'none' && Number(style.opacity) === 1;
};

function later(app) {
    return new Promise((resolve) => app.window.setTimeout(resolve, 0));
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

// The Captain's sequence with Shift's keyup never reaching the page (as on
// Wayland with an input method), in each way of reading: the next pointer
// move, saying Shift isn't held, ends the ghost.
for (const [mode, options] of [
    ['normal', {}],
    ['Compare', { compare: true }],
    ['Reading mode', { readerMode: true }],
    ['Flowing', { storage: { 'gospel-getter-layout': 'flowing' } }],
]) {
    test(`${mode}: with no Shift keyup at all, the next pointer move without Shift ends the ghost`, () =>
        withApp({ book: 'John', chapter: 3, translation: 'web', ...options }, async (app) => {
            app.click(app.verse(16, 'web'));
            app.key('Shift', { shiftKey: true, code: 'ShiftLeft' });
            await shiftClick(app, app.verse(17, 'web'));
            await shiftClick(app, app.verse(18, 'web'));
            assert.deepEqual(app.selectedVerseNumbers('web'), [16, 17, 18]);
            assert.ok(ghost(app), 'a ghost while Shift is held');
            // Shift is let go and its keyup never comes; the mouse moves,
            // its event saying Shift isn't held.
            pointer(app, 'pointermove', app.verse(18, 'web'), { shiftKey: false });
            assert.ok(solid(app), 'the card comes back: no ghost, opacity 1, clickable');
            const button = card(app).querySelector('.verse-copy-btn');
            assert.notEqual(app.window.getComputedStyle(button).pointerEvents, 'none', 'its controls can be reached');
            await app.copyButton();
            assert.ok(app.clipboardWrites.at(-1).endsWith('— John 3:16–18 (WEB)'), app.clipboardWrites.at(-1));
            assert.deepEqual(app.selectedVerseNumbers('web'), [16, 17, 18]);
        }));
}

test('with no keyup, the wheel, a button going down or window focus without Shift end the ghost too', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(16));
        for (const end of ['wheel', 'pointerdown', 'focus']) {
            app.key('Shift', { shiftKey: true, code: 'ShiftLeft' });
            assert.ok(ghost(app));
            if (end === 'wheel') {
                const Wheel = app.window.WheelEvent || app.window.MouseEvent;
                app.document.body.dispatchEvent(new Wheel('wheel', { bubbles: true, shiftKey: false }));
            } else if (end === 'pointerdown') {
                pointer(app, 'pointerdown', app.document.body, { shiftKey: false });
                pointer(app, 'pointerup', app.document.body, { shiftKey: false });
                await later(app);
            } else {
                app.window.dispatchEvent(new app.window.Event('focus'));
            }
            assert.ok(solid(app), `${end} ends it`);
        }
    }));

test('Shift still held when the window gets focus back is picked up by the next event that says so', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(16));
        app.key('Shift', { shiftKey: true, code: 'ShiftLeft' });
        assert.ok(ghost(app));
        app.window.dispatchEvent(new app.window.Event('focus'));
        assert.ok(solid(app), 'focus back lets go of what was held');
        // Shift is still down: the next pointer move says so.
        pointer(app, 'pointermove', app.verse(17), { shiftKey: true });
        assert.ok(ghost(app), 'a pointer move with Shift makes it a ghost again');
        await shiftClick(app, app.verse(18));
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17, 18]);
        assert.ok(ghost(app));
        // And a key with Shift does too (Shift+J extends the selection).
        app.window.dispatchEvent(new app.window.Event('focus'));
        assert.ok(solid(app));
        app.key('J', { shiftKey: true, code: 'KeyJ' });
        assert.ok(ghost(app), 'a key with Shift makes it a ghost again');
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17, 18, 19]);
        app.document.body.dispatchEvent(new app.window.KeyboardEvent('keyup',
            { key: 'Shift', code: 'ShiftLeft', shiftKey: false, bubbles: true }));
        assert.ok(solid(app));
    }));

// A still mouse on a ghost no keyup ended: the press is the first event to
// say Shift isn't held, but the browser has already hit-tested it through
// the ghost, so it and its click land on the verse beneath. jsdom lays
// nothing out, so the card is given a box, and the events go where the
// webview would send them.
function stillClick(app, target, x, y) {
    const init = { bubbles: true, cancelable: true, button: 0, detail: 1, shiftKey: false, clientX: x, clientY: y };
    pointer(app, 'pointerdown', target, init);
    target.dispatchEvent(new app.window.MouseEvent('mousedown', init));
    pointer(app, 'pointerup', target, init);
    target.dispatchEvent(new app.window.MouseEvent('mouseup', init));
    const click = new app.window.MouseEvent('click', init);
    target.dispatchEvent(click);
    return click;
}

const CARD_BOX = { left: 100, top: 200, right: 500, bottom: 400, width: 400, height: 200, x: 100, y: 200 };

test('a still click inside a ghost no keyup ended is the card\'s: dropped, the selection kept, the card itself for the next', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(16));
        app.key('Shift', { shiftKey: true, code: 'ShiftLeft' });
        assert.ok(ghost(app), 'a ghost, and no keyup will come');
        card(app).getBoundingClientRect = () => CARD_BOX;
        const click = stillClick(app, app.verse(17), 300, 300);
        assert.ok(click.defaultPrevented, 'the click is dropped');
        assert.deepEqual(app.selectedVerseNumbers(), [16], 'the selection is as it was');
        await later(app);
        assert.ok(solid(app), 'the card is itself for the next click');
        await app.copyButton();
        assert.ok(app.clipboardWrites.at(-1).endsWith('— John 3:16 (WEB)'), app.clipboardWrites.at(-1));
        // Only that one click: the next on a verse selects it.
        app.click(app.verse(18), { detail: 1 });
        assert.deepEqual(app.selectedVerseNumbers(), [18]);
    }));

test('a still click outside the card on a ghost no keyup ended goes to the verse as usual', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(16));
        app.key('Shift', { shiftKey: true, code: 'ShiftLeft' });
        card(app).getBoundingClientRect = () => CARD_BOX;
        const click = stillClick(app, app.verse(20), 50, 50);
        assert.ok(!click.defaultPrevented);
        assert.deepEqual(app.selectedVerseNumbers(), [20], 'a plain click on verse 20');
        await later(app);
        assert.ok(solid(app));
    }));

test('with both Shifts held, letting go of one leaves the card a ghost; letting go of the other ends it', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(16));
        app.key('Shift', { shiftKey: true, code: 'ShiftLeft' });
        app.key('Shift', { shiftKey: true, code: 'ShiftRight' });
        assert.ok(ghost(app));
        // The left comes up; the right still holds Shift (shiftKey true).
        app.document.body.dispatchEvent(new app.window.KeyboardEvent('keyup',
            { key: 'Shift', code: 'ShiftLeft', shiftKey: true, bubbles: true }));
        assert.ok(ghost(app), 'still a ghost: the right Shift is down');
        await shiftClick(app, app.verse(18));
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17, 18]);
        assert.ok(ghost(app));
        app.document.body.dispatchEvent(new app.window.KeyboardEvent('keyup',
            { key: 'Shift', code: 'ShiftRight', shiftKey: false, bubbles: true }));
        assert.ok(solid(app), 'both let go: the card is back');
    }));

test('a card opened by a Shift-click while Shift is held is a ghost at once', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        assert.ok(!card(app), 'nothing selected yet: no card');
        app.key('Shift', { shiftKey: true, code: 'ShiftLeft' });
        await shiftClick(app, app.verse(16));
        assert.ok(card(app).isConnected, 'the Shift-click opened the card');
        assert.ok(ghost(app), 'a ghost at once');
        assert.equal(app.window.getComputedStyle(card(app)).pointerEvents, 'none', 'the next Shift-click goes through it');
        await shiftClick(app, app.verse(18));
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17, 18]);
        assert.ok(ghost(app));
        app.document.body.dispatchEvent(new app.window.KeyboardEvent('keyup',
            { key: 'Shift', code: 'ShiftLeft', shiftKey: false, bubbles: true }));
        assert.ok(solid(app));
    }));

test('Shift let go before the mouse button comes up: the Shift-click still extends the selection, which stays', () =>
    withApp({ book: 'John', chapter: 3, translation: 'web' }, async (app) => {
        app.click(app.verse(16));
        app.key('Shift', { shiftKey: true, code: 'ShiftLeft' });
        assert.ok(ghost(app));
        const target = app.verse(18);
        const down = { bubbles: true, cancelable: true, button: 0, detail: 1, shiftKey: true };
        pointer(app, 'pointerdown', target, down);
        target.dispatchEvent(new app.window.MouseEvent('mousedown', down));
        // Shift comes up while the button is still down.
        app.document.body.dispatchEvent(new app.window.KeyboardEvent('keyup',
            { key: 'Shift', code: 'ShiftLeft', shiftKey: false, bubbles: true }));
        assert.equal(app.window.getComputedStyle(card(app)).pointerEvents, 'none',
            'the card still lets the button-up through to the verse it was pressed on');
        // The button comes up on that verse, the events no longer saying Shift.
        const up = { ...down, shiftKey: false };
        pointer(app, 'pointerup', target, up);
        target.dispatchEvent(new app.window.MouseEvent('mouseup', up));
        target.dispatchEvent(new app.window.MouseEvent('click', up));
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17, 18], 'the Shift-click extended the selection');
        await later(app);
        assert.ok(solid(app), 'once the click is done, the card is back');
        assert.deepEqual(app.selectedVerseNumbers(), [16, 17, 18], 'and the selection stays');
        assert.equal(card(app).previousElementSibling, app.verse(18));

        // The next plain click is a plain click again.
        app.click(app.verse(17));
        assert.deepEqual(app.selectedVerseNumbers(), [17]);
    }));
