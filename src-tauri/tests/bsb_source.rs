//! The bundled Berean Standard Bible against Berean's own files in
//! `sources/bsb/` (see its README): `data/bsb.json` must be exactly what
//! `ingest/bsb.rs` derives from them, its verse text exactly Berean's plain
//! text, and its footnotes exactly Berean's USFM. Also pinned: the counts,
//! the verses the BSB doesn't number, how its numbering differs from the
//! bundled KJV's, and where Berean's USFM differs from their plain text
//! (`sources/bsb/artifacts.tsv`), which is never shown.

#[path = "../ingest/bsb.rs"]
mod bsb;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The raw files' SHA-256, as downloaded from bereanbible.com on
/// 2026-10-16 (see `sources/bsb/README.md`).
const BSB_TXT_SHA256: &str = "2ac3af1de52d4e68261cba91d85c320b7eadc6560e830d99e591767b8ff5ca96";
const BSB_USFM_ZIP_SHA256: &str =
    "f0c56438eb89c98e8f62c5c00461e25cc35b098ad5b179bc350cede759ee5f87";

/// The KJV's and WEB's bundled files, unchanged by the BSB's coming.
const KJV_JSON_SHA256: &str = "d9446b7a70a71fa431fe84d31f7758a0bda8eb4b6d048e0d098a79d51130b646";
const WEB_JSON_SHA256: &str = "be8550eec61d2991239fba6366e835e623235a2aa5d669d770a61b1ddec1ae72";

/// Every BSB verse, as "book|chapter|verse|text\n" in order, hashed.
const BSB_TEXT_FINGERPRINT: &str =
    "11f0bd27e15aaf1f010d83567eb45b597d735f3491a0bf437acdaf78e9703343";

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

/// The USFM books from Berean's zip, in canonical order.
fn usfm_books() -> &'static Vec<String> {
    static BOOKS: OnceLock<Vec<String>> = OnceLock::new();
    BOOKS.get_or_init(|| {
        let file = std::fs::File::open(root().join("../sources/bsb/bsb_usfm.zip")).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        bsb::USFM_BOOKS
            .iter()
            .map(|code| {
                let mut text = String::new();
                archive
                    .by_name(&format!("bsb_usfm/{code}.usfm"))
                    .unwrap()
                    .read_to_string(&mut text)
                    .unwrap();
                text
            })
            .collect()
    })
}

fn derived() -> &'static bsb::Derived {
    static DERIVED: OnceLock<bsb::Derived> = OnceLock::new();
    DERIVED.get_or_init(|| bsb::derive(&read("../sources/bsb/bsb.txt"), usfm_books()).unwrap())
}

#[derive(Deserialize)]
struct Book {
    name: String,
    testament: String,
    chapters: Vec<Vec<Option<String>>>,
    footnotes: Vec<Footnote>,
}

#[derive(Deserialize)]
struct Footnote {
    chapter: usize,
    verse: usize,
    at: usize,
    usfm: String,
}

fn bundled() -> Vec<Book> {
    serde_json::from_slice(&read("data/bsb.json")).unwrap()
}

/// `bsb.txt`'s line for "Book c:v", after its tab: read straight from the
/// file, not through the ingest.
fn txt_line(reference: &str) -> String {
    let raw = String::from_utf8(read("../sources/bsb/bsb.txt")).unwrap();
    let prefix = format!("\r\n{reference}\t");
    let start = raw.find(&prefix).unwrap_or_else(|| panic!("{reference}")) + prefix.len();
    let end = start + raw[start..].find("\r\n").unwrap();
    raw[start..end].to_string()
}

fn verse(books: &[Book], book: usize, chapter: usize, verse: usize) -> Option<&str> {
    books[book - 1].chapters[chapter - 1][verse - 1].as_deref()
}

#[test]
fn the_raw_sources_are_the_files_berean_published() {
    assert_eq!(sha256(&read("../sources/bsb/bsb.txt")), BSB_TXT_SHA256);
    assert_eq!(
        sha256(&read("../sources/bsb/bsb_usfm.zip")),
        BSB_USFM_ZIP_SHA256
    );
}

#[test]
fn the_bundled_bsb_and_its_artifacts_list_are_exactly_what_the_sources_give() {
    let derived = derived();
    assert_eq!(derived.books.len(), 66);
    assert!(
        derived.json.as_bytes() == read("data/bsb.json").as_slice(),
        "data/bsb.json isn't what sources/bsb/ derives; run `cargo run --example bsb_ingest`"
    );
    assert!(
        derived.artifacts.as_bytes() == read("../sources/bsb/artifacts.tsv").as_slice(),
        "sources/bsb/artifacts.tsv isn't what sources/bsb/ derives"
    );
}

#[test]
fn the_kjv_and_web_are_unchanged() {
    assert_eq!(sha256(&read("data/kjv.json")), KJV_JSON_SHA256);
    assert_eq!(sha256(&read("data/web.json")), WEB_JSON_SHA256);
}

#[test]
fn counts_match_the_sources() {
    let books = bundled();
    assert_eq!(books.len(), 66);
    assert_eq!(books.iter().filter(|b| b.testament == "OT").count(), 39);
    assert_eq!(books.iter().map(|b| b.chapters.len()).sum::<usize>(), 1_189);
    let verses: usize = books
        .iter()
        .flat_map(|b| &b.chapters)
        .map(|c| c.iter().flatten().count())
        .sum();
    assert_eq!(verses, 31_086);
    // The text lists 31,102 verses: those, and the 16 below, left empty.
    let raw = String::from_utf8(read("../sources/bsb/bsb.txt")).unwrap();
    assert_eq!(raw.matches("\r\n").count() - 3, 31_102);
    // The USFM numbers the same 31,086.
    let usfm_verses: usize = usfm_books().iter().map(|b| b.matches("\\v ").count()).sum();
    assert_eq!(usfm_verses, 31_086);

    // Every footnote in the USFM is bundled, once, in its own book.
    let notes: usize = books.iter().map(|b| b.footnotes.len()).sum();
    assert_eq!(notes, 4_854);
    for (book, usfm) in books.iter().zip(usfm_books()) {
        assert_eq!(
            book.footnotes.len(),
            usfm.matches("\\f + ").count(),
            "{}",
            book.name
        );
        assert_eq!(usfm.matches("\\f + ").count(), usfm.matches("\\f*").count());
    }
}

#[test]
fn the_verse_text_is_bereans_plain_text_byte_for_byte() {
    let books = bundled();
    let samples: Vec<(usize, usize, usize, String)> = [
        (1, 1, 1, "Genesis 1:1"),
        (1, 1, 3, "Genesis 1:3"),
        (1, 35, 18, "Genesis 35:18"),
        (2, 38, 28, "Exodus 38:28"),
        (35, 3, 19, "Habakkuk 3:19"),
        (43, 3, 16, "John 3:16"),
        (45, 8, 28, "Romans 8:28"),
        (40, 17, 20, "Matthew 17:20"),
    ]
    .into_iter()
    .map(|(b, c, v, r)| (b, c, v, r.to_string()))
    .chain((1..=6).map(|v| (19, 23, v, format!("Psalm 23:{v}"))))
    .collect();
    for (b, c, v, reference) in samples {
        assert_eq!(
            verse(&books, b, c, v),
            Some(txt_line(&reference).as_str()),
            "{reference}"
        );
    }
    assert_eq!(
        verse(&books, 1, 1, 1),
        Some("In the beginning God created the heavens and the earth.")
    );
    assert_eq!(
        verse(&books, 43, 3, 16),
        Some(
            "For God so loved the world that He gave His one and only Son, that everyone who \
             believes in Him shall not perish but have eternal life."
        )
    );
    // The Psalm titles are in verse 1, as Berean has them.
    assert_eq!(
        verse(&books, 19, 23, 1),
        Some("A Psalm of David. The LORD is my shepherd; I shall not want.")
    );
    // Habakkuk 3:19 ends as the text has it, not as the USFM's heading.
    assert!(verse(&books, 35, 3, 19).unwrap().ends_with(
        "He makes me walk upon the heights! For the choirmaster. With stringed instruments."
    ));

    let mut hash = Sha256::new();
    for (b, book) in books.iter().enumerate() {
        for (c, chapter) in book.chapters.iter().enumerate() {
            for (v, text) in chapter.iter().enumerate() {
                if let Some(text) = text {
                    hash.update(format!("{}|{}|{}|{text}\n", b + 1, c + 1, v + 1));
                }
            }
        }
    }
    let fingerprint: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(fingerprint, BSB_TEXT_FINGERPRINT);
}

#[test]
fn the_verses_the_bsb_does_not_number_are_left_out_where_they_fall() {
    let books = bundled();
    let missing: Vec<String> = books
        .iter()
        .flat_map(|b| {
            b.chapters.iter().enumerate().flat_map(move |(c, vs)| {
                vs.iter()
                    .enumerate()
                    .filter(|(_, t)| t.is_none())
                    .map(move |(v, _)| format!("{} {}:{}", b.name, c + 1, v + 1))
            })
        })
        .collect();
    assert_eq!(
        missing,
        [
            "Matthew 17:21",
            "Matthew 18:11",
            "Matthew 23:14",
            "Mark 7:16",
            "Mark 9:44",
            "Mark 9:46",
            "Mark 11:26",
            "Mark 15:28",
            "Luke 17:36",
            "Luke 23:17",
            "John 5:4",
            "Acts 8:37",
            "Acts 15:34",
            "Acts 24:7",
            "Acts 28:29",
            "Romans 16:24",
        ]
    );
    for reference in &missing {
        // Each is an empty line in the text…
        assert_eq!(txt_line(reference), "", "{reference}");
        // …and in mid-chapter: the next verse is numbered.
        let (name, numbers) = reference.rsplit_once(' ').unwrap();
        let (c, v) = numbers.split_once(':').unwrap();
        let book = books.iter().find(|b| b.name == name).unwrap();
        let chapter = &book.chapters[c.parse::<usize>().unwrap() - 1];
        assert!(
            chapter[v.parse::<usize>().unwrap()].is_some(),
            "{reference}"
        );
    }
}

#[test]
fn the_bsbs_numbering_against_the_bundled_kjvs() {
    #[derive(Deserialize)]
    struct KjvBook {
        name: String,
        chapters: Vec<Vec<String>>,
    }
    let kjv: Vec<KjvBook> = serde_json::from_slice(&read("data/kjv.json")).unwrap();
    let books = bundled();
    let mut only_kjv = Vec::new();
    let mut only_bsb = Vec::new();
    for (k, b) in kjv.iter().zip(&books) {
        assert_eq!(k.chapters.len(), b.chapters.len(), "{}", k.name);
        for (c, (kc, bc)) in k.chapters.iter().zip(&b.chapters).enumerate() {
            for v in 0..kc.len().max(bc.len()) {
                let in_kjv = v < kc.len();
                let in_bsb = bc.get(v).is_some_and(Option::is_some);
                let reference = format!("{} {}:{}", k.name, c + 1, v + 1);
                match (in_kjv, in_bsb) {
                    (true, false) => only_kjv.push(reference),
                    (false, true) => only_bsb.push(reference),
                    _ => {}
                }
            }
        }
    }
    assert_eq!(
        only_kjv,
        [
            "1 Samuel 20:43",
            "1 Kings 22:54",
            "Matthew 17:21",
            "Matthew 18:11",
            "Matthew 23:14",
            "Mark 7:16",
            "Mark 9:44",
            "Mark 9:46",
            "Mark 11:26",
            "Mark 15:28",
            "Luke 17:36",
            "Luke 23:17",
            "John 5:4",
            "Acts 8:37",
            "Acts 15:34",
            "Acts 24:7",
            "Acts 28:29",
            "Romans 16:24",
            "3 John 1:15",
            "Revelation 12:18",
        ]
    );
    assert_eq!(
        only_bsb,
        [
            "Matthew 2:23",
            "Matthew 22:46",
            "Matthew 26:75",
            "Mark 4:41",
            "Mark 7:37",
            "Mark 8:38",
        ]
    );
}

#[test]
fn each_footnote_is_bereans_usfm_exactly_and_stands_where_berean_put_it() {
    let books = bundled();
    for (book, usfm) in books.iter().zip(usfm_books()) {
        let mut last = (0, 0, 0);
        for note in &book.footnotes {
            // Its characters are the USFM's own, and they read as a note.
            assert!(
                usfm.contains(&note.usfm),
                "{} {}:{}",
                book.name,
                note.chapter,
                note.verse
            );
            assert!(note.usfm.starts_with("\\f + \\fr ") && note.usfm.ends_with("\\f*"));
            // Its own reference is its verse's.
            assert!(
                note.usfm
                    .starts_with(&format!("\\f + \\fr {}:{} ", note.chapter, note.verse)),
                "{}",
                note.usfm
            );
            // In order, and inside its verse's text.
            let text = book.chapters[note.chapter - 1][note.verse - 1]
                .as_deref()
                .expect("a note on a numbered verse");
            assert!(note.at <= text.chars().count());
            assert!((note.chapter, note.verse, note.at) >= last);
            last = (note.chapter, note.verse, note.at);
        }
    }

    // Where a few stand, by the words before them.
    let before = |b: usize, c: usize, v: usize, nth: usize| -> String {
        let note = books[b - 1]
            .footnotes
            .iter()
            .filter(|n| (n.chapter, n.verse) == (c, v))
            .nth(nth)
            .unwrap();
        let text = verse(&books, b, c, v).unwrap();
        let chars: Vec<char> = text.chars().collect();
        chars[..note.at].iter().collect()
    };
    assert!(before(1, 1, 3, 0).ends_with("“Let there be light,”"));
    assert!(before(1, 35, 18, 0).ends_with("she named him Ben-oni."));
    assert!(before(1, 35, 18, 1).ends_with("his father called him Benjamin."));
    // The USFM's "[of silver]": the note follows "silver", which both have.
    assert!(before(2, 38, 28, 0).ends_with("1,775 shekels of silver"));
    // Matthew 17:20 carries the note on the verse the BSB leaves out.
    let note = &books[39]
        .footnotes
        .iter()
        .find(|n| (n.chapter, n.verse) == (17, 20))
        .unwrap();
    assert!(
        note.usfm
            .contains("\\fv 21\\fv*But this kind does not come out")
    );
}

#[test]
fn where_the_usfm_differs_from_the_text_is_listed_and_pinned() {
    let artifacts = derived().artifacts.clone();
    let rows: Vec<Vec<&str>> = artifacts
        .lines()
        .skip(1)
        .map(|l| l.split('\t').collect())
        .collect();
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for row in &rows {
        for kind in row[1].split(" + ") {
            *kinds.entry(kind).or_default() += 1;
        }
    }
    assert_eq!(rows.len(), 1_375);
    assert_eq!(
        kinds.into_iter().collect::<Vec<_>>(),
        [
            ("brackets", 2),
            ("ellipsis", 11),
            ("spaced dash", 1_253),
            ("spacing", 57),
            ("stray [’’]", 59),
            ("stray letters", 1),
            ("text outside the verse in the USFM", 1),
        ]
    );
    let row = |reference: &str| {
        rows.iter()
            .find(|r| r[0] == reference)
            .unwrap_or_else(|| panic!("{reference}"))
            .join("\t")
    };
    assert!(row("Genesis 35:18").contains("«vvv »→«»"));
    assert!(row("Exodus 20:26").contains("«[’’]»→«»"));
    assert!(row("Numbers 26:18").contains("«. . . »→«»"));
    assert!(row("Exodus 38:28").contains("«[»→«»"));
    assert!(row("Jeremiah 22:24").contains("«»→« »"));
    assert!(row("Habakkuk 3:19").contains("«»→« For the choirmaster. With stringed instruments.»"));
    assert!(row("Genesis 1:5").starts_with("Genesis 1:5\tspaced dash\t"));

    // None of it is in what the app stores.
    let text_only: String = bundled()
        .iter()
        .flat_map(|b| b.chapters.iter().flatten().flatten().cloned())
        .collect();
    for artifact in ["vvv", "[’’]", ". . .", "[of silver]", " — ", "evenif"] {
        assert!(
            !text_only.contains(artifact),
            "{artifact:?} is in the verse text"
        );
    }
}
