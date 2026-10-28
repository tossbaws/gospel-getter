//! The books' usual abbreviations and other names ("Rom" for Romans, "Mk"
//! for Mark). The backend (`src-tauri`) reads a typed reference with them
//! (`domain::query`); the page's Rust pieces (`ui-leptos`) find a note in
//! the Library's search by its book's abbreviation with them, so "Mk 4"
//! finds the notes a Go to "Mk 4" would go to.
//!
//! This one file is compiled into both crates, as their `book_aliases`
//! module (`#[path]`), so the table is written once.

/// Common abbreviations and alternative names, by canonical book number
/// (Genesis = 1 ... Revelation = 66), in the normalized form the backend's
/// `domain::query::book_key` produces: lowercase, no spaces or periods,
/// with a leading book number as a digit. Every book's own stored name
/// (normalized the same way) is accepted too, so it isn't repeated here.
pub const ALIASES: &[(i64, &[&str])] = &[
    (1, &["gen", "ge", "gn"]),
    (2, &["exod", "exo", "ex"]),
    (3, &["lev", "le", "lv"]),
    (4, &["num", "nu", "nm", "nb"]),
    (5, &["deut", "deu", "de", "dt"]),
    (6, &["josh", "jos", "jsh"]),
    (7, &["judg", "jdg", "jg", "jdgs"]),
    (8, &["rth", "ru"]),
    (9, &["1sam", "1sa", "1sm"]),
    (10, &["2sam", "2sa", "2sm"]),
    (11, &["1kgs", "1ki", "1kin"]),
    (12, &["2kgs", "2ki", "2kin"]),
    (13, &["1chron", "1chr", "1ch"]),
    (14, &["2chron", "2chr", "2ch"]),
    (15, &["ezr"]),
    (16, &["neh", "ne"]),
    (17, &["esth", "est", "es"]),
    (18, &["jb"]),
    (19, &["psalm", "ps", "psa", "psm", "pss"]),
    (20, &["prov", "pro", "prv", "pr"]),
    (21, &["eccles", "eccl", "ecc", "ec", "qoh"]),
    (
        22,
        &[
            "songofsongs",
            "song",
            "sos",
            "so",
            "canticles",
            "cant",
            "sg",
        ],
    ),
    (23, &["isa", "is"]),
    (24, &["jer", "je", "jr"]),
    (25, &["lam", "la"]),
    (26, &["ezek", "eze", "ezk"]),
    (27, &["dan", "da", "dn"]),
    (28, &["hos", "ho"]),
    (29, &["jl"]),
    (30, &["am"]),
    (31, &["obad", "ob"]),
    (32, &["jnh", "jon"]),
    (33, &["mic", "mc"]),
    (34, &["nah", "na"]),
    (35, &["hab", "hb"]),
    (36, &["zeph", "zep", "zp"]),
    (37, &["hag", "hg"]),
    (38, &["zech", "zec", "zc"]),
    (39, &["mal", "ml"]),
    (40, &["matt", "mat", "mt"]),
    (41, &["mrk", "mar", "mk", "mr"]),
    (42, &["luk", "lk"]),
    (43, &["jhn", "jn", "joh"]),
    (44, &["act", "ac"]),
    (45, &["rom", "ro", "rm"]),
    (46, &["1cor", "1co"]),
    (47, &["2cor", "2co"]),
    (48, &["gal", "ga"]),
    (49, &["ephes", "eph"]),
    (50, &["phil", "php", "pp"]),
    (51, &["col"]),
    (52, &["1thess", "1thes", "1th"]),
    (53, &["2thess", "2thes", "2th"]),
    (54, &["1tim", "1ti"]),
    (55, &["2tim", "2ti"]),
    (56, &["tit", "ti"]),
    (57, &["philem", "phm", "pm"]),
    (58, &["heb"]),
    (59, &["jas", "jm"]),
    (60, &["1pet", "1pe", "1pt"]),
    (61, &["2pet", "2pe", "2pt"]),
    (62, &["1jn", "1jhn", "1jo"]),
    (63, &["2jn", "2jhn", "2jo"]),
    (64, &["3jn", "3jhn", "3jo"]),
    (65, &["jud", "jd"]),
    (66, &["rev", "re", "rv", "revelations"]),
];

/// Book `book_id`'s abbreviations and other names, in the table's form;
/// none for a number that isn't a book.
pub fn aliases_of(book_id: i64) -> &'static [&'static str] {
    ALIASES
        .iter()
        .find(|(id, _)| *id == book_id)
        .map_or(&[], |(_, aliases)| aliases)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_book_has_its_own_row_in_order() {
        assert_eq!(ALIASES.len(), 66);
        assert_eq!(aliases_of(45), ["rom", "ro", "rm"]);
        assert_eq!(aliases_of(41), ["mrk", "mar", "mk", "mr"]);
        assert!(aliases_of(0).is_empty() && aliases_of(67).is_empty());
    }
}
