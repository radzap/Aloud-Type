//! Shared helpers for the offline recognizer tests: locate cached models, load recorded clips.
#![allow(dead_code)]
use std::path::PathBuf;

/// Directory holding the (uncommitted) ggml models. Override with `ALOUDTYPE_MODEL_DIR`.
pub fn model_dir() -> PathBuf {
    if let Ok(d) = std::env::var("ALOUDTYPE_MODEL_DIR") {
        return d.into();
    }
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("AloudTypeDev").join("models")
}

/// Path of a cached model, or `None` (test skips with a note) when it is not downloaded.
/// Set `ALOUDTYPE_REQUIRE_MODELS=1` to turn a missing model into a failure.
pub fn model(name: &str) -> Option<PathBuf> {
    let p = model_dir().join(name);
    if p.exists() {
        return Some(p);
    }
    assert!(
        std::env::var("ALOUDTYPE_REQUIRE_MODELS").is_err(),
        "required model {name} missing in {}",
        model_dir().display()
    );
    eprintln!("SKIP: model {name} not found in {}", model_dir().display());
    None
}

/// Load a 16-bit PCM mono 16 kHz WAV as f32 samples.
pub fn wav(name: &str) -> Vec<f32> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    let b = std::fs::read(path).expect("fixture");
    assert_eq!(&b[0..4], b"RIFF");
    let mut i = 12;
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let len = u32::from_le_bytes(b[i + 4..i + 8].try_into().unwrap()) as usize;
        if id == b"data" {
            return b[i + 8..i + 8 + len]
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
                .collect();
        }
        i += 8 + len;
    }
    panic!("no data chunk");
}
