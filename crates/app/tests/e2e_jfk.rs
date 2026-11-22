//! End to end through the real Whisper recognizer and Silero gate: a held key,
//! the recorded jfk clip as the microphone, a recording text sink. Skips when
//! the models are not installed (ALOUDTYPE_MODEL_DIR or %LOCALAPPDATA%\AloudTypeDev\models).
use aloud_type_app::testing::*;
use aloud_type_app::*;
use aloud_type_core::lifecycle::LifecycleConfig;
use aloud_type_core::listening::Indicator;
use aloud_type_core::testing::{FakeClock, FakeKeyboard};
use aloud_type_core::{Clock, KeyEvent};
use aloud_type_recognizer::{pins, SileroVoiceGate, WhisperRecognizer};
use std::path::PathBuf;
use std::time::Instant;

fn model(name: &str) -> Option<PathBuf> {
    let dir = std::env::var("ALOUDTYPE_MODEL_DIR").map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into())).join("AloudTypeDev").join("models")
    });
    let p = dir.join(name);
    if p.exists() {
        return Some(p);
    }
    assert!(std::env::var("ALOUDTYPE_REQUIRE_MODELS").is_err(), "required model {name} missing");
    eprintln!("SKIP: model {name} not found in {}", dir.display());
    None
}

fn wav() -> Vec<f32> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../recognizer/tests/fixtures/jfk.wav");
    let b = std::fs::read(path).expect("fixture");
    let mut i = 12;
    while i + 8 <= b.len() {
        let len = u32::from_le_bytes(b[i + 4..i + 8].try_into().unwrap()) as usize;
        if &b[i..i + 4] == b"data" {
            return b[i + 8..i + 8 + len].as_chunks::<2>().0.iter().map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect();
        }
        i += 8 + len;
    }
    panic!("no data chunk");
}

fn norm(s: &str) -> String {
    s.split_whitespace()
        .map(|w| w.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

const EXPECTED: &str =
    "and so my fellow americans ask not what your country can do for you ask what you can do for your country";

#[test]
fn holding_the_key_while_the_jfk_clip_plays_types_it_streaming_and_flushes_on_release() {
    let (Some(m), Some(v)) = (model(pins::BASE_MODEL_FILE), model(pins::VAD_MODEL_FILE)) else { return };
    let (kb, clock, cap, sink) = (FakeKeyboard::new(), FakeClock::new(), FakeCapture::default(), FakeSink::default());
    let mut c = Controller::new(Parts {
        keyboard: kb.clone(),
        clock: clock.clone(),
        lifecycle: LifecycleConfig { modifier_key: true, ..Default::default() },
        capture: Box::new(cap.clone()),
        recognizer: Box::new(WhisperRecognizer::load(&m).expect("load model")),
        voice_gate: Box::new(SileroVoiceGate::load(&v).expect("load vad")),
        sink: Box::new(sink.clone()),
        start_gate: Box::new(FakeStartGate::default()),
        typing_faults: Box::new(sink.clone()),
        settings: Box::new(|| DictationSettings { language: "en".into(), microphone: Mic::SystemDefault }),
        config: ControllerConfig::default(),
    });

    kb.press(KeyEvent::DictationKeyDown);
    c.step();
    assert!(cap.is_open());

    let audio = wav();
    let t0 = Instant::now();
    let mut first_text_at = None;
    for packet in audio.chunks(1600) {
        // 100 ms of speech per tick
        clock.advance(100);
        cap.push(packet);
        c.step();
        if first_text_at.is_none() && !sink.text().is_empty() {
            first_text_at = Some((t0.elapsed(), clock.now()));
        }
    }
    let before_release = sink.text();
    assert!(!before_release.is_empty(), "words were typed while the key was still held");
    assert!(!before_release.starts_with(' '));

    let release_at = Instant::now();
    kb.press(KeyEvent::DictationKeyUp);
    c.step();
    let flush = release_at.elapsed();

    let typed = sink.text();
    assert_eq!(norm(&typed), EXPECTED);
    assert!(!typed.starts_with(' ') && !typed.ends_with(' '), "no stray spaces: {typed:?}");
    assert!(!typed.contains("  "), "single spaces: {typed:?}");
    assert!(!cap.is_open());
    assert_eq!(c.indicator(), Indicator::Hidden);

    let (wall, audio_ms) = first_text_at.expect("text appeared");
    eprintln!(
        "LATENCY first text after {audio_ms} ms of audio (wall {wall:?} incl. all decodes so far); flush on release took {flush:?}; total wall {:?}",
        t0.elapsed()
    );
}
