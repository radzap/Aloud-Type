Aloud Type {version}
====================

Aloud Type turns your speech into text typed into whatever text field has focus.
It runs fully on your computer. After you download a Speech model once, it makes no
network calls. There are no accounts, no telemetry and no update checks; to update,
download a new release.

Run
---
Portable zip: unzip anywhere and run aloud-type.exe. The first start opens Settings,
where one click downloads the Speech model. The zip never touches the registry unless
you switch on "Launch at login" in Settings.
Installer: per-user, needs no administrator rights, adds a Start menu entry.

Your data
---------
Settings and Speech models are stored in %LOCALAPPDATA%\AloudType, separate from the
program files. The installer and the portable zip share that folder, so upgrading,
reinstalling or switching between the two keeps your settings and models. The
uninstaller asks before deleting it.

Unsigned releases
-----------------
Until a code-signing certificate is available, releases are unsigned. Windows
SmartScreen may warn on first run ("More info", then "Run anyway"), and some managed
Windows 11 machines block unsigned apps from the microphone.

Licence and source
------------------
Aloud Type is free software under the GNU Affero General Public License v3 or later
(see LICENSE). Third-party components are listed in THIRD-PARTY-NOTICES.txt.
Source code: https://github.com/radzap/Aloud-Type
