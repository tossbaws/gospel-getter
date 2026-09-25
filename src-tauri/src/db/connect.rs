use anyhow::Context;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;

/// Connect to the SQLite database, creating the file (and its parent
/// directory) on first run so a fresh checkout starts up on its own instead
/// of requiring the database to already exist.
pub async fn create_pool(database_url: &str) -> anyhow::Result<SqlitePool> {
    if let Some(path) = database_url.strip_prefix("sqlite:") {
        let path = path.split('?').next().unwrap_or(path);
        if let Some(parent) = std::path::Path::new(path).parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).context(format!(
                "Failed to create database directory {}",
                parent.display()
            ))?;
        }
    }

    let options = SqliteConnectOptions::from_str(database_url)
        .context(format!("Invalid database URL: {}", database_url))?
        .create_if_missing(true);

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .context(format!("Failed to connect to database at {}", database_url))
}
