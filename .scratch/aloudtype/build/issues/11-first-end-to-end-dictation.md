# 11: First end-to-end dictation

**What to build:** The walking skeleton: hold Right Ctrl, speak, and text appears in Notepad as it is confirmed, with a console reporting state and no UI. Wires the keyboard hook, audio capture, Whisper recognizer, streaming insertion and text injection together on a machine with the base model installed by hand.

**Blocked by:** 04 Speech filtering and decoder limits, 08 Windows text injection, 09 Audio capture and metering, 10 Whisper recognizer

**Status:** resolved

- [x] Speaking a sentence types it into Notepad within the latency target on a reference-class machine, or the gap is recorded
- [x] Ending the Dictation flushes the remaining words
- [x] The six core behaviours (hold, tap, streaming, spacing, flush, fallback) are demonstrated end to end

## Answer

Done (TDD, vertical slices) in new crate `crates/app` (`aloud-type-app`: lib `Controller` + binary `aloud-type`). Shared files touched: `core/lifecycle.rs` gained `DictationEngine::reset()` (+test). Seams tested: the public `Controller` (key events in, typed text / indicator / tray out) with fakes (`aloud_type_app::testing`), plus an end-to-end test through the real Whisper recognizer and Silero gate using `jfk.wav`.
- `Controller::step()` wires: lifecycle engine -> `ListeningMonitor` -> capture -> `StreamingInsertion` -> sink. Wiring gaps from earlier tickets closed: 08 `check_can_start` on key-down (amber notice, nothing starts) and sink fault slot -> `typing_fault`; 05 engine reset after a hands-free fault and flush on `EndDictationFlushAudio` (also a tap on a standing hands-free fault only dismisses); 09 `take_fault` -> monitor; 04 `StreamingInsertion::fault()` (model too slow) -> amber; 20 sleep (`power`: ends without flushing, closes audio, returns hook-reinstall/re-enumerate actions), worker faults (`worker_fault`, `recover`), `run_supervised` around the loop in `main`, hook `key_unavailable` and `--verbose` Logger and `SettingsNotice` (printed to console) in `main`.
- `main.rs` (Windows): hook (Right Ctrl, modifier chord rule), cpal capture, Whisper + Silero from `ggml-base.bin`/`ggml-silero-v5.1.2.bin` (folder: `ALOUDTYPE_MODEL_DIR`, `%LOCALAPPDATA%\AloudType\models` or `...\AloudTypeDev\models`), SendInput sink; console prints state changes. Headless smoke run: starts, loads models, prints `idle`.
- A real finding: Silero verdicts per chunk punched holes in speech and dropped words; fixed with a 1 s hangover in the host (test: `a_brief_non_speech_verdict_inside_speech_does_not_cut_a_hole_in_the_audio`). Host chunk size is 1000 ms: with 300/500 ms chunks the jfk clip lost or changed words ("ask not" -> "and not"), because Streaming insertion types by word index from consecutive decodes and cannot retract.
- Six behaviours: hold, tap (hands-free), streaming (text typed before release in the e2e test), spacing (no leading/trailing/double spaces asserted), flush (release/next tap), fallback (core tests from ticket 03; not exercised in the e2e since it runs on a fake clock) are covered by fakes; hold/streaming/flush/spacing by the real recognizer.

Gaps (not done or not verifiable headlessly):
- Not run with a real keyboard, microphone or Notepad (no human/devices): hook feel, real capture, SendInput into Notepad are only covered by their own tickets' fakes. Run `cargo run -p aloud-type-app` and try it.
- Latency NOT within target on this dev machine (debug test build, base f16 model, CPU): jfk (11 s): first text after about 2 s of audio, 5.6 s wall; flush after release 1.5 s (spec target p50 <= 1.0 s for 3-8 s utterances; this clip is longer); decoding the whole 11 s clip took about 22 s wall in total, i.e. slower than real time because each chunk re-decodes the growing utterance. Not measured in release or on the target laptop class; the quantised `base-q5_1`, a smaller window/decode scheme or GPU are the levers. The e2e test uses a fake clock, so slow-decode fallback and the 30 s "too slow" fault do not trigger there.
- Whisper runs on the main loop thread, so key events queue during a decode (spec wants a below-normal-priority decode worker); audio keeps buffering meanwhile. Worker restart/fault are supervised as one worker (Recognizer); after a final fault `main` prints the fault and exits instead of staying resident.
- No tray, overlay, settings window or startup notice wired (tickets 12/13/15): tray/overlay state is printed to the console; the invalid-settings notice is printed rather than `TrayHandle::notify`. Settings are re-read only at Dictation start; key rebinding, the stored `dictation_key` and model choice (`base-q5_1` default) are ignored (hand-installed `ggml-base.bin`, Right Ctrl). Chunk size and hangover are untuned.
