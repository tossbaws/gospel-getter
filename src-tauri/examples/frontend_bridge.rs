//! Test-only bridge between the jsdom frontend tests (`tests/frontend/`)
//! and the app's real command logic: it opens a disposable database at the
//! path given, prepares it exactly as the app does at startup (migrate,
//! seed, build the search index), then answers one JSON request per line
//! on stdin — `{"id": 1, "cmd": "get_reading", "args": {...}}`, with the
//! same camelCase arguments the frontend passes to Tauri's `invoke` — with
//! one JSON line on stdout: `{"id": 1, "ok": ...}` or `{"id": 1, "err":
//! "..."}`. Never shipped; never pointed at a real install's database.
//!
//! The native file dialogs of export and import can't run here, so a test
//! scripts the next one's answer with `__set_picker` (a path, or `null` for
//! the reader cancelling); the export and import logic behind them is the
//! app's own, reading and writing real files.

use anyhow::{Context, bail};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use gospel_getter_lib::commands::{self, AppState};
use gospel_getter_lib::db;

/// The scripted answer for the next file dialog: `Some(None)` is a
/// cancel. Taken (and cleared) by the dialog it answers.
static PICKER: Mutex<Option<Option<PathBuf>>> = Mutex::new(None);

fn pick() -> anyhow::Result<Option<PathBuf>> {
    PICKER
        .lock()
        .map_err(|_| anyhow::anyhow!("picker lock poisoned"))?
        .take()
        .context("a file dialog opened, but the test set no answer for it (__set_picker)")
}

fn arg<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> anyhow::Result<T> {
    serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null))
        .with_context(|| format!("bad or missing argument {name}"))
}

fn to_value(value: impl serde::Serialize) -> anyhow::Result<Value> {
    Ok(serde_json::to_value(value)?)
}

/// One command, as the Tauri command of the same name would answer it.
/// `Err` is a command error (what `invoke` would reject with).
async fn dispatch(
    state: &AppState,
    pool: &SqlitePool,
    cmd: &str,
    args: &Value,
) -> anyhow::Result<Result<Value, String>> {
    let code: Option<String> = arg(args, "translationCode")?;
    let code = code.as_deref();
    Ok(match cmd {
        "get_home" => Ok(to_value(commands::home(state).await)?),
        "get_chapters" => Ok(to_value(commands::build_chapters_dto(
            state,
            arg(args, "bookId")?,
            arg(args, "active")?,
        ))?),
        "get_reading" => Ok(to_value(
            commands::reading(state, arg(args, "bookId")?, arg(args, "chapter")?, code).await,
        )?),
        "get_xref_text" => Ok(to_value(
            commands::xref_text(
                state,
                arg(args, "bookId")?,
                arg(args, "chapter")?,
                arg(args, "verse")?,
                arg(args, "endVerse")?,
                code,
            )
            .await,
        )?),
        "list_bookmarks" => match commands::bookmarks_in(state, code).await {
            Ok(list) => Ok(to_value(list)?),
            Err(e) => Err(format!("Bookmarks couldn't be loaded: {e:#}")),
        },
        "add_bookmark" => commands::bookmark_passage(
            state,
            arg(args, "bookId")?,
            arg(args, "chapter")?,
            arg(args, "verseStart")?,
            arg(args, "verseEnd")?,
        )
        .await
        .map(Value::from),
        "remove_bookmark" => Ok(Value::from(
            state.store.remove_bookmark(arg(args, "id")?).await?,
        )),
        "search" => {
            let query: String = arg(args, "query")?;
            let offset: Option<i64> = arg(args, "offset")?;
            Ok(to_value(
                commands::run_search(state, &query, code, offset.unwrap_or(0)).await?,
            )?)
        }
        // Like the app's commands of the same names, with `pick()` in place
        // of the native dialogs.
        "export_reader_data" => {
            let data = commands::build_reader_data(state, arg(args, "preferences")?).await?;
            match pick()? {
                None => Ok(to_value(commands::ExportOutcome::Cancelled)?),
                Some(path) => match commands::save_reader_data(&data, &path).await {
                    Ok(outcome) => Ok(to_value(outcome)?),
                    Err(e) => Err(format!("The file couldn't be saved: {e:#}")),
                },
            }
        }
        "choose_import_file" => match pick()? {
            None => Ok(to_value(commands::ImportChoice::Cancelled)?),
            Some(path) => Ok(to_value(commands::prepare_import(state, &path).await?)?),
        },
        "apply_import" => match commands::apply_pending_import(
            state,
            arg(args, "token")?,
            arg(args, "replace")?,
            arg(args, "restorePosition")?,
        )
        .await
        {
            Ok(result) => Ok(to_value(result)?),
            Err(message) => Err(message),
        },
        "cancel_import" => {
            commands::discard_pending_import(state, arg(args, "token")?);
            Ok(Value::Null)
        }
        // Test setup: the answer the next file dialog gives.
        "__set_picker" => {
            let path: Option<PathBuf> = arg(args, "path")?;
            *PICKER
                .lock()
                .map_err(|_| anyhow::anyhow!("picker lock poisoned"))? = Some(path);
            Ok(Value::Null)
        }
        "get_compare" => Ok(to_value(
            commands::compare_chapter(state, arg(args, "bookId")?, arg(args, "chapter")?).await?,
        )?),
        // Test setup, not an app command: forget the reading position
        // (and bookmarks, unless `keepBookmarks` — a restart rather than
        // a new test), then optionally save a reading position (what the
        // app would reopen at).
        "__reset" => {
            *state
                .pending_import
                .lock()
                .map_err(|_| anyhow::anyhow!("pending import lock poisoned"))? = None;
            *PICKER
                .lock()
                .map_err(|_| anyhow::anyhow!("picker lock poisoned"))? = None;
            if !arg::<Option<bool>>(args, "keepBookmarks")?.unwrap_or(false) {
                sqlx::query("DELETE FROM bookmarks").execute(pool).await?;
            }
            sqlx::query("DELETE FROM reading_position")
                .execute(pool)
                .await?;
            if let Some(book_id) = arg::<Option<i64>>(args, "bookId")? {
                let translation_id = state
                    .translations
                    .iter()
                    .find(|t| Some(t.code.as_str()) == code)
                    .map_or(1, |t| t.id);
                state
                    .store
                    .save_reading_position(translation_id, book_id, arg(args, "chapter")?)
                    .await?;
            }
            Ok(Value::Null)
        }
        _ => bail!("unknown command {cmd}"),
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .context("usage: frontend_bridge <database path>")?;
    let pool = db::create_pool(&format!("sqlite:{path}")).await?;
    db::prepare(&pool).await?;
    db::ensure_search_index(&pool).await?;
    let state = AppState::load(pool.clone()).await?;
    state.search_index.set_ready();

    let mut stdout = tokio::io::stdout();
    stdout.write_all(b"{\"ready\":true}\n").await?;
    stdout.flush().await?;

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let request: Value = serde_json::from_str(&line).context("bad request line")?;
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let cmd = request.get("cmd").and_then(Value::as_str).unwrap_or("");
        let args = request.get("args").cloned().unwrap_or_else(|| json!({}));
        let response = match dispatch(&state, &pool, cmd, &args).await {
            Ok(Ok(value)) => json!({ "id": id, "ok": value }),
            Ok(Err(message)) => json!({ "id": id, "err": message }),
            Err(e) => json!({ "id": id, "err": format!("bridge error: {e:#}") }),
        };
        stdout.write_all(format!("{response}\n").as_bytes()).await?;
        stdout.flush().await?;
    }
    Ok(())
}
