// Behavior tests for the Berean Standard Bible and the translation
// chooser: Aa's list of translations, the BSB read with its footnotes (kept
// apart from its text, lettered where Berean put them), compare with any
// two translations (the one being read on the left, the chosen one beside
// it, remembered and carried in a backup), the verses the BSB doesn't
// number, and About's credits — against the real backend.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BIBLES, backend, bookId, closeBridge, openApp, shownVerse, storedVerse } from './harness.mjs';

const dir = mkdtempSync(join(tmpdir(), 'gospel-getter-bsb-'));
after(async () => {
    await closeBridge();
    rmSync(dir, { recursive: true, force: true });
});

const EM = '—';

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

async function clickAndWait(app, target, cmd) {
    const before = app.calls.length;
    app.click(target);
    await app.idle(before, cmd);
}

const el = (app, id) => app.document.getElementById(id);
const radios = (app) => [...el(app, 'translation-choices').querySelectorAll('[role="radio"]')];
const withButtons = (app) => [...el(app, 'compare-with-choices').querySelectorAll('button')];
const markers = (verse) => [...verse.querySelectorAll('.fn-marker')].map((m) => m.textContent);
const verseText = (verse) => [...verse.querySelector('.verse-text').childNodes]
    .filter((n) => !(n.classList && n.classList.contains('fn-marker')))
    .map((n) => n.textContent)
    .join('');
const card = (app) => el(app, 'selection-card');
const summaryRows = (app) => [...app.document.querySelectorAll('.footnote-summary .footnote-row')];
const footnotesOf = (verse) => BIBLES.bsb[bookId('Genesis') - 1].footnotes.filter((f) => f.chapter === 1 && f.verse === verse);

test('Aa lists every translation on a line of its own, as a radio group, KJV, BSB, WEB', () =>
    withApp({ book: 'John', chapter: 3, translation: 'kjv' }, async (app) => {
        app.open('aa');
        const group = el(app, 'translation-choices');
        assert.equal(group.getAttribute('role'), 'radiogroup');
        assert.equal(group.getAttribute('aria-labelledby'), 'translation-label');
        assert.deepEqual(radios(app).map((r) => r.textContent), [
            'KJV · King James Version · 1611/1769 · public domain',
            'BSB · Berean Standard Bible · 2023 · public domain',
            'WEB · World English Bible · public domain',
        ]);
        // The dots are only seen; the code, name, date and licence are read.
        for (const dot of group.querySelectorAll('.translation-dot')) assert.equal(dot.getAttribute('aria-hidden'), 'true');
        assert.deepEqual(radios(app).map((r) => r.getAttribute('aria-checked')), ['true', 'false', 'false']);
        // One tab stop: the chosen one, which Aa focuses when it opens.
        assert.deepEqual(radios(app).map((r) => r.tabIndex), [0, -1, -1]);
        assert.equal(app.document.activeElement, radios(app)[0]);

        // The arrow keys move and choose, and never turn the chapter.
        let before = app.calls.length;
        app.key('ArrowDown', {}, radios(app)[0]);
        await app.idle(before, 'get_reading');
        assert.equal(el(app, 'translation-select').value, 'bsb');
        assert.equal(app.heading(), 'John 3');
        assert.equal(app.document.activeElement, radios(app)[1]);
        assert.deepEqual(radios(app).map((r) => r.tabIndex), [-1, 0, -1]);
        assert.equal(verseText(app.verse(16, 'bsb')), storedVerse('bsb', 'John', 3, 16));
        before = app.calls.length;
        app.key('End', {}, radios(app)[1]);
        await app.idle(before, 'get_reading');
        assert.equal(el(app, 'translation-select').value, 'web');
        before = app.calls.length;
        app.key('ArrowRight', {}, radios(app)[2]);
        await app.idle(before, 'get_reading');
        assert.equal(el(app, 'translation-select').value, 'kjv', 'round to the first');
        before = app.calls.length;
        app.key('ArrowUp', {}, radios(app)[0]);
        await app.idle(before, 'get_reading');
        assert.equal(el(app, 'translation-select').value, 'web', 'and back to the last');
        assert.equal(app.heading(), 'John 3');
        // A click chooses too.
        await clickAndWait(app, radios(app)[1], 'get_reading');
        assert.equal(el(app, 'translation-select').value, 'bsb');
    }));

test('the BSB reads as Berean has it: the reference, the text, the faded chapters, a restart', async () => {
    let storage;
    await withApp({ book: 'Psalms', chapter: 23, translation: 'bsb' }, async (app) => {
        assert.equal(el(app, 'ref-text').textContent, 'Psalms 23 · BSB');
        assert.equal(
            el(app, 'ref-btn').getAttribute('aria-label'),
            'Psalms 23, Berean Standard Bible. Choose a book and chapter',
        );
        assert.equal(app.document.title, 'Psalms 23 · BSB — Gospel Getter');
        for (let n = 1; n <= 6; n++) {
            assert.equal(verseText(app.verse(n, 'bsb')), storedVerse('bsb', 'Psalms', 23, n));
        }
        // The Psalm's title is in verse 1, as Berean has it.
        assert.ok(storedVerse('bsb', 'Psalms', 23, 1).startsWith('A Psalm of David. '));
        assert.equal(app.sideVerse('prev', 1).querySelector('.verse-text').textContent, storedVerse('bsb', 'Psalms', 22, 1));
        storage = app.storage();
    });
    // The reading position keeps the translation.
    await withApp({ book: 'Psalms', chapter: 23, translation: 'bsb', storage }, async (app) => {
        assert.equal(el(app, 'translation-select').value, 'bsb');
        assert.equal(el(app, 'ref-text').textContent, 'Psalms 23 · BSB');
    });
});

test('BSB footnotes: a lettered marker where Berean put each, the card, and the chapter’s Footnotes', () =>
    withApp({ book: 'Genesis', chapter: 1, translation: 'bsb' }, async (app) => {
        // Genesis 1:3's note follows “light,”; the verse's words are its own.
        const v3 = app.verse(3, 'bsb');
        assert.deepEqual(markers(v3), ['a']);
        const html = v3.querySelector('.verse-text').innerHTML;
        assert.ok(html.includes('“Let there be light,”<sup class="fn-marker"'), html);
        assert.equal(verseText(v3), storedVerse('bsb', 'Genesis', 1, 3));
        assert.equal(footnotesOf(3).length, 1);
        // Every verse: as many markers as Berean has notes on it.
        for (let n = 1; n <= 31; n++) {
            assert.equal(markers(app.verse(n, 'bsb')).length, footnotesOf(n).length, `Genesis 1:${n}`);
        }

        // The card: verse 6's note, its alternative readings in italics.
        await clickAndWait(app, app.verse(6, 'bsb'), 'list_notes');
        const entry = card(app).querySelector('.selection-footnotes .footnote-entry');
        assert.ok(entry);
        assert.ok(entry.textContent.endsWith('Or a canopy or a firmament or a vault; also in verses 7, 8, 14, 15, 17, and 20'), entry.textContent);
        assert.deepEqual([...entry.querySelectorAll('em.footnote-alt')].map((e) => e.textContent), ['a canopy ', 'a firmament ', 'a vault']);

        // The chapter's Footnotes: every note, in order, collapsed until
        // opened and then remembered, as for the KJV.
        const section = app.document.querySelector('.footnote-summary');
        const notes = BIBLES.bsb[0].footnotes.filter((f) => f.chapter === 1).length;
        assert.equal(section.querySelector('h3').textContent, `Footnotes (${notes})`);
        assert.equal(summaryRows(app).length, notes);
        assert.equal(summaryRows(app)[0].querySelector('.footnote-ref').textContent, '1:3a');
        assert.equal(summaryRows(app)[0].querySelector('.footnote-text').textContent, 'Cited in 2 Corinthians 4:6');
        // Faded neighbours show no markers.
        assert.equal(app.document.querySelectorAll('.chapter-side .fn-marker').length, 0);
    }));

test('a note on a verse the BSB leaves out gives its wording, with the verse number set small', () =>
    withApp({ book: 'Matthew', chapter: 17, translation: 'bsb' }, async (app) => {
        await clickAndWait(app, app.verse(20, 'bsb'), 'list_notes');
        const entry = card(app).querySelector('.selection-footnotes .footnote-entry');
        assert.equal(entry.querySelector('sup.footnote-verse').textContent, '21');
        assert.equal(entry.querySelector('em.footnote-alt').textContent, 'But this kind does not come out except by prayer and fasting');
        assert.ok(entry.textContent.includes('BYZ and TR include 21But this kind'), entry.textContent);
        assert.ok(!entry.innerHTML.includes('\\'), 'no USFM markers shown');
    }));

test('Copy, the note panel and previews use the BSB’s text without its markers', () =>
    withApp({ book: 'Genesis', chapter: 1, translation: 'bsb' }, async (app) => {
        await clickAndWait(app, app.verse(3, 'bsb'), 'list_notes');
        await app.copyButton();
        assert.equal(app.clipboardWrites.at(-1), `${storedVerse('bsb', 'Genesis', 1, 3)} ${EM} Genesis 1:3 (BSB)`);

        await app.click(app.actions().querySelector('.verse-note-btn'));
        const quote = el(app, 'note-passage');
        assert.equal(quote.querySelectorAll('.fn-marker').length, 0);
        assert.equal(quote.textContent, `3${storedVerse('bsb', 'Genesis', 1, 3)}`);

        // Previewed from elsewhere: Genesis 1:3 cited by 2 Corinthians 4:6.
        const passage = await backend('get_passage', { translationCode: 'bsb', bookId: 1, chapter: 1, start: 3, end: 3 });
        assert.deepEqual(passage.verses[0].footnotes, []);
        assert.ok(passage.verses[0].parts.every((p) => p.kind === 'text'));
        assert.equal(passage.verses[0].text, storedVerse('bsb', 'Genesis', 1, 3));
    }));

test('a verse the BSB doesn’t number: no row, and a notice that says so when it’s opened', () =>
    withApp({ book: 'Matthew', chapter: 17, translation: 'bsb' }, async (app) => {
        assert.equal(app.document.querySelector('.chapter-main .verse[data-translation="bsb"][data-verse="21"]'), null);
        assert.ok(app.verse(20, 'bsb') && app.verse(22, 'bsb'));
        // Shift-click across it: the range is 20–22, 21 left out.
        app.click(app.verse(20, 'bsb'));
        app.click(app.verse(22, 'bsb'), { shiftKey: true });
        await app.idle(0);
        assert.deepEqual(app.selectedVerseNumbers('bsb'), [20, 22]);
        await app.copyButton();
        assert.equal(
            app.clipboardWrites.at(-1),
            `${storedVerse('bsb', 'Matthew', 17, 20)} ${storedVerse('bsb', 'Matthew', 17, 22)} ${EM} Matthew 17:20–22 (BSB)`,
        );
        app.key('Escape');

        // Going to it: the chapter, and why nothing is selected. It isn't
        // past the chapter's end, so there's no "ends at verse" there.
        await app.search('Matthew 17:21');
        const before = app.calls.length;
        app.key('Enter', {}, app.searchInput());
        await app.idle(before, 'get_reading');
        assert.equal(app.heading(), 'Matthew 17');
        assert.deepEqual(app.selectedVerseNumbers(), []);
        assert.equal(app.notice(), "Matthew 17:21 isn't numbered in the Berean Standard Bible. Showing the whole chapter.");
    }));

const PAIRS = [
    ['kjv', 'bsb'], ['kjv', 'web'],
    ['bsb', 'kjv'], ['bsb', 'web'],
    ['web', 'kjv'], ['web', 'bsb'],
];
const NAMES = { kjv: 'King James Version (KJV)', bsb: 'Berean Standard Bible (BSB)', web: 'World English Bible (WEB)' };

test('compare any two: the one being read on the left, the chosen one beside it', async () => {
    for (const [left, right] of PAIRS) {
        await withApp({
            book: 'Romans', chapter: 14, translation: left, compare: true,
            storage: { 'gospel-getter-compare-with': right },
        }, async (app) => {
            assert.deepEqual(app.compareColumns(), [NAMES[left], NAMES[right]], `${left} + ${right}`);
            assert.equal(el(app, 'ref-text').textContent, `Romans 14 · ${left.toUpperCase()} + ${right.toUpperCase()}`);
            const call = app.calls.find((c) => c.cmd === 'get_compare');
            assert.deepEqual([...call.args.translationCodes], [left, right]);
            // Each column is its own translation's text, as shown.
            for (const code of [left, right]) {
                const shown = await shownVerse(code, 'Romans', 14, 1);
                assert.equal(verseText(app.verse(1, code)), code === 'bsb' ? storedVerse('bsb', 'Romans', 14, 1) : shown.text, code);
            }
            // Aa's "with" offers the other two, the chosen one pressed.
            app.open('aa');
            assert.deepEqual(withButtons(app).map((b) => b.dataset.value), ['kjv', 'bsb', 'web'].filter((c) => c !== left));
            assert.deepEqual(withButtons(app).filter((b) => b.getAttribute('aria-pressed') === 'true').map((b) => b.dataset.value), [right]);
        });
    }
});

test('until one is chosen, compare pairs the KJV and the WEB as it always has; the choice is remembered', async () => {
    let storage;
    await withApp({ book: 'John', chapter: 3, translation: 'kjv', compare: true }, async (app) => {
        assert.deepEqual(app.compareColumns(), [NAMES.kjv, NAMES.web]);
        app.open('aa');
        assert.equal(el(app, 'compare-with-choices').getAttribute('aria-label'), 'Compare with');
        const bsb = withButtons(app).find((b) => b.dataset.value === 'bsb');
        await clickAndWait(app, bsb, 'get_compare');
        assert.deepEqual(app.compareColumns(), [NAMES.kjv, NAMES.bsb]);
        assert.equal(app.storage()['gospel-getter-compare-with'], 'bsb');
        assert.ok(app.surface('aa') && !app.surface('aa').hidden, 'Aa stays open');
        storage = app.storage();
    });
    // After a restart; reading the BSB, its partner falls back to the KJV.
    await withApp({ book: 'John', chapter: 3, translation: 'kjv', compare: true, storage }, async (app) => {
        assert.deepEqual(app.compareColumns(), [NAMES.kjv, NAMES.bsb]);
        app.open('aa');
        await clickAndWait(app, radios(app)[1], 'get_compare');
        assert.deepEqual(app.compareColumns(), [NAMES.bsb, NAMES.kjv]);
        assert.deepEqual(withButtons(app).map((b) => b.dataset.value), ['kjv', 'web']);
    });
    // Without compare, choosing "with" waits for C.
    await withApp({ book: 'John', chapter: 3, translation: 'bsb' }, async (app) => {
        app.open('aa');
        app.click(withButtons(app).find((b) => b.dataset.value === 'web'));
        await app.idle(0);
        assert.ok(!app.isComparing());
        app.key('Escape', {}, app.document.activeElement);
        const before = app.calls.length;
        app.key('c');
        await app.idle(before, 'get_compare');
        assert.deepEqual(app.compareColumns(), [NAMES.bsb, NAMES.web]);
    });
});

test('compare with the BSB: its gaps, its footnotes in its column, and the preview from the left', () =>
    withApp({ book: 'Matthew', chapter: 17, translation: 'kjv', compare: true, storage: { 'gospel-getter-compare-with': 'bsb' } }, async (app) => {
        const gap = app.document.querySelector('.compare-gap[data-gap-verse="21"]');
        assert.ok(gap);
        assert.equal(gap.dataset.code, 'BSB');
        assert.match(gap.textContent, /Not numbered in the BSB/);
        assert.ok(app.verse(21, 'kjv'));
        assert.deepEqual(markers(app.verse(20, 'bsb')), ['a']);
        // Each column's footnotes are listed for it, named.
        const titles = [...app.document.querySelectorAll('.footnote-summary h3')].map((h) => h.textContent);
        assert.ok(titles.some((t) => t.startsWith('BSB footnotes (')), titles.join(' | '));
        // Selecting in the BSB column; the card lists its notes.
        await clickAndWait(app, app.verse(20, 'bsb'), 'list_notes');
        assert.deepEqual(app.selectedVerseNumbers('bsb'), [20]);
        assert.ok(card(app).querySelector('.selection-footnotes .footnote-entry').textContent.includes('BYZ and TR include'));
        // Its cross-references preview in the KJV, the left column.
        const chip = card(app).querySelector('.xref-citation');
        await clickAndWait(app, chip, 'get_passage');
        assert.match(el(app, 'xref-preview-title').textContent, /\(KJV\)$/);
        // Flowing is ignored while comparing: a verse to a row.
        assert.equal(app.document.querySelector('.compare-grid .chapter-text'), null);
    }));

test('compare’s partner goes into a backup, and comes back from one', async () => {
    const path = join(dir, 'compare-with.gospel-getter.json');
    await withApp({ book: 'John', chapter: 3, translation: 'web', storage: { 'gospel-getter-compare-with': 'bsb' } }, async (app) => {
        await app.pick(path);
        app.open('menu');
        const before = app.calls.length;
        app.click(el(app, 'export-data-btn'));
        await app.idle(before);
        const json = JSON.parse(readFileSync(path, 'utf8'));
        assert.equal(json.preferences.compare_with, 'bsb');
    });
    // Unchosen, it isn't exported at all.
    await withApp({ book: 'John', chapter: 3 }, async (app) => {
        const prefs = app.window.gospelGetterPreferences.current();
        assert.ok(!('compare_with' in prefs), JSON.stringify(prefs));
    });
    await closeBridge();
    await withApp({ book: 'Genesis', chapter: 1, translation: 'kjv', compare: true }, async (app) => {
        assert.deepEqual(app.compareColumns(), [NAMES.kjv, NAMES.web]);
        await app.pick(path);
        app.open('menu');
        let before = app.calls.length;
        app.click(el(app, 'import-data-btn'));
        await app.idle(before);
        assert.match(el(app, 'import-prefs-label').textContent, /compare with the BSB/);
        before = app.calls.length;
        app.click(el(app, 'import-confirm'));
        await app.idle(before, 'apply_import');
        assert.equal(app.document.documentElement.dataset.compareWith, 'bsb');
        assert.equal(app.storage()['gospel-getter-compare-with'], 'bsb');
    });
});

test('About credits every bundled text, and the cross-references', () =>
    withApp({}, async (app) => {
        const credits = [...el(app, 'about-credits').querySelectorAll('li')].map((li) => li.textContent);
        assert.deepEqual(credits, [
            'King James Version (KJV), public domain, from the thiagobodruk/bible dataset',
            'Berean Standard Bible (BSB), public domain, berean.bible',
            'World English Bible (WEB), public domain, from TehShrike/world-english-bible (via ebible.org)',
            'Cross-references from OpenBible.info, CC BY 4.0',
        ]);
        assert.ok(el(app, 'about-panel').contains(el(app, 'about-credits')));
        assert.equal(el(app, 'credits-label').textContent, 'Texts');
    }));
