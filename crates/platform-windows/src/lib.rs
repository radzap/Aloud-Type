//! Windows platform adapters (keyboard hook, injection, audio, tray, overlay).
pub mod classify;
#[cfg(windows)]
pub mod hook;
#[cfg(windows)]
pub mod instance;
#[cfg(windows)]
pub mod tray;
pub mod tray_model;
