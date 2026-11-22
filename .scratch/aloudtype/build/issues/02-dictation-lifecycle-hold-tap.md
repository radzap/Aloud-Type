# 02: Dictation lifecycle: hold and tap

**What to build:** The core engine turns Dictation key events into a Dictation that starts and ends. Holding dictates while held; a quick tap (under 250 ms, a configurable hidden value) latches hands-free and the next key-down ends it. For a modifier Dictation key, pressing another key while it is held cancels that Dictation, discards its audio and asks the platform to replay the modifier plus the other key.

**Blocked by:** 01 Workspace, shared test support and CI

**Status:** resolved

- [x] Hold, tap-to-latch and end-of-latch behave as specified, driven only by key events and the fake clock
- [x] The chord rule cancels and requests the replay; it does not apply to a latched Dictation
- [x] Threshold edge cases (just under and over 250 ms) are covered by seam tests

## Answer

Done (TDD, vertical slices). Seam: `DictationEngine` in `crates/core/src/lifecycle.rs`, driven only by `FakeKeyboard` events and `FakeClock`; `poll()` returns `Effect`s (StartDictation, EndDictation, CancelDictation, ReplayModifierChord) and `phase()` exposes Idle/Holding/Latched.
- Hold ends on release at >= threshold; release under 250 ms (configurable `tap_threshold_ms`) latches; next key-down ends and its release is ignored. Tests cover 249/250/251 ms.
- Chord rule: with `modifier_key`, `OtherKeyDown` while held (any duration) emits Cancel + ReplayModifierChord; not applied when latched or for non-modifier keys.
- Gaps: engine is not yet wired to audio capture/overlay (later tickets consume Effects); the real replay and key swallowing are platform work (spike item 5). The hidden threshold is a plain config field here; reading it from the settings file belongs to the config ticket.
