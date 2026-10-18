//! The note editor's rules and words: how long a note may be, when Save
//! can save, what the length line says, and whether a note has changed.
//! The pure part of what `ui/index.html`'s note panel did
//! (`charCount`, `noteChanged`, `updateNoteControls`), kept free of the DOM
//! so it is tested natively (`cargo test`).

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
}
