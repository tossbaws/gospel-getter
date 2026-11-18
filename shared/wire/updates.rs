//! What `install_update` reports on its channel as the update downloads
//! (`updater`; the page's banner shows it).

use serde::{Deserialize, Serialize};

/// Download progress, sent to the banner while an update downloads. (The
/// frontend tests' bridge reads the progress it scripts as these too.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum DownloadEvent {
    #[serde(rename_all = "camelCase")]
    Started { content_length: Option<u64> },
    #[serde(rename_all = "camelCase")]
    Progress { chunk_length: u64 },
    /// Downloaded and verified; installing, then restarting.
    Finished,
}

#[cfg(test)]
mod tests {
    use super::super::pin;
    use super::*;

    #[test]
    fn download_event() {
        pin(
            &DownloadEvent::Started {
                content_length: Some(1000),
            },
            r#"{"event":"started","data":{"contentLength":1000}}"#,
        );
        pin(
            &DownloadEvent::Started {
                content_length: None,
            },
            r#"{"event":"started","data":{"contentLength":null}}"#,
        );
        pin(
            &DownloadEvent::Progress { chunk_length: 4 },
            r#"{"event":"progress","data":{"chunkLength":4}}"#,
        );
        pin(&DownloadEvent::Finished, r#"{"event":"finished"}"#);
    }
}
