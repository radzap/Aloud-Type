//! Trait seams implemented by platform-windows and recognizer, faked in tests.

/// Milliseconds on a monotonic clock.
pub type Millis = u64;

/// Key events relevant to Dictation, already classified by the platform hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    DictationKeyDown,
    DictationKeyUp,
    /// Another key pressed while the Dictation key is held.
    OtherKeyDown,
}

/// Platform seam: source of Dictation key events.
pub trait Keyboard {
    fn next_event(&mut self) -> Option<KeyEvent>;
}

/// Platform seam: source of 16 kHz mono audio.
pub trait AudioSource {
    /// Take the samples captured since the last call (empty if none).
    fn read(&mut self) -> Vec<f32>;
}

/// Monotonic time source.
pub trait Clock {
    fn now(&self) -> Millis;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub text: String,
    pub start_ms: Millis,
    pub end_ms: Millis,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub text: String,
    pub words: Vec<Word>,
    pub no_speech_prob: f32,
}

/// Recognizer seam: decode a window of 16 kHz mono audio.
pub trait Recognizer {
    fn recognize(&mut self, audio: &[f32], language: &str, prompt: Option<&str>) -> Vec<Segment>;
}

/// Voice-gate seam: speech / non-speech for an audio slice.
pub trait VoiceGate {
    fn is_speech(&mut self, audio: &[f32]) -> bool;
}
