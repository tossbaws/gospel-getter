//! The bundled King James Version against eBible.org's USFM in
//! `sources/kjv/` (see its README). `data/kjv.json` is the 2.29.1 file
//! with exactly the chapters `ingest/kjv.rs` takes from the USFM
//! (`kjv::TAKEN`: ten chapters and Revelation 13:1) replaced by what it
//! reads there, and every other verse byte for byte as it was. Also
//! pinned: the KJV's 31,102 verses, every chapter's count (the same as
//! Berean's verse list, an independent one), the verses the earlier source
//! left out or split, and how the ingest maps the USFM.

#[path = "../ingest/kjv.rs"]
mod kjv;

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The zip's SHA-256, as downloaded from ebible.org on 2026-11-10 (see
/// `sources/kjv/README.md`).
const KJV_USFM_ZIP_SHA256: &str =
    "dce1ec3ea45a155ac05e975fc8d817da403a817713d554eb9cc3a2177a6cfd1c";

/// `data/kjv.json` as 2.29.1 bundled it (and every version before 2.29.2),
/// at main fa7c1ab: `git show fa7c1ab:src-tauri/data/kjv.json | sha256sum`.
const KJV_2_29_1_JSON_SHA256: &str =
    "d9446b7a70a71fa431fe84d31f7758a0bda8eb4b6d048e0d098a79d51130b646";

/// Every verse of the KJV outside `kjv::TAKEN`, 30,715 of them, hashed as
/// "book|chapter|verse|text\n" each (the book's name, the text exactly as
/// the file has it), in the file's order: from `data/kjv.json` at main
/// fa7c1ab, the 2.29.1 file, computed with Python's `json` and `hashlib`
/// over `git show fa7c1ab:src-tauri/data/kjv.json`, skipping the ten
/// chapters and Revelation 13:1. The bundled file must give the same.
const KJV_OUTSIDE_TAKEN_SHA256: &str =
    "b6308cdd7801bcd6f00b66c246dd00e3e769104978fa9514a69f4878db64e14b";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &str) -> Vec<u8> {
    std::fs::read(root().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The USFM books from the zip, in canonical order.
fn usfm_books() -> &'static Vec<String> {
    static BOOKS: OnceLock<Vec<String>> = OnceLock::new();
    BOOKS.get_or_init(|| {
        let file = std::fs::File::open(root().join("../sources/kjv/eng-kjv2006_usfm.zip")).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let names: Vec<String> = archive
            .file_names()
            .map(|name| name.map(|n| n.into_owned()))
            .collect::<Result<_, _>>()
            .unwrap();
        kjv::USFM_BOOKS
            .iter()
            .map(|code| {
                let mut text = String::new();
                archive
                    .by_name(&kjv::entry_name(&names, code).unwrap())
                    .unwrap()
                    .read_to_string(&mut text)
                    .unwrap();
                text
            })
            .collect()
    })
}

/// What the ingest reads in the whole USFM.
fn derived() -> &'static Vec<kjv::Book> {
    static BOOKS: OnceLock<Vec<kjv::Book>> = OnceLock::new();
    BOOKS.get_or_init(|| kjv::derive(usfm_books()).unwrap())
}

/// The bundled file, read as the splice reads it.
fn bundled() -> &'static Vec<kjv::Book> {
    static BOOKS: OnceLock<Vec<kjv::Book>> = OnceLock::new();
    BOOKS.get_or_init(|| kjv::read_bundled(&read("data/kjv.json")).unwrap())
}

fn book(books: &[kjv::Book], name: &str) -> usize {
    books.iter().position(|b| b.name == name).unwrap()
}

fn verse(name: &str, chapter: usize, verse: usize) -> &'static str {
    &bundled()[book(bundled(), name)].chapters[chapter - 1][verse - 1]
}

fn derived_verse(name: &str, chapter: usize, verse: usize) -> &'static str {
    &derived()[book(derived(), name)].chapters[chapter - 1][verse - 1]
}

#[test]
fn the_raw_source_is_the_file_ebible_published() {
    assert_eq!(
        sha256(&read("../sources/kjv/eng-kjv2006_usfm.zip")),
        KJV_USFM_ZIP_SHA256
    );
}

/// Every verse the bundled file doesn't take from the USFM is the 2.29.1
/// file's, byte for byte, in its place.
#[test]
fn every_verse_outside_the_chapters_taken_is_as_it_was() {
    let mut hash = Sha256::new();
    let mut verses = 0;
    for (b, book) in bundled().iter().enumerate() {
        for (c, chapter) in book.chapters.iter().enumerate() {
            for (v, text) in chapter.iter().enumerate() {
                if kjv::taken((b + 1, c + 1, v + 1)) {
                    continue;
                }
                hash.update(format!("{}|{}|{}|{text}\n", book.name, c + 1, v + 1));
                verses += 1;
            }
        }
    }
    let hash: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!((verses, hash.as_str()), (30_715, KJV_OUTSIDE_TAKEN_SHA256));
}

/// The bundled file is exactly the splice of the 2.29.1 file and the USFM:
/// putting back what `sources/kjv/kjv_2_29_1_replaced.tsv` says was
/// replaced gives the 2.29.1 file, byte for byte, and splicing that again
/// gives the bundled file and that list, byte for byte.
#[test]
fn the_bundled_kjv_is_the_2_29_1_file_with_the_chapters_taken_replaced() {
    let replaced = String::from_utf8(read("../sources/kjv/kjv_2_29_1_replaced.tsv")).unwrap();
    let before = kjv::unsplice(bundled(), &replaced).unwrap();
    let before_json = serde_json::to_string(&before).unwrap();
    assert_eq!(sha256(before_json.as_bytes()), KJV_2_29_1_JSON_SHA256);
    assert_eq!(replaced.lines().count(), 385);

    let spliced = kjv::splice(&before, derived()).unwrap();
    assert!(
        spliced.json.as_bytes() == read("data/kjv.json").as_slice(),
        "data/kjv.json isn't the splice; run `cargo run --example kjv_splice` on the 2.29.1 file"
    );
    assert!(&spliced.books == bundled());
    assert_eq!(spliced.replaced, replaced);
    // Splicing the bundled file changes nothing.
    assert_eq!(
        kjv::splice(bundled(), derived()).unwrap().json,
        spliced.json
    );

    // What's taken is the USFM's, exactly.
    for (b, book) in bundled().iter().enumerate() {
        for (c, chapter) in book.chapters.iter().enumerate() {
            for (v, text) in chapter.iter().enumerate() {
                if kjv::taken((b + 1, c + 1, v + 1)) {
                    assert_eq!(text, &derived()[b].chapters[c][v], "{} {c}:{v}", book.name);
                }
            }
        }
    }
}

#[test]
fn the_kjv_has_its_31_102_verses_in_the_standard_chapters() {
    let books = bundled();
    assert_eq!(books.len(), 66);
    assert_eq!(books.iter().filter(|b| b.testament == "OT").count(), 39);
    assert_eq!(books.iter().map(|b| b.chapters.len()).sum::<usize>(), 1_189);
    let verses: usize = books.iter().flat_map(|b| &b.chapters).map(Vec::len).sum();
    assert_eq!(verses, 31_102);
    // The USFM numbers the same.
    let usfm_verses: usize = usfm_books().iter().map(|b| b.matches("\\v ").count()).sum();
    assert_eq!(usfm_verses, 31_102);

    // Every chapter has as many verses as Berean's list gives it: a verse
    // list of the same English numbering, from another publisher.
    let raw = String::from_utf8(read("../sources/bsb/bsb.txt")).unwrap();
    let mut berean: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    let mut names: Vec<String> = Vec::new();
    for line in raw.split("\r\n").skip(3).filter(|l| !l.is_empty()) {
        let (reference, _) = line.split_once('\t').unwrap();
        let (book, numbers) = reference.rsplit_once(' ').unwrap();
        let (c, v) = numbers.split_once(':').unwrap();
        if names.last().map(String::as_str) != Some(book) {
            names.push(book.to_string());
        }
        let count = berean.entry((names.len(), c.parse().unwrap())).or_default();
        *count = (*count).max(v.parse().unwrap());
    }
    for (b, book) in books.iter().enumerate() {
        for (c, chapter) in book.chapters.iter().enumerate() {
            assert_eq!(
                Some(&chapter.len()),
                berean.get(&(b + 1, c + 1)),
                "{} {}",
                book.name,
                c + 1
            );
        }
    }
    assert_eq!(berean.len(), 1_189);

    // The ten chapters an earlier source (thiagobodruk/bible) numbered
    // wrongly: six verses it left out, four it split.
    let count = |name: &str, chapter: usize| books[book(books, name)].chapters[chapter - 1].len();
    for (name, chapter, verses) in [
        ("1 Samuel", 20, 42),
        ("1 Kings", 22, 53),
        ("Matthew", 2, 23),
        ("Matthew", 22, 46),
        ("Matthew", 26, 75),
        ("Mark", 4, 41),
        ("Mark", 7, 37),
        ("Mark", 8, 38),
        ("3 John", 1, 14),
        ("Revelation", 12, 17),
    ] {
        assert_eq!(count(name, chapter), verses, "{name} {chapter}");
    }
}

#[test]
fn the_verses_an_earlier_source_lost_are_the_kjvs() {
    assert!(verse("Matthew", 2, 16).contains("mocked of the wise men"));
    assert!(verse("Matthew", 2, 17).contains("spoken by Jeremy the prophet"));
    assert!(verse("Matthew", 2, 23).contains("Nazarene"));
    assert!(verse("Revelation", 13, 1).starts_with("And I stood upon the sand of the sea"));
    assert_eq!(bundled()[65].chapters[11].len(), 17, "Revelation 12");
    assert_eq!(bundled()[63].chapters[0].len(), 14, "3 John");
    assert!(verse("Matthew", 22, 1).contains("spake unto them again by parables"));
    assert!(verse("Mark", 4, 40).contains("how is it that ye have no faith?"));
}

/// A supplied span's closing punctuation stands after its `}`: only the
/// brace is placed, and every character stays where the source has it.
#[test]
fn supplied_words_closing_punctuation_stands_after_the_brace() {
    let read = |verses: &str| {
        kjv::read_book("GEN", &format!("\\id GEN test\n\\c 1\n\\p\n{verses}")).map(|c| c[0].clone())
    };
    assert_eq!(
        read(
            "\\v 1 thine only \\add son:\\add* and\n\
             \\v 2 seeing \\add \\+w it|strong=\"H7200\"\\+w*;\\add* \\w nor|strong=\"H3808\"\\w*\n\
             \\v 3 to reveal \\add him.\\add*\n\
             \\v 4 for \\add nought?\\add* yea, \\add what is it!\\add* so\n\
             \\v 5 he saw \\add \\+w it|strong=\"H7200\"\\+w*: \\add*and went\n\
             \\v 6 and the \\add other\\add*, the \\add tarry: for\\add* I\n"
        )
        .unwrap(),
        [
            "thine only {son}: and",
            "seeing {it}; nor",
            "to reveal {him}.",
            "for {nought}? yea, {what is it}! so",
            "he saw {it}: and went",
            "and the {other}, the {tarry: for} I",
        ]
    );
    // A subscription's supplied words too.
    assert_eq!(
        read("\\v 1 Amen.\n\\s1 Written \\add and sent:\\add* by Phebe.\n").unwrap(),
        ["Amen. «Written {and sent}: by Phebe.»"]
    );
    // Supplied words that are only punctuation can't be read.
    assert!(read("\\v 1 so \\add :\\add* be it\n").is_err());

    // In the whole source: no supplied span keeps closing punctuation
    // inside its brace (358 of them end with some), and the spans that end
    // a verse are exactly the notes the USFM has.
    for (book, usfm) in derived().iter().zip(usfm_books()) {
        let mut closing_runs = 0;
        for (c, chapter) in book.chapters.iter().enumerate() {
            for (v, text) in chapter.iter().enumerate() {
                let spans: Vec<(usize, usize)> = text
                    .match_indices('{')
                    .map(|(start, _)| (start, start + text[start..].find('}').unwrap() + 1))
                    .collect();
                let mut tail = text.len();
                for &(start, end) in spans.iter().rev() {
                    if text[end..tail].trim().is_empty() {
                        tail = start;
                        closing_runs += 1;
                    } else {
                        break;
                    }
                }
                for &(start, end) in spans.iter().filter(|(start, _)| *start < tail) {
                    let inner = &text[start + 1..end - 1];
                    assert!(
                        !inner.ends_with([',', ';', ':', '.', '?', '!']),
                        "{} {}:{}: {inner:?}",
                        book.name,
                        c + 1,
                        v + 1
                    );
                }
            }
        }
        assert_eq!(
            closing_runs,
            usfm.matches("\\f + ").count(),
            "{}",
            book.name
        );
    }
}

#[test]
fn the_markup_is_mapped_as_the_ingest_says() {
    // Supplied words in braces, notes after the verse, a title before it.
    assert_eq!(
        derived_verse("Psalms", 23, 1),
        "[A Psalm of David.] The LORD {is} my shepherd; I shall not want."
    );
    assert_eq!(
        derived_verse("Psalms", 1, 1),
        "Blessed {is} the man that walketh not in the counsel of the ungodly, nor standeth \
         in the way of sinners, nor sitteth in the seat of the scornful. {ungodly: or, wicked}"
    );
    assert_eq!(
        derived_verse("Genesis", 1, 1),
        "In the beginning God created the heaven and the earth."
    );
    assert_eq!(
        derived_verse("Genesis", 22, 16),
        "And said, By myself have I sworn, saith the LORD, for because thou hast done this \
         thing, and hast not withheld thy son, thine only {son}:"
    );
    // Psalm 119's acrostic headings, and an epistle's subscription.
    assert!(derived_verse("Psalms", 119, 1).starts_with("א ALEPH. Blessed {are} the undefiled"));
    assert!(derived_verse("Romans", 16, 27).ends_with(
        "Amen. «Written to the Romans from Corinthus, {and sent} by Phebe servant of the \
         church at Cenchrea.»"
    ));

    // No USFM is left, and every brace is a supplied-words marker's or a
    // note's: one span each.
    for (book, usfm) in derived().iter().zip(usfm_books()) {
        let mut braces = (0, 0);
        for chapter in &book.chapters {
            for text in chapter {
                assert!(!text.contains(['\\', '|']), "{}: {text}", book.name);
                assert_eq!(text.trim(), text);
                assert!(!text.contains("  "), "{}: {text}", book.name);
                braces.0 += text.matches('{').count();
                braces.1 += text.matches('}').count();
            }
        }
        let spans = usfm.matches("\\add ").count()
            + usfm.matches("\\+add ").count()
            + usfm.matches("\\f + ").count();
        assert_eq!(braces, (spans, spans), "{}", book.name);
    }
    let titles: usize = usfm_books()
        .iter()
        .map(|b| b.matches("\n\\d ").count())
        .sum();
    let derived_titles = derived()[18]
        .chapters
        .iter()
        .filter(|c| c[0].starts_with('['))
        .count();
    assert_eq!((titles, derived_titles), (116, 116));
}
