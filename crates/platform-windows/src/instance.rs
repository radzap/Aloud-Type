//! Single instance via a named mutex. A second launch signals a named event
//! that the first instance's waiter thread blocks on (no timers, no polling),
//! then exits. The first instance turns that signal into an open-Settings
//! request.
use std::thread;

type Handle = isize;
const ERROR_ALREADY_EXISTS: u32 = 183;
const INFINITE: u32 = 0xFFFF_FFFF;
const WAIT_OBJECT_0: u32 = 0;

#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(attrs: *const u8, initial_owner: i32, name: *const u16) -> Handle;
    fn CreateEventW(attrs: *const u8, manual: i32, initial: i32, name: *const u16) -> Handle;
    fn SetEvent(h: Handle) -> i32;
    fn WaitForSingleObject(h: Handle, ms: u32) -> u32;
    fn GetLastError() -> u32;
    fn CloseHandle(h: Handle) -> i32;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Keeps the instance mutex alive for the life of the first instance.
pub struct InstanceGuard {
    _mutex: Handle,
    event: Handle,
}

pub enum Launch {
    /// This is the resident process. Call [`InstanceGuard::on_open_settings`].
    First(InstanceGuard),
    /// The first instance has been asked to open Settings; exit now.
    Second,
}

/// `name` namespaces the objects (tests use their own so they never clash with
/// a real running app).
pub fn acquire(name: &str) -> std::io::Result<Launch> {
    let mutex_name = wide(&format!("Local\\{name}.Instance"));
    let event_name = wide(&format!("Local\\{name}.OpenSettings"));
    unsafe {
        let mutex = CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr());
        if mutex == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let already = GetLastError() == ERROR_ALREADY_EXISTS;
        let event = CreateEventW(std::ptr::null(), 0, 0, event_name.as_ptr());
        if event == 0 {
            return Err(std::io::Error::last_os_error());
        }
        if already {
            SetEvent(event);
            CloseHandle(event);
            CloseHandle(mutex);
            Ok(Launch::Second)
        } else {
            Ok(Launch::First(InstanceGuard {
                _mutex: mutex,
                event,
            }))
        }
    }
}

impl InstanceGuard {
    /// Runs `f` on a background thread each time a second launch asks for
    /// Settings. The thread sleeps in the kernel between requests.
    pub fn on_open_settings(&self, f: impl Fn() + Send + 'static) {
        let event = self.event;
        thread::spawn(move || loop {
            if unsafe { WaitForSingleObject(event, INFINITE) } != WAIT_OBJECT_0 {
                return;
            }
            f();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn second_launch_asks_the_first_to_open_settings_and_exits() {
        let name = format!("AloudTypeTest{}", std::process::id());
        let Launch::First(guard) = acquire(&name).unwrap() else {
            panic!("expected first")
        };
        let (tx, rx) = channel();
        guard.on_open_settings(move || {
            let _ = tx.send(());
        });
        assert!(matches!(acquire(&name).unwrap(), Launch::Second));
        rx.recv_timeout(Duration::from_secs(5))
            .expect("first instance was asked to open Settings");
    }
}
