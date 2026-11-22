# Host wiring notes (branch ticket/wiring)

## What is wired (crates/app)
- Threads: main = tray message loop; worker = controller + Whisper decoding (off the tray thread); hook, overlay, power, Settings pipe each on their own thread.
- Single instance (named mutex; second launch opens Settings in the first). Tray with state, menu (Settings, privacy while blocked, Exit), notices (`TrayHandle::notify`: invalid settings file, model notices), Startup notice naming the configured key, first run (models `setup_complete`) opens Settings instead of the notice.
- Key press before setup: `SetupGate` -> Settings opens, no Dictation starts.
- Overlay: `set_indicator` every tick from the controller; style/placement from settings, live.
- Pipe server: suspend/resume -> hook; reload-settings -> `SettingsHost::reload` (key, tap threshold/modifier rule, style, placement, memory, model; mic/language at next Dictation); retest-gpu -> `gpu::retest_on_disk`.
- Dictation key comes from settings (`dictation_vks`), including combinations; chord rule only for a lone modifier.
- `--probe-gpu`, `--settings`, `--verbose` log, supervisor (restart once, then fault), power (sleep/resume), hook health (`key_unavailable`), elevated-window gate, injection fault slot.
- After a spent restart budget the app stays resident: tray fault kept, hook suspended (key passes through), overlay fault hidden after 6 s, Settings still reachable. An idle worker death now shows its fault (`Controller::worker_fault` opens the monitor first).
- Per-monitor DPI manifest embedded via `crates/app/build.rs` (MSVC link args).
- Seams/tests: `wiring.rs` (`SettingsHost`/`Targets`, `Publisher`/`Surfaces`, `handle_report`, `settings_exe_beside`), `Controller::set_setup_gate/set_lifecycle`, `DictationEngine::set_config`; tests in `crates/app/tests/{wiring,controller}.rs`.
- Smoke (headless, fake model dir, isolated LOCALAPPDATA): resident process runs, second launch exits at once, `--verbose` writes the log, `--settings` starts the window, manifest present in the exe.

## Decisions / remaining gaps
- Settings is the sibling `aloud-type-settings-ui.exe` (the tray launches `aloud-type.exe --settings`, which runs that file). I did not link settings-ui into the app because the no-network audit (`scripts/aloud_release/audit.py`) requires the resident crate to link no HTTP/TLS crates. DECISION FOR A HUMAN: either ship `aloud-type-settings-ui.exe` in the zip/installer (packaging scripts currently list only the one exe) or relax the audit (e.g. `cargo tree --prune aloud-type-settings-ui`) and link it in.
- The Silero VAD model is not in the downloadable catalog and is not shipped; the gate is found lazily in the models folders and until found everything counts as speech (hallucination filter only). Needs a delivery decision (bundle or download).
- Suspend/resume from the pipe is applied on the worker's 20 ms loop; a Whisper decode in flight delays it by up to that decode (capture start in Settings could briefly race). A hook-owned suspend path could remove that.
- Settings are re-read only on `reload-settings` (no file watcher); hand edits need a Settings change or restart. Microphone and language apply at the next Dictation.
- Tray Exit stops the process with `process::exit`; the hook and overlay die with it. No graceful worker join.
- Not verifiable headlessly: real tray/overlay look, notice focus behaviour, key rebinding and combination feel, elevated-window gate against a real admin window, sleep/resume, Settings window raise, DPI behaviour (manifest present, not run on mixed DPI), Vulkan/GPU path, idle CPU/memory, real-model dictation after the restructure (e2e test still passes with JFK clip).
- Second launch still uses the named event (not the pipe); fine functionally.
- Fault display after a spent restart budget: Windows overlay fault is hidden after 6 s by the degraded loop (not by the monitor's own linger timing).

## Review fixes (branch ticket/review-fixes)
- Worker no longer polls: it blocks on a `Wake` (hook, pipe, power, residency load, settings-file watcher notify it) and uses a timeout only when `Controller::poll_interval()` has a deadline (a Dictation, key press, fault, or the idle-unload time). Measured idle CPU ~0 ms over 8 s. The hook thread keeps one 5 s watchdog timer.
- Settings file is watched with `ReadDirectoryChangesW` (`platform-windows/file_watch.rs`); pipe `reload-settings` still works.
- Both exes ship (zip, installer). Settings program also runs `--background-models` (no window): downloads the Silero VAD model (verified, `models::VAD_MODEL`) and, once a GPU probe is approved, the Small model, then makes it the default if the user still has the default model and tells the resident process. Resident spawns it via `BackgroundModels` (wiring.rs); the resident still links no network code. Settings also fetches the VAD model after a first speech-model download.
- Removed the dev model fallback (`ALOUDTYPE_MODEL_DIR`, `AloudTypeDev`, unchecksummed legacy model) from the shipped binary; state println! is only printed with `--verbose`.
- Default chunk is 600 ms (was 1000): first words ~1.8 s into speech on the jfk clip. 500 and 750 ms changed the confirmed text on that clip ("and"/"asked"), so 600 is tuned to the fixture; a smaller chunk raises the risk of typing an unstable word. `end()` reuses the last decode when no new audio arrived.
- Remaining: recent-text recognition prompt is still None; Small-model switch is evaluated after a model load / settings change / reload, not on a timer; real-device checks still open (tickets 22-24).
