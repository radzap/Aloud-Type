//! Windows platform adapters (keyboard hook, injection, audio, tray, overlay).
pub mod classify;
pub mod inject;
#[cfg(windows)]
pub mod inject_win;
#[cfg(windows)]
pub mod hook;
#[cfg(windows)]
pub mod instance;
#[cfg(windows)]
pub mod tray;
#[cfg(windows)]
pub mod overlay;
pub mod overlay_model;
pub mod tray_model;
