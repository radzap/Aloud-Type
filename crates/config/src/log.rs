//! Optional diagnostic log. Off by default: no file is created unless the
//! `--verbose` command-line flag is present. The API cannot carry audio or
//! dictated text: events hold only fixed labels and numbers.
use crate::{atomic_write, Fs};
use std::path::PathBuf;

pub const VERBOSE_FLAG: &str = "--verbose";
pub const LOG_FILE_NAME: &str = "aloud-type.log";

/// True when the command line asks for the verbose log.
pub fn verbose_requested<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|a| a.as_ref() == VERBOSE_FLAG)
}

/// A loggable event. Free-form strings are deliberately impossible: labels are
/// `&'static str` literals and details are numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub label: &'static str,
    /// Optional numeric detail such as a platform error code or a count.
    pub code: Option<i64>,
}

impl Event {
    pub const fn new(label: &'static str) -> Self {
        Self { label, code: None }
    }
    pub const fn with_code(label: &'static str, code: i64) -> Self {
        Self {
            label,
            code: Some(code),
        }
    }
}

pub struct Logger<F: Fs> {
    fs: F,
    path: Option<PathBuf>,
}

impl<F: Fs> Logger<F> {
    /// `dir` is the app data folder. Without the verbose flag the logger is
    /// inert and never touches the file system.
    pub fn new(fs: F, dir: &std::path::Path, verbose: bool) -> Self {
        Self {
            fs,
            path: verbose.then(|| dir.join(LOG_FILE_NAME)),
        }
    }

    pub fn enabled(&self) -> bool {
        self.path.is_some()
    }

    /// Appends one line `<ms> <label>[ code=<n>]`. Failures are ignored:
    /// logging must never disturb a Dictation.
    pub fn log(&self, at_ms: u64, e: Event) {
        let Some(path) = &self.path else { return };
        let mut line = String::new();
        if let Some(prev) = self.fs.read_to_string(path).ok().flatten() {
            line.push_str(&prev);
        }
        line.push_str(&format!("{at_ms} {}", e.label));
        if let Some(c) = e.code {
            line.push_str(&format!(" code={c}"));
        }
        line.push('\n');
        let _ = atomic_write(&self.fs, path, line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StdFs;

    #[test]
    fn flag_is_recognised_only_when_present() {
        assert!(!verbose_requested(["aloud-type.exe"]));
        assert!(verbose_requested(["aloud-type.exe", "--verbose"]));
        assert!(!verbose_requested(["aloud-type.exe", "--verbosee"]));
    }

    #[test]
    fn no_file_or_folder_is_created_without_the_flag() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("AloudType");
        let log = Logger::new(StdFs, &app, false);
        assert!(!log.enabled());
        log.log(1, Event::new("dictation started"));
        log.log(2, Event::with_code("microphone error", 5));
        assert!(!app.exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn with_the_flag_events_are_appended_as_labels_and_numbers_only() {
        let dir = tempfile::tempdir().unwrap();
        let log = Logger::new(StdFs, dir.path(), true);
        log.log(10, Event::new("dictation started"));
        log.log(25, Event::with_code("microphone error", -2147024891));
        let text = std::fs::read_to_string(dir.path().join(LOG_FILE_NAME)).unwrap();
        assert_eq!(
            text,
            "10 dictation started
25 microphone error code=-2147024891
"
        );
    }
}
