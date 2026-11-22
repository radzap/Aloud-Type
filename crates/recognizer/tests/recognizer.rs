mod support;
use aloud_type_core::Recognizer;
use aloud_type_recognizer::WhisperRecognizer;

fn norm(w: &str) -> String {
    w.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase()
}

#[test]
fn english_clip_decodes_to_expected_words_with_timings_and_confidence() {
    let Some(model) = support::model("ggml-base.bin") else { return };
    let audio = support::wav("jfk.wav");
    let mut rec = WhisperRecognizer::load(&model).expect("load");
    let segs = rec.recognize(&audio, "en", None);
    let words: Vec<_> = segs.iter().flat_map(|s| s.words.iter()).collect();
    let text: Vec<String> = words.iter().map(|w| norm(&w.text)).filter(|w| !w.is_empty()).collect();
    assert_eq!(
        text.join(" "),
        "and so my fellow americans ask not what your country can do for you ask what you can do for your country"
    );
    let mut prev = 0;
    for w in &words {
        assert!(w.start_ms >= prev && w.end_ms >= w.start_ms, "{w:?}");
        assert!((0.0..=1.0).contains(&w.confidence) && w.confidence > 0.0, "{w:?}");
        prev = w.start_ms;
    }
    assert!(words.last().unwrap().end_ms <= 11_000);
    assert!(segs.iter().all(|s| s.no_speech_prob < 0.5));
}

#[test]
fn voice_gate_separates_speech_from_silence() {
    use aloud_type_core::VoiceGate;
    use aloud_type_recognizer::{pins, SileroVoiceGate};
    let Some(vad) = support::model(pins::VAD_MODEL_FILE) else { return };
    let mut gate = SileroVoiceGate::load(&vad).expect("load vad");
    let speech = support::wav("jfk.wav");
    assert!(gate.is_speech(&speech));
    assert!(!gate.is_speech(&vec![0.0; 16_000 * 3]), "digital silence");
    // Quiet room noise (deterministic LCG, about -50 dBFS).
    let mut x: u32 = 12345;
    let noise: Vec<f32> = (0..16_000 * 3)
        .map(|_| {
            x = x.wrapping_mul(1664525).wrapping_add(1013904223);
            ((x >> 16) as f32 / 32768.0 - 1.0) * 0.003
        })
        .collect();
    assert!(!gate.is_speech(&noise), "room noise");
    assert!(!gate.is_speech(&[]));
    // Speech surrounded by silence is still speech.
    let mut padded = vec![0.0; 16_000];
    padded.extend(&speech);
    padded.extend(vec![0.0; 16_000]);
    assert!(gate.is_speech(&padded));
}

#[test]
fn language_selection_fixed_and_auto_is_honoured() {
    let Some(model) = support::model("ggml-base.bin") else { return };
    let audio = support::wav("jfk.wav");
    let mut rec = WhisperRecognizer::load(&model).expect("load");
    rec.recognize(&audio, "auto", None);
    assert_eq!(rec.last_language(), Some("en"), "auto detects English");
    rec.recognize(&audio, "de", None);
    assert_eq!(rec.last_language(), Some("de"), "fixed language is used as given");
    rec.recognize(&audio, "en", None);
    assert_eq!(rec.last_language(), Some("en"));
}

#[test]
fn prompt_is_accepted_and_thread_count_follows_the_performance_rule() {
    let Some(model) = support::model("ggml-base.bin") else { return };
    let mut rec = WhisperRecognizer::load_with_threads(&model, 2).expect("load");
    assert_eq!(rec.threads(), 2);
    let segs = rec.recognize(&support::wav("jfk.wav"), "en", Some("Ask not what your country can do."));
    assert!(!segs.is_empty());
    let default = WhisperRecognizer::load(&model).expect("load");
    assert!((1..=4).contains(&default.threads()));
}

#[test]
fn pinned_models_match_their_recorded_sha256() {
    use aloud_type_recognizer::pins;
    use sha2::{Digest, Sha256};
    for (file, sha) in [
        (pins::BASE_MODEL_FILE, pins::BASE_MODEL_SHA256),
        (pins::VAD_MODEL_FILE, pins::VAD_MODEL_SHA256),
    ] {
        let Some(p) = support::model(file) else { continue };
        let got = format!("{:x}", Sha256::digest(std::fs::read(p).unwrap()));
        assert_eq!(got, sha, "{file}");
    }
}

#[test]
fn engine_and_crate_versions_are_pinned() {
    use aloud_type_recognizer::pins;
    assert_eq!(whisper_rs::get_whisper_version(), pins::WHISPER_CPP);
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    assert!(manifest.contains(&format!("whisper-rs = \"={}\"", pins::WHISPER_RS)));
    let lock = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock")).unwrap();
    assert!(lock.contains(&format!("name = \"whisper-rs-sys\"\nversion = \"{}\"", pins::WHISPER_RS_SYS)));
}
