//! Writes the bundled King James Version, `data/kjv.json`, from eBible.org's
//! USFM in `sources/kjv/` (see `ingest/kjv.rs` and that folder's README).
//! `tests/kjv_source.rs` checks the bundled file is what this writes.
//!
//! From `src-tauri/`: `cargo run --example kjv_ingest`

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
    let zip = root.join("../sources/kjv/eng-kjv2006_usfm.zip");
    let derived = kjv::derive(&usfm_books(&zip)).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(root.join("data/kjv.json"), &derived.json).expect("write kjv.json");
    let verses: usize = derived
        .books
        .iter()
        .flat_map(|b| &b.chapters)
        .map(Vec::len)
        .sum();
    println!("kjv.json: {} books, {verses} verses", derived.books.len());
}
