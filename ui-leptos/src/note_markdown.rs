//! A note's text as Markdown (R17), free of the DOM so it's tested
//! natively (`cargo test`): what the card's notes, the Library's Notes
//! list and the margin show. Notes stay plain text underneath, stored
//! and backed up as typed; this only draws them.
//!
//! The subset, and nothing more: paragraphs (a blank line between) and
//! line breaks (each line of a paragraph on its own line); `**bold**`
//! (or `__bold__`) and `*italics*` (or `_italics_`); bulleted (`-`, `*`,
//! `+`) and numbered (`1.` or `1)`) lists, one level; `` `inline code` ``;
//! links `[text](https://…)` and bare `https://…` (or `http://`) web
//! addresses; and the references to passages the backend found in the
//! text (`NoteLink`), each a link to its passage. A backslash before a
//! punctuation mark shows the mark as itself.
//!
//! Nothing else is markup: HTML is shown as typed (escaped, never
//! parsed), and so are headings (`#`), quotes (`>`), code blocks, rules,
//! tables and images (`![…](…)`); a link to anything but `http` or
//! `https` is its text as typed. No address is ever fetched: a link opens
//! in the browser when it's clicked, through the backend, which takes
//! only `http` and `https` (`commands::open_web_link`).
//!
//! The emphasis rules are CommonMark's (left- and right-flanking runs of
//! `*` and `_`, matched innermost first), so what's bold or italic here is
//! what any Markdown app makes of it; what's out of the subset reads as
//! the characters typed, rather than as something the reader didn't see
//! when they typed it.

use crate::dto::NoteLink;
use crate::library_text::text_marks;
use crate::reader_text::escape_html;

/// How a note is laid out: as it's written (the card, the margin), or run
/// onto one line (the Library's previews, which show its first lines),
/// where a paragraph or a line break is a space, and a list's items
/// follow one another, each after its bullet or number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Blocks,
    OneLine,
}

/// The note `body` as HTML, with `links` (the backend's references in
/// it, by byte offsets) as links to their passages, and the words a
/// search found (`words`, `library_text::query_words`) marked.
pub fn note_html(body: &str, links: &[NoteLink], words: &[String], layout: Layout) -> String {
    let mut out = String::new();
    let render = Render { body, links, words };
    for (index, block) in blocks(body).iter().enumerate() {
        match (layout, block) {
            (Layout::Blocks, Block::Paragraph(lines)) => {
                out.push_str("<p>");
                render.inline(&mut out, lines, "<br>");
                out.push_str("</p>");
            }
            (
                Layout::Blocks,
                Block::List {
                    ordered,
                    start,
                    items,
                },
            ) => {
                let tag = if *ordered { "ol" } else { "ul" };
                out.push('<');
                out.push_str(tag);
                if *ordered && *start != 1 {
                    out.push_str(&format!(" start=\"{start}\""));
                }
                out.push('>');
                for item in items {
                    out.push_str("<li>");
                    render.inline(&mut out, item, "<br>");
                    out.push_str("</li>");
                }
                out.push_str("</");
                out.push_str(tag);
                out.push('>');
            }
            (Layout::OneLine, Block::Paragraph(lines)) => {
                if index > 0 {
                    out.push(' ');
                }
                render.inline(&mut out, lines, " ");
            }
            (
                Layout::OneLine,
                Block::List {
                    ordered,
                    start,
                    items,
                },
            ) => {
                for (n, item) in items.iter().enumerate() {
                    if index > 0 || n > 0 {
                        out.push(' ');
                    }
                    if *ordered {
                        out.push_str(&format!("{}. ", start + n as u64));
                    } else {
                        out.push_str("\u{2022} ");
                    }
                    render.inline(&mut out, item, " ");
                }
            }
        }
    }
    out
}

// ---- Blocks.

/// A line's text, as byte offsets into the note.
type Line = (usize, usize);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Block {
    /// Its lines.
    Paragraph(Vec<Line>),
    /// Its items, each its lines (the first without its marker).
    List {
        ordered: bool,
        start: u64,
        items: Vec<Vec<Line>>,
    },
}

/// A list item's marker at the start of a line (after up to three
/// spaces): whether it's numbered, its number, and where its text starts.
fn list_marker(line: &str) -> Option<(bool, u64, usize)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let after_marker = |marker_len: usize| {
        let text = &rest[marker_len..];
        // A space or tab after it, or nothing (an empty item).
        if text.is_empty() {
            Some(indent + marker_len)
        } else if text.starts_with([' ', '\t']) {
            Some(indent + marker_len + 1)
        } else {
            None
        }
    };
    if rest.starts_with(['-', '*', '+']) {
        return after_marker(1).map(|at| (false, 0, at));
    }
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if (1..=9).contains(&digits) {
        if !rest[digits..].starts_with(['.', ')']) {
            return None;
        }
        let number = rest[..digits].parse().ok()?;
        return after_marker(digits + 1).map(|at| (true, number, at));
    }
    None
}

/// The note's blocks: runs of lines between blank lines, each a
/// paragraph, a list, or a paragraph then a list (a bulleted list, or a
/// numbered one from 1, can follow a paragraph's line straight away).
fn blocks(body: &str) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    // Whether the last block takes the next line: a blank line ends both
    // a paragraph and a list.
    let mut open = false;
    let mut at = 0;
    for raw in body.split('\n') {
        let start = at;
        at += raw.len() + 1;
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let end = start + line.len();
        if line.trim().is_empty() {
            open = false;
            continue;
        }
        let marker = list_marker(line);
        let last = out.last_mut().filter(|_| open);
        match (last, marker) {
            // Another item of the same kind of list.
            (Some(Block::List { ordered, items, .. }), Some((numbered, _, text)))
                if *ordered == numbered =>
            {
                items.push(vec![trimmed(body, start + text, end)]);
            }
            // A line under an item, indented: more of the item.
            (Some(Block::List { items, .. }), None) if line.starts_with([' ', '\t']) => {
                if let Some(item) = items.last_mut() {
                    item.push(trimmed(body, start, end));
                }
            }
            // Any other line goes on a paragraph, unless it starts a list
            // (one numbered from 1, if numbered).
            (Some(Block::Paragraph(lines)), marker)
                if marker.is_none_or(|(numbered, number, _)| numbered && number != 1) =>
            {
                lines.push(trimmed(body, start, end));
            }
            (_, Some((ordered, number, text))) => {
                out.push(Block::List {
                    ordered,
                    start: number,
                    items: vec![vec![trimmed(body, start + text, end)]],
                });
                open = true;
            }
            (_, None) => {
                out.push(Block::Paragraph(vec![trimmed(body, start, end)]));
                open = true;
            }
        }
    }
    out
}

/// The line `start..end` of `body` without the spaces at either end.
fn trimmed(body: &str, start: usize, end: usize) -> Line {
    let line = &body[start..end];
    let from = start + (line.len() - line.trim_start().len());
    let to = start + line.trim_end().len();
    (from, to.max(from))
}

// ---- Inline: emphasis, code, links.

/// What a block's lines hold, in order.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    /// The note's own characters, `start..end`, shown as they are.
    Text(usize, usize),
    /// A character shown as itself (an escaped one), at `start..end`.
    Escaped(usize, usize),
    /// `inline code`, its text `start..end`.
    Code(usize, usize),
    /// A run of `*` or `_` at `start` of `count` (`left` of them still
    /// unmatched), which may open or close emphasis; and the tags it
    /// opens (outermost last) or closes (innermost first) once matched.
    Delimiter {
        start: usize,
        count: usize,
        left: usize,
        mark: u8,
        can_open: bool,
        can_close: bool,
        opens: Vec<&'static str>,
        closes: Vec<&'static str>,
    },
    /// A web link: its address, and its text's pieces.
    Link(String, Vec<Piece>),
    /// A bare web address, `start..end`.
    Address(usize, usize),
    /// Between two lines of a block.
    Break,
}

struct Render<'a> {
    body: &'a str,
    links: &'a [NoteLink],
    words: &'a [String],
}

impl Render<'_> {
    /// `lines` of the note as inline HTML, with `between` between them.
    fn inline(&self, out: &mut String, lines: &[Line], between: &str) {
        let mut pieces = Vec::new();
        for (n, &(start, end)) in lines.iter().enumerate() {
            if n > 0 {
                pieces.push(Piece::Break);
            }
            pieces.extend(scan(self.body, start, end));
        }
        match_emphasis(&mut pieces);
        self.write(out, &pieces, between, false);
    }

    fn write(&self, out: &mut String, pieces: &[Piece], between: &str, in_link: bool) {
        let mut i = 0;
        while i < pieces.len() {
            match &pieces[i] {
                Piece::Text(start, end) => {
                    // The note's own characters, as one run however the
                    // scan cut them, so a reference in them is whole.
                    let mut end = *end;
                    while let Some(Piece::Text(next, next_end)) = pieces.get(i + 1) {
                        if *next != end {
                            break;
                        }
                        end = *next_end;
                        i += 1;
                    }
                    self.text(out, *start, end, !in_link);
                }
                Piece::Escaped(start, end) => self.text(out, *start, *end, false),
                Piece::Code(start, end) => {
                    out.push_str("<code>");
                    out.push_str(&escape_html(&self.body[*start..*end]));
                    out.push_str("</code>");
                }
                Piece::Delimiter {
                    start,
                    left,
                    opens,
                    closes,
                    ..
                } => {
                    // What it closes takes its first characters, what it
                    // opens its last; any left over are text.
                    for tag in closes {
                        out.push_str(&format!("</{tag}>"));
                    }
                    let text = start + tags_length(closes);
                    out.push_str(&escape_html(&self.body[text..text + left]));
                    for tag in opens.iter().rev() {
                        out.push_str(&format!("<{tag}>"));
                    }
                }
                Piece::Link(address, text) => {
                    out.push_str(&web_link_open(address));
                    self.write(out, text, between, true);
                    out.push_str("</a>");
                }
                Piece::Address(start, end) => {
                    let address = &self.body[*start..*end];
                    if in_link {
                        out.push_str(&escape_html(address));
                    } else {
                        out.push_str(&web_link_open(address));
                        out.push_str(&escape_html(address));
                        out.push_str("</a>");
                    }
                }
                Piece::Break => out.push_str(between),
            }
            i += 1;
        }
    }

    /// The note's characters `start..end`, escaped, with the references
    /// in them as links (`linked`) and the search's words marked.
    fn text(&self, out: &mut String, start: usize, end: usize, linked: bool) {
        let mut at = start;
        if linked {
            for link in self
                .links
                .iter()
                .filter(|l| l.start >= start && l.end <= end && l.start < l.end)
            {
                if link.start < at {
                    continue;
                }
                self.marked(out, at, link.start);
                let verses = match (link.verse_start, link.verse_end) {
                    (Some(first), Some(last)) => {
                        format!(" data-start=\"{first}\" data-end=\"{last}\"")
                    }
                    _ => String::new(),
                };
                out.push_str(&format!(
                    "<a class=\"note-ref-link\" href=\"#\" data-book=\"{}\" data-chapter=\"{}\"{verses} title=\"Go to {}\">",
                    link.book_id,
                    link.chapter,
                    escape_html(&link.reference),
                ));
                self.marked(out, link.start, link.end);
                out.push_str("</a>");
                at = link.end;
            }
        }
        self.marked(out, at, end);
    }

    /// The note's characters `start..end`, escaped, with the search's
    /// words marked.
    fn marked(&self, out: &mut String, start: usize, end: usize) {
        let text = &self.body[start..end];
        let mut at = 0;
        for (from, to) in text_marks(text, self.words) {
            out.push_str(&escape_html(&text[at..from]));
            out.push_str("<mark class=\"note-search-mark\">");
            out.push_str(&escape_html(&text[from..to]));
            out.push_str("</mark>");
            at = to;
        }
        out.push_str(&escape_html(&text[at..]));
    }
}

/// How many of a run's characters `tags` take: two for bold, one for
/// italics.
fn tags_length(tags: &[&str]) -> usize {
    tags.iter()
        .map(|tag| if *tag == "strong" { 2 } else { 1 })
        .sum()
}

/// A web link's opening tag: the address it opens in the browser, and a
/// tooltip saying so.
fn web_link_open(address: &str) -> String {
    let address = escape_html(address);
    format!(
        "<a class=\"note-web-link\" href=\"{address}\" title=\"{address} (opens in your browser)\">"
    )
}

/// ASCII punctuation, which a backslash shows as itself.
fn escapable(c: char) -> bool {
    c.is_ascii_punctuation()
}

/// Unicode white space or punctuation, for the flanking rules.
fn space_or_punct(c: Option<char>) -> (bool, bool) {
    match c {
        None => (true, false),
        Some(c) => (
            c.is_whitespace(),
            c.is_ascii_punctuation() || (!c.is_alphanumeric() && !c.is_whitespace()),
        ),
    }
}

/// The pieces of `body[start..end]`, one line.
fn scan(body: &str, start: usize, end: usize) -> Vec<Piece> {
    let line = &body[start..end];
    let mut pieces = Vec::new();
    let mut text_from = 0;
    let mut i = 0;
    let flush = |pieces: &mut Vec<Piece>, from: usize, to: usize| {
        if to > from {
            pieces.push(Piece::Text(start + from, start + to));
        }
    };
    while i < line.len() {
        let rest = &line[i..];
        let c = rest.chars().next().unwrap_or(' ');
        match c {
            '\\' => {
                if let Some(next) = rest[1..].chars().next().filter(|&n| escapable(n)) {
                    flush(&mut pieces, text_from, i);
                    pieces.push(Piece::Escaped(
                        start + i + 1,
                        start + i + 1 + next.len_utf8(),
                    ));
                    i += 1 + next.len_utf8();
                    text_from = i;
                    continue;
                }
            }
            '`' => {
                let ticks = rest.bytes().take_while(|&b| b == b'`').count();
                if let Some(close) = closing_ticks(&rest[ticks..], ticks) {
                    flush(&mut pieces, text_from, i);
                    let (mut from, mut to) = (i + ticks, i + ticks + close);
                    let inner = &line[from..to];
                    if inner.len() >= 2
                        && inner.starts_with(' ')
                        && inner.ends_with(' ')
                        && !inner.trim().is_empty()
                    {
                        from += 1;
                        to -= 1;
                    }
                    pieces.push(Piece::Code(start + from, start + to));
                    i += ticks + close + ticks;
                    text_from = i;
                    continue;
                }
                // No closing run: the backticks are text.
                i += ticks;
                continue;
            }
            '*' | '_' => {
                let count = rest.bytes().take_while(|&b| b == c as u8).count();
                let before = line[..i].chars().next_back();
                let after = rest[count..].chars().next();
                let (before_space, before_punct) = space_or_punct(before);
                let (after_space, after_punct) = space_or_punct(after);
                let left = !after_space && (!after_punct || before_space || before_punct);
                let right = !before_space && (!before_punct || after_space || after_punct);
                let (can_open, can_close) = if c == '*' {
                    (left, right)
                } else {
                    (
                        left && (!right || before_punct),
                        right && (!left || after_punct),
                    )
                };
                flush(&mut pieces, text_from, i);
                pieces.push(Piece::Delimiter {
                    start: start + i,
                    count,
                    left: count,
                    mark: c as u8,
                    can_open,
                    can_close,
                    opens: Vec::new(),
                    closes: Vec::new(),
                });
                i += count;
                text_from = i;
                continue;
            }
            '[' => {
                if let Some((text_end, address, length)) = web_link(rest) {
                    flush(&mut pieces, text_from, i);
                    let mut inner = scan(body, start + i + 1, start + i + 1 + text_end);
                    match_emphasis(&mut inner);
                    // No link inside a link: an address in its text is
                    // text.
                    pieces.push(Piece::Link(address, inner));
                    i += length;
                    text_from = i;
                    continue;
                }
            }
            '!' if rest.starts_with("![") => {
                // An image is its text as typed: "![" and all.
                i += 2;
                continue;
            }
            'h' | 'H' => {
                let boundary = line[..i]
                    .chars()
                    .next_back()
                    .is_none_or(|b| !b.is_alphanumeric());
                if boundary && let Some(length) = bare_address(rest) {
                    flush(&mut pieces, text_from, i);
                    pieces.push(Piece::Address(start + i, start + i + length));
                    i += length;
                    text_from = i;
                    continue;
                }
            }
            _ => {}
        }
        i += c.len_utf8();
    }
    flush(&mut pieces, text_from, line.len());
    pieces
}

/// Where a code span's closing run of exactly `ticks` backticks starts in
/// `rest`, if there's one.
fn closing_ticks(rest: &str, ticks: usize) -> Option<usize> {
    let mut i = 0;
    while i < rest.len() {
        if rest.as_bytes()[i] == b'`' {
            let run = rest[i..].bytes().take_while(|&b| b == b'`').count();
            if run == ticks {
                return Some(i);
            }
            i += run;
        } else {
            i += 1;
        }
    }
    None
}

/// A web address: `http://` or `https://` and a host, as far as the next
/// space or `<` or `>`, less any punctuation at its end (a sentence's
/// period, a closing bracket it didn't open).
fn bare_address(rest: &str) -> Option<usize> {
    let scheme = ["https://", "http://"]
        .into_iter()
        .find(|s| rest.len() > s.len() && rest[..s.len()].eq_ignore_ascii_case(s))?;
    let mut end = rest
        .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '`'))
        .unwrap_or(rest.len());
    loop {
        let address = &rest[..end];
        let last = address.chars().next_back()?;
        let unbalanced = last == ')' && address.matches(')').count() > address.matches('(').count();
        if matches!(
            last,
            '.' | ',' | ':' | ';' | '!' | '?' | '\'' | '*' | '_' | '~'
        ) || unbalanced
        {
            end -= last.len_utf8();
        } else {
            break;
        }
    }
    (end > scheme.len()).then_some(end)
}

/// A link `[text](address)` at the start of `rest` whose address is a web
/// address: where its text ends (after the `[`), the address, and the
/// link's whole length.
fn web_link(rest: &str) -> Option<(usize, String, usize)> {
    let mut depth = 0usize;
    let mut text_end = None;
    let mut chars = rest.char_indices().skip(1);
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '[' => depth += 1,
            ']' if depth == 0 => {
                text_end = Some(i);
                break;
            }
            ']' => depth -= 1,
            _ => {}
        }
    }
    let text_end = text_end?;
    let after = &rest[text_end + 1..];
    let inside = after.strip_prefix('(')?;
    let close = inside.find(')')?;
    let address = inside[..close].trim();
    if address.contains(char::is_whitespace) || bare_address(address) != Some(address.len()) {
        return None;
    }
    Some((
        text_end - 1,
        address.to_string(),
        text_end + 1 + 1 + close + 1,
    ))
}

/// Matches emphasis runs, CommonMark's way: each closer, from the left,
/// with the nearest opener before it of the same character (not both
/// sides' run counting to a multiple of 3 when either can do both, unless
/// both are); two of each when both have two (bold), else one (italics);
/// the runs between them can no longer match.
fn match_emphasis(pieces: &mut [Piece]) {
    let delimiters: Vec<usize> = pieces
        .iter()
        .enumerate()
        .filter(|(_, p)| run_of(p).is_some())
        .map(|(i, _)| i)
        .collect();
    let mut active: Vec<usize> = Vec::new();
    for &closer in &delimiters {
        while let Some(close) = run_of(&pieces[closer]).filter(|r| r.can_close && r.left > 0) {
            let found = active.iter().rposition(|&opener| {
                run_of(&pieces[opener]).is_some_and(|open| {
                    let odd_match = (close.can_open || open.can_close)
                        && (open.count + close.count) % 3 == 0
                        && !(open.count % 3 == 0 && close.count % 3 == 0);
                    open.mark == close.mark && open.can_open && open.left > 0 && !odd_match
                })
            });
            let Some(at) = found else {
                break;
            };
            let opener = active[at];
            // Runs between them are text now.
            active.truncate(at + 1);
            let opener_left = run_of(&pieces[opener]).map_or(0, |r| r.left);
            let (tag, used) = if opener_left >= 2 && close.left >= 2 {
                ("strong", 2)
            } else {
                ("em", 1)
            };
            if let Piece::Delimiter { left, opens, .. } = &mut pieces[opener] {
                *left -= used;
                opens.push(tag);
            }
            if let Piece::Delimiter { left, closes, .. } = &mut pieces[closer] {
                *left -= used;
                closes.push(tag);
            }
            if opener_left == used {
                active.pop();
            }
        }
        if run_of(&pieces[closer]).is_some_and(|r| r.can_open && r.left > 0) {
            active.push(closer);
        }
    }
}

/// A delimiter run's facts, for matching.
#[derive(Debug, Clone, Copy)]
struct Run {
    mark: u8,
    count: usize,
    left: usize,
    can_open: bool,
    can_close: bool,
}

/// `piece`'s run, if it's a delimiter.
fn run_of(piece: &Piece) -> Option<Run> {
    match piece {
        Piece::Delimiter {
            mark,
            count,
            left,
            can_open,
            can_close,
            ..
        } => Some(Run {
            mark: *mark,
            count: *count,
            left: *left,
            can_open: *can_open,
            can_close: *can_close,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(body: &str) -> String {
        note_html(body, &[], &[], Layout::Blocks)
    }

    fn line(body: &str) -> String {
        note_html(body, &[], &[], Layout::OneLine)
    }

    #[test]
    fn paragraphs_and_line_breaks_are_kept() {
        assert_eq!(html("One line."), "<p>One line.</p>");
        assert_eq!(
            html("First line\nsecond line\n\n\nNew paragraph"),
            "<p>First line<br>second line</p><p>New paragraph</p>"
        );
        assert_eq!(html("  spaced  \r\n  out  "), "<p>spaced<br>out</p>");
        assert_eq!(html(""), "");
        assert_eq!(html(" \n \n"), "");
    }

    #[test]
    fn bold_italics_and_code() {
        assert_eq!(
            html("**bold** and __bold__"),
            "<p><strong>bold</strong> and <strong>bold</strong></p>"
        );
        assert_eq!(html("*it* and _it_"), "<p><em>it</em> and <em>it</em></p>");
        assert_eq!(html("***both***"), "<p><em><strong>both</strong></em></p>");
        assert_eq!(
            html("**bold *in* it**"),
            "<p><strong>bold <em>in</em> it</strong></p>"
        );
        assert_eq!(
            html("`a <b> & *c*`"),
            "<p><code>a &lt;b&gt; &amp; *c*</code></p>"
        );
        assert_eq!(html("`` a ` b ``"), "<p><code>a ` b</code></p>");
        // Across a line break, within a paragraph.
        assert_eq!(html("*one\ntwo*"), "<p><em>one<br>two</em></p>");
    }

    #[test]
    fn what_isn_t_emphasis_reads_as_typed() {
        for (typed, shown) in [
            ("2 * 3 * 4", "<p>2 * 3 * 4</p>"),
            ("snake_case_name", "<p>snake_case_name</p>"),
            ("**not closed", "<p>**not closed</p>"),
            ("a * b*", "<p>a * b*</p>"),
            ("`not closed", "<p>`not closed</p>"),
            ("\\*escaped\\*", "<p>*escaped*</p>"),
            ("C:\\path", "<p>C:\\path</p>"),
            ("**", "<p>**</p>"),
            ("_ _", "<p>_ _</p>"),
        ] {
            assert_eq!(html(typed), shown, "{typed:?}");
        }
        // CommonMark's own: intraword stars emphasize, underscores don't.
        assert_eq!(
            html("un*frigging*believable"),
            "<p>un<em>frigging</em>believable</p>"
        );
    }

    #[test]
    fn bulleted_and_numbered_lists() {
        assert_eq!(
            html("- one\n- two\n* three"),
            "<ul><li>one</li><li>two</li><li>three</li></ul>"
        );
        assert_eq!(html("1. one\n2. two"), "<ol><li>one</li><li>two</li></ol>");
        assert_eq!(
            html("3) three\n4) four"),
            "<ol start=\"3\"><li>three</li><li>four</li></ol>"
        );
        // After a paragraph's line: a bulleted list, or a numbered one from
        // 1; any other number goes on the paragraph.
        assert_eq!(
            html("Things:\n- a\n- b"),
            "<p>Things:</p><ul><li>a</li><li>b</li></ul>"
        );
        assert_eq!(html("Steps:\n1. a"), "<p>Steps:</p><ol><li>a</li></ol>");
        assert_eq!(
            html("Born in\n1984. Moved"),
            "<p>Born in<br>1984. Moved</p>"
        );
        // An indented line goes on the item; one that isn't is a paragraph.
        assert_eq!(
            html("- one\n  more of one\n- two\nAfter."),
            "<ul><li>one<br>more of one</li><li>two</li></ul><p>After.</p>"
        );
        // Emphasis in an item, and a blank line between lists.
        assert_eq!(
            html("- **a**\n\n- b"),
            "<ul><li><strong>a</strong></li></ul><ul><li>b</li></ul>"
        );
        // Not a marker: no space after it.
        assert_eq!(
            html("-5 degrees\n+1\n1.5 miles"),
            "<p>-5 degrees<br>+1<br>1.5 miles</p>"
        );
    }

    #[test]
    fn links_open_only_web_addresses() {
        assert_eq!(
            html("[Bible Gateway](https://www.biblegateway.com/?q=a&b=c)"),
            "<p><a class=\"note-web-link\" href=\"https://www.biblegateway.com/?q=a&amp;b=c\" \
             title=\"https://www.biblegateway.com/?q=a&amp;b=c (opens in your browser)\">Bible Gateway</a></p>"
        );
        assert_eq!(
            html("See https://example.com/a_b."),
            "<p>See <a class=\"note-web-link\" href=\"https://example.com/a_b\" \
             title=\"https://example.com/a_b (opens in your browser)\">https://example.com/a_b</a>.</p>"
        );
        assert!(
            html("(http://example.com/x)")
                .starts_with("<p>(<a class=\"note-web-link\" href=\"http://example.com/x\"")
        );
        assert!(
            html("http://example.com/wiki/A_(b)")
                .contains("href=\"http://example.com/wiki/A_(b)\"")
        );
        // Anything but http and https is its text as typed.
        for typed in [
            "[click](javascript:alert(1))",
            "[file](file:///etc/passwd)",
            "[mail](mailto:a@b.c)",
            "[data](data:text/html,x)",
            "[rel](/somewhere)",
            "javascript:alert(1)",
            "ftp://example.com",
            "xhttps://example.com",
            "https://",
        ] {
            let shown = html(typed);
            assert!(!shown.contains("<a "), "{typed:?}: {shown}");
            assert!(!shown.contains("href"), "{typed:?}: {shown}");
        }
        // Emphasis in a link's text, never a link in a link.
        assert_eq!(
            html("[**bold** https://b.example](https://a.example)"),
            "<p><a class=\"note-web-link\" href=\"https://a.example\" title=\"https://a.example (opens in your browser)\">\
             <strong>bold</strong> https://b.example</a></p>"
        );
    }

    #[test]
    fn html_is_shown_as_typed_never_as_markup() {
        assert_eq!(
            html("<img src=x onerror=\"alert(1)\"><b>bold?</b>\n&amp; <script>alert(1)</script>"),
            "<p>&lt;img src=x onerror=&quot;alert(1)&quot;&gt;&lt;b&gt;bold?&lt;/b&gt;<br>&amp;amp; &lt;script&gt;alert(1)&lt;/script&gt;</p>"
        );
        assert_eq!(html("a ' quote"), "<p>a &#39; quote</p>");
        // Out of the subset: as typed.
        for (typed, shown) in [
            ("# Not a heading", "<p># Not a heading</p>"),
            ("> not a quote", "<p>&gt; not a quote</p>"),
            ("    not code", "<p>not code</p>"),
            ("```\nfence\n```", "<p>```<br>fence<br>```</p>"),
            ("---", "<p>---</p>"),
            ("| a | b |", "<p>| a | b |</p>"),
            (
                "![alt](https://example.com/i.png)",
                "<p>![alt](<a class=\"note-web-link\" href=\"https://example.com/i.png\" title=\"https://example.com/i.png (opens in your browser)\">https://example.com/i.png</a>)</p>",
            ),
        ] {
            assert_eq!(html(typed), shown, "{typed:?}");
        }
    }

    fn link(
        body: &str,
        typed: &str,
        book_id: i64,
        chapter: i64,
        verses: Option<(i64, i64)>,
        reference: &str,
    ) -> NoteLink {
        let start = body.find(typed).unwrap();
        NoteLink {
            start,
            end: start + typed.len(),
            book_id,
            chapter,
            verse_start: verses.map(|v| v.0),
            verse_end: verses.map(|v| v.1),
            reference: reference.to_string(),
        }
    }

    #[test]
    fn references_are_links_to_their_passages() {
        let body = "See Rom 5:8 and **Ps 23**.";
        let links = [
            link(body, "Rom 5:8", 45, 5, Some((8, 8)), "Romans 5:8"),
            link(body, "Ps 23", 19, 23, None, "Psalms 23"),
        ];
        assert_eq!(
            note_html(body, &links, &[], Layout::Blocks),
            "<p>See <a class=\"note-ref-link\" href=\"#\" data-book=\"45\" data-chapter=\"5\" data-start=\"8\" data-end=\"8\" title=\"Go to Romans 5:8\">Rom 5:8</a> and \
             <strong><a class=\"note-ref-link\" href=\"#\" data-book=\"19\" data-chapter=\"23\" title=\"Go to Psalms 23\">Ps 23</a></strong>.</p>"
        );
        // In code, or in a web link's text, a reference is text.
        let body = "`Rom 5:8` [Rom 5:8](https://example.com)";
        let links = [
            NoteLink {
                start: 1,
                end: 8,
                ..link(body, "Rom 5:8", 45, 5, Some((8, 8)), "Romans 5:8")
            },
            NoteLink {
                start: 11,
                end: 18,
                ..link(body, "Rom 5:8", 45, 5, Some((8, 8)), "Romans 5:8")
            },
        ];
        let shown = note_html(body, &links, &[], Layout::Blocks);
        assert!(!shown.contains("note-ref-link"), "{shown}");
        // A span that doesn't fall on the note's own text is left alone.
        let body = "Rom 5\\:8";
        let wrong = NoteLink {
            start: 0,
            end: body.len(),
            ..link(body, "Rom", 45, 5, None, "Romans 5")
        };
        assert!(!note_html(body, &[wrong], &[], Layout::Blocks).contains("<a"));
    }

    #[test]
    fn a_search_s_words_are_marked_in_the_text_and_its_links() {
        let body = "God so **loved** the world: John 3:16.";
        let links = [link(body, "John 3:16", 43, 3, Some((16, 16)), "John 3:16")];
        let words = crate::library_text::query_words("lov john");
        assert_eq!(
            note_html(body, &links, &words, Layout::OneLine),
            "God so <strong><mark class=\"note-search-mark\">lov</mark>ed</strong> the world: \
             <a class=\"note-ref-link\" href=\"#\" data-book=\"43\" data-chapter=\"3\" data-start=\"16\" data-end=\"16\" title=\"Go to John 3:16\">\
             <mark class=\"note-search-mark\">John</mark> 3:16</a>."
        );
    }

    #[test]
    fn one_line_runs_the_note_on_for_a_preview() {
        assert_eq!(line("First\nsecond\n\nThird"), "First second Third");
        assert_eq!(
            line("Two:\n- a\n- **b**"),
            "Two: \u{2022} a \u{2022} <strong>b</strong>"
        );
        assert_eq!(line("1. a\n2. b"), "1. a 2. b");
        assert_eq!(line("<b>x</b>\n&"), "&lt;b&gt;x&lt;/b&gt; &amp;");
    }
}
