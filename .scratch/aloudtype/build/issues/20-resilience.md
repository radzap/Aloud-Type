# 20: Resilience

**What to build:** Suspend ends any Dictation and closes audio; resume re-installs the hook and re-enumerates microphones. A panicked recognizer or audio worker is restarted once and then shows a fault. If the hook cannot be restored the tray shows 'Dictation key unavailable'. An invalid settings file keeps the last good settings and shows a tray notice. Verbose logging is available only through a command-line flag and never records audio or dictated text.

**Blocked by:** 08 Windows text injection, 12 Tray and Startup notice

**Status:** resolved

- [ ] Sleep and resume leave the app working
- [ ] A forced worker panic restarts once, then faults visibly
- [ ] No log file is written without the verbose flag, and none ever contains audio or text

## Answer

Done (TDD, new modules; shared files touched minimally: `lib.rs` module lines, `hook.rs`, `tray.rs`).
- `core/supervisor.rs`: `Supervisor` (one restart per worker per process) and `run_supervised` (catch_unwind loop). A forced panic restarts once and recovers; a second panic reports `Faulted`: audio maps to a red Listening fault "Microphone error", recognizer to an amber Typing fault "Speech recognition stopped working".
- `core/power.rs`: `on_power` (Suspend: EndDictation, CloseAudio; Resume: ReinstallHook, ReenumerateMicrophones) and `PowerTracker` (dedups Windows' double resume notice). `platform-windows/power_win.rs`: hidden window mapping `WM_POWERBROADCAST` to `PowerEvent` on a channel (tested with simulated messages).
- `platform-windows/hook_health.rs` + `hook.rs`: `reinstall_now()` (for resume) and `key_unavailable()`, true while the last (re)install failed; feeds `TrayHandle::set_state(.., key_unavailable, ..)` (ticket 12's "Dictation key unavailable").
- `config/notice.rs`: `SettingsNotice` fires once per newly invalid settings file (last good settings are kept by ConfigFile); `TrayHandle::notify(text)` added to show it as a brief notice.
- `config/log.rs`: `--verbose` flag (`verbose_requested`) and `Logger`; without the flag no file or folder is ever created. `Event` holds only a `&'static str` label and a numeric code, so audio or dictated text cannot be passed in by construction.

Gaps: no controller/main exists yet, so nothing wires these together (host must: run workers via `run_supervised` and surface `Report::Faulted` as the fault, apply `on_power` actions, call `reinstall_now`, pass `key_unavailable` to the tray, call `SettingsNotice::observe` after each settings reload and `notify`, create the `Logger` from `std::env::args`). Not verified on a real device: actual sleep/resume (audio stream close, microphone re-enumeration, hook surviving resume), the notice's look, a real hook install failure (only the pure health flag is tested). The restart budget is never replenished within a process. The log is a simple append-by-rewrite (no rotation or size cap) and is written only in verbose mode.
