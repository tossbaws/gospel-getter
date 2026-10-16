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
//!
//! Updates can't be checked or installed here either (they need the real
//! app and GitHub), so a test scripts them with `__set_update`: what
//! `check_for_update` answers (no update, an update, or a problem) and what
//! `install_update` does (the progress events it sends, then a restart,
//! which never answers, or a problem). Problems answer with the app's own
//! messages (`updater::check_message` and `install_message`). Progress
//! events are written as their own lines, `{"id": 1, "channel": "onEvent",
//! "message": ...}`, before the answer, as a Tauri `Channel` delivers them.
//!
//! The notes export's dates are the computer's, as in the app, unless a
//! test fixes the time and time zone with `__set_clock`; `__note_written`
//! backdates a note, which the app's own commands can't.

use anyhow::{Context, bail};
use chrono::{DateTime, FixedOffset, Local, Offset, Utc};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use gospel_getter_lib::commands::{self, AppState};
use gospel_getter_lib::db::{self, Passage};
use gospel_getter_lib::domain::kjv_markup::VerseText;
use gospel_getter_lib::updater::{self, Problem, UpdateInfo};

/// The scripted answer for the next file dialog: `Some(None)` is a
/// cancel. Taken (and cleared) by the dialog it answers.
static PICKER: Mutex<Option<Option<PathBuf>>> = Mutex::new(None);

/// The scripted update behavior (see `__set_update`); `None` is the
/// default: no update available.
static UPDATE: Mutex<Option<Value>> = Mutex::new(None);

/// A fixed time and time zone for the notes export (see `__set_clock`);
/// `None` is the computer's own.
static CLOCK: Mutex<Option<(DateTime<Utc>, FixedOffset)>> = Mutex::new(None);

/// Now, and the time zone the export reads dates in.
fn clock() -> anyhow::Result<(DateTime<Utc>, FixedOffset)> {
    let fixed = *CLOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("clock lock poisoned"))?;
    Ok(fixed.unwrap_or_else(|| (Utc::now(), Local::now().offset().fix())))
}

fn update_script() -> anyhow::Result<Value> {
    Ok(UPDATE
        .lock()
        .map_err(|_| anyhow::anyhow!("update lock poisoned"))?
        .clone()
        .unwrap_or_else(|| json!({ "check": null })))
}

fn problem(name: &str) -> anyhow::Result<Problem> {
    Ok(match name {
        "offline" => Problem::Offline,
        "noReleaseInfo" => Problem::NoReleaseInfo,
        "notForThisInstall" => Problem::NotForThisInstall,
        "notVerified" => Problem::NotVerified,
        "notAuthorized" => Problem::NotAuthorized,
        "other" => Problem::Other,
        _ => bail!("unknown update problem {name}"),
    })
}

/// The lines `install_update` writes: its progress events, then its
/// answer, unless it "restarts" (the real command never returns then).
fn install_lines(id: &Value) -> anyhow::Result<Vec<Value>> {
    let script = update_script()?;
    let install = script.get("install").cloned().unwrap_or(Value::Null);
    let mut lines: Vec<Value> = install
        .get("events")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|message| json!({ "id": id, "channel": "onEvent", "message": message }))
        .collect();
    if let Some(name) = install.get("problem").and_then(Value::as_str) {
        lines.push(json!({ "id": id, "err": updater::install_message(problem(name)?) }));
    }
    Ok(lines)
}

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

/// A command's answer: its value, or the message it fails with.
fn answer(result: Result<impl serde::Serialize, String>) -> anyhow::Result<Result<Value, String>> {
    Ok(match result {
        Ok(value) => Ok(to_value(value)?),
        Err(message) => Err(message),
    })
}

/// The passage named by `bookId`, `chapter`, `verseStart` and `verseEnd`.
fn passage(args: &Value) -> anyhow::Result<Passage> {
    Ok(Passage {
        book_id: arg(args, "bookId")?,
        chapter: arg(args, "chapter")?,
        verse_start: arg(args, "verseStart")?,
        verse_end: arg(args, "verseEnd")?,
    })
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
        "get_app_version" => Ok(Value::from(commands::app_version())),
        "get_home" => Ok(to_value(commands::home(state).await)?),
        "get_chapters" => Ok(to_value(commands::build_chapters_dto(
            state,
            arg(args, "bookId")?,
            arg(args, "active")?,
        ))?),
        "get_reading" => Ok(to_value(
            commands::reading(state, arg(args, "bookId")?, arg(args, "chapter")?, code).await,
        )?),
        "get_passage" => match commands::passage_text(
            state,
            code.unwrap_or_default(),
            arg(args, "bookId")?,
            arg(args, "chapter")?,
            arg(args, "start")?,
            arg(args, "end")?,
        )
        .await
        {
            Ok(dto) => Ok(to_value(dto)?),
            Err(e) => Err(e),
        },
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
        "list_highlights" => match commands::highlights_list(state).await {
            Ok(list) => Ok(to_value(list)?),
            Err(e) => Err(format!("Highlights couldn't be loaded: {e:#}")),
        },
        "list_highlight_passages" => match commands::highlight_passages_in(state, code).await {
            Ok(list) => Ok(to_value(list)?),
            Err(e) => Err(format!("Highlights couldn't be loaded: {e:#}")),
        },
        "set_highlight" => {
            let color: String = arg(args, "color")?;
            commands::highlight_passage(
                state,
                arg(args, "bookId")?,
                arg(args, "chapter")?,
                arg(args, "verseStart")?,
                arg(args, "verseEnd")?,
                &color,
            )
            .await
            .map(|()| Value::Null)
        }
        "remove_highlight" => commands::unhighlight_passage(
            state,
            arg(args, "bookId")?,
            arg(args, "chapter")?,
            arg(args, "verseStart")?,
            arg(args, "verseEnd")?,
        )
        .await
        .map(Value::from),
        "list_notes" => answer(commands::notes_overlapping(state, passage(args)?).await)?,
        "list_all_notes" => match commands::notes_list(state).await {
            Ok(list) => Ok(to_value(list)?),
            Err(e) => Err(format!("Notes couldn't be loaded: {e:#}")),
        },
        "add_note" => {
            let body: String = arg(args, "body")?;
            answer(commands::note_add(state, passage(args)?, &body).await)?
        }
        "update_note" => {
            let body: String = arg(args, "body")?;
            answer(commands::note_update(state, arg(args, "id")?, &body).await)?
        }
        "delete_note" => answer(commands::note_delete(state, arg(args, "id")?).await)?,
        "restore_note" => {
            let body: String = arg(args, "body")?;
            let created_at: String = arg(args, "createdAt")?;
            let updated_at: String = arg(args, "updatedAt")?;
            answer(
                commands::note_restore(state, passage(args)?, &body, &created_at, &updated_at)
                    .await,
            )?
        }
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
        "get_notes_export_options" => {
            let (now, tz) = clock()?;
            match commands::notes_export_options(state, now, &tz).await {
                Ok(options) => Ok(to_value(options)?),
                Err(message) => Err(message),
            }
        }
        "count_notes_export" => {
            let (_, tz) = clock()?;
            match commands::count_notes_to_export(state, &arg(args, "filter")?, &tz).await {
                Ok(count) => Ok(to_value(count)?),
                Err(message) => Err(message),
            }
        }
        "export_notes" => {
            let code: String = arg(args, "translationCode")?;
            let (now, tz) = clock()?;
            match commands::build_notes_export(state, &code, &arg(args, "filter")?, now, &tz).await
            {
                Err(message) => Err(message),
                Ok(file) => match pick()? {
                    None => Ok(to_value(commands::NotesExportOutcome::Cancelled)?),
                    Some(path) => match commands::save_notes_export(&file, &path).await {
                        Ok(outcome) => Ok(to_value(outcome)?),
                        Err(e) => Err(format!("The file couldn't be saved: {e:#}")),
                    },
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
        // Test helper, not an app command: a verse as the app shows it,
        // from its stored text, so tests can check the page against the
        // app's one rule (`VerseText`) without a copy of it.
        "__verse_text" => {
            let code: String = arg(args, "code")?;
            let text: String = arg(args, "text")?;
            Ok(to_value(VerseText::new(&code, &text))?)
        }
        // Test setup: the notes export's time and time zone from now on
        // (`now`, RFC 3339, and `offsetMinutes` east of UTC).
        "__set_clock" => {
            let now: String = arg(args, "now")?;
            let minutes: i32 = arg(args, "offsetMinutes")?;
            let now = DateTime::parse_from_rfc3339(&now)?.to_utc();
            let tz = FixedOffset::east_opt(minutes * 60).context("bad offset")?;
            *CLOCK
                .lock()
                .map_err(|_| anyhow::anyhow!("clock lock poisoned"))? = Some((now, tz));
            Ok(Value::Null)
        }
        // Test setup: the note(s) with this `body` were written (and last
        // edited) at `createdAt`.
        "__note_written" => {
            let body: String = arg(args, "body")?;
            let at: String = arg(args, "createdAt")?;
            let done =
                sqlx::query("UPDATE notes SET created_at = ?, updated_at = ? WHERE body = ?")
                    .bind(&at)
                    .bind(&at)
                    .bind(&body)
                    .execute(pool)
                    .await?;
            if done.rows_affected() == 0 {
                bail!("no note says {body:?}");
            }
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
        "check_for_update" => {
            let script = update_script()?;
            match script.get("check") {
                None | Some(Value::Null) => Ok(Value::Null),
                Some(check) => match check.get("problem").and_then(Value::as_str) {
                    Some(name) => Err(updater::check_message(problem(name)?).to_string()),
                    None => {
                        let info = UpdateInfo::new(
                            arg(check, "version")?,
                            arg(check, "notes")?,
                            arg(check, "date")?,
                            arg(check, "canSelfUpdate")?,
                        );
                        Ok(to_value(info)?)
                    }
                },
            }
        }
        // Opening a browser is the system's job; the call itself is what
        // the tests check.
        "open_download_page" | "open_release_notes" => Ok(Value::Null),
        // Test setup: how updates behave from now on (see the module docs).
        "__set_update" => {
            *UPDATE
                .lock()
                .map_err(|_| anyhow::anyhow!("update lock poisoned"))? = Some(args.clone());
            Ok(Value::Null)
        }
        "get_compare" => Ok(to_value(
            commands::compare_chapter(state, arg(args, "bookId")?, arg(args, "chapter")?).await?,
        )?),
        // Test setup, not an app command: forget the reading position
        // (and bookmarks, highlights and notes, unless `keepBookmarks` — a restart
        // rather than a new test), then optionally save a reading position (what the
        // app would reopen at).
        "__reset" => {
            *state
                .pending_import
                .lock()
                .map_err(|_| anyhow::anyhow!("pending import lock poisoned"))? = None;
            *PICKER
                .lock()
                .map_err(|_| anyhow::anyhow!("picker lock poisoned"))? = None;
            *UPDATE
                .lock()
                .map_err(|_| anyhow::anyhow!("update lock poisoned"))? = None;
            *CLOCK
                .lock()
                .map_err(|_| anyhow::anyhow!("clock lock poisoned"))? = None;
            if !arg::<Option<bool>>(args, "keepBookmarks")?.unwrap_or(false) {
                sqlx::query("DELETE FROM bookmarks").execute(pool).await?;
                sqlx::query("DELETE FROM highlights").execute(pool).await?;
                sqlx::query("DELETE FROM notes").execute(pool).await?;
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
        if cmd == "install_update" {
            let lines = install_lines(&id).unwrap_or_else(|e| {
                vec![json!({ "id": id, "err": format!("bridge error: {e:#}") })]
            });
            for line in lines {
                stdout.write_all(format!("{line}\n").as_bytes()).await?;
            }
            stdout.flush().await?;
            continue;
        }
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
