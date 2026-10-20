//! A fake update server for the updater checks (9 to 13), on 127.0.0.1:
//! a `latest.json` naming a newer version (99.0.0) and a test package,
//! signed for that version with a key pair made for this run (never
//! written anywhere, so no key is ever committed or reused). The package
//! is served slowly, so the download's progress can be seen moving.
//!
//! The app reaches it only through the GUI checks' test build (feature
//! `gui-test-updater`, `src-tauri/src/updater/test_hook.rs`): that build
//! reads where to check, with which public key, in which mode and how long
//! to wait from the settings file the checks write, and never installs
//! what it downloads. A release build ignores the file, which check 9
//! confirms: the shipped app's session (checks 1 to 8, 14 to 16) sends this server
//! nothing.
//!
//! Also here: a "black hole" for check 13, which takes connections and
//! never answers, so the check runs out of time.

use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// The version the server announces: newer than any real one.
pub const VERSION: &str = "99.0.0";

/// The release notes it announces, as Markdown (About shows them as
/// blocks; the banner as text).
pub const NOTES: &str = "A test release, from the GUI checks' own update server.\n\n## Fixes\n- **Search** finds words again.\n  - Even `G`.\n- The About box.";

/// The test package: its size, and how it's sent (in pieces, with a pause
/// after each, so it takes about eight seconds and the progress can be
/// seen, and photographed, part way).
const PACKAGE_SIZE: usize = 64 * CHUNK;
const CHUNK: usize = 32 * 1024;
const CHUNK_PAUSE: Duration = Duration::from_millis(120);

/// What the server has been asked for, so far.
#[derive(Debug, Default)]
pub struct Requests {
    pub manifests: AtomicU64,
    pub packages: AtomicU64,
    /// Bytes of the package sent, over all downloads.
    pub package_bytes: AtomicU64,
}

/// The running server, its key and the black hole.
pub struct UpdateServer {
    /// `http://127.0.0.1:<port>/latest.json`.
    pub endpoint: String,
    /// The black hole's address, as an endpoint.
    pub black_hole: String,
    /// The run's public key, as `tauri.conf.json` writes one (base64 of
    /// minisign's public key file).
    pub pubkey: String,
    pub requests: Arc<Requests>,
    pub package_size: u64,
}

impl UpdateServer {
    /// Makes the key pair, signs the package and starts serving.
    pub async fn start() -> Result<Self> {
        let keys = minisign::KeyPair::generate_unencrypted_keypair()
            .context("making the run's signing key")?;
        let package: Vec<u8> = (0..PACKAGE_SIZE)
            .map(|i| u8::try_from(i % 251).unwrap_or(0))
            .collect();
        // As the Tauri CLI signs an update: the version in the trusted
        // comment (the app requires it, `requireSignedVersion`).
        let trusted = format!(
            "timestamp:{}\tfile:gui-check-update.bin\tversion:{VERSION}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
        );
        let signature = minisign::sign(
            Some(&keys.pk),
            &keys.sk,
            Cursor::new(&package),
            Some(&trusted),
            Some("signature from the GUI checks' test key"),
        )
        .context("signing the test package")?;
        let pubkey = STANDARD.encode(
            keys.pk
                .to_box()
                .context("writing the run's public key")?
                .to_string(),
        );
        let signature = STANDARD.encode(signature.to_string());

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("starting the test update server")?;
        let port = listener.local_addr()?.port();
        let base = format!("http://127.0.0.1:{port}");
        let entry =
            json!({ "signature": signature, "url": format!("{base}/gui-check-update.bin") });
        let mut platforms = serde_json::Map::new();
        for os in ["linux-x86_64", "windows-x86_64"] {
            platforms.insert(os.to_string(), entry.clone());
        }
        for key in [
            "linux-x86_64-appimage",
            "linux-x86_64-deb",
            "windows-x86_64-nsis",
            "windows-x86_64-msi",
        ] {
            platforms.insert(key.to_string(), entry.clone());
        }
        let manifest = json!({
            "version": VERSION,
            "notes": NOTES,
            "pub_date": "2026-10-20T00:00:00Z",
            "platforms": platforms,
        })
        .to_string();

        let requests = Arc::new(Requests::default());
        let served = Arc::new((manifest, package));
        tokio::spawn({
            let requests = requests.clone();
            async move {
                while let Ok((stream, _)) = listener.accept().await {
                    let requests = requests.clone();
                    let served = served.clone();
                    tokio::spawn(async move {
                        let _ = serve(stream, &served.0, &served.1, &requests).await;
                    });
                }
            }
        });

        let hole = TcpListener::bind("127.0.0.1:0")
            .await
            .context("starting the black hole")?;
        let hole_port = hole.local_addr()?.port();
        tokio::spawn(async move {
            // Kept open, never answered, until the run ends.
            let mut held = Vec::new();
            while let Ok((stream, _)) = hole.accept().await {
                held.push(stream);
            }
        });

        Ok(Self {
            endpoint: format!("{base}/latest.json"),
            black_hole: format!("http://127.0.0.1:{hole_port}/latest.json"),
            pubkey,
            requests,
            package_size: PACKAGE_SIZE as u64,
        })
    }

    /// Writes the test build's settings (see `updater/test_hook.rs`):
    /// `mode` is `self` or `link`; `offline` points it at the black hole
    /// with a short time limit.
    pub async fn write_settings(&self, path: &Path, mode: &str, offline: bool) -> Result<()> {
        let mut settings = json!({
            "endpoint": if offline { &self.black_hole } else { &self.endpoint },
            "pubkey": self.pubkey,
            "mode": mode,
        });
        if offline {
            settings["timeoutMs"] = json!(OFFLINE_LIMIT.as_millis() as u64);
        }
        tokio::fs::write(path, settings.to_string())
            .await
            .with_context(|| format!("writing the test updater settings {}", path.display()))
    }

    pub fn manifests(&self) -> u64 {
        self.requests.manifests.load(Ordering::SeqCst)
    }

    pub fn packages(&self) -> u64 {
        self.requests.packages.load(Ordering::SeqCst)
    }

    pub fn package_bytes(&self) -> u64 {
        self.requests.package_bytes.load(Ordering::SeqCst)
    }
}

/// The check's time limit check 13 sets: the app gives up after this.
pub const OFFLINE_LIMIT: Duration = Duration::from_secs(2);

/// Answers one request: the manifest, the package (slowly), or 404.
async fn serve(
    mut stream: TcpStream,
    manifest: &str,
    package: &[u8],
    requests: &Requests,
) -> Result<()> {
    let mut head = Vec::new();
    let mut buf = [0u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = stream.read(&mut buf).await?;
        if n == 0 || head.len() > 16 * 1024 {
            return Ok(());
        }
        head.extend_from_slice(&buf[..n]);
    }
    let head = String::from_utf8_lossy(&head);
    let path = head.split_whitespace().nth(1).unwrap_or("/");
    match path {
        "/latest.json" => {
            requests.manifests.fetch_add(1, Ordering::SeqCst);
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{manifest}",
                manifest.len()
            );
            stream.write_all(reply.as_bytes()).await?;
        }
        "/gui-check-update.bin" => {
            requests.packages.fetch_add(1, Ordering::SeqCst);
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                package.len()
            );
            stream.write_all(reply.as_bytes()).await?;
            for chunk in package.chunks(CHUNK) {
                stream.write_all(chunk).await?;
                stream.flush().await?;
                requests
                    .package_bytes
                    .fetch_add(chunk.len() as u64, Ordering::SeqCst);
                tokio::time::sleep(CHUNK_PAUSE).await;
            }
        }
        _ => {
            stream
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await?;
        }
    }
    stream.shutdown().await?;
    Ok(())
}
