//! Shared settings types and file I/O: the user settings file and the machine-state file.
mod file;
mod log;
mod notice;
mod fs;
mod paths;
mod settings;
mod state;

pub use file::{ConfigError, ConfigFile, LoadStatus, Schema};
pub use log::{verbose_requested, Event, Logger, LOG_FILE_NAME, VERBOSE_FLAG};
pub use notice::SettingsNotice;
pub use fs::{atomic_write, Fs, StdFs};
pub use paths::AppPaths;
pub use settings::*;
pub use state::{GpuProbe, State};
