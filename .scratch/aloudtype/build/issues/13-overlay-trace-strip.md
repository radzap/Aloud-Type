# 13: Overlay: Trace strip

**What to build:** The default Listening indicator: a click-through, always-on-top, non-focus-stealing overlay that draws loudness as a trace strip at about 30 fps only during a Dictation, with fast-attack slow-release smoothing, about 120 ms in and 180 ms out, the red fault state that grows in about 220 ms with the reason and hint, and the amber Typing fault variant, at Bottom and Center.

**Blocked by:** 05 Listening faults and indicator state, 12 Tray and Startup notice

**Status:** resolved

- [ ] Overlay appears and animates with voice loudness and never takes focus or blocks clicks
- [ ] Red and amber states show the correct messages and sizes
- [ ] No timer runs outside a Dictation
- [ ] Verified over a fullscreen app and a multi-monitor setup, or the gaps are recorded

## Answer

Done (TDD) in two new modules of `crates/platform-windows`; shared files touched only by module lines in `lib.rs`. Hand-written FFI, no new dependencies.
- `overlay_model.rs` (pure, 22 tests): `Animator` (set indicator + `tick(now)`; returns `None` when nothing is on screen = stop the timer) with fast-attack/slow-release smoothing (tau 36 ms / 205 ms, about the prototype's per-frame factors), 64-sample scrolling trace, 120 ms fade in, 180 ms fade out, 220 ms ease-out grow from the strip to the fault size, text shown once mostly grown; `look()` maps core `Indicator` to size/tone/text (strip 150x22; red 280x66 with reason and hint, 280x44 when no hint; amber 300x44 or 300x62 for long messages; missing-microphone note 300x36); `place()` for Top/Bottom x Left/Center/Right within a monitor work area (default Bottom/Center, taskbar-aware, DPI-scaled margin); `render()` draws the rounded body and trace into a premultiplied BGRA buffer; `restore_alpha()` repairs alpha after GDI text. Red hints are defined here (core carries only the reason message).
- `overlay.rs`: `spawn(placement) -> OverlayHandle` runs a dedicated overlay thread with a layered window (TOPMOST, TOOLWINDOW, NOACTIVATE, LAYERED, TRANSPARENT; `HTTRANSPARENT` on hit test, `MA_NOACTIVATE`), `UpdateLayeredWindow` per-pixel alpha, GDI Segoe UI text, monitor/DPI from the foreground window's monitor. API: `set_indicator(Indicator)` (call with `ListeningMonitor::indicator()` on each controller tick; free while hidden), `set_placement`, `is_ticking`, `shutdown`. The 33 ms timer exists only while visible or fading; the thread otherwise blocks in `GetMessage`.
- Test on this machine (a real desktop session was available): one real-window test checks the style flags, click-through and no-activate replies, hidden and not ticking when idle, visible and ticking in a Dictation, window size 150x22 then 280x66 on a fault, foreground window unchanged, hidden and timer stopped after the fade-out, and a second Dictation working.

Gaps (not verified, need on-device checking, ticket 23): how it looks (colours, text, antialiasing, the amber text contrast); behaviour over fullscreen apps (borderless should show; exclusive fullscreen and games will not show a topmost window); a real multi-monitor and mixed-DPI setup (code uses the foreground window's monitor work area and `GetDpiForMonitor`, read when the overlay appears, but only the single-monitor case ran); the host executable must declare per-monitor DPI awareness in its manifest, otherwise Windows bitmap-scales the overlay; click-through in real apps beneath it (styles and hit-test reply are verified, real clicks are not); the fade is opacity only (the prototype also scaled from 85%); the hands-free/hold wiring of `set_indicator` into the controller is the integration ticket's job. Bar pill and Orb styles and the Settings-driven style/position are ticket 14 (the `Placement` enum here mirrors the config enums without depending on the config crate). Pre-existing, unrelated: `aloud-type-recognizer` test `engine_and_crate_versions_are_pinned` fails on this checkout (the Cargo.lock string match, probably CRLF line endings), also `cargo fmt --check` flags `examples/hook_console.rs`.
