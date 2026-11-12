//! Writes the bundled King James Version, `data/kjv.json`: the file as it
//! is, with the chapters it takes from eBible.org's USFM in `sources/kjv/`
//! (`kjv::TAKEN`) replaced by what `ingest/kjv.rs` reads there, and every
//! other verse copied through byte for byte. What it replaced goes to
//! `sources/kjv/kjv_2_29_1_replaced.tsv`. `tests/kjv_source.rs` checks both
//! are what this writes.
//!
//! From `src-tauri/`: `cargo run --example kjv_splice`
//!
//! Run on a file it has already spliced, it changes nothing: the file is
//! the same, and the list of what it replaced (the 2.29.1 file's verses)
//! is left as it is.

#[path = "../ingest/kjv.rs"]
mod kjv;

use std::io::Read;
use std::path::Path;

/// The USFM books from eBible.org's zip, in canonical order.
fn usfm_books(zip_path: &Path) -> Vec<String> {
    let file = std::fs::File::open(zip_path).expect("open eng-kjv2006_usfm.zip");
    let mut archive = zip::ZipArchive::new(file).expect("read eng-kjv2006_usfm.zip");
    let names: Vec<String> = archive
        .file_names()
        .map(|name| name.map(|n| n.into_owned()))
        .collect::<Result<_, _>>()
        .expect("the zip's file names");
    kjv::USFM_BOOKS
        .iter()
        .map(|code| {
            let name = kjv::entry_name(&names, code).unwrap_or_else(|e| panic!("{e}"));
            let mut text = String::new();
            archive
                .by_name(&name)
                .unwrap_or_else(|e| panic!("{name}: {e}"))
                .read_to_string(&mut text)
                .expect("UTF-8 USFM");
            text
        })
        .collect()
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let sources = root.join("../sources/kjv");
    let file = root.join("data/kjv.json");
    let bytes = std::fs::read(&file).expect("read kjv.json");
    let bundled = kjv::read_bundled(&bytes).unwrap_or_else(|e| panic!("kjv.json: {e}"));
    let derived = kjv::derive(&usfm_books(&sources.join("eng-kjv2006_usfm.zip")))
        .unwrap_or_else(|e| panic!("{e}"));
    let spliced = kjv::splice(&bundled, &derived).unwrap_or_else(|e| panic!("{e}"));
    // What it replaced, put back, is the file it read.
    let restored =
        kjv::unsplice(&spliced.books, &spliced.replaced).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        restored == bundled,
        "putting back what was replaced isn't the file read"
    );
    if spliced.json.as_bytes() == bytes.as_slice() {
        println!("kjv.json is already spliced; nothing changed");
        return;
    }
    std::fs::write(&file, &spliced.json).expect("write kjv.json");
    std::fs::write(sources.join("kjv_2_29_1_replaced.tsv"), &spliced.replaced)
        .expect("write kjv_2_29_1_replaced.tsv");
    let verses = |books: &[kjv::Book]| -> usize {
        books.iter().flat_map(|b| &b.chapters).map(Vec::len).sum()
    };
    println!(
        "kjv.json: {} verses, {} before; replaced {} verses with {}",
        verses(&spliced.books),
        verses(&bundled),
        spliced.replaced.lines().count(),
        kjv::replaced_tsv(&spliced.books)
            .expect("the spliced verses")
            .lines()
            .count(),
    );
}
