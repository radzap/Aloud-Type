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

Follow-up (2026-11-22, on-device on RTX 3060 Laptop + Iris Xe):
- Adapter discovery queried a mistyped display-class GUID (`bf1c` for `bfc1`), so no adapter was ever found and the probe never ran. Fixed (`DISPLAY_CLASS_KEY`).
- Registry order (Intel 0, NVIDIA 1) differs from Vulkan order (discrete first). The probe child and `load_gpu` now resolve the Vulkan index by adapter name (`backend_index`).
- whisper-rs-sys 0.15 cannot build `GGML_BACKEND_DL` (forces `BUILD_SHARED_LIBS=OFF`, links ggml-vulkan statically), so the backend is static (+55 MB exe) and ggml creates a Vulkan instance at every model load, CPU included. `vulkan-1.dll` is delay-loaded; delay-load hooks in `crates/app/src/vulkan_guard.rs` make Vulkan init fail cleanly when it is missing or `ALOUDTYPE_NO_VULKAN=1`, so the app runs on CPU. `load_library` checks the ggml registry before enumerating devices.
- App feature `vulkan`; `scripts/release.py` builds with it (opt out `--cpu-only`) in a short target dir (`ALOUDTYPE_TARGET_DIR`, default `<drive>:\aloudtype-target`) because Vulkan's nested CMake paths exceed MAX_PATH under `target\`.
- Measured probe (base q5_1, JFK clip): NVIDIA 12-13x, Iris Xe 3.4x. Verbose log records `speech_model_on_gpu` / `speech_model_on_cpu`.
