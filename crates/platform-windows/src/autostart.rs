//! Launch at login: one value in the per-user registry Run key. Off by default;
//! the app never touches the registry unless the Settings toggle is flipped.
use std::io;
use std::path::Path;

pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const VALUE_NAME: &str = "AloudType";

/// Registry seam over the per-user Run key.
pub trait RunKey {
    fn get(&self, name: &str) -> Option<String>;
    fn set(&mut self, name: &str, command: &str) -> io::Result<()>;
    /// Removing a value that is not there is not an error.
    fn remove(&mut self, name: &str) -> io::Result<()>;
}

pub struct AutoStart<R: RunKey> {
    run: R,
    name: String,
}

impl<R: RunKey> AutoStart<R> {
    pub fn new(run: R) -> Self {
        Self {
            run,
            name: VALUE_NAME.into(),
        }
    }

    pub fn with_value_name(run: R, name: &str) -> Self {
        Self {
            run,
            name: name.into(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.run.get(&self.name).is_some()
    }

    /// Enable (pointing at `exe`, quoted so spaces survive) or disable.
    pub fn set_enabled(&mut self, enabled: bool, exe: &Path) -> io::Result<()> {
        if enabled {
            self.run.set(&self.name, &format!("\"{}\"", exe.display()))
        } else {
            self.run.remove(&self.name)
        }
    }
}

/// The real HKCU Run key.
#[cfg(windows)]
pub struct WinRunKey;

#[cfg(windows)]
impl WinRunKey {
    fn key(write: bool) -> io::Result<winreg::RegKey> {
        let root = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        if write {
            root.create_subkey(RUN_KEY).map(|(k, _)| k)
        } else {
            root.open_subkey(RUN_KEY)
        }
    }
}

#[cfg(windows)]
impl RunKey for WinRunKey {
    fn get(&self, name: &str) -> Option<String> {
        Self::key(false).ok()?.get_value::<String, _>(name).ok()
    }
    fn set(&mut self, name: &str, command: &str) -> io::Result<()> {
        Self::key(true)?.set_value(name, &command)
    }
    fn remove(&mut self, name: &str) -> io::Result<()> {
        match Self::key(true)?.delete_value(name) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            r => r,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[derive(Default)]
    struct Fake(HashMap<String, String>);
    impl RunKey for Fake {
        fn get(&self, n: &str) -> Option<String> {
            self.0.get(n).cloned()
        }
        fn set(&mut self, n: &str, c: &str) -> io::Result<()> {
            self.0.insert(n.into(), c.into());
            Ok(())
        }
        fn remove(&mut self, n: &str) -> io::Result<()> {
            self.0.remove(n);
            Ok(())
        }
    }

    #[test]
    fn off_by_default_and_nothing_written() {
        let a = AutoStart::new(Fake::default());
        assert!(!a.is_enabled());
    }

    #[test]
    fn enabling_writes_the_quoted_exe_path_and_disabling_removes_it() {
        let mut a = AutoStart::new(Fake::default());
        let exe = PathBuf::from(r"C:\Program Files\Aloud Type\aloud-type.exe");
        a.set_enabled(true, &exe).unwrap();
        assert!(a.is_enabled());
        assert_eq!(
            a.run.get("AloudType").unwrap(),
            r#""C:\Program Files\Aloud Type\aloud-type.exe""#
        );
        a.set_enabled(false, &exe).unwrap();
        assert!(!a.is_enabled());
        a.set_enabled(false, &exe).unwrap(); // idempotent
    }

    #[cfg(windows)]
    #[test]
    fn real_run_key_round_trip_with_a_test_value() {
        let mut a = AutoStart::with_value_name(WinRunKey, "AloudTypeTestValue");
        let exe = PathBuf::from(r"C:\x\y.exe");
        a.set_enabled(true, &exe).unwrap();
        assert!(a.is_enabled());
        a.set_enabled(false, &exe).unwrap();
        assert!(!a.is_enabled());
    }
}
