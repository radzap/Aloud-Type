# 10: Whisper recognizer

**What to build:** The recognizer crate implements the recognition and voice-gate traits on top of the pinned Whisper binding with the base model: a window of 16 kHz audio in, text segments with word timings, word confidence and a no-speech probability out, with language and optional prompt, and decode threads set per the performance rules. Verified offline by decoding recorded clips through the same interface the engine uses.

**Blocked by:** 01 Workspace, shared test support and CI

**Status:** resolved

- [x] A recorded English clip decodes to the expected words with timings and confidence
- [x] The voice gate separates speech from silence on recorded clips
- [x] Language selection (fixed and Auto) is honoured
- [x] Model, crate and vendored engine versions are pinned

## Answer

Done (TDD; seams tested: core `Recognizer` and `VoiceGate` traits via the public `WhisperRecognizer` / `SileroVoiceGate`, offline).
- `crates/recognizer`: `WhisperRecognizer` (whisper-rs, greedy, token timestamps, `no_context`, optional initial prompt, language "auto" or fixed code) returns segments with words (tokens merged into words, confidence = weakest token probability, ms timings) and no-speech probability; `try_recognize` exposes errors, `last_language()` reports the language used/detected. `SileroVoiceGate` uses whisper.cpp's Silero VAD. `decode_threads` = min(physical cores - 1, 4), at least 1.
- Pins: `whisper-rs =0.16.0` (Cargo.toml + Cargo.lock; `whisper-rs-sys` 0.15.0, whisper.cpp 1.8.3, asserted at test time), `ggml-base.bin` SHA-256 60ed5bc3...fba2efe, `ggml-silero-v5.1.2.bin` SHA-256 29940d98...ea2cf (constants in `pins`, verified by a test).
- Tests (6): jfk clip decodes to the exact expected words with monotonic timings and confidence; gate: speech true, digital silence/room noise/empty false, padded speech true; Auto detects "en", fixed "de" is honoured; prompt and thread override; pins. Recorded clip: `tests/fixtures/jfk.wav` (whisper.cpp sample, public-domain speech, 352 KB, committed).
- Models are not committed. They are read from `%LOCALAPPDATA%\AloudTypeDev\models` (override `ALOUDTYPE_MODEL_DIR`); downloaded from huggingface.co/ggerganov/whisper.cpp and huggingface.co/ggml-org/whisper-vad. Without them the tests SKIP; set `ALOUDTYPE_REQUIRE_MODELS=1` to make that a failure.
- Gaps: only one English clip and synthetic silence/noise (no recorded noisy-room or multilingual clips); CI does not yet download the models so the model tests skip there; build needs LIBCLANG_PATH (bindgen); whisper.cpp debug logging is noisy in test output (logging hooks feature not enabled); `Recognizer::recognize` swallows errors (logs and returns empty); decode worker thread priority is left to the worker ticket; CPU only, no latency benchmark on target hardware.
