//! The updater's words and its download arithmetic (its sentences,
//! `megabytes`, and what each download event does to the progress bars and
//! the status line), Skip this version's rule ([`launch_offer`]) and Go
//! back's words, kept free of the DOM so it is tested natively
//! (`cargo test`).
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

/// The local storage key Skip this version keeps the skipped version
/// under, beside the settings: this computer's, so never in a backup (an
/// export carries only the settings `preferences` names for a file).
pub const SKIPPED_KEY: &str = "gospel-getter-update-skipped";

/// About's note under an update the reader skipped.
pub const SKIPPED_NOTE: &str =
    "You skipped this version, so it isn’t offered when Gospel Getter starts.";

/// Where an update a check found shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offer {
    /// As always: the banner, or About if it's open.
    Show,
    /// In About, noting it was skipped (Check now still offers it).
    Skipped,
    /// Nowhere: the launch check found the version the reader skipped.
    Quiet,
}

/// Skip this version's rule, for `version`, found by the launch check
/// (`quiet`) or Check now, with `skipped` saved: the launch check stays
/// quiet for exactly the skipped version (a newer one shows again), and
/// Check now always shows it, saying it was skipped.
pub fn launch_offer(version: &str, skipped: Option<&str>, quiet: bool) -> Offer {
    match (skipped == Some(version), quiet) {
        (false, _) => Offer::Show,
        (true, true) => Offer::Quiet,
        (true, false) => Offer::Skipped,
    }
}

/// About's Go back button.
pub fn go_back(version: &str) -> String {
    format!("Go back to {version}")
}

/// About's line in place of Go back, when `version` can't use the data
/// as this one has left it (a step since isn't compatible both ways).
pub fn go_back_unavailable(version: &str) -> String {
    format!(
        "Going back to {version} isn’t offered: it can’t read the data this version has updated."
    )
}

/// Go back's confirm dialog's title.
pub fn go_back_title(version: &str) -> String {
    format!("Go back to {version}?")
}

/// Go back's confirm dialog's words. They promise nothing about the newer
/// version being offered again: the older one may not know Skip.
pub fn go_back_body(version: &str) -> String {
    format!(
        "Gospel Getter will save a full backup of your notes, bookmarks, highlights and settings \
         next to its database, then install {version} and restart."
    )
}

/// Go back's status while the backup is saved.
pub const BACKING_UP: &str = "Saving a backup of your notes, bookmarks, highlights and settings…";

/// Go back's line once the backup is saved.
pub fn backup_saved(path: &str) -> String {
    format!("Backup saved: {path}")
}

/// The banner's title while going back, if About closes part way.
pub fn going_back(version: &str) -> String {
    format!("Going back to {version}")
}

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

/// Bytes as megabytes to one place: "12.3 MB".
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
/// come, and what it's of (`None`: the update).
#[derive(Debug, Clone, Default)]
pub struct Download {
    total: Option<u64>,
    received: u64,
    going_back_to: Option<String>,
}

impl Download {
    /// Go back's download of `version`: its words name it.
    pub fn going_back_to(version: &str) -> Self {
        Self {
            going_back_to: Some(version.to_string()),
            ..Self::default()
        }
    }

    /// The status as the download starts.
    pub fn downloading(&self) -> String {
        match &self.going_back_to {
            Some(version) => format!("Downloading {version}…"),
            None => DOWNLOADING.to_string(),
        }
    }

    fn installing(&self) -> String {
        match &self.going_back_to {
            Some(version) => {
                format!("Installing {version}. Gospel Getter will restart when it’s done.")
            }
            None => INSTALLING.to_string(),
        }
    }

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
                            status: Some(format!("{} {}%", self.downloading(), got * 100 / total)),
                            ..Step::default()
                        }
                    }
                    None => Step {
                        status: Some(format!(
                            "{} {}",
                            self.downloading(),
                            megabytes(self.received)
                        )),
                        ..Step::default()
                    },
                }
            }
            DownloadEvent::Finished => Step {
                value: self.total,
                status: Some(self.installing()),
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

    /// Skip this version: the launch check stays quiet for exactly the
    /// skipped version, a newer one shows again, and Check now shows the
    /// skipped one, noting it.
    #[test]
    fn a_skipped_version_is_quiet_at_launch_and_shown_by_check_now() {
        assert_eq!(launch_offer("2.37.0", Some("2.37.0"), true), Offer::Quiet);
        assert_eq!(
            launch_offer("2.37.0", Some("2.37.0"), false),
            Offer::Skipped
        );
        assert_eq!(launch_offer("2.38.0", Some("2.37.0"), true), Offer::Show);
        assert_eq!(launch_offer("2.38.0", Some("2.37.0"), false), Offer::Show);
        assert_eq!(launch_offer("2.37.0", None, true), Offer::Show);
        assert_eq!(launch_offer("2.37.0", None, false), Offer::Show);
        // Exactly the version: no prefix or near match.
        assert_eq!(launch_offer("2.37.1", Some("2.37"), true), Offer::Show);
        assert_eq!(SKIPPED_KEY, "gospel-getter-update-skipped");
    }

    #[test]
    fn go_backs_words() {
        assert_eq!(go_back("2.35.0"), "Go back to 2.35.0");
        assert_eq!(go_back_title("2.35.0"), "Go back to 2.35.0?");
        assert_eq!(
            go_back_body("2.35.0"),
            "Gospel Getter will save a full backup of your notes, bookmarks, highlights and \
             settings next to its database, then install 2.35.0 and restart."
        );
        assert_eq!(
            go_back_unavailable("2.35.0"),
            "Going back to 2.35.0 isn’t offered: it can’t read the data this version has updated."
        );
        assert_eq!(backup_saved("/d/b.json"), "Backup saved: /d/b.json");
        assert_eq!(going_back("2.35.0"), "Going back to 2.35.0");
    }

    #[test]
    fn going_backs_download_names_the_version() {
        let mut d = Download::going_back_to("2.35.0");
        d.on_event(&DownloadEvent::Started {
            content_length: Some(100),
        });
        let step = d.on_event(&DownloadEvent::Progress { chunk_length: 50 });
        assert_eq!(step.status.as_deref(), Some("Downloading 2.35.0… 50%"));
        let step = d.on_event(&DownloadEvent::Finished);
        assert_eq!(
            step.status.as_deref(),
            Some("Installing 2.35.0. Gospel Getter will restart when it’s done.")
        );
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
