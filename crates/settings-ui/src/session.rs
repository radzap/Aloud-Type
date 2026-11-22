//! Rebinding session: ties key capture, the settings file and the resident
//! process together. Works whether or not the resident process is running.
use crate::capture::{Capture, CaptureState};
use crate::ipc::{Command, ResidentLink, Transport};
use aloud_type_config::{ConfigFile, Settings};

pub struct RebindSession<T: Transport> {
    config: ConfigFile<Settings>,
    link: ResidentLink<T>,
    capture: Option<Capture>,
    save_error: Option<String>,
}

impl<T: Transport> RebindSession<T> {
    pub fn new(config: ConfigFile<Settings>, transport: T) -> Self {
        Self { config, link: ResidentLink::new(transport), capture: None, save_error: None }
    }

    pub fn dictation_key(&self) -> &[String] {
        &self.config.value().dictation_key
    }

    pub fn save_error(&self) -> Option<&str> {
        self.save_error.as_deref()
    }

    pub fn is_capturing(&self) -> bool {
        self.capture.is_some()
    }

    /// The user clicked the field: suspend the resident hook and listen.
    pub fn begin_capture(&mut self, now_ms: u64) {
        if self.capture.is_none() {
            self.link.send(Command::SuspendKeys);
            self.capture = Some(Capture::start(now_ms));
        }
        self.save_error = None;
    }

    pub fn key_down(&mut self, vk: u32) -> Option<CaptureState> {
        let st = self.capture.as_mut()?.key_down(vk);
        Some(self.after(st))
    }

    pub fn key_up(&mut self, vk: u32, now_ms: u64) -> Option<CaptureState> {
        let st = self.capture.as_mut()?.key_up(vk, now_ms);
        Some(self.after(st))
    }

    pub fn tick(&mut self, now_ms: u64) -> Option<CaptureState> {
        let st = self.capture.as_mut()?.tick(now_ms);
        Some(self.after(st))
    }

    fn after(&mut self, st: CaptureState) -> CaptureState {
        match &st {
            CaptureState::Capturing { .. } => {}
            CaptureState::Cancelled(_) => self.finish(),
            CaptureState::Done { keys, .. } => {
                let keys = keys.clone();
                match self.config.update(|s| s.dictation_key = keys) {
                    Ok(()) => {
                        self.link.send(Command::ReloadSettings);
                    }
                    Err(e) => self.save_error = Some(e.to_string()),
                }
                self.finish();
            }
        }
        st
    }

    fn finish(&mut self) {
        if self.capture.take().is_some() {
            self.link.send(Command::ResumeKeys);
        }
    }
}

impl<T: Transport> Drop for RebindSession<T> {
    fn drop(&mut self) {
        self.finish(); // window closed mid-capture: never leave the hook suspended
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::io;
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct Fake {
        log: Rc<RefCell<Vec<String>>>,
        down: bool,
    }
    impl Transport for Fake {
        fn request(&mut self, line: &str) -> io::Result<String> {
            if self.down {
                return Err(io::ErrorKind::NotFound.into());
            }
            self.log.borrow_mut().push(line.to_string());
            Ok("ok\n".into())
        }
    }

    fn session(fake: Fake) -> (RebindSession<Fake>, tempfile::TempDir, std::path::PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.toml");
        (RebindSession::new(ConfigFile::open(&p), fake), d, p)
    }

    const A: u32 = 0x41;
    const ESC: u32 = 0x1B;
    const F9: u32 = 0x78;

    #[test]
    fn rebinding_suspends_saves_reloads_and_resumes() {
        let fake = Fake::default();
        let log = fake.log.clone();
        let (mut s, _d, p) = session(fake);
        s.begin_capture(0);
        assert_eq!(*log.borrow(), ["suspend-keys"]);
        s.key_down(F9);
        s.key_up(F9, 100);
        assert_eq!(*log.borrow(), ["suspend-keys", "reload-settings", "resume-keys"]);
        assert!(!s.is_capturing());
        assert_eq!(s.dictation_key(), ["F9".to_string()]);
        let reread = ConfigFile::<Settings>::open(&p);
        assert_eq!(reread.value().dictation_key, vec!["F9".to_string()]);
    }

    #[test]
    fn escape_resumes_without_saving() {
        let fake = Fake::default();
        let log = fake.log.clone();
        let (mut s, _d, p) = session(fake);
        s.begin_capture(0);
        s.key_down(ESC);
        assert_eq!(*log.borrow(), ["suspend-keys", "resume-keys"]);
        assert!(!p.exists());
        assert_eq!(s.dictation_key(), ["RightCtrl".to_string()]);
    }

    #[test]
    fn rejected_key_keeps_the_hook_suspended_and_the_old_binding() {
        let fake = Fake::default();
        let log = fake.log.clone();
        let (mut s, _d, p) = session(fake);
        s.begin_capture(0);
        s.key_down(A);
        s.key_up(A, 10);
        assert!(s.is_capturing());
        assert_eq!(*log.borrow(), ["suspend-keys"]);
        assert!(!p.exists());
    }

    #[test]
    fn works_when_the_resident_process_is_not_running() {
        let (mut s, _d, p) = session(Fake { down: true, ..Default::default() });
        s.begin_capture(0);
        s.key_down(F9);
        s.key_up(F9, 100);
        assert_eq!(ConfigFile::<Settings>::open(&p).value().dictation_key, vec!["F9".to_string()]);
    }

    #[test]
    fn timeout_resumes() {
        let fake = Fake::default();
        let log = fake.log.clone();
        let (mut s, _d, _p) = session(fake);
        s.begin_capture(0);
        s.tick(10_000);
        assert_eq!(*log.borrow(), ["suspend-keys", "resume-keys"]);
    }

    #[test]
    fn closing_mid_capture_resumes_the_hook() {
        let fake = Fake::default();
        let log = fake.log.clone();
        let (mut s, _d, _p) = session(fake);
        s.begin_capture(0);
        drop(s);
        assert_eq!(*log.borrow(), ["suspend-keys", "resume-keys"]);
    }
}
