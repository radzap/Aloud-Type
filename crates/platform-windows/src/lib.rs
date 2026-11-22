//! Windows platform adapters (keyboard hook, injection, audio, tray, overlay).
pub mod audio;
pub mod classify;
pub mod inject;
#[cfg(windows)]
pub mod inject_win;
#[cfg(windows)]
pub mod hook;
