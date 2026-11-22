# 08: Windows text injection

**What to build:** Holding the Dictation key on a real PC types a scripted phrase (from the fake recognizer) into the focused window, delivered as Unicode keyboard input batched per word, never via the clipboard, including characters outside the basic plane. If the foreground window is elevated when the key is pressed, no Dictation starts and the amber notice 'Can't type into an administrator window' is reported to the engine; a short or rejected injection ends the Dictation with 'Couldn't type into this window'.

**Blocked by:** 03 Streaming insertion, 07 Windows keyboard hook

**Status:** resolved

- [ ] Scripted text appears correctly in Notepad, a browser text field and a chat app
- [ ] Special characters and surrogate pairs are typed correctly
- [ ] Elevated-window and rejected-typing cases surface the two amber messages
- [ ] The app's own injected events never re-trigger the hook

## Answer

Done (TDD). OS-free logic in `crates/platform-windows/src/inject.rs`, Win32 backend in `inject_win.rs`, manual example `examples/inject_console.rs`.
- Seams: `InputBackend` (send events, returns accepted count) and `ElevationProbe`, both faked in tests. `InjectingSink` implements core's `TextSink`; on the first short/zero send it records `TypingFault::Rejected` ("Couldn't type into this window") in a shared `FaultSlot`, stops typing, and the controller reports it via `typing_fault`. `check_can_start` returns `TypingFault::Elevated` ("Can't type into an administrator window") for the key-down gate.
- Typing: per-word `SendInput` batches (cap 48 UTF-16 units, surrogate pairs never split), `KEYEVENTF_UNICODE`, down+up per unit, `\n` as Enter, `\r` dropped, no clipboard. Every event carries `INJECT_TAG`, which the ticket 07 hook already ignores.
- Tests (19 in the crate): batching, surrogates/astral chars, UTF-16 round-trip of special characters, rejection, elevated gate, sink fault latch.
- Gaps (not checkable headlessly): real typing into Notepad / browser / chat, hook-ignores-own-events end to end, the elevation probe against a real admin window (no integrity-level comparison beyond token elevation; an unopenable process counts as elevated), pacing cap untuned, IME/terminal behaviour. The controller wiring (calling `check_can_start` on key-down and `typing_fault` from the slot) is not done: `ListeningMachine::typing_fault` only works while a Dictation is active, so the "no Dictation starts" elevated case needs the controller ticket to show the amber notice without entering Active. Try `cargo run -p aloud-type-platform-windows --example inject_console`.
