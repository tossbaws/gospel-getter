//! The app's log, kept in files as well as on stderr, so a cause logged on
//! Windows (whose GUI build has no console) or by a launcher that drops
//! stderr can still be found: `gospel-getter.log` in the app's log folder,
//! started afresh once it reaches [`MAX_LOG_BYTES`], keeping the
//! [`LOG_FILES`] most recent (`gospel-getter.1.log` is the one before).
//!
//! Hand-rolled rather than `tracing-appender`, which rotates by time only:
//! a size cap keeps the log small however long the app runs, and it's a few
//! lines on the standard library, with no new dependency.
//!
//! What's logged is the app's own words, places in the Bible and file paths
//! (never a note's text, the words of a search or a file's contents); the
//! files are the reader's alone (0600 on Unix, in a 0700 folder), as the
//! database is.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tracing_subscriber::EnvFilter;

/// The filter used unless `RUST_LOG` sets another: up to info, but only
/// warnings from zbus (the session bus that `window::single_instance_plugin`
/// uses on Linux), whose connections' info-level spans would fill the log on
/// every launch.
pub const DEFAULT_LOG_FILTER: &str = "info,zbus=warn";

/// How big the log file grows before it's started afresh.
pub const MAX_LOG_BYTES: u64 = 1024 * 1024;

/// How many log files are kept: the one being written and the ones before.
pub const LOG_FILES: usize = 3;

/// The log file's name; older ones have a number before the extension.
const LOG_NAME: &str = "gospel-getter";

/// What to log: `rust_log` (the `RUST_LOG` variable) if it's set to a
/// filter that reads, otherwise [`DEFAULT_LOG_FILTER`].
pub fn log_filter(rust_log: Option<&str>) -> EnvFilter {
    rust_log
        .filter(|directives| !directives.trim().is_empty())
        .and_then(|directives| EnvFilter::try_new(directives).ok())
        .unwrap_or_else(|| EnvFilter::new(DEFAULT_LOG_FILTER))
}

/// The log files: written to from any thread, through
/// `tracing_subscriber`'s `MakeWriter` (cloning is cheap: the clones share
/// the file). Until [`LogFile::open_in`] gives it a folder, what's written
/// to it goes nowhere (stderr still has it).
#[derive(Clone, Debug)]
pub struct LogFile(Arc<Mutex<Files>>);

#[derive(Debug)]
struct Files {
    dir: Option<PathBuf>,
    file: Option<File>,
    /// How many bytes the open file holds.
    written: u64,
    max_bytes: u64,
    keep: usize,
}

impl LogFile {
    /// Log files of at most `max_bytes` each (a line is never split; one
    /// longer than that is a file to itself), keeping `keep` of them (at
    /// least one), once they have a folder.
    pub fn new(max_bytes: u64, keep: usize) -> Self {
        Self(Arc::new(Mutex::new(Files {
            dir: None,
            file: None,
            written: 0,
            max_bytes,
            keep: keep.max(1),
        })))
    }

    /// Write the log into `dir` from now on, making the folder (the reader's
    /// alone) if it isn't there, and adding to the log already there.
    pub fn open_in(&self, dir: &Path) -> anyhow::Result<()> {
        use anyhow::Context;
        crate::db::create_private_dir(dir)?;
        let path = file_path(dir, 0);
        let file =
            open_private(&path).with_context(|| format!("Failed to open {}", path.display()))?;
        let written = file
            .metadata()
            .with_context(|| format!("Failed to read {}", path.display()))?
            .len();
        let mut files = self.lock();
        files.dir = Some(dir.to_path_buf());
        files.file = Some(file);
        files.written = written;
        Ok(())
    }

    fn lock(&self) -> MutexGuard<'_, Files> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The log file `age` files back in `dir`: 0 is the one being written.
fn file_path(dir: &Path, age: usize) -> PathBuf {
    if age == 0 {
        dir.join(format!("{LOG_NAME}.log"))
    } else {
        dir.join(format!("{LOG_NAME}.{age}.log"))
    }
}

/// Open `path` to add to, creating it readable and writable by the reader
/// alone (on Unix; the umask can only narrow that).
fn open_private(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

impl Files {
    /// Write `line`, starting a new file first if it would take this one
    /// past its size.
    fn write_line(&mut self, line: &[u8]) -> io::Result<()> {
        let Some(dir) = self.dir.clone() else {
            return Ok(());
        };
        let len = line.len() as u64;
        if self.written > 0 && self.written.saturating_add(len) > self.max_bytes {
            self.rotate(&dir)?;
        }
        let file = match &mut self.file {
            Some(file) => file,
            None => self.file.insert(open_private(&file_path(&dir, 0))?),
        };
        file.write_all(line)?;
        self.written = self.written.saturating_add(len);
        Ok(())
    }

    /// Move each file one older (the oldest kept is removed), and start the
    /// current one afresh.
    fn rotate(&mut self, dir: &Path) -> io::Result<()> {
        // Closed first: Windows can't rename a file that's open.
        self.file = None;
        self.written = 0;
        let oldest = file_path(dir, self.keep - 1);
        match std::fs::remove_file(&oldest) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        for age in (0..self.keep - 1).rev() {
            match std::fs::rename(file_path(dir, age), file_path(dir, age + 1)) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        self.file = Some(open_private(&file_path(dir, 0))?);
        Ok(())
    }
}

/// One event's writer: the formatter writes the whole line at once, and it
/// goes into the file under the lock, so lines from different threads never
/// mix and a file is only ever started afresh between lines.
pub struct LogLine<'a>(&'a LogFile);

impl Write for LogLine<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().write_line(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.0.lock().file {
            Some(file) => file.flush(),
            None => Ok(()),
        }
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogFile {
    type Writer = LogLine<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        LogLine(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch folder, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "gospel_getter_log_{label}_{}_{:x}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos())
            ));
            std::fs::create_dir_all(&dir).expect("make a scratch folder");
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .expect("list the folder")
            .map(|e| {
                e.expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    fn line(log: &LogFile, text: &str) {
        let mut writer = tracing_subscriber::fmt::MakeWriter::make_writer(log);
        writer.write_all(text.as_bytes()).expect("write a line");
        writer.flush().expect("flush it");
    }

    /// Lines go nowhere until there's a folder; then into the log file,
    /// which is started afresh when the next line would take it past its
    /// size, keeping the newest files, each whole lines and within the size.
    #[test]
    fn the_log_is_kept_in_a_few_small_files() {
        let scratch = Scratch::new("rotate");
        let dir = scratch.0.join("logs");
        let log = LogFile::new(100, 3);
        line(&log, "before there's a folder\n");
        log.open_in(&dir).expect("open the log");
        assert_eq!(names(&dir), ["gospel-getter.log"]);

        for i in 0..20 {
            line(&log, &format!("line {i:02} of the log, 30 bytes\n"));
        }
        assert_eq!(
            names(&dir),
            [
                "gospel-getter.1.log",
                "gospel-getter.2.log",
                "gospel-getter.log"
            ]
        );
        let read = |age: usize| std::fs::read_to_string(file_path(&dir, age)).expect("a log file");
        for age in 0..3 {
            let text = read(age);
            assert!(text.len() <= 100, "{age}: {} bytes", text.len());
            assert!(text.ends_with('\n') && !text.contains("before"), "{text:?}");
        }
        // The newest lines, in order, across the files.
        let all = format!("{}{}{}", read(2), read(1), read(0));
        let numbers: Vec<&str> = all.lines().map(|l| &l[5..7]).collect();
        assert_eq!(numbers.last(), Some(&"19"));
        assert!(numbers.windows(2).all(|w| w[0] < w[1]), "{numbers:?}");

        // A later launch adds to the current file, and rotates on.
        let size = read(0).len();
        let again = LogFile::new(100, 3);
        again.open_in(&dir).expect("open it again");
        line(&again, "next launch\n");
        assert!(read(0).len() == size + 12 || read(0) == "next launch\n");
        assert_eq!(names(&dir).len(), 3);
    }

    /// A line longer than a file's size is a file to itself, whole.
    #[test]
    fn a_long_line_is_never_split() {
        let scratch = Scratch::new("long");
        let log = LogFile::new(10, 2);
        log.open_in(&scratch.0).expect("open the log");
        line(&log, "short\n");
        line(&log, "a line longer than the files are\n");
        line(&log, "after\n");
        let read = |age: usize| std::fs::read_to_string(file_path(&scratch.0, age)).unwrap();
        assert_eq!(read(1), "a line longer than the files are\n");
        assert_eq!(read(0), "after\n");
        assert_eq!(names(&scratch.0).len(), 2);
    }

    /// The log files and their folder are the reader's alone.
    #[cfg(unix)]
    #[test]
    fn the_log_is_the_readers_alone() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = Scratch::new("private");
        let dir = scratch.0.join("logs");
        let log = LogFile::new(20, 2);
        log.open_in(&dir).expect("open the log");
        line(&log, "one line of the log\n");
        line(&log, "and another, rotated\n");
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&file_path(&dir, 0)), 0o600);
        assert_eq!(mode(&file_path(&dir, 1)), 0o600);
    }

    /// `RUST_LOG` chooses what's logged; unset, empty or unreadable, it's
    /// the usual filter.
    #[test]
    fn rust_log_chooses_what_is_logged() {
        assert_eq!(
            log_filter(None).to_string(),
            log_filter(Some("")).to_string()
        );
        assert_eq!(
            log_filter(None).to_string(),
            EnvFilter::new(DEFAULT_LOG_FILTER).to_string()
        );
        assert_eq!(
            log_filter(Some("debug,sqlx=warn")).to_string(),
            EnvFilter::new("debug,sqlx=warn").to_string()
        );
        assert_ne!(
            log_filter(Some("debug")).to_string(),
            log_filter(None).to_string()
        );
        assert_eq!(
            log_filter(Some("[[not a filter")).to_string(),
            log_filter(None).to_string()
        );
    }
}
