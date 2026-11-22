# 05: Listening faults and indicator state

**What to build:** The core engine reports the Listening indicator and tray state: loudness level, the recoverable 'No sound detected' (no voice in the first 4 s, clears when a voice is heard), ending faults (stall over 1 s, digital silence for 4 s, and the platform-reported reasons) with the seven reason messages, the missing-microphone fallback note, hold-mode lingering of 2 s, hands-free auto-dismiss after 5 s or on the next tap, a key press during a lingering fault starting a new Dictation, and text and audio handling around an ending fault.

**Blocked by:** 02 Dictation lifecycle: hold and tap

**Status:** ready-for-agent

- [ ] Every fault rule and timing from the spec is covered by a seam test using the fake clock
- [ ] Red (Listening) and amber (Typing) states are distinct in the reported state
- [ ] Audio captured before an ending fault is flushed and typed
- [ ] The recoverable fault never changes the tray state
