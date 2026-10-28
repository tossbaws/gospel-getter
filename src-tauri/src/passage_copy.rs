//! Copying a passage: the selected verses, or a whole chapter, as text to
//! paste. One formatter serves Copy (and Ctrl+C), Copy as…, Copy chapter
//! and the notes export's quoted verses, so none of them can disagree
//! with another.
//!
//! This is presentation only. Each verse's words are the reader's own
//! plain text (`VerseText::text`: the KJV's supplied words as plain words,
//! its margin notes left out; any other translation exactly as stored,
//! the BSB's footnotes being kept apart from its text), or, in Markdown,
//! the notes export's rendering of them (the KJV's supplied words in
//! italics, everything else escaped). No format adds, drops or changes a
//! word of a verse; they differ only in the numbers, the reference and
//! the punctuation around them.
//!
//! A verse the translation doesn't number isn't in a passage's verses at
//! all, so it's skipped without comment, and the reference still names
//! the passage asked for ("Matthew 17:20–22" with no verse 21), as Copy
//! always has.

use serde::{Deserialize, Serialize};

use crate::db::Verse;
use crate::domain::kjv_markup::{self, PartKind, VerseText};
use crate::markdown::{escape_block, escape_inline};

/// What Copy as… offers, in the menu's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CopyFormat {
    /// "Text with reference": the verses, then " — John 3:16–18 (WEB)".
    /// What Copy and Ctrl+C copy.
    Reference,
    /// "With verse numbers": a verse to a line, each after its number,
    /// then the reference on a line of its own.
    Numbered,
    /// "Markdown quote": the notes export's quote of the verses, then the
    /// reference, in the quote.
    Markdown,
    /// "Plain text": the verses only.
    Plain,
}

impl CopyFormat {
    pub const ALL: [CopyFormat; 4] = [
        CopyFormat::Reference,
        CopyFormat::Numbered,
        CopyFormat::Markdown,
        CopyFormat::Plain,
    ];
}

/// The verses of one passage in one translation, to copy.
#[derive(Debug, Clone, Copy)]
pub struct Passage<'a> {
    /// The translation's code ("kjv", "bsb", "web"), which decides how its
    /// verses read (see `VerseText`).
    pub translation_code: &'a str,
    /// "John 3:16–18 (WEB)", or "John 3 (WEB)" for a whole chapter.
    pub citation: &'a str,
    /// The verses the translation numbers, in order, as stored.
    pub verses: &'a [Verse],
}

impl Passage<'_> {
    /// The passage in `format`. Lines end in LF, with none after the last.
    pub fn render(&self, format: CopyFormat) -> String {
        match format {
            CopyFormat::Reference => format!("{} \u{2014} {}", self.plain(), self.citation),
            CopyFormat::Numbered => {
                let mut out: Vec<String> = self
                    .verses
                    .iter()
                    .map(|v| format!("{} {}", v.verse, plain_text(self.translation_code, v)))
                    .collect();
                out.push(format!("\u{2014} {}", self.citation));
                out.join("\n")
            }
            CopyFormat::Markdown => {
                let verses: Vec<&Verse> = self.verses.iter().collect();
                format!(
                    "{}\n>\n> \u{2014} {}",
                    markdown_quote(self.translation_code, &verses),
                    escape_inline(self.citation)
                )
            }
            CopyFormat::Plain => self.plain(),
        }
    }

    /// The verses' plain text, joined by single spaces.
    fn plain(&self) -> String {
        self.verses
            .iter()
            .map(|v| plain_text(self.translation_code, v))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// A verse's plain text, as the reader shows and copies it.
fn plain_text(translation_code: &str, verse: &Verse) -> String {
    VerseText::new(translation_code, &verse.text).text
}

/// Verses as a Markdown block quote, as the notes export quotes a note's
/// verses: each verse's number in bold before it ("**16** For God…"), the
/// verses run on with single spaces, and every line of that started with
/// "> ". No newline after the last line.
pub fn markdown_quote(translation_code: &str, verses: &[&Verse]) -> String {
    let text = verses
        .iter()
        .map(|v| {
            format!(
                "**{}** {}",
                v.verse,
                verse_markdown(translation_code, &v.text)
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    text.split('\n')
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// One verse's stored text as Markdown. The KJV's `{…}` markup becomes
/// italics for supplied words, and its margin notes are left out (see
/// `domain::kjv_markup`); any other translation's text, and a KJV verse
/// whose markup isn't well formed, is quoted exactly, escaped.
pub(crate) fn verse_markdown(translation_code: &str, text: &str) -> String {
    (translation_code == kjv_markup::KJV)
        .then(|| kjv_markdown(text))
        .flatten()
        .unwrap_or_else(|| escape_block(text))
}

/// A KJV verse as escaped Markdown, with supplied words in italics and
/// margin notes (and the space before each) left out, or `None` when it
/// should be quoted exactly as stored instead (see `kjv_markup::marked`).
pub(crate) fn kjv_markdown(text: &str) -> Option<String> {
    let verse = kjv_markup::marked(text)?;
    Some(
        verse
            .parts
            .iter()
            .map(|part| match part.kind {
                PartKind::Text => escape_inline(&part.text),
                // Escaped first, so the words can't form other Markdown;
                // the asterisks are the only markup added.
                PartKind::Supplied => format!("*{}*", escape_inline(&part.text)),
                PartKind::Note => String::new(),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::usfm_note::StoredFootnote;

    fn verse(number: i64, text: &str) -> Verse {
        Verse {
            verse: number,
            text: text.to_string(),
            footnotes: Vec::new(),
        }
    }

    fn all(code: &str, citation: &str, verses: &[Verse]) -> [String; 4] {
        let passage = Passage {
            translation_code: code,
            citation,
            verses,
        };
        CopyFormat::ALL.map(|f| passage.render(f))
    }

    const JOHN_16: &str = "For God so loved the world, that he gave his one and only Son, \
                           that whoever believes in him should not perish, but have eternal life.";
    const JOHN_17: &str = "For God didn\u{2019}t send his Son into the world to judge the world, \
                           but that the world should be saved through him.";

    #[test]
    fn a_single_verse_in_each_format() {
        let [reference, numbered, markdown, plain] =
            all("web", "John 3:16 (WEB)", &[verse(16, JOHN_16)]);
        assert_eq!(reference, format!("{JOHN_16} \u{2014} John 3:16 (WEB)"));
        assert_eq!(numbered, format!("16 {JOHN_16}\n\u{2014} John 3:16 (WEB)"));
        assert_eq!(
            markdown,
            format!("> **16** {JOHN_16}\n>\n> \u{2014} John 3:16 (WEB)")
        );
        assert_eq!(plain, JOHN_16);
    }

    #[test]
    fn a_range_in_each_format() {
        let verses = [verse(16, JOHN_16), verse(17, JOHN_17)];
        let [reference, numbered, markdown, plain] =
            all("web", "John 3:16\u{2013}17 (WEB)", &verses);
        assert_eq!(
            reference,
            format!("{JOHN_16} {JOHN_17} \u{2014} John 3:16\u{2013}17 (WEB)")
        );
        assert_eq!(
            numbered,
            format!("16 {JOHN_16}\n17 {JOHN_17}\n\u{2014} John 3:16\u{2013}17 (WEB)")
        );
        assert_eq!(
            markdown,
            format!("> **16** {JOHN_16} **17** {JOHN_17}\n>\n> \u{2014} John 3:16\u{2013}17 (WEB)")
        );
        assert_eq!(plain, format!("{JOHN_16} {JOHN_17}"));
    }

    /// Supplied words are plain words (italics in Markdown); margin notes,
    /// and the space before each, are never copied; the Psalm's title is
    /// the verse's own words, brackets and all.
    #[test]
    fn a_kjv_verse_with_supplied_words_and_margin_notes() {
        let verses = [
            verse(
                1,
                "[A Psalm of David.] The LORD {is} my shepherd; I shall not want.",
            ),
            verse(
                2,
                "He maketh me to lie down in green pastures: he leadeth me beside the still \
                 waters. {green...: Heb. pastures of tender grass} {still...: Heb. waters of \
                 quietness}",
            ),
        ];
        let [reference, numbered, markdown, plain] =
            all("kjv", "Psalms 23:1\u{2013}2 (KJV)", &verses);
        let one = "[A Psalm of David.] The LORD is my shepherd; I shall not want.";
        let two = "He maketh me to lie down in green pastures: he leadeth me beside the still \
                   waters.";
        assert_eq!(
            reference,
            format!("{one} {two} \u{2014} Psalms 23:1\u{2013}2 (KJV)")
        );
        assert_eq!(
            numbered,
            format!("1 {one}\n2 {two}\n\u{2014} Psalms 23:1\u{2013}2 (KJV)")
        );
        assert_eq!(
            markdown,
            format!(
                "> **1** \\[A Psalm of David.\\] The LORD *is* my shepherd; I shall not want. \
                 **2** {two}\n>\n> \u{2014} Psalms 23:1\u{2013}2 (KJV)"
            )
        );
        assert_eq!(plain, format!("{one} {two}"));
        for text in [&reference, &numbered, &markdown, &plain] {
            assert!(!text.contains("Heb.") && !text.contains('{'), "{text}");
        }
    }

    /// The BSB's footnotes are kept apart from its text, so no format has
    /// a marker or a note's words: just the stored verse.
    #[test]
    fn a_bsb_verse_with_footnotes() {
        let stored = "In the beginning God created the heavens and the earth.";
        let verses = [Verse {
            verse: 1,
            text: stored.to_string(),
            footnotes: vec![StoredFootnote {
                position: 14,
                usfm: "\\f + \\fr 1:1 \\ft Or *Elohim*\\f*".to_string(),
            }],
        }];
        let [reference, numbered, markdown, plain] = all("bsb", "Genesis 1:1 (BSB)", &verses);
        assert_eq!(reference, format!("{stored} \u{2014} Genesis 1:1 (BSB)"));
        assert_eq!(numbered, format!("1 {stored}\n\u{2014} Genesis 1:1 (BSB)"));
        assert_eq!(
            markdown,
            format!("> **1** {stored}\n>\n> \u{2014} Genesis 1:1 (BSB)")
        );
        assert_eq!(plain, stored);
    }

    /// A verse the translation doesn't number is skipped silently; the
    /// reference still names the passage asked for.
    #[test]
    fn a_range_across_an_unnumbered_verse() {
        let verses = [verse(20, "Twenty."), verse(22, "Twenty-two.")];
        let [reference, numbered, markdown, plain] =
            all("bsb", "Matthew 17:20\u{2013}22 (BSB)", &verses);
        assert_eq!(
            reference,
            "Twenty. Twenty-two. \u{2014} Matthew 17:20\u{2013}22 (BSB)"
        );
        assert_eq!(
            numbered,
            "20 Twenty.\n22 Twenty-two.\n\u{2014} Matthew 17:20\u{2013}22 (BSB)"
        );
        assert_eq!(
            markdown,
            "> **20** Twenty. **22** Twenty-two.\n>\n> \u{2014} Matthew 17:20\u{2013}22 (BSB)"
        );
        assert_eq!(plain, "Twenty. Twenty-two.");
    }

    /// Markdown escapes what it must, and only there: plain formats copy
    /// the characters as they are.
    #[test]
    fn only_markdown_escapes() {
        let verses = [verse(1, "a *b* [c] <d>")];
        let [reference, _, markdown, plain] = all("web", "Book 1:1 (WEB)", &verses);
        assert_eq!(plain, "a *b* [c] <d>");
        assert_eq!(reference, "a *b* [c] <d> \u{2014} Book 1:1 (WEB)");
        assert_eq!(
            markdown,
            "> **1** a \\*b\\* \\[c\\] \\<d\\>\n>\n> \u{2014} Book 1:1 (WEB)"
        );
    }

    /// Every bundled verse, in every format, is its plain text exactly:
    /// one numbered line holds "N " and the verse, nothing else.
    #[test]
    fn every_bundled_verse_is_copied_as_the_reader_shows_it() {
        let check = |code: &str, number: i64, text: &str| {
            let shown = VerseText::new(code, text).text;
            let verses = [verse(number, text)];
            let passage = Passage {
                translation_code: code,
                citation: "X 1:1 (X)",
                verses: &verses,
            };
            assert_eq!(passage.render(CopyFormat::Plain), shown);
            assert_eq!(
                passage.render(CopyFormat::Numbered),
                format!("{number} {shown}\n\u{2014} X 1:1 (X)")
            );
        };
        let mut count = 0;
        for code in ["kjv", "web"] {
            for book in crate::db::test_support::bundled(code) {
                for chapter in &book.chapters {
                    for (i, text) in chapter.iter().enumerate() {
                        check(code, i as i64 + 1, text);
                        count += 1;
                    }
                }
            }
        }
        for book in crate::db::test_support::bundled_bsb() {
            for chapter in &book.chapters {
                for (i, text) in chapter.iter().enumerate() {
                    if let Some(text) = text {
                        check("bsb", i as i64 + 1, text);
                        count += 1;
                    }
                }
            }
        }
        assert!(count > 90_000, "{count}");
    }
}
