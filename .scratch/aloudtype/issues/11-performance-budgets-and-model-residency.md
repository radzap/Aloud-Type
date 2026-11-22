# Performance budgets and model residency

Type: grilling
Status: resolved

## Question

Given the research (CPU-viable models are base/small, GPU optional, Settings as a separate short-lived process), what are the idle RAM/CPU budgets and the speech-to-typed-text latency target? Does the Speech model stay loaded between Dictations (fast start, hundreds of MB resident) or load on demand (slow start, minimal idle footprint), and when is it unloaded? Which default model does a CPU-only machine get versus one with a detected GPU?

## Answer

Resolved by grilling, 2026-11-22.

**Budgets are soft design targets, not release gates.** Guiding rule: keep system impact low, but never starve the Speech model. The engine gets the compute buffers and audio buffering it wants. Nothing wasteful is held: no second model copy outside a swap, no unused GPU backend, no audio history beyond the current Dictation. The benchmark spike is a nice-to-have in the backlog and blocks nothing.

- **Reference machine** (for orientation only, not measured as a gate): 2020-era laptop, 4 cores / 8 threads (i5-1135G7 / Ryzen 5 4500U class), 8 GB RAM, SSD, iGPU only.
- **Latency target**: from VAD end-of-speech to the last character of the utterance typed, for a 3-8 s utterance in Stable mode, excluding pause detection. p50 ≤ 1.0 s, p95 ≤ 2.0 s on the reference machine with the default CPU model; p50 ≤ 0.5 s with a confirmed GPU. If the default model is clearly too slow, prefer a smaller default model over relaxing the target.
- **Idle (Tray presence)**: 0% CPU, with no polling or timers (wakes only on hook events and tray messages). No audio stream open while idle. Non-model footprint target: working set ≤ 25 MB, private bytes ≤ 40 MB.
- **During a Dictation**: decode threads = min(physical cores − 1, 4), at least 1. The decode worker runs at below-normal priority; hook and audio threads run at normal or above. Peak RAM target: idle + model runtime size + ~50 MB.
- **Model residency**: the Speech model loads on a worker thread at app start, so Dictation can begin immediately. The Startup notice does not wait for the load. If the Dictation key is pressed before the load finishes, audio is captured and buffered at once and the first decode waits (no Listening fault).
  - New Settings item, **Speech model memory**: *Keep loaded* (default: always resident) or *Unload when idle* (the model is freed 10 minutes after the last Dictation, or after the initial load, and reloaded on the next Dictation key-down in parallel with audio capture). The 10 minutes is fixed. A change takes effect immediately, with no restart.
- **Model or GPU change from Settings**: the resident process loads the new model while the old one keeps serving, then swaps (peak memory is briefly two models). If the load fails, the old model stays and a notice is shown. The Settings-to-resident signalling is left to the architecture work.
- **Default model by hardware**: the first-run download is always base q5_1 (multilingual, ~60 MB), which is also the GPU probe model. CPU-only machines keep base q5_1. With a confirmed GPU, small q5_1 (~190 MB) downloads in the background and becomes the default once ready. large-v3-turbo is offered only with a confirmed GPU, as an opt-in. The user can override this in Settings. This supersedes the small-q5_1-for-everyone default suggested in the models research.
- **Glossary**: no change. "Keep loaded" and "Unload when idle" are setting labels; "warm window" is a spec-only design term.
