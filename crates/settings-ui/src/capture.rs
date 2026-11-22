//! Click-to-capture state machine for the Dictation key (spec "Rebind capture").
//! Pure: driven by key events and a clock supplied by the caller.
use crate::keys::{canonical, name_for_vk};
use crate::rules::{validate, Verdict};

pub const CAPTURE_TIMEOUT_MS: u64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureState {
    /// Waiting for keys; `held` shows the keys currently down.
    Capturing { held: Vec<String>, message: Option<String> },
    Done { keys: Vec<String>, warning: Option<String> },
    Cancelled(CancelReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelReason {
    Escape,
    Timeout,
}

pub struct Capture {
    started_ms: u64,
    held: Vec<String>,
    peak: Vec<String>,
    message: Option<String>,
    result: Option<CaptureState>,
}

impl Capture {
    pub fn start(now_ms: u64) -> Self {
        Capture { started_ms: now_ms, held: vec![], peak: vec![], message: None, result: None }
    }

    pub fn state(&self) -> CaptureState {
        self.result.clone().unwrap_or(CaptureState::Capturing {
            held: canonical(&self.held),
            message: self.message.clone(),
        })
    }

    pub fn is_finished(&self) -> bool {
        self.result.is_some()
    }

    pub fn key_down(&mut self, vk: u32) -> CaptureState {
        if self.result.is_some() {
            return self.state();
        }
        let Some(name) = name_for_vk(vk) else { return self.state() };
        if name == "Escape" && self.held.is_empty() {
            self.result = Some(CaptureState::Cancelled(CancelReason::Escape));
            return self.state();
        }
        if !self.held.contains(&name) {
            self.held.push(name);
        }
        self.message = None;
        if self.held.len() >= self.peak.len() {
            self.peak = self.held.clone();
        }
        self.state()
    }

    pub fn key_up(&mut self, vk: u32, now_ms: u64) -> CaptureState {
        if self.result.is_some() {
            return self.state();
        }
        let Some(name) = name_for_vk(vk) else { return self.state() };
        self.held.retain(|k| *k != name);
        if self.held.is_empty() && !self.peak.is_empty() {
            let keys = canonical(&self.peak);
            self.peak.clear();
            match validate(&keys) {
                Verdict::Reject(why) => {
                    self.message = Some(why);
                    self.started_ms = now_ms; // capture continues with a fresh timeout
                }
                Verdict::Accept => self.result = Some(CaptureState::Done { keys, warning: None }),
                Verdict::Warn(w) => self.result = Some(CaptureState::Done { keys, warning: Some(w) }),
            }
        }
        self.state()
    }

    /// Call periodically (or at the deadline) to apply the 10 second timeout.
    pub fn tick(&mut self, now_ms: u64) -> CaptureState {
        if self.result.is_none()
            && self.held.is_empty()
            && now_ms.saturating_sub(self.started_ms) >= CAPTURE_TIMEOUT_MS
        {
            self.result = Some(CaptureState::Cancelled(CancelReason::Timeout));
        }
        self.state()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const RCTRL: u32 = 0xA3;
    const LCTRL: u32 = 0xA2;
    const A: u32 = 0x41;
    const ESC: u32 = 0x1B;
    const Q: u32 = 0x51;

    fn s(a: &[&str]) -> Vec<String> {
        a.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn single_key_is_recorded_on_release() {
        let mut c = Capture::start(0);
        assert!(matches!(c.key_down(RCTRL), CaptureState::Capturing { .. }));
        assert_eq!(c.key_up(RCTRL, 300), CaptureState::Done { keys: s(&["RightCtrl"]), warning: None });
    }
    #[test]
    fn combination_is_the_peak_set_even_if_released_in_any_order() {
        let mut c = Capture::start(0);
        c.key_down(LCTRL);
        c.key_down(Q);
        c.key_up(LCTRL, 100);
        assert_eq!(c.key_up(Q, 120), CaptureState::Done { keys: s(&["LeftCtrl", "Q"]), warning: None });
    }
    #[test]
    fn rejected_key_shows_reason_and_capture_continues() {
        let mut c = Capture::start(0);
        c.key_down(A);
        match c.key_up(A, 50) {
            CaptureState::Capturing { message: Some(m), .. } => assert!(m.contains("typing")),
            o => panic!("{o:?}"),
        }
        c.key_down(RCTRL);
        assert_eq!(c.key_up(RCTRL, 90), CaptureState::Done { keys: s(&["RightCtrl"]), warning: None });
    }
    #[test]
    fn warned_key_is_accepted_with_the_warning() {
        let mut c = Capture::start(0);
        c.key_down(0xA5);
        match c.key_up(0xA5, 10) {
            CaptureState::Done { warning: Some(_), keys } => assert_eq!(keys, s(&["RightAlt"])),
            o => panic!("{o:?}"),
        }
    }
    #[test]
    fn escape_cancels() {
        let mut c = Capture::start(0);
        assert_eq!(c.key_down(ESC), CaptureState::Cancelled(CancelReason::Escape));
    }
    #[test]
    fn ten_seconds_without_a_binding_cancels() {
        let mut c = Capture::start(1000);
        assert!(matches!(c.tick(10_999), CaptureState::Capturing { .. }));
        assert_eq!(c.tick(11_000), CaptureState::Cancelled(CancelReason::Timeout));
    }
    #[test]
    fn rejection_restarts_the_timeout() {
        let mut c = Capture::start(0);
        c.key_down(A);
        c.key_up(A, 9_000);
        assert!(matches!(c.tick(18_000), CaptureState::Capturing { .. }));
        assert_eq!(c.tick(19_000), CaptureState::Cancelled(CancelReason::Timeout));
    }
    #[test]
    fn finished_capture_ignores_later_keys() {
        let mut c = Capture::start(0);
        c.key_down(ESC);
        assert_eq!(c.key_down(RCTRL), CaptureState::Cancelled(CancelReason::Escape));
    }
}
