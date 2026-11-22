# Listening indicator look and behaviour
Type: prototype
Status: resolved
Blocked by: 05

## Question
What should the minimal overlay look like (equalizer style, size, position options, animation, normal vs red grown fault state with text), prototyped cheaply so the user can react to it?

## Comments

- 2026-11-22: Prototype asset: [prototypes/10-listening-indicator-PROTOTYPE.html](../prototypes/10-listening-indicator-PROTOTYPE.html). Four variants (A Bar pill, B Orb, C Trace strip, D Caret buddy) on a fake desktop, with position/size/wallpaper/fault controls. Fault reason text is borrowed from research 04 as a placeholder; final wording belongs to [Listening fault rules](09-listening-fault-rules.md). Awaiting owner reaction.

## Answer

Decided by the owner after reacting to the [prototype](../prototypes/10-listening-indicator-PROTOTYPE.html).

- **Styles shipped in v1: A (Bar pill), B (Orb), C (Trace strip).** D (Caret buddy) is dropped, which also removes the fragile "find the text caret in other apps" dependency.
- **Default style: C (Trace strip).** The user picks the style in Settings (new Settings item: Listening indicator style).
- **Position: two independent selections in Settings.** Vertical: Top / Bottom. Horizontal: Left / Center / Right. Default: **Bottom + Center**. Replaces the old single "Listening indicator position" setting. Needs per-monitor DPI and taskbar-aware placement (see research 05).
- **Timing accepted:** about 120 ms fade/scale in, 180 ms out, about 220 ms to grow into the fault state.
- **Fault state (not explicitly contested, kept as prototyped):** turns red, grows, shows the reason (plus a short hint line when known) and a crossed-out mic where the style has room. Final reason wording and thresholds belong to [Listening fault rules](09-listening-fault-rules.md). Every style needs a fault layout that fits the reason text.
- **Behaviour carried over:** loudness drawn at 30 fps by a timer that runs only during Dictation; fast attack, slow release smoothing; click-through, no focus steal.
