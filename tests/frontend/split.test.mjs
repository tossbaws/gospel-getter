// Behavior tests for two passages side by side (ui-leptos/src/pane.rs),
// reached for now only through `window.gospelGetterPanes`: each pane's
// chapter read from the backend in its own translation, the keys and the
// header acting on the active pane, a press making a pane active, marks
// and the Library shared by both, each pane's own history, Open beside
// again replacing the second pane (S12) and leaving it active (S13),
// Compare paused (S14), the second pane's place never saved (S15), ×
// putting the page back exactly as it was, and reading mode showing the
// active pane alone (D6). The tabs on a narrow window are
// split-narrow.test.mjs's.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, closeBridge, settle, shownVerse, withApp } from './harness.mjs';

after(closeBridge);

const ROMANS = bookId('Romans');
const JOHN = bookId('John');

const panes = (app) => app.window.gospelGetterPanes;
const inner = (app, n) => app.document.getElementById(n === 1 ? 'reading-pane-inner' : 'reading-pane-2-inner');
const root = (app, n) => app.document.getElementById(n === 1 ? 'reading-pane' : 'reading-pane-2');
const heading = (app, n) => root(app, n).querySelector('.chapter-heading').textContent;
const column = (app, name) => app.document.querySelector(`.pane[data-pane="${name}"]`);
const bar = (app, name) => column(app, name).querySelector('.pane-bar');
const verseIn = (app, n, verse) => root(app, n).querySelector(`.chapter-main .verse[data-translation][data-verse="${verse}"]`);
const marked = (app, n, cls) => [...root(app, n).querySelectorAll(`.chapter-main .verse[data-translation].${cls}`)]
    .map((v) => Number(v.dataset.verse));
// The reference button's label: where the active pane is.
const place = (app) => app.document.getElementById('ref-btn').getAttribute('aria-label');
const readings = (app, since = 0) => app.calls.slice(since).filter((c) => c.cmd === 'get_reading')
    .map((c) => JSON.parse(JSON.stringify(c.args)));

/** Opens `args` beside, through the test global, and waits for the backend. */
async function openBeside(app, args) {
    const before = app.calls.length;
    const loaded = await panes(app).openBeside(args);
    await app.idle(before);
    return loaded;
}

/** A pointer pressed in `el` (D8), then its click. */
async function press(app, el) {
    el.dispatchEvent(new app.window.MouseEvent('pointerdown', { bubbles: true, cancelable: true, button: 0 }));
    await app.click(el);
}

/** A mouse's whole press of `el`, as the webview sends it: pointerdown, mousedown, pointerup, mouseup, click. */
async function pressFully(app, el) {
    const init = { bubbles: true, cancelable: true, button: 0 };
    const Pointer = app.window.PointerEvent ?? app.window.MouseEvent;
    el.dispatchEvent(new Pointer('pointerdown', init));
    el.dispatchEvent(new app.window.MouseEvent('mousedown', init));
    el.dispatchEvent(new Pointer('pointerup', init));
    el.dispatchEvent(new app.window.MouseEvent('mouseup', init));
    el.dispatchEvent(new app.window.MouseEvent('click', init));
    await app.idle();
}

test('two panes: each chapter is its own get_reading, in its own translation, through the same markup', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        assert.deepEqual([...panes(app).shown()], ['first']);
        assert.ok(app.document.querySelector('#reading-pane .chapter-side'), 'one pane has its faded neighbors');
        const before = app.calls.length;
        assert.equal(await openBeside(app, { bookId: ROMANS, chapter: 5, translation: 'bsb' }), true);
        assert.deepEqual(readings(app, before), [{ bookId: ROMANS, chapter: 5, translationCode: 'bsb' }]);
        assert.deepEqual([...panes(app).shown()], ['first', 'second']);
        assert.equal(app.document.documentElement.dataset.split, 'on');
        assert.equal(heading(app, 1), 'John 3');
        assert.equal(heading(app, 2), 'Romans 5');
        assert.equal(inner(app, 2).dataset.currentBook, String(ROMANS));
        assert.ok([...root(app, 1).querySelectorAll('.verse[data-translation]')].every((v) => v.dataset.translation === 'kjv'));
        assert.ok([...root(app, 2).querySelectorAll('.verse[data-translation]')].every((v) => v.dataset.translation === 'bsb'));
        // The backend's words, as one pane shows them.
        for (const [n, code, book, chapter, verse] of [[2, 'bsb', 'Romans', 5, 1], [1, 'kjv', 'John', 3, 16]]) {
            const shown = await shownVerse(code, book, chapter, verse);
            const words = verseIn(app, n, verse).querySelector('.verse-text').cloneNode(true);
            for (const marker of words.querySelectorAll('.fn-marker')) marker.remove();
            assert.equal(words.textContent, shown.text.trim());
        }
        // The pane's own ids, and no faded neighbors while split (D7).
        assert.ok(app.document.getElementById('reader-notice-2'));
        assert.equal(app.document.querySelectorAll('.chapter-side').length, 0);
        // Each bar names its pane's place and translation; the top bar's
        // controls are copies, with no ids of their own.
        assert.match(bar(app, 'first').querySelector('.ref-text').textContent, /^John 3/);
        assert.match(bar(app, 'second').querySelector('.ref-text').textContent, /^Romans 5/);
        assert.equal(bar(app, 'first').querySelector('.translation-toggle').textContent, 'KJV');
        assert.equal(bar(app, 'second').querySelector('.translation-toggle').textContent, 'BSB');
        assert.equal(bar(app, 'first').querySelectorAll('[id]').length, 0);
        assert.equal(bar(app, 'second').querySelectorAll('.pane-close').length, 1);
        assert.equal(bar(app, 'first').querySelectorAll('.pane-close').length, 0);
        const ids = [...app.document.querySelectorAll('[id]')].map((el) => el.id);
        assert.equal(new Set(ids).size, ids.length, 'every id once');
    }));

test('the second pane is active once opened beside (S13); a press in a pane makes it active, with its heading rule and the reference', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await openBeside(app, { bookId: ROMANS, chapter: 5, translation: 'bsb' });
        assert.equal(panes(app).active(), 'second');
        assert.equal(column(app, 'second').dataset.paneActive, 'true');
        assert.equal(column(app, 'first').dataset.paneActive, 'false');
        assert.match(place(app), /^Romans 5,/);
        assert.equal(app.document.getElementById('translation-select').value, 'bsb');
        assert.equal(app.document.getElementById('translation-toggle').textContent, 'BSB');

        // A press in the first pane.
        const before = app.calls.length;
        await press(app, verseIn(app, 1, 3));
        await app.idle(before);
        assert.equal(panes(app).active(), 'first');
        assert.equal(column(app, 'first').dataset.paneActive, 'true');
        assert.equal(column(app, 'second').dataset.paneActive, 'false');
        assert.match(place(app), /^John 3,/);
        // The record says the active pane's translation; neither pane is
        // read again for it.
        assert.equal(app.document.getElementById('translation-select').value, 'kjv');
        assert.deepEqual(readings(app, before), []);

        // A click from the keyboard (no press) is for its pane too.
        await app.click(verseIn(app, 2, 1));
        assert.equal(panes(app).active(), 'second');
        assert.match(place(app), /^Romans 5,/);
        // One selection at a time (D3).
        assert.deepEqual(marked(app, 2, 'verse-selected'), [1]);
        await press(app, verseIn(app, 1, 3));
        assert.deepEqual(marked(app, 2, 'verse-selected'), []);
        assert.deepEqual(marked(app, 1, 'verse-selected'), [3]);
    }));

test('← and →, G, T and N act on the active pane only', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await openBeside(app, { bookId: ROMANS, chapter: 5, translation: 'bsb' });
        let before = app.calls.length;
        app.key('ArrowRight');
        await app.idle(before, 'get_reading');
        assert.deepEqual(readings(app, before), [{ bookId: ROMANS, chapter: 6, translationCode: 'bsb' }]);
        assert.equal(heading(app, 2), 'Romans 6');
        assert.equal(heading(app, 1), 'John 3');

        await press(app, verseIn(app, 1, 1));
        before = app.calls.length;
        app.key('ArrowLeft');
        await app.idle(before, 'get_reading');
        assert.deepEqual(readings(app, before), [{ bookId: JOHN, chapter: 2, translationCode: 'kjv' }]);
        assert.equal(heading(app, 1), 'John 2');
        assert.equal(heading(app, 2), 'Romans 6');

        // T: the translation popover, under the active pane's own button.
        await press(app, verseIn(app, 2, 1));
        await app.key('t');
        const popover = app.surface('translation');
        assert.equal(popover.hidden, false);
        assert.ok(bar(app, 'second').contains(popover));
        assert.equal(bar(app, 'second').querySelector('.translation-toggle').getAttribute('aria-expanded'), 'true');
        // Choosing a translation there reads the active pane in it.
        before = app.calls.length;
        app.click(popover.querySelector('#translation-choices [data-value="web"]'));
        await app.idle(before, 'get_reading');
        assert.deepEqual(readings(app, before), [{ bookId: ROMANS, chapter: 6, translationCode: 'web' }]);
        assert.equal(bar(app, 'second').querySelector('.translation-toggle').textContent, 'WEB');
        assert.equal(bar(app, 'first').querySelector('.translation-toggle').textContent, 'KJV');
        await app.key('Escape');
        assert.equal(popover.hidden, true);
        assert.equal(popover.parentElement, app.document.getElementById('translation-toggle').parentElement, 'back in the top bar');

        // G: the picker marks the active pane's chapter.
        await app.key('g');
        const current = () => app.document.querySelector('#picker .book-item[aria-current]');
        assert.match(current().textContent, /Romans/);
        await app.key('Escape');
        await press(app, verseIn(app, 1, 1));
        assert.match(current().textContent, /John/);

        // N: a note on the active pane's selection.
        await press(app, verseIn(app, 2, 4));
        await app.key('n');
        assert.equal(app.document.getElementById('note-overlay').hidden, false);
        assert.match(app.document.getElementById('note-overlay').textContent, /Romans 6:4/);
    }));

test('a bookmark or highlight made in the second pane marks the first pane’s same verse, and reaches the Library', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await openBeside(app, { bookId: JOHN, chapter: 3, translation: 'web' });
        await app.click(verseIn(app, 2, 16));
        let before = app.calls.length;
        app.click(app.bookmarkButton());
        await app.idle(before, 'list_bookmarks');
        assert.deepEqual(marked(app, 2, 'is-bookmarked'), [16]);
        assert.deepEqual(marked(app, 1, 'is-bookmarked'), [16]);
        assert.ok(app.bookmarkItems().some((b) => b.reference.startsWith('John 3:16')));

        before = app.calls.length;
        app.click(app.actions().querySelector('.highlight-swatch[data-color="yellow"]'));
        await app.idle(before, 'set_highlight');
        assert.equal(verseIn(app, 2, 16).dataset.highlight, 'yellow');
        assert.equal(verseIn(app, 1, 16).dataset.highlight, 'yellow');
    }));

test('Back in the second pane leaves the first where it is; Open beside again replaces the second pane, whose Back returns (S12)', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await openBeside(app, { bookId: ROMANS, chapter: 5 });
        // A second Open beside: a jump in the second pane, the first
        // untouched, the second still active.
        await press(app, verseIn(app, 1, 1));
        await openBeside(app, { bookId: ROMANS, chapter: 8, verse: 28 });
        assert.equal(heading(app, 2), 'Romans 8');
        assert.equal(heading(app, 1), 'John 3');
        assert.equal(panes(app).active(), 'second');
        assert.deepEqual(marked(app, 2, 'verse-selected'), [28]);
        assert.equal(app.document.getElementById('history-back').getAttribute('aria-disabled'), null);

        let before = app.calls.length;
        app.key('ArrowLeft', { altKey: true });
        await app.idle(before, 'get_reading');
        assert.equal(heading(app, 2), 'Romans 5');
        assert.equal(heading(app, 1), 'John 3');
        assert.deepEqual(readings(app, before).map((r) => r.bookId), [ROMANS]);

        // The first pane's history is its own: nothing behind it.
        await press(app, verseIn(app, 1, 1));
        assert.equal(app.document.getElementById('history-back').getAttribute('aria-disabled'), 'true');
        before = app.calls.length;
        app.key('ArrowLeft', { altKey: true });
        await app.idle(before);
        assert.deepEqual(readings(app, before), []);
        assert.equal(heading(app, 2), 'Romans 5');
    }));

test('Compare is paused while two panes are open, says why, and comes back as the second closes (S14)', () =>
    withApp({ book: 'John', chapter: 3, compare: true }, async (app) => {
        assert.ok(app.isComparing());
        await openBeside(app, { bookId: ROMANS, chapter: 5 });
        assert.equal(app.isComparing(), false);
        assert.equal(app.document.querySelectorAll('.compare-grid, .compare-head').length, 0);
        const before = app.calls.length;
        await app.key('c');
        assert.equal(app.toast().textContent, 'Compare shows one passage at a time; close the second pane first.');
        app.toast().textContent = '';
        await app.click(app.document.getElementById('compare-btn'));
        assert.equal(app.toast().textContent, 'Compare shows one passage at a time; close the second pane first.');
        assert.deepEqual(app.calls.slice(before).filter((c) => c.cmd === 'get_compare'), []);
        assert.equal(app.window.localStorage.getItem('gospel-getter-compare'), 'on', 'the setting is left alone');
        assert.equal(app.document.getElementById('compare-btn').getAttribute('aria-pressed'), 'true');

        await app.click(bar(app, 'second').querySelector('.pane-close'));
        await app.idle(before);
        assert.ok(app.isComparing());
        assert.equal(app.document.documentElement.dataset.split, undefined);
    }));

test('the second pane’s place is never saved (S15)', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        const saves = () => app.calls.filter((c) => c.cmd === 'save_reading_position').map((c) => c.args.bookId);
        await openBeside(app, { bookId: ROMANS, chapter: 5 });
        app.key('ArrowRight');
        await app.idle(0, 'get_reading');
        column(app, 'second').dispatchEvent(new app.window.Event('scroll'));
        await new Promise((r) => setTimeout(r, 500));
        await app.idle();
        // The first pane's still is, from its own column.
        column(app, 'first').dispatchEvent(new app.window.Event('scroll'));
        app.window.dispatchEvent(new app.window.Event('blur'));
        await app.idle();
        assert.ok(saves().includes(JOHN));
        assert.ok(!saves().includes(ROMANS), `saved: ${saves()}`);
        assert.equal(app.window.gospelGetterPlace.saved().bookId, JOHN);
    }));

test('× puts the page back exactly as it was, the first pane active and its reference button focused', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        const html = app.document.documentElement;
        // The page as the reader has it: the reading view and the top bar,
        // whose popovers' contents are drawn as they open (and Aa's again
        // as anything in it changes), so they're left out.
        // Attributes in name order: one taken off and put back again (the
        // picker's current book) is the same attribute.
        const canonical = (el, empty = '') => {
            const copy = el.cloneNode(true);
            if (empty) for (const inside of copy.querySelectorAll(empty)) inside.replaceChildren();
            for (const node of [copy, ...copy.querySelectorAll('*')]) {
                const attributes = [...node.attributes].map((a) => [a.name, a.value]).sort();
                for (const [name] of attributes) node.removeAttribute(name);
                for (const [name, value] of attributes) node.setAttribute(name, value);
            }
            return copy.outerHTML;
        };
        const snapshot = () => ({
            attributes: [...html.attributes].map((a) => `${a.name}=${a.value}`).sort(),
            main: canonical(app.document.querySelector('main')),
            topBar: canonical(app.document.getElementById('top-bar'), '.popover'),
            picker: canonical(app.document.getElementById('picker')),
            title: app.document.title,
            select: app.document.getElementById('translation-select').value,
        });
        const was = snapshot();
        await openBeside(app, { bookId: ROMANS, chapter: 5, translation: 'bsb' });
        await app.click(verseIn(app, 2, 2));
        let before = app.calls.length;
        app.key('ArrowRight');
        await app.idle(before, 'get_reading');
        before = app.calls.length;
        await app.click(bar(app, 'second').querySelector('.pane-close'));
        await app.idle(before);
        assert.deepEqual([...panes(app).shown()], ['first']);
        assert.equal(panes(app).active(), 'first');
        assert.equal(app.document.activeElement, app.document.getElementById('ref-btn'));
        assert.deepEqual(snapshot(), was);
        assert.equal(app.document.getElementById('reading-pane-2'), null);

        // And it opens again, fresh.
        await openBeside(app, { bookId: ROMANS, chapter: 12 });
        assert.equal(heading(app, 2), 'Romans 12');
        assert.equal(app.document.getElementById('history-back').getAttribute('aria-disabled'), 'true');
        panes(app).close();
        await app.idle();
        assert.deepEqual(snapshot(), was);
    }));

test('pressing the second pane’s × keeps the first pane’s selection, and Copy as… for it (GG32-14)', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        await openBeside(app, { bookId: ROMANS, chapter: 5, translation: 'bsb' });
        await press(app, verseIn(app, 1, 16));
        assert.equal(panes(app).active(), 'first');
        assert.deepEqual(marked(app, 1, 'verse-selected'), [16]);
        const copyAs = () => app.document.querySelector('.verse-actions .verse-copy-as-btn');
        assert.ok(copyAs(), 'Copy as… is offered for it');

        await pressFully(app, bar(app, 'second').querySelector('.pane-close'));
        assert.deepEqual([...panes(app).shown()], ['first']);
        assert.equal(panes(app).active(), 'first');
        assert.deepEqual(marked(app, 1, 'verse-selected'), [16], 'the selection is still there');
        assert.ok(copyAs(), 'Copy as… is still offered');
        assert.equal(copyAs().textContent, 'Copy as…');

        // X, the key, likewise.
        await openBeside(app, { bookId: ROMANS, chapter: 5, translation: 'bsb' });
        await press(app, verseIn(app, 1, 16));
        await app.key('x');
        await app.idle();
        assert.deepEqual([...panes(app).shown()], ['first']);
        assert.deepEqual(marked(app, 1, 'verse-selected'), [16]);

        // Any other control in the second pane's bar still makes it active,
        // and the selection goes with the pane left (D3).
        await openBeside(app, { bookId: ROMANS, chapter: 5, translation: 'bsb' });
        await press(app, verseIn(app, 1, 16));
        assert.equal(panes(app).active(), 'first');
        await pressFully(app, bar(app, 'second').querySelector('.translation-toggle'));
        assert.equal(panes(app).active(), 'second');
        assert.deepEqual(marked(app, 1, 'verse-selected'), []);
    }));

test('reading mode shows the active pane alone, the top bar acting on it, and leaving it brings the split back (D6)', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        const html = app.document.documentElement;
        const seen = () => ({
            readingMode: html.dataset.readerMode === 'on',
            tabs: html.hasAttribute('data-pane-tabs'),
            hidden: ['first', 'second'].filter((name) => column(app, name).hidden),
            shown: [...panes(app).shown()],
            active: panes(app).active(),
        });
        await openBeside(app, { bookId: ROMANS, chapter: 5, translation: 'bsb' });
        await app.key('r');
        assert.deepEqual(seen(), {
            readingMode: true, tabs: false, hidden: ['first'], shown: ['first', 'second'], active: 'second',
        });
        assert.equal(app.document.querySelector('.pane-tabs').hidden, true, 'no tabs in reading mode');
        assert.equal(app.document.activeElement, app.document.getElementById('reader-exit'));
        assert.match(app.document.getElementById('ref-text').textContent, /^Romans 5/);
        // → turns the pane shown.
        app.document.activeElement.blur();
        let before = app.calls.length;
        app.key('ArrowRight');
        await app.idle(before, 'get_reading');
        assert.deepEqual(readings(app, before), [{ bookId: ROMANS, chapter: 6, translationCode: 'bsb' }]);
        assert.equal(heading(app, 1), 'John 3');

        // Escape leaves it: both panes again, the same one active.
        await app.key('Escape');
        assert.deepEqual(seen(), {
            readingMode: false, tabs: false, hidden: [], shown: ['first', 'second'], active: 'second',
        });

        // On a narrow window, reading mode is still the active pane alone,
        // not the tabs; leaving it, the tabs come back.
        await press(app, verseIn(app, 1, 1));
        app.resize(700);
        await app.key('Escape');
        assert.equal(seen().tabs, true);
        await app.key('r');
        assert.deepEqual(seen(), {
            readingMode: true, tabs: false, hidden: ['second'], shown: ['first', 'second'], active: 'first',
        });
        assert.equal(app.document.querySelector('.pane-tabs').hidden, true);
        await app.key('r');
        assert.deepEqual(seen(), {
            readingMode: false, tabs: true, hidden: ['second'], shown: ['first', 'second'], active: 'first',
        });
        assert.equal(app.document.querySelector('.pane-tabs').hidden, false);

        // Reading mode from an import: the same.
        app.resize(1280);
        before = app.calls.length;
        app.window.gospelGetterPreferences.apply({ reading_mode: true });
        assert.deepEqual(seen(), {
            readingMode: true, tabs: false, hidden: ['second'], shown: ['first', 'second'], active: 'first',
        });
        app.window.gospelGetterPreferences.apply({ reading_mode: false });
        assert.deepEqual(seen().hidden, []);
        await app.idle(before);
    }));

test('opened beside in reading mode, both panes show (B12); R shows the second alone; × brings the first back as it was', () =>
    withApp({ book: 'John', chapter: 3, readerMode: true }, async (app) => {
        await openBeside(app, { bookId: ROMANS, chapter: 5 });
        assert.equal(app.document.documentElement.dataset.readerMode, undefined);
        assert.equal(panes(app).active(), 'second');
        assert.equal(column(app, 'first').hidden, false);
        assert.equal(column(app, 'second').hidden, false);
        app.key('r');
        await settle();
        assert.equal(app.document.documentElement.dataset.readerMode, 'on');
        assert.equal(column(app, 'first').hidden, true);
        assert.equal(column(app, 'second').hidden, false);
        const before = app.calls.length;
        panes(app).close();
        await app.idle(before);
        assert.deepEqual([...panes(app).shown()], ['first']);
        assert.equal(app.document.documentElement.dataset.readerMode, 'on');
        assert.equal(app.document.getElementById('reading-pane').closest('.pane'), null);
        assert.equal(heading(app, 1), 'John 3');
    }));

test('printing while split fills the Cross references list of the pane that prints, the active one (D14)', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        await openBeside(app, { bookId: ROMANS, chapter: 5 });
        const list = (id) => app.document.getElementById(id);
        assert.equal(list('xref-summary-list').childNodes.length, 0);
        assert.equal(list('xref-summary-list-2').childNodes.length, 0);
        app.window.dispatchEvent(new app.window.Event('beforeprint'));
        assert.ok(list('xref-summary-list-2').querySelector('.xref-summary-row'), 'the second pane’s, which prints');
        assert.equal(list('xref-summary-list').childNodes.length, 0, 'not the first’s, which doesn’t');
        assert.equal(list('xref-summary-list-2').hidden, true, 'still closed on screen');

        await press(app, verseIn(app, 1, 1));
        app.window.dispatchEvent(new app.window.Event('beforeprint'));
        assert.ok(list('xref-summary-list').querySelector('.xref-summary-row'));
    }));

// D-07: a Back from the second pane still on its way as the pane closes
// never ends a step taken after it opens again, however many steps match
// its count: the history's token goes on across the close.
test('a late Back from a closed second pane is dropped, never ending a newer step of the pane opened again', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        const back = () => app.key('ArrowLeft', { altKey: true });
        const forward = () => app.document.getElementById('history-forward');
        await openBeside(app, { bookId: ROMANS, chapter: 5 });
        await openBeside(app, { bookId: ROMANS, chapter: 8 });
        assert.equal(heading(app, 2), 'Romans 8');
        // Back, its answer held; the pane closes meanwhile.
        const late = app.holdAnswer('get_reading');
        let before = app.calls.length;
        await back();
        await app.until(() => app.calls.length > before, 'the Back’s reading');
        panes(app).close();
        // Opened again: as many steps as before, the last one's answer held too.
        await openBeside(app, { bookId: JOHN, chapter: 1 });
        await openBeside(app, { bookId: JOHN, chapter: 2 });
        assert.equal(heading(app, 2), 'John 2');
        const newer = app.holdAnswer('get_reading');
        before = app.calls.length;
        await back();
        await app.until(() => app.calls.length > before, 'the newer Back’s reading');
        // The late answer comes first, then the newer one.
        late();
        await settle();
        newer();
        await app.idle();
        assert.equal(heading(app, 2), 'John 1');
        assert.equal(panes(app).active(), 'second');
        // Back on John 1, where the pane's history began: Forward goes to
        // John 2, and there's nothing earlier.
        assert.equal(forward().getAttribute('aria-disabled'), null, 'Forward to John 2');
        assert.equal(app.document.getElementById('history-back').getAttribute('aria-disabled'), 'true');
        before = app.calls.length;
        await app.key('ArrowRight', { altKey: true });
        await app.idle(before);
        assert.equal(heading(app, 2), 'John 2');
    }));

// D-06: a Library note's Edit opens the editor on the passage its jump put
// on screen, in the pane that jumped, even if the reader switched panes
// while the chapter was on its way.
test('a note’s Edit, its jump landing after a switch of pane, quotes the passage the jump put on screen', async () => {
    await backend('__reset', { bookId: bookId('Psalms'), chapter: 23, translationCode: 'kjv' });
    await backend('add_note', { bookId: JOHN, chapter: 3, verseStart: 16, verseEnd: 16, body: 'Loved.' });
    await withApp({ book: 'Psalms', chapter: 23, translation: 'kjv', keepBookmarks: true }, async (app) => {
        await openBeside(app, { bookId: ROMANS, chapter: 5 });
        assert.equal(panes(app).active(), 'second');
        app.openLibrary('notes');
        await app.idle(0);
        const row = [...app.document.querySelectorAll('#note-list .note-item')].find((r) => r.textContent.includes('John 3:16'));
        const land = app.holdAnswer('get_reading');
        app.click(row.querySelector('.note-list-edit'));
        await settle();
        await app.key('F6');
        assert.equal(panes(app).active(), 'first');
        land();
        await app.until(() => !app.document.getElementById('note-overlay').hidden, 'the note editor');
        await settle();
        assert.equal(heading(app, 2), 'John 3');
        assert.equal(app.document.getElementById('note-title').textContent, 'John 3:16');
        assert.match(app.document.querySelector('.note-passage').textContent, /For God so loved the world/);
        assert.equal(app.document.getElementById('note-input').value, 'Loved.');
    });
});
