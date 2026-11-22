# 02 - GPU acceleration and detection (Windows, Rust, offline)

Researched 2026-11-22. Ticket: `../issues/02-gpu-acceleration.md`.
Sources are primary (whisper.cpp / ggml source and PRs, ONNX Runtime docs, whisper-rs manifest). Where only secondary reports exist, that is flagged. Items marked UNVERIFIED are not backed by a first-party measurement.

## Verdict (short)

1. **Backend: Vulkan via whisper.cpp/ggml.** It is the only cross-vendor (Intel, AMD, NVIDIA) GPU path shipped in whisper.cpp and exposed by the Rust binding. DirectML/ONNX is not recommended.
2. **Ship the CPU baseline always; ship Vulkan as an optional, separately loaded DLL** (`GGML_BACKEND_DL`). If the DLL or `vulkan-1.dll` is missing or Vulkan init fails, ggml never registers the backend and whisper.cpp runs on CPU.
3. **Do not treat "a GPU exists" as "the GPU is faster".** Integrated GPUs can be slower than the CPU. Decide by a one-time micro-benchmark (GPU vs CPU on the real Speech model) on first start, store the result, and offer a Settings override (Auto / CPU / specific GPU).
4. Run first-time GPU probing in a **child process** so a driver crash cannot take the app down.
5. Cost: roughly +25 to +65 MB on disk for the Vulkan backend DLL (shaders), about +5 MB compressed installer. No first-party idle-RAM figure exists; with the backend in a separate DLL, a CPU-only user pays nothing at runtime.

## 1. Backends that work on Windows

### Vulkan (whisper.cpp / ggml) - recommended
- whisper.cpp README: Vulkan is a "cross-vendor solution which allows you to accelerate workload on your GPU", requires a driver with Vulkan support, built with `-DGGML_VULKAN=1`. https://github.com/ggml-org/whisper.cpp/blob/master/README.md
- Integrated GPU support is explicit: `whisper_backend_init_gpu()` accepts both `GGML_BACKEND_DEVICE_TYPE_GPU` and `GGML_BACKEND_DEVICE_TYPE_IGPU` and picks the Nth match of `params.gpu_device` (default 0; `use_gpu` default true). Source: https://github.com/ggml-org/whisper.cpp/blob/master/src/whisper.cpp (`whisper_backend_init_gpu`, `whisper_context_default_params`). The iGPU fix is PR #3492 (shipped in v1.8.3): backend init "now properly recognizes and utilizes integrated GPUs". https://github.com/ggml-org/whisper.cpp/pull/3492
- The Vulkan backend reports IGPU vs GPU itself (`ggml_backend_vk_device_get_type`). https://github.com/ggml-org/whisper.cpp/blob/master/ggml/src/ggml-vulkan/ggml-vulkan.cpp
- Windows build needs the LunarG Vulkan SDK (glslc, headers) at build time only; runtime needs only the vendor driver's `vulkan-1.dll`. https://github.com/ggml-org/llama.cpp/blob/master/docs/build.md (same ggml backend).
- Rust binding: `whisper-rs` 0.16.0 has a `vulkan` feature (also `cuda`, `hipblas`, `intel-sycl`, `openblas`, `coreml`, `metal`); none enabled by default. https://codeberg.org/tazz4843/whisper-rs/raw/branch/master/Cargo.toml
- Upstream Windows release zips (v1.8.3) are CPU, BLAS and cuBLAS only (cuBLAS 12.4 zip ~460 MB, 11.8 zip ~62 MB, plain x64 ~4 MB). No upstream Vulkan Windows binary, so we build it. https://api.github.com/repos/ggml-org/whisper.cpp/releases/tags/v1.8.3 (latest v1.9.5, 2026-10-06, lists no assets via the API).

### OpenVINO (Intel only) - not recommended as primary
- Accelerates the encoder only, on x86 CPUs and Intel GPUs (integrated and discrete); first run compiles the model (slow, then cached); needs the OpenVINO runtime and a model conversion step; README gives no numbers. https://github.com/ggml-org/whisper.cpp/blob/master/README.md
- Intel-only plus extra runtime and per-model conversion conflicts with "minimal footprint". Vulkan already covers Intel iGPUs.

### CUDA (NVIDIA only) - skip for v1
- Fast, but the upstream Windows cuBLAS zip is 62-460 MB and NVIDIA-only. Vulkan also drives NVIDIA cards. Vendor-specific tuning is out of scope per the map.

### DirectML / ONNX Runtime - not recommended
- ONNX Runtime docs: DirectML is "in sustained engineering"; new feature development moved to WinML. Needs a DirectX 12 device (Intel Haswell HD and newer, NVIDIA Kepler and newer). The provider does not support memory-pattern optimization or parallel execution, and only one thread may call `Run` per session. https://onnxruntime.ai/docs/execution-providers/DirectML-ExecutionProvider.html
- Using it would mean abandoning whisper.cpp for an ONNX Whisper export plus ORT DLLs, on a provider in maintenance. (The "no upside over Vulkan" judgement is mine; the maintenance status is from the doc.)
- Revisit only if WinML becomes a stable Rust-reachable path; out of scope for v1.

## 2. Real speedup vs CPU

First-party numbers are thin; treat all as order of magnitude.

| Evidence | Result | Source |
|---|---|---|
| PR #3492 author; Ryzen 7 6800H (Radeon 680M) and Core Ultra 7 155H (Arc iGPU); Vulkan | "3-4x better realtime factor" vs CPU-only (CPU RTF ~0.3). Title says "12x"; the PR never reconciles the two and gives no model size or timings. Do not budget on 12x. | https://github.com/ggml-org/whisper.cpp/pull/3492 |
| Benchmark thread; Ryzen AI 9 HX 375 / Radeon 890M; base.en; under WSL via Dozen (non-conformant driver) | Encode ~87 ms (Vulkan) vs ~395-437 ms (CPU, AVX512). But the poster says full transcription of jfk.wav was faster on CPU: Vulkan decode ~1500 ms vs low tens of ms. | https://github.com/ggml-org/whisper.cpp/issues/89 |
| Secondary: whisper medium, x86 iGPU vs CPU | ~4.3x (10,370 vs 2,408, units presumed ms, iGPU unnamed) | https://dev.to/maruhana/whisper-turbo-vs-medium-on-arm-and-cuda-the-faster-model-still-flipped-between-cpu-and-gpu-5g59 |
| Secondary: Parakeet (not whisper) on Iris Xe Vulkan | ~1.9x | https://whispaste.de/en/engine-benchmarks/ |

Takeaways:
- The encoder gets big wins on iGPU; the decoder (many tiny sequential steps) can be slower on GPU than on a good CPU. Dictation on short utterances is decoder-heavy, so the gain on tiny/base models may be small or negative; it matters most for small and larger models.
- Planning range: about 2-4x on recent iGPUs with small-and-larger models; about 1x or worse for tiny models or old iGPUs. Real numbers must come from the first-start benchmark, not the spec.
- Prototype needed (not answerable from docs): CPU vs Vulkan latency for streaming-size chunks with the chosen model on at least one Intel and one AMD iGPU. Feed this to the streaming-runtime ticket (01) and the performance-budget decision.

## 3. Detection and safe fallback

### What the code does
- No GPU device registered: `whisper_backend_init_gpu` logs "no GPU found" and returns null; whisper.cpp uses the CPU backend. Device init failure logs an error and returns null (CPU). Source: `src/whisper.cpp`.
- `ggml_backend_vk_reg()` wraps `ggml_vk_instance_init()` in try/catch (`vk::SystemError`, `std::exception`, `...`) and returns null on failure, so Vulkan is simply not registered. Source: `ggml/src/ggml-vulkan/ggml-vulkan.cpp`.
- With `GGML_BACKEND_DL`, `ggml_backend_load_all()` tries each backend DLL (`vulkan`, `cuda`, `cpu`, ...), loads the best-scoring CPU variant, and a missing DLL is skipped. `GGML_BACKEND_PATH` loads an out-of-tree backend; `GGML_DISABLE_VULKAN` blocks registration in static builds. Sources: https://github.com/ggml-org/whisper.cpp/blob/master/ggml/src/ggml-backend-reg.cpp and `ggml/CMakeLists.txt` (`GGML_BACKEND_DL`, "build backends as dynamic libraries").

### Known failure modes (why benchmark and isolation are needed)
- Maintainer on issue #2411 (Vulkan crashes or fails to load on many machines): "I think the fallback mechanism only applies to operators that are not yet implemented on the backend." So there is no automatic whole-model fallback if Vulkan loads but misbehaves; he suggests the app check GPU memory before loading. https://github.com/ggml-org/whisper.cpp/issues/2411
- Old Intel GPUs (before Gen8/Broadwell) lack `VK_KHR_16bit_storage`, which the backend needs (secondary, downstream report): https://github.com/jatinkrmalik/vocalinux/issues/238
- Reports of very poor recognition with Vulkan on some setups: https://github.com/ggml-org/whisper.cpp/issues/2400 (v1.7.1, old; re-check on the pinned version).
- Windows non-ReBAR discrete GPU: ~24x slowdown, workaround `GGML_VK_DISABLE_HOST_VISIBLE_VIDMEM=1` (variable confirmed in ggml source; the report is secondary): https://github.com/cjpais/Handy/issues/2535
- Downstream Windows build note (secondary): not loading `ggml-vulkan.dll` at all is safer than `use_gpu=false`, because a driver crash during Vulkan startup happens at backend init. https://github.com/TheRealEminem/trailmix/pull/11
- Env switches verified in the ggml-vulkan source (escape hatches, set before init): `GGML_VK_VISIBLE_DEVICES`, `GGML_VK_DISABLE_F16`, `GGML_VK_FORCE_MAX_ALLOCATION_SIZE`, `GGML_VK_DISABLE_HOST_VISIBLE_VIDMEM`, `GGML_VK_PREFER_HOST_MEMORY`, `GGML_VK_DISABLE_COOPMAT`.

### Proposed first-start flow (for the spec)
1. Is `ggml-vulkan.dll` present and does `vulkan-1.dll` load? If not: CPU, done (Settings shows "No usable GPU").
2. Spawn a probe child process (same exe, hidden flag) with a timeout (for example 20 s). It enumerates devices (whisper.cpp already logs each device and type), loads the selected Speech model on GPU, transcribes a fixed test clip, and times it against a CPU run. Crash, timeout, error, or empty/garbage text rejects the GPU.
3. Persist the result per (device name + driver version): chosen backend, device index, measured speedup. Re-probe only when device, driver, or model class changes.
4. Use the GPU only if speedup is at least about 1.3x (a tuning threshold, not from a source) and output matches CPU within tolerance.
5. Settings "GPU override": Auto (default) / CPU only / specific GPU device. Applies on next model load.
6. Runtime guard: if a GPU transcription errors or exceeds a latency ceiling, reload on CPU, show a Startup-notice-style message, and remember the failure.

Open detail: `gpu_device` counts GPU and IGPU devices together in enumeration order, and ordering can affect speed (llama.cpp issue #30841). Persist by device name and map to index at load time.

## 4. Cost: binary size and idle memory

### Disk
- `ggml-vulkan.dll` reported at about 24-54 MB depending on version (secondary, dated): https://www.dllme.com/dll/files/ggml-vulkan
- A downstream Windows app that added the Vulkan backend (alongside nine CPU-variant DLLs) went from installer 77 to 82 MB (+5 MB) and installed 339 to 403 MB (+64 MB), attributed mostly to shaders in `ggml-vulkan.dll`. Secondary but concrete: https://github.com/TheRealEminem/trailmix/pull/11
- Plan: about +25-65 MB uncompressed, about +5 MB compressed installer. Measure on our own build. CUDA would be an order of magnitude bigger.
- `GGML_BACKEND_DL` produces several CPU-variant DLLs; keeping only chosen variants reduces footprint (packaging ticket decision).

### Memory
- No first-party idle-RAM numbers found; the downstream PR reports none either. From the code (reasoning, not a measurement): no Vulkan instance or device exists until the backend is registered, so with a separate DLL a CPU-only user never loads it. When active, model weights live in GPU-visible memory (system RAM for iGPU, VRAM for discrete) instead of, not in addition to, CPU RAM, plus a compute buffer and driver overhead. Cards with 2-4 GB VRAM can show pressure alongside other apps (same PR).
- Spec implication: decide whether the Speech model stays loaded between Dictations. Unloading when idle saves RAM/VRAM but costs reload latency (Vulkan pipeline/shader setup cost is UNVERIFIED; measure). Flag to the performance-budget ticket.

## 5. Answers to the ticket question

- **Which backends work for Intel/AMD iGPUs and others?** Vulkan in whisper.cpp (all vendors, iGPU supported since v1.8.3). OpenVINO is Intel-only and encoder-only. DirectML/ONNX works but is in sustained engineering and would replace the runtime. CUDA/HIP are vendor-specific and heavy.
- **Real speedup?** About 3-4x realtime factor on recent AMD/Intel iGPUs per the PR author (the 12x headline is unsubstantiated); decode can be slower than CPU; small models and old iGPUs may not benefit. Must be measured per machine.
- **Detect a usable GPU and fall back safely?** Check `vulkan-1.dll` and the backend DLL; probe in a child process with timeout; benchmark GPU vs CPU on the real model; require speedup and matching output; persist per device+driver; Settings override. ggml has no whole-model fallback, so the app must provide it.
- **Cost?** About +5 MB installer / +25-65 MB on disk for the Vulkan DLL; idle RAM impact unmeasured, zero when the backend is not loaded.

## Gaps / follow-ups
- Prototype CPU vs Vulkan latency for streaming-size chunks on real Intel and AMD iGPUs.
- Measure actual `ggml-vulkan.dll` size and idle/active RSS in our own build.
- Confirm issue #2400 (bad Vulkan recognition) no longer reproduces on the pinned whisper.cpp version.
