//! Windows platform adapters (keyboard hook, injection, audio, tray, overlay).
pub mod audio;
pub mod autostart;
pub mod classify;
#[cfg(windows)]
pub mod file_watch;
#[cfg(windows)]
pub mod hook;
pub mod hook_health;
pub mod inject;
#[cfg(windows)]
pub mod inject_win;
#[cfg(windows)]
pub mod instance;
#[cfg(windows)]
pub mod overlay;
pub mod overlay_model;
#[cfg(windows)]
pub mod pipe_server;
#[cfg(windows)]
pub mod power_win;
pub mod settings_launcher;
#[cfg(windows)]
pub mod thread_priority;
#[cfg(windows)]
pub mod tray;
pub mod tray_model;
#[cfg(windows)]
pub mod win32;
