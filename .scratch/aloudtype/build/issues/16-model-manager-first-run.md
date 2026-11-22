# 16: Model manager and first-run flow

**What to build:** The Settings window lists the supported speech models with sizes and download status, downloads the selected model with progress, resumable partial files and checksum verification, shows the inline errors with Retry, and the resident process loads only verified files. First run: Settings opens automatically at every start until a model is installed, the base model is one click away, and pressing the Dictation key before setup brings Settings to the front.

**Blocked by:** 06 Config and state files, 15 Settings window and Dictation key rebinding

**Status:** ready-for-agent

- [ ] Interrupted downloads resume; a corrupted download is rejected after one automatic retry
- [ ] Unverified models are never loaded
- [ ] First-run behaviour matches the spec until a model is installed
- [ ] English-only models disable the language setting
