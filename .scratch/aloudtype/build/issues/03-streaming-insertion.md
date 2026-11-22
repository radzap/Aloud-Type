# 03: Streaming insertion

**What to build:** With a scripted recognizer, a Dictation produces typed text as it is confirmed: only leading words two consecutive decodes agree on are typed, the last word is held back until confirmed, one space precedes each later chunk and none at the start or end, and ending the Dictation flushes everything. If the recognizer cannot keep up, the engine falls back to typing the whole utterance when the speaker pauses.

**Blocked by:** 02 Dictation lifecycle: hold and tap

**Status:** resolved

- [x] Typed output matches the confirmed-prefix, held-back-word and spacing rules for scripted decodes
- [x] Ending a Dictation flushes all pending words
- [x] Slow-recognizer scripts trigger the type-on-pause fallback automatically
- [x] No test depends on internal state

## Answer

Done (TDD, vertical slices). Seam: `StreamingInsertion` in `crates/core/src/streaming.rs`, driven by `feed(audio)`/`end()` with a scripted `FakeRecognizer`, `FakeVoiceGate`, `FakeClock` and a `TextSink` output trait (new seam; test fake lives in the module's tests).
- Types the words two consecutive decodes agree on, holding back the last agreed word; one leading space per later chunk, none at start/end; `end()` flushes everything.
- Fallback: a decode taking longer than `slow_decode_ms` (default 1000, measured on the Clock) switches to typing the whole utterance after `pause_ms` (default 700) of silence per the VoiceGate; later utterances are space-separated.
- Gaps: not wired to a real audio loop or the 15 s cap / "model too slow" Typing fault (later tickets); thresholds are untuned defaults; the fallback does not switch back to streaming; word comparison is exact-string.
