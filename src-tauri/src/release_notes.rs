//! A release's notes, read for About: the little Markdown the release notes
//! use (headings, lists, paragraphs, tables, bold, code and links), as
//! blocks of plain text the page puts on screen with `textContent`. Nothing
//! here is ever HTML: a tag in the notes is text, shown as written.

use serde::Serialize;

/// A run of text, maybe bold or code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Span {
    pub text: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub strong: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub code: bool,
}

/// A block of the notes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

/// The deepest list nesting kept; deeper items are shown at this depth.
const MAX_DEPTH: u8 = 3;

/// `notes`, as blocks. A table's rows become paragraphs, their cells joined
/// by " — ", and its separator row goes.
pub fn blocks(notes: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    // The paragraph or item being read, which a plain line continues.
    let mut open: Option<(Option<u8>, String)> = None;
    let flush = |open: &mut Option<(Option<u8>, String)>, blocks: &mut Vec<Block>| {
        if let Some((depth, text)) = open.take() {
            let spans = spans(&text);
            blocks.push(match depth {
                Some(depth) => Block::Item { depth, spans },
                None => Block::Paragraph { spans },
            });
        }
    };
    for line in notes.lines() {
        let trimmed = line.trim();
        let indent = line.len() - line.trim_start().len();
        if trimmed.is_empty() {
            flush(&mut open, &mut blocks);
        } else if let Some(text) = heading(trimmed) {
            flush(&mut open, &mut blocks);
            blocks.push(Block::Heading { spans: spans(text) });
        } else if let Some(text) = item(trimmed) {
            flush(&mut open, &mut blocks);
            let depth = u8::try_from(indent / 2).unwrap_or(MAX_DEPTH).min(MAX_DEPTH);
            open = Some((Some(depth), text.to_string()));
        } else if trimmed.starts_with('|') {
            flush(&mut open, &mut blocks);
            if let Some(text) = table_row(trimmed) {
                blocks.push(Block::Paragraph {
                    spans: spans(&text),
                });
            }
        } else if let Some((_, text)) = open.as_mut() {
            text.push(' ');
            text.push_str(trimmed);
        } else {
            open = Some((None, trimmed.to_string()));
        }
    }
    flush(&mut open, &mut blocks);
    blocks
}

/// A heading's text: one to six `#`s, then a space.
fn heading(line: &str) -> Option<&str> {
    let hashes = line.len() - line.trim_start_matches('#').len();
    (1..=6)
        .contains(&hashes)
        .then(|| line[hashes..].strip_prefix(' '))
        .flatten()
        .map(str::trim)
}

/// A list item's text: `-`, `*` or `+`, then a space.
fn item(line: &str) -> Option<&str> {
    ["- ", "* ", "+ "]
        .iter()
        .find_map(|marker| line.strip_prefix(marker))
        .map(str::trim)
}

/// A table row's cells joined by " — ", or `None` for the separator row.
fn table_row(line: &str) -> Option<String> {
    let inner = line.trim_matches('|');
    let cells: Vec<&str> = inner.split('|').map(str::trim).collect();
    let separator = cells
        .iter()
        .all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':')));
    (!separator).then(|| cells.join(" \u{2014} "))
}

/// The text of a line, with `**bold**` and `` `code` `` marked and a link
/// `[text](url)` as its text. A backslash keeps the next character as it
/// is; markers that never close are text.
fn spans(text: &str) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    let push = |out: &mut Vec<Span>, text: &str, strong: bool, code: bool| {
        if text.is_empty() {
            return;
        }
        match out.last_mut() {
            Some(last) if last.strong == strong && last.code == code => last.text.push_str(text),
            _ => out.push(Span {
                text: text.to_string(),
                strong,
                code,
            }),
        }
    };
    let mut strong = false;
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('\\')
            && let Some(ch) = after.chars().next()
        {
            push(&mut out, &after[..ch.len_utf8()], strong, false);
            rest = &after[ch.len_utf8()..];
        } else if let Some(after) = rest.strip_prefix('`')
            && let Some(end) = after.find('`')
        {
            push(&mut out, &after[..end], strong, true);
            rest = &after[end + 1..];
        } else if let Some(after) = rest.strip_prefix("**")
            && (strong || after.contains("**"))
        {
            strong = !strong;
            rest = after;
        } else if let Some((label, after)) = link(rest) {
            push(&mut out, label, strong, false);
            rest = after;
        } else {
            let ch = rest.chars().next().unwrap_or_default();
            push(&mut out, &rest[..ch.len_utf8()], strong, false);
            rest = &rest[ch.len_utf8()..];
        }
    }
    out
}

/// `[label](url)` at the start of `text`: the label, and what follows.
fn link(text: &str) -> Option<(&str, &str)> {
    let after = text.strip_prefix('[')?;
    let close = after.find("](")?;
    let label = &after[..close];
    let url_and_rest = &after[close + 2..];
    let end = url_and_rest.find(')')?;
    (!label.is_empty() && !label.contains('[')).then(|| (label, &url_and_rest[end + 1..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str) -> Span {
        Span {
            text: text.into(),
            strong: false,
            code: false,
        }
    }

    fn strong(text: &str) -> Span {
        Span {
            strong: true,
            ..plain(text)
        }
    }

    fn code(text: &str) -> Span {
        Span {
            code: true,
            ..plain(text)
        }
    }

    #[test]
    fn headings_lists_and_paragraphs() {
        let notes = "Gospel Getter 2.13.1 fixes two things.\n\n## Fixes\n- **Search** finds\n  words again.\n  - Even `G`.\n- The [About](https://example.com/about) box.\n\nThat's all.";
        assert_eq!(
            blocks(notes),
            vec![
                Block::Paragraph {
                    spans: vec![plain("Gospel Getter 2.13.1 fixes two things.")]
                },
                Block::Heading {
                    spans: vec![plain("Fixes")]
                },
                Block::Item {
                    depth: 0,
                    spans: vec![strong("Search"), plain(" finds words again.")]
                },
                Block::Item {
                    depth: 1,
                    spans: vec![plain("Even "), code("G"), plain(".")]
                },
                Block::Item {
                    depth: 0,
                    spans: vec![plain("The About box.")]
                },
                Block::Paragraph {
                    spans: vec![plain("That's all.")]
                },
            ]
        );
    }

    #[test]
    fn tables_become_lines_of_cells() {
        let notes = "| File | Platform |\n|---|:---:|\n| `a.deb` | Debian |";
        assert_eq!(
            blocks(notes),
            vec![
                Block::Paragraph {
                    spans: vec![plain("File \u{2014} Platform")]
                },
                Block::Paragraph {
                    spans: vec![code("a.deb"), plain(" \u{2014} Debian")]
                },
            ]
        );
    }

    /// HTML in the notes is text: the page shows it as written.
    #[test]
    fn markup_and_unclosed_markers_are_text() {
        assert_eq!(
            blocks("<img src=x onerror=\"alert(1)\"> & <b>bold</b> **open `tick [x](y"),
            vec![Block::Paragraph {
                spans: vec![plain(
                    "<img src=x onerror=\"alert(1)\"> & <b>bold</b> **open `tick [x](y"
                )]
            }]
        );
        assert_eq!(
            blocks(r"a \*\*b\*\* \`c\`"),
            vec![Block::Paragraph {
                spans: vec![plain("a **b** `c`")]
            }]
        );
        assert_eq!(
            blocks("####### seven"),
            vec![Block::Paragraph {
                spans: vec![plain("####### seven")]
            }]
        );
    }

    #[test]
    fn deep_lists_stop_at_three() {
        assert_eq!(
            blocks("          - deep"),
            vec![Block::Item {
                depth: 3,
                spans: vec![plain("deep")]
            }]
        );
    }

    #[test]
    fn blocks_serialize_as_the_page_reads_them() {
        assert_eq!(
            serde_json::to_value(blocks("## New\n- **Bold** and `code`")).unwrap(),
            serde_json::json!([
                { "kind": "heading", "spans": [{ "text": "New" }] },
                { "kind": "item", "depth": 0, "spans": [
                    { "text": "Bold", "strong": true },
                    { "text": " and " },
                    { "text": "code", "code": true },
                ] },
            ])
        );
    }

    /// Every release's notes so far read cleanly: no Markdown markers are
    /// left in the text (bar the KJV-style "*" nobody uses here).
    #[test]
    fn real_release_notes_read_cleanly() {
        let notes = "Gospel Getter 2.12.0 is a new look.\n\n## A new look (#61)\n- **Scripture is set in Literata**, a typeface.\n- **Three new themes:**\n  - **Chi Rho**, the new default.\n\n## Downloads\n| File | Platform |\n|---|---|\n| `Gospel-Getter_amd64.AppImage` | Linux |\n\nBuilt by run 38676371496 from [the tag](https://github.com/x).";
        let blocks = blocks(notes);
        assert_eq!(blocks.len(), 9);
        for block in &blocks {
            let (Block::Heading { spans } | Block::Paragraph { spans } | Block::Item { spans, .. }) =
                block;
            for span in spans {
                for marker in ["**", "`", "](", "## ", "|"] {
                    assert!(!span.text.contains(marker), "{marker} left in {span:?}");
                }
            }
        }
    }
}
