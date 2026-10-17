// Behavior tests for KJV footnotes: the KJV's {…} markup read by the
// backend (supplied words in italics, margin notes as lettered footnotes),
// shown in the reader, its popup, the chapter's Footnotes section and
// compare — against the real backend.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { backend, bookId, closeBridge, openApp, shownVerse, storedVerse } from './harness.mjs';

after(closeBridge);

const EM = '—';

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const markers = (el) => [...el.querySelectorAll('.fn-marker')].map((m) => m.textContent);
const supplied = (el) => [...el.querySelectorAll('.supplied')].map((m) => m.textContent);
const summary = (app) => app.document.querySelector('.footnote-summary');
const summaryRows = (app) => [...app.document.querySelectorAll('.footnote-summary .footnote-row')]
    .map((row) => `${row.querySelector('.footnote-ref').textContent} ${row.querySelector('.footnote-text').textContent}`);
// The selection card, while it's open on `verse` (it follows the last
// selected verse).
const visiblePopup = (verse) => {
    const card = verse.ownerDocument.getElementById('selection-card');
    return card && card.previousElementSibling === verse ? card : null;
};
const popupEntries = (verse) => [...(visiblePopup(verse)?.querySelectorAll('.selection-footnotes .footnote-entry') || [])];
const popupFootnotes = (verse) => popupEntries(verse)
    .map((entry) => [entry.querySelector('.footnote-label').textContent, entry.textContent.slice(entry.querySelector('.footnote-label').textContent.length)]);

async function clickAndWait(app, target, cmd) {
    const before = app.calls.length;
    const event = app.click(target);
    await app.idle(before, cmd);
    return event;
}

const PSALM_23 = [
    '23:2a green...: Heb. pastures of tender grass',
    '23:2b still...: Heb. waters of quietness',
    '23:5a anointest: Heb. makest fat',
    '23:6a for ever: Heb. to length of days',
];

test('the KJV shows supplied words in italics and each margin note as a lettered marker', () =>
    withApp({ book: 'Psalms', chapter: 23, translation: 'kjv' }, async (app) => {
        const v1 = app.verse(1);
        assert.deepEqual(supplied(v1), ['is']);
        assert.equal(v1.querySelector('.supplied').tagName, 'EM');
        assert.deepEqual(markers(v1), []);
        assert.equal(v1.querySelector('.verse-text').textContent, '[A Psalm of David.] The LORD is my shepherd; I shall not want.');

        // Two notes at the end of verse 2, lettered a and b where they stood.
        const v2 = app.verse(2);
        assert.deepEqual(markers(v2), ['a', 'b']);
        assert.equal(
            v2.querySelector('.verse-text').textContent,
            'He maketh me to lie down in green pastures: he leadeth me beside the still waters.ab',
        );
        assert.equal(v2.querySelector('.verse-text').lastElementChild.className, 'fn-marker');
        // Letters start again at a in every verse.
        assert.deepEqual(markers(app.verse(5)), ['a']);
        assert.deepEqual(markers(app.verse(6)), ['a']);

        // No braces anywhere in the chapter being read.
        for (const verse of app.document.querySelectorAll('.chapter-main .verse')) {
            assert.doesNotMatch(verse.querySelector('.verse-text').textContent, /[{}]/);
        }
        // Every verse as the backend shaped it.
        for (let n = 1; n <= 6; n++) {
            const shown = await shownVerse('kjv', 'Psalms', 23, n);
            assert.deepEqual(markers(app.verse(n)), shown.footnotes.map((f) => f.label));
            assert.deepEqual(supplied(app.verse(n)), shown.parts.filter((p) => p.kind === 'supplied').map((p) => p.text));
        }
    }));

test('markers are no tab stops and screen readers hear "has footnotes" once per verse', () =>
    withApp({ book: 'Psalms', chapter: 23, translation: 'kjv' }, async (app) => {
        for (const marker of app.document.querySelectorAll('.fn-marker')) {
            assert.equal(marker.tagName, 'SUP');
            assert.equal(marker.getAttribute('tabindex'), null);
            assert.ok(marker.tabIndex < 0, 'not focusable');
            assert.equal(marker.getAttribute('aria-hidden'), 'true');
            assert.equal(marker.closest('a, button'), null);
        }
        for (let n = 1; n <= 6; n++) {
            const cues = app.verse(n).querySelectorAll('.footnote-cue');
            const expected = markers(app.verse(n)).length ? 1 : 0;
            assert.equal(cues.length, expected, `verse ${n}`);
            if (expected) {
                assert.equal(cues[0].textContent, '(has footnotes) ');
                assert.ok(cues[0].classList.contains('visually-hidden'));
                assert.ok(app.verse(n).classList.contains('has-footnotes'));
            }
        }
    }));

test('the Footnotes section lists the chapter’s footnotes, above Cross References', () =>
    withApp({ book: 'Psalms', chapter: 23, translation: 'kjv' }, async (app) => {
        const section = summary(app);
        assert.ok(section);
        assert.equal(section.querySelector('h3').textContent, 'Footnotes (4)');
        assert.deepEqual(summaryRows(app), PSALM_23);
        assert.ok(section.nextElementSibling.classList.contains('xref-summary'), 'right above Cross References');
        assert.equal(section.closest('.chapter-main').querySelectorAll('.footnote-summary').length, 1);
        // Only the chapter being read: the faded neighbors' notes aren't
        // listed, and the neighbors show no markers (but do show italics).
        assert.ok(summaryRows(app).every((row) => row.startsWith('23:')));
        assert.equal(app.document.querySelectorAll('.chapter-side .fn-marker').length, 0);
        assert.ok(app.document.querySelectorAll('.chapter-side .supplied').length > 0);
        assert.doesNotMatch(app.document.querySelector('.chapter-side').textContent, /[{}]/);
    }));

test('Psalms 119 lists 119:105a, every row in order', () =>
    withApp({ book: 'Psalms', chapter: 119, translation: 'kjv' }, async (app) => {
        const rows = summaryRows(app);
        assert.ok(rows.includes('119:105a lamp: or, candle'));
        const expected = [];
        for (let n = 1; n <= 176; n++) {
            for (const f of (await shownVerse('kjv', 'Psalms', 119, n)).footnotes) expected.push(`119:${n}${f.label} ${f.text}`);
        }
        assert.deepEqual(rows, expected);
    }));

// Each footnote's lemma (the italic words before its first colon), or
// null, and its text, for the section's rows or a popup's entries.
const lemmas = (els) => els.map((el) => {
    const ems = el.querySelectorAll('em');
    assert.ok(ems.length <= 1);
    return ems[0] ? ems[0].textContent : null;
});
const sectionLemmas = (app) => lemmas([...app.document.querySelectorAll('.footnote-summary .footnote-text')]);
const popupLemmas = (verse) => lemmas(popupEntries(verse));

test('a footnote’s lemma is in italics, in the section and in the selection card, the note otherwise as stored', () =>
    withApp({ book: 'Psalms', chapter: 23, translation: 'kjv' }, async (app) => {
        assert.deepEqual(sectionLemmas(app), ['green...', 'still...', 'anointest', 'for ever']);
        const row = app.document.querySelector('.footnote-summary .footnote-text');
        assert.equal(row.innerHTML, '<em class="footnote-lemma">green...</em>: Heb. pastures of tender grass');
        // Still the stored note, character for character.
        assert.deepEqual(summaryRows(app), PSALM_23);

        const verse = app.verse(2);
        await clickAndWait(app, verse.querySelector('.fn-marker'), 'list_notes');
        assert.deepEqual(popupLemmas(verse), ['green...', 'still...']);
        assert.deepEqual(popupFootnotes(verse), [
            ['a', 'green...: Heb. pastures of tender grass'],
            ['b', 'still...: Heb. waters of quietness'],
        ]);
    }));

test('a note without a colon has no lemma: Psalms 73:22’s second, plain', () =>
    withApp({ book: 'Psalms', chapter: 73, translation: 'kjv' }, async (app) => {
        const v22 = await shownVerse('kjv', 'Psalms', 73, 22);
        assert.deepEqual(
            v22.footnotes.map((f) => [f.lemma, f.rest, f.text]),
            [['ignorant', ': Heb. I knew not', 'ignorant: Heb. I knew not'], [null, 'before Heb. with', 'before Heb. with']],
        );
        const rows = [...app.document.querySelectorAll('.footnote-summary .footnote-row')]
            .filter((r) => r.querySelector('.footnote-ref').textContent.startsWith('73:22'));
        assert.deepEqual(lemmas(rows.map((r) => r.querySelector('.footnote-text'))), ['ignorant', null]);
        assert.equal(rows[1].querySelector('.footnote-text').innerHTML, 'before Heb. with');

        const verse = app.verse(22);
        await clickAndWait(app, verse.querySelector('.fn-marker'), 'list_notes');
        assert.deepEqual(popupLemmas(verse), ['ignorant', null]);
    }));

test('clicking a marker selects its verse and the card lists its footnotes, then its cross references', () =>
    withApp({ book: 'Psalms', chapter: 119, translation: 'kjv' }, async (app) => {
        const verse = app.verse(105);
        const marker = verse.querySelector('.fn-marker');
        assert.equal(marker.textContent, 'a');
        await clickAndWait(app, marker, 'list_notes');
        assert.deepEqual(app.selectedVerseNumbers(), [105]);
        const popup = visiblePopup(verse);
        assert.ok(popup, 'the card shows');
        assert.deepEqual(popupFootnotes(verse), [['a', 'lamp: or, candle']]);
        const titles = [...popup.querySelectorAll('.xref-popup-title')].filter((t) => !t.closest('[hidden]')).map((t) => t.textContent);
        assert.deepEqual(titles, ['Footnotes', 'Cross references']);
        assert.ok(popup.querySelectorAll('.xref-citation').length > 0);

        // Clicking the marker again keeps it selected (the verse itself
        // toggles), and clicking the footnote's text leaves it be.
        await clickAndWait(app, marker, 'list_notes');
        assert.deepEqual(app.selectedVerseNumbers(), [105]);
        app.click(popup.querySelector('.footnote-entry'));
        await app.idle(0);
        assert.deepEqual(app.selectedVerseNumbers(), [105]);
        // Clicking the verse's words toggles it off, as before.
        app.click(verse.querySelector('.verse-text'));
        await app.idle(0);
        assert.deepEqual(app.selectedVerseNumbers(), []);
        assert.equal(visiblePopup(verse), null);
    }));

test('the card shows a verse’s footnotes and cross references exactly when it has them', () =>
    withApp({ book: 'Genesis', chapter: 1, translation: 'kjv' }, async (app) => {
        const dto = await backend('get_reading', { bookId: bookId('Genesis'), chapter: 1, translationCode: 'kjv' });
        const v13 = dto.currentVerses[12];
        assert.deepEqual(v13.xrefs, [], 'fixture: Genesis 1:13 has no cross references');
        assert.equal(v13.footnotes.length, 1);

        const verse = app.verse(13);
        assert.ok(!verse.classList.contains('has-xref'));
        await clickAndWait(app, verse, 'list_notes');
        const popup = visiblePopup(verse);
        assert.ok(popup);
        assert.deepEqual(popupFootnotes(verse), [['a', 'And the evening...: Heb. And the evening was, and the morning was etc.']]);
        assert.deepEqual([...popup.querySelectorAll('.xref-popup-title')].filter((t) => !t.closest('[hidden]')).map((t) => t.textContent), ['Footnotes']);
        // The verse's actions are first in it, as with cross references.
        assert.equal(popup.firstElementChild, app.actions());

        // Genesis 1:15 has neither: the card has only its actions.
        const v15 = dto.currentVerses[14];
        assert.deepEqual([v15.xrefs, v15.footnotes], [[], []], 'fixture: Genesis 1:15 has neither');
        for (const v of dto.currentVerses) {
            await clickAndWait(app, app.verse(v.number), 'list_notes');
            const card = visiblePopup(app.verse(v.number));
            assert.equal(card.querySelector('.selection-footnotes').hidden, v.footnotes.length === 0, `Genesis 1:${v.number} footnotes`);
            assert.equal(card.querySelector('.selection-xrefs').hidden, v.xrefs.length === 0, `Genesis 1:${v.number} cross references`);
        }
    }));

test('Copy and the note panel’s quote use the plain text and italics, with no markers', () =>
    withApp({ book: 'Psalms', chapter: 23, translation: 'kjv' }, async (app) => {
        await clickAndWait(app, app.verse(2), 'list_notes');
        await app.copyButton();
        assert.equal(
            app.clipboardWrites.at(-1),
            `He maketh me to lie down in green pastures: he leadeth me beside the still waters. ${EM} Psalms 23:2 (KJV)`,
        );

        app.click(app.verse(1));
        app.click(app.verse(2), { shiftKey: true });
        await app.idle(0);
        app.click(app.actions().querySelector('.verse-note-btn'));
        const quote = app.document.getElementById('note-passage');
        assert.equal(quote.querySelectorAll('.fn-marker').length, 0);
        assert.deepEqual(supplied(quote), ['is']);
        assert.equal(
            quote.textContent,
            '1[A Psalm of David.] The LORD is my shepherd; I shall not want. '
                + '2He maketh me to lie down in green pastures: he leadeth me beside the still waters.',
        );
    }));

test('a cross reference chip’s Go to opens its passage, selected, with the KJV’s italics and no braces', () =>
    withApp({ book: 'John', chapter: 10, translation: 'kjv' }, async (app) => {
        // John 10:11 refers to Psalms 23:1.
        const verse = app.verse(11);
        await clickAndWait(app, verse, 'list_notes');
        const citation = [...visiblePopup(verse).querySelectorAll('.xref-citation')].find((c) => c.textContent === 'Psalms 23:1');
        assert.ok(citation, 'fixture: John 10:11 cites Psalms 23:1');
        await clickAndWait(app, citation, 'get_passage');
        await clickAndWait(app, visiblePopup(verse).querySelector('.xref-preview-go'), 'get_reading');
        assert.equal(app.heading(), 'Psalms 23');
        assert.deepEqual(app.selectedVerseNumbers(), [1]);
        assert.deepEqual(supplied(app.verse(1)), ['is']);
        assert.doesNotMatch(app.verse(1).querySelector('.verse-text').textContent, /[{}]/);
    }));

test('Library previews are the plain text', () =>
    withApp({ book: 'Psalms', chapter: 119, translation: 'kjv' }, async (app) => {
        await clickAndWait(app, app.verse(105), 'list_notes');
        await clickAndWait(app, app.actions().querySelector('.verse-bookmark-btn'), 'list_bookmarks');
        const preview = app.document.querySelector('.bookmark-preview');
        assert.equal(preview.textContent, 'NUN. Thy word is a lamp unto my feet, and a light unto my path.');
        assert.equal(preview.querySelectorAll('*').length, 0);
    }));

test('the WEB is unchanged: no markers, no italics, no Footnotes section', () =>
    withApp({ book: 'Psalms', chapter: 119, translation: 'web' }, async (app) => {
        assert.equal(app.document.querySelectorAll('.fn-marker, .supplied, .footnote-cue').length, 0);
        assert.equal(summary(app), null);
        assert.ok(app.document.querySelector('.xref-summary'));
        assert.equal(app.verse(105).querySelector('.verse-text').textContent, storedVerse('web', 'Psalms', 119, 105));
    }));

test('comparing: the KJV column has the markers, and the section is the KJV’s, named', () =>
    withApp({ book: 'Psalms', chapter: 23, translation: 'web', compare: true }, async (app) => {
        assert.deepEqual(markers(app.verse(2, 'kjv')), ['a', 'b']);
        assert.deepEqual(supplied(app.verse(1, 'kjv')), ['is']);
        assert.deepEqual(markers(app.verse(2, 'web')), []);
        assert.deepEqual(supplied(app.verse(1, 'web')), []);
        assert.equal(app.verse(2, 'web').querySelector('.verse-text').textContent, storedVerse('web', 'Psalms', 23, 2));

        const section = summary(app);
        assert.equal(section.querySelector('h3').textContent, 'KJV footnotes (4)');
        assert.deepEqual(summaryRows(app), PSALM_23);
        assert.ok(section.nextElementSibling.classList.contains('xref-summary'));

        // A KJV marker selects its verse in the KJV column.
        await clickAndWait(app, app.verse(2, 'kjv').querySelector('.fn-marker'), 'list_notes');
        assert.deepEqual(app.selectedVerseNumbers('kjv'), [2]);
        assert.deepEqual(popupFootnotes(app.verse(2, 'kjv')).map(([label]) => label), ['a', 'b']);
    }));

test('footnotes and supplied words are rendered as text, never as markup (synthetic DTO)', () =>
    withApp(
        {
            book: 'Psalms',
            chapter: 23,
            translation: 'kjv',
            alterReading: (dto) => ({
                ...dto,
                currentVerses: dto.currentVerses.map((v) => (v.number === 1
                    ? {
                        ...v,
                        text: 'SYNTHETIC <i>x</i> & y',
                        parts: [
                            { kind: 'text', text: 'SYNTHETIC ' },
                            { kind: 'supplied', text: '<i>x</i>' },
                            { kind: 'text', text: ' & y' },
                            { kind: 'note', text: '<b>n</b>', label: '<u>a</u>' },
                        ],
                        footnotes: [{
                            label: '<u>a</u>',
                            lemma: '<img src=x onerror="window.pwned=1">',
                            rest: ': <b>"q"</b> \'a\'',
                            text: '<img src=x onerror="window.pwned=1">: <b>"q"</b> \'a\'',
                        }],
                    }
                    : v)),
            }),
            // The text copied comes from the backend's formatter.
            alter: {
                get_copy_text: (copy) => (copy.reference === 'Psalms 23:1'
                    ? { ...copy, formats: { ...copy.formats, reference: `SYNTHETIC <i>x</i> & y ${EM} Psalms 23:1 (KJV)` } }
                    : copy),
            },
        },
        async (app) => {
            const verse = app.verse(1);
            assert.equal(app.document.querySelector('#reading-pane img, #reading-pane b, #reading-pane u, #reading-pane i'), null);
            assert.equal(verse.querySelector('.supplied').textContent, '<i>x</i>');
            assert.equal(verse.querySelector('.fn-marker').textContent, '<u>a</u>');
            assert.ok(summaryRows(app).includes('23:1<u>a</u> <img src=x onerror="window.pwned=1">: <b>"q"</b> \'a\''));
            assert.deepEqual(sectionLemmas(app)[0], '<img src=x onerror="window.pwned=1">');
            await clickAndWait(app, verse, 'list_notes');
            assert.deepEqual(popupFootnotes(verse), [['<u>a</u>', '<img src=x onerror="window.pwned=1">: <b>"q"</b> \'a\'']]);
            assert.deepEqual(popupLemmas(verse), ['<img src=x onerror="window.pwned=1">']);
            assert.equal(app.window.pwned, undefined);
            await app.copyButton();
            assert.equal(app.clipboardWrites.at(-1), `SYNTHETIC <i>x</i> & y ${EM} Psalms 23:1 (KJV)`);
        },
    ));
