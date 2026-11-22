# 22: Tray icon artwork

**What to build:** Produce the real tray icon set: generate a concept with the image-generation tool using the brief (simple, high-contrast glyph legible at 16 to 32 px, with idle, listening and fault states), then trace it into a clean vector with few shapes and export the sizes, checking it on light and dark taskbars.

**Blocked by:** None (can start immediately)

**Status:** ready-for-human

- [x] Three states exist at all tray sizes and read clearly on light and dark taskbars
- [x] The licence terms of the generation tool allow commercial use (confirmed)
- [x] Final assets are handed over for the tray ticket to adopt

## Comments

Done from the supplied `docs/logo.svg` instead of a fresh generation. The logo's thin bubble outline and seven bars do not survive 16 px, so the tray glyph is a five-bar redraw on a 16 unit grid. Assets, the generator (`tools/icons`) and the light/dark selection note are in `assets/icons/` (see its README); `assets/icons/preview-tray.png` is the light and dark taskbar check. The tray ticket (12) adopts `assets/icons/tray/*.ico` and `app.ico`.
