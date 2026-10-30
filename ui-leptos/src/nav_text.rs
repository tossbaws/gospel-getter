//! The top bar's way-finding controls' words and rules, free of the DOM so
//! they are tested natively (`cargo test`): Back and Forward (what each
//! says, and whether it can go), and Recent places, the last few places the
//! reader went to on purpose (what goes in, the list as it's kept on this
//! computer, read back and checked, and the menu that lists it). `nav`
//! puts them on the page.
//!
//! Recent places are the places the passage history records on a jump (a
//! place chosen in the picker, search, a link, or this menu), and the place
//! being left, newest first: one per chapter, at most [`RECENT_LIMIT`].
//! Plain chapter steps (← and →, the chapter's end) add none. Unlike the
//! history, they last across restarts, in local storage under
//! [`RECENT_KEY`], on this computer only: not a display preference, and in
//! no backup or export.

use serde::{Deserialize, Serialize};

use crate::passage_reference::passage_reference;
use crate::picker_text::Book;
use crate::reader_text::escape_html;
use crate::shortcut_table::shortcut;

/// Recent places' key in local storage.
pub const RECENT_KEY: &str = "gospel-getter-recent-places";

/// The most places kept; the oldest go.
pub const RECENT_LIMIT: usize = 10;

/// The longest value read back from local storage, in UTF-16 code units
/// (about 80 bytes a place, ten places, and room to spare). A longer one
/// isn't this app's, and is dropped whole.
pub const STORED_MAX: u32 = 4096;

/// The window is narrow: Forward folds into the Recent places menu, and
/// the reference drops its translation (`.ref-codes`), which the
/// translation button shows.
pub const NARROW_QUERY: &str = "(max-width: 759px)";

/// The most verses a chapter has (Psalm 119's 176): a verse past it isn't
/// a place.
const MAX_VERSE: i64 = 176;

/// One recent place: a chapter, the verses a jump selected there (if any),
/// and the translation it was read in, by code ("kjv").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecentPlace {
    pub book_id: i64,
    pub chapter: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<i64>,
    pub translation: String,
}

impl RecentPlace {
    /// The same chapter as `other`: only one place per chapter is kept.
    fn same_chapter(&self, book_id: i64, chapter: i64) -> bool {
        self.book_id == book_id && self.chapter == chapter
    }

    /// Whether this can be a place at all, by the books there are (`books`)
    /// and the translations (`codes`, lowercase): a book there is, one of
    /// its chapters, verses that could be in it (both or neither, in
    /// order), and a translation that's bundled.
    fn is_valid(&self, books: &[Book], codes: &[String]) -> bool {
        let chapter_ok = books
            .iter()
            .find(|b| b.id == self.book_id)
            .is_some_and(|b| (1..=b.chapter_count).contains(&self.chapter));
        let verses_ok = match (self.start, self.end) {
            (None, None) => true,
            (Some(start), Some(end)) => 1 <= start && start <= end && end <= MAX_VERSE,
            _ => false,
        };
        chapter_ok && verses_ok && codes.contains(&self.translation)
    }
}

/// One entry of the stored list, as read back: a place, or anything else
/// (which is dropped, the rest kept).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Stored {
    Place(RecentPlace),
    Other(serde::de::IgnoredAny),
}

/// Recent places, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecentPlaces {
    places: Vec<RecentPlace>,
}

impl RecentPlaces {
    /// The places, newest first.
    pub fn places(&self) -> &[RecentPlace] {
        &self.places
    }

    /// `place` is the newest: any other place in its chapter goes, and the
    /// oldest past [`RECENT_LIMIT`].
    pub fn record(&mut self, place: RecentPlace) {
        self.places
            .retain(|p| !p.same_chapter(place.book_id, place.chapter));
        self.places.insert(0, place);
        self.places.truncate(RECENT_LIMIT);
    }

    /// The list as it was kept (`stored`, in its order), checked: each
    /// entry that isn't a place there can be (an unknown book, a chapter
    /// it doesn't have, verses out of order, a translation that isn't
    /// bundled, or not a place at all) is dropped, as is any later place in
    /// a chapter already listed, and any past [`RECENT_LIMIT`].
    pub fn from_stored(stored: Vec<Stored>, books: &[Book], codes: &[String]) -> Self {
        let mut places: Vec<RecentPlace> = Vec::new();
        for entry in stored {
            let Stored::Place(place) = entry else {
                continue;
            };
            if place.is_valid(books, codes)
                && !places
                    .iter()
                    .any(|p| p.same_chapter(place.book_id, place.chapter))
            {
                places.push(place);
            }
            if places.len() == RECENT_LIMIT {
                break;
            }
        }
        Self { places }
    }
}

/// A stored value's length (in UTF-16 code units, as local storage counts
/// it) that is worth reading: not over [`STORED_MAX`].
pub fn readable(length: u32) -> bool {
    length <= STORED_MAX
}

/// How a place is named: "John 3", "John 3:16" or "John 3:16–18".
pub fn place_name(book_name: &str, chapter: i64, start: Option<i64>, end: Option<i64>) -> String {
    passage_reference(book_name, chapter, start.zip(end.or(start)))
}

/// What Back or Forward says, as the keys do it: its name (`aria-label`),
/// its tooltip, and whether it can go now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryButton {
    pub label: String,
    pub title: String,
    pub disabled: bool,
}

/// Back (`step` -1) or Forward (1), when it `can` go: to `to`, the place's
/// name, if it's known ("Back to Psalms 23 (Alt+←)", or "Back (Alt+←)");
/// otherwise nowhere ("Nothing earlier in this session (Alt+←)").
pub fn history_button(step: i32, can: bool, to: Option<&str>) -> HistoryButton {
    let (name, id, end) = if step < 0 {
        ("Back", "back", "Nothing earlier in this session")
    } else {
        ("Forward", "forward", "Nothing later in this session")
    };
    let keys = shortcut(id).map(|s| s.first_keys()).unwrap_or_default();
    match (can, to) {
        (true, Some(place)) => HistoryButton {
            label: format!("{name} to {place}"),
            title: format!("{name} to {place} ({keys})"),
            disabled: false,
        },
        (true, None) => HistoryButton {
            label: name.to_owned(),
            title: format!("{name} ({keys})"),
            disabled: false,
        },
        (false, _) => HistoryButton {
            label: name.to_owned(),
            title: format!("{end} ({keys})"),
            disabled: true,
        },
    }
}

/// One row of the Recent places menu: the place's name and translation
/// ("Romans 8:28 · KJV"), and whether it's where the reader is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuRow {
    pub text: String,
    pub here: bool,
}

/// A place's row: `name` (see [`place_name`]) and its translation, in
/// capitals.
pub fn menu_row(name: &str, translation: &str, here: bool) -> MenuRow {
    MenuRow {
        text: format!("{name} \u{b7} {}", translation.to_uppercase()),
        here,
    }
}

/// The Recent places menu's markup: on a narrow window (`forward` given)
/// Forward first (disabled, `Some(false)`, if there's nothing later), then
/// the heading and a row each, newest first, the reader's place tinted and
/// marked; or a line saying there are none yet. Each row is a menu item
/// with its index in the list (`data-index`).
pub fn menu_markup(rows: &[MenuRow], forward: Option<bool>) -> String {
    let mut out = String::new();
    if let Some(can) = forward {
        let keys = shortcut("forward")
            .map(|s| (s.first_keys(), s.aria))
            .unwrap_or_default();
        out.push_str(&format!(
            r#"<button type="button" id="recent-forward" class="menu-item recent-forward" role="menuitem" tabindex="-1" aria-keyshortcuts="{}"{}><span>Forward</span><span class="recent-key" aria-hidden="true">{}</span></button><div class="menu-separator" role="separator"></div>"#,
            keys.1,
            if can { "" } else { " disabled" },
            escape_html(&keys.0),
        ));
    }
    out.push_str(r#"<div class="recent-label" aria-hidden="true">Recent places</div>"#);
    if rows.is_empty() {
        out.push_str(r#"<p class="recent-empty">No recent places yet.</p>"#);
    }
    for (i, row) in rows.iter().enumerate() {
        let (class, current, mark) = if row.here {
            (
                " is-here",
                r#" aria-current="location""#,
                r#"<span class="recent-here">you are here</span>"#,
            )
        } else {
            ("", "", "")
        };
        out.push_str(&format!(
            r#"<button type="button" class="menu-item recent-item{class}" role="menuitem" tabindex="-1" data-index="{i}"{current}><span class="recent-place">{}</span>{mark}</button>"#,
            escape_html(&row.text),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn books() -> Vec<Book> {
        vec![
            Book {
                id: 1,
                name: "Genesis".into(),
                chapter_count: 50,
            },
            Book {
                id: 19,
                name: "Psalms".into(),
                chapter_count: 150,
            },
            Book {
                id: 43,
                name: "John".into(),
                chapter_count: 21,
            },
        ]
    }

    fn codes() -> Vec<String> {
        vec!["kjv".into(), "bsb".into(), "web".into()]
    }

    fn place(book_id: i64, chapter: i64, verses: Option<(i64, i64)>, code: &str) -> RecentPlace {
        RecentPlace {
            book_id,
            chapter,
            start: verses.map(|v| v.0),
            end: verses.map(|v| v.1),
            translation: code.into(),
        }
    }

    fn chapters(recent: &RecentPlaces) -> Vec<(i64, i64)> {
        recent
            .places()
            .iter()
            .map(|p| (p.book_id, p.chapter))
            .collect()
    }

    #[test]
    fn the_newest_comes_first_one_per_chapter() {
        let mut recent = RecentPlaces::default();
        recent.record(place(43, 3, None, "kjv"));
        recent.record(place(19, 23, None, "kjv"));
        recent.record(place(43, 3, Some((16, 16)), "web"));
        assert_eq!(chapters(&recent), [(43, 3), (19, 23)]);
        // The newest of a chapter wins: its verses and translation.
        assert_eq!(recent.places()[0], place(43, 3, Some((16, 16)), "web"));
    }

    #[test]
    fn only_the_last_ten_are_kept() {
        let mut recent = RecentPlaces::default();
        for chapter in 1..=12 {
            recent.record(place(1, chapter, None, "kjv"));
        }
        assert_eq!(recent.places().len(), RECENT_LIMIT);
        assert_eq!(recent.places()[0].chapter, 12);
        assert_eq!(recent.places()[9].chapter, 3);
    }

    #[test]
    fn kept_as_json_and_read_back_the_same() {
        let mut recent = RecentPlaces::default();
        recent.record(place(43, 3, Some((16, 18)), "kjv"));
        recent.record(place(19, 23, None, "web"));
        let json = serde_json::to_string(recent.places()).unwrap();
        assert_eq!(
            json,
            r#"[{"bookId":19,"chapter":23,"translation":"web"},{"bookId":43,"chapter":3,"start":16,"end":18,"translation":"kjv"}]"#
        );
        let stored: Vec<Stored> = serde_json::from_str(&json).unwrap();
        assert_eq!(
            RecentPlaces::from_stored(stored, &books(), &codes()),
            recent
        );
    }

    #[test]
    fn what_cant_be_a_place_is_dropped_and_the_rest_kept() {
        let json = r#"[
            {"bookId":43,"chapter":3,"translation":"kjv"},
            {"bookId":99,"chapter":1,"translation":"kjv"},
            {"bookId":43,"chapter":22,"translation":"kjv"},
            {"bookId":43,"chapter":0,"translation":"kjv"},
            {"bookId":1,"chapter":1,"start":5,"end":2,"translation":"kjv"},
            {"bookId":1,"chapter":1,"start":5,"translation":"kjv"},
            {"bookId":1,"chapter":1,"start":0,"end":0,"translation":"kjv"},
            {"bookId":1,"chapter":1,"start":1,"end":177,"translation":"kjv"},
            {"bookId":1,"chapter":2,"translation":"niv"},
            {"bookId":1,"chapter":2,"translation":"<b>"},
            {"bookId":"1","chapter":2,"translation":"kjv"},
            {"bookId":1,"chapter":2.5,"translation":"kjv"},
            {"bookId":1,"chapter":2},
            {"bookId":1,"chapter":2,"translation":"kjv","extra":true},
            "John 3",
            null,
            [1, 2],
            {"bookId":43,"chapter":3,"start":16,"end":16,"translation":"web"},
            {"bookId":19,"chapter":119,"start":176,"end":176,"translation":"bsb"}
        ]"#;
        let stored: Vec<Stored> = serde_json::from_str(json).unwrap();
        let recent = RecentPlaces::from_stored(stored, &books(), &codes());
        assert_eq!(
            recent.places(),
            [
                place(43, 3, None, "kjv"),
                place(19, 119, Some((176, 176)), "bsb")
            ]
        );
    }

    #[test]
    fn a_long_stored_list_is_cut_to_ten() {
        let stored: Vec<Stored> = (1..=30)
            .map(|chapter| Stored::Place(place(1, chapter, None, "kjv")))
            .collect();
        let recent = RecentPlaces::from_stored(stored, &books(), &codes());
        assert_eq!(recent.places().len(), RECENT_LIMIT);
        assert_eq!(recent.places()[0].chapter, 1);
    }

    #[test]
    fn a_value_too_long_to_be_ours_isnt_read() {
        assert!(readable(0));
        assert!(readable(STORED_MAX));
        assert!(!readable(STORED_MAX + 1));
        // Ten of the longest places fit well within it.
        let longest = place(43, 21, Some((176, 176)), "web");
        let json = serde_json::to_string(&vec![longest; RECENT_LIMIT]).unwrap();
        assert!(readable(u32::try_from(json.len()).unwrap()));
    }

    #[test]
    fn places_are_named_as_passages_are() {
        assert_eq!(place_name("Psalms", 23, None, None), "Psalms 23");
        assert_eq!(place_name("John", 3, Some(16), Some(16)), "John 3:16");
        assert_eq!(
            place_name("John", 3, Some(16), Some(18)),
            "John 3:16\u{2013}18"
        );
        assert_eq!(place_name("John", 3, Some(16), None), "John 3:16");
    }

    #[test]
    fn back_and_forward_name_where_they_go_and_their_keys() {
        assert_eq!(
            history_button(-1, true, Some("Psalms 23")),
            HistoryButton {
                label: "Back to Psalms 23".into(),
                title: "Back to Psalms 23 (Alt+←)".into(),
                disabled: false,
            }
        );
        assert_eq!(
            history_button(1, true, Some("Romans 8:28")).title,
            "Forward to Romans 8:28 (Alt+→)"
        );
        assert_eq!(
            history_button(-1, false, None),
            HistoryButton {
                label: "Back".into(),
                title: "Nothing earlier in this session (Alt+←)".into(),
                disabled: true,
            }
        );
        // Before the books are known, it still goes, unnamed.
        assert_eq!(history_button(-1, true, None).title, "Back (Alt+←)");
        assert!(!history_button(-1, true, None).disabled);
        let forward = history_button(1, false, Some("ignored"));
        assert_eq!(forward.label, "Forward");
        assert_eq!(forward.title, "Nothing later in this session (Alt+→)");
        assert!(forward.disabled);
    }

    #[test]
    fn the_menu_lists_the_places_newest_first_marking_where_you_are() {
        let rows = [
            menu_row("Psalms 23", "kjv", true),
            menu_row("Romans 8:28", "web", false),
        ];
        assert_eq!(rows[1].text, "Romans 8:28 \u{b7} WEB");
        let menu = menu_markup(&rows, None);
        assert!(
            menu.starts_with(r#"<div class="recent-label" aria-hidden="true">Recent places</div>"#)
        );
        assert!(!menu.contains("Forward"));
        assert!(menu.contains(
            r#"<button type="button" class="menu-item recent-item is-here" role="menuitem" tabindex="-1" data-index="0" aria-current="location"><span class="recent-place">Psalms 23 · KJV</span><span class="recent-here">you are here</span></button>"#
        ));
        assert!(menu.contains(
            r#"<button type="button" class="menu-item recent-item" role="menuitem" tabindex="-1" data-index="1"><span class="recent-place">Romans 8:28 · WEB</span></button>"#
        ));
        assert_eq!(menu.matches("you are here").count(), 1);
    }

    #[test]
    fn on_a_narrow_window_forward_comes_first() {
        let rows = [menu_row("Psalms 23", "kjv", true)];
        let menu = menu_markup(&rows, Some(true));
        assert!(menu.starts_with(
            r#"<button type="button" id="recent-forward" class="menu-item recent-forward" role="menuitem" tabindex="-1" aria-keyshortcuts="Alt+ArrowRight"><span>Forward</span><span class="recent-key" aria-hidden="true">Alt+→</span></button><div class="menu-separator" role="separator"></div>"#
        ));
        let nowhere = menu_markup(&rows, Some(false));
        assert!(nowhere.contains(r#"aria-keyshortcuts="Alt+ArrowRight" disabled>"#));
    }

    #[test]
    fn with_no_places_the_menu_says_so() {
        let menu = menu_markup(&[], None);
        assert!(menu.contains(r#"<p class="recent-empty">No recent places yet.</p>"#));
        assert!(!menu.contains("menuitem"));
    }

    #[test]
    fn a_row_is_escaped() {
        let menu = menu_markup(&[menu_row("<b>&", "kjv", false)], None);
        assert!(menu.contains("&lt;b&gt;&amp; · KJV"));
    }
}
