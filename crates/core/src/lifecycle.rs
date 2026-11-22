//! Dictation lifecycle: turns Dictation key events into a Dictation that
//! starts and ends (hold, tap-to-latch, chord cancel).
use crate::seams::*;

/// Default tap threshold: a release before this many ms is a tap.
pub const DEFAULT_TAP_THRESHOLD_MS: Millis = 250;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifecycleConfig {
    /// Hidden tuning value; releases strictly under it are taps.
    pub tap_threshold_ms: Millis,
    /// True when the Dictation key is a modifier (enables the chord rule).
    pub modifier_key: bool,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self { tap_threshold_ms: DEFAULT_TAP_THRESHOLD_MS, modifier_key: false }
    }
}

/// What the engine asks the rest of the app to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    StartDictation,
    /// End the Dictation, flushing remaining words.
    EndDictation,
    /// Cancel the Dictation and discard its audio (text already typed stays).
    CancelDictation,
    /// Ask the platform to replay the modifier key-down plus the other key.
    ReplayModifierChord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    /// Key physically held; release decides tap vs hold.
    Holding,
    /// Hands-free Dictation; next key-down ends it.
    Latched,
}

pub struct DictationEngine<K: Keyboard, C: Clock> {
    keyboard: K,
    clock: C,
    config: LifecycleConfig,
    phase: Phase,
    down_at: Millis,
}

impl<K: Keyboard, C: Clock> DictationEngine<K, C> {
    pub fn new(keyboard: K, clock: C, config: LifecycleConfig) -> Self {
        Self { keyboard, clock, config, phase: Phase::Idle, down_at: 0 }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// True while a Dictation is in progress.
    pub fn dictating(&self) -> bool {
        self.phase != Phase::Idle
    }

    /// Process pending key events (timestamped now) and return the effects.
    pub fn poll(&mut self) -> Vec<Effect> {
        let now = self.clock.now();
        let mut out = Vec::new();
        while let Some(e) = self.keyboard.next_event() {
            self.handle(e, now, &mut out);
        }
        out
    }

    fn handle(&mut self, e: KeyEvent, now: Millis, out: &mut Vec<Effect>) {
        match (e, self.phase) {
            (KeyEvent::DictationKeyDown, Phase::Idle) => {
                self.phase = Phase::Holding;
                self.down_at = now;
                out.push(Effect::StartDictation);
            }
            (KeyEvent::DictationKeyDown, Phase::Latched) => {
                self.phase = Phase::Idle;
                out.push(Effect::EndDictation);
            }
            (KeyEvent::DictationKeyUp, Phase::Holding) => {
                if now.saturating_sub(self.down_at) < self.config.tap_threshold_ms {
                    self.phase = Phase::Latched;
                } else {
                    self.phase = Phase::Idle;
                    out.push(Effect::EndDictation);
                }
            }
            (KeyEvent::OtherKeyDown, Phase::Holding) if self.config.modifier_key => {
                self.phase = Phase::Idle;
                out.push(Effect::CancelDictation);
                out.push(Effect::ReplayModifierChord);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeClock, FakeKeyboard};
    use Effect::*;

    type Rig = (DictationEngine<FakeKeyboard, FakeClock>, FakeKeyboard, FakeClock);

    fn engine(modifier: bool) -> Rig {
        let (kb, clock) = (FakeKeyboard::new(), FakeClock::new());
        let cfg = LifecycleConfig { modifier_key: modifier, ..Default::default() };
        (DictationEngine::new(kb.clone(), clock.clone(), cfg), kb, clock)
    }

    #[test]
    fn holding_dictates_while_held_and_release_ends() {
        let (mut e, kb, clock) = engine(false);
        kb.press(KeyEvent::DictationKeyDown);
        assert_eq!(e.poll(), vec![StartDictation]);
        clock.advance(1000);
        assert!(e.poll().is_empty());
        assert!(e.dictating());
        kb.press(KeyEvent::DictationKeyUp);
        assert_eq!(e.poll(), vec![EndDictation]);
        assert!(!e.dictating());
    }

    #[test]
    fn tap_latches_and_next_key_down_ends_ignoring_its_release() {
        let (mut e, kb, clock) = engine(false);
        kb.press(KeyEvent::DictationKeyDown);
        e.poll();
        clock.advance(100);
        kb.press(KeyEvent::DictationKeyUp);
        assert!(e.poll().is_empty());
        assert_eq!(e.phase(), Phase::Latched);
        clock.advance(5000);
        kb.press(KeyEvent::DictationKeyDown);
        assert_eq!(e.poll(), vec![EndDictation]);
        kb.press(KeyEvent::DictationKeyUp);
        assert!(e.poll().is_empty());
        kb.press(KeyEvent::DictationKeyDown);
        assert_eq!(e.poll(), vec![StartDictation]);
    }

    #[test]
    fn threshold_edges_249_is_tap_250_is_hold() {
        for (ms, latched) in [(249, true), (250, false), (251, false)] {
            let (mut e, kb, clock) = engine(false);
            kb.press(KeyEvent::DictationKeyDown);
            e.poll();
            clock.advance(ms);
            kb.press(KeyEvent::DictationKeyUp);
            let fx = e.poll();
            assert_eq!(e.phase() == Phase::Latched, latched, "{ms} ms");
            assert_eq!(fx.contains(&EndDictation), !latched, "{ms} ms");
        }
    }

    #[test]
    fn threshold_is_configurable() {
        let (kb, clock) = (FakeKeyboard::new(), FakeClock::new());
        let cfg = LifecycleConfig { tap_threshold_ms: 400, modifier_key: false };
        let mut e = DictationEngine::new(kb.clone(), clock.clone(), cfg);
        kb.press(KeyEvent::DictationKeyDown);
        e.poll();
        clock.advance(300);
        kb.press(KeyEvent::DictationKeyUp);
        e.poll();
        assert_eq!(e.phase(), Phase::Latched);
    }

    #[test]
    fn chord_on_modifier_key_cancels_and_requests_replay() {
        let (mut e, kb, clock) = engine(true);
        kb.press(KeyEvent::DictationKeyDown);
        e.poll();
        clock.advance(80);
        kb.press(KeyEvent::OtherKeyDown);
        assert_eq!(e.poll(), vec![CancelDictation, ReplayModifierChord]);
        assert!(!e.dictating());
        clock.advance(1000);
        kb.press(KeyEvent::DictationKeyUp);
        assert!(e.poll().is_empty());
    }

    #[test]
    fn chord_also_cancels_after_threshold_while_still_held() {
        let (mut e, kb, clock) = engine(true);
        kb.press(KeyEvent::DictationKeyDown);
        e.poll();
        clock.advance(2000);
        kb.press(KeyEvent::OtherKeyDown);
        assert_eq!(e.poll(), vec![CancelDictation, ReplayModifierChord]);
    }

    #[test]
    fn chord_rule_does_not_apply_to_non_modifier_keys() {
        let (mut e, kb, _) = engine(false);
        kb.press(KeyEvent::DictationKeyDown);
        e.poll();
        kb.press(KeyEvent::OtherKeyDown);
        assert!(e.poll().is_empty());
        assert!(e.dictating());
    }

    #[test]
    fn chord_rule_does_not_apply_to_a_latched_dictation() {
        let (mut e, kb, clock) = engine(true);
        kb.press(KeyEvent::DictationKeyDown);
        e.poll();
        clock.advance(50);
        kb.press(KeyEvent::DictationKeyUp);
        e.poll();
        kb.press(KeyEvent::OtherKeyDown);
        assert!(e.poll().is_empty());
        assert_eq!(e.phase(), Phase::Latched);
    }

    #[test]
    fn other_keys_while_idle_are_ignored() {
        let (mut e, kb, _) = engine(true);
        kb.press(KeyEvent::OtherKeyDown);
        assert!(e.poll().is_empty());
    }
}
