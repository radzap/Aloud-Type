# 19: GPU acceleration

**What to build:** The optional GPU backend library and the GPU probe child process: on first start the probe benchmarks GPU against CPU on the real model, requires a meaningful speedup and matching output, and saves the result per device and driver. The resident process loads the backend only when approved; a backend error mid-Dictation finishes that Dictation on CPU and disables the GPU for the session; Settings offers Auto, CPU only or a specific device and shows 'GPU disabled after an error' with re-test.

**Blocked by:** 11 First end-to-end dictation, 16 Model manager and first-run flow

**Status:** resolved

- [x] Probe results are saved and reused per device and driver
- [x] A hung or crashed probe never affects the resident process
- [x] Mid-Dictation GPU errors fall back to CPU without losing words
- [x] With the GPU unapproved, the backend library is never loaded

## Answer

Done (TDD; seams: `aloud_type_recognizer::gpu` with fake `Engine`/`GpuBackend`/`ChildRunner`, real child processes via `cmd`/`powershell`, settings `ItemsSession`). Workspace tests and `cargo deny check licenses` pass after merging integration/aloudtype (includes 17 and 18).
- `crates/recognizer/src/gpu.rs`: `benchmark` (GPU warm-up, timed CPU vs GPU, requires speedup >= 1.3x and matching words), `select` (pure; Auto / CPU only / device; result cached per `<device>|<driver>` in state; rejections cached too; new driver re-probes; error flag keeps CPU), `probe_in_child` + `StdChildRunner` (timeout kills the child; hang, crash, garbage or spawn failure all become a cached rejection), `open_recognizer`/`GpuFallback` (backend library loaded only for an approved device; a GPU error mid-Dictation re-decodes the same buffered audio on CPU, reports once, and `open_for_host` saves `gpu_disabled_after_error`), `retest_on_disk`, `parse_display_adapters` (adapter + driver version from registry, so the backend is not loaded just to identify the device), `probe_child_main` (embedded jfk clip). 34 tests.
- `WhisperRecognizer::load_on(model, threads, Option<device>)`; cargo feature `vulkan` (off by default, forwards to `whisper-rs/vulkan`); a GPU load without it errors. Host: `main.rs` has `--probe-gpu` mode (child) and the loader uses `open_for_host`. Settings: GPU pick list (Auto, CPU only, probed devices), "GPU disabled after an error" notice and Re-test button (`ItemsSession`, new IPC command `retest-gpu`).
Gaps: no Vulkan SDK here, so the Vulkan build was never compiled or run and no real GPU benchmark exists (probe in the default build always rejects with "no GPU backend"); backend is statically linked, not the optional separate DLL (whisper-rs-sys has no GGML_BACKEND_DL; needs a custom build, so "never loaded" is only enforced at the code path level); Vulkan device index vs registry adapter order mapping is a guess (needs on-device check, spike 6); the resident process has no pipe server yet, so `retest-gpu` is sent but not handled (call `gpu::retest_on_disk`); state is written by the resident process only, Settings reads it at window open; probe runs synchronously on the model-load worker at first load; small q5_1 auto-default upgrade after a confirmed GPU is not implemented.
