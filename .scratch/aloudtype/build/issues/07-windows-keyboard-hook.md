# 07: Windows keyboard hook

**What to build:** On a real PC, the configured Dictation key (default Right Ctrl) drives the core engine through a low-level keyboard hook on its own thread that only classifies, swallows or passes events and posts them. Includes modifier swallowing with the replay for the chord rule, the mask-key handling for a lone Alt or Win, tagging of the app's own injected events, and a watchdog that re-installs the hook if Windows removes it. Verified through a debug console showing Dictation start and end.

**Blocked by:** 02 Dictation lifecycle: hold and tap

**Status:** resolved

- [ ] Holding and tapping the key produces the expected Dictation events on a real machine
- [ ] Ctrl-based shortcuts still work through the chord rule
- [ ] The watchdog recovers from a forced hook removal
- [ ] The hook callback never blocks

## Answer

Done (TDD). `crates/platform-windows`:
- `classify.rs`: pure `Classifier` (tested): swallow/post of Dictation key down/up, auto-repeat swallowed, chord rule (swallow the other key, post `OtherKeyDown`, inject tagged modifier-down + key-down replay, let the real modifier-up through), mask-key (Left Ctrl tap) for lone Alt/Win, own-event filtering via `INJECT_TAG` dwExtraInfo (foreign injected input still handled), and the `should_reinstall` watchdog policy (not while key held).
- `hook.rs`: `KeyboardHook` (implements core `Keyboard`): `WH_KEYBOARD_LL` on its own message-loop thread; callback only classifies and posts to channels (replay `SendInput` runs on a separate worker thread; never blocks). Watchdog = 1 s thread timer, re-installs every 10 s (configurable) unless the key is held. `simulate_forced_removal()` / `reinstall_count()` for verification.
- `examples/hook_console.rs`: debug console printing engine effects; `remove` arg forces hook removal. Smoke run headlessly: hook installs, forced removal then watchdog re-install (count 1) observed.
- Hand-written FFI, no new dependencies.

Gaps (need a real keyboard/device, not verified): hold/tap feel, Ctrl shortcuts via chord replay (replay is async via a worker, so ordering vs. fast typing is unverified), whether swallowed modifier hides state from target apps, mask key effectiveness on Windows 11 for Alt/Win, elevated-window behaviour (UIPI), real Windows-initiated hook removal (only simulated), watchdog interval tuning (10 s is a guess; periodic re-install changes hook chain order), hook timestamps unused (engine uses poll-time clock from ticket 02).
