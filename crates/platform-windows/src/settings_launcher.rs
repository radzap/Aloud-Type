//! Opens the Settings process from the tray ("Settings" menu item, double-click,
//! second launch). At most one Settings process at a time: while the child is
//! still running a further request does nothing.
use std::io;

/// A running Settings process.
pub trait Child {
    fn is_running(&mut self) -> bool;
    /// Raise the Settings window (a request while it is already open, e.g.
    /// the Dictation key pressed before setup is complete).
    fn bring_to_front(&mut self) {}
}

pub trait Spawner {
    type Child: Child;
    fn spawn(&mut self) -> io::Result<Self::Child>;
}

pub struct SettingsLauncher<S: Spawner> {
    spawner: S,
    child: Option<S::Child>,
}

impl<S: Spawner> SettingsLauncher<S> {
    pub fn new(spawner: S) -> Self {
        Self {
            spawner,
            child: None,
        }
    }

    /// Spawns Settings unless one is already open, in which case that window
    /// is brought to the front. Returns whether a new process was started.
    pub fn open(&mut self) -> io::Result<bool> {
        if let Some(c) = self.child.as_mut() {
            if c.is_running() {
                c.bring_to_front();
                return Ok(false);
            }
        }
        self.child = Some(self.spawner.spawn()?);
        Ok(true)
    }
}

/// Spawns `<this exe> --settings`.
pub struct ExeSpawner;

pub struct ProcessChild(std::process::Child);

impl Child for ProcessChild {
    fn is_running(&mut self) -> bool {
        matches!(self.0.try_wait(), Ok(None))
    }
    fn bring_to_front(&mut self) {
        raise_windows_of_process(self.0.id());
    }
}

#[cfg(windows)]
fn raise_windows_of_process(pid: u32) {
    use crate::win32::*;
    extern "system" fn each(h: Hwnd, param: isize) -> i32 {
        unsafe {
            let mut p = 0u32;
            GetWindowThreadProcessId(h, &mut p);
            if p == param as u32 && IsWindowVisible(h) != 0 {
                if IsIconic(h) != 0 {
                    ShowWindow(h, 9); // SW_RESTORE
                }
                SetForegroundWindow(h);
                return 0;
            }
        }
        1
    }
    unsafe {
        let _ = AllowSetForegroundWindow(pid);
        EnumWindows(each, pid as isize);
    }
}

#[cfg(not(windows))]
fn raise_windows_of_process(_pid: u32) {}

impl Spawner for ExeSpawner {
    type Child = ProcessChild;
    fn spawn(&mut self) -> io::Result<ProcessChild> {
        std::process::Command::new(std::env::current_exe()?)
            .arg("--settings")
            .spawn()
            .map(ProcessChild)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    thread_local!(static RAISED: Cell<u32> = const { Cell::new(0) });
    struct FakeChild(Rc<Cell<bool>>);
    impl Child for FakeChild {
        fn is_running(&mut self) -> bool {
            self.0.get()
        }
        fn bring_to_front(&mut self) {
            RAISED.with(|r| r.set(r.get() + 1));
        }
    }
    struct FakeSpawner {
        running: Rc<Cell<bool>>,
        spawned: Rc<Cell<u32>>,
        fail: bool,
    }
    impl Spawner for FakeSpawner {
        type Child = FakeChild;
        fn spawn(&mut self) -> io::Result<FakeChild> {
            if self.fail {
                return Err(io::ErrorKind::NotFound.into());
            }
            self.spawned.set(self.spawned.get() + 1);
            self.running.set(true);
            Ok(FakeChild(self.running.clone()))
        }
    }
    fn launcher(fail: bool) -> (SettingsLauncher<FakeSpawner>, Rc<Cell<bool>>, Rc<Cell<u32>>) {
        let running = Rc::new(Cell::new(false));
        let spawned = Rc::new(Cell::new(0));
        (
            SettingsLauncher::new(FakeSpawner {
                running: running.clone(),
                spawned: spawned.clone(),
                fail,
            }),
            running,
            spawned,
        )
    }

    #[test]
    fn opening_twice_while_open_starts_one_process() {
        let (mut l, _r, n) = launcher(false);
        assert!(l.open().unwrap());
        assert!(!l.open().unwrap());
        assert_eq!(n.get(), 1);
    }

    #[test]
    fn a_second_request_while_open_brings_the_window_to_the_front() {
        let (mut l, _r, _n) = launcher(false);
        RAISED.with(|r| r.set(0));
        l.open().unwrap();
        assert_eq!(
            RAISED.with(|r| r.get()),
            0,
            "a fresh window needs no raising"
        );
        l.open().unwrap();
        assert_eq!(RAISED.with(|r| r.get()), 1);
    }

    #[test]
    fn reopens_after_the_window_was_closed() {
        let (mut l, running, n) = launcher(false);
        l.open().unwrap();
        running.set(false);
        assert!(l.open().unwrap());
        assert_eq!(n.get(), 2);
    }

    #[test]
    fn spawn_failure_is_reported() {
        let (mut l, _r, _n) = launcher(true);
        assert!(l.open().is_err());
    }
}
