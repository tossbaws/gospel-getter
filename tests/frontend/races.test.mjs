// Answers that come back late, or out of order: what the page shows is
// always the latest thing the reader asked for. Compare switched while a
// chapter is on its way; the bookmark list and the highlights refreshed
// twice in quick succession; and a failure whose error text is empty.
// (Check now during an install is in updates.test.mjs.)

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, closeBridge, openApp } from './harness.mjs';

after(closeBridge);

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

// As the harness's `until`, with longer for a few writes in a row.
async function eventually(check, what) {
    for (let i = 0; i < 1000; i++) {
        if (check()) return;
        await new Promise((r) => setTimeout(r, 10));
    }
    throw new Error(`timed out waiting for ${what}`);
}

const COMPARE_FAILED = /couldn’t be compared|couldn't be compared/;

test('compare switched on while a chapter is on its way: it comes in compare, with no false failure', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        const release = app.holdCommand('get_reading');
        app.key('ArrowRight');
        app.key('c');
        release();
        await app.waitForHeading('John 4');
        await app.until(() => app.compareColumns().length === 2, 'the compare columns');
        await app.idle(0);
        assert.equal(app.heading(), 'John 4');
        assert.equal(app.compareColumns().length, 2);
        assert.doesNotMatch(app.notice(), COMPARE_FAILED);
        assert.equal(app.notice(), '');
        // Asked for again, now with its comparison.
        const john4 = app.calls.filter((c) => c.cmd === 'get_compare' && c.args.chapter === 4);
        assert.equal(john4.length, 1);
    }));

test('compare switched off while a chapter is on its way: it comes in one column', () =>
    withApp({ book: 'John', chapter: 3, compare: true }, async (app) => {
        assert.equal(app.compareColumns().length, 2);
        const release = app.holdCommand('get_reading');
        app.key('ArrowRight');
        app.key('c');
        release();
        await app.waitForHeading('John 4');
        await app.idle(0);
        assert.equal(app.isComparing(), false);
        assert.equal(app.document.querySelectorAll('.chapter-side').length, 2, 'the usual layout');
        assert.equal(app.notice(), '');
    }));

test('two quick translation switches: the bookmark list is labelled in the last one', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        let before = app.calls.length;
        app.click(app.verse(16));
        app.click(app.bookmarkButton());
        await app.idle(before, 'list_bookmarks');
        const preview = async (code) => (await backend('list_bookmarks', { translationCode: code }))[0].preview;
        const [web, bsb] = [await preview('web'), await preview('bsb')];
        assert.notEqual(web, bsb, 'fixture: the previews differ');

        // The WEB's list is answered, but arrives after the BSB's.
        const releaseWeb = app.holdAnswer('list_bookmarks');
        const select = app.document.getElementById('translation-select');
        for (const code of ['web', 'bsb']) {
            select.value = code;
            select.dispatchEvent(new app.window.Event('change', { bubbles: true }));
        }
        await app.until(() => app.bookmarkItems()[0]?.preview === bsb, 'the BSB list');
        before = app.calls.length;
        releaseWeb();
        await app.until(() => app.calls.every((c) => c.done), 'the WEB list');
        await app.idle(before);
        assert.equal(app.bookmarkItems()[0].preview, bsb);
        assert.equal(app.bookmarkItems().length, 1);
    }));

test('two quick highlight removals: the first refresh, arriving last, doesn’t bring one back', () =>
    withApp({ book: 'John', chapter: 3 }, async (app) => {
        for (const [verse, color] of [[16, 'yellow'], [18, 'green']]) {
            const before = app.calls.length;
            app.click(app.verse(verse));
            app.click(app.actions().querySelector(`.highlight-swatch[data-color="${color}"]`));
            await app.idle(before, 'list_highlight_passages');
        }
        app.key('Escape');
        let before = app.calls.length;
        app.openLibrary('highlights');
        await app.idle(before);
        const removes = () => [...app.document.querySelectorAll('#highlight-list .bookmark-remove')];
        await app.until(() => removes().length === 2, 'two highlights listed');

        const since = app.calls.length;
        const refreshes = () => app.calls.slice(since).filter((c) => c.cmd === 'list_highlights');
        const releaseFirst = app.holdAnswer('list_highlights');
        app.click(removes()[0]);
        await eventually(() => refreshes().length === 1, 'the first refresh');
        app.click(removes()[1]);
        await eventually(() => refreshes().length === 2 && refreshes()[1].done, 'the second refresh');
        assert.equal(refreshes()[0].done, false, 'the first answer is still on its way');
        await app.until(() => !app.verse(16).dataset.highlight && !app.verse(18).dataset.highlight, 'both gone');
        before = app.calls.length;
        releaseFirst();
        await app.until(() => app.calls.every((c) => c.done), 'the first refresh to land');
        await app.idle(before);
        assert.equal(app.verse(16).dataset.highlight, undefined);
        assert.equal(app.verse(18).dataset.highlight, undefined, 'not brought back by the older answer');
        assert.equal(removes().length, 0);
    }));

test('a cross-reference that fails with no error text still says it failed', () =>
    withApp({ book: 'John', chapter: 3, failing: { get_passage: '' } }, async (app) => {
        let before = app.calls.length;
        app.click(app.verse(16));
        await app.idle(before, 'list_notes');
        const chip = [...app.document.querySelectorAll('#selection-card .xref-citation')]
            .find((c) => c.textContent === '1 John 4:9-10');
        before = app.calls.length;
        app.click(chip);
        await app.idle(before, 'get_passage');
        const preview = app.document.getElementById('xref-preview');
        assert.equal(preview.hidden, false);
        assert.equal(preview.querySelector('.xref-preview-notice').textContent, 'Couldn\'t read 1 John 4:9–10: Something went wrong.');
        assert.ok(preview.querySelector('.xref-preview-go'));
        assert.deepEqual(app.consoleErrors.filter(([first]) => String(first).includes('TypeError')), []);
    }));

test('other failures with no error text say so too, rather than nothing', () =>
    withApp({ book: 'John', chapter: 3, failing: { open_release_notes: '', open_credit_link: '' } }, async (app) => {
        await app.idle(0, 'get_app_version');
        app.open('about');
        let before = app.calls.length;
        app.click(app.document.getElementById('release-notes-link'));
        await app.idle(before, 'open_release_notes');
        assert.equal(app.document.getElementById('release-notes-status').textContent, 'Something went wrong.');
        app.key('Escape', {}, app.document.activeElement);

        before = app.calls.length;
        app.click(app.document.querySelector('.xref-credit a'));
        await app.idle(before, 'open_credit_link');
        assert.equal(app.toast().textContent, 'Something went wrong.');
        assert.ok(app.toast().classList.contains('is-error'));
    }));
