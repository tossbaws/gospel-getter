//! Plain text as Markdown that shows exactly the characters written:
//! everything Markdown or HTML could read as structure is escaped. The
//! notes export (`notes_export`) and Copy as… Markdown (`passage_copy`)
//! both write with these.

/// Characters that could start inline Markdown or HTML anywhere in a line:
/// emphasis, code, links and images, raw HTML and autolinks, entities,
/// GFM strikethrough and tables, and the backslash itself; and Obsidian's
/// comments (`%%`, hidden in reading view), math (`$`), highlights (`==`)
/// and tags (`#` mid-line), since the notes export is written for a vault
/// too (B-04). CommonMark shows any escaped ASCII punctuation as itself, so
/// escaping one more never changes what a reader sees. No bundled verse has
/// `%`, `$`, `=` or `#` (`markdown`'s test counts them), so a quoted
/// verse's Markdown is byte for byte as before.
const INLINE_SPECIAL: &[char] = &[
    '\\', '`', '*', '_', '[', ']', '<', '>', '&', '~', '|', '%', '$', '=', '#',
];

/// Characters that could start a block (a list, or a setext underline or
/// thematic break of dashes) at the start of a line, beyond those
/// `INLINE_SPECIAL` already escapes (a heading's `#` and a setext `=` are).
const LINE_START_SPECIAL: &[char] = &['-', '+'];

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
            ("## Two", r"\#\# Two"),
            ("- item", r"\- item"),
            ("* item", r"\* item"),
            ("+ item", r"\+ item"),
            ("> quote", r"\> quote"),
            ("1. first", r"1\. first"),
            ("12) twelfth", r"12\) twelfth"),
            ("```rust", r"\`\`\`rust"),
            ("~~~", r"\~\~\~"),
            ("---", r"\---"),
            ("===", r"\=\=\="),
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
        // Obsidian's comments, math, highlights and tags (B-04).
        assert_eq!(
            escape_block("Seen %%hidden%% and $x$, ==marked== and a #tag here"),
            r"Seen \%\%hidden\%\% and \$x\$, \=\=marked\=\= and a \#tag here"
        );
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

    /// What CommonMark shows for `markdown`: each backslash-escaped ASCII
    /// punctuation character as itself.
    fn rendered(markdown: &str) -> String {
        let mut out = String::new();
        let mut chars = markdown.chars().peekable();
        while let Some(c) = chars.next() {
            match chars.peek() {
                Some(&next) if c == '\\' && next.is_ascii_punctuation() => {
                    out.push(next);
                    chars.next();
                }
                _ => out.push(c),
            }
        }
        out
    }

    /// The escaper runs over quoted verse text too (Copy as… Markdown, the
    /// notes export's quotes). No bundled verse has a character B-04 added,
    /// so their Markdown is unchanged, and a few verses' Markdown shows
    /// exactly their text.
    #[test]
    fn quoted_verses_read_exactly_as_before() {
        use crate::db::test_support::{bundled, bundled_bsb};
        use crate::domain::kjv_markup::VerseText;
        let added = |text: &str| text.matches(['%', '$', '=', '#']).count();
        for code in ["kjv", "web"] {
            let count: usize = bundled(code)
                .iter()
                .flat_map(|book| book.chapters.iter().flatten())
                .map(|verse| added(verse))
                .sum();
            assert_eq!(count, 0, "{code}");
        }
        let count: usize = bundled_bsb()
            .iter()
            .flat_map(|book| book.chapters.iter().flatten().flatten())
            .map(|verse| added(verse))
            .sum();
        assert_eq!(count, 0, "bsb");

        let kjv = bundled("kjv");
        // John 3:16, Psalm 23:1 and Hebrews 10:34 (its markup broken in the
        // source, so shown as stored).
        for (book, chapter, verse) in [(42, 3, 16), (18, 23, 1), (57, 10, 34)] {
            let stored = &kjv[book].chapters[chapter - 1][verse - 1];
            let shown = VerseText::new("kjv", stored).text;
            assert_eq!(rendered(&escape_inline(&shown)), shown);
        }
    }
}
