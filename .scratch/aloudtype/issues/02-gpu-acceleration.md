# Integrated and discrete GPU acceleration and detection
Type: research
Status: resolved

## Question
Which GPU backends work on Windows for Intel/AMD integrated GPUs and others (Vulkan in whisper.cpp, DirectML/ONNX), what is the real speedup versus CPU, how can the app detect a usable GPU on first start and fall back safely, and what does it cost in binary size and idle memory?

## Answer
Use **Vulkan via whisper.cpp** (`whisper-rs` `vulkan` feature), built with `GGML_BACKEND_DL` so the Vulkan backend is an optional separate DLL; CPU is always the baseline. Vulkan covers Intel, AMD and NVIDIA, and iGPUs are supported since whisper.cpp v1.8.3. OpenVINO is Intel-only and encoder-only. DirectML/ONNX is in sustained engineering and would replace the runtime, so it is not recommended.

Speedup is real but uneven. The PR author reports about 3-4x realtime factor on recent Radeon 680M and Intel Arc iGPUs. The "12x" headline is unsubstantiated, and the GPU decoder can be slower than CPU, so tiny models and old iGPUs may not benefit.

ggml has no whole-model fallback, so the app provides it. On first start, check that `vulkan-1.dll` and the backend DLL load. Then probe in a child process with a timeout, benchmark GPU against CPU on the real Speech model, and require a speedup and matching output. Persist the result per device and driver, and offer a Settings override of Auto, CPU only, or a specific device.

Cost is about +5 MB installer and +25-65 MB on disk for the Vulkan DLL. No first-party idle-RAM figure exists; it is zero when the backend is not loaded. Follow-up: prototype streaming-chunk latency on real Intel and AMD iGPUs.

Full findings and sources: [research/02-gpu-acceleration.md](../research/02-gpu-acceleration.md)
