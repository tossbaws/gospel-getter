//! `gospel-getter-cli`: lets a local AI agent, or a person in a terminal, read
//! the reader's own Gospel Getter install. Subcommands print JSON on stdout;
//! `mcp` serves the same tools over stdio. Both are thin layers over `query`,
//! which only ever holds a read-only connection to the database.
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
    CompareArgs, GetCrossReferencesArgs, GetPassageArgs, ListBookmarksArgs, ListHighlightsArgs,
    ListNotesArgs, Request, SearchArgs, VerifyQuoteArgs,
};

/// Read your Gospel Getter install, read-only. Prints JSON on stdout; errors
/// are plain text on stderr, with a non-zero exit.
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
    #[command(name = "list_translations", visible_alias = "translations")]
    ListTranslations,
    /// A passage's exact text.
    #[command(name = "get_passage", visible_alias = "passage")]
    GetPassage(GetPassageArgs),
    /// Search the text of one translation, in Bible order.
    #[command(name = "search")]
    Search(SearchArgs),
    /// A passage in several translations, each numbered its own way.
    #[command(name = "compare")]
    Compare(CompareArgs),
    /// The cross-references of a verse, a range or a chapter.
    #[command(name = "get_cross_references", visible_alias = "cross_references")]
    GetCrossReferences(GetCrossReferencesArgs),
    /// Where the reader last was.
    #[command(name = "get_reading_place", visible_alias = "reading_place")]
    GetReadingPlace,
    /// The reader's bookmarks, newest first, with their verses.
    #[command(name = "list_bookmarks", visible_alias = "bookmarks")]
    ListBookmarks(ListBookmarksArgs),
    /// The reader's highlights, in Bible order, with their verses.
    #[command(name = "list_highlights", visible_alias = "highlights")]
    ListHighlights(ListHighlightsArgs),
    /// The reader's notes, in Bible order, with their verses.
    #[command(name = "list_notes", visible_alias = "notes")]
    ListNotes(ListNotesArgs),
    /// Check a quotation against the real text, character for character.
    #[command(name = "verify_quote", visible_alias = "verify-quote")]
    VerifyQuote(VerifyQuoteArgs),
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
            Self::Search(args) => Request::Search(args),
            Self::Compare(args) => Request::Compare(args),
            Self::GetCrossReferences(args) => Request::GetCrossReferences(args),
            Self::GetReadingPlace => Request::GetReadingPlace,
            Self::ListBookmarks(args) => Request::ListBookmarks(args),
            Self::ListHighlights(args) => Request::ListHighlights(args),
            Self::ListNotes(args) => Request::ListNotes(args),
            Self::VerifyQuote(args) => Request::VerifyQuote(args),
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

    /// The names of the tools work as subcommands, and so do the short forms.
    #[test]
    fn subcommands_answer_to_tool_names_and_short_forms() {
        for (name, short) in [
            ("list_translations", "translations"),
            ("get_reading_place", "reading_place"),
        ] {
            for word in [name, short] {
                assert!(Cli::try_parse_from(["gospel-getter-cli", word]).is_ok());
            }
        }
        for (name, short) in [
            ("get_passage", "passage"),
            ("get_cross_references", "cross_references"),
        ] {
            for word in [name, short] {
                assert!(Cli::try_parse_from(["gospel-getter-cli", word, "John 3:16"]).is_ok());
            }
        }
        for word in ["verify_quote", "verify-quote"] {
            let cli = Cli::try_parse_from([
                "gospel-getter-cli",
                word,
                "Jesus wept.",
                "--reference",
                "John 11:35",
                "--translation",
                "kjv",
            ])
            .expect("it parses");
            assert!(matches!(cli.command, Command::VerifyQuote(_)));
        }
    }
}
