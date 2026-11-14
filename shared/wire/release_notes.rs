//! A release's notes as blocks, as `check_for_update` sends them in
//! `noteBlocks` (`release_notes` parses them; the page's About shows them).

use serde::{Deserialize, Serialize};

/// A run of text, maybe bold or code. `strong` and `code` are left out of
/// the JSON when false.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub strong: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub code: bool,
}

/// A block of the notes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Block {
    /// Any level of heading.
    Heading {
        spans: Vec<Span>,
    },
    Paragraph {
        spans: Vec<Span>,
    },
    /// A list item, `depth` lists deep (0 for a top-level item). Items in
    /// a row make one list.
    Item {
        depth: u8,
        spans: Vec<Span>,
    },
}

#[cfg(test)]
mod tests {
    use super::super::pin;
    use super::*;

    fn plain() -> Span {
        Span {
            text: "Fixed ".into(),
            strong: false,
            code: false,
        }
    }

    fn marked() -> Span {
        Span {
            text: "trunk".into(),
            strong: true,
            code: true,
        }
    }

    #[test]
    fn span() {
        pin(&plain(), r#"{"text":"Fixed "}"#);
        pin(&marked(), r#"{"text":"trunk","strong":true,"code":true}"#);
    }

    #[test]
    fn block() {
        pin(
            &Block::Heading {
                spans: vec![plain()],
            },
            r#"{"kind":"heading","spans":[{"text":"Fixed "}]}"#,
        );
        let paragraph = Block::Paragraph {
            spans: vec![plain(), marked()],
        };
        pin(
            &paragraph,
            r#"{"kind":"paragraph","spans":[{"text":"Fixed "},{"text":"trunk","strong":true,"code":true}]}"#,
        );
        let item = Block::Item {
            depth: 1,
            spans: vec![marked()],
        };
        pin(
            &item,
            r#"{"kind":"item","depth":1,"spans":[{"text":"trunk","strong":true,"code":true}]}"#,
        );
    }
}
