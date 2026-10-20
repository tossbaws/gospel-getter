//! The updater's words and its download arithmetic: the pure part of what
//! `ui/index.html`'s updater script did (its sentences, `megabytes`, and
//! what each download event does to the progress bars and the status
//! line), kept free of the DOM so it is tested natively (`cargo test`).
//! The check's time limit and its "couldn't reach GitHub" message are the
//! backend's (`updater::CHECK_TIMEOUT`, `messages::OFFLINE`).

use crate::dto::DownloadEvent;

/// Check now's line while it checks.
pub const CHECKING: &str = "Checking for updates…";

/// The status while the download starts.
pub const DOWNLOADING: &str = "Downloading the update…";

/// The status once the download is done (installing, then restarting).
pub const INSTALLING: &str = "Installing the update. Gospel Getter will restart when it’s done.";

/// After Download page opened the releases page.
pub const RELEASES_OPEN: &str = "The releases page is open in your browser.";

/// The banner's and About's heading for an update.
pub fn available(version: &str) -> String {
    format!("Gospel Getter {version} is available")
}

/// Check now's line for an update About shows (said once; the heading
/// shows it).
pub fn available_sentence(version: &str) -> String {
    format!("Gospel Getter {version} is available.")
}

/// Check now's line when nothing newer was found: the version, if known.
pub fn latest(version: Option<&str>) -> String {
    match version {
        Some(version) if !version.is_empty() => {
            format!("Gospel Getter {version} is the latest version.")
        }
        _ => "You have the latest version.".to_string(),
    }
}

/// Check now's line while an update is already downloading.
pub fn already_downloading(version: &str) -> String {
    format!("Gospel Getter {version} is already downloading.")
}

/// About's notes box's name.
pub fn whats_new_in(version: &str) -> String {
    format!("What’s new in {version}")
}

/// About's version line.
pub fn version_line(version: &str) -> String {
    format!("Version {version}")
}

/// Bytes as megabytes to one place: "12.3 MB" (the page's `megabytes`).
pub fn megabytes(bytes: u64) -> String {
    // Exact for any download size an app has (well under 2^53 bytes).
    #[allow(clippy::cast_precision_loss)]
    let mb = bytes as f64 / 1_048_576.0;
    format!("{mb:.1} MB")
}

/// What one download event changes: the progress bars' `max` and `value`
/// (both views), and the status line. `None` leaves it as it is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Step {
    pub max: Option<u64>,
    pub value: Option<u64>,
    pub status: Option<String>,
}

/// A download under way: its size, if the server said, and how much has
/// come.
#[derive(Debug, Clone, Default)]
pub struct Download {
    total: Option<u64>,
    received: u64,
}

impl Download {
    /// What `event` changes on screen.
    pub fn on_event(&mut self, event: &DownloadEvent) -> Step {
        match *event {
            DownloadEvent::Started { content_length } => {
                self.total = content_length.filter(|&n| n > 0);
                Step {
                    max: self.total,
                    ..Step::default()
                }
            }
            DownloadEvent::Progress { chunk_length } => {
                self.received = self.received.saturating_add(chunk_length);
                match self.total {
                    Some(total) => {
                        let got = self.received.min(total);
                        Step {
                            value: Some(got),
                            status: Some(format!("{DOWNLOADING} {}%", got * 100 / total)),
                            ..Step::default()
                        }
                    }
                    None => Step {
                        status: Some(format!("{DOWNLOADING} {}", megabytes(self.received))),
                        ..Step::default()
                    },
                }
            }
            DownloadEvent::Finished => Step {
                value: self.total,
                status: Some(INSTALLING.to_string()),
                ..Step::default()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sentences() {
        assert_eq!(available("2.6.0"), "Gospel Getter 2.6.0 is available");
        assert_eq!(
            available_sentence("2.6.0"),
            "Gospel Getter 2.6.0 is available."
        );
        assert_eq!(
            latest(Some("2.16.3")),
            "Gospel Getter 2.16.3 is the latest version."
        );
        assert_eq!(latest(None), "You have the latest version.");
        assert_eq!(latest(Some("")), "You have the latest version.");
        assert_eq!(
            already_downloading("2.6.0"),
            "Gospel Getter 2.6.0 is already downloading."
        );
        assert_eq!(whats_new_in("2.13.1"), "What’s new in 2.13.1");
        assert_eq!(version_line("2.16.3"), "Version 2.16.3");
    }

    #[test]
    fn megabytes_to_one_place() {
        assert_eq!(megabytes(0), "0.0 MB");
        assert_eq!(megabytes(3_145_728), "3.0 MB");
        assert_eq!(megabytes(1_572_864), "1.5 MB");
        assert_eq!(megabytes(12_897_485), "12.3 MB");
    }

    #[test]
    fn a_download_of_known_size_counts_in_percent() {
        let mut d = Download::default();
        assert_eq!(
            d.on_event(&DownloadEvent::Started {
                content_length: Some(1000)
            }),
            Step {
                max: Some(1000),
                ..Step::default()
            }
        );
        let step = d.on_event(&DownloadEvent::Progress { chunk_length: 250 });
        assert_eq!(step.value, Some(250));
        assert_eq!(step.status.as_deref(), Some("Downloading the update… 25%"));
        // Rounded down, and never past the size.
        let step = d.on_event(&DownloadEvent::Progress { chunk_length: 749 });
        assert_eq!(step.status.as_deref(), Some("Downloading the update… 99%"));
        let step = d.on_event(&DownloadEvent::Progress { chunk_length: 500 });
        assert_eq!(step.value, Some(1000));
        assert_eq!(step.status.as_deref(), Some("Downloading the update… 100%"));
        let step = d.on_event(&DownloadEvent::Finished);
        assert_eq!(step.value, Some(1000));
        assert_eq!(step.status.as_deref(), Some(INSTALLING));
    }

    #[test]
    fn a_download_of_unknown_size_counts_in_megabytes() {
        let mut d = Download::default();
        for length in [None, Some(0)] {
            assert_eq!(
                d.on_event(&DownloadEvent::Started {
                    content_length: length
                }),
                Step::default(),
                "no bar maximum: an indeterminate bar"
            );
        }
        let step = d.on_event(&DownloadEvent::Progress {
            chunk_length: 3_145_728,
        });
        assert_eq!(step.value, None);
        assert_eq!(
            step.status.as_deref(),
            Some("Downloading the update… 3.0 MB")
        );
        let step = d.on_event(&DownloadEvent::Finished);
        assert_eq!(step.value, None);
        assert_eq!(step.status.as_deref(), Some(INSTALLING));
    }
}
