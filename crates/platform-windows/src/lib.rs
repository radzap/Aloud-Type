//! Windows platform adapters (keyboard hook, injection, audio, tray, overlay).
pub mod audio;
pub mod autostart;
pub mod classify;
pub mod hook_health;
pub mod inject;
#[cfg(windows)]
pub mod inject_win;
#[cfg(windows)]
pub mod hook;
#[cfg(windows)]
pub mod instance;
#[cfg(windows)]
pub mod pipe_server;
#[cfg(windows)]
pub mod power_win;
#[cfg(windows)]
pub mod tray;
pub mod settings_launcher;
#[cfg(windows)]
pub mod overlay;
pub mod overlay_model;
pub mod tray_model;
