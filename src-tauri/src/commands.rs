//! The commands the page calls with `invoke`, and the state they share
//! (`AppState`). Each `#[tauri::command]` is a thin wrapper over a function
//! taking `&AppState` (`get_reading` over `reading`), which the frontend
//! tests' bridge (`examples/frontend_bridge.rs`) calls directly. A failure
//! reaches the page as a sentence for the reader; its cause is logged.
//!
//! One module per area of the app, each with its own tests; everything they
//! make public is re-exported here, so a command is `commands::get_reading`
//! wherever it's named (`lib.rs`'s handler list, the bridge, the tests):
//!
//! - `state`: `AppState` and the search index's progress.
//! - `lookup`: a book's name, a translation's id or code, and `failed`.
//! - `reading`: the home screen, the chapter being read and the saved position.
//! - `passages`: a passage read in place (a cross-reference) or copied.
//! - `bookmarks`, `highlights`, `notes`: the reader's own marks on verses.
//! - `search`: search, and going to a reference.
//! - `compare`: a chapter in several translations side by side.
//! - `backup`: exporting and importing the reader's data.
//! - `export_notes`: the notes as Markdown.
//! - `files`: saving a file completely or not at all, and the file dialog.

mod backup;
mod bookmarks;
mod compare;
mod export_notes;
mod files;
mod highlights;
mod lookup;
mod notes;
mod passages;
mod reading;
mod search;
mod state;

pub use backup::*;
pub use bookmarks::*;
pub use compare::*;
pub use export_notes::*;
pub(crate) use files::*;
pub use highlights::*;
use lookup::*;
pub use notes::*;
pub use passages::*;
pub use reading::*;
pub use search::*;
pub use state::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Passage;
    use crate::db::test_support::{bundled, bundled_bsb, fresh_database};
    use crate::domain::kjv_markup::{PartKind, VerseText};
    use crate::notes_export::NotesFilter;

    pub(super) async fn app_state(label: &str) -> (crate::db::test_support::TempDb, AppState) {
        let (db, pool) = fresh_database(label).await;
        let state = AppState::load(pool).await.expect("load state");
        state.search_index.set_ready();
        (db, state)
    }

    pub(super) async fn search_in(state: &AppState, query: &str, code: &str) -> serde_json::Value {
        let dto = run_search(state, query, Some(code), 0)
            .await
            .expect("search");
        serde_json::to_value(dto).expect("serialize")
    }

    #[tokio::test]
    async fn kjv_verses_are_shown_clean_everywhere_and_the_web_as_stored() {
        let (_db, state) = app_state("cmd_kjv_shown").await;
        let psalm_119_105 =
            "NUN. Thy word is a lamp unto my feet, and a light unto my path.".to_string();

        // The chapter being read (which the note panel quotes from too):
        // plain text, parts and footnotes.
        let dto = reading(&state, 19, 119, Some("kjv"))
            .await
            .unwrap()
            .unwrap();
        let v105 = serde_json::to_value(&dto.current_verses[104]).unwrap();
        assert_eq!(
            v105,
            serde_json::json!({
                "number": 105,
                "text": psalm_119_105,
                "parts": [
                    {"kind": "text", "text": "NUN. Thy word "},
                    {"kind": "supplied", "text": "is"},
                    {"kind": "text", "text": " a lamp unto my feet, and a light unto my path."},
                    {"kind": "note", "text": "lamp: or, candle", "label": "a"},
                ],
                "footnotes": [{
                    "label": "a",
                    "lemma": "lamp",
                    "rest": ": or, candle",
                    "text": "lamp: or, candle",
                }],
                "xrefs": v105["xrefs"],
            })
        );
        // A note without a colon has no lemma: Psalms 73:22's second.
        let psalm_73 = reading(&state, 19, 73, Some("kjv")).await.unwrap().unwrap();
        assert_eq!(
            serde_json::to_value(&psalm_73.current_verses[21].shown.footnotes).unwrap(),
            serde_json::json!([
                {
                    "label": "a",
                    "lemma": "ignorant",
                    "rest": ": Heb. I knew not",
                    "text": "ignorant: Heb. I knew not",
                },
                {"label": "b", "lemma": null, "rest": "before Heb. with", "text": "before Heb. with"},
            ])
        );
        assert!(!v105["xrefs"].as_array().unwrap().is_empty());
        let kjv = bundled("kjv");
        let every_verse = dto
            .current_verses
            .iter()
            .map(|v| (&v.shown, v.number))
            .chain(
                dto.prev
                    .iter()
                    .chain(&dto.next)
                    .flat_map(|side| side.verses.iter().map(|v| (&v.shown, v.number))),
            );
        for (shown, _) in every_verse {
            assert!(!shown.text.contains(['{', '}']), "{}", shown.text);
        }
        // The neighbors are shown the same way (Psalms 118 and 120).
        let prev = dto.prev.as_ref().unwrap();
        assert_eq!(
            prev.verses[0].shown,
            VerseText::new("kjv", &kjv[18].chapters[117][0])
        );

        // A cross-reference previewed (without its margin note), and the
        // comparison's KJV column.
        let previewed = passage_text(&state, "kjv", 19, 119, 105, 105)
            .await
            .unwrap();
        assert_eq!(previewed.verses[0].shown.text, psalm_119_105);
        assert!(previewed.verses[0].shown.footnotes.is_empty());
        let codes = ["kjv".to_string(), "web".to_string()];
        let compare = compare_chapter(&state, 19, 119, &codes)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(compare.columns[0].verses[104].shown.text, psalm_119_105);

        // Library previews.
        bookmark_passage(&state, 19, 119, 105, 106).await.unwrap();
        highlight_passage(&state, 19, 119, 105, 105, "yellow")
            .await
            .unwrap();
        let bookmarks = bookmarks_in(&state, Some("kjv")).await.unwrap();
        assert_eq!(
            bookmarks[0].preview.as_deref(),
            Some(psalm_119_105.as_str())
        );
        let highlights = highlight_passages_in(&state, Some("kjv")).await.unwrap();
        assert_eq!(
            highlights[0].preview.as_deref(),
            Some(psalm_119_105.as_str())
        );

        // A malformed verse is shown exactly as stored (Hebrews 10:34).
        let hebrews = reading(&state, 58, 10, Some("kjv")).await.unwrap().unwrap();
        let stored = &kjv[57].chapters[9][33];
        assert_eq!(
            hebrews.current_verses[33].shown,
            VerseText::as_stored(stored)
        );

        // The WEB, everywhere, exactly as stored: one text part, no
        // footnotes.
        let web = bundled("web");
        let dto = reading(&state, 19, 119, Some("web"))
            .await
            .unwrap()
            .unwrap();
        for (v, stored) in dto.current_verses.iter().zip(&web[18].chapters[118]) {
            assert_eq!(v.shown, VerseText::as_stored(stored));
        }
        let previewed = passage_text(&state, "web", 19, 119, 105, 105)
            .await
            .unwrap();
        assert_eq!(
            previewed.verses[0].shown,
            VerseText::as_stored(&web[18].chapters[118][104])
        );
        let bookmarks = bookmarks_in(&state, Some("web")).await.unwrap();
        assert_eq!(
            bookmarks[0].preview.as_ref(),
            Some(&web[18].chapters[118][104])
        );
    }

    /// The BSB: listed between the KJV and the WEB (the KJV still the
    /// default), read with its footnotes kept apart from its text, and
    /// working in every place a translation's verses are used, including
    /// across the verses it doesn't number in mid-chapter.
    #[tokio::test]
    async fn the_bsb_is_a_translation_like_the_others_with_its_own_footnotes() {
        use crate::notes_export::escape_block;
        let (_db, state) = app_state("cmd_bsb").await;
        let bsb = bundled_bsb();
        let text = |b: usize, c: usize, v: usize| {
            bsb[b - 1].chapters[c - 1][v - 1].clone().expect("numbered")
        };

        // The list, in order, with what Aa's chooser shows.
        let home = home(&state).await;
        let listed: Vec<_> = home
            .translations
            .iter()
            .map(|t| {
                (
                    t.code.as_str(),
                    t.name.as_str(),
                    t.date.as_deref(),
                    t.licence.as_str(),
                )
            })
            .collect();
        assert_eq!(
            listed,
            [
                (
                    "kjv",
                    "King James Version",
                    Some("1611/1769"),
                    "public domain"
                ),
                (
                    "bsb",
                    "Berean Standard Bible",
                    Some("2023"),
                    "public domain"
                ),
                ("web", "World English Bible", None, "public domain"),
            ]
        );
        assert_eq!(
            home.translations[1].credit,
            "Berean Standard Bible (BSB), public domain, berean.bible"
        );
        assert_eq!(home.current_translation_code, "kjv");

        // Genesis 1: each verse is its stored text, and verse 3's note
        // stands after “light,”, lettered, its words as Berean's.
        let dto = reading(&state, 1, 1, Some("bsb")).await.unwrap().unwrap();
        for v in &dto.current_verses {
            assert_eq!(v.shown.text, text(1, 1, v.number as usize));
        }
        let v3 = &dto.current_verses[2].shown;
        let kinds: Vec<_> = v3.parts.iter().map(|p| (p.kind, p.text.as_str())).collect();
        assert_eq!(
            kinds,
            [
                (PartKind::Text, "And God said, “Let there be light,”"),
                (PartKind::Note, "Cited in 2 Corinthians 4:6"),
                (PartKind::Text, " and there was light."),
            ]
        );
        assert_eq!(v3.footnotes[0].label, "a");
        // Verse 6's note: its alternative readings in italics.
        let v6 = &dto.current_verses[5].shown.footnotes[0];
        assert_eq!(
            v6.text,
            "Or a canopy or a firmament or a vault; also in verses 7, 8, 14, 15, 17, and 20"
        );
        let segments = v6.segments.as_ref().unwrap();
        assert_eq!(segments[1].kind, crate::domain::usfm_note::SegmentKind::Alt);
        assert_eq!(segments[1].text, "a canopy ");
        // The faded neighbours carry them too; the frontend leaves their
        // markers out, as for the KJV.
        assert!(
            dto.next
                .as_ref()
                .unwrap()
                .verses
                .iter()
                .any(|v| !v.shown.footnotes.is_empty())
        );

        // Matthew 17 in the BSB: no verse 21, and verse 20's note says so.
        let dto = reading(&state, 40, 17, Some("bsb")).await.unwrap().unwrap();
        let numbers: Vec<i64> = dto.current_verses.iter().map(|v| v.number).collect();
        assert_eq!(numbers, (1..=27).filter(|&n| n != 21).collect::<Vec<_>>());
        assert!(
            dto.current_verses[19].shown.footnotes[0]
                .text
                .starts_with("BYZ and TR include 21But")
        );

        // A preview: the verses it numbers, no notes, and which it doesn't.
        let passage = passage_text(&state, "bsb", 40, 17, 20, 22).await.unwrap();
        assert_eq!(
            passage.verses.iter().map(|v| v.number).collect::<Vec<_>>(),
            [20, 22]
        );
        assert!(passage.verses.iter().all(|v| v.shown.footnotes.is_empty()
            && v.shown.parts.iter().all(|p| p.kind != PartKind::Note)));
        assert_eq!(passage.verses[0].shown.text, text(40, 17, 20));
        assert_eq!(
            passage.notice.as_deref(),
            Some(
                "In Matthew 17:20\u{2013}22, verse 21 isn't numbered in the Berean Standard Bible."
            )
        );
        let missing = passage_text(&state, "bsb", 40, 17, 21, 21).await.unwrap();
        assert!(missing.verses.is_empty());
        assert_eq!(
            missing.notice.as_deref(),
            Some("In Matthew 17:21, verse 21 isn't numbered in the Berean Standard Bible.")
        );

        // Search: the verse text, never the footnotes.
        let v = search_in(&state, "shepherd want", "bsb").await;
        let results = v["results"].as_array().unwrap();
        let psalm = results
            .iter()
            .find(|r| r["reference"] == "Psalms 23:1")
            .unwrap();
        assert_eq!(psalm["text"], text(19, 23, 1));
        let v = search_in(&state, "BYZ", "bsb").await;
        assert_eq!(v["results"].as_array().map_or(0, Vec::len), 0, "{v}");

        // Bookmarks belong to no translation: one across the gap is partly
        // numbered in the BSB, one on the gap not at all.
        bookmark_passage(&state, 40, 17, 20, 22).await.unwrap();
        bookmark_passage(&state, 40, 17, 21, 21).await.unwrap();
        let marks = bookmarks_in(&state, Some("bsb")).await.unwrap();
        let across = marks.iter().find(|b| b.verse_start == 20).unwrap();
        assert!(!across.complete);
        assert_eq!(across.preview.as_deref(), Some(text(40, 17, 20).as_str()));
        let on = marks.iter().find(|b| b.verse_start == 21).unwrap();
        assert_eq!((on.complete, on.preview.as_deref()), (false, None));

        // The notes export quotes the BSB, and says what it doesn't number.
        note_add(&state, passage_at(40, 17, 20, 22), "The gap.")
            .await
            .unwrap();
        let tz = chrono::FixedOffset::east_opt(0).unwrap();
        let file = build_notes_export(
            &state,
            "bsb",
            &NotesFilter::default(),
            chrono::Utc::now(),
            &tz,
        )
        .await
        .unwrap();
        assert!(file.contents.contains("Verse text: Berean Standard Bible"));
        for verse in [20, 22] {
            let quoted = format!("**{verse}** {}", escape_block(&text(40, 17, verse)));
            assert!(file.contents.contains(&quoted), "{quoted}");
        }
        assert!(
            file.contents
                .contains("*Not all of it is numbered in the BSB.*")
        );
        assert!(!file.contents.contains("BYZ"));
    }

    fn passage_at(book_id: i64, chapter: i64, verse_start: i64, verse_end: i64) -> Passage {
        Passage {
            book_id,
            chapter,
            verse_start,
            verse_end,
        }
    }
}
