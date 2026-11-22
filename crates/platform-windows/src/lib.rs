//! Windows platform adapters (keyboard hook, injection, audio, tray, overlay).
pub mod classify;
#[cfg(windows)]
pub mod hook;
