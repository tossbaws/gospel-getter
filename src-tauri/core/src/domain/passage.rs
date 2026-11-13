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
    (!missing.is_empty()).then(|| {
        let what = passage_reference(book_name, chapter, Some((start, end)));
        let (first_missing, _) = missing[0];
        let (_, last_missing) = missing[missing.len() - 1];
        if first_missing > last {
            let missing =
                passage_reference(book_name, chapter, Some((first_missing, last_missing)));
            format!(
                "{missing} isn't numbered in the {translation_name} \u{2014} {book_name} {chapter} ends at verse {last} there."
            )
        } else {
            let (noun, verb) = if first_missing == last_missing {
                ("verse", "isn't")
            } else {
                ("verses", "aren't")
            };
            format!(
                "In {what}, {noun} {} {verb} numbered in the {translation_name}.",
                verse_list(&missing),
            )
        }
    })
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
}
