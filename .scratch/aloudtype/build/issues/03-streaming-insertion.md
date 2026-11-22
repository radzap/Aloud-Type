# 03: Streaming insertion

**What to build:** With a scripted recognizer, a Dictation produces typed text as it is confirmed: only leading words two consecutive decodes agree on are typed, the last word is held back until confirmed, one space precedes each later chunk and none at the start or end, and ending the Dictation flushes everything. If the recognizer cannot keep up, the engine falls back to typing the whole utterance when the speaker pauses.

**Blocked by:** 02 Dictation lifecycle: hold and tap

**Status:** ready-for-agent

- [ ] Typed output matches the confirmed-prefix, held-back-word and spacing rules for scripted decodes
- [ ] Ending a Dictation flushes all pending words
- [ ] Slow-recognizer scripts trigger the type-on-pause fallback automatically
- [ ] No test depends on internal state
