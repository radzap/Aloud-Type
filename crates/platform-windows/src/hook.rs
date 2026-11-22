//! `WH_KEYBOARD_LL` hook on its own thread. The callback only classifies
//! (see `classify`), swallows or passes, and posts to channels; injection of
//! the chord replay runs on a separate worker so the callback never blocks.
use crate::win32::{GetMessageW, Msg};
use crate::hook_health::HookHealth;
use crate::classify::{should_reinstall, Classifier, Inject, RawKey, INJECT_TAG, VK_RCONTROL};
use aloud_type_core::{KeyEvent, Keyboard};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

type Hhook = isize;
/// Called after the hook posts an event or its health changes, so a sleeping host wakes.
pub type Notify = Arc<dyn Fn() + Send + Sync>;
const WH_KEYBOARD_LL: i32 = 13;
/// How often the hook thread wakes to check the hook is still installed.
const WATCHDOG_TICK_MS: u32 = 5000;
const WM_TIMER: u32 = 0x0113;
const WM_QUIT: u32 = 0x0012;
const WM_APP: u32 = 0x8000;
const LLKHF_INJECTED: u32 = 0x10;
const INPUT_KEYBOARD: u32 = 1;
const KEYEVENTF_EXTENDEDKEY: u32 = 1;
const KEYEVENTF_KEYUP: u32 = 2;

#[repr(C)]
struct KbdLlHookStruct {
    vk_code: u32,
    scan_code: u32,
    flags: u32,
    time: u32,
    extra_info: usize,
}
#[repr(C)]
struct KeybdInput {
    vk: u16,
    scan: u16,
    flags: u32,
    time: u32,
    extra_info: usize,
}
#[repr(C)]
struct Input {
    kind: u32,
    ki: KeybdInput,
    _pad: [usize; 1], // INPUT's union is sized by MOUSEINPUT
}

#[link(name = "user32")]
extern "system" {
    fn SetWindowsHookExW(id: i32, f: unsafe extern "system" fn(i32, usize, isize) -> isize, module: isize, thread: u32) -> Hhook;
    fn UnhookWindowsHookEx(h: Hhook) -> i32;
    fn CallNextHookEx(h: Hhook, code: i32, w: usize, l: isize) -> isize;
    fn SetTimer(hwnd: isize, id: usize, ms: u32, f: usize) -> usize;
    fn PostThreadMessageW(thread: u32, msg: u32, w: usize, l: isize) -> i32;
    fn SendInput(n: u32, inputs: *const Input, size: i32) -> u32;
}
#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentThreadId() -> u32;
}

struct CbState {
    classifier: Classifier,
    events: Sender<KeyEvent>,
    inject: Sender<Vec<Inject>>,
    notify: Notify,
}

thread_local! {
    static STATE: RefCell<Option<CbState>> = const { RefCell::new(None) };
    static HOOK: RefCell<Hhook> = const { RefCell::new(0) };
}

unsafe extern "system" fn callback(code: i32, w: usize, l: isize) -> isize {
    if code == 0 && l != 0 {
        let k = &*(l as *const KbdLlHookStruct);
        let raw = RawKey {
            vk: k.vk_code,
            up: matches!(w, 0x101 | 0x105),
            injected: k.flags & LLKHF_INJECTED != 0,
            extra_info: k.extra_info,
        };
        // try_borrow_mut: never panic or block inside the callback.
        let swallow = STATE.with(|s| match s.try_borrow_mut() {
            Ok(mut s) => match s.as_mut() {
                Some(st) => {
                    let d = st.classifier.process(raw);
                    if let Some(e) = d.post {
                        let _ = st.events.send(e);
                        (st.notify)();
                    }
                    if !d.inject.is_empty() {
                        let _ = st.inject.send(d.inject);
                    }
                    d.swallow
                }
                None => false,
            },
            Err(_) => false,
        });
        if swallow {
            return 1;
        }
    }
    CallNextHookEx(0, code, w, l)
}

fn install() -> bool {
    let h = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, callback, 0, 0) };
    HOOK.with(|c| *c.borrow_mut() = h);
    h != 0
}

fn uninstall() {
    HOOK.with(|c| {
        let h = std::mem::take(&mut *c.borrow_mut());
        if h != 0 {
            unsafe { UnhookWindowsHookEx(h) };
        }
    });
}

fn send(ops: &[Inject]) {
    let inputs: Vec<Input> = ops
        .iter()
        .map(|op| {
            let (vk, up) = match *op {
                Inject::Down(v) => (v, false),
                Inject::Up(v) => (v, true),
            };
            let ext = matches!(vk, 0xA3 | 0xA5 | 0x5B | 0x5C);
            let flags = (if up { KEYEVENTF_KEYUP } else { 0 }) | (if ext { KEYEVENTF_EXTENDEDKEY } else { 0 });
            Input { kind: INPUT_KEYBOARD, ki: KeybdInput { vk: vk as u16, scan: 0, flags, time: 0, extra_info: INJECT_TAG }, _pad: [0] }
        })
        .collect();
    unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<Input>() as i32) };
}

#[derive(Debug, Clone, Copy)]
pub struct HookConfig {
    pub dictation_vk: u32,
    /// Re-install the hook this often (when the key is not held).
    pub watchdog_interval: Duration,
}

impl Default for HookConfig {
    fn default() -> Self {
        Self { dictation_vk: VK_RCONTROL, watchdog_interval: Duration::from_secs(10) }
    }
}

const SIMULATE_REMOVAL: u32 = WM_APP + 1;
const REINSTALL_NOW: u32 = WM_APP + 2;
const SET_SUSPENDED: u32 = WM_APP + 3;
const SET_KEYS: u32 = WM_APP + 4;

/// Handle to the running hook thread; implements the core `Keyboard` seam.
pub struct KeyboardHook {
    rx: Receiver<KeyEvent>,
    thread_id: u32,
    reinstalls: Arc<AtomicU32>,
    health: Arc<HookHealth>,
    pending_keys: Arc<std::sync::Mutex<Vec<u32>>>,
}

impl KeyboardHook {
    pub fn start(cfg: HookConfig) -> Result<Self, String> {
        Self::start_notifying(cfg, Arc::new(|| {}))
    }

    /// Like `start`, calling `notify` whenever an event is posted or the hook's health changes.
    pub fn start_notifying(cfg: HookConfig, notify: Notify) -> Result<Self, String> {
        let (ev_tx, ev_rx) = channel();
        let (inj_tx, inj_rx) = channel::<Vec<Inject>>();
        std::thread::spawn(move || {
            while let Ok(ops) = inj_rx.recv() {
                send(&ops);
            }
        });
        let reinstalls = Arc::new(AtomicU32::new(0));
        let counter = reinstalls.clone();
        let health = Arc::new(HookHealth::default());
        let thread_health = health.clone();
        let pending_keys = Arc::new(std::sync::Mutex::new(Vec::<u32>::new()));
        let thread_pending = pending_keys.clone();
        let (ready_tx, ready_rx) = channel::<Result<u32, String>>();
        std::thread::spawn(move || {
            STATE.with(|s| {
                *s.borrow_mut() = Some(CbState { classifier: Classifier::new(cfg.dictation_vk), events: ev_tx, inject: inj_tx, notify: notify.clone() })
            });
            if !install() {
                let _ = ready_tx.send(Err("SetWindowsHookExW failed".into()));
                return;
            }
            let _ = ready_tx.send(Ok(unsafe { GetCurrentThreadId() }));
            // Watchdog tick: slow on purpose, an idle app should hardly wake at all.
            unsafe { SetTimer(0, 1, WATCHDOG_TICK_MS.min(cfg.watchdog_interval.as_millis() as u32), 0) };
            let started = Instant::now();
            let mut last_install = 0u64;
            let mut msg = Msg::default();
            while unsafe { GetMessageW(&mut msg, 0, 0, 0) } > 0 {
                match msg.message {
                    WM_TIMER => {
                        let now = started.elapsed().as_millis() as u64;
                        let held = STATE.with(|s| s.borrow().as_ref().is_some_and(|st| st.classifier.held()));
                        if should_reinstall(now, last_install, cfg.watchdog_interval.as_millis() as u64, held) {
                            uninstall();
                            let ok = install();
                            thread_health.record_install(ok);
                            notify();
                            if ok {
                                counter.fetch_add(1, Ordering::Relaxed);
                            }
                            last_install = now;
                        }
                    }
                    REINSTALL_NOW => {
                        uninstall();
                        let ok = install();
                        thread_health.record_install(ok);
                        notify();
                        if ok {
                            counter.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    SIMULATE_REMOVAL => uninstall(),
                    SET_SUSPENDED => STATE.with(|s| {
                        if let Some(st) = s.borrow_mut().as_mut() {
                            st.classifier.set_suspended(msg.wparam != 0);
                        }
                    }),
                    SET_KEYS => {
                        let keys = thread_pending.lock().map(|k| k.clone()).unwrap_or_default();
                        if !keys.is_empty() {
                            STATE.with(|s| {
                                if let Some(st) = s.borrow_mut().as_mut() {
                                    st.classifier.set_keys(&keys);
                                }
                            });
                        }
                    }
                    _ => {}
                }
            }
            uninstall();
        });
        let thread_id = ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Self { rx: ev_rx, thread_id, reinstalls, health, pending_keys })
    }

    /// Debug aid: remove the hook the way Windows would, silently.
    pub fn simulate_forced_removal(&self) {
        unsafe { PostThreadMessageW(self.thread_id, SIMULATE_REMOVAL, 0, 0) };
    }

    /// Re-installs the hook now (after resume from sleep).
    pub fn reinstall_now(&self) {
        unsafe { PostThreadMessageW(self.thread_id, REINSTALL_NOW, 0, 0) };
    }

    /// True while the last (re)install failed: the tray shows
    /// "Dictation key unavailable".
    pub fn key_unavailable(&self) -> bool {
        self.health.key_unavailable()
    }

    /// Rebinds the Dictation key (one key, or a combination) without a restart.
    pub fn set_keys(&self, vks: &[u32]) {
        if let Ok(mut k) = self.pending_keys.lock() {
            *k = vks.to_vec();
        }
        unsafe { PostThreadMessageW(self.thread_id, SET_KEYS, 0, 0) };
    }

    /// While suspended every key passes through (Settings is capturing a key).
    pub fn set_suspended(&self, suspended: bool) {
        unsafe { PostThreadMessageW(self.thread_id, SET_SUSPENDED, suspended as usize, 0) };
    }

    /// Non-blocking poll usable through a shared reference.
    pub fn try_next(&self) -> Option<KeyEvent> {
        self.rx.try_recv().ok()
    }

    /// How many times the watchdog has re-installed the hook.
    pub fn reinstall_count(&self) -> u32 {
        self.reinstalls.load(Ordering::Relaxed)
    }
}

impl Keyboard for KeyboardHook {
    fn next_event(&mut self) -> Option<KeyEvent> {
        self.try_next()
    }
}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };
    }
}
