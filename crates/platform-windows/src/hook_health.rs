//! Whether the Dictation key can work. Fed by hook (re)install results; read
//! by the controller to show "Dictation key unavailable" in the tray.
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Default)]
pub struct HookHealth {
    unavailable: AtomicBool,
}

impl HookHealth {
    /// Records the outcome of an install attempt; a later success clears the fault.
    pub fn record_install(&self, ok: bool) {
        self.unavailable.store(!ok, Ordering::SeqCst);
    }
    pub fn key_unavailable(&self) -> bool {
        self.unavailable.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_reinstall_marks_the_key_unavailable_until_a_later_success() {
        let h = HookHealth::default();
        assert!(!h.key_unavailable());
        h.record_install(false);
        assert!(h.key_unavailable());
        h.record_install(true);
        assert!(!h.key_unavailable());
    }
}
