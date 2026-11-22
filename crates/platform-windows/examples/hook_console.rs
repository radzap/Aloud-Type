//! Debug console: prints Dictation start/end from the real keyboard hook.
//! Run: cargo run -p aloud-type-platform-windows --example hook_console [remove]
//! Ctrl+C quits. With `remove`, the hook is forcibly removed after 5 s; the
//! watchdog should re-install it (reinstall count is printed) and the key
//! must keep working.
use aloud_type_core::lifecycle::{DictationEngine, LifecycleConfig};
use aloud_type_core::seams::{Clock, KeyEvent, Keyboard, Millis};
use aloud_type_platform_windows::hook::{HookConfig, KeyboardHook};
use std::rc::Rc;
use std::time::{Duration, Instant};

struct Mono(Instant);
impl Clock for Mono {
    fn now(&self) -> Millis {
        self.0.elapsed().as_millis() as Millis
    }
}

struct Shared(Rc<KeyboardHook>);
impl Keyboard for Shared {
    fn next_event(&mut self) -> Option<KeyEvent> {
        self.0.try_next()
    }
}

fn main() {
    let hook = Rc::new(KeyboardHook::start(HookConfig::default()).expect("hook"));
    let remove = std::env::args().any(|a| a == "remove");
    let t0 = Instant::now();
    let mut removed = false;
    let cfg = LifecycleConfig {
        modifier_key: true,
        ..Default::default()
    };
    let mut engine = DictationEngine::new(Shared(hook.clone()), Mono(t0), cfg);
    println!("Right Ctrl is the Dictation key. Ctrl+C to quit.");
    let mut seen = 0;
    loop {
        for e in engine.poll() {
            println!("[{:>6} ms] {:?}", t0.elapsed().as_millis(), e);
        }
        if remove && !removed && t0.elapsed() > Duration::from_secs(5) {
            println!("forcing hook removal");
            hook.simulate_forced_removal();
            removed = true;
        }
        if hook.reinstall_count() != seen {
            seen = hook.reinstall_count();
            println!("watchdog re-installed hook (count {seen})");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
