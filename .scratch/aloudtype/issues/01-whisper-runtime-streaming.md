# Whisper runtime and streaming approach in Rust
Type: research
Status: resolved

## Question
Which Rust-usable Whisper runtime (whisper.cpp bindings, candle, ONNX-based options) best fits a fast, low-footprint Windows app, and how can it produce near-streaming partial output (chunking, VAD, sliding windows, stability of partial text)? Report speed per model size on CPU, binary/dependency size, licensing, and maintenance health.

## Answer
Use whisper.cpp (MIT, releases every 2-6 weeks, latest v1.9.5) via the `whisper-rs` crate (Unlicense, 0.16.0; thinly maintained, so pin the version and whisper.cpp commit and keep a recognizer seam). CPU is the baseline (AVX2 is essential, about 10x slower without); optional Vulkan feature covers integrated and discrete GPUs on Windows with CPU fallback. Reference speed (30 s encoder window, 8 threads, Ryzen 5950X): tiny 0.2 s, base 0.4 s, small 1.4 s, medium 4.4 s, large 8.1 s; so `small`/`base` are the CPU-viable models and medium+/turbo need the GPU. candle (no streaming path, no Vulkan) and ort (rc-only API, DIY decode loop) are rejected; sherpa-onnx (Apache-2.0) is the fallback engine for true streaming. Streaming design: own WASAPI capture, Silero VAD gate (built into whisper.cpp), utterance chunking with a forced max span, and a commit-only text policy for Stable mode (finalize on pause, or LocalAgreement-2 prefix confirmation); never send non-speech to the decoder. Live-correct (typing + backspacing) is likely to flicker and misbehave in target apps; recommend not shipping it in v1. Full findings, sources and unverified items: [research/01-whisper-runtime-streaming.md](../research/01-whisper-runtime-streaming.md)
