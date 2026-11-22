//! Reusable fakes for the core seams. Cloneable fakes share state so a test
//! can keep a handle while the core owns another.
use crate::seams::*;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

#[derive(Clone, Default)]
pub struct FakeKeyboard(Rc<RefCell<VecDeque<KeyEvent>>>);

impl FakeKeyboard {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn press(&self, e: KeyEvent) {
        self.0.borrow_mut().push_back(e);
    }
}

impl Keyboard for FakeKeyboard {
    fn next_event(&mut self) -> Option<KeyEvent> {
        self.0.borrow_mut().pop_front()
    }
}

#[derive(Clone, Default)]
pub struct FakeAudioSource(Rc<RefCell<Vec<f32>>>);

impl FakeAudioSource {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn push(&self, samples: &[f32]) {
        self.0.borrow_mut().extend_from_slice(samples);
    }
}

impl AudioSource for FakeAudioSource {
    fn read(&mut self) -> Vec<f32> {
        std::mem::take(&mut *self.0.borrow_mut())
    }
}

#[derive(Clone, Default)]
pub struct FakeClock(Rc<Cell<Millis>>);

impl FakeClock {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn advance(&self, ms: Millis) {
        self.0.set(self.0.get() + ms);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Millis {
        self.0.get()
    }
}

/// Returns scripted segment lists in order, then empty results.
#[derive(Default)]
pub struct FakeRecognizer {
    script: VecDeque<Vec<Segment>>,
    /// (audio length, language, prompt) of each call.
    pub calls: Vec<(usize, String, Option<String>)>,
}

impl FakeRecognizer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn then(mut self, segments: Vec<Segment>) -> Self {
        self.script.push_back(segments);
        self
    }
}

impl Recognizer for FakeRecognizer {
    fn recognize(&mut self, audio: &[f32], language: &str, prompt: Option<&str>) -> Vec<Segment> {
        self.calls
            .push((audio.len(), language.into(), prompt.map(Into::into)));
        self.script.pop_front().unwrap_or_default()
    }
}

/// Voice gate that reports speech when any sample magnitude exceeds a threshold.
pub struct FakeVoiceGate(pub f32);

impl VoiceGate for FakeVoiceGate {
    fn is_speech(&mut self, audio: &[f32]) -> bool {
        audio.iter().any(|s| s.abs() > self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_source_drains_pushed_samples() {
        let mut a = FakeAudioSource::new();
        a.push(&[0.1, 0.2]);
        assert_eq!(a.read(), vec![0.1, 0.2]);
        assert!(a.read().is_empty());
    }

    #[test]
    fn recognizer_replays_script_and_records_calls() {
        let seg = Segment {
            text: "hi".into(),
            words: vec![],
            no_speech_prob: 0.0,
        };
        let mut r = FakeRecognizer::new().then(vec![seg.clone()]);
        assert_eq!(r.recognize(&[0.0; 4], "en", Some("p")), vec![seg]);
        assert!(r.recognize(&[], "en", None).is_empty());
        assert_eq!(r.calls[0], (4, "en".to_string(), Some("p".to_string())));
    }

    #[test]
    fn voice_gate_thresholds() {
        let mut g = FakeVoiceGate(0.5);
        assert!(g.is_speech(&[0.9]));
        assert!(!g.is_speech(&[0.1]));
    }
}
