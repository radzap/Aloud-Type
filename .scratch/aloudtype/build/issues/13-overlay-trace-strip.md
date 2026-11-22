# 13: Overlay: Trace strip

**What to build:** The default Listening indicator: a click-through, always-on-top, non-focus-stealing overlay that draws loudness as a trace strip at about 30 fps only during a Dictation, with fast-attack slow-release smoothing, about 120 ms in and 180 ms out, the red fault state that grows in about 220 ms with the reason and hint, and the amber Typing fault variant, at Bottom and Center.

**Blocked by:** 05 Listening faults and indicator state, 12 Tray and Startup notice

**Status:** ready-for-agent

- [ ] Overlay appears and animates with voice loudness and never takes focus or blocks clicks
- [ ] Red and amber states show the correct messages and sizes
- [ ] No timer runs outside a Dictation
- [ ] Verified over a fullscreen app and a multi-monitor setup, or the gaps are recorded
