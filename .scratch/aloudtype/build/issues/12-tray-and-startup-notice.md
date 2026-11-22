# 12: Tray and Startup notice

**What to build:** The resident process shows a tray icon with idle, listening and fault states, double-click and a Settings and Exit menu, the 3 second Startup notice naming the real Dictation key (never taking focus, not waiting for the model), single-instance behaviour (a second launch asks the first to open Settings), and the conditional 'Open microphone privacy settings' menu entry. Placeholder icon art until the artwork ticket lands.

**Blocked by:** 05 Listening faults and indicator state, 07 Windows keyboard hook

**Status:** ready-for-agent

- [ ] Tray states follow the engine's reported state, including the tooltip reason after an ending fault
- [ ] Double-click and the menu open the Settings request
- [ ] A second launch opens Settings in the first instance and exits
- [ ] Idle CPU is zero with no timers
