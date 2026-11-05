//! What the app knows apart from the database's tables: chapter
//! navigation (`bible`), reading what was typed into the search box
//! (`query`), the KJV's and BSB's markup (`kjv_markup`, `usfm_note`), and
//! `Store`, the database access the commands go through.

pub mod bible;
pub mod kjv_markup;
pub mod query;
pub mod store;
pub mod usfm_note;

pub use store::Store;
