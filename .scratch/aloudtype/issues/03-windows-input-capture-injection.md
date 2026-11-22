# Windows global key capture and text injection
Type: research
Status: resolved

## Question
How should the app capture a rebindable global Dictation key (hold and tap detection, modifier-only keys, low-level hooks vs RegisterHotKey vs raw input) and inject text as if typed (SendInput unicode, clipboard avoidance, IME and elevated-window limits, apps that reject injected input)? Include feasibility and risks of sending Backspace for a Live-correct typing mode.

## Answer

- Capture with one `WH_KEYBOARD_LL` hook on a dedicated message-loop thread whose callback only classifies, swallows (nonzero return) or passes, and posts to a channel. RegisterHotKey has no key-up (no hold) and no documented modifier-only support. Raw Input cannot swallow keys. Hold vs tap is a small state machine on hook timestamps (start capture on key-down; tap-release latches; threshold is ticket 08's call, about 200-300 ms to start). The hook can be removed silently on timeout (max 1000 ms), so add a watchdog/re-install heartbeat.
- Inject with `SendInput` + `KEYEVENTF_UNICODE` (VK_PACKET), batched per word, never the clipboard. Tag our events via `dwExtraInfo` and ignore them in our own hook (`LLKHF_INJECTED`).
- Elevated windows are a hard limit: UIPI blocks injection and the API reports no error. Detect the foreground process's elevation and show a notice instead of failing silently. Skip uiAccess in v1 (signing, secure-location and assistive-tech rules).
- Prefer a non-modifier default Dictation key (modifiers held during typing risk shortcuts; Alt/Win need a menu-mask trick). Feeds ticket 08.
- Live-correct (Backspace) is feasible but fragile: no read-back, grapheme and editor differences, user interference, undo-stack noise. If it ships: opt-in and experimental, rewrite only a bounded tail with exact typed-character counts, send backspaces plus replacement in one batch, and freeze to Stable on any real key, focus change or short `SendInput`. Feeds ticket 07.
- Rust: `windows-sys` plus about 150 lines of hand-written FFI; no input-simulation crate. Spike list (elevated-hook behaviour, modifier swallowing, chunk pacing, surrogates, IME, Live-correct per app) is in the findings.

Full findings and source links: [research/03-windows-input-capture-injection.md](../research/03-windows-input-capture-injection.md)
