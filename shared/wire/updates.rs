//! What the updater sends the page: `install_update`'s progress on its
//! channel as an update downloads (the banner and About show it), and Go
//! back's answers (`updater`).

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

/// The release About's Go back offers, from this release's
/// `previous.json` (`updater::check_previous_version`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviousInfo {
    pub version: String,
    /// Whether that release can use the database as it is now
    /// (`migrate::older_app_can_use`): if not, About says so in place of
    /// offering it.
    pub reads_this_data: bool,
}

/// Where Go back saved its backup (`updater::save_backup_before_going_back`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupSaved {
    pub path: String,
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

    #[test]
    fn go_back() {
        pin(
            &PreviousInfo {
                version: "2.35.0".into(),
                reads_this_data: false,
            },
            r#"{"version":"2.35.0","readsThisData":false}"#,
        );
        pin(
            &BackupSaved {
                path: "/data/gospel-getter-backup-before-2.35.0-2026-11-22.gospel-getter.json"
                    .into(),
            },
            r#"{"path":"/data/gospel-getter-backup-before-2.35.0-2026-11-22.gospel-getter.json"}"#,
        );
    }
}
