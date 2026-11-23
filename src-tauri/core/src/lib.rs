//! What Gospel Getter knows without its window: the SQLite database (`db`),
//! the domain logic over it (`domain`), and the files and text made from
//! it (the backup format, the notes export, a copied passage). A crate of
//! its own, free of Tauri, so that `gospel-getter-cli` reads a reader's
//! install through the app's own code without linking the webview, and the
//! tests of that code link no webview either. The app re-exports these
//! modules, so inside it they keep their `crate::` paths.

// The books' abbreviations, shared with the page's Rust pieces (ui-leptos),
// which compile the same file.
#[path = "../../../shared/book_aliases.rs"]
pub mod book_aliases;
// The values the page must agree on (the highlight colors, the support
// page), shared with the page's Rust pieces (ui-leptos), which compile the
// same file.
#[path = "../../../shared/constants.rs"]
pub mod constants;
pub mod db;
pub mod domain;
pub mod markdown;
pub mod notes_export;
pub mod passage_copy;
// How a passage is named ("John 3:16–18"), shared with the page's Rust
// pieces (ui-leptos), which compile the same file.
#[path = "../../../shared/passage_reference.rs"]
pub mod passage_reference;
// The saved settings' one table, shared with the page's Rust pieces
// (ui-leptos), which compile the same file.
#[path = "../../../shared/preferences.rs"]
pub mod preferences;
pub mod reader_data;
pub mod release_notes;
pub mod start_here;
// A verse's parts and footnotes as shown, shared with the page's Rust
// pieces (ui-leptos), which compile the same file.
#[path = "../../../shared/verse_parts.rs"]
pub mod verse_parts;
// The commands' answers the page reads whole, shared with the page's Rust
// pieces (ui-leptos), which compile the same file.
#[path = "../../../shared/wire.rs"]
pub mod wire;
