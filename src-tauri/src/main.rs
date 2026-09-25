// On Windows, this is a windowed desktop app with no console UI of its
// own — suppress the console window that would otherwise pop up when
// launched from a shortcut or at login. No effect on other platforms.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod commands;
mod db;
mod domain;

use anyhow::Context;
use tauri::Manager;
use tracing::{Level, info};
use tracing_subscriber::fmt::format::FmtSpan;

use commands::AppState;
use db::{create_pool, migrate, seed_cross_references_if_empty, seed_missing};

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_span_events(FmtSpan::CLOSE)
        .init();

    tauri::Builder::default()
        .setup(|app| {
            setup_app(app).map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_home,
            commands::get_chapters,
            commands::get_reading,
            commands::get_xref_text,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Gospel Getter");
}

/// Resolve where this app's database lives, migrate the pre-Tauri
/// (systemd/browser-wrapper) install's database into place on first run if
/// there is one, connect, and load everything into `AppState` — all before
/// the window is shown, so there's nothing left to "load" once it appears.
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
    app.manage(state);

    Ok(())
}

/// Connect to the database, run migrations/seeding, and load the books and
/// translations that stay in memory for the life of the app.
async fn init_state(database_url: &str) -> anyhow::Result<AppState> {
    let pool = create_pool(database_url)
        .await
        .context("Failed to connect to database")?;

    migrate(&pool)
        .await
        .context("Failed to run database migrations")?;
    seed_missing(&pool)
        .await
        .context("Failed to seed Bible data")?;
    seed_cross_references_if_empty(&pool)
        .await
        .context("Failed to seed cross-reference data")?;

    let books = db::get_all_books(&pool)
        .await
        .context("Failed to load books")?;
    info!("Loaded {} books", books.len());

    let translations = db::get_all_translations(&pool)
        .await
        .context("Failed to load translations")?;
    info!(
        "Loaded {} translations: {}",
        translations.len(),
        translations
            .iter()
            .map(|t| t.code.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );

    Ok(AppState {
        store: domain::Store::new(pool),
        books,
        translations,
    })
}
