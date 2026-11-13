//! Gospel Getter: an offline Bible reader. The Tauri app itself is
//! `run`; the modules are public so the frontend test bridge
//! (`examples/frontend_bridge.rs`) can drive the same command logic.

pub mod commands;
pub mod credit_links;
pub mod log_file;
pub mod markdown;
pub mod notes_export;
pub mod passage_copy;
// The saved settings' one table, shared with the page's Rust pieces
// (ui-leptos), which compile the same file.
#[path = "../../shared/preferences.rs"]
pub mod preferences;
pub mod reader_data;
#[cfg(test)]
mod reader_data_tests;
pub mod release_notes;
pub mod retry;
pub mod start_here;
pub mod support_link;
pub mod updater;
pub mod web_link;
pub mod window;

// The database and the domain logic, in a crate of their own, free of Tauri,
// so that `gospel-getter-cli` can use them without linking the webview.
pub use gospel_getter_core::{book_aliases, db, domain, passage_reference, verse_parts};

use anyhow::Context;
use tauri::Manager;
use tracing::info;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use commands::AppState;
use db::create_pool;
use log_file::LogFile;

/// Start the app: set up logging, open the database and show the window
/// where it was last time.
pub fn run() {
    // What `RUST_LOG` asks for, or `log_file::DEFAULT_LOG_FILTER`, to stderr
    // and, once `setup_app` has the app's log folder, to the log files.
    let log = LogFile::new(log_file::MAX_LOG_BYTES, log_file::LOG_FILES);
    tracing_subscriber::registry()
        .with(log_file::log_filter(
            std::env::var("RUST_LOG").ok().as_deref(),
        ))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_span_events(FmtSpan::CLOSE),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(log.clone())
                .with_ansi(false)
                .with_span_events(FmtSpan::CLOSE),
        )
        .init();

    tauri::Builder::default()
        // First, before anything else starts: a second launch hands over to
        // the app already open and exits (see `window::single_instance_plugin`).
        .plugin(window::single_instance_plugin())
        // Native open/save dialogs, used only from Rust (see
        // `commands::export_reader_data`); the webview gets no dialog or
        // file-system permission.
        .plugin(tauri_plugin_dialog::init())
        // Updates, likewise used only from Rust (see `updater`). The
        // releases page and the credit links open through the `open` crate,
        // also only from Rust, so nothing takes over link clicks in the page.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updater::PendingUpdate::default())
        .setup(move |app| {
            open_log_file(app, &log);
            // A failure is logged whole, with every cause under it, and told
            // in a native message, which quits the app with code 1 when it's
            // closed. Handed to Tauri, it would panic with its outer words
            // alone, and log nothing.
            if let Err(e) = setup_app(app) {
                tracing::error!("Gospel Getter couldn't start: {e:#}");
                tell_start_failed(app, log.path().as_deref(), &e);
            }
            Ok(())
        })
        .invoke_handler(invoke_handler())
        .run(app_context())
        .expect("error while running Gospel Getter");
}

/// The app's Tauri context: `tauri.conf.json`, compiled in, as it is. Only
/// the GUI checks' test build (feature `gui-test-updater`) changes it, to
/// let its updater reach their local server (see `updater::test_hook`).
pub fn app_context() -> tauri::Context<tauri::Wry> {
    #[cfg_attr(not(feature = "gui-test-updater"), allow(unused_mut))]
    let mut context = tauri::generate_context!();
    #[cfg(feature = "gui-test-updater")]
    updater::test_hook::prepare_context(&mut context);
    context
}

/// Every command the frontend can `invoke`.
fn invoke_handler<R: tauri::Runtime>()
-> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        commands::get_app_version,
        commands::get_home,
        commands::get_chapters,
        commands::get_reading,
        commands::save_reading_position,
        commands::get_passage,
        commands::get_copy_text,
        commands::list_bookmarks,
        commands::add_bookmark,
        commands::remove_bookmark,
        commands::list_highlights,
        commands::list_highlight_passages,
        commands::set_highlight,
        commands::remove_highlight,
        commands::list_notes,
        commands::list_all_notes,
        commands::get_note_markdown,
        commands::add_note,
        commands::update_note,
        commands::delete_note,
        commands::restore_note,
        commands::list_findings,
        commands::list_finding_requests,
        commands::remove_finding,
        commands::remove_finding_request,
        commands::restore_findings,
        commands::keep_finding_as_note,
        commands::get_ai_findings_enabled,
        commands::set_ai_findings_enabled,
        commands::search,
        commands::get_compare,
        commands::export_reader_data,
        commands::get_notes_export_options,
        commands::count_notes_export,
        commands::export_notes,
        commands::export_notes_by_book,
        commands::confirm_notes_export_by_book,
        commands::get_notes_commentary,
        commands::choose_import_file,
        commands::apply_import,
        commands::cancel_import,
        updater::check_for_update,
        updater::install_update,
        updater::open_download_page,
        updater::open_release_notes,
        credit_links::open_credit_link,
        support_link::open_support_page,
        web_link::open_web_link,
    ]
}

/// Keep the log in the app's log folder from now on, as well as on stderr.
/// Without it the app still runs, logging to stderr alone.
fn open_log_file(app: &tauri::App, log: &LogFile) {
    match app.path().app_log_dir() {
        Ok(dir) => match log.open_in(&dir) {
            Ok(()) => info!("Logging to {}", dir.display()),
            Err(e) => tracing::warn!("Failed to keep a log file in {}: {e:#}", dir.display()),
        },
        Err(e) => tracing::warn!("Failed to find the app's log folder: {e}"),
    }
}

/// Resolve where this app's database lives, migrate the pre-Tauri
/// (systemd/browser-wrapper) install's database into place on first run if
/// there is one, connect, and load everything into `AppState` — all before
/// the window is created, so there's nothing left to "load" once it
/// appears. Then create the window, where it was last time.
fn setup_app(app: &mut tauri::App) -> anyhow::Result<()> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .context("Failed to resolve the app data directory")?;
    db::create_private_dir(&app_data_dir).context("Failed to create the app data directory")?;
    let db_path = app_data_dir.join(db::DATABASE_FILE);
    info!("Using database at {}", db_path.display());

    // If we can't resolve the home directory, or the legacy database isn't
    // there, `migrate_legacy_db_if_needed` is a no-op and we just proceed to
    // create a fresh database, same as any other first run. If it's there
    // and can't be brought over, the app doesn't start: a fresh database
    // in its place would be taken for the migrated one on every later
    // launch, and the reader's place left behind for good. Nothing was
    // changed, so the next launch simply tries again.
    if let Ok(home_dir) = app.path().home_dir() {
        let legacy_db_path = home_dir.join(".local/share/gospel-getter/data/gospel_getter.db");
        let migrated = tauri::async_runtime::block_on(db::migrate_legacy_db_if_needed(
            &db_path,
            &legacy_db_path,
        ));
        match migrated {
            Ok(true) => info!(
                "Migrated legacy database from {} to {}",
                legacy_db_path.display(),
                db_path.display()
            ),
            Ok(false) => {}
            Err(e) => {
                tracing::error!(
                    "Failed to migrate legacy database from {}: {e:#}. It was left as it \
                     was and no database was created at {}, so the next launch tries again.",
                    legacy_db_path.display(),
                    db_path.display()
                );
                tell_migration_failed(app, &legacy_db_path, &e);
                return Ok(());
            }
        }
    }

    let state = tauri::async_runtime::block_on(init_state(&db_path))
        .context("Failed to initialize application state")?;
    spawn_search_index_build(&state);
    app.manage(state);

    // Desktop only (as the plugin is). Registered here, not on the
    // builder, as its file goes in the data directory resolved above.
    #[cfg(desktop)]
    {
        app.handle()
            .plugin(window::state_plugin(&app_data_dir)?)
            .context("Failed to set up the window state")?;
        window::create_main_window(app)?;
    }

    Ok(())
}

/// The title of the native message that says the app couldn't start.
const START_FAILED_TITLE: &str = "Gospel Getter couldn't start";

/// What the reader is told when the earlier Gospel Getter's database
/// couldn't be brought over, so the app hasn't started: there's no window
/// yet, so it's a native message, and the app quits when it's closed.
fn tell_migration_failed(app: &tauri::App, legacy_db_path: &std::path::Path, e: &anyhow::Error) {
    tell_and_quit(app, migration_failed_message(legacy_db_path, e));
}

/// What the reader is told when anything else stopped the app starting
/// (`setup_app` failed): a native message, as for a failed migration, and
/// the app quits with code 1 when it's closed.
fn tell_start_failed(app: &tauri::App, log: Option<&std::path::Path>, e: &anyhow::Error) {
    tell_and_quit(app, start_failed_message(log, e));
}

/// Show `message` as a native error, titled [`START_FAILED_TITLE`] (there's
/// no window), and quit with code 1 once it's closed.
fn tell_and_quit(app: &tauri::App, message: String) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
    let handle = app.handle().clone();
    app.dialog()
        .message(message)
        .title(START_FAILED_TITLE)
        .kind(MessageDialogKind::Error)
        .show(move |_| handle.exit(1));
}

/// The words of [`tell_start_failed`]: that it couldn't start, what went
/// wrong (the error and every cause under it, which name only the app's own
/// steps and files, never anything of the reader's), and where the log is
/// (`log`, if one could be kept).
fn start_failed_message(log: Option<&std::path::Path>, e: &anyhow::Error) -> String {
    let log = match log {
        Some(path) => format!("The log has this too: {}", path.display()),
        None => "No log file could be kept, so this message is all there is to go on.".to_string(),
    };
    format!("Gospel Getter couldn't start.\n\nWhat went wrong: {e:#}\n\n{log}")
}

/// The words of [`tell_migration_failed`].
fn migration_failed_message(legacy_db_path: &std::path::Path, e: &anyhow::Error) -> String {
    format!(
        "Gospel Getter couldn't bring over the database of the earlier Gospel Getter, so it \
         hasn't started: starting without it would leave your reading place behind. Nothing \
         was changed.\n\nThat database is {}. If the earlier Gospel Getter is open, close it, \
         then open Gospel Getter again to try once more. To start without it instead, move that \
         file somewhere else first.\n\nWhat went wrong: {e:#}",
        legacy_db_path.display()
    )
}

/// Connect to the database, run migrations/seeding, and load the books and
/// translations that stay in memory for the life of the app.
async fn init_state(db_path: &std::path::Path) -> anyhow::Result<AppState> {
    let database = create_pool(db_path)
        .await
        .context("Failed to connect to database")?;

    db::prepare(database.write()).await?;

    let state = AppState::load(database).await?;
    info!("Loaded {} books", state.books.len());
    info!(
        "Loaded {} translations: {}",
        state.translations.len(),
        state
            .translations
            .iter()
            .map(|t| t.code.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(state)
}

/// The pauses before the search index build is tried again, if it fails
/// (each time it picks up where it stopped: finished translations stay
/// done). Only the last try's failure leaves search unavailable.
const SEARCH_INDEX_RETRIES: [std::time::Duration; 3] = [
    std::time::Duration::from_secs(1),
    std::time::Duration::from_secs(2),
    std::time::Duration::from_secs(4),
];

/// Build whatever part of the search index is missing — all of it on the
/// first launch after installing or upgrading, nothing on later launches —
/// in the background, so the window doesn't wait on it. Searching reports
/// that it's still being prepared until this finishes, tries again
/// included.
fn spawn_search_index_build(state: &AppState) {
    let store = state.store.clone();
    let status = state.search_index.clone();
    tauri::async_runtime::spawn(async move {
        let started = std::time::Instant::now();
        let built = retry::with_retries("Building the search index", &SEARCH_INDEX_RETRIES, || {
            store.ensure_search_index()
        })
        .await;
        match built {
            Ok(indexed) => {
                if !indexed.is_empty() {
                    info!(
                        "Built search index for translation ids {indexed:?} in {:?}",
                        started.elapsed()
                    );
                }
                status.set_ready();
            }
            Err(e) => {
                tracing::error!("Failed to build search index: {e:#}");
                status.set_failed();
            }
        }
    });
}

#[cfg(test)]
mod migration_tests {
    /// The reader is told where the earlier database is, that nothing was
    /// changed, how to try again or start without it, and what went wrong.
    #[test]
    fn a_failed_migration_tells_the_reader_where_the_file_is_and_what_to_do() {
        let path = std::path::Path::new("/home/reader/.local/share/gospel-getter/data/x.db");
        let e = anyhow::anyhow!("disk I/O error").context("Failed to snapshot it");
        let message = super::migration_failed_message(path, &e);
        for part in [
            "/home/reader/.local/share/gospel-getter/data/x.db",
            "Nothing was changed.",
            "open Gospel Getter again to try once more",
            "move that file somewhere else first",
            "What went wrong: Failed to snapshot it: disk I/O error",
        ] {
            assert!(message.contains(part), "{part:?} in {message:?}");
        }
    }

    /// Any other failure to start says so in plain words, with what went
    /// wrong down to its root cause, and where the log is; nothing else, so
    /// nothing of the reader's (only the error's own words, which name the
    /// app's steps and files).
    #[test]
    fn a_failed_start_says_what_went_wrong_and_where_the_log_is() {
        let root = std::io::Error::other("(code: 14) unable to open database file");
        let e = anyhow::Error::new(root)
            .context("Failed to connect to database at /home/reader/data/gospel_getter.db")
            .context("Failed to initialize application state");
        let log = std::path::Path::new("/home/reader/data/logs/gospel-getter.log");
        assert_eq!(
            super::start_failed_message(Some(log), &e),
            "Gospel Getter couldn't start.\n\nWhat went wrong: Failed to initialize application \
             state: Failed to connect to database at /home/reader/data/gospel_getter.db: (code: \
             14) unable to open database file\n\nThe log has this too: \
             /home/reader/data/logs/gospel-getter.log"
        );
        let unlogged = super::start_failed_message(None, &e);
        assert!(
            unlogged.contains("unable to open database file"),
            "{unlogged}"
        );
        assert!(
            unlogged
                .ends_with("No log file could be kept, so this message is all there is to go on."),
            "{unlogged}"
        );
    }
}

#[cfg(test)]
mod ipc_tests {
    //! The commands, called through Tauri's own IPC layer (on its mock
    //! runtime) with exactly the argument objects `ui/index.html` passes
    //! to `invoke` — so a renamed argument on either side fails here.

    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use chrono::{DateTime, FixedOffset, Utc};
    use serde_json::{Value, json};
    use tauri::Manager;
    use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder, mock_context, noop_assets};
    use tauri::webview::InvokeRequest;

    use crate::commands::{AppState, BridgeScript, ScriptedCheck, ScriptedInstall};
    use crate::db::test_support::{bundled, fresh_database};
    use crate::updater::{DownloadEvent, Problem, UpdateInfo};

    /// Where the page is served from, so a request from it counts as the
    /// app's own: Windows serves the `tauri://` protocol over http.
    const APP_URL: &str = if cfg!(windows) {
        "http://tauri.localhost"
    } else {
        "tauri://localhost"
    };

    fn invoke(
        webview: &tauri::WebviewWindow<tauri::test::MockRuntime>,
        cmd: &str,
        args: Value,
    ) -> Result<Value, Value> {
        get_ipc_response(
            webview,
            InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: APP_URL.parse().expect("url"),
                body: InvokeBody::Json(args),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .map(|body| body.deserialize::<Value>().expect("JSON response"))
    }

    #[test]
    fn frontend_invoke_arguments_reach_every_command() {
        let (db, state) = tauri::async_runtime::block_on(async {
            let (db, pool) = fresh_database("ipc").await;
            let state = AppState::load(pool).await.expect("load state");
            state.search_index.set_ready();
            (db, state)
        });
        let app = mock_builder()
            .manage(state)
            .invoke_handler(super::invoke_handler())
            .build(mock_context(noop_assets()))
            .expect("build mock app");
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("build webview");

        assert_eq!(
            invoke(&webview, "get_app_version", json!({})).unwrap(),
            env!("CARGO_PKG_VERSION")
        );

        let home = invoke(&webview, "get_home", json!({})).unwrap();
        assert_eq!(home["currentTranslationCode"], "kjv");
        // A fresh database: nothing has been read yet.
        assert_eq!(home["hasSavedPosition"], false);

        let chapters = invoke(
            &webview,
            "get_chapters",
            json!({ "bookId": 43, "active": 3 }),
        )
        .unwrap();
        assert_eq!(chapters["chapters"][2]["isActive"], true);

        let reading = invoke(
            &webview,
            "get_reading",
            json!({ "bookId": 43, "chapter": 3, "translationCode": "web" }),
        )
        .unwrap();
        assert_eq!(
            reading["currentVerses"][15]["text"],
            bundled("web")[42].chapters[2][15].as_str()
        );
        // Reading a chapter saves nothing; the page saves the one it shows,
        // so the next launch isn't a first run and opens there.
        let home = invoke(&webview, "get_home", json!({})).unwrap();
        assert_eq!(home["hasSavedPosition"], false);
        assert_eq!(
            invoke(
                &webview,
                "save_reading_position",
                json!({ "bookId": 43, "chapter": 3, "translationCode": "web" }),
            )
            .unwrap(),
            Value::Null
        );
        let home = invoke(&webview, "get_home", json!({})).unwrap();
        assert_eq!(home["hasSavedPosition"], true);
        assert_eq!(
            (
                &home["currentTranslationCode"],
                &home["currentBookId"],
                &home["currentChapter"]
            ),
            (&json!("web"), &json!(43), &json!(3))
        );
        assert_eq!(
            (&home["currentVerse"], &home["currentVerseOffset"]),
            (&Value::Null, &Value::Null)
        );
        // Where in the chapter the reader was comes back with it.
        assert_eq!(
            invoke(
                &webview,
                "save_reading_position",
                json!({ "bookId": 43, "chapter": 3, "translationCode": "web", "verse": 16, "offset": -12.5 }),
            )
            .unwrap(),
            Value::Null
        );
        let home = invoke(&webview, "get_home", json!({})).unwrap();
        assert_eq!(
            (&home["currentVerse"], &home["currentVerseOffset"]),
            (&json!(16), &json!(-12.5))
        );
        assert_eq!(
            invoke(
                &webview,
                "save_reading_position",
                json!({ "bookId": 43, "chapter": 22, "translationCode": "web" }),
            ),
            Err(json!("John has no chapter 22."))
        );
        assert_eq!(
            invoke(
                &webview,
                "save_reading_position",
                json!({ "bookId": 43, "chapter": 3, "translationCode": "web", "verse": 37 }),
            ),
            Err(json!("John 3 has no verse 37."))
        );

        let passage = invoke(
            &webview,
            "get_passage",
            json!({ "translationCode": "kjv", "bookId": 45, "chapter": 5, "start": 8, "end": 8 }),
        )
        .unwrap();
        assert_eq!(passage["verses"][0]["number"], 8);
        assert_eq!(passage["reference"], "Romans 5:8");

        let copied = invoke(
            &webview,
            "get_copy_text",
            json!({ "translationCode": "web", "bookId": 43, "chapter": 3, "start": 16, "end": 17 }),
        )
        .unwrap();
        assert_eq!(copied["reference"], "John 3:16\u{2013}17");
        assert!(
            copied["formats"]["numbered"]
                .as_str()
                .unwrap()
                .starts_with("16 ")
        );
        let chapter = invoke(
            &webview,
            "get_copy_text",
            json!({ "translationCode": "kjv", "bookId": 19, "chapter": 23 }),
        )
        .unwrap();
        assert_eq!(chapter["reference"], "Psalms 23");

        let id = invoke(
            &webview,
            "add_bookmark",
            json!({ "bookId": 43, "chapter": 3, "verseStart": 16, "verseEnd": 18 }),
        )
        .unwrap();
        let list = invoke(
            &webview,
            "list_bookmarks",
            json!({ "translationCode": "kjv" }),
        )
        .unwrap();
        assert_eq!(list[0]["id"], id);
        assert_eq!(list[0]["reference"], "John 3:16\u{2013}18");
        assert_eq!(
            invoke(&webview, "remove_bookmark", json!({ "id": id })).unwrap(),
            true
        );
        // A command error reaches the frontend as the rejection value.
        assert_eq!(
            invoke(
                &webview,
                "add_bookmark",
                json!({ "bookId": 43, "chapter": 22, "verseStart": 1, "verseEnd": 1 }),
            ),
            Err(json!("There's no John 22."))
        );

        // Highlights: set over a range, recolor, list, remove.
        assert_eq!(
            invoke(
                &webview,
                "set_highlight",
                json!({ "bookId": 43, "chapter": 3, "verseStart": 16, "verseEnd": 17, "color": "yellow" }),
            )
            .unwrap(),
            Value::Null
        );
        invoke(
            &webview,
            "set_highlight",
            json!({ "bookId": 43, "chapter": 3, "verseStart": 17, "verseEnd": 17, "color": "pink" }),
        )
        .unwrap();
        assert_eq!(
            invoke(&webview, "list_highlights", json!({})).unwrap(),
            json!([
                { "bookId": 43, "chapter": 3, "verse": 16, "color": "yellow" },
                { "bookId": 43, "chapter": 3, "verse": 17, "color": "pink" },
            ])
        );
        let passages = invoke(
            &webview,
            "list_highlight_passages",
            json!({ "translationCode": "kjv" }),
        )
        .unwrap();
        assert_eq!(
            (&passages[0]["reference"], &passages[0]["color"]),
            (&json!("John 3:16"), &json!("yellow"))
        );
        assert_eq!(
            (&passages[1]["reference"], &passages[1]["color"]),
            (&json!("John 3:17"), &json!("pink"))
        );
        assert_eq!(
            invoke(
                &webview,
                "set_highlight",
                json!({ "bookId": 43, "chapter": 3, "verseStart": 16, "verseEnd": 16, "color": "purple" }),
            ),
            Err(json!(
                "\u{201c}purple\u{201d} isn't a highlight color (yellow, green, blue or pink)."
            ))
        );
        assert_eq!(
            invoke(
                &webview,
                "set_highlight",
                json!({ "bookId": 43, "chapter": 22, "verseStart": 1, "verseEnd": 1, "color": "blue" }),
            ),
            Err(json!("There's no John 22."))
        );
        assert_eq!(
            invoke(
                &webview,
                "remove_highlight",
                json!({ "bookId": 43, "chapter": 3, "verseStart": 16, "verseEnd": 18 }),
            )
            .unwrap(),
            2
        );
        assert_eq!(
            invoke(&webview, "list_highlights", json!({})).unwrap(),
            json!([])
        );

        // Notes: add, list (for a passage, and all), edit, delete, restore.
        let note = invoke(
            &webview,
            "add_note",
            json!({ "bookId": 43, "chapter": 3, "verseStart": 16, "verseEnd": 18, "body": " First. " }),
        )
        .unwrap();
        assert_eq!(
            (&note["reference"], &note["body"]),
            (&json!("John 3:16\u{2013}18"), &json!("First."))
        );
        let overlapping = invoke(
            &webview,
            "list_notes",
            json!({ "bookId": 43, "chapter": 3, "verseStart": 17, "verseEnd": 17 }),
        )
        .unwrap();
        assert_eq!(overlapping[0]["id"], note["id"]);
        assert_eq!(
            invoke(&webview, "list_all_notes", json!({})).unwrap()[0]["id"],
            note["id"]
        );
        let edited = invoke(
            &webview,
            "update_note",
            json!({ "id": note["id"], "body": "Edited." }),
        )
        .unwrap();
        assert_eq!(edited["body"], "Edited.");
        assert_eq!(
            invoke(
                &webview,
                "update_note",
                json!({ "id": note["id"], "body": "  " }),
            ),
            Err(json!("A note can't be empty."))
        );
        let deleted = invoke(&webview, "delete_note", json!({ "id": note["id"] })).unwrap();
        assert_eq!(deleted["body"], "Edited.");
        let restored = invoke(
            &webview,
            "restore_note",
            json!({
                "bookId": deleted["bookId"], "chapter": deleted["chapter"],
                "verseStart": deleted["verseStart"], "verseEnd": deleted["verseEnd"],
                "body": deleted["body"], "createdAt": deleted["createdAt"],
                "updatedAt": deleted["updatedAt"],
            }),
        )
        .unwrap();
        assert_eq!(restored["createdAt"], deleted["createdAt"]);
        assert_eq!(
            invoke(&webview, "delete_note", json!({ "id": 999_999 })).unwrap(),
            Value::Null
        );

        let found = invoke(
            &webview,
            "search",
            json!({ "query": "faith hope charity", "translationCode": "kjv", "offset": 0 }),
        )
        .unwrap();
        assert_eq!(found["kind"], "text");
        let passage = invoke(
            &webview,
            "search",
            json!({ "query": "jn 3:16", "translationCode": "web", "offset": 0 }),
        )
        .unwrap();
        assert_eq!(passage["kind"], "passage");

        let compare = invoke(
            &webview,
            "get_compare",
            json!({ "bookId": 45, "chapter": 14, "translationCodes": ["bsb", "web"] }),
        )
        .unwrap();
        assert_eq!(compare["columns"][0]["code"], "bsb");
        assert_eq!(
            compare["columns"][1]["verses"].as_array().map(Vec::len),
            Some(26)
        );

        // Import: the file is chosen (in the app, through the native
        // dialog) and previewed, then confirmed or cancelled over IPC.
        let file = db.path().with_file_name("ipc.gospel-getter.json");
        std::fs::write(
            &file,
            r#"{"format": "gospel-getter-reader-data", "format_version": 1,
                "exported_at": "2026-10-04T00:00:00.000Z", "app_version": "2.2.0",
                "reading_position": {"translation": "web", "book": 19, "chapter": 23},
                "bookmarks": [{"book": 19, "chapter": 23, "verse_start": 1, "verse_end": 4}]}"#,
        )
        .expect("write import file");
        let preview = |app: &tauri::App<tauri::test::MockRuntime>| {
            let state = app.state::<AppState>();
            match tauri::async_runtime::block_on(crate::commands::prepare_import(&state, &file))
                .expect("prepare")
            {
                crate::commands::ImportChoice::Ready { preview } => preview.token,
                other => panic!("expected a valid file, got {other:?}"),
            }
        };
        let cancelled = preview(&app);
        assert_eq!(
            invoke(&webview, "cancel_import", json!({ "token": cancelled })).unwrap(),
            Value::Null
        );
        assert!(
            invoke(
                &webview,
                "apply_import",
                json!({
                    "token": cancelled,
                    "replace": false,
                    "restorePosition": true,
                    "restoreFindingsPermission": false,
                }),
            )
            .is_err()
        );
        let token = preview(&app);
        let result = invoke(
            &webview,
            "apply_import",
            json!({
                "token": token,
                "replace": false,
                "restorePosition": true,
                "restoreFindingsPermission": false,
            }),
        )
        .unwrap();
        assert_eq!(result["added"], 1);
        assert_eq!(result["findingsPermission"], Value::Null);
        assert_eq!(result["position"]["reference"], "Psalms 23 (WEB)");
    }

    /// What a [`TestScript`] was asked, and the answer its file dialogs give.
    #[derive(Default)]
    struct Asked {
        chosen: Option<PathBuf>,
        opened: Vec<String>,
        restarted: bool,
    }

    /// A [`BridgeScript`] like the frontend tests' bridge's: its file dialogs
    /// answer `Asked::chosen`, its clock is 2026-02-28 20:00 UTC in UTC+10
    /// (Mar 1 there), its browser only notes what it's asked to open, a
    /// check finds 9.9.9, and an install sends two events, then fails as
    /// not verified.
    struct TestScript(Arc<Mutex<Asked>>);

    impl TestScript {
        fn asked(&self) -> std::sync::MutexGuard<'_, Asked> {
            self.0.lock().unwrap_or_else(|e| e.into_inner())
        }
    }

    impl BridgeScript for TestScript {
        fn choose_file(&self) -> Result<Option<PathBuf>, String> {
            Ok(self.asked().chosen.clone())
        }

        fn clock(&self) -> (DateTime<Utc>, FixedOffset) {
            let now = DateTime::parse_from_rfc3339("2026-02-28T20:00:00Z").expect("time");
            (
                now.to_utc(),
                FixedOffset::east_opt(10 * 3600).expect("offset"),
            )
        }

        fn open_in_browser(&self, url: &str) -> std::io::Result<()> {
            self.asked().opened.push(url.to_string());
            Ok(())
        }

        fn check_for_update(&self) -> ScriptedCheck {
            let update = UpdateInfo::new("9.9.9".into(), Some("Notes.".into()), None, true);
            ScriptedCheck {
                limit: Duration::from_secs(5),
                answer: Some(Ok(Some(update))),
            }
        }

        fn install_update(&self) -> ScriptedInstall {
            ScriptedInstall {
                events: vec![
                    DownloadEvent::Started {
                        content_length: Some(10),
                    },
                    DownloadEvent::Finished,
                ],
                problem: Some(Problem::NotVerified),
            }
        }

        fn restart(&self) {
            self.asked().restarted = true;
        }
    }

    /// A mock app with the app's handler and state, as the frontend tests'
    /// bridge builds it: `state`, with `script` set, and the updater's
    /// pending update. Every message a `Channel` sends is kept in
    /// `heard`, with the channel's id, as the bridge's interceptor reads it.
    fn scripted_app(
        state: AppState,
        script: TestScript,
        heard: Arc<Mutex<Vec<(u32, Value)>>>,
    ) -> (
        tauri::App<tauri::test::MockRuntime>,
        tauri::WebviewWindow<tauri::test::MockRuntime>,
    ) {
        let app = mock_builder()
            .manage(state.with_bridge_script(script))
            .manage(crate::updater::PendingUpdate::default())
            .invoke_handler(super::invoke_handler())
            .channel_interceptor(move |_, channel, _, body| {
                if let InvokeResponseBody::Json(json) = body {
                    let message = serde_json::from_str(json).expect("a JSON message");
                    heard
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push((channel.0, message));
                }
                true
            })
            .build(mock_context(noop_assets()))
            .expect("build mock app");
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("build webview");
        (app, webview)
    }

    /// With the frontend tests' bridge's script set, the commands that need
    /// what the mock runtime doesn't have (a file dialog, the browser, the
    /// clock, the updater) ask it instead, through the same IPC, and go on
    /// with their own logic: the app's rules for links, its messages, its
    /// export and import. A progress `Channel` works on the mock runtime,
    /// its messages reaching the channel interceptor.
    #[test]
    fn the_commands_ask_the_bridge_script_for_what_the_mock_runtime_lacks() {
        let (db, state) = tauri::async_runtime::block_on(async {
            let (db, pool) = fresh_database("ipc_script").await;
            let state = AppState::load(pool).await.expect("load state");
            state.search_index.set_ready();
            (db, state)
        });
        let asked = Arc::new(Mutex::new(Asked::default()));
        let script = TestScript(asked.clone());
        let heard = Arc::new(Mutex::new(Vec::new()));
        let (_app, webview) = scripted_app(state, script, heard.clone());
        let choose = |path: Option<PathBuf>| asked.lock().expect("asked").chosen = path;

        // The clock: it's Mar 1 in the export's time zone.
        let options = invoke(&webview, "get_notes_export_options", json!({})).unwrap();
        assert_eq!(options["dates"]["today"], "2026-03-01");

        // The file dialogs: cancelled, then a file chosen.
        assert_eq!(
            invoke(&webview, "choose_import_file", json!({})).unwrap()["status"],
            "cancelled"
        );
        let backup = db.path().with_file_name("ipc_script.gospel-getter.json");
        choose(Some(backup.clone()));
        let saved = invoke(&webview, "export_reader_data", json!({ "preferences": {} })).unwrap();
        assert_eq!(saved["status"], "saved");
        assert!(backup.is_file());
        assert_eq!(
            invoke(&webview, "choose_import_file", json!({})).unwrap()["status"],
            "ready"
        );
        invoke(
            &webview,
            "add_note",
            json!({ "bookId": 43, "chapter": 3, "verseStart": 16, "verseEnd": 16, "body": "Loved." }),
        )
        .unwrap();
        let notes = db.path().with_file_name("ipc_script.md");
        choose(Some(notes.clone()));
        let exported = invoke(
            &webview,
            "export_notes",
            json!({ "translationCode": "kjv", "filter": {} }),
        )
        .unwrap();
        assert_eq!(
            (&exported["status"], &exported["notes"]),
            (&json!("saved"), &json!(1))
        );
        let markdown = std::fs::read_to_string(&notes).expect("read notes");
        assert!(markdown.contains("Exported Mar 1, 2026"), "{markdown}");

        // The browser: each command opens its own page, by the app's rules.
        for (cmd, args) in [
            ("open_download_page", json!({})),
            ("open_release_notes", json!({})),
            ("open_support_page", json!({})),
            ("open_credit_link", json!({ "which": "cc-by" })),
            ("open_web_link", json!({ "url": "https://example.com" })),
        ] {
            assert_eq!(invoke(&webview, cmd, args), Ok(Value::Null), "{cmd}");
        }
        assert_eq!(
            invoke(
                &webview,
                "open_web_link",
                json!({ "url": "file:///etc/passwd" })
            ),
            Err(json!(crate::web_link::messages::NOT_A_WEB_LINK))
        );
        assert_eq!(
            invoke(
                &webview,
                "open_credit_link",
                json!({ "which": "elsewhere" })
            ),
            Err(json!(crate::credit_links::messages::UNKNOWN_LINK))
        );
        assert_eq!(
            asked.lock().expect("asked").opened,
            [
                crate::updater::RELEASES_PAGE.to_string(),
                crate::updater::release_notes_page(env!("CARGO_PKG_VERSION")),
                crate::support_link::SUPPORT_PAGE.to_string(),
                "https://creativecommons.org/licenses/by/4.0/".to_string(),
                "https://example.com/".to_string(),
            ]
        );

        // The updater: the check finds what's scripted; the install sends
        // its progress through the Channel, then fails with the app's words.
        let found = invoke(&webview, "check_for_update", json!({})).unwrap();
        assert_eq!(
            (&found["version"], &found["canSelfUpdate"]),
            (&json!("9.9.9"), &json!(true))
        );
        assert_eq!(
            invoke(
                &webview,
                "install_update",
                json!({ "onEvent": "__CHANNEL__:7" })
            ),
            Err(json!(crate::updater::messages::NOT_VERIFIED))
        );
        assert_eq!(
            *heard.lock().expect("heard"),
            [
                (
                    7,
                    json!({ "event": "started", "data": { "contentLength": 10 } })
                ),
                (7, json!({ "event": "finished" })),
            ]
        );
        assert!(!asked.lock().expect("asked").restarted);
    }
}
