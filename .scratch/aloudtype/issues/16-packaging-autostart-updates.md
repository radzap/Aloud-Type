# Packaging, launch at login and updates

Type: grilling
Status: resolved
Blocked by: 14

## Question

How is Aloud Type packaged and installed (portable exe vs installer, MSIX vs MSI vs plain zip), how does launch at login work (registry Run key vs startup folder vs task), where do models and the Vulkan backend DLL live, how are the third-party notices and AGPL source offer shipped, and how are updates delivered given the app makes no network calls except the model download (manual only, or an opt-in check)? Include the code-signing question and the release checklist (trademark, Store, winget, domain checks).

## Answer

Decided with the owner (all recommendations accepted):
- **Distribution**: two forms from one build: a portable zip (exe, Vulkan backend DLL, licence, third-party notices) and a per-user Inno Setup installer (no admin rights, installs under `%LOCALAPPDATA%\Programs\AloudType`, Start menu entry, uninstaller, never touches Program Files). No MSIX in v1: package identity complicates the global hook, injection and file locations and v1 does not need it.
- **Launch at login**: a Settings toggle, off by default, writing `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (no admin, no scheduled task, switchable in Windows' Startup Apps page). The installer asks once whether to enable it; the portable zip never touches the registry unless the user flips the toggle.
- **Where files go**: exe and Vulkan DLL together in the install folder; user data (`config.toml`, `state.toml`, `models\`) stays in `%LOCALAPPDATA%\AloudType\`, so upgrades and reinstalls never touch models. The uninstaller removes the program and asks before deleting user data. The portable zip uses the same data location, so settings and models are shared between forms.
- **Updates**: manual only in v1, no update checks, preserving "no network except the model download". Settings shows the app version and a link to the releases page. An opt-in update check may come later.
- **Compliance files**: each release carries the AGPL-3.0 licence text and a `THIRD-PARTY-NOTICES` file (generated with `cargo-about`; `cargo-deny` in CI), plus a source-code link visible in Settings.
- **Signing and release checklist**: sign release builds when a certificate is available (unsigned apps trigger SmartScreen and can be microphone-blocked on managed Windows 11); until then ship unsigned with a README note on the SmartScreen prompt. Release checklist carried into the spec: trademark searches for "Aloud Type" (USPTO, EUIPO, WIPO), Microsoft Store name check, winget identifier check, domain check, `cargo-deny`/`cargo-about` runs, smoke test on a clean Windows 11 VM.

Note: this adds Settings items beyond the earlier closed list: Launch at login (already listed), app version + releases link, source link. These are informational, not new options.
