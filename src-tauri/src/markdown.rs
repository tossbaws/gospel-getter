//! Plain text as Markdown that shows exactly the characters written:
//! everything Markdown or HTML could read as structure is escaped. The
//! notes export (`notes_export`) and Copy as… Markdown (`passage_copy`)
//! both write with these.

/// Characters that could start inline Markdown or HTML anywhere in a line:
/// emphasis, code, links and images, raw HTML and autolinks, entities,
/// GFM strikethrough and tables, and the backslash itself.
const INLINE_SPECIAL: &[char] = &['\\', '`', '*', '_', '[', ']', '<', '>', '&', '~', '|'];

/// Characters that could start a block (a heading, list, quote, setext
/// underline or thematic break) at the start of a line, beyond those
/// `INLINE_SPECIAL` already escapes.
const LINE_START_SPECIAL: &[char] = &['#', '-', '+', '='];

/// Plain text within a line, with every Markdown/HTML character
/// backslash-escaped (CommonMark shows an escaped punctuation character as
/// itself).
pub fn escape_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if INLINE_SPECIAL.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// One line of plain text, escaped so it can't start a block either.
/// Leading spaces and tabs (which could make an indented code block)
/// become no-break spaces, so the indent still shows.
fn escape_line(line: &str) -> String {
    let rest = line.trim_start_matches([' ', '\t']);
    let mut out = String::new();
    for c in line[..line.len() - rest.len()].chars() {
        out.push_str(if c == '\t' {
            "\u{a0}\u{a0}\u{a0}\u{a0}"
        } else {
            "\u{a0}"
        });
    }
    let escaped = escape_inline(rest);
    // "1." or "1)" would start an ordered list.
    let digits = escaped.bytes().take_while(u8::is_ascii_digit).count();
    if (1..=9).contains(&digits) && matches!(escaped.as_bytes().get(digits), Some(b'.' | b')')) {
        out.push_str(&escaped[..digits]);
        out.push('\\');
        out.push_str(&escaped[digits..]);
    } else {
        if escaped.starts_with(LINE_START_SPECIAL) {
            out.push('\\');
        }
        out.push_str(&escaped);
    }
    out
}

/// Plain text that may run over several lines, as Markdown that shows the
/// same lines: blank lines separate paragraphs (several count as one),
/// and each single line break is a hard break (a backslash at the end of
/// the line). Trailing spaces, which show as nothing, are dropped.
pub fn escape_block(text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut blank = false;
    for line in text.split('\n') {
        let line = line.trim_end_matches([' ', '\t', '\r']);
        if line.trim().is_empty() {
            blank = !lines.is_empty();
            continue;
        }
        if blank {
            lines.push(String::new());
            blank = false;
        }
        lines.push(escape_line(line));
    }
    for i in 1..lines.len() {
        if !lines[i].is_empty() && !lines[i - 1].is_empty() {
            lines[i - 1].push('\\');
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_start_can_never_begin_a_block() {
        for (line, expected) in [
            ("# Heading", r"\# Heading"),
            ("## Two", r"\## Two"),
            ("- item", r"\- item"),
            ("* item", r"\* item"),
            ("+ item", r"\+ item"),
            ("> quote", r"\> quote"),
            ("1. first", r"1\. first"),
            ("12) twelfth", r"12\) twelfth"),
            ("```rust", r"\`\`\`rust"),
            ("~~~", r"\~\~\~"),
            ("---", r"\---"),
            ("===", r"\==="),
            ("___", r"\_\_\_"),
            ("| a | b |", r"\| a \| b \|"),
            ("2026 was a year.", "2026 was a year."),
            ("1.5 miles", r"1\.5 miles"),
        ] {
            assert_eq!(escape_block(line), expected, "{line:?}");
        }
    }

    #[test]
    fn inline_markdown_and_html_are_shown_as_typed() {
        assert_eq!(
            escape_block("so *loved* the _world_, `code`, [a](b) and \\ too"),
            r"so \*loved\* the \_world\_, \`code\`, \[a\](b) and \\ too"
        );
        assert_eq!(
            escape_block("<script>alert(1)</script> &amp; <b>bold</b>"),
            r"\<script\>alert(1)\</script\> \&amp; \<b\>bold\</b\>"
        );
        assert_eq!(escape_block("~~struck~~"), r"\~\~struck\~\~");
        // Indentation stays, but never makes a code block.
        assert_eq!(
            escape_block("    indented\n\tone tab"),
            "\u{a0}\u{a0}\u{a0}\u{a0}indented\\\n\u{a0}\u{a0}\u{a0}\u{a0}one tab"
        );
    }

    #[test]
    fn line_breaks_and_blank_lines_are_kept() {
        assert_eq!(
            escape_block("First line\nsecond line\n\nNew paragraph"),
            "First line\\\nsecond line\n\nNew paragraph"
        );
        // Several blank (or blank-looking) lines are one paragraph break;
        // trailing spaces go.
        assert_eq!(escape_block("a  \n\n \n\t\nb"), "a\n\nb");
        // A line ending in a backslash keeps it, and still breaks.
        assert_eq!(escape_block("C:\\\nnext"), "C:\\\\\\\nnext");
    }
}
