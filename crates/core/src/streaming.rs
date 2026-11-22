//! Streaming insertion: confirmed-prefix typing of recognized words, with a
//! type-on-pause fallback when the recognizer cannot keep up.
use crate::seams::*;

/// Output seam: types text into the focused text field.
pub trait TextSink {
    fn type_text(&mut self, text: &str);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamingConfig {
    /// A single decode taking longer than this means the recognizer is too slow.
    pub slow_decode_ms: Millis,
    /// In fallback mode, silence this long counts as the speaker pausing.
    pub pause_ms: Millis,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self { slow_decode_ms: 1000, pause_ms: 700 }
    }
}

/// One Dictation's Streaming insertion.
pub struct StreamingInsertion<R: Recognizer, V: VoiceGate, T: TextSink, C: Clock> {
    recognizer: R,
    gate: V,
    sink: T,
    clock: C,
    config: StreamingConfig,
    language: String,
    audio: Vec<f32>,
    previous: Vec<String>,
    typed_words: usize,
    typed_any: bool,
    fallback: bool,
    heard_speech: bool,
    last_speech_at: Millis,
}

impl<R: Recognizer, V: VoiceGate, T: TextSink, C: Clock> StreamingInsertion<R, V, T, C> {
    pub fn new(recognizer: R, gate: V, sink: T, clock: C, config: StreamingConfig, language: &str) -> Self {
        Self {
            recognizer,
            gate,
            sink,
            clock,
            config,
            language: language.into(),
            audio: Vec::new(),
            previous: Vec::new(),
            typed_words: 0,
            typed_any: false,
            fallback: false,
            heard_speech: false,
            last_speech_at: 0,
        }
    }

    /// Feed newly captured audio; may type text.
    pub fn feed(&mut self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }
        let now = self.clock.now();
        if self.gate.is_speech(samples) {
            self.heard_speech = true;
            self.last_speech_at = now;
        }
        self.audio.extend_from_slice(samples);
        if self.fallback {
            if self.heard_speech && now.saturating_sub(self.last_speech_at) >= self.config.pause_ms {
                let words = self.decode();
                self.type_words(&words, words.len());
                self.audio.clear();
                self.previous.clear();
                self.typed_words = 0;
                self.heard_speech = false;
            }
            return;
        }
        let started = self.clock.now();
        let words = self.decode();
        if self.clock.now().saturating_sub(started) > self.config.slow_decode_ms {
            self.fallback = true;
        }
        let agreed = self.previous.iter().zip(&words).take_while(|(a, b)| a == b).count();
        self.previous = words.clone();
        // Hold back the last agreed word until a later decode confirms it.
        self.type_words(&words, agreed.saturating_sub(1));
    }

    /// End the Dictation: decode what is left and type every pending word.
    pub fn end(&mut self) {
        if self.audio.is_empty() {
            return;
        }
        let words = self.decode();
        self.type_words(&words, words.len());
        self.audio.clear();
    }

    fn decode(&mut self) -> Vec<String> {
        self.recognizer
            .recognize(&self.audio, &self.language, None)
            .iter()
            .flat_map(|s| {
                if s.words.is_empty() {
                    s.text.split_whitespace().map(String::from).collect::<Vec<_>>()
                } else {
                    s.words.iter().map(|w| w.text.trim().to_string()).collect()
                }
            })
            .filter(|w| !w.is_empty())
            .collect()
    }

    fn type_words(&mut self, words: &[String], upto: usize) {
        let upto = upto.min(words.len());
        if upto <= self.typed_words {
            return;
        }
        let chunk = words[self.typed_words..upto].join(" ");
        let text = if self.typed_any { format!(" {chunk}") } else { chunk };
        self.sink.type_text(&text);
        self.typed_any = true;
        self.typed_words = upto;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeClock, FakeRecognizer, FakeVoiceGate};
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct FakeSink(Rc<RefCell<Vec<String>>>);
    impl TextSink for FakeSink {
        fn type_text(&mut self, text: &str) {
            self.0.borrow_mut().push(text.into());
        }
    }
    impl FakeSink {
        fn typed(&self) -> String {
            self.0.borrow().concat()
        }
    }

    /// Recognizer wrapper that makes each decode take `ms` on the fake clock.
    struct Timed(FakeRecognizer, FakeClock, Millis);
    impl Recognizer for Timed {
        fn recognize(&mut self, a: &[f32], l: &str, p: Option<&str>) -> Vec<Segment> {
            self.1.advance(self.2);
            self.0.recognize(a, l, p)
        }
    }

    fn seg(text: &str) -> Vec<Segment> {
        vec![Segment { text: text.into(), words: vec![], no_speech_prob: 0.0 }]
    }

    struct Rig {
        ins: StreamingInsertion<Timed, FakeVoiceGate, FakeSink, FakeClock>,
        sink: FakeSink,
        clock: FakeClock,
    }

    fn rig(script: Vec<&str>, decode_ms: Millis) -> Rig {
        let clock = FakeClock::new();
        let sink = FakeSink::default();
        let mut rec = FakeRecognizer::new();
        for s in script {
            rec = rec.then(seg(s));
        }
        let ins = StreamingInsertion::new(
            Timed(rec, clock.clone(), decode_ms),
            FakeVoiceGate(0.5),
            sink.clone(),
            clock.clone(),
            StreamingConfig::default(),
            "en",
        );
        Rig { ins, sink, clock }
    }

    const SPEECH: [f32; 4] = [0.9; 4];
    const SILENCE: [f32; 4] = [0.0; 4];

    #[test]
    fn types_only_words_two_decodes_agree_on_and_holds_back_the_last() {
        let mut r = rig(vec!["hello wor", "hello world how", "hello world how are"], 10);
        r.ins.feed(&SPEECH);
        assert_eq!(r.sink.typed(), "");
        r.ins.feed(&SPEECH); // agree on "hello": nothing but the held-back word
        assert_eq!(r.sink.typed(), "");
        r.ins.feed(&SPEECH); // agree on "hello world how": type "hello world", hold "how"
        assert_eq!(r.sink.typed(), "hello world");
    }

    #[test]
    fn later_chunks_get_one_leading_space() {
        let mut r = rig(vec!["a b", "a b c", "a b c d", "a b c d e"], 10);
        for _ in 0..4 {
            r.ins.feed(&SPEECH);
        }
        assert_eq!(*r.sink.0.borrow(), vec!["a", " b", " c"]);
    }

    #[test]
    fn ending_flushes_pending_words_without_trailing_space() {
        let mut r = rig(vec!["hello world", "hello world", "hello world again"], 10);
        r.ins.feed(&SPEECH);
        r.ins.feed(&SPEECH);
        assert_eq!(r.sink.typed(), "hello");
        r.ins.end();
        assert_eq!(r.sink.typed(), "hello world again");
    }

    #[test]
    fn slow_recognizer_falls_back_to_typing_whole_utterance_on_pause() {
        let mut r = rig(vec!["hello wor", "hello world"], 1500);
        r.ins.feed(&SPEECH); // slow decode triggers fallback
        r.ins.feed(&SPEECH);
        assert_eq!(r.sink.typed(), "");
        r.clock.advance(800);
        r.ins.feed(&SILENCE);
        assert_eq!(r.sink.typed(), "hello world");
        r.ins.end();
        assert_eq!(r.sink.typed(), "hello world");
    }

    #[test]
    fn fallback_utterances_are_separated_by_one_space() {
        let mut r = rig(vec!["one", "one two", "three"], 1500);
        r.ins.feed(&SPEECH);
        r.clock.advance(800);
        r.ins.feed(&SILENCE);
        assert_eq!(r.sink.typed(), "one two");
        r.ins.feed(&SPEECH);
        r.ins.end();
        assert_eq!(r.sink.typed(), "one two three");
    }
}
