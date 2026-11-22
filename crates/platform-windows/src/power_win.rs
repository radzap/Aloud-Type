//! Sleep/resume notifications from Windows via a hidden message window on its
//! own thread. Events are sent on a channel; the controller applies
//! `aloud_type_core::power::on_power` to them.
use aloud_type_core::power::PowerEvent;
use std::sync::mpsc::{channel, Receiver, Sender};

const WM_POWERBROADCAST: u32 = 0x0218;
const PBT_APMSUSPEND: usize = 0x4;
const PBT_APMRESUMESUSPEND: usize = 0x7;
const PBT_APMRESUMEAUTOMATIC: usize = 0x12;

/// Maps a `WM_POWERBROADCAST` wParam to the events the app cares about.
pub fn map_power_broadcast(wparam: usize) -> Option<PowerEvent> {
    match wparam {
        PBT_APMSUSPEND => Some(PowerEvent::Suspend),
        PBT_APMRESUMESUSPEND | PBT_APMRESUMEAUTOMATIC => Some(PowerEvent::Resume),
        _ => None,
    }
}

use crate::win32::{DispatchMessageW, GetMessageW, Hwnd, Msg};
#[repr(C)]
struct WndClassW {
    style: u32,
    proc_: unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize,
    cls_extra: i32,
    wnd_extra: i32,
    instance: isize,
    icon: isize,
    cursor: isize,
    background: isize,
    menu: *const u16,
    class_name: *const u16,
}
#[link(name = "user32")]
extern "system" {
    fn RegisterClassW(c: *const WndClassW) -> u16;
    fn CreateWindowExW(
        ex: u32,
        class: *const u16,
        name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: Hwnd,
        menu: isize,
        inst: isize,
        param: *const u8,
    ) -> Hwnd;
    fn DefWindowProcW(h: Hwnd, m: u32, w: usize, l: isize) -> isize;
    fn PostMessageW(h: Hwnd, m: u32, w: usize, l: isize) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(n: *const u16) -> isize;
}

type Notify = std::sync::Arc<dyn Fn() + Send + Sync>;

thread_local! {
    static TX: std::cell::RefCell<Option<(Sender<PowerEvent>, Notify)>> = const { std::cell::RefCell::new(None) };
}

unsafe extern "system" fn proc_(h: Hwnd, m: u32, w: usize, l: isize) -> isize {
    if m == WM_POWERBROADCAST {
        if let Some(e) = map_power_broadcast(w) {
            TX.with(|t| {
                if let Some((t, notify)) = t.borrow().as_ref() {
                    let _ = t.send(e);
                    notify();
                }
            });
        }
        return 1; // TRUE
    }
    DefWindowProcW(h, m, w, l)
}

pub struct PowerWatcher {
    pub events: Receiver<PowerEvent>,
    hwnd: Hwnd,
}

impl PowerWatcher {
    pub fn start() -> std::io::Result<Self> {
        Self::start_notifying(std::sync::Arc::new(|| {}))
    }

    /// Like `start`, calling `notify` after each power event is queued.
    pub fn start_notifying(notify: Notify) -> std::io::Result<Self> {
        let (tx, rx) = channel();
        let (ready_tx, ready_rx) = channel::<Hwnd>();
        std::thread::spawn(move || unsafe {
            TX.with(|t| *t.borrow_mut() = Some((tx, notify)));
            let name: Vec<u16> = "AloudTypePower\0".encode_utf16().collect();
            let inst = GetModuleHandleW(std::ptr::null());
            let wc = WndClassW {
                style: 0,
                proc_,
                cls_extra: 0,
                wnd_extra: 0,
                instance: inst,
                icon: 0,
                cursor: 0,
                background: 0,
                menu: std::ptr::null(),
                class_name: name.as_ptr(),
            };
            RegisterClassW(&wc);
            // Top-level (not message-only) so it receives broadcasts.
            let h = CreateWindowExW(
                0x80,
                name.as_ptr(),
                std::ptr::null(),
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                inst,
                std::ptr::null(),
            );
            let _ = ready_tx.send(h);
            if h == 0 {
                return;
            }
            let mut m = Msg::default();
            while GetMessageW(&mut m, 0, 0, 0) > 0 {
                DispatchMessageW(&m);
            }
        });
        let hwnd = ready_rx.recv().unwrap_or(0);
        if hwnd == 0 {
            return Err(std::io::Error::other("power window creation failed"));
        }
        Ok(Self { events: rx, hwnd })
    }

    /// Test/debug aid: deliver a notification as Windows would.
    pub fn simulate(&self, wparam: usize) {
        unsafe { PostMessageW(self.hwnd, WM_POWERBROADCAST, wparam, 0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn maps_suspend_and_both_resume_notifications() {
        assert_eq!(map_power_broadcast(0x4), Some(PowerEvent::Suspend));
        assert_eq!(map_power_broadcast(0x12), Some(PowerEvent::Resume));
        assert_eq!(map_power_broadcast(0x7), Some(PowerEvent::Resume));
        assert_eq!(map_power_broadcast(0xA), None);
    }

    #[test]
    fn window_forwards_power_messages_on_the_channel() {
        let w = PowerWatcher::start().unwrap();
        w.simulate(PBT_APMSUSPEND);
        w.simulate(PBT_APMRESUMEAUTOMATIC);
        assert_eq!(
            w.events.recv_timeout(Duration::from_secs(2)),
            Ok(PowerEvent::Suspend)
        );
        assert_eq!(
            w.events.recv_timeout(Duration::from_secs(2)),
            Ok(PowerEvent::Resume)
        );
    }
}
