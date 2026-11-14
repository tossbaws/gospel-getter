//! Test-only bridge between the jsdom frontend tests (`tests/frontend/`)
//! and the app's own commands: it makes a disposable database at the path
//! given, a copy of this build's template database (see [`template`]),
//! runs the app's startup steps on it (migrate, seed, build the search
//! index; on a copy they find nothing left to do), then answers one JSON
//! request per line on stdin — `{"id": 1, "cmd": "get_reading", "args":
//! {...}}`, with the same camelCase arguments the frontend passes to
//! Tauri's `invoke` — with one JSON line on stdout: `{"id": 1, "ok": ...}`
//! or `{"id": 1, "err": "..."}`. Never shipped; never pointed at a real
//! install's database.
//!
//! Every command is answered by the app itself: the request goes through
//! Tauri's IPC to the app's own handler (`gospel_getter_lib::invoke_handler`,
//! every command in its `generate_handler!`), on Tauri's mock runtime, with
//! the state the app manages, as `lib.rs`'s `ipc_tests` call it. So a
//! command's arguments, answers and messages are the app's, and a command
//! added to the handler is answered here with no change to this file. The
//! bridge answers only its own test hooks, whose names start with `__`,
//! which no command's does (`ipc_tests::the_frontend_bridge_answers_no_command_itself`).
//!
//! The mock runtime has no native file dialog, browser or updater, so the
//! commands that need one ask the bridge's script instead
//! (`commands::BridgeScript`), and go on with their own logic:
//!
//! - A file dialog answers what a test scripted with `__set_picker` (a
//!   path, or `null` for the reader cancelling; a folder, for the notes
//!   export's one file per book); the export and import logic behind it is
//!   the app's own, reading and writing real files.
//! - Nothing opens in a browser; the app's own rules (`web_link`,
//!   `credit_links`) still accept or refuse a link.
//! - Updates are scripted with `__set_update`: what `check_for_update`
//!   answers (no update, an update, a problem, or nothing at all: `{
//!   noAnswer: true }`) and what `install_update` does (the progress events
//!   it sends, then a restart, which never answers, or a problem). The check
//!   runs under the app's own time limit (`updater::within_check_time`,
//!   20 s), which `checkTimeoutMs` shortens, so a check that never answers
//!   is given up on as the app gives it up. Problems answer with the app's
//!   own messages (`updater::check_message` and `install_message`). The
//!   progress goes through the command's own Tauri `Channel`, and is written
//!   as its own lines, `{"id": 1, "channel": "onEvent", "message": ...}`,
//!   before the answer.
//! - The notes export's dates are the computer's, unless a test fixes the
//!   time and time zone with `__set_clock`.
//!
//! The other hooks set up what the app's commands can't: `__reset` starts a
//! test (or a restart) afresh, `__note_written` backdates a note,
//! `__save_findings` saves findings as an agent does, and `__verse_text`
//! shows a verse by the app's own rule.
//!
//! Requests are answered one at a time, in order, as they always were here:
//! the next is read once the last has answered, or, for an install that
//! restarts, once it has restarted (it never answers).
//!
//! `frontend_bridge <database path> --fresh` prepares the database from
//! an empty file instead, seeding it then and there as a new install's is:
//! seconds of work, for a test of the seeding itself (`harness.mjs`'s
//! `useFreshDatabase`).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, mpsc};
use std::time::UNIX_EPOCH;

use anyhow::{Context, bail};
use chrono::{DateTime, FixedOffset, Local, Offset, Utc};
use serde_json::{Value, json};
use tauri::Manager;
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponse, InvokeResponseBody};
use tauri::test::{INVOKE_KEY, MockRuntime, mock_builder, mock_context, noop_assets};
use tauri::webview::InvokeRequest;

use gospel_getter_lib::commands::{AppState, BridgeScript, ScriptedCheck, ScriptedInstall};
use gospel_getter_lib::db::{self, Passage, WritePool};
use gospel_getter_lib::domain::kjv_markup::VerseText;
use gospel_getter_lib::updater::{self, DownloadEvent, Problem, UpdateInfo};

/// Where the page is served from, so a request from it counts as the app's
/// own (as in `lib.rs`'s `ipc_tests`): Windows serves the `tauri://`
/// protocol over http.
const APP_URL: &str = if cfg!(windows) {
    "http://tauri.localhost"
} else {
    "tauri://localhost"
};

/// How a Tauri `Channel` is passed as an argument: this, then its id (the
/// harness's `Channel.toJSON`, as `@tauri-apps/api`'s).
const CHANNEL_PREFIX: &str = "__CHANNEL__:";

/// The scripted answer for the next file dialog: `Some(None)` is a
/// cancel. Taken (and cleared) by the dialog it answers.
static PICKER: Mutex<Option<Option<PathBuf>>> = Mutex::new(None);

/// The scripted update behavior (see `__set_update`); `None` is the
/// default: no update available, and an install that restarts.
static UPDATE: Mutex<Option<UpdateScript>> = Mutex::new(None);

/// A fixed time and time zone for the notes export (see `__set_clock`);
/// `None` is the computer's own.
static CLOCK: Mutex<Option<(DateTime<Utc>, FixedOffset)>> = Mutex::new(None);

/// Each `Channel` passed to a command: the request it came with, and the
/// argument it was (`onEvent`), by its id.
static CHANNELS: Mutex<BTreeMap<u32, (Value, String)>> = Mutex::new(BTreeMap::new());

/// Where the request being answered hears that its command restarted the
/// app, so it never answers (see `Script::restart`).
static RESTARTED: Mutex<Option<mpsc::Sender<Answer>>> = Mutex::new(None);

/// `mutex`, locked, poisoned or not: a test hook that panicked leaves
/// nothing half-changed behind it.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// What `__set_update` scripted.
#[derive(Clone)]
struct UpdateScript {
    check: ScriptedCheck,
    install: ScriptedInstall,
}

impl Default for UpdateScript {
    fn default() -> Self {
        Self {
            check: ScriptedCheck {
                limit: updater::CHECK_TIMEOUT,
                answer: Some(Ok(None)),
            },
            install: ScriptedInstall::default(),
        }
    }
}

impl UpdateScript {
    /// `{ check, checkTimeoutMs, install }`, as `harness.mjs`'s `setUpdate`
    /// documents it.
    fn parse(script: &Value) -> anyhow::Result<Self> {
        let limit = script
            .get("checkTimeoutMs")
            .and_then(Value::as_u64)
            .map_or(updater::CHECK_TIMEOUT, std::time::Duration::from_millis);
        let answer = match script.get("check") {
            None | Some(Value::Null) => Some(Ok(None)),
            Some(check) if check.get("noAnswer") == Some(&Value::Bool(true)) => None,
            Some(check) => Some(match check.get("problem").and_then(Value::as_str) {
                Some(name) => Err(problem(name)?),
                None => Ok(Some(UpdateInfo::new(
                    arg(check, "version")?,
                    arg(check, "notes")?,
                    arg(check, "date")?,
                    arg(check, "canSelfUpdate")?,
                ))),
            }),
        };
        let install = script.get("install").cloned().unwrap_or(Value::Null);
        let events: Option<Vec<DownloadEvent>> = arg(&install, "events")?;
        let problem = match install.get("problem").and_then(Value::as_str) {
            Some(name) => Some(problem(name)?),
            None => None,
        };
        Ok(Self {
            check: ScriptedCheck { limit, answer },
            install: ScriptedInstall {
                events: events.unwrap_or_default(),
                problem,
            },
        })
    }
}

fn problem(name: &str) -> anyhow::Result<Problem> {
    Ok(match name {
        "offline" => Problem::Offline,
        "noReleaseInfo" => Problem::NoReleaseInfo,
        "notForThisInstall" => Problem::NotForThisInstall,
        "notVerified" => Problem::NotVerified,
        "packageNotInstalled" => Problem::PackageNotInstalled,
        "installerDidntStart" => Problem::InstallerDidntStart,
        "other" => Problem::Other,
        _ => bail!("unknown update problem {name}"),
    })
}

/// The bridge's stand-ins for the file dialogs, the browser, the clock
/// and the updater, which the mock runtime doesn't have.
struct Script;

impl BridgeScript for Script {
    fn choose_file(&self) -> Result<Option<PathBuf>, String> {
        lock(&PICKER).take().ok_or_else(|| {
            "a file dialog opened, but the test set no answer for it (__set_picker)".to_string()
        })
    }

    fn clock(&self) -> (DateTime<Utc>, FixedOffset) {
        let fixed = *lock(&CLOCK);
        fixed.unwrap_or_else(|| (Utc::now(), Local::now().offset().fix()))
    }

    /// Opening a browser is the system's job; the call itself is what the
    /// tests check.
    fn open_in_browser(&self, _url: &str) -> std::io::Result<()> {
        Ok(())
    }

    fn check_for_update(&self) -> ScriptedCheck {
        lock(&UPDATE).clone().unwrap_or_default().check
    }

    fn install_update(&self) -> ScriptedInstall {
        lock(&UPDATE).clone().unwrap_or_default().install
    }

    fn restart(&self) {
        if let Some(restarted) = lock(&RESTARTED).take() {
            let _ = restarted.send(Answer::Restarted);
        }
    }
}

/// How a request ended.
enum Answer {
    /// Its command answered.
    Answered(InvokeResponse),
    /// Its command restarted the app, so it never answers.
    Restarted,
}

fn arg<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> anyhow::Result<T> {
    serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null))
        .with_context(|| format!("bad or missing argument {name}"))
}

/// Write `line` to stdout, whole, as one line.
fn write_line(line: &Value) -> anyhow::Result<()> {
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{line}")?;
    stdout.flush()?;
    Ok(())
}

/// Write a message a command sent through `channel` as the `channel` line
/// of the request it was passed with; `false` (not ours) for any other.
fn channel_line(channel: CallbackFn, body: &InvokeResponseBody) -> bool {
    let Some((id, name)) = lock(&CHANNELS).get(&channel.0).cloned() else {
        return false;
    };
    let message = match body {
        InvokeResponseBody::Json(json) => {
            serde_json::from_str(json).unwrap_or_else(|_| Value::String(json.clone()))
        }
        InvokeResponseBody::Raw(bytes) => json!(bytes),
    };
    if let Err(e) = write_line(&json!({ "id": id, "channel": name, "message": message })) {
        eprintln!("couldn't write a channel message: {e:#}");
    }
    true
}

/// Note the `Channel`s among `args`, so their messages are written as
/// request `id`'s.
fn note_channels(id: &Value, args: &Value) {
    let Some(args) = args.as_object() else {
        return;
    };
    let mut channels = lock(&CHANNELS);
    for (name, value) in args {
        let channel = value
            .as_str()
            .and_then(|v| v.strip_prefix(CHANNEL_PREFIX))
            .and_then(|n| n.parse::<u32>().ok());
        if let Some(channel) = channel {
            channels.insert(channel, (id.clone(), name.clone()));
        }
    }
}

/// Answer `cmd` through the app's own IPC handler, as Tauri does for the
/// page's `invoke` (`tauri::test::get_ipc_response`, with a responder that
/// doesn't block, so a command that never answers doesn't stop the bridge).
/// `None` if its command restarted the app.
fn invoke(
    webview: &tauri::WebviewWindow<MockRuntime>,
    id: &Value,
    cmd: &str,
    args: Value,
) -> anyhow::Result<Option<Result<Value, Value>>> {
    note_channels(id, &args);
    let (answered, answer) = mpsc::channel();
    *lock(&RESTARTED) = Some(answered.clone());
    let request = InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: APP_URL.parse().context("the app's URL")?,
        body: InvokeBody::Json(args),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    };
    webview.as_ref().clone().on_message(
        request,
        Box::new(move |_, _, response, _, _| {
            let _ = answered.send(Answer::Answered(response));
        }),
    );
    let answer = answer.recv().context("the command dropped its answer")?;
    *lock(&RESTARTED) = None;
    Ok(match answer {
        Answer::Restarted => None,
        Answer::Answered(InvokeResponse::Ok(body)) => Some(Ok(body
            .deserialize::<Value>()
            .context("the command's answer isn't JSON")?)),
        Answer::Answered(InvokeResponse::Err(error)) => Some(Err(error.0)),
    })
}

/// One of the bridge's own test hooks (`__…`), which aren't app commands.
async fn hook(
    state: &AppState,
    writer: &WritePool,
    cmd: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    Ok(match cmd {
        // An agent saving findings, as `gospel-getter-cli`'s save_findings
        // does (the app itself never adds one): `{ request, agent,
        // references: [{ bookId, chapter, verse }] }`, the reader's switch
        // aside.
        "__save_findings" => {
            let request: String = arg(args, "request")?;
            let agent: String = arg(args, "agent")?;
            let verses: Vec<Value> = arg(args, "references")?;
            let mut findings = Vec::with_capacity(verses.len());
            for v in &verses {
                let verse: i64 = arg(v, "verse")?;
                findings.push(db::NewFinding {
                    passage: Passage {
                        book_id: arg(v, "bookId")?,
                        chapter: arg(v, "chapter")?,
                        verse_start: verse,
                        verse_end: verse,
                    },
                    agent: agent.clone(),
                    request: request.clone(),
                    created_at: arg(v, "createdAt")?,
                });
            }
            let saved = state.store.add_findings(&findings).await?;
            Value::from(saved.saved.len())
        }
        // A verse as the app shows it, from its stored text, so tests can
        // check the page against the app's one rule (`VerseText`) without a
        // copy of it.
        "__verse_text" => {
            let code: String = arg(args, "code")?;
            let text: String = arg(args, "text")?;
            serde_json::to_value(VerseText::new(&code, &text))?
        }
        // The notes export's time and time zone from now on (`now`, RFC
        // 3339, and `offsetMinutes` east of UTC).
        "__set_clock" => {
            let now: String = arg(args, "now")?;
            let minutes: i32 = arg(args, "offsetMinutes")?;
            let now = DateTime::parse_from_rfc3339(&now)?.to_utc();
            let tz = FixedOffset::east_opt(minutes * 60).context("bad offset")?;
            *lock(&CLOCK) = Some((now, tz));
            Value::Null
        }
        // The note(s) with this `body` were written (and last edited) at
        // `createdAt`.
        "__note_written" => {
            let body: String = arg(args, "body")?;
            let at: String = arg(args, "createdAt")?;
            let done =
                sqlx::query("UPDATE notes SET created_at = ?, updated_at = ? WHERE body = ?")
                    .bind(&at)
                    .bind(&at)
                    .bind(&body)
                    .execute(writer.pool())
                    .await?;
            if done.rows_affected() == 0 {
                bail!("no note says {body:?}");
            }
            Value::Null
        }
        // The answer the next file dialog gives.
        "__set_picker" => {
            let path: Option<PathBuf> = arg(args, "path")?;
            *lock(&PICKER) = Some(path);
            Value::Null
        }
        // How updates behave from now on (see the module docs).
        "__set_update" => {
            *lock(&UPDATE) = Some(UpdateScript::parse(args)?);
            Value::Null
        }
        // Forget the reading position and the place in it (and bookmarks,
        // highlights and notes, unless `keepBookmarks` — a restart rather
        // than a new test), then optionally save a reading position (what
        // the app would reopen at). With `keepPosition`, a restart: the
        // position and place stay as saved.
        "__reset" => {
            *state
                .pending_import
                .lock()
                .map_err(|_| anyhow::anyhow!("pending import lock poisoned"))? = None;
            *lock(&PICKER) = None;
            *lock(&UPDATE) = None;
            *lock(&CLOCK) = None;
            if !arg::<Option<bool>>(args, "keepBookmarks")?.unwrap_or(false) {
                for delete in [
                    "DELETE FROM bookmarks",
                    "DELETE FROM highlights",
                    "DELETE FROM notes",
                    "DELETE FROM ai_findings",
                    "DELETE FROM app_settings",
                ] {
                    sqlx::query(delete).execute(writer.pool()).await?;
                }
            }
            // A restart (`keepPosition`) opens where the last session left.
            if arg::<Option<bool>>(args, "keepPosition")?.unwrap_or(false) {
                return Ok(Value::Null);
            }
            for delete in ["DELETE FROM reading_position", "DELETE FROM reading_place"] {
                sqlx::query(delete).execute(writer.pool()).await?;
            }
            if let Some(book_id) = arg::<Option<i64>>(args, "bookId")? {
                let code: Option<String> = arg(args, "translationCode")?;
                let translation_id = state
                    .translations
                    .iter()
                    .find(|t| Some(&t.code) == code.as_ref())
                    .map_or(1, |t| t.id);
                state
                    .store
                    .save_reading_position(translation_id, book_id, arg(args, "chapter")?, None)
                    .await?;
            }
            Value::Null
        }
        _ => bail!("unknown test hook {cmd}"),
    })
}

// ---- The template database

/// This bridge build's template database: a database prepared and indexed
/// as the app does at startup, written whole with `VACUUM INTO`, which each
/// bridge copies for its own. Preparing one (seeding three Bibles and the
/// cross-references, then building the search index) takes seconds, where
/// a copy takes milliseconds, and every test file starts a bridge.
///
/// It's built once per bridge build, by the first bridge to start, and kept
/// beside the executable (in cargo's target folder, which `cargo clean`
/// clears, rather than the OS temp folder, often a small one in memory),
/// named for the executable's size and modification time. The seed data
/// and every migration are compiled into the executable, so a rebuild that
/// could change what `prepare` makes is a new executable, whose template
/// has a new name: a stale one is never found, and the new build's first
/// bridge sweeps it away.
///
/// Test files start at once, each with its own bridge. One builds while
/// the others wait on `build.lock` (the OS releases it however the
/// builder ends), then finds the template there. The build writes to a
/// file of its own, and a rename publishes it: nothing at the template's
/// name is ever a part-written file. The rename is onto no file (the
/// builder found none, holding the lock), so it behaves the same on
/// Windows, where a rename onto a file someone has open fails.
async fn template() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe().context("Failed to find the bridge's own executable")?;
    let built = std::fs::metadata(&exe)
        .with_context(|| format!("Failed to read {}'s metadata", exe.display()))?;
    let modified = built
        .modified()
        .context("Failed to read the bridge executable's modification time")?
        .duration_since(UNIX_EPOCH)
        .context("The bridge executable's modification time is before 1970")?
        .as_nanos();
    let name = format!("{}-{modified}.db", built.len());
    let dir = exe.with_file_name("frontend_bridge_templates");
    let template = dir.join(&name);
    // Published whole and never changed, so it can be copied without the
    // lock.
    if template.is_file() {
        return Ok(template);
    }

    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create the templates folder {}", dir.display()))?;
    // Opened, never written: not truncated, which on Windows fails while
    // another bridge holds the lock.
    let lock_path = dir.join("build.lock");
    let build_lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("Failed to open {}", lock_path.display()))?;
    build_lock
        .lock()
        .with_context(|| format!("Failed to lock {}", lock_path.display()))?;
    // Another bridge built it while this one waited.
    if template.is_file() {
        return Ok(template);
    }
    sweep(&dir, &name);
    // On stderr, which the harness shows when a bridge fails.
    eprintln!("frontend_bridge: building {}", template.display());
    build_template(&dir, &template)
        .await
        .context("Failed to build the template database")?;
    Ok(template)
}

/// Prepare and index a database in `dir` as the app does at startup, and
/// publish a whole copy of it at `template`. Called holding `build.lock`,
/// with nothing at `template`.
async fn build_template(dir: &Path, template: &Path) -> anyhow::Result<()> {
    let pid = std::process::id();
    let work = dir.join(format!("{pid}.work.db"));
    let whole = dir.join(format!("{pid}.whole.db"));
    let pool = db::create_pool(&work).await?;
    db::prepare(pool.write()).await?;
    db::ensure_search_index(&pool).await?;
    // The work database's last writes are still in its write-ahead log, so
    // a copy of its file alone would be part of a database.
    db::test_support::write_whole_copy(&pool, &whole).await?;
    pool.close().await;
    remove_database(&work);
    match std::fs::rename(&whole, template) {
        Ok(()) => Ok(()),
        // Published by another bridge after all, which only a file system
        // without working locks allows: the same database, so it's used.
        Err(_) if template.is_file() => {
            let _ = std::fs::remove_file(&whole);
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&whole);
            Err(e).with_context(|| format!("Failed to publish {}", template.display()))
        }
    }
}

/// Remove everything in the templates folder `dir` but the lock and the
/// template `keep`: earlier builds' templates, and what a build that ended
/// part-way left. Called holding `build.lock`, so no build is under way. A
/// file that can't be removed (on Windows, one a bridge of an earlier build
/// is still copying) is left for the next sweep.
fn sweep(dir: &Path, keep: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name != "build.lock" && name != keep {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Remove a database file and the files SQLite keeps beside it.
fn remove_database(path: &Path) {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut file = path.as_os_str().to_owned();
        file.push(suffix);
        let _ = std::fs::remove_file(file);
    }
}

/// Copy `template` to `path`, which must not exist yet: the bridge makes
/// its database only where there's none to overwrite.
fn copy_new(template: &Path, path: &Path) -> anyhow::Result<()> {
    let mut from =
        File::open(template).with_context(|| format!("Failed to open {}", template.display()))?;
    let mut to = File::options()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("Failed to create a new database at {}", path.display()))?;
    std::io::copy(&mut from, &mut to).with_context(|| {
        format!(
            "Failed to copy {} to {}",
            template.display(),
            path.display()
        )
    })?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    // A panic anywhere, a command's on Tauri's runtime too, ends the bridge
    // (as it did when the bridge ran the commands itself), so the tests see
    // it, with its message on stderr, rather than wait for an answer that
    // never comes.
    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        report(panic);
        std::process::exit(101);
    }));

    const USAGE: &str = "usage: frontend_bridge <database path> [--fresh]";
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(args.next().context(USAGE)?);
    let fresh = match args.next() {
        None => false,
        Some(flag) if flag == "--fresh" => true,
        Some(other) => bail!("{USAGE}, not {}", other.display()),
    };
    if let Some(extra) = args.next() {
        bail!("{USAGE}, with nothing after it: {}", extra.display());
    }
    let (pool, state) = tauri::async_runtime::block_on(async {
        if !fresh {
            let template = template().await?;
            copy_new(&template, &path)?;
        }
        // The app's own startup steps, as at every launch. On a copy of the
        // template they find everything made, seeded and indexed, and take
        // a fraction of a second.
        let pool = db::create_pool(&path).await?;
        db::prepare(pool.write()).await?;
        db::ensure_search_index(&pool).await?;
        let state = AppState::load(pool.clone()).await?;
        state.search_index.set_ready();
        anyhow::Ok((pool, state))
    })?;

    // The app's own handler and state, on Tauri's mock runtime, as
    // `lib.rs`'s `ipc_tests` build it; the bridge's script stands in for
    // what the mock runtime lacks.
    let app = mock_builder()
        .manage(state.with_bridge_script(Script))
        .manage(updater::PendingUpdate::default())
        .invoke_handler(gospel_getter_lib::invoke_handler())
        .channel_interceptor(|_, channel, _, body| channel_line(channel, body))
        .build(mock_context(noop_assets()))
        .context("Failed to build the mock app")?;
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .context("Failed to build the mock webview")?;
    let state = app.state::<AppState>();

    write_line(&json!({ "ready": true }))?;
    for line in std::io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line?).context("bad request line")?;
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let cmd = request.get("cmd").and_then(Value::as_str).unwrap_or("");
        let args = request.get("args").cloned().unwrap_or_else(|| json!({}));
        let response = if cmd.starts_with("__") {
            match tauri::async_runtime::block_on(hook(&state, pool.write(), cmd, &args)) {
                Ok(value) => Some(Ok(value)),
                Err(e) => Some(Err(json!(format!("bridge error: {e:#}")))),
            }
        } else {
            invoke(&webview, &id, cmd, args)?
        };
        match response {
            Some(Ok(value)) => write_line(&json!({ "id": id, "ok": value }))?,
            Some(Err(message)) => write_line(&json!({ "id": id, "err": message }))?,
            // Restarted: it never answers, as in the app.
            None => {}
        }
    }
    Ok(())
}
