# 15: Settings window and Dictation key rebinding

**What to build:** A short-lived settings process opened from the tray that talks to the resident process over the named pipe and edits the user settings file. First setting: click-to-capture rebinding of the Dictation key with the rules (single key or combination, left and right modifiers distinguished, reject lone typing keys, Win+L and Ctrl+Alt+Del, warn on system shortcuts, Esc or 10 s timeout cancels, the resident hook suspended while capturing, the help line for intercepted keys).

**Blocked by:** 06 Config and state files, 07 Windows keyboard hook, 12 Tray and Startup notice

**Status:** resolved

- [ ] Settings opens from the tray and frees all memory when closed
- [ ] Rebinding works end to end and applies without restart
- [ ] Rejected and warned keys show the right messages
- [ ] Settings works when the resident process is not running

## Answer

Done (TDD where logic exists), with the real-desktop checks recorded as gaps. 24 settings-ui tests plus new tests in core and platform-windows; `cargo test --workspace` and `cargo deny check licenses` pass.

What exists:
- `crates/core`: `keynames` (VK <-> stored key names, canonical order, shared by Settings and the hook) and `ipc` (`Command`, `PIPE_NAME`, per-connection `Connection` that resumes keys when a client that suspended them disconnects).
- `crates/settings-ui`: `rules` (reject lone typing keys, Win+L, Ctrl+Alt+Del, bad combos; warn on Alt+Tab, Alt+F4, Win+letter, Ctrl+Space, Right Alt, lone Alt/Win), `capture` (peak set recorded on release, Esc and 10 s timeout cancel, a rejection shows its reason and keeps capturing), `session` (suspend on click, save and reload on success, resume on every exit path including window close, works with no resident process), `ipc` client, `codes` (iced physical key names to VK, display text), and the iced window (`window.rs`, tiny-skia only, no wgpu) with the rebinding button, "Press a key..." state, reject/warning messages and the help line for intercepted keys. `run()` is the entry point the host calls for `--settings`; a stand-alone `aloud-type-settings-ui.exe` runs the same. Smoke: the exe starts and stays alive (killed by timeout), nothing more.
- `crates/platform-windows`: `pipe_server` (real named pipe `\.\pipe\AloudType`, remote clients rejected, one client at a time; tested over a real pipe: commands acknowledged, unknown command refused, auto-resume when the client drops while suspended, next client served), `settings_launcher` (spawns `<exe> --settings`, never two at once, reopens after close; tested with fakes), hook `set_keys` / `set_suspended` for live rebinding and suspension, and `Classifier` combination support (modifiers pass through, only the non-modifier key is swallowed, start when the last key of the set goes down, end when any is released; suspended classifier passes everything).
- `deny.toml`: added BSL-1.0 (iced deps) and Unlicense (whisper-rs, previously failing).
- Merged integration/aloudtype (the recognizer pin test fix comes from there).

Gaps (not verified, no real desktop/keyboard):
- Window look, click-to-capture feel, and whether winit delivers all keys (Win key, Right Alt/AltGr reports an extra LeftCtrl on some layouts) are unverified. Alt+Tab, Win+L, Ctrl+Alt+Del never reach the window anyway.
- Memory freed on close relies on process exit; not measured. Iced startup time not measured.
- Host wiring is not done because no host binary exists yet: the host must (a) call `PipeServer::start(PIPE_NAME, ...)` mapping SuspendKeys/ResumeKeys to `KeyboardHook::set_suspended`, and ReloadSettings to reloading settings then `set_keys` with `vk_for_name` of `dictation_key`, (b) route `Command::OpenSettings` from the tray to `SettingsLauncher::open`, (c) call `aloud_type_settings_ui::run()` when started with `--settings`. The hook live-control messages (`set_keys`, `set_suspended`) are not exercised by a test (they need a real hook thread).
- Combination limits in the hook: the chord rule and the Alt/Win mask-key trick are single-key only; with a combo the modifiers reach the app (so Ctrl+Q type bindings show Ctrl briefly to apps) and a combination containing a lone Alt/Win may open menus. Generic modifier codes (VK_CONTROL) are not bound. Combos are untested against real keyboards.
- Tray double-click and second-launch events use the existing named event; moving it to the pipe is left to the integration ticket.
