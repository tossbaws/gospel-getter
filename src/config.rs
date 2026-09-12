use anyhow::Context;
use std::net::SocketAddr;

/// Application configuration loaded from environment variables.
pub struct Config {
    pub listen_addr: SocketAddr,
    pub database_url: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let listen_addr: SocketAddr = std::env::var("LISTEN_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:3002".to_string())
            .parse()
            .context("LISTEN_ADDR must be a valid socket address (e.g. 0.0.0.0:3002)")?;

        let database_url = std::env::var("DATABASE_URL")
            .context("DATABASE_URL environment variable is required")?;

        Ok(Self {
            listen_addr,
            database_url,
        })
    }
}
