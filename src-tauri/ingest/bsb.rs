//! Derives the bundled Berean Standard Bible (`data/bsb.json`) from
//! Berean's own files in `sources/bsb/` (see its README): the verse text
//! from their plain text (`bsb.txt`), the footnotes from their USFM
//! (`bsb_usfm.zip`).
//!
//! Not part of the app: `examples/bsb_ingest.rs` runs it to write the
//! bundled file, and `tests/bsb_source.rs` runs it again to check that
//! file, byte for byte.
//!
//! What it does, and all it does:
//!
//! - **The verse text** is each line of `bsb.txt` after its tab, exactly.
//!   Reading the file only decodes it: the UTF-8 byte-order mark is dropped
//!   and lines are split at CRLF. A verse the text lists with nothing after
//!   its tab is one the BSB doesn't number; it's `null`.
//! - **Each footnote** is the USFM's own characters, from `\f` to `\f*`,
//!   copied whole. Where it goes is where the USFM puts it: the USFM's
//!   wording of the verse is lined up with the text's by a shortest edit
//!   script, and the note goes right after the last character before it
//!   that both share. The USFM's wording is only used for that, never
//!   stored.
//! - **Where the two wordings differ** is listed in
//!   `sources/bsb/artifacts.tsv`, so a change in either file shows up.

use serde::Serialize;
use std::collections::BTreeSet;

/// The USFM books, in canonical order.
pub const USFM_BOOKS: [&str; 66] = [
    "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA", "1KI", "2KI", "1CH",
    "2CH", "EZR", "NEH", "EST", "JOB", "PSA", "PRO", "ECC", "SNG", "ISA", "JER", "LAM", "EZK",
    "DAN", "HOS", "JOL", "AMO", "OBA", "JON", "MIC", "NAM", "HAB", "ZEP", "HAG", "ZEC", "MAL",
    "MAT", "MRK", "LUK", "JHN", "ACT", "ROM", "1CO", "2CO", "GAL", "EPH", "PHP", "COL", "1TH",
    "2TH", "1TI", "2TI", "TIT", "PHM", "HEB", "JAS", "1PE", "2PE", "1JN", "2JN", "3JN", "JUD",
    "REV",
];

/// The third line of `bsb.txt`, its column heading. The two before it are
/// Berean's attribution notice.
const TXT_COLUMNS: &str = "Verse\tBerean Standard Bible";

/// One book of the bundled BSB: the shape of `kjv.json`'s books, with
/// `null` for a verse the BSB doesn't number, and its footnotes.
#[derive(Debug, Serialize)]
pub struct Book {
    pub name: String,
    pub testament: &'static str,
    pub chapters: Vec<Vec<Option<String>>>,
    pub footnotes: Vec<Footnote>,
}

/// A footnote: the USFM's `\f … \f*`, exactly, after the `at`th character
/// (Unicode scalar value) of its verse's text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Footnote {
    pub chapter: u32,
    pub verse: u32,
    pub at: usize,
    pub usfm: String,
}

/// What `derive` makes: the bundled file's contents and the artifacts
/// list's.
pub struct Derived {
    pub books: Vec<Book>,
    pub json: String,
    pub artifacts: String,
}

/// One line of `bsb.txt`: its book (an index from 0, in order of
/// appearance), chapter, verse and text.
type TxtVerse = (usize, u32, u32, String);

/// `bsb.txt`'s book names, in order of appearance, and its verses, in
/// order.
fn read_txt(bytes: &[u8]) -> Result<(Vec<String>, Vec<TxtVerse>), String> {
    let text = std::str::from_utf8(bytes).map_err(|e| format!("bsb.txt isn't UTF-8: {e}"))?;
    let text = text
        .strip_prefix('\u{feff}')
        .ok_or("bsb.txt doesn't start with its byte-order mark")?;
    let text = text
        .strip_suffix("\r\n")
        .ok_or("bsb.txt doesn't end with CRLF")?;
    let lines: Vec<&str> = text.split("\r\n").collect();
    if let Some(line) = lines.iter().find(|l| l.contains(['\r', '\n'])) {
        return Err(format!(
            "bsb.txt has a line break that isn't CRLF: {line:?}"
        ));
    }
    if lines.get(2) != Some(&TXT_COLUMNS) {
        return Err(format!("bsb.txt's third line isn't {TXT_COLUMNS:?}"));
    }
    let mut names: Vec<String> = Vec::new();
    let mut verses = Vec::new();
    for line in &lines[3..] {
        let (reference, text) = line
            .split_once('\t')
            .ok_or_else(|| format!("bsb.txt line without a tab: {line:?}"))?;
        if text.contains('\t') {
            return Err(format!("bsb.txt line with two tabs: {line:?}"));
        }
        let (book, numbers) = reference
            .rsplit_once(' ')
            .ok_or_else(|| format!("bsb.txt reference without a book: {reference:?}"))?;
        let (chapter, verse) = numbers
            .split_once(':')
            .ok_or_else(|| format!("bsb.txt reference without a verse: {reference:?}"))?;
        let chapter: u32 = chapter
            .parse()
            .map_err(|_| format!("bad chapter in {reference:?}"))?;
        let verse: u32 = verse
            .parse()
            .map_err(|_| format!("bad verse in {reference:?}"))?;
        if names.last().map(String::as_str) != Some(book) {
            if names.iter().any(|n| n == book) {
                return Err(format!("bsb.txt's {book} isn't in one piece"));
            }
            names.push(book.to_string());
        }
        verses.push((names.len() - 1, chapter, verse, text.to_string()));
    }
    Ok((names, verses))
}

/// One verse of the USFM: its wording (only to place the notes) and its
/// notes, each with where it stood in that wording, in characters.
#[derive(Debug, Default, Clone)]
pub struct UsfmVerse {
    pub wording: String,
    pub notes: Vec<(usize, String)>,
}

/// Stands for a note in a verse's wording while it's built; never in the
/// USFM (`read_usfm` checks).
const NOTE: char = '\u{e000}';

/// Paragraph markers: a break in the text, read as a space.
const PARAGRAPHS: [&str; 11] = [
    "p", "m", "b", "pmo", "pc", "q1", "q2", "qr", "li1", "li2", "d",
];
/// Headings and the book's own titles: not verse text.
const HEADINGS: [&str; 13] = [
    "id", "usfm", "h", "toc1", "toc2", "toc3", "mt1", "mt2", "ms", "mr", "s1", "s2", "r",
];

/// The verses of one USFM book, keyed by (chapter, verse), and the text
/// found outside every verse (the Psalter's "Psalms 1–41" and the like).
#[allow(clippy::type_complexity)] // a map and a list, named where they're used
pub fn read_usfm(
    usfm: &str,
) -> Result<
    (
        std::collections::BTreeMap<(u32, u32), UsfmVerse>,
        Vec<(u32, String)>,
    ),
    String,
> {
    if usfm.contains(NOTE) || usfm.contains('\r') {
        return Err("the USFM has a character this reader relies on not finding".to_string());
    }
    let mut verses: std::collections::BTreeMap<(u32, u32), String> = Default::default();
    let mut notes: std::collections::BTreeMap<(u32, u32), Vec<String>> = Default::default();
    let mut outside = Vec::new();
    let mut chapter = 0u32;
    let mut verse: Option<u32> = None;
    let mut heading = false;
    for line in usfm.split('\n') {
        let mut rest = line;
        while !rest.is_empty() {
            if let Some(after) = rest.strip_prefix('\\') {
                // A marker's name, then a `*` if it closes something.
                let mut name_len = after
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '+'))
                    .unwrap_or(after.len());
                if after[name_len..].starts_with('*') {
                    name_len += 1;
                }
                let name = &after[..name_len];
                let mut tail = &after[name_len..];
                // An opening marker's delimiting space isn't text.
                if !name.ends_with('*') {
                    tail = tail.strip_prefix(' ').unwrap_or(tail);
                }
                match name {
                    "c" => {
                        let end = tail.find(' ').unwrap_or(tail.len());
                        chapter = tail[..end]
                            .parse()
                            .map_err(|_| format!("bad chapter number in {line:?}"))?;
                        verse = None;
                        heading = false;
                        rest = &tail[end..];
                    }
                    "v" => {
                        let end = tail.find(' ').unwrap_or(tail.len());
                        let n: u32 = tail[..end]
                            .parse()
                            .map_err(|_| format!("bad verse number in {line:?}"))?;
                        if verses.insert((chapter, n), String::new()).is_some() {
                            return Err(format!("{chapter}:{n} twice"));
                        }
                        verse = Some(n);
                        heading = false;
                        rest = tail[end..].strip_prefix(' ').unwrap_or(&tail[end..]);
                    }
                    "f" => {
                        // The note, `\f` to `\f*`, exactly.
                        let end = rest
                            .find("\\f*")
                            .ok_or_else(|| format!("unclosed note in {line:?}"))?;
                        let note = &rest[..end + 3];
                        let key = (
                            chapter,
                            verse.ok_or_else(|| format!("a note outside a verse: {note:?}"))?,
                        );
                        if heading {
                            return Err(format!("a note in a heading: {note:?}"));
                        }
                        verses.get_mut(&key).expect("the verse").push(NOTE);
                        notes.entry(key).or_default().push(note.to_string());
                        rest = &rest[end + 3..];
                    }
                    "wj" | "wj*" => rest = tail,
                    "qa" => {
                        // An acrostic letter ("Aleph"): a heading.
                        heading = true;
                        rest = tail;
                    }
                    _ if HEADINGS.contains(&name) => {
                        heading = true;
                        rest = tail;
                    }
                    _ if PARAGRAPHS.contains(&name) => {
                        heading = false;
                        if let Some(v) = verse {
                            verses.get_mut(&(chapter, v)).expect("the verse").push(' ');
                        }
                        rest = tail;
                    }
                    "ref" | "ref*" if heading => rest = tail,
                    _ => return Err(format!("unexpected marker \\{name} in {line:?}")),
                }
                continue;
            }
            let end = rest.find('\\').unwrap_or(rest.len());
            let text = &rest[..end];
            rest = &rest[end..];
            if heading || chapter == 0 {
                continue;
            }
            match verse {
                Some(v) => verses
                    .get_mut(&(chapter, v))
                    .expect("the verse")
                    .push_str(text),
                None if !text.trim().is_empty() => outside.push((chapter, text.trim().to_string())),
                None => {}
            }
        }
        // A line ends a heading; a verse's line break is a space.
        heading = false;
        if let Some(v) = verse {
            verses.get_mut(&(chapter, v)).expect("the verse").push(' ');
        }
    }
    let mut out = std::collections::BTreeMap::new();
    for (key, raw) in verses {
        // Spaces run together, as USFM reads them; the ends trimmed.
        let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut wording = String::new();
        let mut at = Vec::new();
        for c in collapsed.chars() {
            if c == NOTE {
                at.push(wording.chars().count());
            } else {
                wording.push(c);
            }
        }
        let notes = notes.remove(&key).unwrap_or_default();
        out.insert(
            key,
            UsfmVerse {
                wording,
                notes: at.into_iter().zip(notes).collect(),
            },
        );
    }
    Ok((out, outside))
}

/// The pairs (i, j) of characters `a[i] == b[j]` a shortest edit script
/// from `a` to `b` keeps (Myers' algorithm), in order.
fn common(a: &[char], b: &[char]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let max = (n + m) as usize;
    let offset = max as isize + 1;
    let mut v = vec![0isize; 2 * max + 3];
    let mut trace: Vec<Vec<isize>> = Vec::new();
    'outer: for d in 0..=max as isize {
        trace.push(v.clone());
        let mut k = -d;
        while k <= d {
            let idx = (k + offset) as usize;
            let mut x = if k == -d || (k != d && v[idx - 1] < v[idx + 1]) {
                v[idx + 1]
            } else {
                v[idx - 1] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[idx] = x;
            if x >= n && y >= m {
                break 'outer;
            }
            k += 2;
        }
    }
    // Walk back through the trace.
    let mut pairs = Vec::new();
    let (mut x, mut y) = (n, m);
    for d in (0..trace.len() as isize).rev() {
        let v = &trace[d as usize];
        let k = x - y;
        let idx = (k + offset) as usize;
        let prev_k = if k == -d || (k != d && v[idx - 1] < v[idx + 1]) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = if d == 0 {
            0
        } else {
            v[(prev_k + offset) as usize]
        };
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            x -= 1;
            y -= 1;
            pairs.push((x as usize, y as usize));
        }
        if d > 0 {
            x = prev_x;
            y = prev_y;
        }
    }
    pairs.reverse();
    pairs
}

/// Where a note `at` characters into the USFM's wording `a` goes in the
/// text's wording `b`: right after the last character before it that the
/// two share (the start, if none).
fn place(pairs: &[(usize, usize)], at: usize) -> usize {
    pairs
        .iter()
        .take_while(|&&(i, _)| i < at)
        .last()
        .map_or(0, |&(_, j)| j + 1)
}

/// A verse's differences between the USFM's wording and the text's, as
/// hunks (the USFM's characters, the text's), with a little of what's
/// around them.
fn hunks(
    a: &[char],
    b: &[char],
    pairs: &[(usize, usize)],
) -> Vec<(String, String, String, String)> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut ends: Vec<(usize, usize)> = pairs.to_vec();
    ends.push((a.len(), b.len()));
    for (pi, pj) in ends {
        if pi > i || pj > j {
            let before: String = a[i.saturating_sub(12)..i].iter().collect();
            let after: String = a[pi..(pi + 12).min(a.len())].iter().collect();
            out.push((
                before,
                a[i..pi].iter().collect(),
                b[j..pj].iter().collect(),
                after,
            ));
        }
        i = pi + 1;
        j = pj + 1;
    }
    out
}

/// What kind of difference a hunk is, for the artifacts list.
fn kind(before: &str, usfm: &str, txt: &str, after: &str) -> &'static str {
    let blank = |s: &str| s.chars().all(char::is_whitespace);
    if blank(usfm) && blank(txt) {
        let dash = before.ends_with('\u{2014}') || after.starts_with('\u{2014}');
        if dash { "spaced dash" } else { "spacing" }
    } else if usfm.contains("[\u{2019}\u{2019}]") {
        "stray [’’]"
    } else if usfm.contains(". . .") {
        "ellipsis"
    } else if txt.trim().is_empty() && (usfm.contains('[') || usfm.contains(']')) {
        "brackets"
    } else if usfm.trim() == "vvv" {
        "stray letters"
    } else if usfm.trim().is_empty() && txt.trim().chars().count() > 3 {
        "text outside the verse in the USFM"
    } else {
        "other"
    }
}

/// Derive the bundled BSB from `bsb.txt`'s bytes and the USFM books'
/// texts, in canonical order.
pub fn derive(txt: &[u8], usfm_books: &[String]) -> Result<Derived, String> {
    if usfm_books.len() != 66 {
        return Err(format!("{} USFM books, not 66", usfm_books.len()));
    }
    let (names, txt_verses) = read_txt(txt)?;
    if names.len() != 66 {
        return Err(format!("bsb.txt has {} books, not 66", names.len()));
    }
    let mut books: Vec<Book> = names
        .iter()
        .enumerate()
        .map(|(i, name)| Book {
            name: name.clone(),
            testament: if i < 39 { "OT" } else { "NT" },
            chapters: Vec::new(),
            footnotes: Vec::new(),
        })
        .collect();
    for (book, chapter, verse, text) in &txt_verses {
        let chapters = &mut books[*book].chapters;
        if *chapter as usize == chapters.len() + 1 && *verse == 1 {
            chapters.push(Vec::new());
        }
        if *chapter as usize != chapters.len() {
            return Err(format!(
                "bsb.txt's {} {chapter} is out of order",
                names[*book]
            ));
        }
        let verses = chapters.last_mut().expect("a chapter");
        if *verse as usize != verses.len() + 1 {
            return Err(format!(
                "bsb.txt's {} {chapter}:{verse} is out of order",
                names[*book]
            ));
        }
        verses.push((!text.is_empty()).then(|| text.clone()));
    }

    let mut artifacts = String::from("reference\tkinds\tdifferences (USFM → text)\n");
    for (b, usfm) in usfm_books.iter().enumerate() {
        let (usfm_verses, _outside) =
            read_usfm(usfm).map_err(|e| format!("{}: {e}", USFM_BOOKS[b]))?;
        let book = &mut books[b];
        // The same verses, numbered the same way.
        let numbered: BTreeSet<(u32, u32)> = book
            .chapters
            .iter()
            .enumerate()
            .flat_map(|(c, vs)| {
                vs.iter()
                    .enumerate()
                    .filter(|(_, v)| v.is_some())
                    .map(move |(v, _)| (c as u32 + 1, v as u32 + 1))
            })
            .collect();
        let in_usfm: BTreeSet<(u32, u32)> = usfm_verses.keys().copied().collect();
        if numbered != in_usfm {
            let only_txt: Vec<_> = numbered.difference(&in_usfm).collect();
            let only_usfm: Vec<_> = in_usfm.difference(&numbered).collect();
            return Err(format!(
                "{}: verses only in the text {only_txt:?}, only in the USFM {only_usfm:?}",
                book.name
            ));
        }
        for ((chapter, verse), u) in &usfm_verses {
            let text = book.chapters[*chapter as usize - 1][*verse as usize - 1]
                .as_deref()
                .expect("numbered");
            if u.wording == text && u.notes.is_empty() {
                continue;
            }
            let a: Vec<char> = u.wording.chars().collect();
            let t: Vec<char> = text.chars().collect();
            let pairs = if u.wording == text {
                (0..a.len()).map(|i| (i, i)).collect()
            } else {
                common(&a, &t)
            };
            for (at, note) in &u.notes {
                book.footnotes.push(Footnote {
                    chapter: *chapter,
                    verse: *verse,
                    at: place(&pairs, *at),
                    usfm: note.clone(),
                });
            }
            if u.wording != text {
                let hs = hunks(&a, &t, &pairs);
                let kinds: BTreeSet<&str> =
                    hs.iter().map(|(b, u, t, a)| kind(b, u, t, a)).collect();
                let shown: Vec<String> = hs
                    .iter()
                    .map(|(b, u, t, a)| format!("{b}«{u}»→«{t}»{a}"))
                    .collect();
                artifacts.push_str(&format!(
                    "{} {chapter}:{verse}\t{}\t{}\n",
                    book.name,
                    kinds.into_iter().collect::<Vec<_>>().join(" + "),
                    shown.join(" · ")
                ));
            }
        }
    }
    let json = serde_json::to_string(&books).map_err(|e| e.to_string())?;
    Ok(Derived {
        books,
        json,
        artifacts,
    })
}
