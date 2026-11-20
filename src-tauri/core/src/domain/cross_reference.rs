//! A cross-reference's target as every surface shows and opens it: the
//! reader's lists, its preview and Go to (through `get_reading`), and
//! `gospel-getter-cli`'s `get_cross_references`.

use crate::passage_reference::passage_reference;

/// The verses a cross-reference opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub verse_start: i64,
    /// `verse_start` for a single verse, and for a range that isn't
    /// available.
    pub verse_end: i64,
    /// Whether the data gives a range this shows only the first verse of
    /// (said with [`RANGE_UNAVAILABLE`]).
    pub range_unavailable: bool,
}

pub use crate::passage_reference::RANGE_UNAVAILABLE;

/// The verses a cross-reference to `verse` (through `end_verse`, if the data
/// gives an end) of a chapter whose last verse is `last_verse` opens.
///
/// A range is shown as one only when its end is after its start and within
/// the chapter, `last_verse` being the highest any installed translation
/// numbers there (`chapter_last_verse`, the limit a bookmark's range is
/// checked against). Otherwise it's the start verse alone, marked
/// unavailable. The bundled data gives a range's ending verse but not its
/// ending chapter, so a range that crosses into the next chapter arrives as
/// "Hebrews 6:20–3" or "Job 40:15–34" (Job 40 ends at 24). The end is never
/// inferred from the chapter's length and the entry is never changed or
/// dropped: which end was meant is unknown until the upstream file is
/// identified and pinned (AF10, `sources/cross_references/README.md`).
pub fn target(verse: i64, end_verse: Option<i64>, last_verse: i64) -> Target {
    match end_verse {
        Some(end) if end > verse && end <= last_verse => Target {
            verse_start: verse,
            verse_end: end,
            range_unavailable: false,
        },
        Some(end) if end != verse => Target {
            verse_start: verse,
            verse_end: verse,
            range_unavailable: true,
        },
        _ => Target {
            verse_start: verse,
            verse_end: verse,
            range_unavailable: false,
        },
    }
}

/// A cross-reference's name: "Romans 8:28" or "Romans 8:28–30", with an en
/// dash, as every passage is named (`passage_reference`). One whose range
/// isn't available is named by its start verse; [`RANGE_UNAVAILABLE`] is
/// said beside it, not in it, so the name stays one Go to reads.
pub fn label(book_name: &str, chapter: i64, target: Target) -> String {
    passage_reference(
        book_name,
        chapter,
        Some((target.verse_start, target.verse_end)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::prepared_database;

    #[test]
    fn a_range_that_fits_its_chapter_is_a_range() {
        let t = target(28, Some(30), 39);
        assert_eq!(
            t,
            Target {
                verse_start: 28,
                verse_end: 30,
                range_unavailable: false,
            }
        );
        assert_eq!(label("Romans", 8, t), "Romans 8:28\u{2013}30");
        // Its end on the chapter's last verse still fits.
        assert!(!target(15, Some(24), 24).range_unavailable);
    }

    #[test]
    fn a_single_verse_is_one_verse() {
        for t in [target(28, None, 39), target(28, Some(28), 39)] {
            assert_eq!(
                t,
                Target {
                    verse_start: 28,
                    verse_end: 28,
                    range_unavailable: false,
                }
            );
            assert_eq!(label("Romans", 8, t), "Romans 8:28");
        }
    }

    #[test]
    fn a_range_that_ends_before_it_starts_is_its_start_verse_marked() {
        // Genesis 14:18's "Hebrews 6:20–3".
        let t = target(20, Some(3), 20);
        assert_eq!(
            t,
            Target {
                verse_start: 20,
                verse_end: 20,
                range_unavailable: true,
            }
        );
        assert_eq!(label("Hebrews", 6, t), "Hebrews 6:20");
    }

    #[test]
    fn a_range_that_ends_past_its_chapter_is_its_start_verse_marked() {
        // Psalm 8:8's "Job 40:15–34"; Job 40 ends at 24.
        let t = target(15, Some(34), 24);
        assert!(t.range_unavailable);
        assert_eq!(label("Job", 40, t), "Job 40:15");
    }

    /// Every bundled cross-reference through the rule, as a new install
    /// seeds them: 214 of the 133,770 give a range that can't be shown, 196
    /// ending before they start and 18 past their chapter's last verse. A
    /// change to the data, or to the rule, shows up here.
    #[tokio::test]
    async fn the_bundled_data_has_214_ranges_that_cant_be_shown() {
        let (_db, pool) = prepared_database("xref_corpus").await;
        let books = crate::db::get_all_books(pool.read()).await.unwrap();
        let (mut all, mut before, mut past) = (0, 0, 0);
        for book in &books {
            for chapter in 1..=book.chapter_count {
                let found = crate::db::get_chapter_cross_references(pool.read(), book.id, chapter)
                    .await
                    .unwrap();
                for r in found {
                    all += 1;
                    let t = r.target();
                    assert!(t.verse_start <= r.ref_chapter_last_verse, "{r:?}");
                    if !t.range_unavailable {
                        continue;
                    }
                    let end = r.ref_end_verse.expect("only a range is unavailable");
                    if end < r.ref_verse {
                        before += 1;
                    } else {
                        assert!(end > r.ref_chapter_last_verse, "{r:?}");
                        past += 1;
                    }
                }
            }
        }
        assert_eq!(all, 133_770);
        assert_eq!((before, past), (196, 18));
        pool.close().await;
    }
}
