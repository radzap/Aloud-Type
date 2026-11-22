# 04: Speech filtering and decoder limits

**What to build:** The engine gates audio with the voice gate so non-speech never reaches the recognizer, drops segments with a high no-speech probability or matching a short hallucination blocklist, caps a single utterance at about 15 seconds, and ends the Dictation with the amber Typing fault 'Speech model is too slow for this PC. Choose a smaller model in Settings.' when the decoder is more than about 30 seconds behind real time, still typing audio captured so far.

**Blocked by:** 03 Streaming insertion

**Status:** ready-for-agent

- [ ] Silence and noise clips produce no typed text
- [ ] Blocklisted and low-confidence segments are dropped
- [ ] Utterances over the cap are split correctly
- [ ] The too-slow fault fires and still flushes captured audio
