# Icons

Generated from [`docs/logo.svg`](../../docs/logo.svg) by `tools/icons` (`cd tools/icons && npm install && npm run build`). Do not edit the outputs by hand.

| File | Use |
| --- | --- |
| `app.ico` | Executable, installer, Start menu. 16 to 256 px; 16 to 40 px use the solid `svg/app-small.svg`, 48 px and up use the full logo `svg/app.svg` |
| `png/app-256.png`, `png/app-512.png` | Raster copies of the full logo |
| `tray/tray-<state>-<taskbar>.ico` | Tray icon, 16, 20, 24, 32 and 48 px per file |
| `svg/` | Vector sources for all of the above |
| `preview-tray.png` | All tray states at real and magnified size on both taskbar colours |

Tray states: `idle` (neutral bars), `listening` (taller coral bars), `fault` (dimmed bars with a red alert badge).
The tray never shows the recoverable "No sound detected", so there is no fourth state.

Taskbar variants: Windows does not recolour tray icons, so each state ships twice. `light` is for a light taskbar (dark glyph) and `dark` is for a dark taskbar (light glyph). Pick by the system theme value `SystemUsesLightTheme` under `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`, and re-pick when it changes.
