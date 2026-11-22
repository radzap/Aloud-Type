# 08: Windows text injection

**What to build:** Holding the Dictation key on a real PC types a scripted phrase (from the fake recognizer) into the focused window, delivered as Unicode keyboard input batched per word, never via the clipboard, including characters outside the basic plane. If the foreground window is elevated when the key is pressed, no Dictation starts and the amber notice 'Can't type into an administrator window' is reported to the engine; a short or rejected injection ends the Dictation with 'Couldn't type into this window'.

**Blocked by:** 03 Streaming insertion, 07 Windows keyboard hook

**Status:** ready-for-agent

- [ ] Scripted text appears correctly in Notepad, a browser text field and a chat app
- [ ] Special characters and surrogate pairs are typed correctly
- [ ] Elevated-window and rejected-typing cases surface the two amber messages
- [ ] The app's own injected events never re-trigger the hook
