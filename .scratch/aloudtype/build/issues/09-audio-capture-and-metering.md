# 09: Audio capture and metering

**What to build:** The real microphone feeds the engine: shared-mode event-driven capture opened only for a Dictation, converted to 16 kHz mono, with loudness metering every 20-30 ms. System default follows the Windows default and a default change mid-Dictation keeps the current stream. The capture layer classifies failures into the Listening fault reasons (no microphone, disconnected, in use, blocked by Windows, muted or silent, other) and runs the registry privacy pre-check that faults only on an explicit Deny.

**Blocked by:** 05 Listening faults and indicator state

**Status:** ready-for-agent

- [ ] Real audio reaches the engine and the level reflects loudness
- [ ] Unplugging, muting and blocking the microphone each produce the right reason
- [ ] A missing chosen microphone falls back to System default with the 2 s note
- [ ] No audio stream is open while idle
