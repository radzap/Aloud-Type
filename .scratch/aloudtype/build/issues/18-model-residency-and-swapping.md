# 18: Model residency and swapping

**What to build:** The model loads at start on a worker thread so Dictation is available at once (a Dictation before load finishes buffers audio and waits), 'Keep loaded' and 'Unload when idle' (10 minutes, reload in parallel with capture) both work and change immediately, and switching models loads the new one while the old keeps serving, then swaps, keeping the old model if the load fails and showing a notice.

**Blocked by:** 11 First end-to-end dictation, 16 Model manager and first-run flow

**Status:** ready-for-agent

- [ ] Both residency modes behave as specified
- [ ] A model swap never interrupts a Dictation
- [ ] A failed swap keeps the old model
- [ ] Idle memory with Unload matches the footprint target or the gap is recorded
