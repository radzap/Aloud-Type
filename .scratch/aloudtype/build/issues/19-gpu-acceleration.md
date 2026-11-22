# 19: GPU acceleration

**What to build:** The optional GPU backend library and the GPU probe child process: on first start the probe benchmarks GPU against CPU on the real model, requires a meaningful speedup and matching output, and saves the result per device and driver. The resident process loads the backend only when approved; a backend error mid-Dictation finishes that Dictation on CPU and disables the GPU for the session; Settings offers Auto, CPU only or a specific device and shows 'GPU disabled after an error' with re-test.

**Blocked by:** 11 First end-to-end dictation, 16 Model manager and first-run flow

**Status:** ready-for-agent

- [ ] Probe results are saved and reused per device and driver
- [ ] A hung or crashed probe never affects the resident process
- [ ] Mid-Dictation GPU errors fall back to CPU without losing words
- [ ] With the GPU unapproved, the backend library is never loaded
