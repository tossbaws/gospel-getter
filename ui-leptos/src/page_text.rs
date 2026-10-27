//! The page glue's words and rules (`page`), apart from the DOM so they can
//! be tested on the host: what the toast says when something isn't there
//! yet, what Export data… says it saved, and which translation Compare
//! shows beside the one being read.

/// Said when Export notes' dialog isn't there to open.
pub const EXPORT_NOTES_UNAVAILABLE: &str = "Export notes isn’t available.";
/// Said when the import isn't there to begin.
pub const IMPORT_UNAVAILABLE: &str = "Import isn’t available.";
/// Said when the welcome isn't there to open.
pub const WELCOME_UNAVAILABLE: &str = "The welcome isn’t available.";
/// Said when the shortcuts sheet isn't there to open.
pub const SHORTCUTS_UNAVAILABLE: &str = "Keyboard shortcuts aren’t available.";
/// Said when the search box isn't there to open.
pub const SEARCH_UNAVAILABLE: &str = "Search isn’t available.";
/// Said when About isn't there to open.
pub const ABOUT_UNAVAILABLE: &str = "About isn’t available.";

/// Export data…, while the save dialog is open.
pub const EXPORT_CHOOSING: &str = "Choose where to save the file…";
/// Export data…, the dialog closed without saving.
pub const EXPORT_CANCELLED: &str = "Export cancelled. No file was saved.";

/// `n` as JavaScript's `toLocaleString('en')` writes a whole number: its
/// thousands grouped with commas.
pub fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// `n` things: "1 bookmark", "2 bookmarks", "1,200 notes".
pub fn plural(n: u64, one: &str) -> String {
    let unit = if n == 1 {
        one.to_owned()
    } else {
        format!("{one}s")
    };
    format!("{} {unit}", grouped(n))
}

/// What an export wrote (`export_reader_data`'s answer when saved).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Saved<'a> {
    pub path: &'a str,
    pub bookmarks: u64,
    pub highlights: u64,
    pub notes: u64,
    pub has_position: bool,
    pub preferences: u64,
}

/// What Export data… says it saved: always the bookmarks, then whatever
/// else there was, and where.
pub fn export_saved(saved: &Saved) -> String {
    let mut parts = vec![plural(saved.bookmarks, "bookmark")];
    if saved.highlights > 0 {
        parts.push(plural(saved.highlights, "highlighted verse"));
    }
    if saved.notes > 0 {
        parts.push(plural(saved.notes, "note"));
    }
    if saved.has_position {
        parts.push("your reading position".to_owned());
    }
    if saved.preferences > 0 {
        parts.push("your display settings".to_owned());
    }
    format!(
        "Saved {} to {}. Bible text isn’t included.",
        parts.join(", "),
        saved.path
    )
}

/// The translations Compare shows beside the one being read when the reader
/// hasn't chosen one that's there: the one compare always showed, the KJV,
/// or the WEB beside the KJV.
pub const COMPARE_DEFAULTS: [&str; 2] = ["kjv", "web"];

/// The translation Compare shows beside `current`: the one the reader chose
/// (Aa's "with", `chosen`), if it's one of `codes` (the translations there
/// are) and isn't `current`; otherwise the first of [`COMPARE_DEFAULTS`]
/// that is; failing those, the first other. `None` when there's no other.
pub fn partner(chosen: Option<&str>, current: &str, codes: &[String]) -> Option<String> {
    chosen
        .into_iter()
        .chain(COMPARE_DEFAULTS)
        .chain(codes.iter().map(String::as_str))
        .find(|code| !code.is_empty() && *code != current && codes.iter().any(|c| c == code))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_grouped_as_javascript_writes_them() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(7), "7");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1000), "1,000");
        assert_eq!(grouped(31102), "31,102");
        assert_eq!(grouped(1234567), "1,234,567");
        assert_eq!(plural(1, "bookmark"), "1 bookmark");
        assert_eq!(plural(0, "bookmark"), "0 bookmarks");
        assert_eq!(
            plural(1200, "highlighted verse"),
            "1,200 highlighted verses"
        );
    }

    #[test]
    fn an_export_says_what_it_saved() {
        let only = Saved {
            path: "/home/r/gg.json",
            ..Saved::default()
        };
        assert_eq!(
            export_saved(&only),
            "Saved 0 bookmarks to /home/r/gg.json. Bible text isn’t included."
        );
        let all = Saved {
            path: "C:\\b.json",
            bookmarks: 1,
            highlights: 3,
            notes: 1,
            has_position: true,
            preferences: 9,
        };
        assert_eq!(
            export_saved(&all),
            "Saved 1 bookmark, 3 highlighted verses, 1 note, your reading position, your display \
             settings to C:\\b.json. Bible text isn’t included."
        );
    }

    #[test]
    fn compare_shows_the_chosen_translation_or_a_default_beside_this_one() {
        let codes: Vec<String> = ["kjv", "web", "bsb"].map(String::from).to_vec();
        assert_eq!(partner(Some("bsb"), "kjv", &codes).as_deref(), Some("bsb"));
        // The chosen one being read, or not there: the defaults.
        assert_eq!(partner(Some("kjv"), "kjv", &codes).as_deref(), Some("web"));
        assert_eq!(partner(Some("esv"), "bsb", &codes).as_deref(), Some("kjv"));
        assert_eq!(partner(None, "web", &codes).as_deref(), Some("kjv"));
        assert_eq!(partner(Some(""), "kjv", &codes).as_deref(), Some("web"));
        // Neither default there: the first other.
        let others: Vec<String> = ["bsb", "asv", "ylt"].map(String::from).to_vec();
        assert_eq!(partner(None, "bsb", &others).as_deref(), Some("asv"));
        // No other: none.
        assert_eq!(partner(None, "kjv", &["kjv".to_owned()]), None);
        assert_eq!(partner(Some("web"), "kjv", &[]), None);
    }
}
