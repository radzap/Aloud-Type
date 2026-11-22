# 20: Resilience

**What to build:** Suspend ends any Dictation and closes audio; resume re-installs the hook and re-enumerates microphones. A panicked recognizer or audio worker is restarted once and then shows a fault. If the hook cannot be restored the tray shows 'Dictation key unavailable'. An invalid settings file keeps the last good settings and shows a tray notice. Verbose logging is available only through a command-line flag and never records audio or dictated text.

**Blocked by:** 08 Windows text injection, 12 Tray and Startup notice

**Status:** ready-for-agent

- [ ] Sleep and resume leave the app working
- [ ] A forced worker panic restarts once, then faults visibly
- [ ] No log file is written without the verbose flag, and none ever contains audio or text
