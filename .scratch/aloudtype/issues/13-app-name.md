# App name

Type: grilling
Status: resolved

## Question

What should the app be called? The earlier working name is only a placeholder and sits close to an existing commercial product's mark, which is a reason to move away from it. Pin down naming criteria (short, pronounceable, no confusion with existing dictation products, crate/binary/exe name friendly, tray tooltip friendly), generate candidates, and check availability (crates.io, GitHub, domain-ish, trademark-obvious conflicts). Decide the name; then the glossary, spec and tray icon brief use it.

## Answer

The app is named **Aloud Type**. Criteria: one or two plain words, ASCII, usable as an exe/crate name, reads as voice input, no clash with a known dictation product, no "whisper"/"wispr"/"flow". Four candidate batches were checked (see `research/13-name-availability.md`): the Sotto/Murmur/Hush/Quill families and SpeakAloud, SpeakType, SpeakKeys and Utter Type all collide with existing dictation products or read as text-to-speech. Aloud Type had no existing product and its joined/hyphenated crate and GitHub names were free (2026-11-22, weak evidence). Known risk: "aloud" can suggest text-to-speech, so the tagline and store text must say voice-to-text. Not a trademark clearance: run USPTO/EUIPO/WIPO, Microsoft Store, winget and domain checks before any public release (release checklist in the spec). Suggested identifiers: `aloud-type` (crate), `aloudtype.exe`. The earlier working name is retired.
