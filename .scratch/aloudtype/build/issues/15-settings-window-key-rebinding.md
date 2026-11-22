# 15: Settings window and Dictation key rebinding

**What to build:** A short-lived settings process opened from the tray that talks to the resident process over the named pipe and edits the user settings file. First setting: click-to-capture rebinding of the Dictation key with the rules (single key or combination, left and right modifiers distinguished, reject lone typing keys, Win+L and Ctrl+Alt+Del, warn on system shortcuts, Esc or 10 s timeout cancels, the resident hook suspended while capturing, the help line for intercepted keys).

**Blocked by:** 06 Config and state files, 07 Windows keyboard hook, 12 Tray and Startup notice

**Status:** ready-for-agent

- [ ] Settings opens from the tray and frees all memory when closed
- [ ] Rebinding works end to end and applies without restart
- [ ] Rejected and warned keys show the right messages
- [ ] Settings works when the resident process is not running
