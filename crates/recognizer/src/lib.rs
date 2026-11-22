//! Whisper recognizer and Silero voice gate: implementations of the core seams
//! ([`Recognizer`], [`VoiceGate`]) on top of the pinned `whisper-rs` binding.

pub mod gpu;

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

    /// CPU-only load.
    pub fn load_with_threads(model: &Path, threads: usize) -> Result<Self, RecognizerError> {
        Self::load_on(model, threads, None)
    }

    /// Load on the GPU backend device `gpu_device` (CPU when `None`). Without the `vulkan`
    /// feature there is no GPU backend and a GPU load is an error.
    pub fn load_on(
        model: &Path,
        threads: usize,
        gpu_device: Option<i32>,
    ) -> Result<Self, RecognizerError> {
        let path = model
            .to_str()
            .ok_or_else(|| RecognizerError("non-UTF-8 model path".into()))?;
        if gpu_device.is_some() && !cfg!(feature = "vulkan") {
            return Err(RecognizerError("this build has no GPU backend".into()));
        }
        let mut params = WhisperContextParameters::default();
        params.use_gpu(gpu_device.is_some());
        if let Some(d) = gpu_device {
            params.gpu_device(d);
        }
        let ctx =
            WhisperContext::new_with_params(path, params).map_err(|e| err("load model", e))?;
        let state = ctx.create_state().map_err(|e| err("create state", e))?;
        Ok(Self {
            _ctx: ctx,
            state,
            threads: threads.max(1),
            detected: None,
        })
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
        p.set_language(Some(if language.is_empty() {
            "auto"
        } else {
            language
        }));
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
            let mut pieces = Vec::new();
            for i in 0..seg.n_tokens() {
                let Some(tok) = seg.get_token(i) else {
                    continue;
                };
                let bytes = tok.to_bytes().map_err(|e| err("token text", e))?;
                let d = tok.token_data();
                pieces.push(TokenPiece {
                    bytes,
                    start_ms: (d.t0.max(0) as Millis) * 10,
                    end_ms: (d.t1.max(0) as Millis) * 10,
                    confidence: tok.token_probability(),
                });
            }
            let words = assemble_words(&pieces);
            out.push(Segment {
                text: seg
                    .to_str_lossy()
                    .map_err(|e| err("segment text", e))?
                    .trim()
                    .to_string(),
                words,
                no_speech_prob: seg.no_speech_probability(),
            });
        }
        Ok(out)
    }
}

/// One decoded token as raw bytes. Whisper's tokens are byte-level: a multi-byte UTF-8
/// character (Slovak "Č", "ť", ...) can be split across two tokens, so a token on its own is
/// not always valid UTF-8.
pub struct TokenPiece<'a> {
    pub bytes: &'a [u8],
    pub start_ms: Millis,
    pub end_ms: Millis,
    pub confidence: f32,
}

/// Join tokens into words. A token starting with a space starts a new word; others continue
/// the current word (word pieces, punctuation, the rest of a split character). Bytes are
/// collected per word and decoded only once the word is complete. Special tokens
/// (`<|...|>`, `[_TT_..]`) carry no words.
pub fn assemble_words(tokens: &[TokenPiece]) -> Vec<Word> {
    struct Building {
        bytes: Vec<u8>,
        start_ms: Millis,
        end_ms: Millis,
        confidence: f32,
    }
    let mut words: Vec<Building> = Vec::new();
    let mut continuing = false;
    for t in tokens {
        let b = t.bytes;
        if b.starts_with(b"[_") || b.starts_with(b"<|") {
            continue;
        }
        if continuing && b.first() != Some(&b' ') && !words.is_empty() {
            let w = words.last_mut().unwrap();
            w.bytes.extend_from_slice(b);
            w.end_ms = t.end_ms.max(w.end_ms);
            w.confidence = w.confidence.min(t.confidence); // as sure as its weakest piece
        } else if !b.iter().all(u8::is_ascii_whitespace) {
            words.push(Building {
                bytes: b.to_vec(),
                start_ms: t.start_ms,
                end_ms: t.end_ms.max(t.start_ms),
                confidence: t.confidence,
            });
            continuing = true;
        } else {
            continuing = false;
        }
    }
    words
        .into_iter()
        .map(|w| Word {
            text: String::from_utf8_lossy(&w.bytes).trim_start().to_string(),
            start_ms: w.start_ms,
            end_ms: w.end_ms,
            confidence: w.confidence,
        })
        .collect()
}

impl Recognizer for WhisperRecognizer {
    /// Errors are logged and yield no segments; use [`WhisperRecognizer::try_recognize`] to see them.
    fn recognize(&mut self, audio: &[f32], language: &str, prompt: Option<&str>) -> Vec<Segment> {
        self.try_recognize(audio, language, prompt)
            .unwrap_or_else(|e| {
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
        let path = model
            .to_str()
            .ok_or_else(|| RecognizerError("non-UTF-8 model path".into()))?;
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
        match self
            .ctx
            .segments_from_samples(WhisperVadParams::new(), audio)
        {
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
        let got: Vec<_> = [1, 2, 3, 4, 5, 6, 8, 16]
            .iter()
            .map(|&c| decode_threads(c))
            .collect();
        assert_eq!(got, [1, 1, 2, 3, 4, 4, 4, 4]);
    }

    fn piece(bytes: &[u8], start_ms: Millis) -> TokenPiece<'_> {
        TokenPiece {
            bytes,
            start_ms,
            end_ms: start_ms + 100,
            confidence: 0.9,
        }
    }

    fn texts(tokens: &[TokenPiece]) -> Vec<String> {
        assemble_words(tokens).into_iter().map(|w| w.text).collect()
    }

    #[test]
    fn a_character_split_across_tokens_is_decoded_whole() {
        // "Čo má toto znamenať" where "Č" (C4 8C) and "ť" (C5 A5) are each split in two tokens.
        let toks = [
            piece(b"<|sk|>", 0),
            piece(b" \xC4", 0),
            piece(b"\x8Co", 100),
            piece(" má".as_bytes(), 200),
            piece(b" toto", 300),
            piece(b" znamena", 400),
            piece(b"\xC5", 500),
            piece(b"\xA5", 500),
            piece(b"?", 600),
        ];
        assert_eq!(texts(&toks), ["Čo", "má", "toto", "znamenať?"]);
    }

    #[test]
    fn words_keep_their_span_and_weakest_confidence() {
        let mut b = piece(b"llo", 100);
        b.confidence = 0.4;
        let w = assemble_words(&[piece(b" he", 0), b]);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].text, "hello");
        assert_eq!((w[0].start_ms, w[0].end_ms), (0, 200));
        assert_eq!(w[0].confidence, 0.4);
    }

    #[test]
    fn special_and_blank_tokens_make_no_words() {
        let toks = [piece(b"[_TT_50]", 0), piece(b" ", 0), piece(b" hi", 0)];
        assert_eq!(texts(&toks), ["hi"]);
    }
}
