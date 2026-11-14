//! The commands' answers that the backend sends and the page reads whole,
//! field for field: one definition of each, so neither side can rename,
//! retype or drop a field the other still reads. The backend (`src-tauri`)
//! serializes them and the page's Rust pieces (`ui-leptos`) deserialize
//! them, each crate under the names it already used.
//!
//! This one file is compiled into both crates, as their `wire` module
//! (`#[path]`), with its parts by area under `wire/`. It may use serde and
//! nothing else only one crate has: no Tauri, SQLx, chrono or web-sys
//! types. A shape the page reads only part of, or reads as another type,
//! stays in `ui-leptos/src/dto.rs`, whose header says why.
//!
//! Each part's tests pin its shapes' JSON to literals, in both crates'
//! `cargo test`. Text fields carry what the backend wrote, unchanged
//! (`SearchResultDto::text` is a verse's stored text).

#[path = "wire/notes.rs"]
mod notes;
#[path = "wire/reader.rs"]
mod reader;
#[path = "wire/release_notes.rs"]
mod release_notes;

pub use notes::*;
pub use reader::*;
pub use release_notes::*;

/// `value` is written as `json`, byte for byte, and `json` reads back as
/// `value`. The literals are the JSON the commands sent before these
/// shapes were shared: a test that needs a new literal means the wire
/// changed.
#[cfg(test)]
#[track_caller]
fn pin<T>(value: &T, json: &str)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    assert_eq!(serde_json::to_string(value).expect("writes"), json);
    assert_eq!(&serde_json::from_str::<T>(json).expect("reads"), value);
}
