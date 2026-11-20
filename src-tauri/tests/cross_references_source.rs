//! The bundled cross-references are the file `sources/cross_references/`'s
//! README describes: its SHA-256 there is the file's.

use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[test]
fn the_bundled_cross_references_are_the_file_the_readme_pins() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let data = std::fs::read(root.join("data/cross_references.json")).expect("the bundled file");
    let hash: String = Sha256::digest(&data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let readme = std::fs::read_to_string(root.join("../sources/cross_references/README.md"))
        .expect("its README");
    assert!(
        readme.contains(&format!(
            "`src-tauri/data/cross_references.json` | `{hash}`"
        )),
        "sources/cross_references/README.md doesn't pin {hash}"
    );
}
