//! `gospel-getter-cli`: lets a local AI agent, or a person in a terminal, read
//! the reader's own Gospel Getter install. Subcommands print JSON on stdout;
//! `mcp` serves the same tools over stdio. Both are thin layers over `query`,
//! which reads through a read-only connection to the database. The one thing
//! either writes is an AI-flagged finding (`save_findings`), once the reader
//! has turned that on in the app, and through a writer that can do nothing
//! else.
//!
//! A normal console program on every platform (no `windows_subsystem`, unlike
//! the app), so it prints in a Windows terminal.

mod mcp;
mod query;

use anyhow::Context;
use clap::{Parser, Subcommand};
use gospel_getter_core::db;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use query::{
    CompareArgs, GetCrossReferencesArgs, GetPassageArgs, GetPassagesArgs, ListBookmarksArgs,
    ListFindingsArgs, ListHighlightsArgs, ListNotesArgs, Request, SaveFindingsArgs, SearchArgs,
    VerifyQuoteArgs,
};

/// Read your Gospel Getter install, read-only but for saving AI findings
/// when you've turned that on. Prints JSON on stdout; errors are plain text
/// on stderr, with a non-zero exit.
#[derive(Debug, Parser)]
#[command(name = "gospel-getter-cli", version)]
struct Cli {
    /// The database to read, instead of the one the app uses.
    #[arg(long, global = true, value_name = "PATH")]
    db: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

/// Each is the MCP tool of the same name, with the same arguments and JSON.
#[derive(Debug, Subcommand)]
enum Command {
    /// The translations in this install, and which one is the default.
    #[command(name = "list_translations", visible_aliases = ["translations", "list-translations"])]
    ListTranslations,
    /// A passage's exact text, or several: "John 3:16, 18; Rom 5:8".
    #[command(name = "get_passage", visible_aliases = ["passage", "get-passage"])]
    GetPassage(GetPassageArgs),
    /// Many passages in one call, all in one translation.
    #[command(name = "get_passages", visible_aliases = ["passages", "get-passages"])]
    GetPassages(GetPassagesArgs),
    /// Search the text of one translation, in Bible order.
    #[command(name = "search")]
    Search(SearchArgs),
    /// A passage in several translations, each numbered its own way.
    #[command(name = "compare")]
    Compare(CompareArgs),
    /// The cross-references of a verse, a range or a chapter.
    #[command(
        name = "get_cross_references",
        visible_aliases = ["cross_references", "get-cross-references", "cross-references"]
    )]
    GetCrossReferences(GetCrossReferencesArgs),
    /// Where the reader last was.
    #[command(
        name = "get_reading_place",
        visible_aliases = ["reading_place", "get-reading-place", "reading-place"]
    )]
    GetReadingPlace,
    /// The reader's bookmarks, newest first, with their verses.
    #[command(name = "list_bookmarks", visible_aliases = ["bookmarks", "list-bookmarks"])]
    ListBookmarks(ListBookmarksArgs),
    /// The reader's highlights, in Bible order, with their verses.
    #[command(name = "list_highlights", visible_aliases = ["highlights", "list-highlights"])]
    ListHighlights(ListHighlightsArgs),
    /// The reader's notes, in Bible order, with their verses.
    #[command(name = "list_notes", visible_aliases = ["notes", "list-notes"])]
    ListNotes(ListNotesArgs),
    /// Check a quotation against the real text, character for character.
    #[command(name = "verify_quote", visible_alias = "verify-quote")]
    VerifyQuote(VerifyQuoteArgs),
    /// The AI-flagged findings saved, in Bible order: where, by whom, for what.
    #[command(name = "list_findings", visible_aliases = ["findings", "list-findings"])]
    ListFindings(ListFindingsArgs),
    /// Mark verses as AI-flagged findings for one of the reader's requests,
    /// once they've turned that on in the app (Aa, then AI agents). Only the
    /// places: nothing about what the verses say.
    #[command(name = "save_findings", visible_alias = "save-findings")]
    SaveFindings(SaveFindingsArgs),
    /// Serve these tools to an MCP client over stdio.
    #[command(name = "mcp")]
    Mcp,
}

impl Command {
    /// The question this asks, or `None` for `mcp`, which asks none itself.
    fn request(self) -> Option<Request> {
        Some(match self {
            Self::ListTranslations => Request::ListTranslations,
            Self::GetPassage(args) => Request::GetPassage(args),
            Self::GetPassages(args) => Request::GetPassages(args),
            Self::Search(args) => Request::Search(args),
            Self::Compare(args) => Request::Compare(args),
            Self::GetCrossReferences(args) => Request::GetCrossReferences(args),
            Self::GetReadingPlace => Request::GetReadingPlace,
            Self::ListBookmarks(args) => Request::ListBookmarks(args),
            Self::ListHighlights(args) => Request::ListHighlights(args),
            Self::ListNotes(args) => Request::ListNotes(args),
            Self::VerifyQuote(args) => Request::VerifyQuote(args),
            Self::ListFindings(args) => Request::ListFindings(args),
            // On the command line there's no client to name: the agent is
            // `--agent`, else "AI agent".
            Self::SaveFindings(args) => Request::SaveFindings { args, client: None },
            Self::Mcp => return None,
        })
    }
}

/// The app's database, where the app resolves it: its data folder is the
/// platform's data directory and its identifier, as Tauri's `app_data_dir`.
fn default_database() -> anyhow::Result<PathBuf> {
    let data_dir = dirs::data_dir().context("Couldn't find this computer's data folder")?;
    Ok(database_in(&data_dir))
}

fn database_in(data_dir: &Path) -> PathBuf {
    data_dir.join(db::APP_IDENTIFIER).join(db::DATABASE_FILE)
}

/// Print one answer as pretty JSON. A reader that stopped reading (`| head`)
/// isn't a failure.
fn print_json(json: &str) -> anyhow::Result<()> {
    let mut stdout = std::io::stdout().lock();
    match writeln!(stdout, "{json}").and_then(|()| stdout.flush()) {
        Err(e) if e.kind() != std::io::ErrorKind::BrokenPipe => {
            Err(e).context("Couldn't write to stdout")
        }
        _ => Ok(()),
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let database = match cli.db {
        Some(path) => path,
        None => default_database()?,
    };
    match cli.command.request() {
        Some(request) => {
            let answer = query::run(&database, request).await?;
            let json = serde_json::to_string_pretty(&answer).context("Couldn't write the JSON")?;
            print_json(&json)
        }
        None => mcp::serve(database).await,
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    /// The database is where the app's `setup_app` puts it.
    #[test]
    fn the_default_database_is_in_the_apps_data_folder() {
        assert_eq!(
            database_in(Path::new("data")),
            Path::new("data")
                .join("com.tossbaws.gospel-getter")
                .join("gospel_getter.db")
        );
    }

    /// What the program parses these words as.
    fn command_of(words: &[&str]) -> Command {
        let line = std::iter::once("gospel-getter-cli").chain(words.iter().copied());
        Cli::try_parse_from(line)
            .unwrap_or_else(|e| panic!("{words:?}: {e}"))
            .command
    }

    /// Every name of every subcommand: the tool's own, its short form, each in
    /// snake and kebab case, all of it the same command. Scripts use the old
    /// names, so these only ever grow.
    #[test]
    fn every_name_of_a_subcommand_is_that_subcommand() {
        type Is = fn(&Command) -> bool;
        let table: [(&[&str], &[&str], Is); 13] = [
            (
                &["list_translations", "translations", "list-translations"],
                &[],
                |c| matches!(c, Command::ListTranslations),
            ),
            (
                &["get_passage", "passage", "get-passage"],
                &["John 3:16"],
                |c| matches!(c, Command::GetPassage(_)),
            ),
            (
                &["get_passages", "passages", "get-passages"],
                &["John 3:16", "Rom 5:8"],
                |c| matches!(c, Command::GetPassages(_)),
            ),
            (&["search"], &["grace"], |c| matches!(c, Command::Search(_))),
            (&["compare"], &["John 3:16"], |c| {
                matches!(c, Command::Compare(_))
            }),
            (
                &[
                    "get_cross_references",
                    "cross_references",
                    "get-cross-references",
                    "cross-references",
                ],
                &["John 3:16"],
                |c| matches!(c, Command::GetCrossReferences(_)),
            ),
            (
                &[
                    "get_reading_place",
                    "reading_place",
                    "get-reading-place",
                    "reading-place",
                ],
                &[],
                |c| matches!(c, Command::GetReadingPlace),
            ),
            (
                &["list_bookmarks", "bookmarks", "list-bookmarks"],
                &[],
                |c| matches!(c, Command::ListBookmarks(_)),
            ),
            (
                &["list_highlights", "highlights", "list-highlights"],
                &[],
                |c| matches!(c, Command::ListHighlights(_)),
            ),
            (&["list_notes", "notes", "list-notes"], &[], |c| {
                matches!(c, Command::ListNotes(_))
            }),
            (
                &["verify_quote", "verify-quote"],
                &[
                    "Jesus wept.",
                    "--reference",
                    "John 11:35",
                    "--translation",
                    "kjv",
                ],
                |c| matches!(c, Command::VerifyQuote(_)),
            ),
            (&["list_findings", "findings", "list-findings"], &[], |c| {
                matches!(c, Command::ListFindings(_))
            }),
            (
                &["save_findings", "save-findings"],
                &["--request", "grief", "John 11:35"],
                |c| matches!(c, Command::SaveFindings(_)),
            ),
        ];
        for (names, rest, is) in table {
            for name in names {
                let words: Vec<&str> = std::iter::once(*name).chain(rest.iter().copied()).collect();
                assert!(is(&command_of(&words)), "{words:?}");
            }
        }
        assert!(matches!(command_of(&["mcp"]), Command::Mcp));
        // A subcommand that needs its arguments doesn't run without them.
        assert!(Cli::try_parse_from(["gospel-getter-cli", "get_passages"]).is_err());
        assert!(Cli::try_parse_from(["gospel-getter-cli", "verify-quote"]).is_err());
        assert!(Cli::try_parse_from(["gospel-getter-cli", "save-findings", "John 11:35"]).is_err());
        assert!(
            Cli::try_parse_from(["gospel-getter-cli", "save-findings", "--request", "grief"])
                .is_err()
        );
    }
}
