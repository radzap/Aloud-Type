//! Whisper recognizer and Silero voice gate: implementations of the core seams
//! ([`Recognizer`], [`VoiceGate`]) on top of the pinned `whisper-rs` binding.

use aloud_type_core::{Millis, Recognizer, Segment, VoiceGate, Word};
use std::path::Path;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
    WhisperVadContext, WhisperVadContextParams, WhisperVadParams,
};

/// Pinned versions. `whisper-rs` is pinned with `=` in Cargo.toml and by Cargo.lock; it vendors
/// `whisper-rs-sys` 0.15.0 which vendors whisper.cpp 1.8.3.
pub mod pins {
    pub const WHISPER_RS: &str = "0.16.0";
    pub const WHISPER_RS_SYS: &str = "0.15.0";
    /// whisper.cpp version vendored by `whisper-rs-sys` (reported at runtime by whisper.cpp itself).
    pub const WHISPER_CPP: &str = "1.8.3";
    /// Default Speech model file (ggml `base`, multilingual) and its SHA-256.
    pub const BASE_MODEL_FILE: &str = "ggml-base.bin";
    pub const BASE_MODEL_SHA256: &str =
        "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe";
    /// Silero voice-activity model (ggml port) and its SHA-256.
    pub const VAD_MODEL_FILE: &str = "ggml-silero-v5.1.2.bin";
    pub const VAD_MODEL_SHA256: &str =
        "29940d98d42b91fbd05ce489f3ecf7c72f0a42f027e4875919a28fb4c04ea2cf";
}

#[derive(Debug)]
pub struct RecognizerError(pub String);

impl std::fmt::Display for RecognizerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for RecognizerError {}

fn err(what: &str, e: impl std::fmt::Display) -> RecognizerError {
    RecognizerError(format!("{what}: {e}"))
}

/// Decode threads = min(physical cores - 1, 4), at least 1 (spec, performance targets).
pub fn decode_threads(physical_cores: usize) -> usize {
    physical_cores.saturating_sub(1).clamp(1, 4)
}

pub struct WhisperRecognizer {
    // Kept alive for the state's lifetime.
    _ctx: WhisperContext,
    state: WhisperState,
    threads: usize,
    detected: Option<&'static str>,
}

impl WhisperRecognizer {
    /// Load a ggml model with the spec's default thread count.
    pub fn load(model: &Path) -> Result<Self, RecognizerError> {
        Self::load_with_threads(model, decode_threads(num_cpus::get_physical()))
    }

    pub fn load_with_threads(model: &Path, threads: usize) -> Result<Self, RecognizerError> {
        let path = model.to_str().ok_or_else(|| RecognizerError("non-UTF-8 model path".into()))?;
        let ctx = WhisperContext::new_with_params(path, WhisperContextParameters::default())
            .map_err(|e| err("load model", e))?;
        let state = ctx.create_state().map_err(|e| err("create state", e))?;
        Ok(Self { _ctx: ctx, state, threads: threads.max(1), detected: None })
    }

    pub fn threads(&self) -> usize {
        self.threads
    }

    /// Language Whisper used for the last decode (the detected one when the language was "auto").
    pub fn last_language(&self) -> Option<&'static str> {
        self.detected
    }

    pub fn try_recognize(
        &mut self,
        audio: &[f32],
        language: &str,
        prompt: Option<&str>,
    ) -> Result<Vec<Segment>, RecognizerError> {
        let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        p.set_n_threads(self.threads as i32);
        p.set_language(Some(if language.is_empty() { "auto" } else { language }));
        p.set_token_timestamps(true);
        p.set_no_context(true);
        p.set_print_special(false);
        p.set_print_progress(false);
        p.set_print_realtime(false);
        p.set_print_timestamps(false);
        if let Some(t) = prompt.filter(|t| !t.is_empty()) {
            p.set_initial_prompt(t);
        }
        self.state.full(p, audio).map_err(|e| err("decode", e))?;
        self.detected = whisper_rs::get_lang_str(self.state.full_lang_id_from_state());

        let mut out = Vec::new();
        for seg in self.state.as_iter() {
            let mut words: Vec<Word> = Vec::new();
            let mut continuing = false;
            for i in 0..seg.n_tokens() {
                let Some(tok) = seg.get_token(i) else { continue };
                let text = tok.to_str_lossy().map_err(|e| err("token text", e))?;
                // Special tokens (<|...|>, [_TT_..]) carry no words.
                if text.starts_with("[_") || text.starts_with("<|") {
                    continue;
                }
                let d = tok.token_data();
                let (s, e) = ((d.t0.max(0) as Millis) * 10, (d.t1.max(0) as Millis) * 10);
                let p = tok.token_probability();
                if continuing && !text.starts_with(' ') && !words.is_empty() {
                    let w = words.last_mut().unwrap();
                    w.text.push_str(&text);
                    w.end_ms = e.max(w.end_ms);
                    w.confidence = w.confidence.min(p); // a word is as sure as its weakest piece
                } else if !text.trim().is_empty() {
                    words.push(Word { text: text.trim_start().to_string(), start_ms: s, end_ms: e.max(s), confidence: p });
                    continuing = true;
                } else {
                    continuing = false;
                }
            }
            // Punctuation-only tokens attach to the previous word via the branch above.
            out.push(Segment {
                text: seg.to_str_lossy().map_err(|e| err("segment text", e))?.trim().to_string(),
                words,
                no_speech_prob: seg.no_speech_probability(),
            });
        }
        Ok(out)
    }
}

impl Recognizer for WhisperRecognizer {
    /// Errors are logged and yield no segments; use [`WhisperRecognizer::try_recognize`] to see them.
    fn recognize(&mut self, audio: &[f32], language: &str, prompt: Option<&str>) -> Vec<Segment> {
        self.try_recognize(audio, language, prompt).unwrap_or_else(|e| {
            eprintln!("recognizer: {e}");
            Vec::new()
        })
    }
}

/// Silero voice gate: true when the slice contains speech.
pub struct SileroVoiceGate {
    ctx: WhisperVadContext,
}

impl SileroVoiceGate {
    pub fn load(model: &Path) -> Result<Self, RecognizerError> {
        let path = model.to_str().ok_or_else(|| RecognizerError("non-UTF-8 model path".into()))?;
        let mut p = WhisperVadContextParams::new();
        p.set_n_threads(1);
        p.set_use_gpu(false);
        let ctx = WhisperVadContext::new(path, p).map_err(|e| err("load VAD model", e))?;
        Ok(Self { ctx })
    }
}

impl VoiceGate for SileroVoiceGate {
    fn is_speech(&mut self, audio: &[f32]) -> bool {
        if audio.is_empty() {
            return false;
        }
        match self.ctx.segments_from_samples(WhisperVadParams::new(), audio) {
            Ok(s) => s.num_segments() > 0,
            Err(e) => {
                eprintln!("voice gate: {e}");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_threads_is_cores_minus_one_between_one_and_four() {
        let got: Vec<_> = [1, 2, 3, 4, 5, 6, 8, 16].iter().map(|&c| decode_threads(c)).collect();
        assert_eq!(got, [1, 1, 2, 3, 4, 4, 4, 4]);
    }
}
