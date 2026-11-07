//! The one query layer behind both front ends: what each tool takes, what it
//! answers, and how, all through the app's own code (`gospel_getter_core`).
//!
//! It can't write, but for one thing. The only connection it reads with is
//! `db::open_read_only`'s, and every function in `db` that writes takes a
//! `WritePool`, which only the app's `create_pool` makes and nothing here has.
//! The one thing it writes is an AI-flagged finding (`save_findings`, in
//! `findings`), through `db::FindingsWriter`, which can only add findings, and
//! only once the reader has turned that on in the app.
//!
//! Verse text is what the app's Copy gives: `VerseText::text`, the stored text
//! with the KJV's supplied words as plain words and its margin notes left out,
//! and any other translation exactly as stored. Footnotes come apart from it.
//! Nothing here changes a word of a verse.

use anyhow::{Context, anyhow, bail};
use clap::Args;
use gospel_getter_core::db::migrate::SCHEMA_VERSION;
use gospel_getter_core::db::{self, Book, HighlightColor, Translation};
use gospel_getter_core::domain::kjv_markup::{KJV, VerseText};
use gospel_getter_core::domain::passage::{numbering_notice, translations_named};
use gospel_getter_core::domain::query::{self, Interpretation, PassageRef, SearchQuery};
use gospel_getter_core::passage_reference::passage_reference;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::Path;

mod findings;
mod resources;
mod verify;

pub use findings::{
    FindingsOut, ListFindingsArgs, SaveFindingsArgs, SaveFindingsOut, save_findings,
};
pub use resources::{RESOURCE_TEMPLATE, book_name, chapter_resources, read_resource};
pub use verify::{VerifyQuoteArgs, VerifyQuoteOut};

/// What every refusal to read an install that isn't ready ends with.
const OPEN_THE_APP: &str = "Open Gospel Getter once first, then try again.";

/// The most verses of a passage returned at once: more than any chapter has.
const MAX_PASSAGE_VERSES: usize = 200;
/// How many search results come back unless asked otherwise, and the most.
const DEFAULT_SEARCH_LIMIT: u32 = 50;
const MAX_SEARCH_LIMIT: u32 = 200;
/// How many records a list tool returns unless asked otherwise, and the most.
const DEFAULT_LIST_LIMIT: u32 = 100;
const MAX_LIST_LIMIT: u32 = 500;
/// The most cross-references returned at once.
const MAX_CROSS_REFERENCES: usize = 500;
/// The most references, and verses in all, one call for many passages returns.
const MAX_BATCH_REFERENCES: usize = 50;
const MAX_BATCH_VERSES: usize = 500;

// ---- What each tool takes. The subcommands and the MCP tools share these
// structs, so they can't differ: their docs are both the `--help` text and
// the argument schema's descriptions.

/// Arguments of `get_passage`.
#[derive(Debug, Args, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetPassageArgs {
    /// A reference, as the app's Go to reads it: "John 3:16", "John 3:16-18" or "Psalm 23". Or several, separated by commas or semicolons: "John 3:16, 18; Rom 5:8".
    pub reference: String,
    /// A translation's code, such as kjv, bsb or web. Default: the one the reader last read.
    #[arg(long)]
    pub translation: Option<String>,
}

/// Arguments of `get_passages`.
#[derive(Debug, Args, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetPassagesArgs {
    /// References, each as the app's Go to reads it: at most 50, and at most 500 verses in all.
    #[arg(required = true, value_name = "REF")]
    pub references: Vec<String>,
    /// A translation's code, such as kjv, bsb or web, for all of them. Default: the one the reader last read.
    #[arg(long)]
    pub translation: Option<String>,
}

/// Arguments of `search`.
#[derive(Debug, Args, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchArgs {
    /// Words a verse must have; words in double quotes must be side by side.
    pub query: String,
    /// A translation's code, such as kjv, bsb or web. Default: the one the reader last read.
    #[arg(long)]
    pub translation: Option<String>,
    /// Search only this book, such as Romans or 1 Cor.
    #[arg(long)]
    pub book: Option<String>,
    /// How many verses to return: 50 unless given, at most 200.
    #[arg(long)]
    pub limit: Option<u32>,
}

/// Arguments of `compare`.
#[derive(Debug, Args, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompareArgs {
    /// A reference, as the app's Go to reads it: "John 3:16", "John 3:16-18" or "Psalm 23".
    pub reference: String,
    /// Translations' codes, in the order wanted. Default: every translation in this install.
    #[arg(long, value_delimiter = ',')]
    pub translations: Option<Vec<String>>,
}

/// Arguments of `get_cross_references`.
#[derive(Debug, Args, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetCrossReferencesArgs {
    /// A reference, as the app's Go to reads it: "John 3:16", "John 3:16-18" or "Psalm 23".
    pub reference: String,
}

/// Arguments of `list_bookmarks`.
#[derive(Debug, Args, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListBookmarksArgs {
    /// The translation to quote the verses in. Default: the one the reader last read.
    #[arg(long)]
    pub translation: Option<String>,
    /// How many to skip.
    #[arg(long)]
    pub offset: Option<u32>,
    /// How many to return: 100 unless given, at most 500.
    #[arg(long)]
    pub limit: Option<u32>,
}

/// Arguments of `list_highlights`.
#[derive(Debug, Args, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListHighlightsArgs {
    /// Only this color: yellow, green, blue or pink.
    #[arg(long)]
    pub color: Option<String>,
    /// The translation to quote the verses in. Default: the one the reader last read.
    #[arg(long)]
    pub translation: Option<String>,
    /// How many to skip.
    #[arg(long)]
    pub offset: Option<u32>,
    /// How many to return: 100 unless given, at most 500.
    #[arg(long)]
    pub limit: Option<u32>,
}

/// Arguments of `list_notes`.
#[derive(Debug, Args, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListNotesArgs {
    /// Only notes on this book, such as Philippians or Phil.
    #[arg(long)]
    pub book: Option<String>,
    /// Only notes with this text in them, in any case.
    #[arg(long)]
    pub contains: Option<String>,
    /// The translation to quote the verses in. Default: the one the reader last read.
    #[arg(long)]
    pub translation: Option<String>,
    /// How many to skip.
    #[arg(long)]
    pub offset: Option<u32>,
    /// How many to return: 100 unless given, at most 500.
    #[arg(long)]
    pub limit: Option<u32>,
}

/// One question, from either front end.
#[derive(Debug)]
pub enum Request {
    ListTranslations,
    GetPassage(GetPassageArgs),
    GetPassages(GetPassagesArgs),
    Search(SearchArgs),
    Compare(CompareArgs),
    GetCrossReferences(GetCrossReferencesArgs),
    GetReadingPlace,
    ListBookmarks(ListBookmarksArgs),
    ListHighlights(ListHighlightsArgs),
    ListNotes(ListNotesArgs),
    VerifyQuote(VerifyQuoteArgs),
    ListFindings(ListFindingsArgs),
    /// The one that writes: `client` is the MCP client's name, the agent's
    /// when it gives none.
    SaveFindings {
        args: SaveFindingsArgs,
        client: Option<String>,
    },
}

// ---- What each tool answers.

/// One tool's answer, serialized as that tool's own JSON.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Answer {
    Translations(TranslationsOut),
    Passage(PassageOut),
    Passages(PassagesOut),
    Search(SearchOut),
    Compare(CompareOut),
    CrossReferences(CrossReferencesOut),
    ReadingPlace(ReadingPlaceOut),
    Bookmarks(BookmarksOut),
    Highlights(HighlightsOut),
    Notes(NotesOut),
    VerifyQuote(VerifyQuoteOut),
    Findings(FindingsOut),
    SavedFindings(SaveFindingsOut),
}

/// The answer of `list_translations`.
#[derive(Debug, Serialize)]
pub struct TranslationsOut {
    /// The code used when a tool is given none.
    pub default: String,
    pub translations: Vec<TranslationOut>,
}

/// A translation, as the app's chooser and credits list it.
#[derive(Debug, Serialize)]
pub struct TranslationOut {
    pub code: String,
    pub name: String,
    pub date: Option<&'static str>,
    pub licence: &'static str,
    pub credit: String,
}

/// A verse: its number, its clean text, and any footnotes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerseOut {
    pub verse: i64,
    pub text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub footnotes: Vec<FootnoteOut>,
}

/// A footnote: its marker letter and its words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FootnoteOut {
    pub label: String,
    pub text: String,
}

/// The answer of `get_passage`.
#[derive(Debug, Serialize)]
pub struct PassageOut {
    /// "John 3:16–18".
    pub reference: String,
    /// The book's number (Genesis is 1), for the tools that save a passage.
    #[serde(skip)]
    pub book_id: i64,
    pub translation: String,
    pub translation_name: String,
    pub book: String,
    pub chapter: i64,
    pub verses: Vec<VerseOut>,
    /// Which of the verses asked for this translation doesn't number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

/// The answer of `get_passages`.
#[derive(Debug, Serialize)]
pub struct PassagesOut {
    pub translation: String,
    pub translation_name: String,
    /// How many verses there are in all the passages.
    pub verse_count: usize,
    /// One for each reference, in the order asked.
    pub passages: Vec<PassageEntry>,
}

/// One reference's part of an answer for many.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum PassageEntry {
    /// What `get_passage` gives for it.
    Passage(PassageOut),
    /// The reference as given, and the sentence `get_passage` refuses it with.
    Failed { reference: String, error: String },
}

/// The answer of `search`.
#[derive(Debug, Serialize)]
pub struct SearchOut {
    pub query: String,
    pub translation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub book: Option<String>,
    /// What was searched for: each phrase in curly quotes, then each word.
    pub terms: Vec<String>,
    /// How many verses match, of which `results` are the first `limit`.
    pub total: i64,
    pub limit: u32,
    pub results: Vec<SearchResultOut>,
    /// The passage the query also names, as the app's search box offers it:
    /// `get_passage` reads it. Alone, when the query is only a reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub go_to: Option<String>,
}

/// A verse a search found.
#[derive(Debug, Serialize)]
pub struct SearchResultOut {
    pub reference: String,
    pub book: String,
    pub chapter: i64,
    pub verse: i64,
    pub text: String,
}

/// The answer of `compare`.
#[derive(Debug, Serialize)]
pub struct CompareOut {
    pub reference: String,
    pub book: String,
    pub chapter: i64,
    pub columns: Vec<CompareColumnOut>,
}

/// One translation's verses in a comparison.
#[derive(Debug, Serialize)]
pub struct CompareColumnOut {
    pub translation: String,
    pub translation_name: String,
    pub verses: Vec<VerseOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

/// The answer of `get_cross_references`.
#[derive(Debug, Serialize)]
pub struct CrossReferencesOut {
    pub reference: String,
    /// How many there are, of which `cross_references` are the first 500.
    pub total: usize,
    pub cross_references: Vec<CrossReferenceOut>,
}

/// A cross-reference: which verse it's from, and the passage it names.
#[derive(Debug, Serialize)]
pub struct CrossReferenceOut {
    pub from_verse: i64,
    pub reference: String,
    pub book: String,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
}

/// The answer of `get_reading_place`. Only `saved` when there's no saved place.
#[derive(Debug, Default, Serialize)]
pub struct ReadingPlaceOut {
    pub saved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub book: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapter: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verse: Option<i64>,
}

/// A verse quoted with a bookmark, highlight or note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuoteOut {
    pub verse: i64,
    pub text: String,
}

/// A bookmark or highlight's passage, or a note's. What every record has.
#[derive(Debug, Serialize)]
pub struct RecordPassageOut {
    /// "John 3:16–18".
    pub reference: String,
    pub book: String,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    /// The verses the translation numbers in it, in order.
    pub verses: Vec<QuoteOut>,
}

/// The answer of `list_bookmarks`: one page.
#[derive(Debug, Serialize)]
pub struct BookmarksOut {
    pub translation: String,
    pub total: usize,
    pub offset: u32,
    pub limit: u32,
    pub bookmarks: Vec<BookmarkOut>,
}

/// One bookmark.
#[derive(Debug, Serialize)]
pub struct BookmarkOut {
    pub id: i64,
    #[serde(flatten)]
    pub passage: RecordPassageOut,
    pub created_at: String,
}

/// The answer of `list_highlights`: one page.
#[derive(Debug, Serialize)]
pub struct HighlightsOut {
    pub translation: String,
    pub total: usize,
    pub offset: u32,
    pub limit: u32,
    pub highlights: Vec<HighlightOut>,
}

/// One highlighted passage: consecutive verses of a chapter in one color.
#[derive(Debug, Serialize)]
pub struct HighlightOut {
    pub color: HighlightColor,
    #[serde(flatten)]
    pub passage: RecordPassageOut,
}

/// The answer of `list_notes`: one page.
#[derive(Debug, Serialize)]
pub struct NotesOut {
    pub translation: String,
    pub total: usize,
    pub offset: u32,
    pub limit: u32,
    pub notes: Vec<NoteOut>,
}

/// Where a note's body comes from: the reader. It's data, not instructions.
pub const READER_NOTE: &str = "reader_note";

/// One note.
#[derive(Debug, Serialize)]
pub struct NoteOut {
    pub id: i64,
    /// Always "reader_note": the body is text the reader wrote.
    pub source: &'static str,
    #[serde(flatten)]
    pub passage: RecordPassageOut,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
}

// ---- Answering.

/// Answer one question: open the database, read, and close it again. Saving
/// findings opens it twice: to read and check everything, then to save.
pub async fn run(database: &Path, request: Request) -> anyhow::Result<Answer> {
    if let Request::SaveFindings { args, client } = request {
        return Ok(Answer::SavedFindings(
            save_findings(database, args, client).await?,
        ));
    }
    let library = Library::open(database).await?;
    let answer = library.answer(request).await;
    library.close().await;
    answer
}

/// An install, opened read-only: its connection, books and translations.
#[derive(Debug)]
pub struct Library {
    pool: SqlitePool,
    books: Vec<Book>,
    /// In the order the app lists them.
    translations: Vec<Translation>,
}

impl Library {
    /// Open the install at `database`, or say why it can't be read yet.
    pub async fn open(database: &Path) -> anyhow::Result<Self> {
        if !database.is_file() {
            bail!(
                "There's no Gospel Getter database at {}. {OPEN_THE_APP}",
                database.display()
            );
        }
        let pool = db::open_read_only(database)
            .await
            .with_context(|| format!("Couldn't open the database at {}", database.display()))?;
        match Self::load(pool.clone()).await {
            Ok(library) => Ok(library),
            Err(e) => {
                pool.close().await;
                Err(e.context(format!(
                    "Couldn't read the database at {}",
                    database.display()
                )))
            }
        }
    }

    async fn load(pool: SqlitePool) -> anyhow::Result<Self> {
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&pool)
            .await
            .context("Failed to read the schema version")?;
        if version < SCHEMA_VERSION {
            bail!(
                "This database is from an older Gospel Getter (schema {version}; this program \
                 reads {SCHEMA_VERSION}). {OPEN_THE_APP}"
            );
        }
        let books = db::get_all_books(&pool).await?;
        let translations = db::get_listed_translations(&pool).await?;
        Ok(Self {
            pool,
            books,
            translations,
        })
    }

    /// Close the connection.
    pub async fn close(&self) {
        self.pool.close().await;
    }

    async fn answer(&self, request: Request) -> anyhow::Result<Answer> {
        Ok(match request {
            Request::ListTranslations => Answer::Translations(self.list_translations().await?),
            Request::GetPassage(args) => self.get_passage(args).await?,
            Request::GetPassages(args) => Answer::Passages(self.get_passages(args).await?),
            Request::Search(args) => Answer::Search(self.search(args).await?),
            Request::Compare(args) => Answer::Compare(self.compare(args).await?),
            Request::GetCrossReferences(args) => {
                Answer::CrossReferences(self.get_cross_references(args).await?)
            }
            Request::GetReadingPlace => Answer::ReadingPlace(self.get_reading_place().await?),
            Request::ListBookmarks(args) => Answer::Bookmarks(self.list_bookmarks(args).await?),
            Request::ListHighlights(args) => Answer::Highlights(self.list_highlights(args).await?),
            Request::ListNotes(args) => Answer::Notes(self.list_notes(args).await?),
            Request::VerifyQuote(args) => Answer::VerifyQuote(self.verify_quote(args).await?),
            Request::ListFindings(args) => Answer::Findings(self.list_findings(args).await?),
            // `run` saves findings itself, with the writer; this only reads.
            Request::SaveFindings { .. } => bail!("Saving findings needs the database's path."),
        })
    }

    fn book(&self, book_id: i64) -> anyhow::Result<&Book> {
        self.books
            .iter()
            .find(|b| b.id == book_id)
            .ok_or_else(|| anyhow!("There's no book {book_id} in this database."))
    }

    /// The book named `name`, read as Go to reads one.
    fn book_named(&self, name: &str) -> anyhow::Result<&Book> {
        query::find_book(name, &self.books)
            .ok_or_else(|| anyhow!("There's no book called \u{201c}{name}\u{201d}."))
    }

    /// The translation with this code, in any case.
    fn translation_named(&self, code: &str) -> anyhow::Result<&Translation> {
        self.translations
            .iter()
            .find(|t| t.code.eq_ignore_ascii_case(code))
            .ok_or_else(|| {
                let codes: Vec<&str> = self.translations.iter().map(|t| t.code.as_str()).collect();
                anyhow!(
                    "There's no translation \u{201c}{code}\u{201d}. This install has: {}.",
                    codes.join(", ")
                )
            })
    }

    /// The translation of the saved reading position, else the KJV.
    async fn default_translation(&self) -> anyhow::Result<&Translation> {
        let saved = db::get_reading_position(&self.pool).await?;
        saved
            .and_then(|(id, _, _)| self.translations.iter().find(|t| t.id == id))
            .or_else(|| self.translations.iter().find(|t| t.code == KJV))
            .or(self.translations.first())
            .ok_or_else(|| anyhow!("This database has no translations. {OPEN_THE_APP}"))
    }

    async fn translation(&self, code: Option<&str>) -> anyhow::Result<&Translation> {
        match code {
            Some(code) => self.translation_named(code),
            None => self.default_translation().await,
        }
    }

    /// The passage `reference` names, read as Go to reads it: a book's name
    /// alone is its first chapter.
    fn passage_named(&self, reference: &str) -> anyhow::Result<PassageRef> {
        match query::interpret(reference, &self.books) {
            Interpretation::Passage(passage) | Interpretation::TextOrBook(_, passage) => {
                Ok(passage)
            }
            Interpretation::InvalidReference(message) => bail!(message),
            Interpretation::Empty | Interpretation::Text(_) => bail!(
                "\u{201c}{}\u{201d} isn't a Bible reference. Try something like John 3:16.",
                reference.trim()
            ),
        }
    }

    /// The verses of a passage this translation numbers, as the app shows
    /// them, and a sentence on any it doesn't.
    async fn read(
        &self,
        translation: &Translation,
        passage: PassageRef,
    ) -> anyhow::Result<(Vec<VerseOut>, Option<String>)> {
        let book = self.book(passage.book_id)?;
        let chapter =
            db::get_chapter_verses(&self.pool, translation.id, book.id, passage.chapter).await?;
        let last = chapter.last().map_or(0, |v| v.verse);
        let in_passage = |verse: i64| {
            passage
                .verses
                .is_none_or(|(first, last)| (first..=last).contains(&verse))
        };
        let verses: Vec<VerseOut> = chapter
            .iter()
            .filter(|v| in_passage(v.verse))
            .map(|v| verse_out(v.verse, v.shown(&translation.code)))
            .collect();
        let notice = match passage.verses {
            Some((first, end)) => {
                let last_anywhere =
                    db::get_chapter_last_verse(&self.pool, book.id, passage.chapter).await?;
                let numbered: Vec<i64> = verses.iter().map(|v| v.verse).collect();
                numbering_notice(
                    &book.name,
                    &translation.name,
                    passage.chapter,
                    (first, end),
                    &numbered,
                    last,
                    last_anywhere,
                )
            }
            None => None,
        };
        Ok(capped_passage(verses, notice))
    }

    async fn list_translations(&self) -> anyhow::Result<TranslationsOut> {
        Ok(TranslationsOut {
            default: self.default_translation().await?.code.clone(),
            translations: self
                .translations
                .iter()
                .map(|t| {
                    let source = db::seed::source(&t.code);
                    TranslationOut {
                        code: t.code.clone(),
                        name: t.name.clone(),
                        date: source.and_then(|s| s.date),
                        licence: source.map_or("", |s| s.licence),
                        credit: source.map_or_else(|| t.name.clone(), |s| s.credit.to_string()),
                    }
                })
                .collect(),
        })
    }

    /// One passage, as ever; several references are answered as `get_passages`
    /// answers them.
    async fn get_passage(&self, args: GetPassageArgs) -> anyhow::Result<Answer> {
        let translation = self.translation(args.translation.as_deref()).await?;
        let references = split_references(&args.reference);
        let one = match references.as_slice() {
            [] => &args.reference,
            [one] => one,
            _ => {
                let passages = self.passages_in(translation, &references).await?;
                return Ok(Answer::Passages(passages));
            }
        };
        Ok(Answer::Passage(self.passage_in(translation, one).await?))
    }

    /// What `reference` names in `translation`, or why there's nothing to give.
    async fn passage_in(
        &self,
        translation: &Translation,
        reference: &str,
    ) -> anyhow::Result<PassageOut> {
        self.passage_or_why(translation, reference)
            .await?
            .map_err(|why| anyhow!(why))
    }

    /// As [`Self::passage_in`], but a reference that names nothing there is only
    /// the sentence saying so: an error here is a failure to read the database.
    async fn passage_or_why(
        &self,
        translation: &Translation,
        reference: &str,
    ) -> anyhow::Result<Result<PassageOut, String>> {
        let passage = match self.passage_named(reference) {
            Ok(passage) => passage,
            Err(why) => return Ok(Err(format!("{why:#}"))),
        };
        let book = self.book(passage.book_id)?;
        let reference = passage_reference(&book.name, passage.chapter, passage.verses);
        let (verses, notice) = self.read(translation, passage).await?;
        if verses.is_empty() {
            return Ok(Err(notice.unwrap_or_else(|| {
                format!("{reference} isn't numbered in the {}.", translation.name)
            })));
        }
        Ok(Ok(PassageOut {
            reference,
            book_id: book.id,
            translation: translation.code.clone(),
            translation_name: translation.name.clone(),
            book: book.name.clone(),
            chapter: passage.chapter,
            verses,
            notice,
        }))
    }

    async fn get_passages(&self, args: GetPassagesArgs) -> anyhow::Result<PassagesOut> {
        let translation = self.translation(args.translation.as_deref()).await?;
        self.passages_in(translation, &args.references).await
    }

    /// What each reference names in `translation`, in the order given. Over
    /// either cap, none of it: a reference that fails is only that one's answer.
    async fn passages_in(
        &self,
        translation: &Translation,
        references: &[String],
    ) -> anyhow::Result<PassagesOut> {
        if references.is_empty() {
            bail!("There are no references to read. Try something like John 3:16.");
        }
        let count = references.len();
        if count > MAX_BATCH_REFERENCES {
            bail!("That's {count} references; at most {MAX_BATCH_REFERENCES} per call. Split it.");
        }
        let mut passages = Vec::with_capacity(count);
        let mut verse_count = 0;
        for reference in references {
            passages.push(match self.passage_or_why(translation, reference).await? {
                Ok(passage) => {
                    verse_count += passage.verses.len();
                    PassageEntry::Passage(passage)
                }
                Err(error) => PassageEntry::Failed {
                    reference: reference.clone(),
                    error,
                },
            });
        }
        if verse_count > MAX_BATCH_VERSES {
            bail!("That's {verse_count} verses; at most {MAX_BATCH_VERSES} per call. Split it.");
        }
        Ok(PassagesOut {
            translation: translation.code.clone(),
            translation_name: translation.name.clone(),
            verse_count,
            passages,
        })
    }

    async fn search(&self, args: SearchArgs) -> anyhow::Result<SearchOut> {
        let translation = self.translation(args.translation.as_deref()).await?;
        let book = args
            .book
            .as_deref()
            .map(|name| self.book_named(name))
            .transpose()?;
        let limit = capped(args.limit, DEFAULT_SEARCH_LIMIT, MAX_SEARCH_LIMIT);
        if !db::search::search_index_is_current(&self.pool, translation.id).await? {
            bail!("The search index isn't built yet. {OPEN_THE_APP}");
        }
        let named = |passage: PassageRef| -> anyhow::Result<String> {
            let name = &self.book(passage.book_id)?.name;
            Ok(passage_reference(name, passage.chapter, passage.verses))
        };
        let (terms, go_to) = match query::interpret(&args.query, &self.books) {
            Interpretation::Empty => bail!("There's nothing to search for."),
            Interpretation::InvalidReference(message) => bail!(message),
            Interpretation::Passage(passage) => (SearchQuery::default(), Some(named(passage)?)),
            Interpretation::Text(terms) => (terms, None),
            Interpretation::TextOrBook(terms, passage) => (terms, Some(named(passage)?)),
        };
        let (total, hits) = match book {
            Some(book) => {
                db::search::search_verses_in_book(
                    &self.pool,
                    translation.id,
                    book.id,
                    &terms,
                    i64::from(limit),
                    0,
                )
                .await?
            }
            None => {
                db::search::search_verses(&self.pool, translation.id, &terms, i64::from(limit), 0)
                    .await?
            }
        };
        let results = hits
            .into_iter()
            .map(|hit| {
                let name = self.book(hit.book_id)?.name.clone();
                Ok(SearchResultOut {
                    reference: passage_reference(&name, hit.chapter, Some((hit.verse, hit.verse))),
                    book: name,
                    chapter: hit.chapter,
                    verse: hit.verse,
                    text: VerseText::new(&translation.code, &hit.text).text,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(SearchOut {
            query: args.query,
            translation: translation.code.clone(),
            book: book.map(|b| b.name.clone()),
            terms: terms.labels(),
            total,
            limit,
            results,
            go_to,
        })
    }

    async fn compare(&self, args: CompareArgs) -> anyhow::Result<CompareOut> {
        let passage = self.passage_named(&args.reference)?;
        let book = self.book(passage.book_id)?;
        let reference = passage_reference(&book.name, passage.chapter, passage.verses);
        let translations: Vec<&Translation> = match &args.translations {
            Some(codes) => {
                let codes: Vec<String> = codes.iter().map(|c| c.to_ascii_lowercase()).collect();
                translations_named(&self.translations, &codes).map_err(|e| self.with_codes(e))?
            }
            None => self.translations.iter().collect(),
        };
        let mut columns = Vec::with_capacity(translations.len());
        for translation in translations {
            let (verses, notice) = self.read(translation, passage).await?;
            columns.push(CompareColumnOut {
                translation: translation.code.clone(),
                translation_name: translation.name.clone(),
                verses,
                notice,
            });
        }
        if columns.iter().all(|c| c.verses.is_empty()) {
            let notice = columns.iter().find_map(|c| c.notice.clone());
            bail!(notice.unwrap_or_else(|| format!("{reference} isn't numbered in any of them.")));
        }
        Ok(CompareOut {
            reference,
            book: book.name.clone(),
            chapter: passage.chapter,
            columns,
        })
    }

    /// A refusal of the translations named, saying which there are.
    fn with_codes(&self, error: anyhow::Error) -> anyhow::Error {
        let codes: Vec<&str> = self.translations.iter().map(|t| t.code.as_str()).collect();
        anyhow!("{error} This install has: {}.", codes.join(", "))
    }

    async fn get_cross_references(
        &self,
        args: GetCrossReferencesArgs,
    ) -> anyhow::Result<CrossReferencesOut> {
        let passage = self.passage_named(&args.reference)?;
        let book = self.book(passage.book_id)?;
        let all = db::get_chapter_cross_references(&self.pool, book.id, passage.chapter).await?;
        let found: Vec<_> = all
            .into_iter()
            .filter(|r| {
                passage
                    .verses
                    .is_none_or(|(first, last)| (first..=last).contains(&r.verse))
            })
            .collect();
        let total = found.len();
        let cross_references = found
            .into_iter()
            .take(MAX_CROSS_REFERENCES)
            .map(|r| {
                let name = &self.book(r.ref_book_id)?.name;
                let verse_end = r.ref_end_verse.unwrap_or(r.ref_verse);
                Ok(CrossReferenceOut {
                    from_verse: r.verse,
                    reference: passage_reference(
                        name,
                        r.ref_chapter,
                        Some((r.ref_verse, verse_end)),
                    ),
                    book: name.clone(),
                    chapter: r.ref_chapter,
                    verse_start: r.ref_verse,
                    verse_end,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(CrossReferencesOut {
            reference: passage_reference(&book.name, passage.chapter, passage.verses),
            total,
            cross_references,
        })
    }

    async fn get_reading_place(&self) -> anyhow::Result<ReadingPlaceOut> {
        let Some(place) = db::get_reading_place(&self.pool).await? else {
            return Ok(ReadingPlaceOut::default());
        };
        let translation = self
            .translations
            .iter()
            .find(|t| t.id == place.translation_id)
            .ok_or_else(|| anyhow!("The saved place is in a translation this install lacks."))?;
        let book = self.book(place.book_id)?;
        let verse = place.at.map(|at| at.verse);
        Ok(ReadingPlaceOut {
            saved: true,
            reference: Some(passage_reference(
                &book.name,
                place.chapter,
                verse.map(|v| (v, v)),
            )),
            translation: Some(translation.code.clone()),
            translation_name: Some(translation.name.clone()),
            book: Some(book.name.clone()),
            chapter: Some(place.chapter),
            verse,
        })
    }

    /// A record's passage, its verses quoted in `translation`.
    async fn record_passage(
        &self,
        translation: &Translation,
        book_id: i64,
        chapter: i64,
        (verse_start, verse_end): (i64, i64),
    ) -> anyhow::Result<RecordPassageOut> {
        let book = self.book(book_id)?;
        let verses = db::get_verse_range(
            &self.pool,
            translation.id,
            book_id,
            chapter,
            verse_start,
            verse_end,
        )
        .await?
        .into_iter()
        .map(|v| QuoteOut {
            verse: v.verse,
            text: VerseText::new(&translation.code, &v.text).text,
        })
        .collect();
        Ok(RecordPassageOut {
            reference: passage_reference(&book.name, chapter, Some((verse_start, verse_end))),
            book: book.name.clone(),
            chapter,
            verse_start,
            verse_end,
            verses,
        })
    }

    async fn list_bookmarks(&self, args: ListBookmarksArgs) -> anyhow::Result<BookmarksOut> {
        let translation = self.translation(args.translation.as_deref()).await?;
        let (offset, limit) = page_of(args.offset, args.limit);
        let all = db::bookmarks::list_bookmarks(&self.pool, translation.id).await?;
        let total = all.len();
        let mut bookmarks = Vec::new();
        for found in page(all, offset, limit) {
            let b = found.bookmark;
            bookmarks.push(BookmarkOut {
                id: b.id,
                passage: self
                    .record_passage(
                        translation,
                        b.book_id,
                        b.chapter,
                        (b.verse_start, b.verse_end),
                    )
                    .await?,
                created_at: b.created_at,
            });
        }
        Ok(BookmarksOut {
            translation: translation.code.clone(),
            total,
            offset,
            limit,
            bookmarks,
        })
    }

    async fn list_highlights(&self, args: ListHighlightsArgs) -> anyhow::Result<HighlightsOut> {
        let translation = self.translation(args.translation.as_deref()).await?;
        let color: Option<HighlightColor> = args
            .color
            .as_deref()
            .map(|c| c.to_ascii_lowercase().parse())
            .transpose()
            .map_err(|e| anyhow!("{e}."))?;
        let (offset, limit) = page_of(args.offset, args.limit);
        let all: Vec<_> = db::highlights::list_highlight_passages(&self.pool, translation.id)
            .await?
            .into_iter()
            .filter(|h| color.is_none_or(|c| h.passage.color == c))
            .collect();
        let total = all.len();
        let mut highlights = Vec::new();
        for found in page(all, offset, limit) {
            let h = found.passage;
            highlights.push(HighlightOut {
                color: h.color,
                passage: self
                    .record_passage(
                        translation,
                        h.book_id,
                        h.chapter,
                        (h.verse_start, h.verse_end),
                    )
                    .await?,
            });
        }
        Ok(HighlightsOut {
            translation: translation.code.clone(),
            total,
            offset,
            limit,
            highlights,
        })
    }

    async fn list_notes(&self, args: ListNotesArgs) -> anyhow::Result<NotesOut> {
        let translation = self.translation(args.translation.as_deref()).await?;
        let book_id = args
            .book
            .as_deref()
            .map(|name| self.book_named(name).map(|b| b.id))
            .transpose()?;
        let needle = args.contains.as_deref().map(str::to_lowercase);
        let (offset, limit) = page_of(args.offset, args.limit);
        let all: Vec<_> = db::notes::list_notes(&self.pool)
            .await?
            .into_iter()
            .filter(|n| book_id.is_none_or(|id| n.book_id == id))
            .filter(|n| {
                needle
                    .as_deref()
                    .is_none_or(|needle| n.body.to_lowercase().contains(needle))
            })
            .collect();
        let total = all.len();
        let mut notes = Vec::new();
        for n in page(all, offset, limit) {
            notes.push(NoteOut {
                id: n.id,
                source: READER_NOTE,
                passage: self
                    .record_passage(
                        translation,
                        n.book_id,
                        n.chapter,
                        (n.verse_start, n.verse_end),
                    )
                    .await?,
                body: n.body,
                created_at: n.created_at,
                updated_at: n.updated_at,
            });
        }
        Ok(NotesOut {
            translation: translation.code.clone(),
            total,
            offset,
            limit,
            notes,
        })
    }
}

/// An answer as a tool gives it: compact JSON.
pub fn to_json(answer: &Answer) -> anyhow::Result<String> {
    serde_json::to_string(answer).context("Couldn't write the JSON")
}

/// The references in `input`, which are separated by commas or semicolons. A part
/// of only numbers goes on from the reference before it (see [`continued`]); a
/// part with a letter in it, "1 John" as much as "Rom", is a reference of its own.
/// Empty parts are dropped. What each part means, `query::interpret` decides.
fn split_references(input: &str) -> Vec<String> {
    let mut references: Vec<String> = Vec::new();
    for part in input
        .split([',', ';'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let reference = match references.last() {
            Some(previous) if is_only_numbers(part) => continued(previous, part),
            _ => None,
        };
        references.push(reference.unwrap_or_else(|| part.to_string()));
    }
    references
}

/// Whether a part of a list of references has only what comes after a book's
/// name, as Go to reads it: "18", "4:2" or "18-20".
fn is_only_numbers(part: &str) -> bool {
    part.starts_with(|c: char| c.is_ascii_digit())
        && part
            .chars()
            .all(|c| c.is_ascii_digit() || " \t:.-\u{2013}\u{2014}".contains(c))
}

/// The reference that the numbers `part` make, going on from `previous`, which
/// has a book's name to go on from, or `None`. It's in the same book. It's in the
/// same chapter too if it names no chapter, and `previous` named verses: "John
/// 3:16, 18" is John 3:18, but "John 3:16, 4:2" is John 4:2 and "Psalm 23, 24" is
/// Psalm 24.
fn continued(previous: &str, part: &str) -> Option<String> {
    let (at, last_letter) = previous.char_indices().rfind(|(_, c)| c.is_alphabetic())?;
    let (book, numbers) = previous.split_at(at + last_letter.len_utf8());
    let chapter = numbers
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .split_once([':', '.'])
        .map(|(chapter, _)| chapter);
    Some(match chapter {
        Some(chapter) if !part.contains([':', '.']) => format!("{book} {chapter}:{part}"),
        _ => format!("{book} {part}"),
    })
}

/// A verse as shown, as a tool answers with it: its text and its footnotes'
/// words, apart.
fn verse_out(verse: i64, shown: VerseText) -> VerseOut {
    VerseOut {
        verse,
        text: shown.text,
        footnotes: shown
            .footnotes
            .into_iter()
            .map(|f| FootnoteOut {
                label: f.label,
                text: f.text,
            })
            .collect(),
    }
}

/// At most `MAX_PASSAGE_VERSES` verses, saying so if there were more.
fn capped_passage(
    mut verses: Vec<VerseOut>,
    notice: Option<String>,
) -> (Vec<VerseOut>, Option<String>) {
    if verses.len() <= MAX_PASSAGE_VERSES {
        return (verses, notice);
    }
    let total = verses.len();
    verses.truncate(MAX_PASSAGE_VERSES);
    let capped = format!("Only the first {MAX_PASSAGE_VERSES} of {total} verses are here.");
    let notice = match notice {
        Some(notice) => format!("{notice} {capped}"),
        None => capped,
    };
    (verses, Some(notice))
}

/// The limit asked for, `default` if none, between 1 and `max`.
fn capped(asked: Option<u32>, default: u32, max: u32) -> u32 {
    asked.unwrap_or(default).clamp(1, max)
}

/// A list tool's offset and limit.
fn page_of(offset: Option<u32>, limit: Option<u32>) -> (u32, u32) {
    (
        offset.unwrap_or(0),
        capped(limit, DEFAULT_LIST_LIMIT, MAX_LIST_LIMIT),
    )
}

/// The records of one page.
fn page<T>(all: Vec<T>, offset: u32, limit: u32) -> impl Iterator<Item = T> {
    all.into_iter().skip(offset as usize).take(limit as usize)
}

#[cfg(test)]
mod tests;
