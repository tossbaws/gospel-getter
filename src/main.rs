// On Windows, this is a background server with no console UI of its own —
// suppress the console window that would otherwise pop up when launched
// from a shortcut or at login. No effect on other platforms.
#![cfg_attr(windows, windows_subsystem = "windows")]

use anyhow::Context;
use axum::serve;
use clap::Parser;
use tokio::signal;
use tracing::{Level, info};
use tracing_subscriber::fmt::format::FmtSpan;

mod config;
mod db;
mod domain;
mod dotenv;
mod web;

use config::Config;
use db::{create_pool, migrate, seed_cross_references_if_empty, seed_missing};

#[derive(Parser, Debug)]
#[command(
    name = "gospel_getter",
    about = "Read the Bible, book by book, chapter by chapter"
)]
struct Cli {}

pub async fn run() -> anyhow::Result<()> {
    let _cli = Cli::parse();
    dotenv::init();

    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_span_events(FmtSpan::CLOSE)
        .init();

    let config = Config::from_env().context("Failed to load configuration")?;
    info!("Starting gospel_getter on {}", config.listen_addr);

    let pool = create_pool(&config.database_url)
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

    let app = web::build_router(domain::Store::new(pool), books, translations);

    let listener = tokio::net::TcpListener::bind(config.listen_addr)
        .await
        .context("Failed to bind to address")?;
    info!("Listening on http://{}", config.listen_addr);

    serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    info!("Shutting down");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => { info!("Received Ctrl+C, shutting down"); }
        _ = terminate => { info!("Received SIGTERM, shutting down"); }
    }
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
