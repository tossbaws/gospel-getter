//! What the app knows apart from the database's tables: chapter
//! navigation (`bible`), a cross-reference's target (`cross_reference`),
//! reading what was typed into the search box (`query`), the KJV's and BSB's
//! markup (`kjv_markup`, `usfm_note`), what a translation numbers of a
//! passage (`passage`), and `Store`, the database access the commands go
//! through.

pub mod bible;
pub mod cross_reference;
pub mod kjv_markup;
pub mod passage;
pub mod query;
pub mod store;
pub mod usfm_note;

pub use store::Store;
