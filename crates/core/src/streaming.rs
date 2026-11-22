//! Streaming insertion: confirmed-prefix typing of recognized words, with a
//! type-on-pause fallback when the recognizer cannot keep up.
use crate::seams::*;
use crate::speech_filter::should_drop;

/// Amber Typing fault raised by the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypingFault {
    ModelTooSlow,
}

impl TypingFault {
    pub fn message(self) -> &'static str {
        match self {
            TypingFault::ModelTooSlow => "Speech model is too slow for this PC. Choose a smaller model in Settings.",
        }
    }
}

const SAMPLES_PER_MS: u64 = 16;

/// Output seam: types text into the focused text field.
pub trait TextSink {
    fn type_text(&mut self, text: &str);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamingConfig {
    /// A single decode taking longer than this means the recognizer is too slow.
    pub slow_decode_ms: Millis,
    /// In fallback mode, silence this long counts as the speaker pausing.
    pub pause_ms: Millis,
    /// Segments at or above this no-speech probability are dropped.
    pub no_speech_threshold: f32,
    /// A single utterance is capped at this many 16 kHz samples (about 15 s).
    pub max_utterance_samples: usize,
    /// Decoder lag behind real time beyond which the Dictation ends with a fault.
    pub too_slow_ms: Millis,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            slow_decode_ms: 1000,
            pause_ms: 700,
            no_speech_threshold: 0.6,
            max_utterance_samples: 15_000 * SAMPLES_PER_MS as usize,
            too_slow_ms: 30_000,
        }
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
    start: Option<Millis>,
    fed_samples: u64,
    fault: Option<TypingFault>,
    /// The words of the last decode and the utterance length it covered.
    last_decode: Option<(usize, Vec<String>)>,
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
            start: None,
            fed_samples: 0,
            fault: None,
            last_decode: None,
        }
    }

    /// The Typing fault that ended this Dictation, if any.
    pub fn fault(&self) -> Option<TypingFault> {
        self.fault
    }

    /// Feed newly captured audio; may type text.
    pub fn feed(&mut self, samples: &[f32]) {
        if samples.is_empty() || self.fault.is_some() {
            return;
        }
        let now = self.clock.now();
        self.start.get_or_insert(now);
        self.fed_samples += samples.len() as u64;
        if !self.gate.is_speech(samples) {
            // Non-speech never reaches the recognizer; it only times pauses.
            self.on_silence(now);
            return;
        }
        self.heard_speech = true;
        self.last_speech_at = now;
        let mut rest = samples;
        while !rest.is_empty() && self.fault.is_none() {
            let room = self.config.max_utterance_samples.saturating_sub(self.audio.len());
            let (part, tail) = rest.split_at(room.min(rest.len()));
            self.audio.extend_from_slice(part);
            rest = tail;
            if self.audio.len() >= self.config.max_utterance_samples {
                self.finish_utterance();
            } else if !self.fallback {
                self.stream_decode();
            }
        }
    }

    fn on_silence(&mut self, now: Millis) {
        if self.fallback && self.heard_speech && now.saturating_sub(self.last_speech_at) >= self.config.pause_ms {
            self.finish_utterance();
        }
    }

    fn stream_decode(&mut self) {
        let started = self.clock.now();
        let words = self.decode();
        if self.clock.now().saturating_sub(started) > self.config.slow_decode_ms {
            self.fallback = true;
        }
        if self.check_too_slow(&words) {
            return;
        }
        let agreed = self.previous.iter().zip(&words).take_while(|(a, b)| a == b).count();
        self.previous = words.clone();
        // Hold back the last agreed word until a later decode confirms it.
        self.type_words(&words, agreed.saturating_sub(1));
    }

    /// Decode the whole utterance, type it all and start a fresh utterance.
    fn finish_utterance(&mut self) {
        let words = self.decode();
        self.type_words(&words, words.len());
        self.audio.clear();
        self.last_decode = None;
        self.previous.clear();
        self.typed_words = 0;
        self.heard_speech = false;
        self.check_too_slow(&words);
    }

    /// If the decoder is too far behind real time, type what was captured and end.
    fn check_too_slow(&mut self, words: &[String]) -> bool {
        let start = self.start.unwrap_or(0);
        let elapsed = self.clock.now().saturating_sub(start);
        let audio_ms = self.fed_samples / SAMPLES_PER_MS;
        if elapsed.saturating_sub(audio_ms) > self.config.too_slow_ms {
            self.type_words(words, words.len());
            self.audio.clear();
            self.last_decode = None;
            self.fault = Some(TypingFault::ModelTooSlow);
            return true;
        }
        false
    }

    /// End the Dictation: decode what is left and type every pending word.
    pub fn end(&mut self) {
        if self.audio.is_empty() || self.fault.is_some() {
            return;
        }
        // Audio no decode has seen yet needs one; otherwise the last decode is final.
        let words = match self.last_decode.take() {
            Some((len, words)) if len == self.audio.len() => words,
            _ => self.decode(),
        };
        self.type_words(&words, words.len());
        self.audio.clear();
    }

    fn decode(&mut self) -> Vec<String> {
        let words = self.decode_audio();
        self.last_decode = Some((self.audio.len(), words.clone()));
        words
    }

    fn decode_audio(&mut self) -> Vec<String> {
        self.recognizer
            .recognize(&self.audio, &self.language, None)
            .iter()
            .filter(|s| !should_drop(s, self.config.no_speech_threshold))
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
    struct Timed(FakeRecognizer, FakeClock, Millis, Rc<RefCell<Vec<usize>>>);
    impl Recognizer for Timed {
        fn recognize(&mut self, a: &[f32], l: &str, p: Option<&str>) -> Vec<Segment> {
            self.3.borrow_mut().push(a.len());
            self.1.advance(self.2);
            self.0.recognize(a, l, p)
        }
    }

    fn seg(text: &str) -> Vec<Segment> {
        vec![Segment { text: text.into(), words: vec![], no_speech_prob: 0.0 }]
    }

    fn seg_nsp(text: &str, p: f32) -> Vec<Segment> {
        vec![Segment { text: text.into(), words: vec![], no_speech_prob: p }]
    }

    struct Rig {
        ins: StreamingInsertion<Timed, FakeVoiceGate, FakeSink, FakeClock>,
        sink: FakeSink,
        clock: FakeClock,
        lens: Rc<RefCell<Vec<usize>>>,
    }

    fn rig(script: Vec<&str>, decode_ms: Millis) -> Rig {
        rig_segs(script.into_iter().map(seg).collect(), decode_ms)
    }

    fn rig_segs(script: Vec<Vec<Segment>>, decode_ms: Millis) -> Rig {
        let clock = FakeClock::new();
        let sink = FakeSink::default();
        let lens = Rc::new(RefCell::new(Vec::new()));
        let mut rec = FakeRecognizer::new();
        for s in script {
            rec = rec.then(s);
        }
        let ins = StreamingInsertion::new(
            Timed(rec, clock.clone(), decode_ms, lens.clone()),
            FakeVoiceGate(0.5),
            sink.clone(),
            clock.clone(),
            StreamingConfig::default(),
            "en",
        );
        Rig { ins, sink, clock, lens }
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
        let mut r = rig(vec!["hello world", "hello world again"], 10);
        r.ins.feed(&SPEECH);
        r.ins.feed(&SPEECH);
        assert_eq!(r.sink.typed(), "hello");
        r.ins.end();
        assert_eq!(r.sink.typed(), "hello world again");
    }

    #[test]
    fn ending_right_after_a_decode_does_not_decode_the_same_audio_again() {
        let mut r = rig(vec!["hello world", "hello world today", "this must not be asked for"], 10);
        r.ins.feed(&SPEECH);
        r.ins.feed(&SPEECH);
        assert_eq!(r.lens.borrow().len(), 2);
        r.ins.end();
        assert_eq!(r.lens.borrow().len(), 2, "end() reuses the last decode of the unchanged audio");
        assert_eq!(r.sink.typed(), "hello world today");
    }

    #[test]
    fn ending_decodes_audio_that_no_decode_has_seen() {
        let mut r = rig(vec!["hello world"], 1500); // slow: fallback mode, no streaming decodes
        r.ins.feed(&SPEECH);
        assert_eq!(r.lens.borrow().len(), 1);
        r.ins.feed(&SPEECH);
        r.ins.end();
        assert_eq!(r.lens.borrow().len(), 1 + 1, "audio fed after the last decode is decoded at the end");
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

    #[test]
    fn silence_and_noise_never_reach_the_recognizer_or_type() {
        let mut r = rig(vec!["you", "thank you"], 10);
        for _ in 0..5 {
            r.ins.feed(&SILENCE);
            r.ins.feed(&[0.2; 4]); // noise below the gate threshold
        }
        r.ins.end();
        assert_eq!(r.sink.typed(), "");
        assert!(r.lens.borrow().is_empty());
    }

    #[test]
    fn blocklisted_segments_are_dropped() {
        let mut r = rig(vec!["Thanks for watching!"], 10);
        r.ins.feed(&SPEECH);
        r.ins.end();
        assert_eq!(r.sink.typed(), "");
    }

    #[test]
    fn high_no_speech_probability_segments_are_dropped_but_others_kept() {
        let mut segs = seg_nsp("ghost words", 0.9);
        segs.extend(seg_nsp("real words", 0.1));
        let mut r = rig_segs(vec![segs], 10);
        r.ins.feed(&SPEECH);
        r.ins.end();
        assert_eq!(r.sink.typed(), "real words");
    }

    #[test]
    fn utterance_over_the_cap_is_split_and_every_part_is_typed() {
        let clock = FakeClock::new();
        let sink = FakeSink::default();
        let rec = FakeRecognizer::new().then(seg("first part")).then(seg("second part"));
        let cfg = StreamingConfig { max_utterance_samples: 8, ..StreamingConfig::default() };
        let lens = Rc::new(RefCell::new(Vec::new()));
        let mut ins = StreamingInsertion::new(Timed(rec, clock.clone(), 10, lens.clone()), FakeVoiceGate(0.5), sink.clone(), clock, cfg, "en");
        ins.feed(&[0.9; 12]); // 8 fill the cap and flush; 4 start the next utterance
        assert_eq!(sink.typed(), "first part");
        ins.end();
        assert_eq!(sink.typed(), "first part second part");
        assert_eq!(*lens.borrow(), vec![8, 4]);
    }

    #[test]
    fn decoder_over_30s_behind_ends_with_too_slow_fault_and_types_captured_audio() {
        let mut r = rig(vec!["captured so far"], 40_000);
        r.ins.feed(&[0.9; 16_000]); // 1 s of audio, decode took 40 s
        assert_eq!(r.ins.fault(), Some(TypingFault::ModelTooSlow));
        assert_eq!(r.sink.typed(), "captured so far");
        assert_eq!(
            TypingFault::ModelTooSlow.message(),
            "Speech model is too slow for this PC. Choose a smaller model in Settings."
        );
        r.ins.feed(&SPEECH);
        r.ins.end();
        assert_eq!(r.sink.typed(), "captured so far");
    }

    #[test]
    fn modest_decode_lag_is_not_a_fault() {
        let mut r = rig(vec!["a b", "a b c"], 5_000);
        r.ins.feed(&[0.9; 16_000]);
        assert_eq!(r.ins.fault(), None);
    }
}
