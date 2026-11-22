# 10: Whisper recognizer

**What to build:** The recognizer crate implements the recognition and voice-gate traits on top of the pinned Whisper binding with the base model: a window of 16 kHz audio in, text segments with word timings, word confidence and a no-speech probability out, with language and optional prompt, and decode threads set per the performance rules. Verified offline by decoding recorded clips through the same interface the engine uses.

**Blocked by:** 01 Workspace, shared test support and CI

**Status:** ready-for-agent

- [ ] A recorded English clip decodes to the expected words with timings and confidence
- [ ] The voice gate separates speech from silence on recorded clips
- [ ] Language selection (fixed and Auto) is honoured
- [ ] Model, crate and vendored engine versions are pinned
