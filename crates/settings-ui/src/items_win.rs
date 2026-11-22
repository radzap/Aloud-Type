//! Windows adapters for the Settings items: the HKCU Run key and the capture
//! device list. Only the platform crate tests exercise them headlessly.
use crate::items::{LoginItem, MicLister};
use aloud_type_platform_windows::audio::cpal_backend::CpalBackend;
use aloud_type_platform_windows::audio::session::Backend;
use aloud_type_platform_windows::autostart::{AutoStart, WinRunKey};
use std::io;

pub struct WinLogin(AutoStart<WinRunKey>);

impl WinLogin {
    pub fn new() -> Self {
        Self(AutoStart::new(WinRunKey))
    }
}

impl Default for WinLogin {
    fn default() -> Self {
        Self::new()
    }
}

impl LoginItem for WinLogin {
    fn is_enabled(&self) -> bool {
        self.0.is_enabled()
    }
    fn set_enabled(&mut self, enabled: bool) -> io::Result<()> {
        let exe = std::env::current_exe()?;
        self.0.set_enabled(enabled, &exe)
    }
}

/// Lists microphones when the window opens (re-enumerated on each launch).
pub struct WinMics(Vec<(String, String)>);

impl WinMics {
    pub fn new() -> Self {
        Self(
            CpalBackend
                .list_inputs()
                .into_iter()
                .map(|d| (d.id, d.name))
                .collect(),
        )
    }
}

impl Default for WinMics {
    fn default() -> Self {
        Self::new()
    }
}

impl MicLister for WinMics {
    fn list(&self) -> Vec<(String, String)> {
        self.0.clone()
    }
}
