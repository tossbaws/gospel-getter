//! The note editor's rules and words: how long a note may be, when Save
//! can save, what the length line says, and whether a note has changed
//! (the pure part of what `ui/index.html`'s note panel did: `charCount`,
//! `noteChanged`, `updateNoteControls`); and its drafts: which draft is
//! whose, which are kept and for how long, and when one is offered back.
//! Kept free of the DOM so it is tested natively (`cargo test`).

use serde::{Deserialize, Serialize};

use crate::text::thousands;

/// The longest note, in characters (code points), as the backend allows.
pub const MAX_CHARS: u64 = 10_000;

/// The length line appears this close to the limit.
const WARN_WITHIN: u64 = 500;

/// A note's length: its characters (code points) once trimmed.
pub fn length(text: &str) -> u64 {
    text.trim().chars().count() as u64
}

/// Whether Save can save `text`: something to save, not too long, and
/// nothing under way.
pub fn can_save(text: &str, busy: bool) -> bool {
    let length = length(text);
    !busy && length > 0 && length <= MAX_CHARS
}

/// What the length line says: nothing until near the limit, then how many
/// of how many, then why it's too long.
pub fn length_line(text: &str) -> String {
    let length = length(text);
    if length > MAX_CHARS {
        format!(
            "{} characters: a note can hold up to {}.",
            thousands(length),
            thousands(MAX_CHARS)
        )
    } else if length > MAX_CHARS - WARN_WITHIN {
        format!(
            "{} of {} characters",
            thousands(length),
            thousands(MAX_CHARS)
        )
    } else {
        String::new()
    }
}

/// Whether the text differs from what the editor opened with (spaces at
/// either end don't count).
pub fn changed(text: &str, initial: &str) -> bool {
    text.trim() != initial.trim()
}

// ---- Drafts (R16): what's being typed is kept on this computer, in local
// storage, until it's saved or discarded, and offered back when that note,
// or a new note on that passage, is opened again. Never in a backup (it's
// none of the saved settings, `preferences`) or the notes export.

/// The local storage key every draft is kept under, as one list.
pub const DRAFTS_KEY: &str = "gospel-getter-note-drafts";

/// How long after the last key the text is kept as a draft, in
/// milliseconds (and at once when the text box loses focus, the window is
/// hidden or left, or the editor closes keeping it).
pub const DRAFT_DELAY_MS: i32 = 500;

/// The most drafts kept: the oldest go first.
pub const MAX_DRAFTS: usize = 20;

/// How long a draft is kept, in days.
pub const DRAFT_DAYS: f64 = 30.0;

const DAY_MS: f64 = 24.0 * 60.0 * 60.0 * 1000.0;

/// The verses a draft is on: a note's own, or a new note's. Any
/// translation's: a note belongs to none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftPassage {
    pub book: i64,
    pub chapter: i64,
    pub start: i64,
    pub end: i64,
}

/// Whose a draft is: note `id`'s (an edit), or a new note's on a passage.
pub fn draft_key(note_id: Option<i64>, passage: DraftPassage) -> String {
    match note_id {
        Some(id) => format!("note:{id}"),
        None => format!(
            "new:{}:{}:{}-{}",
            passage.book, passage.chapter, passage.start, passage.end
        ),
    }
}

/// The note a draft key is an edit of, if it is one.
fn note_of(key: &str) -> Option<i64> {
    key.strip_prefix("note:")?.parse().ok()
}

/// One draft: whose it is, its verses, its text as typed, and when it was
/// kept (milliseconds since the epoch, as `Date.now()`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Draft {
    pub key: String,
    pub passage: DraftPassage,
    pub text: String,
    pub at: f64,
}

/// Every draft, oldest first: what local storage holds under
/// [`DRAFTS_KEY`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Drafts {
    pub drafts: Vec<Draft>,
}

/// Whether text typed is worth keeping as a draft: something, and not
/// what the editor opened with.
pub fn worth_keeping(text: &str, initial: &str) -> bool {
    !text.trim().is_empty() && changed(text, initial)
}

impl Drafts {
    /// Drops the drafts older than [`DRAFT_DAYS`] (or kept in the future,
    /// by a clock since put back), then all but the newest [`MAX_DRAFTS`].
    /// Whether any went.
    pub fn prune(&mut self, now: f64) -> bool {
        let before = self.drafts.len();
        self.drafts
            .retain(|d| d.at <= now && now - d.at <= DRAFT_DAYS * DAY_MS);
        self.drafts.sort_by(|a, b| a.at.total_cmp(&b.at));
        let over = self.drafts.len().saturating_sub(MAX_DRAFTS);
        self.drafts.drain(..over);
        self.drafts.len() != before
    }

    /// Keeps `draft`, in place of any draft of the same key.
    pub fn put(&mut self, draft: Draft) {
        self.remove(&draft.key);
        let now = draft.at;
        self.drafts.push(draft);
        self.prune(now);
    }

    /// Forgets the draft `key`. Whether there was one.
    pub fn remove(&mut self, key: &str) -> bool {
        let before = self.drafts.len();
        self.drafts.retain(|d| d.key != key);
        self.drafts.len() != before
    }

    /// Forgets the drafts of notes that aren't among `ids` (every note
    /// there is): a note deleted takes its draft with it. Whether any went.
    pub fn keep_notes(&mut self, ids: &[i64]) -> bool {
        let before = self.drafts.len();
        self.drafts
            .retain(|d| note_of(&d.key).is_none_or(|id| ids.contains(&id)));
        self.drafts.len() != before
    }

    /// The draft to offer when the editor opens with `key` on `passage`,
    /// the note saying `saved` (nothing, for a new note): one kept within
    /// [`DRAFT_DAYS`] on the same verses, whose text is something other
    /// than what's saved.
    pub fn offer(&self, key: &str, passage: DraftPassage, saved: &str, now: f64) -> Option<&Draft> {
        self.drafts.iter().find(|d| {
            d.key == key
                && d.passage == passage
                && d.at <= now
                && now - d.at <= DRAFT_DAYS * DAY_MS
                && worth_keeping(&d.text, saved)
        })
    }
}

/// How long ago something was, in words: "a moment ago", "5 minutes
/// ago", "2 hours ago", "yesterday", "3 days ago".
pub fn ago(elapsed_ms: f64) -> String {
    let minutes = (elapsed_ms.max(0.0) / 60_000.0).floor() as u64;
    let hours = minutes / 60;
    let days = hours / 24;
    match (minutes, hours, days) {
        (0, _, _) => "a moment ago".to_string(),
        (1, _, _) => "a minute ago".to_string(),
        (m, 0, _) => format!("{m} minutes ago"),
        (_, 1, _) => "an hour ago".to_string(),
        (_, h, 0) => format!("{h} hours ago"),
        (_, _, 1) => "yesterday".to_string(),
        (_, _, d) => format!("{} days ago", thousands(d)),
    }
}

/// What the editor says when it opens with a draft to offer, kept at `at`.
pub fn draft_offer(at: f64, now: f64) -> String {
    format!("You have unsaved text from {}.", ago(now - at))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_counts_characters_once_trimmed() {
        assert_eq!(length("  abc \n"), 3);
        assert_eq!(length("ε͂ν ἀρχῇ"), 8);
        assert_eq!(length("😀😀"), 2, "code points, not UTF-16 units");
    }

    #[test]
    fn save_needs_text_within_the_limit_and_nothing_under_way() {
        assert!(!can_save("", false));
        assert!(!can_save("   \n ", false), "whitespace is nothing to save");
        assert!(can_save(" a ", false));
        assert!(!can_save("a", true));
        assert!(can_save(&"x".repeat(10_000), false));
        assert!(!can_save(&"x".repeat(10_001), false));
    }

    #[test]
    fn the_length_line_near_and_over_the_limit() {
        assert_eq!(length_line("short"), "");
        assert_eq!(length_line(&"x".repeat(9_500)), "");
        assert_eq!(
            length_line(&"x".repeat(9_501)),
            "9,501 of 10,000 characters"
        );
        assert_eq!(
            length_line(&"x".repeat(10_001)),
            "10,001 characters: a note can hold up to 10,000."
        );
    }

    #[test]
    fn changed_ignores_spaces_at_the_ends() {
        assert!(!changed("  same ", "same"));
        assert!(changed("same.", "same"));
        assert!(changed("draft", ""));
        assert!(!changed("   ", ""));
    }

    const NOW: f64 = 1_800_000_000_000.0;
    const JOHN_3_16: DraftPassage = DraftPassage {
        book: 43,
        chapter: 3,
        start: 16,
        end: 18,
    };

    fn draft(key: &str, text: &str, at: f64) -> Draft {
        Draft {
            key: key.to_string(),
            passage: JOHN_3_16,
            text: text.to_string(),
            at,
        }
    }

    #[test]
    fn a_draft_is_an_edit_s_by_its_note_or_a_new_note_s_by_its_verses() {
        assert_eq!(draft_key(Some(12), JOHN_3_16), "note:12");
        assert_eq!(draft_key(None, JOHN_3_16), "new:43:3:16-18");
        let one = DraftPassage {
            end: 16,
            ..JOHN_3_16
        };
        assert_ne!(draft_key(None, one), draft_key(None, JOHN_3_16));
        assert_eq!(note_of("note:12"), Some(12));
        assert_eq!(note_of("new:43:3:16-18"), None);
    }

    #[test]
    fn a_draft_is_kept_only_when_something_was_typed_that_is_not_saved() {
        assert!(worth_keeping("Loved.", ""));
        assert!(worth_keeping("Loved, all.", "Loved."));
        assert!(
            !worth_keeping("  Loved. ", "Loved."),
            "spaces at the ends don't count"
        );
        assert!(!worth_keeping(" \n ", ""));
        assert!(
            !worth_keeping("", "Loved."),
            "a note emptied can't be saved either"
        );
    }

    #[test]
    fn one_draft_per_key_the_newest() {
        let mut drafts = Drafts::default();
        drafts.put(draft("note:1", "first", NOW - 2000.0));
        drafts.put(draft("new:43:3:16-18", "other", NOW - 1000.0));
        drafts.put(draft("note:1", "second", NOW));
        assert_eq!(drafts.drafts.len(), 2);
        assert_eq!(
            drafts
                .offer("note:1", JOHN_3_16, "", NOW)
                .map(|d| d.text.as_str()),
            Some("second")
        );
        assert!(drafts.remove("note:1"));
        assert!(!drafts.remove("note:1"));
        assert_eq!(drafts.offer("note:1", JOHN_3_16, "", NOW), None);
    }

    #[test]
    fn drafts_older_than_30_days_go_and_at_most_20_are_kept() {
        let mut drafts = Drafts::default();
        drafts
            .drafts
            .push(draft("note:1", "old", NOW - 30.0 * DAY_MS - 1.0));
        drafts
            .drafts
            .push(draft("note:2", "just in", NOW - 30.0 * DAY_MS));
        drafts
            .drafts
            .push(draft("note:3", "from a clock ahead", NOW + 60_000.0));
        assert!(drafts.prune(NOW));
        assert_eq!(
            drafts
                .drafts
                .iter()
                .map(|d| d.key.as_str())
                .collect::<Vec<_>>(),
            ["note:2"]
        );
        assert!(!drafts.prune(NOW));

        let mut drafts = Drafts::default();
        for i in 0..25 {
            drafts.put(draft(
                &format!("note:{i}"),
                "text",
                NOW - 25_000.0 + f64::from(i) * 1000.0,
            ));
        }
        assert_eq!(drafts.drafts.len(), MAX_DRAFTS);
        assert_eq!(drafts.drafts[0].key, "note:5", "the oldest went first");
        assert_eq!(drafts.drafts[MAX_DRAFTS - 1].key, "note:24");
    }

    #[test]
    fn a_draft_is_offered_only_on_its_verses_within_30_days_and_unlike_what_is_saved() {
        let mut drafts = Drafts::default();
        drafts.put(draft("note:7", "Loved, all of it.", NOW - 5.0 * 60_000.0));
        let offered = |saved: &str, passage: DraftPassage, now: f64| {
            drafts
                .offer("note:7", passage, saved, now)
                .map(|d| d.text.clone())
        };
        assert_eq!(
            offered("Loved.", JOHN_3_16, NOW).as_deref(),
            Some("Loved, all of it.")
        );
        assert_eq!(
            offered("Loved, all of it. ", JOHN_3_16, NOW),
            None,
            "what's saved already"
        );
        // A note of that id on other verses is another note.
        let other = DraftPassage {
            book: 45,
            ..JOHN_3_16
        };
        assert_eq!(offered("Loved.", other, NOW), None);
        assert_eq!(offered("Loved.", JOHN_3_16, NOW + 31.0 * DAY_MS), None);
        assert_eq!(drafts.offer("note:8", JOHN_3_16, "", NOW), None);
    }

    #[test]
    fn a_deleted_note_takes_its_draft_with_it() {
        let mut drafts = Drafts::default();
        drafts.put(draft("note:1", "a", NOW));
        drafts.put(draft("note:2", "b", NOW));
        drafts.put(draft("new:43:3:16-18", "c", NOW));
        assert!(drafts.keep_notes(&[2, 3]));
        assert_eq!(
            drafts
                .drafts
                .iter()
                .map(|d| d.key.as_str())
                .collect::<Vec<_>>(),
            ["note:2", "new:43:3:16-18"]
        );
        assert!(!drafts.keep_notes(&[2]));
    }

    #[test]
    fn the_offer_says_how_long_ago_in_words() {
        assert_eq!(ago(-5.0), "a moment ago");
        assert_eq!(ago(59_999.0), "a moment ago");
        assert_eq!(ago(60_000.0), "a minute ago");
        assert_eq!(ago(5.0 * 60_000.0), "5 minutes ago");
        assert_eq!(ago(60.0 * 60_000.0), "an hour ago");
        assert_eq!(ago(23.0 * 3_600_000.0), "23 hours ago");
        assert_eq!(ago(DAY_MS), "yesterday");
        assert_eq!(ago(3.5 * DAY_MS), "3 days ago");
        assert_eq!(
            draft_offer(NOW - 5.0 * 60_000.0, NOW),
            "You have unsaved text from 5 minutes ago."
        );
    }
}
