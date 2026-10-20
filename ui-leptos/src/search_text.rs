//! The search box's words and rules (`search`), free of the DOM so they are
//! tested natively (`cargo test`): its status sentences, which option is
//! active as the reader types and moves, Show more's next page, and a
//! verse's text split at the words the search matched (the backend says
//! where they are: `marks`, the one rule for which words those are, beside
//! the search's own). The words are the page's (`ui/index.html` at
//! 201ab8f), byte for byte.

/// The empty box says what it takes.
pub const HINT: &str = "Type words to search, a reference to go there, or > for a command.";

/// A reference was found: Enter goes there.
pub const GO_THERE: &str = "Press Enter to go there.";

/// ">" and words no command has.
pub const NO_COMMANDS: &str = "No commands match.";

/// The search failed, with why.
pub fn failed(error: &str) -> String {
    format!("Search failed: {error}")
}

/// A command that can't run now, chosen: what it is and why not.
pub fn cannot_run(label: &str, reason: &str) -> String {
    format!("{label}: {reason}.")
}

/// `n` as the page wrote counts (`toLocaleString('en')`): "12,345".
pub fn thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, d) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(d);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// The status line for a page of verses: how many verses in `code`
/// (the translation searched, any case) have every term, and how many are
/// shown so far if not all.
pub fn verses_status(total: i64, shown: i64, code: &str, terms: &[String]) -> String {
    let code = code.to_uppercase();
    if total == 0 {
        return format!(
            "No verses in the {code} contain all of: {}.",
            terms.join(", ")
        );
    }
    let count = if total == 1 {
        "1 verse".to_string()
    } else {
        format!("{} verses", thousands(total))
    };
    let showing = if shown < total {
        format!(", showing {}", thousands(shown))
    } else {
        String::new()
    };
    format!("{count} in the {code}{showing}.")
}

/// Where Show more starts, if there are more verses than shown: the page
/// of `count` verses at `offset` of `total`.
pub fn more_from(offset: i64, count: usize, total: i64) -> Option<i64> {
    let next = offset + count as i64;
    (next < total).then_some(next)
}

/// Which option is active once the list is drawn, given the options' keys
/// in order: if the reader chose one with the arrows, that one, if it's
/// still there; otherwise, while typing, the first if it's a Go to or a
/// command (so Enter goes there or runs it, as Enter on a reference always
/// has); with the empty box, or verses first, none until ↓.
pub fn active_after_drawing(
    keys: &[String],
    moved: bool,
    chosen: Option<&str>,
    empty: bool,
) -> Option<usize> {
    if moved {
        return keys.iter().position(|k| Some(k.as_str()) == chosen);
    }
    match keys.first() {
        Some(first) if !empty && !first.starts_with("verse:") => Some(0),
        _ => None,
    }
}

/// ↓ (`down`) or ↑ from the option `at` (none: the box itself) among
/// `count`: ↓ stops at the last, ↑ goes back up to the box.
pub fn step(at: Option<usize>, count: usize, down: bool) -> Option<usize> {
    if count == 0 {
        return at;
    }
    let at = at.map_or(-1, |i| i as isize);
    let next = if down {
        (at + 1).min(count as isize - 1)
    } else {
        (at - 1).max(-1)
    };
    usize::try_from(next).ok()
}

/// `text` in pieces, each marked (a word the search matched) or not:
/// `marks` are byte ranges of `text`, in order, as the backend gives them.
/// Every character of `text` is in exactly one piece, so the pieces
/// together are the text exactly; a mark that doesn't fit (out of order,
/// out of range, or not on a character's edge) is left out.
pub fn marked<'a>(text: &'a str, marks: &[(usize, usize)]) -> Vec<(&'a str, bool)> {
    let mut pieces = Vec::new();
    let mut at = 0;
    for &(start, end) in marks {
        let fits = at <= start
            && start < end
            && end <= text.len()
            && text.is_char_boundary(start)
            && text.is_char_boundary(end);
        if !fits {
            continue;
        }
        if at < start {
            pieces.push((&text[at..start], false));
        }
        pieces.push((&text[start..end], true));
        at = end;
    }
    if at < text.len() {
        pieces.push((&text[at..], false));
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|k| k.to_string()).collect()
    }

    #[test]
    fn counts_are_written_with_thousands_commas() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(50), "50");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(31102), "31,102");
        assert_eq!(thousands(1234567), "1,234,567");
        assert_eq!(thousands(-1234), "-1,234");
    }

    #[test]
    fn the_status_says_how_many_and_how_many_are_shown() {
        let terms = keys(&["faith", "hope"]);
        assert_eq!(
            verses_status(0, 0, "kjv", &terms),
            "No verses in the KJV contain all of: faith, hope."
        );
        assert_eq!(verses_status(1, 1, "web", &terms), "1 verse in the WEB.");
        assert_eq!(
            verses_status(12, 12, "kjv", &terms),
            "12 verses in the KJV."
        );
        assert_eq!(
            verses_status(1234, 50, "WEB", &terms),
            "1,234 verses in the WEB, showing 50."
        );
        assert_eq!(
            verses_status(1234, 1234, "bsb", &terms),
            "1,234 verses in the BSB."
        );
        assert_eq!(failed("Search failed."), "Search failed: Search failed.");
        assert_eq!(
            cannot_run("Compare with: WEB", "Only while comparing"),
            "Compare with: WEB: Only while comparing."
        );
    }

    #[test]
    fn show_more_starts_after_what_is_shown() {
        assert_eq!(more_from(0, 50, 120), Some(50));
        assert_eq!(more_from(50, 50, 120), Some(100));
        assert_eq!(more_from(100, 20, 120), None);
        assert_eq!(more_from(0, 0, 0), None);
    }

    #[test]
    fn the_first_go_to_or_command_is_active_as_the_reader_types() {
        let go = keys(&["go", "cmd:compare"]);
        let commands = keys(&["cmd:compare", "verse:43:3:16"]);
        let verses = keys(&["verse:43:3:16", "verse:43:3:17"]);
        assert_eq!(active_after_drawing(&go, false, None, false), Some(0));
        assert_eq!(active_after_drawing(&commands, false, None, false), Some(0));
        assert_eq!(active_after_drawing(&verses, false, None, false), None);
        assert_eq!(active_after_drawing(&commands, false, None, true), None);
        assert_eq!(active_after_drawing(&[], false, None, false), None);
        // Chosen with the arrows: kept, by its key, while it's there.
        assert_eq!(
            active_after_drawing(&verses, true, Some("verse:43:3:17"), false),
            Some(1)
        );
        assert_eq!(active_after_drawing(&verses, true, Some("go"), false), None);
        assert_eq!(active_after_drawing(&go, true, None, false), None);
    }

    #[test]
    fn the_arrows_stop_at_the_last_and_go_back_up_to_the_box() {
        assert_eq!(step(None, 3, true), Some(0));
        assert_eq!(step(Some(0), 3, true), Some(1));
        assert_eq!(step(Some(2), 3, true), Some(2));
        assert_eq!(step(Some(1), 3, false), Some(0));
        assert_eq!(step(Some(0), 3, false), None);
        assert_eq!(step(None, 3, false), None);
        assert_eq!(step(None, 0, true), None);
        assert_eq!(step(Some(4), 0, true), Some(4), "nothing to move through");
    }

    #[test]
    fn marks_split_the_text_and_keep_every_character() {
        let text = "And now abideth faith, hope, charity";
        let pieces = marked(text, &[(16, 21), (23, 27), (29, 36)]);
        assert_eq!(
            pieces,
            [
                ("And now abideth ", false),
                ("faith", true),
                (", ", false),
                ("hope", true),
                (", ", false),
                ("charity", true),
            ]
        );
        assert_eq!(pieces.iter().map(|(s, _)| *s).collect::<String>(), text);
        assert_eq!(marked(text, &[]), [(text, false)]);
        assert_eq!(marked("", &[]), Vec::<(&str, bool)>::new());
    }

    #[test]
    fn marks_that_dont_fit_are_left_out() {
        let text = "é love";
        // Inside "é" (two bytes), past the end, backwards, empty, and
        // overlapping the one before.
        let pieces = marked(text, &[(1, 2), (3, 7), (6, 99), (5, 4), (4, 4), (4, 6)]);
        assert_eq!(pieces, [("é ", false), ("love", true)]);
        assert_eq!(pieces.iter().map(|(s, _)| *s).collect::<String>(), text);
    }
}
