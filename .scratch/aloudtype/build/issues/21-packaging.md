# 21: Packaging

**What to build:** A portable zip and a per-user Inno Setup installer (no administrator rights, Start menu entry, uninstaller that asks before deleting user data, optional launch at login), program files separate from user data, the AGPL licence and a generated third-party notices file in every release, and a release build step that signs when a certificate is available.

**Blocked by:** 17 Remaining Settings items

**Status:** ready-for-agent

- [ ] Installer and zip produce a working app on a clean Windows 11 machine
- [ ] Upgrades and reinstalls keep settings and models
- [ ] The notices file is generated from the dependency tree
- [ ] The app makes no network calls other than model downloads
