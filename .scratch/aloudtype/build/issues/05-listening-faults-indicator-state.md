# 05: Listening faults and indicator state

**What to build:** The core engine reports the Listening indicator and tray state: loudness level, the recoverable 'No sound detected' (no voice in the first 4 s, clears when a voice is heard), ending faults (stall over 1 s, digital silence for 4 s, and the platform-reported reasons) with the seven reason messages, the missing-microphone fallback note, hold-mode lingering of 2 s, hands-free auto-dismiss after 5 s or on the next tap, a key press during a lingering fault starting a new Dictation, and text and audio handling around an ending fault.

**Blocked by:** 02 Dictation lifecycle: hold and tap

**Status:** resolved

- [ ] Every fault rule and timing from the spec is covered by a seam test using the fake clock
- [ ] Red (Listening) and amber (Typing) states are distinct in the reported state
- [ ] Audio captured before an ending fault is flushed and typed
- [ ] The recoverable fault never changes the tray state

## Answer

Done (TDD). Seam: `ListeningMonitor` in `crates/core/src/listening.rs`, driven by `FakeClock`, audio packets (`audio(samples, voice)`), `platform_fault`, `typing_fault`, `start/latch/release/end/key_down`, `tick()`. Observed through `indicator()` (Hidden / Listening{level, note} / red ListeningFault / amber TypingFault), `tray()` (Idle / Listening / Fault(reason)), `take_actions()` and `blocked_fault_standing()`. 18 tests cover: loudness level; No sound at 4 s (recoverable, clears on voice, never returns, never changes tray); stall over 1 s; 4 s exact digital silence (nonzero sample resets); all seven reason messages; hold linger 2 s after release; hands-free 5 s auto-dismiss or tap-dismiss only; key press during lingering fault starts a new Dictation; fallback microphone note (3 s); tray fault persisting until a voice is heard in the next Dictation; privacy-menu flag for the blocked fault; ending faults emit one `EndDictationFlushAudio`.
- Gaps: the host must wire `Action::EndDictationFlushAudio` to flush captured audio through the recognizer and type it (typing pipeline is ticket 03+; only the action is emitted here), and must reset `DictationEngine` to Idle on a hands-free fault (the lifecycle engine is not modified; use `KeyOutcome` to decide whether a key-down starts or only dismisses). Typing faults set the amber indicator and end the Dictation but do not touch the tray (spec only defines tray fault for Listening faults). Linger/dismiss timings for Typing faults reuse the Listening-fault rules (assumption). Stall maps to "Microphone disconnected" and digital silence to "Microphone is muted or silent"; simultaneous no-sound and silence at 4 s resolves to the ending fault. The 4 s window starts at `start()`, so stream-open latency counts against it (spike item 3).
