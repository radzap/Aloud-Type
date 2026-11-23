# Map: AloudType

Label: wayfinder:map

## Destination

A build-ready spec at `.scratch/aloudtype/spec.md` for AloudType: a free, fully local, Windows-first Rust dictation app. Done when nothing is left to decide before implementation tickets can be cut. No application code is written in this effort.

## Notes

- Use the vocabulary in `GLOSSARY.md` (Dictation, Dictation key, Streaming insertion, Listening indicator, Listening fault, Tray presence, Startup notice, Settings window, Speech model).
- The effort is modelled on the desktop (not cloud).
- Planning only: tickets produce decisions, not deliverables. Tray icon art (OpenArt) is generated at implementation time, outside this map; the spec only carries the icon brief.
- Standing preferences: very fast, minimal OS impact; fully offline after the one-time Speech model download; no cloud, no telemetry, no LLM rewriting.
- Not a git repo: research findings go to `.scratch/aloudtype/research/<name>.md` instead of a branch.

### Settled by grilling

- Platform: Windows-only v1 behind a platform seam; macOS/Linux out of scope.
- Dictation key: rebindable in Settings; hold = dictate while held; tap = latch hands-free, tap again to end; one key does both (separate per-mode keys dropped by owner in the key-defaults ticket).
- Typing: Stable only (never backspaces). Live-correct was cut from v1 (see Out of scope).
- Hardware: CPU baseline; auto-detect integrated/discrete GPU on first start with CPU fallback and a manual override in Settings. Backend chosen by research.
- Language: fixed language (English default) or Auto; mixed-language quality is a research outcome.
- Settings window contents: Dictation key(s) and modes, microphone (System default first, then choices), Speech model choice + download status (selecting a missing model downloads it), language, GPU override, Speech model memory (Keep loaded default / Unload when idle), launch at login, Listening indicator style (Bar pill / Orb / Trace strip, default Trace strip) and position (Top/Bottom x Left/Center/Right, default Bottom Center). Nothing else in v1.
- Settings window opens via double-click on the tray icon or the tray context menu; the menu also has Exit.
- First run: Settings opens automatically on every start until a Speech model is installed (key, mic, model download); the Dictation key before then brings Settings to the front. After setup, later starts show only the Startup notice, then Tray presence.
- Listening indicator: minimal overlay during Dictation showing loudness/variation of the voice. On Listening fault (no microphone, no access, or no sound after a while) it turns red, grows, and shows the reason when known.
- Model delivery: not bundled; downloaded on first run / on demand.

## Decisions so far

- [Whisper runtime and streaming](issues/01-whisper-runtime-streaming.md): whisper.cpp via `whisper-rs` (pin crate + vendored commit, recognizer seam, sherpa-onnx as fallback); built-in Silero VAD gates audio; chunk per utterance (max ~10-15 s); CPU-viable models are base/small; Stable commit via finalize-on-pause or LocalAgreement-2; Live-correct not recommended. Findings: `research/01-whisper-runtime-streaming.md`.
- [GPU acceleration](issues/02-gpu-acceleration.md): Vulkan via whisper.cpp with `GGML_BACKEND_DL` (optional DLL, CPU always baseline); first-start probe in a child process comparing GPU vs CPU (~1.3x speedup + matching output), persisted per device/driver, runtime guard, Settings override (Auto/CPU/device). iGPU gains are unproven for streaming-size chunks: prototype needed.
- [Windows key capture and text injection](issues/03-windows-input-capture-injection.md): one `WH_KEYBOARD_LL` hook on its own thread with a re-install watchdog; hold/tap state machine on hook timestamps; prefer a non-modifier default key; `SendInput` Unicode batched per word, no clipboard, own events tagged; elevated windows block injection silently, so detect and show a notice; Live-correct is feasible but fragile (opt-in, bounded tail, fall back to Stable).
- [Audio capture and failure detection](issues/04-audio-capture-and-faults.md): cpal/WASAPI shared event-driven behind a seam; open stream per Dictation; own resampling to 16 kHz mono; RMS/peak every 20-30 ms for the indicator; twelve fault reasons catalogued; Windows privacy-denial behaviour unconfirmed (needs on-device test; layered detection proposed).
- [Rust UI stack](issues/05-ui-stack.md): resident process uses no toolkit (`tray-icon` + `muda`, hand-drawn layered Win32 overlay and Startup notice); Settings is a short-lived `aloudtype.exe --settings` process using iced + tiny-skia; egui and Slint rejected. Click-through and no-focus-steal need verifying.
- [Speech model lineup and languages](issues/06-models-and-languages.md): quantised ggml models (base.en q5_1 ~60 MB, small q5_1 ~190 MB suggested default, small.en q5_1, large-v3-turbo only with a GPU/opt-in); `.en` models disable the language setting; fixed language is reliable, Auto picks one language per utterance, mid-sentence switching not guaranteed.
- [License compatibility for the app](issues/12-license-compatibility.md): nothing planned blocks AGPL-3.0, GPL-3.0, MIT or Apache-2.0 (all code deps GPLv3-compatible; Whisper/ggml/Silero weights MIT, downloaded on demand); only obligation is a THIRD-PARTY-NOTICES file; AGPL's network clause has nothing to attach to in a local app, so GPL-3.0-or-later is the simpler fit; OpenArt icon needs Plus tier or above for commercial use (hand-drawn avoids it); transitive crates unaudited (`cargo-deny`/`cargo-about`). **Owner's decision: AGPL-3.0** (open detail: "only" vs "or later").
- [Performance budgets and model residency](issues/11-performance-budgets-and-model-residency.md): budgets are soft targets, not gates (p50 ≤ 1 s / p95 ≤ 2 s end-of-speech to typed on a 2020 laptop CPU; 0% idle CPU, ~25 MB non-model working set; decode threads min(cores − 1, 4) at below-normal priority); model loads at app start and stays resident by default, with a new Settings item *Unload when idle* (10 min); model swaps load the new one before releasing the old; default model base q5_1 on CPU, small q5_1 with a confirmed GPU, turbo opt-in.

- [Streaming insertion stability and latency rules](issues/07-streaming-stabilization-rules.md): confirmed-prefix streaming (two decodes agree) with automatic type-on-pause fallback; ~1-2 s lag, flush within ~1 s of ending; last word held back until confirmed; model punctuation as-is with one space between chunks; VAD gate plus hallucination filter; text goes to current focus like a keyboard; no Live-correct in v1.
- [Listening indicator look](issues/10-listening-indicator-look.md): three styles ship (Bar pill, Orb, Trace strip), user-selectable in Settings, default Trace strip; position is Top/Bottom x Left/Center/Right, default Bottom Center; Caret buddy dropped; timing ~120 ms in / 180 ms out / 220 ms fault grow; fault = red, bigger, reason + hint (wording from Listening fault rules). Prototype: `prototypes/10-listening-indicator-PROTOTYPE.html`.
- [Dictation key defaults and conflict handling](issues/08-dictation-key-defaults.md): single key for hold and tap, default Right Ctrl, 250 ms tap/hold threshold (hidden config), chord rule cancels the Dictation and replays the shortcut, reject lone typing keys and warn on OS shortcuts, no conflict detection beyond a static warning list, capture-on-release rebind UI with the resident hook suspended, no cancel key.
- [Listening fault rules](issues/09-listening-fault-rules.md): ending faults (mic unusable) stop the Dictation; "No sound detected" (no voice in first 4 s) is recoverable and self-clears; stall >1 s / digital silence 4 s fault anytime; missing chosen mic falls back to System default with a note; seven reason texts; hold lingers 2 s, latched auto-dismisses 5 s; tray icon shows ending faults until next good start; privacy tray-menu link; registry pre-check trusts only explicit Deny.
- [Architecture and module seams](issues/14-architecture-and-module-seams.md): one `aloudtype.exe` with resident / `--settings` / `--probe-gpu` modes; crates core, platform-windows, recognizer, config, settings-ui with trait seams; plain threads + channels, controller owns state; config file as truth + named pipe for live commands; Settings process downloads models (resumable, checksummed); `Recognizer` and `VoiceGate` traits with word timings/confidence; GPU probe in a child process; no logging unless a verbose flag is passed (never audio or text); controller restarts a failed worker once.
- [Config persistence, first-run flow and Startup notice](issues/15-config-persistence-and-first-run.md): `%LOCALAPPDATA%\AloudType\` with `config.toml` (Settings writes) and `state.toml` (producer writes), atomic writes, last good config kept on invalid file; versioned migration, newer configs never overwritten, unknown keys preserved; setup complete = a model installed, Settings opens each start until then (also on Dictation key); Startup notice ~3 s with the real key name, no setup variant.
- [Packaging, launch at login and updates](issues/16-packaging-autostart-updates.md): portable zip + per-user Inno Setup installer (no MSIX); launch at login is a Settings toggle (HKCU Run key, off by default); program files separate from `%LOCALAPPDATA%\AloudType\` user data; manual updates only, version + releases link in Settings; AGPL text, THIRD-PARTY-NOTICES (`cargo-about`/`cargo-deny`) and a source link in Settings; sign when a certificate is available; release checklist (trademark, Store, winget, domain, clean-VM smoke test) goes in the spec.
- [Error handling and edge cases](issues/17-errors-and-edge-cases.md): new amber Typing fault (red stays for Listening faults): elevated window (no Dictation, 2 s notice) and rejected input (ends Dictation); download failures shown inline with Retry, resumable, verified-only loading; hook loss shows tray fault after silent re-installs; GPU error finishes on CPU and is recorded; single instance (second launch opens Settings); suspend ends Dictation, resume refreshes hook and mics; utterance cap ~15 s and an amber fault if the decoder lags over ~30 s.

## Not yet specified

- On-device verification plan (privacy-denial behaviour and the registry pre-check, no-sound thresholds against the 4 s window, click-through, iGPU streaming latency): a list of spikes for the implementation backlog. CPU latency benchmarking is a nice-to-have, not a release gate.
- Tray icon brief (states: idle, listening, fault; fault state + reason tooltip per Listening fault rules) for later OpenArt generation.

## Out of scope

- Live-correct (type-then-backspace) typing mode: fragile across apps, cut from v1 by [Streaming insertion stability and latency rules](issues/07-streaming-stabilization-rules.md); may return later as an opt-in experiment.
- macOS and Linux support.
- Cloud transcription, accounts, telemetry, LLM rewriting or command mode.
- GPU-specific tuning beyond the auto-detect and override.
- Per-sentence mixed-language guarantees.
- Generating the tray icon artwork (happens at implementation). Owner has OpenArt Plus (commercial use OK); plan is a concept via OpenArt, then a simplified vector for the 16-32 px tray sizes.
