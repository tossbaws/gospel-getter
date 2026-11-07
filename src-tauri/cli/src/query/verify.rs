//! `verify_quote`: whether a quotation is, character for character, in the real
//! text of one translation, so an agent can check its own quoting (B3, B4).
//!
//! The real text is what `get_passage` gives: the clean text the app's Copy
//! gives (F1), a passage's verses joined by one space. Nothing here edits it,
//! and only the quote's own ends are trimmed. Case, punctuation, the style of
//! an apostrophe and every space inside the quote all count.
//!
//! A phrase search only suggests where to look: it folds case and punctuation,
//! so it can't say a quote is there. Only [`verbatim`] and
//! [`JoinedVerses::places_of`], which compare characters, can.

use anyhow::bail;
use clap::Args;
use gospel_getter_core::db::{self, Translation};
use gospel_getter_core::domain::query::SearchQuery;
use gospel_getter_core::passage_reference::passage_reference;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::{Library, MAX_PASSAGE_VERSES, OPEN_THE_APP};

/// The longest quotation checked, in characters.
const MAX_QUOTE_CHARS: usize = 10_000;
/// The most verses one phrase search suggests, and places a quote is said to be.
const MAX_CANDIDATES: u32 = 200;
/// How many of a quote's words make a phrase to search for.
const PHRASE_WORDS: usize = 8;
/// How many of a quote's words a phrase may start at.
const PHRASE_STARTS: usize = 32;
/// How much of the quote and the real text a difference shows, in characters.
const SHOWN_CHARS: usize = 40;

/// Arguments of `verify_quote`.
#[derive(Debug, Args, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerifyQuoteArgs {
    /// The quotation. Compared character for character: only whitespace around the whole of it is trimmed.
    pub text: String,
    /// The one passage it should be in, as the app's Go to reads it: "John 3:16", "John 3:16-18" or "Psalm 23". Default: find it by phrase search.
    #[arg(long)]
    pub reference: Option<String>,
    /// A translation's code, such as kjv, bsb or web. Default: the one the reader last read. No other translation is looked in.
    #[arg(long)]
    pub translation: Option<String>,
}

/// What a quote was found to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuoteStatus {
    /// The whole of the passage, character for character.
    Exact,
    /// A contiguous part of the passage, character for character.
    ExactPart,
    /// Not in the passage: the difference says where it first departs.
    Differs,
    /// Not in the passage given, but character for character in others.
    Elsewhere,
    /// Not in this translation as quoted.
    NotFound,
}

/// Where a quote first departs from a passage.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct QuoteDifference {
    /// Which character of the quote, from 1, counting characters, not bytes.
    pub position: usize,
    /// Which word of the quote it's in, from 1. A space counts to the word
    /// before it.
    pub word: usize,
    /// The quote from that character, up to 40 characters of it.
    pub quoted: String,
    /// The real text from the same point, up to 40 characters of it: empty
    /// where the quote runs on past the end of the text.
    pub actual: String,
}

/// A passage that has a quote, character for character.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuotePlace {
    pub reference: String,
    pub text: String,
}

/// The answer of `verify_quote`.
#[derive(Debug, Serialize)]
pub struct VerifyQuoteOut {
    pub status: QuoteStatus,
    /// The code of the one translation looked in.
    pub translation: String,
    /// The passage checked or found, as `get_passage` names it. None when
    /// `not_found`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// That passage's real text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// All of this in one plain sentence or two, for a person.
    pub message: String,
    /// Where the quote departs from `text`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difference: Option<QuoteDifference>,
    /// Where it is instead (`elsewhere`), or every place it is when no
    /// reference was given and it's in more than one.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub found_at: Vec<QuotePlace>,
}

/// What a search for a quote came to.
#[derive(Debug, Default)]
struct Search {
    /// Where it is, character for character, in Bible order.
    places: Vec<QuotePlace>,
    /// The first verse with the quote's first words, if any: book, chapter
    /// and verse. It may only have those words in another case or punctuation.
    closest: Option<(i64, i64, i64)>,
    /// The quote's words are in more verses than were read, in a way that could
    /// have hidden it: see [`may_have_missed`].
    cut_short: bool,
}

impl Library {
    pub(super) async fn verify_quote(
        &self,
        args: VerifyQuoteArgs,
    ) -> anyhow::Result<VerifyQuoteOut> {
        let quote = args.text.trim();
        check_length(quote)?;
        let translation = self.translation(args.translation.as_deref()).await?;
        match args.reference.as_deref() {
            Some(reference) => self.check_at(translation, quote, reference).await,
            None => self.look_for(translation, quote).await,
        }
    }

    /// Whether the passage `reference` names has the quote, and if it hasn't,
    /// where the quote departs from it and where else the quote is. A passage
    /// that has it needs no search, so no search index.
    async fn check_at(
        &self,
        translation: &Translation,
        quote: &str,
        reference: &str,
    ) -> anyhow::Result<VerifyQuoteOut> {
        let passage = self.passage_in(translation, reference).await?;
        let verses: Vec<&str> = passage.verses.iter().map(|v| v.text.as_str()).collect();
        let checked = QuotePlace {
            reference: passage.reference,
            text: verses.join(" "),
        };
        if let Some(status) = verbatim(quote, &checked.text) {
            return Ok(held(status, translation, checked, Vec::new(), false));
        }
        let search = self.search_for(translation, quote).await?;
        let difference = first_difference(quote, &checked.text);
        let label = label_of(translation);
        let reference = &checked.reference;
        let (status, message) = match search.places.first() {
            Some(first) => (
                QuoteStatus::Elsewhere,
                format!(
                    "This isn't in {reference} in the {label} as quoted, but it is in {}.{}",
                    first.reference,
                    more_places(&search.places, search.cut_short),
                ),
            ),
            None => (
                QuoteStatus::Differs,
                format!(
                    "This differs from {reference} in the {label} at word {}, character {}.{}",
                    difference.word,
                    difference.position,
                    not_all_checked(search.cut_short),
                ),
            ),
        };
        Ok(VerifyQuoteOut {
            status,
            translation: translation.code.clone(),
            reference: Some(checked.reference),
            text: Some(checked.text),
            message,
            difference: Some(difference),
            found_at: search.places,
        })
    }

    /// Where the quote is, given no reference: a place that has it, the whole
    /// of one before a part of one, else the first verse with its words if
    /// there is one, said to be no match, else nowhere.
    async fn look_for(
        &self,
        translation: &Translation,
        quote: &str,
    ) -> anyhow::Result<VerifyQuoteOut> {
        if quote_words(quote).is_empty() {
            bail!("There are no words in that to look for. Give a reference to check it against.");
        }
        let search = self.search_for(translation, quote).await?;
        let best = search.places.iter().find(|place| place.text == quote);
        if let Some(place) = best.or(search.places.first())
            && let Some(status) = verbatim(quote, &place.text)
        {
            let all = if search.places.len() > 1 {
                search.places.clone()
            } else {
                Vec::new()
            };
            return Ok(held(
                status,
                translation,
                place.clone(),
                all,
                search.cut_short,
            ));
        }
        let label = label_of(translation);
        if let Some((book_id, chapter, verse)) = search.closest {
            let joined = self.joined_chapter(translation, book_id, chapter).await?;
            let name = &self.book(book_id)?.name;
            if let Some((verses, text)) = joined.window_from(verse, quote.chars().count()) {
                let reference = passage_reference(name, chapter, Some(verses));
                let difference = first_difference(quote, text);
                let closest = if verses.0 == verses.1 {
                    format!("The closest verse is {reference}")
                } else {
                    format!("The closest verses are {reference}")
                };
                return Ok(VerifyQuoteOut {
                    status: QuoteStatus::Differs,
                    translation: translation.code.clone(),
                    message: format!(
                        "This isn't in the {label} as quoted. {closest}, not a match: it differs \
                         at word {}, character {}.{}",
                        difference.word,
                        difference.position,
                        not_all_checked(search.cut_short),
                    ),
                    reference: Some(reference),
                    text: Some(text.to_string()),
                    difference: Some(difference),
                    found_at: Vec::new(),
                });
            }
        }
        let message = if search.cut_short {
            format!(
                "This wasn't found in the {label} as quoted, but its words are too common to \
                 check every verse. Give a reference to check it there."
            )
        } else {
            format!("This text isn't in the {label} as quoted.")
        };
        Ok(VerifyQuoteOut {
            status: QuoteStatus::NotFound,
            translation: translation.code.clone(),
            reference: None,
            text: None,
            message,
            difference: None,
            found_at: Vec::new(),
        })
    }

    /// Where the quote is in this translation, by phrase search, then reading
    /// each chapter a search suggests. Stops at the first phrase that leads to
    /// the quote, so `places` are in Bible order.
    async fn search_for(&self, translation: &Translation, quote: &str) -> anyhow::Result<Search> {
        if !db::search::search_index_is_current(&self.pool, translation.id).await? {
            bail!("The search index isn't built yet. {OPEN_THE_APP}");
        }
        let words = quote_words(quote);
        let limit = i64::from(MAX_CANDIDATES);
        let mut search = Search::default();
        let mut chapters_read = HashSet::new();
        let mut too_common = HashSet::new();
        for (tried, phrase) in phrases_of(&words).enumerate() {
            let query = SearchQuery {
                words: Vec::new(),
                phrases: vec![phrase.to_vec()],
            };
            let (total, hits) =
                db::search::search_verses(&self.pool, translation.id, &query, limit, 0).await?;
            if tried == 0 {
                search.closest = hits
                    .first()
                    .map(|hit| (hit.book_id, hit.chapter, hit.verse));
            }
            if total > limit {
                too_common.insert(phrase);
            }
            for hit in hits {
                if !chapters_read.insert((hit.book_id, hit.chapter)) {
                    continue;
                }
                let joined = self
                    .joined_chapter(translation, hit.book_id, hit.chapter)
                    .await?;
                let name = &self.book(hit.book_id)?.name;
                search
                    .places
                    .extend(
                        joined
                            .places_of(quote)
                            .into_iter()
                            .map(|(verses, text)| QuotePlace {
                                reference: passage_reference(name, hit.chapter, Some(verses)),
                                text: text.to_string(),
                            }),
                    );
            }
            if !search.places.is_empty() {
                break;
            }
        }
        search.places.truncate(MAX_CANDIDATES as usize);
        search.cut_short = may_have_missed(&words, &too_common);
        Ok(search)
    }

    /// A chapter's clean text, as `get_passage` gives it.
    async fn joined_chapter(
        &self,
        translation: &Translation,
        book_id: i64,
        chapter: i64,
    ) -> anyhow::Result<JoinedVerses> {
        let verses = db::get_chapter_verses(&self.pool, translation.id, book_id, chapter).await?;
        Ok(JoinedVerses::new(
            verses
                .iter()
                .take(MAX_PASSAGE_VERSES)
                .map(|v| (v.verse, v.shown(&translation.code).text)),
        ))
    }
}

/// A verse of a [`JoinedVerses`]: its number, and its bytes in the text.
#[derive(Debug)]
struct JoinedVerse {
    number: i64,
    start: usize,
    end: usize,
}

/// Consecutive verses' text, joined by one space, knowing which verse is where.
#[derive(Debug, Default)]
struct JoinedVerses {
    text: String,
    verses: Vec<JoinedVerse>,
}

impl JoinedVerses {
    fn new(verses: impl IntoIterator<Item = (i64, String)>) -> Self {
        let mut joined = Self::default();
        for (number, text) in verses {
            if !joined.verses.is_empty() {
                joined.text.push(' ');
            }
            let start = joined.text.len();
            joined.text.push_str(&text);
            joined.verses.push(JoinedVerse {
                number,
                start,
                end: joined.text.len(),
            });
        }
        joined
    }

    /// Each place `quote` (not empty, no whitespace at its ends) is, character
    /// for character, even across verses: the verses it's in, first and last,
    /// and their whole text.
    fn places_of(&self, quote: &str) -> Vec<((i64, i64), &str)> {
        let mut places: Vec<((i64, i64), &str)> = self
            .text
            .char_indices()
            .filter(|&(at, _)| self.text[at..].starts_with(quote))
            .filter_map(|(at, _)| {
                let starting_by = |limit: usize| self.verses.iter().rfind(|v| v.start <= limit);
                let first = starting_by(at)?;
                let last = starting_by(at + quote.len().checked_sub(1)?)?;
                let text = &self.text[first.start..last.end];
                Some(((first.number, last.number), text))
            })
            .collect();
        places.dedup();
        places
    }

    /// The verses from `verse` on, as many as it takes to be at least `chars`
    /// characters long, or all of them if they aren't: the first and last
    /// verses, and their whole text.
    fn window_from(&self, verse: i64, chars: usize) -> Option<((i64, i64), &str)> {
        let at = self.verses.iter().position(|v| v.number == verse)?;
        let verses = &self.verses[at..];
        let first = verses.first()?;
        let last = verses
            .iter()
            .find(|v| self.text[first.start..v.end].chars().count() >= chars)
            .or(verses.last())?;
        Some((
            (first.number, last.number),
            &self.text[first.start..last.end],
        ))
    }
}

/// How a quote stands to a passage's text, if it's there at all: the promise of
/// this tool, in characters, with nothing folded and nothing trimmed.
fn verbatim(quote: &str, text: &str) -> Option<QuoteStatus> {
    if quote == text {
        Some(QuoteStatus::Exact)
    } else if text.contains(quote) {
        Some(QuoteStatus::ExactPart)
    } else {
        None
    }
}

/// The answer for a passage that has the quote.
fn held(
    status: QuoteStatus,
    translation: &Translation,
    place: QuotePlace,
    found_at: Vec<QuotePlace>,
    cut_short: bool,
) -> VerifyQuoteOut {
    let label = label_of(translation);
    let QuotePlace { reference, text } = place;
    let what = if status == QuoteStatus::Exact {
        format!("This is exactly {reference} in the {label}.")
    } else {
        format!("This is an exact part of {reference} in the {label}.")
    };
    VerifyQuoteOut {
        status,
        translation: translation.code.clone(),
        message: format!("{what}{}", more_places(&found_at, cut_short)),
        reference: Some(reference),
        text: Some(text),
        difference: None,
        found_at,
    }
}

/// How a translation is named in a message: "KJV".
fn label_of(translation: &Translation) -> String {
    translation.code.to_uppercase()
}

/// What a message adds when a quote is in several places.
fn more_places(places: &[QuotePlace], cut_short: bool) -> String {
    let count = places.len();
    match (count, cut_short) {
        (0 | 1, _) => String::new(),
        (_, false) => format!(" It's in {count} places; found_at lists them."),
        (_, true) => format!(" It's in at least {count} places; found_at lists those."),
    }
}

/// What a message adds when not every verse with the quote's words was read.
fn not_all_checked(cut_short: bool) -> String {
    if cut_short {
        format!(" Its words are in more than {MAX_CANDIDATES} verses, and not all were checked.")
    } else {
        String::new()
    }
}

/// Refuse what can't be checked.
fn check_length(quote: &str) -> anyhow::Result<()> {
    if quote.is_empty() {
        bail!("There's nothing to check: the quotation is empty.");
    }
    let chars = quote.chars().count();
    if chars > MAX_QUOTE_CHARS {
        bail!("That's {chars} characters; at most {MAX_QUOTE_CHARS} per quotation. Split it.");
    }
    Ok(())
}

/// The words of a quote that matter to a search, lowercased: runs of letters
/// and digits, as the search box and the index split them.
fn quote_words(quote: &str) -> Vec<String> {
    quote
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .take(PHRASE_STARTS + PHRASE_WORDS)
        .map(str::to_lowercase)
        .collect()
}

/// The phrases to search for. First the quote's leading words. The index is by
/// verse, so if a verse ends among those they aren't side by side in any one
/// verse, and a verse with them isn't all there is to find: then as many words
/// from each later start, and last the shorter runs of the leading words,
/// longest first, down to one word in a quote short enough for it to tell.
fn phrases_of(words: &[String]) -> impl Iterator<Item = &[String]> {
    let leading = words.len().min(PHRASE_WORDS);
    let first = (leading > 0).then(|| &words[..leading]);
    let later = words.windows(PHRASE_WORDS).skip(1).take(PHRASE_STARTS - 1);
    let shortest = if words.len() > PHRASE_WORDS { 3 } else { 1 };
    let shorter = (shortest..leading)
        .rev()
        .flat_map(move |length| words[..leading].windows(length));
    first.into_iter().chain(later).chain(shorter)
}

/// Whether a phrase too common to read all of could have hidden the quote: its
/// own leading words; or, in a quote short enough for every split to have been
/// tried, the words either side of the end of a verse, with neither side read in
/// full. Had either been, the chapter with the quote would have been read.
fn may_have_missed(words: &[String], too_common: &HashSet<&[String]>) -> bool {
    let leading = &words[..words.len().min(PHRASE_WORDS)];
    too_common.contains(leading)
        || (words.len() <= PHRASE_WORDS
            && (1..words.len()).any(|split| {
                too_common.contains(&words[..split]) && too_common.contains(&words[split..])
            }))
}

/// How many characters two runs share from their start.
fn common_prefix(a: &[char], b: &[char]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

/// Which word the character at `position` (from 1) of `quote` is in, from 1.
fn word_at(quote: &[char], position: usize) -> usize {
    let mut words = 0;
    let mut in_word = false;
    for c in quote.iter().take(position) {
        if c.is_whitespace() {
            in_word = false;
        } else if !in_word {
            in_word = true;
            words += 1;
        }
    }
    words
}

/// Where `quote`, which isn't in `text`, first departs from it. It's aligned at
/// the place in `text` that shares the most with its start, the earliest of
/// any equal. In characters throughout, so nothing is cut inside one.
fn first_difference(quote: &str, text: &str) -> QuoteDifference {
    let quote: Vec<char> = quote.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut start, mut common) = (0, 0);
    for at in 0..text.len() {
        let shared = common_prefix(&quote, &text[at..]);
        if shared > common {
            (start, common) = (at, shared);
        }
    }
    let position = common + 1;
    QuoteDifference {
        position,
        word: word_at(&quote, position),
        quoted: quote[common..].iter().take(SHOWN_CHARS).collect(),
        actual: text[start + common..].iter().take(SHOWN_CHARS).collect(),
    }
}

#[cfg(test)]
mod tests;
