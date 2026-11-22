# 04: Speech filtering and decoder limits

**What to build:** The engine gates audio with the voice gate so non-speech never reaches the recognizer, drops segments with a high no-speech probability or matching a short hallucination blocklist, caps a single utterance at about 15 seconds, and ends the Dictation with the amber Typing fault 'Speech model is too slow for this PC. Choose a smaller model in Settings.' when the decoder is more than about 30 seconds behind real time, still typing audio captured so far.

**Blocked by:** 03 Streaming insertion

**Status:** resolved

- [ ] Silence and noise clips produce no typed text
- [ ] Blocklisted and low-confidence segments are dropped
- [ ] Utterances over the cap are split correctly
- [ ] The too-slow fault fires and still flushes captured audio

## Answer

Done (TDD). Changes in `crates/core`: new `speech_filter.rs` (drops segments with no-speech probability >= 0.6 or whose whole text, normalized, is on a short hallucination blocklist) and `streaming.rs` extended.
- Voice gate: non-speech chunks are never appended or sent to the recognizer (they only time fallback pauses).
- Cap: an utterance is limited to 15 s (240000 samples); a chunk crossing the cap is split, the full part is decoded and typed, the remainder starts a new utterance.
- Too slow: after a decode, if (clock elapsed - audio fed) exceeds 30 s the engine types everything decoded from captured audio, sets `fault() == Some(TypingFault::ModelTooSlow)` (message "Speech model is too slow for this PC. Choose a smaller model in Settings.") and ignores further audio.
- Gaps: the fault is exposed via `fault()` but not yet wired to the overlay/tray (later ticket); blocklist, 0.6 threshold and lag measure are untuned defaults; dropping all non-speech chunks may trim quiet word edges (no pre-roll padding); lag is measured only at decode completion; `cargo fmt --check` already differs in `lifecycle.rs` (not touched).
