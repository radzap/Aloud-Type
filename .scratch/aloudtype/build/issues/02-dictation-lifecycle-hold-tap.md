# 02: Dictation lifecycle: hold and tap

**What to build:** The core engine turns Dictation key events into a Dictation that starts and ends. Holding dictates while held; a quick tap (under 250 ms, a configurable hidden value) latches hands-free and the next key-down ends it. For a modifier Dictation key, pressing another key while it is held cancels that Dictation, discards its audio and asks the platform to replay the modifier plus the other key.

**Blocked by:** 01 Workspace, shared test support and CI

**Status:** ready-for-agent

- [ ] Hold, tap-to-latch and end-of-latch behave as specified, driven only by key events and the fake clock
- [ ] The chord rule cancels and requests the replay; it does not apply to a latched Dictation
- [ ] Threshold edge cases (just under and over 250 ms) are covered by seam tests
