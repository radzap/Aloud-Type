# 16: Model manager and first-run flow

**What to build:** The Settings window lists the supported speech models with sizes and download status, downloads the selected model with progress, resumable partial files and checksum verification, shows the inline errors with Retry, and the resident process loads only verified files. First run: Settings opens automatically at every start until a model is installed, the base model is one click away, and pressing the Dictation key before setup brings Settings to the front.

**Blocked by:** 06 Config and state files, 15 Settings window and Dictation key rebinding

**Status:** resolved

- [ ] Interrupted downloads resume; a corrupted download is rejected after one automatic retry
- [ ] Unverified models are never loaded
- [ ] First-run behaviour matches the spec until a model is installed
- [ ] English-only models disable the language setting

## Answer

Done (TDD; seams: `aloud-type-models` public API, `download_model` against a local test HTTP server, `ModelManager`, `SettingsLauncher`). `cargo test --workspace` and `cargo deny check licenses` pass; no real network is used in tests.
- New crate `crates/models` (no network code, only `sha2`): catalog of 6 quantised models (base q5_1 first-run, small q5_1, base.en, small.en, tiny.en, large-v3-turbo q5_0) with sizes and SHA-256 taken from the Hugging Face LFS metadata (base-q5_1 is the existing settings default); `verified_path` (size + SHA-256) is the only way the resident process should obtain a loadable path; `setup_complete(dir)` (first-run completion), `language_setting_enabled(id)` (false for English-only).
- `settings-ui::download`: `Http` trait + `UreqHttp` (ureq 3, OS TLS via native-tls/schannel, so no ring/rustls); `.part` file resumed with a Range request (a server that ignores Range restarts from zero); a read failure keeps the partial file (`Download interrupted`), unreachable server gives `No internet connection`, checksum mismatch deletes the file and retries once automatically, then `File failed verification`; the file is renamed into the models folder only after verification.
- `settings-ui::manager::ModelManager`: model rows (size, Downloaded / Downloading % / Failed with message), selecting a not-downloaded model requests its download and selects it when done (spec 67), selecting English-only forces language to `en` and `language_enabled()` is false, completion sets `first_run_complete` in state, failures record partial bytes in `state.downloads`, selection sends `ReloadSettings`.
- Settings window: Speech model section (first-run one-click "Download Base" while no model is installed, list with Use / Download / progress / Retry, English-only language line). Downloads run on a worker thread inside the Settings process.
- `platform-windows::settings_launcher`: a request while Settings is already open now brings that window to the front (`Child::bring_to_front`, FFI EnumWindows/SetForegroundWindow).

Gaps / for the host ticket: the real window look and download over the real Hugging Face URL/TLS were not exercised (only the exe starting and staying alive). Host wiring needed: at start pass `!aloud_type_models::setup_complete(models_dir)` as the tray's `first_run_complete` input so Settings opens at every start until a model is installed; on a Dictation key press while setup is incomplete send `Command::OpenSettings` (launcher raises the window); the resident process must load only `verified_path` results (and could re-hash at load). No cancel button for a running download (closing Settings stops it; the `.part` file resumes next time). Progress is polled every 300 ms. The language selector itself is ticket 17; Settings shows only the English-only note. Bring-to-front is subject to Windows foreground-lock rules and is unverified. A download with a model larger than the declared size is treated as a checksum failure. Only one model download at a time per model id (several ids may download in parallel).
