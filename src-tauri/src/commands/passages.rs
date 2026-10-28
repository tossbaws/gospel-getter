//! A passage on its own: read in place (`get_passage`, a cross-reference
//! previewed in the selection card) or copied (`get_copy_text`).

use serde::Serialize;

use super::{AppState, SimpleVerseDto};
use crate::db;
use crate::passage_copy::{self, CopyFormat};
use crate::passage_reference::passage_reference;

/// A passage to read in place: a cross-reference previewed in the
/// selection card. Its verses are the ones `translation_code` numbers in
/// `start..=end`, shown as the reader shows them but without margin-note
/// markers (a preview has no footnotes); `notice` says which of them it
/// doesn't number, when any.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PassageDto {
    pub book_id: i64,
    pub chapter: i64,
    pub start: i64,
    pub end: i64,
    /// "1 John 4:9–10".
    pub reference: String,
    pub translation_code: String,
    pub verses: Vec<SimpleVerseDto>,
    /// e.g. "Romans 16:25–27 isn't numbered in the World English Bible —
    /// Romans 16 ends at verse 24 there."; `None` when every verse is.
    pub notice: Option<String>,
}

/// One passage of one translation, fetched when a cross-reference is
/// previewed rather than embedded up front, since few are.
#[tauri::command]
pub async fn get_passage(
    state: tauri::State<'_, AppState>,
    translation_code: String,
    book_id: i64,
    chapter: i64,
    start: i64,
    end: i64,
) -> Result<PassageDto, String> {
    passage_text(&state, &translation_code, book_id, chapter, start, end).await
}

/// The logic behind `get_passage`. A translation, book or chapter that
/// doesn't exist, or verses that aren't a range from 1 up, are an error;
/// verses past the chapter's end (in this translation) aren't, as they're
/// numbered in another translation. Nor are verses past the end in every
/// translation (a few cross references reach past it): the notice names
/// them like any others, but they're never counted one by one, so `end`
/// sizes no loop and no list.
pub async fn passage_text(
    state: &AppState,
    translation_code: &str,
    book_id: i64,
    chapter: i64,
    start: i64,
    end: i64,
) -> Result<PassageDto, String> {
    let translation = state
        .translations
        .iter()
        .find(|t| t.code == translation_code)
        .ok_or_else(|| format!("There's no translation \"{translation_code}\"."))?;
    let book = state
        .books
        .iter()
        .find(|b| b.id == book_id)
        .ok_or_else(|| format!("There's no book {book_id}."))?;
    if !(1..=book.chapter_count).contains(&chapter) {
        return Err(format!("{} has no chapter {chapter}.", book.name));
    }
    if start < 1 || end < start {
        return Err(format!("Verses {start} to {end} aren't a passage."));
    }
    let unreadable = |e: anyhow::Error| {
        tracing::error!("Failed to read {} {chapter}: {e}", book.name);
        format!("{} {chapter} couldn't be read.", book.name)
    };
    let chapter_verses = state
        .store
        .chapter_verses(translation.id, book_id, chapter)
        .await
        .map_err(unreadable)?;
    let last_anywhere = state
        .store
        .chapter_last_verse(book_id, chapter)
        .await
        .map_err(unreadable)?;
    let last = chapter_verses.last().map_or(0, |v| v.verse);
    let verses: Vec<SimpleVerseDto> = chapter_verses
        .into_iter()
        .filter(|v| (start..=end).contains(&v.verse))
        .map(|v| SimpleVerseDto::without_notes(&translation.code, v))
        .collect();
    let numbered: Vec<i64> = verses.iter().map(|v| v.number).collect();
    let missing = missing_runs(&numbered, start, end, last_anywhere);
    let notice = (!missing.is_empty()).then(|| {
        let what = passage_reference(&book.name, chapter, Some((start, end)));
        let (first_missing, _) = missing[0];
        let (_, last_missing) = missing[missing.len() - 1];
        if first_missing > last {
            let missing =
                passage_reference(&book.name, chapter, Some((first_missing, last_missing)));
            format!(
                "{missing} isn't numbered in the {} — {} {chapter} ends at verse {last} there.",
                translation.name, book.name
            )
        } else {
            let (noun, verb) = if first_missing == last_missing {
                ("verse", "isn't")
            } else {
                ("verses", "aren't")
            };
            format!(
                "In {what}, {noun} {} {verb} numbered in the {}.",
                verse_list(&missing),
                translation.name
            )
        }
    });
    Ok(PassageDto {
        book_id,
        chapter,
        start,
        end,
        reference: passage_reference(&book.name, chapter, Some((start, end))),
        translation_code: translation.code.clone(),
        verses,
        notice,
    })
}

/// A passage in every Copy as… format (see `passage_copy`), fetched
/// whenever verses are selected or a chapter shown, so a copy can be
/// written at once, within the click or key that asked for it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyTextDto {
    /// "John 3:16–18", or "John 3" for a whole chapter: what the status
    /// line says was copied.
    pub reference: String,
    pub translation_code: String,
    pub formats: CopyFormatsDto,
}

/// The passage as each format of `passage_copy::CopyFormat` has it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyFormatsDto {
    pub reference: String,
    pub numbered: String,
    pub markdown: String,
    pub plain: String,
}

/// A passage to copy: verses `start..=end` of a chapter, or, without them,
/// the whole chapter, in `translation_code`.
#[tauri::command]
pub async fn get_copy_text(
    state: tauri::State<'_, AppState>,
    translation_code: String,
    book_id: i64,
    chapter: i64,
    start: Option<i64>,
    end: Option<i64>,
) -> Result<CopyTextDto, String> {
    copy_text(&state, &translation_code, book_id, chapter, start, end).await
}

/// The logic behind `get_copy_text`. A translation, book or chapter that
/// doesn't exist, verses that aren't a range from 1 up, or a passage with
/// no verse the translation numbers, is an error. Verses it doesn't number
/// inside the range are skipped, and the reference still names the range.
pub async fn copy_text(
    state: &AppState,
    translation_code: &str,
    book_id: i64,
    chapter: i64,
    start: Option<i64>,
    end: Option<i64>,
) -> Result<CopyTextDto, String> {
    let translation = state
        .translations
        .iter()
        .find(|t| t.code == translation_code)
        .ok_or_else(|| format!("There's no translation \"{translation_code}\"."))?;
    let book = state
        .books
        .iter()
        .find(|b| b.id == book_id)
        .ok_or_else(|| format!("There's no book {book_id}."))?;
    if !(1..=book.chapter_count).contains(&chapter) {
        return Err(format!("{} has no chapter {chapter}.", book.name));
    }
    let range = match (start, end) {
        (None, None) => None,
        (Some(start), Some(end)) if start >= 1 && end >= start => Some((start, end)),
        _ => return Err("Those verses aren't a passage.".to_string()),
    };
    let verses: Vec<db::Verse> = state
        .store
        .chapter_verses(translation.id, book_id, chapter)
        .await
        .map_err(|e| {
            tracing::error!("Failed to read {} {chapter}: {e}", book.name);
            format!("{} {chapter} couldn't be read.", book.name)
        })?
        .into_iter()
        .filter(|v| range.is_none_or(|(start, end)| (start..=end).contains(&v.verse)))
        .collect();
    let reference = passage_reference(&book.name, chapter, range);
    if verses.is_empty() {
        return Err(format!(
            "{reference} isn't numbered in the {}.",
            translation.name
        ));
    }
    let citation = format!("{reference} ({})", translation.code.to_uppercase());
    let passage = passage_copy::Passage {
        translation_code: &translation.code,
        citation: &citation,
        verses: &verses,
    };
    Ok(CopyTextDto {
        reference,
        translation_code: translation.code.clone(),
        formats: CopyFormatsDto {
            reference: passage.render(CopyFormat::Reference),
            numbered: passage.render(CopyFormat::Numbered),
            markdown: passage.render(CopyFormat::Markdown),
            plain: passage.render(CopyFormat::Plain),
        },
    })
}

/// The verses from `start` to `end` that aren't in `numbered`, in order, as
/// runs `(first, last)`. Only verses up to `last_anywhere` (the chapter's
/// last verse in any translation) are looked at one by one; those past it,
/// however far `end` goes, are one run.
fn missing_runs(numbered: &[i64], start: i64, end: i64, last_anywhere: i64) -> Vec<(i64, i64)> {
    let mut runs: Vec<(i64, i64)> = Vec::new();
    let mut add = |first: i64, last: i64| match runs.last_mut() {
        Some((_, run_end)) if *run_end + 1 == first => *run_end = last,
        _ => runs.push((first, last)),
    };
    for n in start..=end.min(last_anywhere) {
        if !numbered.contains(&n) {
            add(n, n);
        }
    }
    if end > last_anywhere {
        add(start.max(last_anywhere + 1), end);
    }
    runs
}

/// "4", "4, 6" or "4–6, 9": runs of verse numbers, a run of more than one
/// as a range.
fn verse_list(runs: &[(i64, i64)]) -> String {
    runs.iter()
        .map(|&(a, b)| {
            if a == b {
                a.to_string()
            } else {
                format!("{a}\u{2013}{b}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tests::app_state;
    use crate::commands::*;
    use crate::db::test_support::{bundled, bundled_bsb};
    use crate::domain::kjv_markup::{PartKind, VerseText};

    /// A previewed range is every verse in it, in order, with its
    /// reference and translation, and nothing missing.
    #[tokio::test]
    async fn a_passage_is_every_verse_of_its_range() {
        let (_db, state) = app_state("cmd_passage_range").await;
        let dto = passage_text(&state, "web", 62, 4, 9, 10).await.unwrap();
        assert_eq!(dto.reference, "1 John 4:9\u{2013}10");
        assert_eq!(dto.translation_code, "web");
        assert_eq!(
            (dto.book_id, dto.chapter, dto.start, dto.end),
            (62, 4, 9, 10)
        );
        let web = bundled("web");
        let texts: Vec<(i64, &str)> = dto
            .verses
            .iter()
            .map(|v| (v.number, v.shown.text.as_str()))
            .collect();
        assert_eq!(
            texts,
            [
                (9, web[61].chapters[3][8].as_str()),
                (10, web[61].chapters[3][9].as_str())
            ]
        );
        assert_eq!(dto.notice, None);
        let json = serde_json::to_value(&dto).unwrap();
        for key in [
            "bookId",
            "chapter",
            "start",
            "end",
            "reference",
            "translationCode",
            "verses",
            "notice",
        ] {
            assert!(json.get(key).is_some(), "{key}");
        }
        assert_eq!(json["verses"][0]["parts"][0]["kind"], "text");
    }

    /// A KJV preview keeps the supplied words in italics' parts but drops
    /// the margin notes, markers and footnotes alike; its plain text is the
    /// reader's, and the stored verse is untouched.
    #[tokio::test]
    async fn a_kjv_passage_keeps_supplied_words_and_drops_margin_notes() {
        let (_db, state) = app_state("cmd_passage_kjv").await;
        // John 3:5 has supplied words, and 3:3 a margin note.
        let dto = passage_text(&state, "kjv", 43, 3, 3, 5).await.unwrap();
        let kjv = bundled("kjv");
        for v in &dto.verses {
            let stored = &kjv[42].chapters[2][(v.number - 1) as usize];
            let shown = VerseText::new("kjv", stored);
            assert_eq!(v.shown.text, shown.text, "{}", v.number);
            assert!(v.shown.footnotes.is_empty());
            assert!(v.shown.parts.iter().all(|p| p.kind != PartKind::Note));
            let kept: Vec<_> = shown
                .parts
                .iter()
                .filter(|p| p.kind != PartKind::Note)
                .collect();
            assert_eq!(v.shown.parts.iter().collect::<Vec<_>>(), kept);
        }
        assert!(
            dto.verses[0].shown.footnotes.is_empty()
                && VerseText::new("kjv", &kjv[42].chapters[2][2])
                    .footnotes
                    .len()
                    == 1
        );
        assert!(
            dto.verses[2]
                .shown
                .parts
                .iter()
                .any(|p| p.kind == PartKind::Supplied && p.text == "of")
        );
    }

    /// Verses a translation doesn't number are left out, and the notice
    /// says so, as opening the passage does; the rest still come.
    #[tokio::test]
    async fn a_passage_says_which_verses_a_translation_does_not_number() {
        let (_db, state) = app_state("cmd_passage_missing").await;
        // The WEB numbers Romans 14:24-26; the KJV ends the chapter at 23
        // (it has those verses at 16:25-27).
        let kjv = passage_text(&state, "kjv", 45, 14, 24, 26).await.unwrap();
        assert!(kjv.verses.is_empty());
        assert_eq!(
            kjv.notice.as_deref(),
            Some(
                "Romans 14:24\u{2013}26 isn't numbered in the King James Version \u{2014} \
                 Romans 14 ends at verse 23 there."
            )
        );
        let partly = passage_text(&state, "kjv", 45, 14, 23, 25).await.unwrap();
        assert_eq!(partly.verses.len(), 1);
        assert_eq!(partly.verses[0].number, 23);
        assert_eq!(
            partly.notice.as_deref(),
            Some(
                "Romans 14:24\u{2013}25 isn't numbered in the King James Version \u{2014} \
                 Romans 14 ends at verse 23 there."
            )
        );
        let web = passage_text(&state, "web", 45, 14, 24, 26).await.unwrap();
        assert_eq!(web.verses.len(), 3);
        assert_eq!(web.notice, None);
        assert_eq!(
            verse_list(&missing_runs(&[5, 9, 10], 4, 11, 20)),
            "4, 6\u{2013}8, 11"
        );
    }

    /// However far past the chapter's end a passage reaches, it's answered
    /// at once: the verses past the end in every translation are named as
    /// one range, never counted out.
    #[tokio::test]
    async fn a_passage_past_the_end_everywhere_is_named_not_counted() {
        let (_db, state) = app_state("cmd_passage_far").await;
        let far = passage_text(&state, "kjv", 43, 3, 35, i64::MAX)
            .await
            .unwrap();
        assert_eq!(
            far.verses.iter().map(|v| v.number).collect::<Vec<_>>(),
            [35, 36]
        );
        assert_eq!(
            far.notice.as_deref(),
            Some(
                "John 3:37\u{2013}9223372036854775807 isn't numbered in the King James \
                 Version \u{2014} John 3 ends at verse 36 there."
            )
        );
        let none = passage_text(&state, "kjv", 43, 3, i64::MAX, i64::MAX)
            .await
            .unwrap();
        assert!(none.verses.is_empty());
        assert_eq!(
            none.notice.as_deref(),
            Some(
                "John 3:9223372036854775807 isn't numbered in the King James Version \
                 \u{2014} John 3 ends at verse 36 there."
            )
        );
        // A gap inside the chapter, and the end past it everywhere, are
        // both named. The KJV ends Romans 14 at 23; the WEB at 26.
        let web = passage_text(&state, "web", 45, 14, 25, 1_000_000)
            .await
            .unwrap();
        assert_eq!(web.verses.len(), 2);
        assert_eq!(
            web.notice.as_deref(),
            Some(
                "Romans 14:27\u{2013}1000000 isn't numbered in the World English Bible \
                 \u{2014} Romans 14 ends at verse 26 there."
            )
        );
        assert_eq!(
            missing_runs(&[2, 3], 1, i64::MAX, 3),
            [(1, 1), (4, i64::MAX)]
        );
        assert_eq!(missing_runs(&[], 5, i64::MAX, 3), [(5, i64::MAX)]);
        assert_eq!(missing_runs(&[1, 2, 3], 1, 3, 3), []);
    }

    /// A translation, book or chapter that doesn't exist, or verses that
    /// aren't a range, are refused with a reason.
    #[tokio::test]
    async fn a_passage_refuses_what_is_not_one() {
        let (_db, state) = app_state("cmd_passage_invalid").await;
        for (code, book, chapter, start, end) in [
            ("niv", 43, 3, 16, 16),
            ("kjv", 0, 1, 1, 1),
            ("kjv", 67, 1, 1, 1),
            ("kjv", 43, 0, 1, 1),
            ("kjv", 43, 22, 1, 1),
            ("kjv", 43, 3, 0, 1),
            ("kjv", 43, 3, 17, 16),
        ] {
            let result = passage_text(&state, code, book, chapter, start, end).await;
            assert!(result.is_err(), "{code} {book} {chapter}:{start}-{end}");
        }
        assert_eq!(
            passage_text(&state, "kjv", 43, 22, 1, 1).await.unwrap_err(),
            "John has no chapter 22."
        );
    }

    /// Copy as…'s four formats, from the database, for the passages the
    /// release notes show: John 3:16–17 in the WEB and Psalm 23:1 in the
    /// KJV (its title, brackets and all, and its supplied word).
    #[tokio::test]
    async fn copy_text_has_every_format_of_a_passage() {
        let (_db, state) = app_state("cmd_copy_formats").await;
        let dto = copy_text(&state, "web", 43, 3, Some(16), Some(17))
            .await
            .unwrap();
        let web = bundled("web");
        let (v16, v17) = (&web[42].chapters[2][15], &web[42].chapters[2][16]);
        assert_eq!(dto.reference, "John 3:16\u{2013}17");
        assert_eq!(dto.translation_code, "web");
        assert_eq!(
            dto.formats.reference,
            format!("{v16} {v17} \u{2014} John 3:16\u{2013}17 (WEB)")
        );
        assert_eq!(
            dto.formats.numbered,
            format!("16 {v16}\n17 {v17}\n\u{2014} John 3:16\u{2013}17 (WEB)")
        );
        assert_eq!(
            dto.formats.markdown,
            format!("> **16** {v16} **17** {v17}\n>\n> \u{2014} John 3:16\u{2013}17 (WEB)")
        );
        assert_eq!(dto.formats.plain, format!("{v16} {v17}"));

        let psalm = copy_text(&state, "kjv", 19, 23, Some(1), Some(1))
            .await
            .unwrap();
        let one = "[A Psalm of David.] The LORD is my shepherd; I shall not want.";
        assert_eq!(
            psalm.formats.reference,
            format!("{one} \u{2014} Psalms 23:1 (KJV)")
        );
        assert_eq!(
            psalm.formats.numbered,
            format!("1 {one}\n\u{2014} Psalms 23:1 (KJV)")
        );
        assert_eq!(
            psalm.formats.markdown,
            "> **1** \\[A Psalm of David.\\] The LORD *is* my shepherd; I shall not want.\n>\n\
             > \u{2014} Psalms 23:1 (KJV)"
        );
        assert_eq!(psalm.formats.plain, one);

        let json = serde_json::to_value(&dto).unwrap();
        for key in ["reference", "numbered", "markdown", "plain"] {
            assert!(json["formats"][key].is_string(), "{key}");
        }
        assert_eq!(json["translationCode"], "web");
    }

    /// Copy (and Ctrl+C) copies what it always has: the verses' plain text
    /// as the reader shows it, joined by spaces, then the citation.
    #[tokio::test]
    async fn copy_text_with_a_reference_is_what_copy_always_copied() {
        let (_db, state) = app_state("cmd_copy_as_before").await;
        for (code, book, chapter, start, end, citation) in [
            ("web", 43, 3, 16, 18, "John 3:16\u{2013}18 (WEB)"),
            ("kjv", 19, 119, 105, 106, "Psalms 119:105\u{2013}106 (KJV)"),
            ("bsb", 40, 17, 20, 22, "Matthew 17:20\u{2013}22 (BSB)"),
        ] {
            let shown = reading(&state, book, chapter, Some(code))
                .await
                .unwrap()
                .unwrap();
            let text = shown
                .current_verses
                .iter()
                .filter(|v| (start..=end).contains(&v.number))
                .map(|v| v.shown.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let dto = copy_text(&state, code, book, chapter, Some(start), Some(end))
                .await
                .unwrap();
            assert_eq!(dto.formats.reference, format!("{text} \u{2014} {citation}"));
        }
    }

    /// The KJV's margin notes and the BSB's footnotes are in no format:
    /// Psalm 119:105 ("lamp: or, candle") and Matthew 17:20 (whose note
    /// gives verse 21's wording).
    #[tokio::test]
    async fn copy_text_never_has_a_footnote() {
        let (_db, state) = app_state("cmd_copy_footnotes").await;
        let kjv = copy_text(&state, "kjv", 19, 119, Some(105), Some(105))
            .await
            .unwrap();
        let bsb = copy_text(&state, "bsb", 40, 17, Some(20), Some(20))
            .await
            .unwrap();
        let stored = bundled_bsb()[39].chapters[16][19].clone().unwrap();
        assert_eq!(bsb.formats.plain, stored);
        for (dto, note) in [(&kjv, "candle"), (&bsb, "fasting")] {
            let f = &dto.formats;
            for text in [&f.reference, &f.numbered, &f.markdown, &f.plain] {
                assert!(!text.contains(note), "{text}");
                assert!(!text.contains('{') && !text.contains("\\f"), "{text}");
            }
        }
        // The section's letter is the verse's own, as stored.
        assert_eq!(
            kjv.formats.plain,
            "NUN. Thy word is a lamp unto my feet, and a light unto my path."
        );
    }

    /// A verse the translation doesn't number is skipped without comment,
    /// in a range and in a whole chapter, which is every verse it numbers,
    /// a verse to a line with numbers, named "Matthew 17".
    #[tokio::test]
    async fn copy_text_skips_unnumbered_verses_and_copies_whole_chapters() {
        let (_db, state) = app_state("cmd_copy_chapter").await;
        let bsb = bundled_bsb();
        let text = |n: usize| bsb[39].chapters[16][n - 1].clone().unwrap();
        let range = copy_text(&state, "bsb", 40, 17, Some(20), Some(22))
            .await
            .unwrap();
        assert_eq!(range.reference, "Matthew 17:20\u{2013}22");
        assert_eq!(
            range.formats.numbered,
            format!(
                "20 {}\n22 {}\n\u{2014} Matthew 17:20\u{2013}22 (BSB)",
                text(20),
                text(22)
            )
        );

        let chapter = copy_text(&state, "bsb", 40, 17, None, None).await.unwrap();
        assert_eq!(chapter.reference, "Matthew 17");
        let numbers: Vec<usize> = (1..=27).filter(|&n| n != 21).collect();
        let lines: Vec<String> = numbers
            .iter()
            .map(|&n| format!("{n} {}", text(n)))
            .collect();
        assert_eq!(
            chapter.formats.numbered,
            format!("{}\n\u{2014} Matthew 17 (BSB)", lines.join("\n"))
        );
        let all: Vec<String> = numbers.iter().map(|&n| text(n)).collect();
        assert_eq!(chapter.formats.plain, all.join(" "));
        assert_eq!(
            chapter.formats.reference,
            format!("{} \u{2014} Matthew 17 (BSB)", all.join(" "))
        );
        assert!(chapter.formats.markdown.starts_with("> **1** "));
        assert!(chapter.formats.markdown.contains(" **22** "));
        assert!(!chapter.formats.markdown.contains("**21**"));
        assert!(
            chapter
                .formats
                .markdown
                .ends_with("\n>\n> \u{2014} Matthew 17 (BSB)")
        );

        // A whole KJV chapter: every verse, as the reader shows it.
        let kjv = copy_text(&state, "kjv", 19, 23, None, None).await.unwrap();
        let shown = reading(&state, 19, 23, Some("kjv")).await.unwrap().unwrap();
        let lines: Vec<String> = shown
            .current_verses
            .iter()
            .map(|v| format!("{} {}", v.number, v.shown.text))
            .collect();
        assert_eq!(lines.len(), 6);
        assert_eq!(
            kjv.formats.numbered,
            format!("{}\n\u{2014} Psalms 23 (KJV)", lines.join("\n"))
        );
    }

    /// What isn't a passage, or has no verse the translation numbers, is
    /// refused with a reason.
    #[tokio::test]
    async fn copy_text_refuses_what_is_not_a_passage() {
        let (_db, state) = app_state("cmd_copy_invalid").await;
        for (code, book, chapter, start, end) in [
            ("niv", 43, 3, Some(16), Some(16)),
            ("kjv", 67, 1, None, None),
            ("kjv", 43, 22, None, None),
            ("kjv", 43, 3, Some(0), Some(1)),
            ("kjv", 43, 3, Some(17), Some(16)),
            ("kjv", 43, 3, Some(16), None),
        ] {
            let result = copy_text(&state, code, book, chapter, start, end).await;
            assert!(result.is_err(), "{code} {book} {chapter}:{start:?}-{end:?}");
        }
        assert_eq!(
            copy_text(&state, "bsb", 40, 17, Some(21), Some(21))
                .await
                .unwrap_err(),
            "Matthew 17:21 isn't numbered in the Berean Standard Bible."
        );
    }
}
