//! How the app names a passage: "John 3", "John 3:16" or "John 3:16–18"
//! (an en dash between the verses). The backend (`src-tauri`) names every
//! passage it sends this way (bookmarks, notes, copied text, previews); the
//! page's Rust pieces (`ui-leptos`) name the reader's verse selection, which
//! has no answer of the backend's to carry its name, the same way.
//!
//! This one file is compiled into both crates, as their `passage_reference`
//! module (`#[path]`), so the rule is written once.

/// "16", or "16–18" for a range.
pub fn verse_range(first: i64, last: i64) -> String {
    if first == last {
        first.to_string()
    } else {
        format!("{first}\u{2013}{last}")
    }
}

/// "John 3" for the whole chapter (no verses), "John 3:16" or
/// "John 3:16–18".
pub fn passage_reference(book_name: &str, chapter: i64, verses: Option<(i64, i64)>) -> String {
    match verses {
        None => format!("{book_name} {chapter}"),
        Some((first, last)) => format!("{book_name} {chapter}:{}", verse_range(first, last)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chapter_a_verse_and_a_range() {
        assert_eq!(passage_reference("John", 3, None), "John 3");
        assert_eq!(passage_reference("John", 3, Some((16, 16))), "John 3:16");
        assert_eq!(
            passage_reference("1 John", 4, Some((9, 10))),
            "1 John 4:9\u{2013}10"
        );
        assert_eq!(verse_range(7, 7), "7");
        assert_eq!(verse_range(7, 9), "7–9");
    }
}
