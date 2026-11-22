//! OS-free core of Aloud Type: seams (traits) and test support.
pub mod lifecycle;
pub mod seams;
pub mod testing;
pub use seams::*;

/// Minimal core driver: pulls Dictation key events from a [`Keyboard`] and
/// stamps them using a [`Clock`]. Later tickets grow this into the Dictation state machine.
pub struct Core<K: Keyboard, C: Clock> {
    keyboard: K,
    clock: C,
}

impl<K: Keyboard, C: Clock> Core<K, C> {
    pub fn new(keyboard: K, clock: C) -> Self {
        Self { keyboard, clock }
    }

    /// Drain pending key events, each stamped with the time they were observed.
    pub fn poll(&mut self) -> Vec<(KeyEvent, Millis)> {
        let now = self.clock.now();
        let mut out = Vec::new();
        while let Some(e) = self.keyboard.next_event() {
            out.push((e, now));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeClock, FakeKeyboard};

    #[test]
    fn core_observes_dictation_key_events_at_the_fake_clock_time() {
        let kb = FakeKeyboard::new();
        let clock = FakeClock::new();
        let mut core = Core::new(kb.clone(), clock.clone());

        clock.advance(250);
        kb.press(KeyEvent::DictationKeyDown);
        assert_eq!(core.poll(), vec![(KeyEvent::DictationKeyDown, 250)]);

        clock.advance(500);
        kb.press(KeyEvent::DictationKeyUp);
        assert_eq!(core.poll(), vec![(KeyEvent::DictationKeyUp, 750)]);
        assert!(core.poll().is_empty());
    }
}
