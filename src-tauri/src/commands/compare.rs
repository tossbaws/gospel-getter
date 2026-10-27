//! Compare translations (GH-13): one chapter in several translations,
//! side by side.

use anyhow::Context;
use serde::Serialize;

use super::{AppState, SimpleVerseDto, failed};
use crate::db::Translation;

// ---- Compare translations (GH-13)

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareColumnDto {
    pub code: String,
    pub name: String,
    /// The chapter's verses as this translation numbers them, each as
    /// shown (see `VerseText`).
    pub verses: Vec<SimpleVerseDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareDto {
    pub book_id: i64,
    pub chapter: i64,
    pub columns: Vec<CompareColumnDto>,
}

/// One chapter in the translations `codes`, in that order, for showing
/// side by side: the one being read, then the one it's compared with.
/// Unlike `get_reading`, it doesn't move the reading position. `None` for
/// a book or chapter that doesn't exist; an error for a translation that
/// doesn't, or one named twice.
pub async fn compare_chapter(
    state: &AppState,
    book_id: i64,
    chapter: i64,
    codes: &[String],
) -> anyhow::Result<Option<CompareDto>> {
    let mut translations = Vec::with_capacity(codes.len());
    for code in codes {
        let t = state
            .translations
            .iter()
            .find(|t| &t.code == code)
            .ok_or_else(|| anyhow::anyhow!("There's no translation \"{code}\"."))?;
        if translations
            .iter()
            .any(|seen: &&Translation| seen.id == t.id)
        {
            anyhow::bail!("{} is named twice.", t.name);
        }
        translations.push(t);
    }
    if translations.is_empty() {
        anyhow::bail!("No translations to compare.");
    }
    let Some(book) = state.books.iter().find(|b| b.id == book_id) else {
        return Ok(None);
    };
    if !(1..=book.chapter_count).contains(&chapter) {
        return Ok(None);
    }
    let mut columns = Vec::with_capacity(translations.len());
    for t in translations {
        let verses = state.store.chapter_verses(t.id, book_id, chapter).await?;
        columns.push(CompareColumnDto {
            code: t.code.clone(),
            name: t.name.clone(),
            verses: verses
                .into_iter()
                .map(|v| SimpleVerseDto::new(&t.code, v))
                .collect(),
        });
    }
    Ok(Some(CompareDto {
        book_id,
        chapter,
        columns,
    }))
}

#[tauri::command]
pub async fn get_compare(
    state: tauri::State<'_, AppState>,
    book_id: i64,
    chapter: i64,
    translation_codes: Vec<String>,
) -> Result<Option<CompareDto>, String> {
    compare_chapter(&state, book_id, chapter, &translation_codes)
        .await
        .with_context(|| format!("Failed to load {book_id}:{chapter} for comparison"))
        .map_err(failed("The comparison couldn't be loaded."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tests::app_state;
    use crate::db::test_support::{bundled, bundled_bsb};
    use crate::domain::kjv_markup::VerseText;

    #[tokio::test]
    async fn compare_returns_each_translations_own_numbering_and_text() {
        let (_db, state) = app_state("cmd_compare").await;
        let pair = |a: &str, b: &str| vec![a.to_string(), b.to_string()];

        // Romans 14: 23 verses in the KJV, 26 in the WEB (which places the
        // doxology the KJV has at 16:25-27 here, as 14:24-26).
        let dto = compare_chapter(&state, 45, 14, &pair("kjv", "web"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(dto.columns.len(), 2);
        for (column, code) in dto.columns.iter().zip(["kjv", "web"]) {
            assert_eq!(column.code, code);
            let stored = &bundled(code)[44].chapters[13];
            assert_eq!(column.verses.len(), stored.len());
            for (i, (verse, text)) in column.verses.iter().zip(stored).enumerate() {
                assert_eq!(verse.number, i as i64 + 1);
                // Each as shown: the WEB exactly as stored.
                assert_eq!(verse.shown, VerseText::new(code, text));
            }
        }
        assert_eq!(dto.columns[0].verses.len(), 23);
        assert_eq!(dto.columns[1].verses.len(), 26);

        // Any two, in the order asked: the one being read first. The BSB
        // doesn't number Matthew 17:21, mid-chapter; its column skips it.
        let dto = compare_chapter(&state, 40, 17, &pair("bsb", "kjv"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            dto.columns
                .iter()
                .map(|c| c.code.as_str())
                .collect::<Vec<_>>(),
            ["bsb", "kjv"]
        );
        let numbers = |c: &CompareColumnDto| c.verses.iter().map(|v| v.number).collect::<Vec<_>>();
        assert_eq!(
            numbers(&dto.columns[0]),
            (1..=27).filter(|&n| n != 21).collect::<Vec<_>>()
        );
        assert_eq!(numbers(&dto.columns[1]), (1..=27).collect::<Vec<_>>());
        let bsb = bundled_bsb();
        for verse in &dto.columns[0].verses {
            assert_eq!(
                Some(&verse.shown.text),
                bsb[39].chapters[16][verse.number as usize - 1].as_ref()
            );
        }
        // The BSB's footnotes show in its column.
        assert!(
            dto.columns[0].verses[19].shown.footnotes[0]
                .text
                .contains("BYZ and TR include")
        );

        // Translations that aren't there, or one twice, are refused.
        for codes in [pair("kjv", "niv"), pair("web", "web"), Vec::new()] {
            assert!(
                compare_chapter(&state, 45, 14, &codes).await.is_err(),
                "{codes:?}"
            );
        }
        assert!(
            compare_chapter(&state, 43, 22, &pair("kjv", "web"))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            compare_chapter(&state, 99, 1, &pair("kjv", "web"))
                .await
                .unwrap()
                .is_none()
        );

        // Comparing doesn't move the saved reading position.
        assert_eq!(state.store.reading_position().await.unwrap(), None);
    }
}
