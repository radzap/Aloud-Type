# 12: Tray and Startup notice

**What to build:** The resident process shows a tray icon with idle, listening and fault states, double-click and a Settings and Exit menu, the 3 second Startup notice naming the real Dictation key (never taking focus, not waiting for the model), single-instance behaviour (a second launch asks the first to open Settings), and the conditional 'Open microphone privacy settings' menu entry. Placeholder icon art until the artwork ticket lands.

**Blocked by:** 05 Listening faults and indicator state, 07 Windows keyboard hook

**Status:** resolved

- [ ] Tray states follow the engine's reported state, including the tooltip reason after an ending fault
- [ ] Double-click and the menu open the Settings request
- [ ] A second launch opens Settings in the first instance and exits
- [ ] Idle CPU is zero with no timers

## Answer

Done (TDD) in new modules of `crates/platform-windows`; shared files touched only by `lib.rs` module lines. Hand-written FFI, no new dependencies (no `tray-icon` crate).
- `tray_model.rs` (pure, 8 tests): `view(engine Tray, key_unavailable)` maps the core `Tray` to icon kind + tooltip (fault reason in tooltip; dead hook shows "Dictation key unavailable" and outranks the rest); `menu(blocked_fault_standing)` gives Settings / [Open microphone privacy settings only while the blocked fault stands] / Exit; double-click = OpenSettings; `startup_action(first_run_complete, key)` gives the notice text "Aloud Type is running. Hold Right Ctrl to dictate." from the real configured key (CamelCase key names split into words) or OpenSettings when setup is incomplete; `TrayState` suppresses unchanged updates; placeholder disc icons (grey/blue/red).
- `tray.rs`: hidden message window + `Shell_NotifyIconW` icon, popup menu, TaskbarCreated re-add, own-drawn Startup notice (topmost, no-activate, click-through, 3 s timer that exists only while the notice shows), privacy entry opens `ms-settings:privacy-microphone`. API: `TrayHandle::set_state(engine_tray, key_unavailable, blocked_fault_standing)` (thread-safe, used by the controller), `quit()`, and `run(handle, commands_tx, startup, initial)` which runs the main-thread message loop; commands are forwarded on a channel.
- `instance.rs`: named mutex; a second launch sets a named event and exits; the first instance's waiter thread sleeps in the kernel and calls a callback (one test with a second acquire in-process).
- `examples/tray_demo.rs`: headless smoke run showed the loop runs, second launch signals the first (OpenSettings received) and the loop exits on quit.
- Idle CPU: no timers or polling; message loop blocks in GetMessage, waiter blocks in WaitForSingleObject.

Gaps (need a real desktop, not verified): visual look of icons/notice/menu, light/dark taskbar legibility, notice not stealing focus in real apps (flags set, untested), notice is not DPI-aware and uses the default GDI font, tooltip/icon refresh on live Explorer restart, privacy entry opening the real Windows page, real idle CPU measurement. Icon art is placeholder (ticket 22). Wiring is for the integration ticket: the host must call `set_state` from the controller, act on `Command::OpenSettings`/`Exit` from the channel (spawn Settings via the pipe/process), and call `acquire` first in `main`. The second-launch signal uses a named event rather than the spec's named pipe (the pipe is not built yet); it is a tiny seam to swap later. Hook-dead detection (`key_unavailable`) is an input flag; the hook watchdog does not yet report permanent failure.
