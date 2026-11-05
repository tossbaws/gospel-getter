//! What Gospel Getter knows without its window: the SQLite database (`db`)
//! and the domain logic over it (`domain`). A crate of its own, free of
//! Tauri, so that `gospel-getter-cli` reads a reader's install through the
//! app's own code without linking the webview. The app re-exports these
//! modules, so inside it they're `crate::db` and `crate::domain` as ever.

// The books' abbreviations, shared with the page's Rust pieces (ui-leptos),
// which compile the same file.
#[path = "../../../shared/book_aliases.rs"]
pub mod book_aliases;
pub mod db;
pub mod domain;
// How a passage is named ("John 3:16–18"), shared with the page's Rust
// pieces (ui-leptos), which compile the same file.
#[path = "../../../shared/passage_reference.rs"]
pub mod passage_reference;
// A verse's parts and footnotes as shown, shared with the page's Rust
// pieces (ui-leptos), which compile the same file.
#[path = "../../../shared/verse_parts.rs"]
pub mod verse_parts;
