//! Gospel Getter: an offline Bible reader. The Tauri app itself is
//! `run`; the modules are public so the frontend test bridge
//! (`examples/frontend_bridge.rs`) can drive the same command logic.

pub mod commands;
pub mod credit_links;
pub mod db;
pub mod domain;
pub mod notes_export;
pub mod passage_copy;
pub mod reader_data;
#[cfg(test)]
mod reader_data_tests;
pub mod release_notes;
pub mod retry;
pub mod start_here;
pub mod updater;
pub mod window;

use anyhow::Context;
use tauri::Manager;
use tracing::{Level, info};
use tracing_subscriber::fmt::format::FmtSpan;

use commands::AppState;
use db::create_pool;

/// Start the app: set up logging, open the database and show the window
/// where it was last time.
pub fn run() {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_span_events(FmtSpan::CLOSE)
        .init();

    tauri::Builder::default()
        // Native open/save dialogs, used only from Rust (see
        // `commands::export_reader_data`); the webview gets no dialog or
        // file-system permission.
        .plugin(tauri_plugin_dialog::init())
        // Updates, likewise used only from Rust (see `updater`). The
        // releases page and the credit links open through the `open` crate,
        // also only from Rust, so nothing takes over link clicks in the page.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updater::PendingUpdate::default())
        .setup(|app| {
            setup_app(app).map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })
        })
        .invoke_handler(invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running Gospel Getter");
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
        commands::add_note,
        commands::update_note,
        commands::delete_note,
        commands::restore_note,
        commands::search,
        commands::get_compare,
        commands::export_reader_data,
        commands::get_notes_export_options,
        commands::count_notes_export,
        commands::export_notes,
        commands::choose_import_file,
        commands::apply_import,
        commands::cancel_import,
        updater::check_for_update,
        updater::install_update,
        updater::open_download_page,
        updater::open_release_notes,
        credit_links::open_credit_link,
    ]
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
    std::fs::create_dir_all(&app_data_dir).context("Failed to create the app data directory")?;
    let db_path = app_data_dir.join("gospel_getter.db");
    info!("Using database at {}", db_path.display());

    // Best-effort: if we can't resolve the home directory, or the legacy
    // database isn't there, `migrate_legacy_db_if_needed` is a no-op and
    // we just proceed to create a fresh database, same as any other first
    // run.
    if let Ok(home_dir) = app.path().home_dir() {
        let legacy_db_path = home_dir.join(".local/share/gospel-getter/data/gospel_getter.db");
        match db::migrate_legacy_db_if_needed(&db_path, &legacy_db_path) {
            Ok(true) => info!(
                "Migrated legacy database from {} to {}",
                legacy_db_path.display(),
                db_path.display()
            ),
            Ok(false) => {}
            Err(e) => tracing::warn!(
                "Failed to migrate legacy database from {}: {e}",
                legacy_db_path.display()
            ),
        }
    }

    let database_url = format!("sqlite:{}", db_path.display());
    let state = tauri::async_runtime::block_on(init_state(&database_url))
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

/// Connect to the database, run migrations/seeding, and load the books and
/// translations that stay in memory for the life of the app.
async fn init_state(database_url: &str) -> anyhow::Result<AppState> {
    let database = create_pool(database_url)
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
mod ipc_tests {
    //! The commands, called through Tauri's own IPC layer (on its mock
    //! runtime) with exactly the argument objects `ui/index.html` passes
    //! to `invoke` — so a renamed argument on either side fails here.

    use serde_json::{Value, json};
    use tauri::Manager;
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder, mock_context, noop_assets};
    use tauri::webview::InvokeRequest;

    use crate::commands::AppState;
    use crate::db::test_support::{bundled, fresh_database};

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
            invoke(
                &webview,
                "save_reading_position",
                json!({ "bookId": 43, "chapter": 22, "translationCode": "web" }),
            ),
            Err(json!("John has no chapter 22."))
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
                json!({ "token": cancelled, "replace": false, "restorePosition": true }),
            )
            .is_err()
        );
        let token = preview(&app);
        let result = invoke(
            &webview,
            "apply_import",
            json!({ "token": token, "replace": false, "restorePosition": true }),
        )
        .unwrap();
        assert_eq!(result["added"], 1);
        assert_eq!(result["position"]["reference"], "Psalms 23 (WEB)");
    }
}
