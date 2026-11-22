# Architecture and module seams

Type: grilling
Status: resolved

## Question

Given all decisions so far, what are the processes and modules of Aloud Type and the seams between them: resident process (hook thread, audio capture, recognizer, injector, tray, overlay) versus the short-lived Settings process; the platform seam for Windows-only code; the recognizer seam (whisper-rs now, sherpa-onnx possible); thread model and channels; and how the Settings process signals the resident process (model switch, residency mode, GPU override, key-capture suspend)? Decide the crate/workspace layout and the IPC mechanism.

## Answer

Decided with the owner:
- **Layout**: one Cargo workspace producing one `aloudtype.exe` with three modes: resident (default), `--settings` (short-lived Settings window), `--probe-gpu` (GPU probe). Crates: `core` (no OS code: Dictation state machine, confirmed-prefix stabilizer, fault rules, spacing rules), `platform-windows` (keyboard hook, text injection, audio device details, tray, overlay), `recognizer` (traits + whisper-rs implementation), `config` (shared settings types and file I/O), `settings-ui` (iced). `core` talks only to traits implemented by `platform-windows` and `recognizer`: these are the platform seam and the recognizer seam, and let `core` be tested with fake keyboards, fake audio and recorded clips.
- **Threads (resident)**: plain threads and channels, no async runtime. Main thread runs the Win32 message loop (tray, overlay). A dedicated hook thread only posts events. A controller thread owns the Dictation state machine (single state owner, no locks across the Dictation flow). The audio callback feeds a ring buffer; a recognizer worker decodes from it; an injector sends text to the focused window.
- **Settings to resident**: the config file is the source of truth for saved settings, watched and reloaded by the resident process. A named pipe (resident = server) carries live commands and status: suspend Dictation keys during key capture, model switch/unload requests, download progress. Settings still edits the config file when the resident process is not running.
- **Model downloads**: owned by the Settings process (progress UI, resumable `.part` files, checksum verification, then move into the models folder). The resident process only loads models from that folder and learns of completed files via the pipe or folder watch. Keeps the HTTP client out of the resident process.
- **Recognizer interface**: two engine-agnostic traits. `Recognizer` takes a window of 16 kHz mono audio plus language and an optional prompt of recent text and returns text segments with word timings, word confidence and a no-speech probability (the stabilizer needs timings and confidence). `VoiceGate` reports speech/non-speech for an audio slice (Silero VAD behind it). v1 supports Whisper-style windowed decoding only.
- **GPU probe/backend**: the probe runs as a child process (`--probe-gpu`) with a timeout, result saved in config per device and driver. The resident process loads the Vulkan backend DLL only when the probe approved it and the setting allows it; otherwise it is never loaded.
- **Logging and privacy**: NO logging by default (owner's change to the proposal). A log file is written only when the app is started with a verbose command-line flag, in the app data folder; even then audio and transcribed text are never logged. No telemetry, no network calls except the model download. Tray menu stays Settings, Exit and the privacy link. Faults still reach the user through the Listening indicator and tray, independent of logging.
- **Failure supervision**: the controller restarts a panicked recognizer or audio worker once, then shows a Listening fault if the restart fails. The keyboard hook has the re-install watchdog from the research. With verbose on, a panic hook writes the cause to the log. No whole-process auto-restart; launch at login covers the next session.
