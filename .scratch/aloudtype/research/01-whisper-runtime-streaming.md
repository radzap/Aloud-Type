# Whisper runtime and streaming approach in Rust

Ticket: `issues/01-whisper-runtime-streaming.md`. Researched 2026-11-22 against primary sources (project READMEs, headers, source, crates.io API, papers). Items marked **[unverified]** or **[inference]** are not stated by a source.

## Recommendation (short)

Use **whisper.cpp through the `whisper-rs` crate** (statically linked, CPU baseline, optional Vulkan feature for GPU), with a **VAD-gated, utterance-chunked** pipeline and a **commit-on-agreement** text policy for *Stable* typing. Ship *Live-correct* only as experimental, if at all.

Why, in one line each:
- whisper.cpp is MIT, actively released (v1.9.5, 2026-10-06), supports CPU AVX/AVX2 and Vulkan (cross-vendor, so it covers integrated and discrete GPUs on Windows), has quantized models and a built-in Silero VAD.
- `whisper-rs` exposes the whole needed surface (contexts, `FullParams`, the standalone VAD context) and is public domain, but is maintained thinly (see Risks).
- candle and ONNX paths carry no clear advantage for Whisper here and cost more (see comparison).

## 1. Runtime comparison

| Option | License | Maintenance (as of 2026-11-22) | Windows GPU story | Streaming/VAD support | Fit |
|---|---|---|---|---|---|
| whisper.cpp via `whisper-rs` | whisper.cpp MIT [1]; `whisper-rs` Unlicense [3][4] | whisper.cpp: tagged releases roughly every 2-6 weeks, latest v1.9.5 on 2026-10-06, nightlies to 20 Nov [2]. `whisper-rs` 0.16.0 on 2026-03-12, last repo commit 2026-03-14 [3][4] | Vulkan (`-DGGML_VULKAN=1`, cargo feature `vulkan`), CUDA, OpenBLAS, OpenVINO [1][3] | Built-in Silero VAD (`whisper_vad_*`, `FullParams.vad`) [1][5][6]; naive stream example only [7] | Best |
| candle (Whisper example) | MIT OR Apache-2.0 (`candle-core` 0.11.1, 2026-10-31) [8] | Active | CUDA/Metal only in candle proper **[inference: no Vulkan backend documented]** | Example reads WAV files only; no streaming, no mention of quantized Whisper [9] | Poor: no streaming path, GPU limited to NVIDIA on Windows |
| `ort` (ONNX Runtime wrapper) + exported Whisper | MIT OR Apache-2.0; only `2.0.0-rc.15` exists, no stable release (2026-10-19) [10] | Active but pre-release | DirectML/CUDA execution providers via ONNX Runtime **[unverified in this pass]** | You would write the decode loop, KV-cache handling and tokenizer yourself | Poor: most work, large runtime DLL, release-candidate API |
| sherpa-onnx (`sherpa-onnx` crate 2.0.2, 2026-11-19) | Apache-2.0 (repo and crate) [11][12] | Active | Windows x64/x86/arm64 supported [13] | True streaming models exist (Zipformer, Paraformer, CTC) but Whisper is listed only among **non-streaming** models [13] | Good as an alternative engine, not as a Whisper runtime |
| CTranslate2 / faster-whisper | not verified | n/a | n/a | Reference streaming project uses it as preferred backend [14] | No Rust binding found in this pass; C++/Python only. Rejected on footprint and FFI grounds **[inference]** |

Notes:
- `whisper-rs` Windows build needs Visual Studio C++, CMake and LLVM/clang (`LIBCLANG_PATH`) because bindings are generated with bindgen [15]. That is a developer/CI cost, not a runtime one. `WHISPER_DONT_GENERATE_BINDINGS=1` copies pregenerated bindings (possibly outdated) [4]. Vulkan builds additionally need the Vulkan SDK **[inference; BUILDING.md does not cover Vulkan]**.
- `whisper-rs` does not state which whisper.cpp revision it vendors; it is a git submodule [4]. Pin and record the commit when implementing, because upstream fixes (VAD, Vulkan) land in whisper.cpp first.
- whisper.cpp's v1.9.2 notes carry the most recent VAD work (exposed VAD segments, token timestamp mapping when VAD is on) [2]. Whether `whisper-rs` 0.16.0 includes that revision is **unverified**; `WhisperVadContext` is documented in 0.16.0 [5], so the standalone VAD exists there.

## 2. Speed on CPU

The only primary-source numbers found are community benchmark rows in whisper.cpp issue #89 [16]. They are *encoder time for one 30 s window* (whisper pads input to 30 s unless `audio_ctx` is reduced), in ms, with threads noted.

| CPU (OS) | Threads | tiny | base | small | medium | large |
|---|---|---|---|---|---|---|
| Ryzen 9 5950X (Ubuntu 22.04), AVX2 | 8 | 197 | 421 | 1393 | 4404 | 8118 |
| Ryzen 9 3900X (Ubuntu 20.04), AVX2 | 8 | 422 | 880 | 2874 | 9610 | 16917 |

Other rows, reported as-is [16]:
- Core i5-1135G7 (Windows 10), AVX2, CPU fallback, `medium`, 8 threads: encode 24,890 ms, decode 10,958 ms, total 133,450 ms. A laptop-class CPU is roughly 5x slower than a 5950X for `medium` and not usable interactively.
- Ryzen AI 9 HX 375 (WSL2), `base.en`, AVX512: encode 437 ms (16 threads) vs. Vulkan GPU 86 ms (8 threads). Vulkan cut encode about 5x on that iGPU, though decode was slower (1500 ms vs. 736 ms) in that run.
- Radeon RX 9070 XT (Windows 11), Vulkan, large v3 f16, flash attention on: encode 89 ms, decode 1424 ms, total 2719 ms.
- One Ryzen 7 5700X (Windows 11) row for `base.en` shows total 48.9 s with encode 7.8 s; the log appears to show a build without AVX2, so treat as a misconfigured build, not a typical result **[inference]**.

Reading for this app:
- Real-time budget: an utterance of N seconds is encoded as a full 30 s window by default, so latency is dominated by the fixed encoder cost, not N. On a fast desktop CPU, `base`/`small` encode in 0.4-1.4 s; `medium` and `large` are 4-8 s and are not viable on CPU for dictation.
- `audio_ctx` (shorter encoder context) is a documented experimental speed-up that "can significantly reduce output quality" [6]; the stream example's `--audio_ctx` defaults to 0 (full) [7]. Worth benchmarking on short utterances, but never default without a quality test.
- Quantization (q5_0) reduces memory and disk [1]; `large-v3-turbo-q5_0` is 547 MiB on disk vs 1.5 GiB unquantized [17]. No per-quantization speed numbers were found in primary sources, except the Xeon q5_0 turbo rows (encode about 14 s on 22 threads, a server CPU on Windows Server 2016) [16], which says turbo is not CPU-viable for dictation.
- Model memory (whisper.cpp README): tiny ~273 MB, base ~388 MB, small ~852 MB, medium ~2.1 GB, large ~3.9 GB [1]. Disk: 75 MiB / 142 MiB / 466 MiB / 1.5 GiB / 2.9 GiB [1][17].
- There are no primary-source benchmarks for the target hardware class; the ticket's "speed per model size" is therefore only anchored by the two Ryzen rows above. A benchmark task on 2-3 representative machines belongs in the implementation backlog, using `whisper-bench`-style runs **[inference]**.

Suggested defaults: `small` (or `small.en` when language is English) as the quality default on CPU, `base`/`base.en` as the "fast" option, `tiny` as a fallback. `medium`+ and `large-v3-turbo` only when the GPU backend is active. Model selection and language coverage are owned by ticket 06 (`research/06-models-and-languages.md`).

## 3. Binary and dependency size

- whisper.cpp is C/C++ with only ggml as a dependency, statically linkable via cargo build scripts [1]. SDL2 is needed only for the stream example, not for the library [1][7]; the app should use its own audio capture.
- Release v1.9.5 publishes no assets in the GitHub API response [2], so no official Windows binary sizes were available. **Exact `.exe` size is unverified**; a static CPU-only whisper.cpp build is typically single-digit MB **[inference, not sourced]**. Vulkan adds a runtime dependency on the system's Vulkan loader (`vulkan-1.dll`) shipped with GPU drivers **[inference]**, so no extra DLL is bundled, which is the main footprint advantage over CUDA (which requires CUDA runtime libraries).
- Models are separate downloads (see sizes above); the Silero VAD model is a further small file (`download-vad-model.sh` in the repo) [1].

## 4. Near-streaming design

### What the primary sources show

1. **whisper.cpp's own streaming is naive.** `whisper-stream` samples roughly every step (500 ms in the README example, 3000 ms code default) and re-transcribes a rolling window (5000 ms in the README example, 10000 ms code default), keeping `keep_ms` = 200 ms of tail; it erases and rewrites a terminal line, i.e. it re-decodes overlapping audio and its output text can change from pass to pass [7][18]. Its VAD mode (`--step 0`) is a simple energy/high-pass detector that transcribes the last `--length` ms after silence, and the README itself calls it naive [7][18]. Do not copy it as the design.
2. **Whisper is not natively streaming.** The reference approach (Macháček et al., IJCNLP 2023) turns it into a streaming system with a *local agreement policy*, reporting about 3.3 s latency on unsegmented long-form speech [14][19]. The implementation re-transcribes a growing buffer and emits only the prefix confirmed by 2 consecutive iterations (LocalAgreement-2); the buffer is trimmed after confirmed output at segment boundaries (default) or sentence ends [14]. I could not extract the paper's body text, so the algorithm description comes from the implementation's README [14].
3. **whisper.cpp ships a Silero VAD** with tunable threshold, min speech/silence durations, max speech duration, speech padding and overlap [1], plus `whisper_vad_detect_speech_no_reset` and `whisper_vad_reset_state`, which the header documents as intended for streaming (keep LSTM state across calls, reset between utterances) [6]. `whisper-rs` exposes `WhisperVadContext`/`WhisperVadParams`/`WhisperVadSegments` as a standalone VAD handle [5].
4. **Decode parameters that matter** [6]: `single_segment` ("useful for streaming"), `no_context` (don't prompt the decoder with past text), `initial_prompt` / `carry_initial_prompt`, `audio_ctx` (experimental), `temperature` fallback and the `no_speech_thold`/`logprob_thold`/`entropy_thold` family.

### Proposed pipeline (design recommendation, **[inference]** built on the above)

1. **Capture** 16 kHz mono PCM into a ring buffer while the Dictation key is active. Audio is the app's own (WASAPI), not SDL2.
2. **VAD gate** (Silero via `whisper_vad_*`, state carried across frames, reset at utterance end) segments speech into utterances and pads edges. VAD also drives the Listening indicator's "no sound after a while" Listening fault.
3. **Chunking**: decode on (a) VAD end-of-speech (pause of a few hundred ms) and (b) a forced cut at a maximum span (about 10-15 s, tied to VAD `max_speech_duration`) to bound latency and cost. Because the encoder cost is roughly fixed per 30 s window (section 2), decoding every 0.5 s like the stream example wastes most of the CPU; prefer fewer, larger decodes (0.8-1.5 s steps at most for *Live-correct*).
4. **Text policy for *Stable* (never backspaces)**: type only text that is *confirmed*. Two viable confirmation rules, both avoiding backspace:
   - **Finalize-on-pause (simplest, lowest CPU)**: decode each VAD utterance once at its end and type it whole. Latency = pause length + one decode. No partial text is ever typed, so no instability. Typing begins after the first pause rather than while speaking, which is "near-streaming" by phrase.
   - **LocalAgreement-2 within an utterance**: re-decode the growing utterance every step and type only the longest common word prefix of the last two hypotheses [14]; the tail stays unconfirmed until it agrees or the utterance ends. Costs several decodes per utterance (CPU), gives words while the user is still talking.
   Use `no_context = true` between utterances initially (as the stream example does by default [18]) to avoid cross-utterance hallucination loops; consider a short `initial_prompt` for vocabulary only after testing **[inference]**.
5. **Live-correct (experimental)**: type the current best hypothesis every step and diff against what was typed, backspacing the divergent suffix. Whisper hypotheses for the last words change frequently between re-decodes (the reason the agreement policy exists [14]), so expect visible flicker and backspacing in the target app; many apps (and elevated windows) handle injected backspaces poorly. Recommendation: ship *Stable* only in v1; keep Live-correct behind the setting only if a prototype proves tolerable. This is the "whether Live-correct ships" research outcome: **default to not shipping it in v1; decision for the spec ticket**.
6. **Hold vs. tap modes**: hold mode can flush the final decode on key release; tap-latch mode relies on VAD pauses for chunk boundaries.
7. **Hallucination guards** on silence/short clips: gate on VAD (never send non-speech to the decoder), and use `no_speech_thold`/`logprob_thold`/`entropy_thold` [6]. Whisper's known tendency to emit text on silence is why the VAD gate is mandatory **[inference, general model behavior, not sourced here]**.

### Alternative worth recording

True streaming, low-latency engines exist outside Whisper: sherpa-onnx streaming Zipformer/Paraformer/CTC models [13]; Moonshine reports about 5x less compute than Whisper tiny for a 10 s segment at equal WER and handles variable-length input without 30 s padding [20] (license of its weights/code was not stated on the page I read, so **unverified**; check before relying on it). These address the fixed-30 s-window cost directly and may matter if CPU latency on weak laptops proves unacceptable. Ticket 06 owns model choice; this ticket's conclusion is that the Rust runtime should sit behind a small recognizer seam so a sherpa-onnx engine could replace whisper.cpp later **[inference]**.

## 5. Risks and open items

- **`whisper-rs` bus factor**: single-maintainer; repo has no tags, moved to Codeberg; last release 0.16.0 on 2026-03-12 (about 8 months before this date) [3][4]; versions 0.14.0 and 0.15.0 were yanked [3]. Mitigation: pin version + whisper.cpp commit; alternatively generate bindings directly against whisper.cpp's C API (`whisper.h`, which is documented [6]) if `whisper-rs` stalls. Both options keep the same app seam.
- **Vulkan on integrated GPUs**: one benchmark showed large encode gains but slower decode on an iGPU [16]; recent v1.9.5 notes include Vulkan fixes inherited from ggml syncs (flash attention out-of-bounds write) [2], so GPU stability varies by driver. Keep the CPU fallback and the manual GPU override as already settled in the map; auto-detect should include a startup self-test decode **[inference]**.
- **CPU feature level**: the Windows benchmark row without AVX2 was ~10x slower [16]. Build/run-time selection of AVX2 (and AVX512 where present) matters; whisper.cpp exposes system info via `SystemInfo` in `whisper-rs` [5]. Whether to ship multiple CPU variants is unverified.
- **Unverified here**: exact release binary size; sherpa-onnx and CTranslate2 Windows footprints; ONNX Runtime DirectML details; per-model quantized CPU speed; Moonshine licensing; Whisper-Streaming paper's own numbers beyond the abstract.

## Sources

1. whisper.cpp README (license badge MIT, backends, memory/disk, VAD flags, Windows, stream): https://github.com/ggml-org/whisper.cpp (raw: https://raw.githubusercontent.com/ggml-org/whisper.cpp/master/README.md)
2. whisper.cpp releases (v1.9.5, cadence, VAD/Vulkan notes): https://github.com/ggml-org/whisper.cpp/releases ; API: https://api.github.com/repos/ggml-org/whisper.cpp/releases/latest
3. crates.io `whisper-rs` (0.16.0, Unlicense, yanks): https://crates.io/api/v1/crates/whisper-rs
4. `whisper-rs` repo and README (features, submodule, bindings env var, moved to Codeberg): https://codeberg.org/tazz4843/whisper-rs ; https://raw.githubusercontent.com/tazz4843/whisper-rs/master/README.md
5. `whisper-rs` 0.16.0 API docs (VAD structs, `SystemInfo`): https://docs.rs/whisper-rs/0.16.0/whisper_rs/
6. `whisper.h` (FullParams, VAD API doc comments): https://raw.githubusercontent.com/ggml-org/whisper.cpp/master/include/whisper.h
7. whisper.cpp stream example README: https://raw.githubusercontent.com/ggml-org/whisper.cpp/master/examples/stream/README.md
8. crates.io `candle-core`: https://crates.io/api/v1/crates/candle-core
9. candle Whisper example README: https://raw.githubusercontent.com/huggingface/candle/main/candle-examples/examples/whisper/README.md
10. crates.io `ort`: https://crates.io/api/v1/crates/ort
11. crates.io `sherpa-onnx`: https://crates.io/api/v1/crates/sherpa-onnx
12. sherpa-onnx license (Apache-2.0): https://api.github.com/repos/k2-fsa/sherpa-onnx/license
13. sherpa-onnx README (model families, streaming, Windows, Rust): https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/master/README.md
14. whisper_streaming (LocalAgreement-2, trimming, min-chunk, successor SimulStreaming): https://raw.githubusercontent.com/ufal/whisper_streaming/main/README.md
15. `whisper-rs` BUILDING.md (Windows: VS C++, CMake, LLVM): https://raw.githubusercontent.com/tazz4843/whisper-rs/master/BUILDING.md
16. whisper.cpp benchmark thread (community rows quoted above): https://github.com/ggml-org/whisper.cpp/issues/89
17. whisper.cpp models README (sizes, quantized, turbo, distil): https://raw.githubusercontent.com/ggml-org/whisper.cpp/master/models/README.md
18. whisper.cpp `stream.cpp` (defaults step/length/keep, no_context, single_segment): https://raw.githubusercontent.com/ggml-org/whisper.cpp/master/examples/stream/stream.cpp
19. Macháček, Dabre, Bojar, "Turning Whisper into Real-Time Transcription System", IJCNLP 2023 demos; abstract: https://arxiv.org/abs/2307.14743 ; https://aclanthology.org/2023.ijcnlp-demo.3/
20. Moonshine paper abstract: https://arxiv.org/abs/2410.15608
