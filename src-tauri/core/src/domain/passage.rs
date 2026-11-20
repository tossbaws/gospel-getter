//! What reading a passage in a translation has to work out whatever is
//! asking, the app or `gospel-getter-cli`: which translations were named, and
//! which verses asked for a translation doesn't number.

use crate::db::Translation;
use crate::passage_reference::passage_reference;

/// The translations `codes` name, in that order. A translation that isn't
/// there, one named twice, or none at all, is an error.
pub fn translations_named<'a>(
    translations: &'a [Translation],
    codes: &[String],
) -> anyhow::Result<Vec<&'a Translation>> {
    let mut named = Vec::with_capacity(codes.len());
    for code in codes {
        let t = translations
            .iter()
            .find(|t| &t.code == code)
            .ok_or_else(|| anyhow::anyhow!("There's no translation \"{code}\"."))?;
        if named.iter().any(|seen: &&Translation| seen.id == t.id) {
            anyhow::bail!("{} is named twice.", t.name);
        }
        named.push(t);
    }
    if named.is_empty() {
        anyhow::bail!("No translations to compare.");
    }
    Ok(named)
}

/// What the search box says of a reference to verses past the end of their
/// chapter in the translation searched, `None` when they're within it:
/// e.g. "No such chapter or verse: John 3:99. John 3 has 36 verses in the
/// King James Version." `verse_count` is the chapter's last verse in that
/// translation. The app's search and `gospel-getter-cli`'s say the same.
pub fn no_such_verse(
    book_name: &str,
    translation_name: &str,
    chapter: i64,
    (first, last): (i64, i64),
    verse_count: i64,
) -> Option<String> {
    (last > verse_count).then(|| {
        format!(
            "No such chapter or verse: {}. {book_name} {chapter} has {verse_count} verses in the {translation_name}.",
            passage_reference(book_name, chapter, Some((first, last))),
        )
    })
}

/// What to say about the verses from `start` to `end` of a chapter that the
/// translation doesn't number, `None` when it numbers them all. `numbered` is
/// those it does number, `last` the chapter's last verse in this translation
/// and `last_anywhere` its last in any: e.g. "Romans 16:25–27 isn't numbered
/// in the World English Bible — Romans 16 ends at verse 24 there." Verses past
/// the end in every translation are named like any others, but never counted
/// one by one, so `end` sizes no loop and no list.
pub fn numbering_notice(
    book_name: &str,
    translation_name: &str,
    chapter: i64,
    (start, end): (i64, i64),
    numbered: &[i64],
    last: i64,
    last_anywhere: i64,
) -> Option<String> {
    let missing = missing_runs(numbered, start, end, last_anywhere);
    let what = passage_reference(book_name, chapter, Some((start, end)));
    say_missing(&what, book_name, translation_name, chapter, &missing, last)
}

/// What to say about a whole chapter's verses that the translation doesn't
/// number and another installed translation does, `None` when there are
/// none, in [`numbering_notice`]'s words: e.g. "In Romans 16, verse 24
/// isn't numbered in the Berean Standard Bible." `numbered` is those it
/// numbers, `numbered_anywhere` those any installed translation numbers
/// (`db::get_chapter_verses_numbered_anywhere`) and `last` its last. A
/// verse no translation numbers isn't named. `None` too when it numbers
/// none of them: then the chapter isn't there at all, which the caller
/// says.
pub fn chapter_numbering_notice(
    book_name: &str,
    translation_name: &str,
    chapter: i64,
    numbered: &[i64],
    numbered_anywhere: &[i64],
    last: i64,
) -> Option<String> {
    if numbered.is_empty() {
        return None;
    }
    let mut missing: Vec<(i64, i64)> = Vec::new();
    for &n in numbered_anywhere.iter().filter(|n| !numbered.contains(n)) {
        match missing.last_mut() {
            Some((_, run_end)) if *run_end + 1 == n => *run_end = n,
            _ => missing.push((n, n)),
        }
    }
    let what = passage_reference(book_name, chapter, None);
    say_missing(&what, book_name, translation_name, chapter, &missing, last)
}

/// The notice on `missing` (runs, in order) of `what`: all after the
/// translation's `last` verse, or some within it.
fn say_missing(
    what: &str,
    book_name: &str,
    translation_name: &str,
    chapter: i64,
    missing: &[(i64, i64)],
    last: i64,
) -> Option<String> {
    let (&(first_missing, _), &(_, last_missing)) = (missing.first()?, missing.last()?);
    if first_missing > last {
        let missing = passage_reference(book_name, chapter, Some((first_missing, last_missing)));
        Some(format!(
            "{missing} isn't numbered in the {translation_name} \u{2014} {book_name} {chapter} ends at verse {last} there."
        ))
    } else {
        let (noun, verb) = if first_missing == last_missing {
            ("verse", "isn't")
        } else {
            ("verses", "aren't")
        };
        Some(format!(
            "In {what}, {noun} {} {verb} numbered in the {translation_name}.",
            verse_list(missing),
        ))
    }
}

/// The verses from `start` to `end` that aren't in `numbered`, in order, as
/// runs `(first, last)`. Only verses up to `last_anywhere` (the chapter's
/// last verse in any translation) are looked at one by one; those past it,
/// however far `end` goes, are one run.
pub fn missing_runs(numbered: &[i64], start: i64, end: i64, last_anywhere: i64) -> Vec<(i64, i64)> {
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
pub fn verse_list(runs: &[(i64, i64)]) -> String {
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

    /// The verses a translation doesn't number, as runs, and as a list: two
    /// runs merge only where they meet.
    #[test]
    fn missing_verses_are_listed_in_runs() {
        assert_eq!(
            verse_list(&missing_runs(&[5, 9, 10], 4, 11, 20)),
            "4, 6\u{2013}8, 11"
        );
        // The BSB leaves Mark 9:44 and 9:46 unnumbered, the only chapter of
        // it with two: verses 44 and 46, never 44–46.
        let numbered = [43, 45, 47, 48];
        assert_eq!(missing_runs(&numbered, 43, 48, 50), [(44, 44), (46, 46)]);
        assert_eq!(
            numbering_notice(
                "Mark",
                "Berean Standard Bible",
                9,
                (43, 48),
                &numbered,
                50,
                50
            )
            .as_deref(),
            Some(
                "In Mark 9:43\u{2013}48, verses 44, 46 aren't numbered in the Berean Standard Bible."
            )
        );
    }

    /// A whole chapter names only the verses another translation numbers:
    /// a number none has is no gap, and runs merge only where they meet.
    #[test]
    fn a_whole_chapter_names_what_another_translation_numbers() {
        let notice = |numbered: &[i64], anywhere: &[i64], last| {
            chapter_numbering_notice("Mark", "Berean Standard Bible", 9, numbered, anywhere, last)
        };
        let anywhere: Vec<i64> = (1..=50).collect();
        let numbered: Vec<i64> = (1..=50).filter(|n| ![44, 46].contains(n)).collect();
        assert_eq!(
            notice(&numbered, &anywhere, 50).as_deref(),
            Some("In Mark 9, verses 44, 46 aren't numbered in the Berean Standard Bible.")
        );
        // Verse 51 is numbered nowhere, so it isn't named.
        assert_eq!(notice(&anywhere, &anywhere, 50), None);
        let short: Vec<i64> = (1..=47).collect();
        assert_eq!(
            notice(&short, &anywhere, 47).as_deref(),
            Some(
                "Mark 9:48\u{2013}50 isn't numbered in the Berean Standard Bible \u{2014} \
                 Mark 9 ends at verse 47 there."
            )
        );
        // None numbered: the chapter isn't there, which the caller says.
        assert_eq!(notice(&[], &anywhere, 0), None);
    }
}
