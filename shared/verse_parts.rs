//! A verse as the app shows it, piece by piece: its parts (its words, the
//! words the translators supplied, and where a margin note stands) and its
//! footnotes (a KJV margin note, or a USFM note's runs). The backend
//! (`src-tauri`) builds them from the stored text (`domain::kjv_markup`,
//! `domain::usfm_note`) and sends them with every verse; the page's Rust
//! pieces (`ui-leptos`) read them back to draw the verse (`reader_text`).
//!
//! This one file is compiled into both crates, as their `verse_parts`
//! module (`#[path]`), so the kinds of part and run, and the names they
//! travel by, are written once. Nothing here changes a verse's words: every
//! text field is the stored text's own characters, split.

use serde::{Deserialize, Serialize};

/// What a [`Part`] of a verse is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PartKind {
    /// The verse's own words.
    Text,
    /// Words the translators supplied, without their braces.
    Supplied,
    /// A margin note, shown as its footnote's marker.
    Note,
}

/// One piece of a verse, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    pub kind: PartKind,
    /// The words, or for a note its text exactly as stored inside the
    /// braces.
    pub text: String,
    /// A note's marker letter ("a", "b", …); `None` for anything else.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// One of a verse's margin notes, as a footnote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Footnote {
    /// Its marker letter, lettered per verse from "a".
    pub label: String,
    /// The verse's words the note is on, before its first colon, shown in
    /// italics like a printed footnote's: "lamp" in "lamp: or, candle".
    /// `None` when the note has no lemma (see `Footnote::new` in the
    /// backend's `kjv_markup`).
    #[serde(default)]
    pub lemma: Option<String>,
    /// The rest of the note after the lemma, from its colon on: ": or,
    /// candle". The whole note when there's no lemma. `lemma` and `rest`
    /// together are always exactly `text`.
    #[serde(default)]
    pub rest: String,
    /// The note exactly as stored inside the braces, e.g.
    /// "lamp: or, candle". For a USFM note (see the backend's
    /// `usfm_note`), the note's words as shown.
    #[serde(default)]
    pub text: String,
    /// A USFM note's runs: its words, alternative readings and verse
    /// numbers. `None` for a KJV margin note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segments: Option<Vec<Segment>>,
}

/// What a [`Segment`] of a footnote is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SegmentKind {
    /// The note's own words (`\ft`).
    Text,
    /// An alternative reading (`\fqa`), in italics.
    Alt,
    /// A verse number in the note (`\fv`).
    Verse,
}

/// One run of a footnote, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub kind: SegmentKind,
    pub text: String,
}
