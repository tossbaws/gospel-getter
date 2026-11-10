//! AI-flagged findings: `save_findings`, the one tool that writes, and
//! `list_findings`, which reads them (see the governing doc's C1-C10).
//!
//! A finding marks where a verse is and who found it for which of the
//! reader's requests, nothing more (C3): there's nowhere to put words about
//! a verse. Agents only add them (C4): no tool edits or removes one, and
//! nothing here touches the reader's notes, bookmarks or highlights.
//!
//! Everything is read, checked and refused through the read-only
//! [`Library`] first: whether the install has the findings' tables, whether
//! the reader has turned saving on (C6), the request, each reference, the
//! cap. Only then is [`FindingsWriter`] opened, for the one insert, and closed
//! again; it checks the tables and the switch once more in its own
//! transaction.

use anyhow::{Context, bail};
use clap::Args;
use gospel_getter_core::db::findings::{
    self, DEFAULT_AGENT, FindingsError, HOW_TO_TURN_ON, NOT_READY, normalize_agent,
    normalize_request,
};
use gospel_getter_core::db::{FindingsWriter, Passage};
use gospel_getter_core::passage_reference::passage_reference;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

use super::{Library, MAX_BATCH_VERSES, page, page_of, split_references};

/// The most references one save takes, good or bad.
const MAX_SAVE_REFERENCES: usize = MAX_BATCH_VERSES;

/// Where a finding comes from: an AI agent, not the reader.
pub const AI_FINDING: &str = "ai_finding";

/// Arguments of `save_findings`.
#[derive(Debug, Args, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveFindingsArgs {
    /// The reader's topic in a few words, as they asked it: "grief" or "the sun". A short label: at most 60 characters, with no colon, semicolon, dash (— or –) or line break. The app shows it with each finding.
    #[arg(long)]
    pub request: String,
    /// The verses found, as references the app's Go to reads: "John 11:35", "Job 2:11-13" or "Psalm 6". Each verse is one finding. At most 500 verses in all.
    #[arg(required = true, value_name = "REF")]
    pub references: Vec<String>,
    /// Who found them, as the app names them ("Found by Claude"). Default: the MCP client's name; on the command line, "AI agent". At most 60 characters.
    #[arg(long)]
    pub agent: Option<String>,
    /// The translation whose verse numbers the references use: kjv, bsb or web. Default: the one the reader last read.
    #[arg(long)]
    pub translation: Option<String>,
}

/// Arguments of `list_findings`.
#[derive(Debug, Args, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListFindingsArgs {
    /// Only the findings for this request, exactly as it was saved: case
    /// counts, so "grief" and "Grief" are two requests, as they are saved.
    #[arg(long)]
    pub request: Option<String>,
    /// Only findings in this book, such as Job or 1 Cor.
    #[arg(long)]
    pub book: Option<String>,
    /// How many to skip.
    #[arg(long)]
    pub offset: Option<u32>,
    /// How many to return: 100 unless given, at most 500.
    #[arg(long)]
    pub limit: Option<u32>,
}

/// A verse `save_findings` was given, and what became of it.
#[derive(Debug, Serialize)]
pub struct SavedVerseOut {
    pub reference: String,
    /// "saved", or "already_saved" (the same verse for the same request).
    pub status: &'static str,
}

/// A reference `save_findings` couldn't read, and why.
#[derive(Debug, Serialize)]
pub struct FindingErrorOut {
    pub reference: String,
    pub error: String,
}

/// The answer of `save_findings`.
#[derive(Debug, Serialize)]
pub struct SaveFindingsOut {
    pub request: String,
    pub agent: String,
    pub translation: String,
    /// How many verses were saved now, and how many were already saved.
    pub saved: usize,
    pub already_saved: usize,
    /// Each verse, in the order given.
    pub verses: Vec<SavedVerseOut>,
    /// The references that weren't read, each with why.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<FindingErrorOut>,
    /// All of this in a sentence or two, for a person.
    pub message: String,
}

/// The answer of `list_findings`: one page.
#[derive(Debug, Serialize)]
pub struct FindingsOut {
    /// Whether the reader has turned on "Let AI agents save findings".
    pub saving_enabled: bool,
    pub total: usize,
    pub offset: u32,
    pub limit: u32,
    pub findings: Vec<FindingOut>,
}

/// One finding: where it is, who found it and for what. No text: quote a
/// verse with get_passage.
#[derive(Debug, Serialize)]
pub struct FindingOut {
    pub id: i64,
    /// Always "ai_finding": saved by an AI agent, not written by the reader.
    pub source: &'static str,
    /// "John 11:35".
    pub reference: String,
    pub book: String,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    pub agent: String,
    pub request: String,
    pub created_at: String,
}

/// Save the findings `args` names in the install at `database`: one per
/// verse, by `args.agent`, else `client` (the MCP client's name), else
/// "AI agent".
pub async fn save_findings(
    database: &Path,
    args: SaveFindingsArgs,
    client: Option<String>,
) -> anyhow::Result<SaveFindingsOut> {
    let library = Library::open(database).await?;
    let found = library.findings_to_save(&args).await;
    library.close().await;
    let ToSave {
        request,
        translation,
        passages,
        errors,
    } = found?;
    let agent = normalize_agent(
        args.agent
            .as_deref()
            .or(client.as_deref())
            .unwrap_or(DEFAULT_AGENT),
    );
    let saved = if passages.is_empty() {
        findings::SavedFindings::default()
    } else {
        let writer = FindingsWriter::open(database).await?;
        let saved = writer
            .add(
                &agent,
                &request,
                &passages.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
            )
            .await;
        writer.close().await;
        saved.map_err(|e| match e {
            FindingsError::Database(e) => e.context("The findings couldn't be saved"),
            other => anyhow::anyhow!("{other}"),
        })?
    };
    let already: HashSet<Passage> = saved.already.iter().copied().collect();
    let verses: Vec<SavedVerseOut> = passages
        .into_iter()
        .map(|(passage, reference)| SavedVerseOut {
            reference,
            status: if already.contains(&passage) {
                "already_saved"
            } else {
                "saved"
            },
        })
        .collect();
    let message = saved_message(
        &request,
        &agent,
        saved.saved.len(),
        saved.already.len(),
        errors.len(),
    );
    Ok(SaveFindingsOut {
        saved: saved.saved.len(),
        already_saved: saved.already.len(),
        request,
        agent,
        translation,
        verses,
        errors,
        message,
    })
}

/// What a save says, for a person.
fn saved_message(
    request: &str,
    agent: &str,
    saved: usize,
    already: usize,
    failed: usize,
) -> String {
    let plural = |n: usize, one: &str, many: &str| {
        if n == 1 {
            format!("1 {one}")
        } else {
            format!("{n} {many}")
        }
    };
    let mut message = format!(
        "Saved {} for \u{201c}{request}\u{201d}.",
        plural(saved, "finding", "findings")
    );
    if already > 0 {
        message.push_str(&format!(
            " {} already saved.",
            if already == 1 {
                "1 was".to_string()
            } else {
                format!("{already} were")
            }
        ));
    }
    if failed > 0 {
        message.push_str(&format!(
            " {} couldn't be read; errors says why.",
            plural(failed, "reference", "references")
        ));
    }
    if saved + already > 0 {
        message.push_str(&format!(
            " Gospel Getter shows each beside its verse, marked as found by {agent}, for the \
             reader to read for themselves."
        ));
    }
    message
}

/// What a save will save, once everything's been checked.
struct ToSave {
    request: String,
    translation: String,
    /// Each verse once, in the order given, with its reference.
    passages: Vec<(Passage, String)>,
    errors: Vec<FindingErrorOut>,
}

impl Library {
    /// Refuse an install without the findings' tables, as one where saving
    /// is off.
    async fn findings_ready(&self) -> anyhow::Result<()> {
        let mut conn = self
            .pool
            .acquire()
            .await
            .context("Failed to read the database")?;
        if !findings::findings_ready(&mut conn).await? {
            bail!(NOT_READY);
        }
        Ok(())
    }

    /// The verses a save names, each once, after the checks that refuse it
    /// all: the install, the switch (C6), the request and the cap.
    async fn findings_to_save(&self, args: &SaveFindingsArgs) -> anyhow::Result<ToSave> {
        self.findings_ready().await?;
        if !findings::findings_enabled(&self.pool).await? {
            bail!(HOW_TO_TURN_ON);
        }
        let request = normalize_request(&args.request).map_err(|e| anyhow::anyhow!("{e}"))?;
        let translation = self.translation(args.translation.as_deref()).await?;
        let references: Vec<String> = args
            .references
            .iter()
            .flat_map(|r| split_references(r))
            .collect();
        if references.is_empty() {
            bail!("There are no references to save. Try something like John 11:35.");
        }
        if references.len() > MAX_SAVE_REFERENCES {
            bail!(
                "That's {} references; at most {MAX_SAVE_REFERENCES} per call. Split it.",
                references.len()
            );
        }
        let mut passages: Vec<(Passage, String)> = Vec::new();
        let mut seen = HashSet::new();
        let mut errors = Vec::new();
        for reference in references {
            match self.passage_or_why(translation, &reference).await? {
                Ok(passage) => {
                    let book_id = passage.book_id;
                    for verse in &passage.verses {
                        let at = Passage {
                            book_id,
                            chapter: passage.chapter,
                            verse_start: verse.verse,
                            verse_end: verse.verse,
                        };
                        if seen.insert(at) {
                            let named = passage_reference(
                                &passage.book,
                                passage.chapter,
                                Some((verse.verse, verse.verse)),
                            );
                            passages.push((at, named));
                        }
                    }
                }
                Err(error) => errors.push(FindingErrorOut { reference, error }),
            }
            if passages.len() > MAX_BATCH_VERSES {
                bail!(
                    "That's more than {MAX_BATCH_VERSES} verses; at most {MAX_BATCH_VERSES} per \
                     call. Split it."
                );
            }
        }
        Ok(ToSave {
            request,
            translation: translation.code.clone(),
            passages,
            errors,
        })
    }

    pub(super) async fn list_findings(
        &self,
        args: ListFindingsArgs,
    ) -> anyhow::Result<FindingsOut> {
        self.findings_ready().await?;
        let book_id = args
            .book
            .as_deref()
            .map(|name| self.book_named(name).map(|b| b.id))
            .transpose()?;
        // Exactly, as the table tells requests apart (its uniqueness) and the
        // app groups them: what "grief" lists is what saving "grief" again
        // finds already saved.
        let request = args.request.as_deref().map(str::trim);
        let (offset, limit) = page_of(args.offset, args.limit);
        let all: Vec<_> = findings::all_findings(&self.pool)
            .await?
            .into_iter()
            .filter(|f| book_id.is_none_or(|id| f.book_id == id))
            .filter(|f| request.is_none_or(|r| f.request == r))
            .collect();
        let total = all.len();
        let mut found = Vec::new();
        for f in page(all, offset, limit) {
            let book = self.book(f.book_id)?.name.clone();
            found.push(FindingOut {
                id: f.id,
                source: AI_FINDING,
                reference: passage_reference(&book, f.chapter, Some((f.verse_start, f.verse_end))),
                book,
                chapter: f.chapter,
                verse_start: f.verse_start,
                verse_end: f.verse_end,
                agent: f.agent,
                request: f.request,
                created_at: f.created_at,
            });
        }
        Ok(FindingsOut {
            saving_enabled: findings::findings_enabled(&self.pool).await?,
            total,
            offset,
            limit,
            findings: found,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_save_says_what_it_did_in_plain_words() {
        assert_eq!(
            saved_message("grief", "Claude", 2, 1, 1),
            "Saved 2 findings for \u{201c}grief\u{201d}. 1 was already saved. 1 reference \
             couldn't be read; errors says why. Gospel Getter shows each beside its verse, \
             marked as found by Claude, for the reader to read for themselves."
        );
        assert_eq!(
            saved_message("grief", "Claude", 0, 0, 2),
            "Saved 0 findings for \u{201c}grief\u{201d}. 2 references couldn't be read; errors \
             says why."
        );
    }
}
