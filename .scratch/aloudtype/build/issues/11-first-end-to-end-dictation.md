# 11: First end-to-end dictation

**What to build:** The walking skeleton: hold Right Ctrl, speak, and text appears in Notepad as it is confirmed, with a console reporting state and no UI. Wires the keyboard hook, audio capture, Whisper recognizer, streaming insertion and text injection together on a machine with the base model installed by hand.

**Blocked by:** 04 Speech filtering and decoder limits, 08 Windows text injection, 09 Audio capture and metering, 10 Whisper recognizer

**Status:** ready-for-agent

- [ ] Speaking a sentence types it into Notepad within the latency target on a reference-class machine, or the gap is recorded
- [ ] Ending the Dictation flushes the remaining words
- [ ] The six core behaviours (hold, tap, streaming, spacing, flush, fallback) are demonstrated end to end
