# 21: Packaging

**What to build:** A portable zip and a per-user Inno Setup installer (no administrator rights, Start menu entry, uninstaller that asks before deleting user data, optional launch at login), program files separate from user data, the AGPL licence and a generated third-party notices file in every release, and a release build step that signs when a certificate is available.

**Blocked by:** 17 Remaining Settings items

**Status:** resolved

- [ ] Installer and zip produce a working app on a clean Windows 11 machine
- [ ] Upgrades and reinstalls keep settings and models
- [ ] The notices file is generated from the dependency tree
- [ ] The app makes no network calls other than model downloads

## Answer

Done (TDD in Python, stdlib only; 31 tests: `python -I -m unittest discover -s scripts/tests`, also a `packaging` CI job). New files only: `scripts/` (`aloud_release/{notices,package,audit}.py`, `release.py`, `tests/`), `packaging/aloud-type.iss` + `packaging/README.txt`, `LICENSE` (AGPL-3.0 text), CI job, `.gitignore` (`dist/`).
- Notices: `THIRD-PARTY-NOTICES.txt` generated from `cargo metadata` (normal+build deps for the Windows target, no dev deps), licence texts read from the crate sources and grouped by identical text, plus a model/whisper.cpp/Silero courtesy section. Tested on synthetic trees and on the real tree (every third-party crate appears). Deviation: not `cargo-about` (not installed); 4 crate groups have no licence file in the published crate and are listed with their SPDX expression.
- Zip: `AloudType-<ver>/` with exe, LICENSE, notices, README (+ any `ggml-*.dll` the build emits, for ticket 19); no models or settings. `scripts/release.py` ran for real: audit, release build, stage, zip. Signing: `signtool` with timestamp when `ALOUDTYPE_SIGN_PFX` (+ `ALOUDTYPE_SIGN_PASSWORD`) is set, else unsigned (README note).
- Installer script: per-user (`PrivilegesRequired=lowest`, `{autopf}`), fixed AppId (upgrade in place), Start menu entry, unchecked "launch at login" task writing the same quoted HKCU Run value the app's toggle uses, licence page, closes a running app on upgrade, uninstaller asks (default No, never when silent) before deleting `%LOCALAPPDATA%\AloudType`. Data is never installed or touched by file sections, so upgrades keep settings and models. Script is checked by text tests only.
- No-network audit: source scan (sockets, HTTP clients only in `settings-ui/src/download.rs`, URL allow list = model host + GitHub links, no update endpoints) and a dependency check that `aloud-type-app` links no HTTP/TLS crate. Re-run if the host starts linking `settings-ui`.

Gaps: Inno Setup is not installed here, so the installer was never compiled or run; no signing certificate, signing untested on a real cert; nothing run on a clean Windows 11 machine (criterion 1 unverified, also the exe currently lacks tray/Settings wiring from other tickets); upgrade/reinstall data retention is by design (separate folders) not exercised; the portable zip's Run-key path is stale if moved (ticket 17 gap). Installer Run-key value matches the app's, but the installer's uninstall deletes it only if the task created it in that install.
