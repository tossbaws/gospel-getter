//! The bundled King James Version against eBible.org's USFM in
//! `sources/kjv/` (see its README): `data/kjv.json` must be exactly what
//! `ingest/kjv.rs` derives from it. Also pinned: the KJV's 31,102 verses,
//! every chapter's count (the same as Berean's verse list, an independent
//! one), and the verses an earlier source left out or split.

#[path = "../ingest/kjv.rs"]
mod kjv;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The zip's SHA-256, as downloaded from ebible.org on 2026-11-10 (see
/// `sources/kjv/README.md`).
const KJV_USFM_ZIP_SHA256: &str =
    "dce1ec3ea45a155ac05e975fc8d817da403a817713d554eb9cc3a2177a6cfd1c";

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

#[derive(Deserialize)]
struct Book {
    name: String,
    testament: String,
    chapters: Vec<Vec<String>>,
}

fn bundled() -> &'static Vec<Book> {
    static BOOKS: OnceLock<Vec<Book>> = OnceLock::new();
    BOOKS.get_or_init(|| serde_json::from_slice(&read("data/kjv.json")).unwrap())
}

fn verse(name: &str, chapter: usize, verse: usize) -> &'static str {
    let book = bundled().iter().find(|b| b.name == name).unwrap();
    &book.chapters[chapter - 1][verse - 1]
}

#[test]
fn the_raw_source_is_the_file_ebible_published() {
    assert_eq!(
        sha256(&read("../sources/kjv/eng-kjv2006_usfm.zip")),
        KJV_USFM_ZIP_SHA256
    );
}

#[test]
fn the_bundled_kjv_is_exactly_what_the_source_gives() {
    let derived = kjv::derive(usfm_books()).unwrap();
    assert_eq!(derived.books.len(), 66);
    assert!(
        derived.json.as_bytes() == read("data/kjv.json").as_slice(),
        "data/kjv.json isn't what sources/kjv/ derives; run `cargo run --example kjv_ingest`"
    );
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
    let count = |name: &str, chapter: usize| {
        books.iter().find(|b| b.name == name).unwrap().chapters[chapter - 1].len()
    };
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
    assert!(verse("Revelation", 13, 1).starts_with("And I stood upon the sand of the sea"));
    assert!(verse("Matthew", 22, 1).contains("spake unto them again by parables"));
    assert!(verse("Mark", 4, 40).contains("how is it that ye have no faith?"));
}

#[test]
fn the_markup_is_mapped_as_the_ingest_says() {
    // Supplied words in braces, notes after the verse, a title before it.
    assert_eq!(
        verse("Psalms", 23, 1),
        "[A Psalm of David.] The LORD {is} my shepherd; I shall not want."
    );
    assert_eq!(
        verse("Psalms", 1, 1),
        "Blessed {is} the man that walketh not in the counsel of the ungodly, nor standeth \
         in the way of sinners, nor sitteth in the seat of the scornful. {ungodly: or, wicked}"
    );
    assert_eq!(
        verse("Genesis", 1, 1),
        "In the beginning God created the heaven and the earth."
    );
    // Psalm 119's acrostic headings, and an epistle's subscription.
    assert!(verse("Psalms", 119, 1).starts_with("א ALEPH. Blessed {are} the undefiled"));
    assert!(verse("Romans", 16, 27).ends_with(
        "Amen. «Written to the Romans from Corinthus, {and sent} by Phebe servant of the \
         church at Cenchrea.»"
    ));

    // No USFM is left, and every brace is a supplied-words marker's or a
    // note's: one span each.
    let books = bundled();
    for (book, usfm) in books.iter().zip(usfm_books()) {
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
    let bundled_titles = books[18]
        .chapters
        .iter()
        .filter(|c| c[0].starts_with('['))
        .count();
    assert_eq!((titles, bundled_titles), (116, 116));
}
