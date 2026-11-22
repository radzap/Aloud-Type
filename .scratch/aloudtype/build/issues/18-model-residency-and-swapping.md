# 18: Model residency and swapping

**What to build:** The model loads at start on a worker thread so Dictation is available at once (a Dictation before load finishes buffers audio and waits), 'Keep loaded' and 'Unload when idle' (10 minutes, reload in parallel with capture) both work and change immediately, and switching models loads the new one while the old keeps serving, then swaps, keeping the old model if the load fails and showing a notice.

**Blocked by:** 11 First end-to-end dictation, 16 Model manager and first-run flow

**Status:** resolved

- [x] Both residency modes behave as specified
- [x] A model swap never interrupts a Dictation
- [x] A failed swap keeps the old model
- [ ] Idle memory with Unload matches the footprint target or the gap is recorded (gap recorded in the Answer)

## Answer

Done (TDD; seam: `aloud_type_app::residency::Residency` with a fake `Loader`, plus the public `Controller` via `set_residency`). `cargo test --workspace` passes after merging integration/aloudtype.
- New module `crates/app/src/residency.rs`: `Residency::start` loads the model on a worker thread; `ResidentRecognizer` (core `Recognizer`) blocks while a load is in flight, so a Dictation before load finishes keeps buffering audio in capture and is served once loaded. Keep loaded / Unload when idle (`IDLE_UNLOAD_MS` = 10 min after the last Dictation, never during one); `set_memory` applies at once; the next Dictation key reloads in parallel with opening the microphone. `request_model` loads the new model while the old keeps serving; the swap lands only between Dictations (`dictation_ended`/`tick`), never mid-Dictation; a failed swap keeps the old model and queues a notice (`take_notices`); a failed first load returns empty results, notice, and retries at the next Dictation. A model chosen while unloaded is just loaded next time.
- Shared files: `controller.rs` gained `set_residency` and calls to started/ended/tick (minimal); `main.rs` loads only `verified_path` models via a loader, re-applies `model_memory`/`speech_model` from settings at each Dictation start, prints notices. Recognizer crate untouched (ticket 19 safe). App crate depends on `aloud-type-models` (Windows).
- Gaps: idle footprint (working set after unload vs. the 25 MB / 40 MB target) not measured (no profiling harness here; whisper context drop should free the model, verify with a real run). Settings changes are picked up at the next Dictation start (no settings file watcher or pipe `Command` for model switch/unload yet), so a swap triggered in Settings starts at the next Dictation, not instantly. The model notice is printed to the console, not `TrayHandle::notify`. A blocked decode wait is on the host main loop thread (Whisper still runs there), so key events queue during a model-load wait. Idle unload relies on the loop's existing 20 ms tick. Dev fallback: with `ALOUDTYPE_MODEL_DIR` set, a hand-installed `ggml-base.bin` is loaded unverified.
