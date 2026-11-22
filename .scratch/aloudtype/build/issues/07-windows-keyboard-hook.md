# 07: Windows keyboard hook

**What to build:** On a real PC, the configured Dictation key (default Right Ctrl) drives the core engine through a low-level keyboard hook on its own thread that only classifies, swallows or passes events and posts them. Includes modifier swallowing with the replay for the chord rule, the mask-key handling for a lone Alt or Win, tagging of the app's own injected events, and a watchdog that re-installs the hook if Windows removes it. Verified through a debug console showing Dictation start and end.

**Blocked by:** 02 Dictation lifecycle: hold and tap

**Status:** ready-for-agent

- [ ] Holding and tapping the key produces the expected Dictation events on a real machine
- [ ] Ctrl-based shortcuts still work through the chord rule
- [ ] The watchdog recovers from a forced hook removal
- [ ] The hook callback never blocks
