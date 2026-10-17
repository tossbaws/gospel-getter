//! Writes the bundled Berean Standard Bible, `data/bsb.json`, and the list
//! of where Berean's USFM and plain text differ, `sources/bsb/artifacts.tsv`,
//! from the raw files in `sources/bsb/` (see `ingest/bsb.rs` and that
//! folder's README). `tests/bsb_source.rs` checks both are what this writes.
//!
//! From `src-tauri/`: `cargo run --example bsb_ingest`

#[path = "../ingest/bsb.rs"]
mod bsb;

use std::io::Read;
use std::path::Path;

/// The USFM books from Berean's zip, in canonical order.
fn usfm_books(zip_path: &Path) -> Vec<String> {
    let file = std::fs::File::open(zip_path).expect("open bsb_usfm.zip");
    let mut archive = zip::ZipArchive::new(file).expect("read bsb_usfm.zip");
    bsb::USFM_BOOKS
        .iter()
        .map(|code| {
            let mut entry = archive
                .by_name(&format!("bsb_usfm/{code}.usfm"))
                .unwrap_or_else(|e| panic!("{code}.usfm: {e}"));
            let mut text = String::new();
            entry.read_to_string(&mut text).expect("UTF-8 USFM");
            text
        })
        .collect()
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let sources = root.join("../sources/bsb");
    let txt = std::fs::read(sources.join("bsb.txt")).expect("read bsb.txt");
    let derived = bsb::derive(&txt, &usfm_books(&sources.join("bsb_usfm.zip")))
        .unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(root.join("data/bsb.json"), &derived.json).expect("write bsb.json");
    std::fs::write(sources.join("artifacts.tsv"), &derived.artifacts).expect("write artifacts");
    let verses: usize = derived
        .books
        .iter()
        .flat_map(|b| &b.chapters)
        .map(|c| c.iter().flatten().count())
        .sum();
    let notes: usize = derived.books.iter().map(|b| b.footnotes.len()).sum();
    println!(
        "bsb.json: {} books, {verses} verses, {notes} footnotes; {} artifact lines",
        derived.books.len(),
        derived.artifacts.lines().count() - 1
    );
}
