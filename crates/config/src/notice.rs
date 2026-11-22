//! Tray notice for an invalid settings file: shown once per bad file while
//! the last good settings stay in force.
use crate::LoadStatus;

pub const INVALID_SETTINGS_NOTICE: &str = "Settings file is invalid. Using your last good settings.";

#[derive(Debug, Default)]
pub struct SettingsNotice {
    standing: bool,
}

impl SettingsNotice {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed each reload's status; returns the notice text when a file newly
    /// turns invalid (not on every reload of the same bad file).
    pub fn observe(&mut self, status: &LoadStatus) -> Option<&'static str> {
        let invalid = matches!(status, LoadStatus::Invalid(_));
        let fire = invalid && !self.standing;
        self.standing = invalid;
        fire.then_some(INVALID_SETTINGS_NOTICE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConfigFile, Settings};

    #[test]
    fn invalid_file_notifies_once_and_keeps_last_good_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let mut f = ConfigFile::<Settings>::open(&path);
        f.update(|s| s.launch_at_login = true).unwrap();
        let mut n = SettingsNotice::new();
        assert_eq!(n.observe(f.status()), None);

        std::fs::write(&path, "this is = [not toml").unwrap();
        let st = f.reload().clone();
        assert!(f.value().launch_at_login, "last good settings kept");
        assert_eq!(n.observe(&st), Some(INVALID_SETTINGS_NOTICE));
        assert_eq!(n.observe(&st), None);

        f.reload();
        std::fs::write(&path, "version = 1\nlaunch_at_login = false\n").unwrap();
        let st = f.reload().clone();
        assert_eq!(n.observe(&st), None);
        std::fs::write(&path, "[[[").unwrap();
        let st = f.reload().clone();
        assert_eq!(n.observe(&st), Some(INVALID_SETTINGS_NOTICE));
    }
}
