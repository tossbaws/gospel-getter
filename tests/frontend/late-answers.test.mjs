// Late answers never override a newer reader action (the 2026-11-08 review's
// "freshness narrower than intent", GG-01, GG-07, GG-09, GG-10, GG-11): each
// test holds one backend answer, does something newer while it's on its way,
// then lets it go, and checks that what the reader did last is what stays.
// The answers are held by the harness (holdAnswer, holdFailure,
// holdCommand), never by waiting, so each interleaving is the same every
// run.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, callsOf as switchCalls, closeBridge, el, notePanelOpen as editorOpen, openApp, savingOn, settle } from './harness.mjs';

after(closeBridge);

const JOHN = bookId('John');
const ROMANS = bookId('Romans');
const GENESIS = bookId('Genesis');

/**
 * Resets the reader's data, adds `notes` ([book, chapter, verse, body]) and
 * whatever `seed` adds, then opens John 3.
 */
async function withNotes(notes, body, { seed = async () => {}, ...options } = {}) {
    await backend('__reset', { bookId: JOHN, chapter: 3, translationCode: 'kjv' });
    const added = [];
    for (const [book, chapter, verse, text] of notes) {
        added.push(await backend('add_note', { bookId: book, chapter, verseStart: verse, verseEnd: verse, body: text }));
    }
    await seed();
    const app = await openApp({ book: 'John', chapter: 3, keepBookmarks: true, ...options });
    try {
        await body(app, added);
    } finally {
        app.close();
    }
}

// ---- GG-01: a note's Edit from the Library, waiting for its jump.

const editorTitle = (app) => el(app, 'note-title').textContent;
const toastText = (app) => (app.toast().classList.contains('is-hidden') ? '' : app.toast().textContent);
const STILL_WRITING = 'You’re still writing a note: save it or close it before opening another.';

/** The `get_reading` calls for `book`'s chapters, in order. */
const readings = (app, book) => app.calls.filter((c) => c.cmd === 'get_reading' && c.args.bookId === book);

/** The Library's Notes tab, and the Edit of the note with `id`: clicked, the jump asked for. */
function clickEdit(app, id) {
    app.openLibrary('notes');
    const edit = app.document.querySelector(`#note-list .note-item[data-note-id="${id}"] .note-list-edit`);
    assert.ok(edit, `the Library lists note ${id}`);
    app.click(edit);
    assert.ok(app.surface('library').hidden, 'the Library closes at once');
}

/** Verse `n` of the chapter on screen selected, and its card's + Add note: the editor open on it. */
async function addNoteOn(app, n) {
    const before = app.calls.length;
    app.click(app.verse(n));
    await app.idle(before, 'list_notes');
    app.click(app.actions().querySelector('.verse-note-btn'));
    await settle();
    assert.ok(editorOpen(app), `the editor opens on John 3:${n}`);
}

test('GG-01: a late Edit never replaces a newer note being written, and isn’t opened after it', () =>
    withNotes([[ROMANS, 8, 28, 'All things work together.']], async (app, [romans]) => {
        const release = app.holdAnswer('get_reading');
        clickEdit(app, romans.id);
        const jump = readings(app, ROMANS).at(-1);
        assert.ok(jump && !jump.done, 'Romans 8 is on its way');

        // Newer: a note on John 3:16, still on screen, typed in.
        await addNoteOn(app, 16);
        assert.equal(editorTitle(app), 'John 3:16');
        const typed = 'Typed while the jump was on its way.';
        app.type(el(app, 'note-input'), typed);
        await settle();

        const since = app.calls.length;
        release();
        await app.until(() => jump.done, 'the jump’s answer');
        await app.idle(since);

        assert.ok(editorOpen(app));
        assert.equal(editorTitle(app), 'John 3:16', 'the editor being typed in stays');
        assert.equal(el(app, 'note-input').value, typed, 'with every word typed');
        assert.notEqual(toastText(app), STILL_WRITING, 'the late Edit never even asked');
        // Closed, it was let go: nothing opens after.
        app.click(el(app, 'note-cancel'));
        app.click(el(app, 'note-discard-yes'));
        await app.idle(since);
        assert.ok(!editorOpen(app), 'the Edit isn’t waiting to open');
    }));

test('GG-01: an editor opened and closed while the jump was on its way lets the late Edit go', () =>
    withNotes([[ROMANS, 8, 28, 'All things work together.']], async (app, [romans]) => {
        const release = app.holdAnswer('get_reading');
        clickEdit(app, romans.id);
        const jump = readings(app, ROMANS).at(-1);
        await addNoteOn(app, 16);
        app.click(el(app, 'note-cancel'));
        assert.ok(!editorOpen(app));

        const since = app.calls.length;
        release();
        await app.until(() => jump.done, 'the jump’s answer');
        await app.idle(since);
        assert.equal(app.heading(), 'Romans 8', 'the jump still lands');
        assert.ok(!editorOpen(app), 'but the Edit, older than the editor opened since, doesn’t open');
    }));

test('GG-01: a later navigation lets a waiting Edit go', () =>
    withNotes([[ROMANS, 8, 28, 'All things work together.']], async (app, [romans]) => {
        const release = app.holdAnswer('get_reading');
        clickEdit(app, romans.id);
        const jump = readings(app, ROMANS).at(-1);

        // Newer: → to John 4, which isn't held.
        app.key('ArrowRight');
        await app.waitForHeading('John 4');

        const since = app.calls.length;
        release();
        await app.until(() => jump.done, 'the jump’s answer');
        await app.idle(since);
        assert.equal(app.heading(), 'John 4');
        assert.ok(!editorOpen(app), 'no editor on a note of a chapter the reader left');
    }));

test('GG-01: a jump that fails lets its Edit go', () =>
    withNotes([[ROMANS, 8, 28, 'All things work together.']], async (app, [romans]) => {
        const fail = app.holdFailure('get_reading', 'Romans 8 couldn’t be read.');
        clickEdit(app, romans.id);
        const jump = readings(app, ROMANS).at(-1);

        const since = app.calls.length;
        fail();
        await app.until(() => jump.done, 'the jump’s failure');
        await app.idle(since);
        assert.equal(app.heading(), 'John 3', 'still where the reader was');
        assert.match(app.notice(), /Romans 8 couldn’t be read\./);
        assert.ok(!editorOpen(app), 'no editor on a note whose verses aren’t there');
    }));

test('GG-01: a newer Edit supersedes an older one still on its way', () =>
    withNotes([
        [ROMANS, 8, 28, 'All things work together.'],
        [GENESIS, 1, 1, 'In the beginning.'],
    ], async (app, [romans, genesis]) => {
        const release = app.holdAnswer('get_reading');
        clickEdit(app, romans.id);
        const jump = readings(app, ROMANS).at(-1);

        // Newer: Genesis 1:1's Edit, which isn't held.
        let since = app.calls.length;
        clickEdit(app, genesis.id);
        await app.waitForHeading('Genesis 1');
        await app.until(() => editorOpen(app), 'Genesis 1:1’s editor');
        await app.idle(since);
        assert.equal(editorTitle(app), 'Genesis 1:1');

        since = app.calls.length;
        release();
        await app.until(() => jump.done, 'the older jump’s answer');
        await app.idle(since);
        assert.equal(app.heading(), 'Genesis 1');
        assert.equal(editorTitle(app), 'Genesis 1:1', 'the newer Edit’s note stays open');
        assert.equal(el(app, 'note-input').value, 'In the beginning.');
    }));

test('GG-01: an open editor’s unsaved text survives another note being asked to open', () =>
    withNotes([], async (app) => {
        await addNoteOn(app, 16);
        const typed = 'Not saved yet, and not kept as a draft yet either.';
        app.type(el(app, 'note-input'), typed);
        await settle();

        // Whatever asks (here the card's + Add note, under the editor),
        // the open editor isn't replaced while it holds unsaved text.
        app.click(app.actions().querySelector('.verse-note-btn'));
        await settle();
        assert.ok(editorOpen(app));
        assert.equal(editorTitle(app), 'John 3:16');
        assert.equal(el(app, 'note-input').value, typed);
        assert.equal(toastText(app), STILL_WRITING, 'and the toast says why nothing else opened');
    }));

// ---- GG-07: Aa's "Let AI agents save findings".

const SWITCH_FAILED = 'The AI agents setting couldn’t be saved.';
const aiSwitch = (app) => el(app, 'ai-findings-toggle');

test('GG-07: a read of the switch that began before a change never overwrites it', () =>
    withNotes([], async (app) => {
        assert.equal(await savingOn(), false);
        const release = app.holdAnswer('get_ai_findings_enabled');
        const reads = switchCalls(app, 'get_ai_findings_enabled').length;
        app.open('aa');
        await app.until(() => switchCalls(app, 'get_ai_findings_enabled').length > reads, 'Aa’s read');
        const read = switchCalls(app, 'get_ai_findings_enabled').at(-1);
        assert.ok(!read.done, 'Aa’s read of the switch is on its way');

        // Newer: the reader turns it on, and that's saved.
        aiSwitch(app).click();
        await app.until(() => switchCalls(app, 'set_ai_findings_enabled').every((c) => c.done)
            && switchCalls(app, 'set_ai_findings_enabled').length === 1, 'the save');
        assert.equal(await savingOn(), true);

        release();
        await app.until(() => read.done, 'the older read');
        await app.idle();
        assert.equal(aiSwitch(app).checked, true, 'the switch shows what was saved last');
        assert.equal(await savingOn(), true);
    }));

test('GG-07: an older save’s failure, after newer changes, doesn’t undo them', () =>
    withNotes([], async (app) => {
        app.open('aa');
        await app.idle();
        assert.equal(aiSwitch(app).checked, false);

        const fail = app.holdFailure('set_ai_findings_enabled', 'The database is busy.');
        aiSwitch(app).click(); // On: its save fails, late.
        aiSwitch(app).click(); // Off.
        aiSwitch(app).click(); // On again: the newest.
        assert.equal(aiSwitch(app).checked, true);

        fail();
        await app.until(() => switchCalls(app, 'set_ai_findings_enabled').length >= 2
            && switchCalls(app, 'set_ai_findings_enabled').every((c) => c.done), 'every save');
        await app.idle();
        assert.equal(await savingOn(), true, 'the newest change is saved');
        assert.equal(aiSwitch(app).checked, true, 'and shown: the older failure didn’t put it back');
        assert.notEqual(toastText(app), SWITCH_FAILED, 'nothing the reader still wants failed');
    }));

test('GG-07: a save that fails with nothing newer is put back, and said', () =>
    withNotes([], async (app) => {
        app.open('aa');
        await app.idle();
        const fail = app.holdFailure('set_ai_findings_enabled', 'The database is busy.');
        aiSwitch(app).click();
        assert.equal(aiSwitch(app).checked, true);
        fail();
        await app.until(() => switchCalls(app, 'set_ai_findings_enabled').every((c) => c.done), 'the save');
        await app.idle();
        assert.equal(aiSwitch(app).checked, false, 'put back as it was');
        assert.equal(await savingOn(), false);
        assert.equal(toastText(app), SWITCH_FAILED);
    }));

// ---- GG-09: Enter in the search box, waiting for its search.

const searches = (app) => app.calls.filter((c) => c.cmd === 'search');

/**
 * The search box open, `query` typed and Enter pressed straight after, so
 * Enter waits for its search, whose answer is held: the release, and that
 * search's call.
 */
async function enterBeforeTheAnswer(app, query) {
    if (app.searchPanel().hidden) await app.key('/');
    assert.ok(!app.searchPanel().hidden);
    const release = app.holdAnswer('search');
    const before = searches(app).length;
    app.type(app.searchInput(), query);
    app.key('Enter', {}, app.searchInput());
    assert.equal(searches(app).length, before + 1, 'Enter searches at once');
    const search = searches(app).at(-1);
    assert.equal(search.args.query, query);
    assert.ok(!search.done);
    return { release, search };
}

/** Lets the held search answer, and waits for the page to be quiet again. */
async function answer(app, { release, search }) {
    const since = app.calls.length;
    release();
    await app.until(() => search.done, 'the held search');
    await app.idle(since);
}

const wentTo = (app, book) => readings(app, book).length > 0;

test('GG-09: Enter, then Escape: the late answer runs nothing', () =>
    withNotes([], async (app) => {
        const held = await enterBeforeTheAnswer(app, 'Romans 8:28');
        app.key('Escape', {}, app.searchInput());
        assert.ok(app.searchPanel().hidden);
        await answer(app, held);
        assert.equal(app.heading(), 'John 3');
        assert.ok(!wentTo(app, ROMANS), 'nothing navigated');
    }));

test('GG-09: Enter, then a new query: the old search’s late answer runs neither option', () =>
    withNotes([], async (app) => {
        const held = await enterBeforeTheAnswer(app, 'Romans 8:28');
        const before = searches(app).length;
        app.type(app.searchInput(), 'Genesis 1');
        // Its own search, once typing pauses (not held), answered first.
        await app.until(() => searches(app).length > before && searches(app).at(-1).done, 'the newer search');
        await settle();
        assert.ok(app.document.querySelector('#search-results .search-go'), 'Genesis 1 offered');
        await answer(app, held);
        assert.equal(app.heading(), 'John 3');
        assert.ok(!wentTo(app, ROMANS) && !wentTo(app, GENESIS), 'Enter wasn’t pressed on either');
        assert.ok(!app.searchPanel().hidden, 'the box stays, with the newer words');
        assert.equal(app.searchInput().value, 'Genesis 1');
    }));

test('GG-09: Enter, then a translation switch: the late answer runs nothing', () =>
    withNotes([], async (app) => {
        const held = await enterBeforeTheAnswer(app, 'Romans 8:28');
        const since = app.calls.length;
        const select = el(app, 'translation-select');
        select.value = 'web';
        select.dispatchEvent(new app.window.Event('change', { bubbles: true }));
        // The chapter again in the WEB, and the words searched again in it.
        await app.until(() => searches(app).length > 1 && searches(app).at(-1).args.translationCode === 'web'
            && searches(app).at(-1).done, 'the WEB search');
        await app.idle(since);
        await answer(app, held);
        assert.equal(app.heading(), 'John 3');
        assert.ok(!wentTo(app, ROMANS), 'nothing navigated');
    }));

test('GG-09: Enter, then the box closed and opened again: the late answer runs nothing', () =>
    withNotes([], async (app) => {
        const held = await enterBeforeTheAnswer(app, 'Romans 8:28');
        app.key('Escape', {}, app.searchInput());
        await app.key('/');
        assert.ok(!app.searchPanel().hidden, 'open again, the same words in it');
        assert.equal(app.searchInput().value, 'Romans 8:28');
        await answer(app, held);
        assert.equal(app.heading(), 'John 3');
        assert.ok(!wentTo(app, ROMANS), 'Enter was pressed in the box as it was before');
        assert.ok(!app.searchPanel().hidden);
    }));

test('GG-09: Enter, then a navigation: the late answer runs nothing', () =>
    withNotes([], async (app) => {
        // Somewhere to go back from: Genesis 1, by the box.
        await app.key('/');
        await app.search('Genesis 1');
        app.key('Enter', {}, app.searchInput());
        await app.waitForHeading('Genesis 1');
        await app.idle();

        const held = await enterBeforeTheAnswer(app, 'Romans 8:28');
        // Newer: the mouse's back button, back to John 3, the box still open.
        app.window.dispatchEvent(new app.window.MouseEvent('mouseup', { button: 3, bubbles: true, cancelable: true }));
        await app.waitForHeading('John 3');
        assert.ok(!app.searchPanel().hidden);
        await answer(app, held);
        assert.equal(app.heading(), 'John 3');
        assert.ok(!wentTo(app, ROMANS), 'nothing navigated');
    }));

// ---- GG-10: Compare's own fetch, apart from a chapter's load.

const KJV_WEB = ['King James Version (KJV)', 'World English Bible (WEB)'];
const KJV_BSB = ['King James Version (KJV)', 'Berean Standard Bible (BSB)'];
const COMPARE_FAILED = /couldn’t be compared|couldn't be compared/;
const compares = (app) => app.calls.filter((c) => c.cmd === 'get_compare');

/** Compare's "with" switched to `code`, in the translation button's popover. */
function compareWith(app, code) {
    if (app.surface('translation').hidden) app.open('translation');
    const button = el(app, 'compare-with-choices').querySelector(`button[data-value="${code}"]`);
    assert.ok(button, `Compare can be with the ${code}`);
    app.click(button);
}

/** Waits for every comparison asked for after the first `n` to be answered. */
async function comparedSince(app, n, what) {
    await app.until(() => compares(app).length > n && compares(app).slice(n).every((c) => c.done), what);
    await app.idle(app.calls.length);
}

test('GG-10: the WEB’s comparison, answered after the BSB’s, doesn’t replace it', () =>
    withNotes([], async (app) => {
        // Until one is chosen, the KJV is compared with the WEB.
        const release = app.holdAnswer('get_compare');
        await app.key('c');
        const web = compares(app).at(-1);
        assert.deepEqual([...web.args.translationCodes], ['kjv', 'web']);
        assert.ok(!web.done);

        // Newer: with the BSB, answered at once.
        const n = compares(app).length;
        compareWith(app, 'bsb');
        await comparedSince(app, n, 'the BSB’s comparison');
        assert.deepEqual(app.compareColumns(), KJV_BSB);

        release();
        await app.until(() => web.done, 'the WEB’s comparison');
        await app.idle();
        assert.deepEqual(app.compareColumns(), KJV_BSB, 'still the BSB beside the KJV, in two columns');
        assert.equal(app.notice(), '');
    }));

test('GG-10: an obsolete comparison’s failure says nothing', () =>
    withNotes([], async (app) => {
        const fail = app.holdFailure('get_compare', 'The WEB couldn’t be read.');
        await app.key('c');
        const web = compares(app).at(-1);
        const n = compares(app).length;
        compareWith(app, 'bsb');
        await comparedSince(app, n, 'the BSB’s comparison');

        fail();
        await app.until(() => web.done, 'the WEB’s failure');
        await app.idle();
        assert.deepEqual(app.compareColumns(), KJV_BSB);
        assert.doesNotMatch(app.notice(), COMPARE_FAILED, 'no notice for a comparison no longer asked for');
    }));

test('GG-10: Compare off and on again: the first comparison’s late failure says nothing', () =>
    withNotes([], async (app) => {
        const fail = app.holdFailure('get_compare', 'The WEB couldn’t be read.');
        await app.key('c');
        const first = compares(app).at(-1);
        await app.key('c');
        assert.ok(!app.isComparing(), 'off');
        const n = compares(app).length;
        await app.key('c');
        await comparedSince(app, n, 'the comparison asked for again');
        assert.deepEqual(app.compareColumns(), KJV_WEB);

        fail();
        await app.until(() => first.done, 'the first comparison’s failure');
        await app.idle();
        assert.deepEqual(app.compareColumns(), KJV_WEB, 'the newer comparison stays');
        assert.doesNotMatch(app.notice(), COMPARE_FAILED);
    }));

test('GG-10: Compare turned off while its comparison is on its way: its failure says nothing', () =>
    withNotes([], async (app) => {
        const fail = app.holdFailure('get_compare', 'The WEB couldn’t be read.');
        await app.key('c');
        const first = compares(app).at(-1);
        await app.key('c');
        fail();
        await app.until(() => first.done, 'the comparison’s failure');
        await app.idle();
        assert.ok(!app.isComparing());
        assert.equal(app.notice(), '', 'nothing said about a comparison the reader turned off');
    }));

// ---- GG-11: focus after a Library removal, once it's done.

const bookmarked = (verses) => async () => {
    for (const verse of verses) {
        await backend('add_bookmark', { bookId: JOHN, chapter: 3, verseStart: verse, verseEnd: verse });
    }
};
const highlighted = (passages) => async () => {
    for (const [verse, color] of passages) {
        await backend('set_highlight', { bookId: JOHN, chapter: 3, verseStart: verse, verseEnd: verse, color });
    }
};
const removes = (app, list) => [...app.document.querySelectorAll(`#${list} .bookmark-remove`)];
const focused = (app) => app.document.activeElement;

/** × on row `i` of `list` (the Library open on `tab`), its removal held: the release, and the removal's call. */
async function removeHeld(app, tab, list, cmd, i) {
    app.openLibrary(tab);
    await app.until(() => removes(app, list).length > i, `the ${tab}`);
    const release = app.holdCommand(cmd);
    app.click(removes(app, list)[i]);
    await app.until(() => app.calls.some((c) => c.cmd === cmd && !c.done), `the ${cmd} call`);
    return release;
}

/** Lets the held removal go, and waits for its reloads. */
async function removed(app, release, reload) {
    const since = app.calls.length;
    release();
    await app.idle(since, reload);
}

test('GG-11: a bookmark removed late leaves focus in a note editor opened since', () =>
    withNotes([], async (app) => {
        const release = await removeHeld(app, 'bookmarks', 'bookmark-list', 'remove_bookmark', 1);
        // Newer: the Library closed, and a note begun on John 3:16.
        app.click(el(app, 'library-close'));
        assert.ok(app.surface('library').hidden);
        await addNoteOn(app, 16);
        assert.equal(focused(app).id, 'note-input');

        await removed(app, release, 'list_bookmarks');
        assert.equal(removes(app, 'bookmark-list').length, 2, 'the bookmark was removed');
        assert.equal(focused(app).id, 'note-input', 'focus stays in the editor');
    }, { seed: bookmarked([1, 2, 3]) }));

test('GG-11: a highlight removed late leaves focus in the search box opened since', () =>
    withNotes([], async (app) => {
        const release = await removeHeld(app, 'highlights', 'highlight-list', 'remove_highlight', 0);
        app.click(el(app, 'library-close'));
        await app.key('/');
        assert.equal(focused(app), app.searchInput());

        await removed(app, release, 'list_highlight_passages');
        assert.equal(removes(app, 'highlight-list').length, 1, 'the highlight was removed');
        assert.equal(focused(app), app.searchInput(), 'focus stays in the search box');
    }, { seed: highlighted([[15, 'green'], [17, 'pink']]) }));

test('GG-11: a removal finished after another tab was chosen leaves focus alone', () =>
    withNotes([], async (app) => {
        const release = await removeHeld(app, 'bookmarks', 'bookmark-list', 'remove_bookmark', 1);
        app.click(el(app, 'library-tab-highlights'));
        const before = focused(app);

        await removed(app, release, 'list_bookmarks');
        assert.equal(removes(app, 'bookmark-list').length, 2);
        assert.equal(focused(app), before, 'never into the hidden Bookmarks list');
    }, { seed: bookmarked([1, 2, 3]) }));

test('GG-11: a removal finished after the color filter changed leaves focus alone', () =>
    withNotes([], async (app) => {
        const release = await removeHeld(app, 'highlights', 'highlight-list', 'remove_highlight', 0);
        app.click(app.document.querySelector('.highlight-filter-btn[data-filter="green"]'));
        const before = focused(app);

        await removed(app, release, 'list_highlight_passages');
        assert.equal(focused(app), before, 'the reader’s filter, not a row');
    }, { seed: highlighted([[15, 'green'], [16, 'green'], [17, 'pink']]) }));

test('GG-11: with nothing newer, a late removal still moves focus to the next row', () =>
    withNotes([], async (app) => {
        const release = await removeHeld(app, 'bookmarks', 'bookmark-list', 'remove_bookmark', 1);
        await removed(app, release, 'list_bookmarks');
        assert.equal(removes(app, 'bookmark-list').length, 2);
        assert.equal(focused(app), removes(app, 'bookmark-list')[1], 'the next bookmark’s ×');
    }, { seed: bookmarked([1, 2, 3]) }));
