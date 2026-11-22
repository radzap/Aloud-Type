# 13 - Name availability (internal, not legal advice)

Date: 2026-11-22. Method: web search + crates.io API + GitHub API. No formal trademark clearance (USPTO/EUIPO/WIPO TESS searches NOT run; only incidental web hits). Status key: V = verified by a fetched/API result, U = unverified (search-snippet only or absent from search, not proven absent).

## Table

| Candidate | Same/similar-name dictation software | crates.io (joined/hyphenated) | GitHub user/org + repo names | TM signal (software) | Reads as voice input? | Wispr Flow closeness | Verdict |
|---|---|---|---|---|---|---|---|
| Sotto Dictate | HEAVY: at least 5 dictation apps named "Sotto", incl. Windows OSS ones (stofll/Sotto, smirk-dev/sotto "Wispr Flow alternative"), paid Mac app sotto.to ($49). "Dictate" suffix adds little. | sotto-dictate, sottodictate free (V). Bare `sotto` taken (Whisper STT for Linux) (V). | sotto-dictate / sottodictate users free (V); repo search 0 for "sotto dictate" (V). | Unknown; crowded mark, unverified (U). | Moderate: "sotto voce" = quietly; "Dictate" clarifies. | Not close in name, but the smirk-dev repo markets itself against Wispr Flow. | Avoid: crowded, direct Windows collision. |
| Murmur Dictate | HEAVY: many "Murmur" dictation apps incl. Windows (murmurvt.com, murmur-app.com, dikkadev/murmur archived), Mac, iOS. | murmur-dictate, murmurdictate free (V). Bare `murmur` taken, unrelated (V). | users free (V). | "MURMUR" marks exist in other classes (earphones dead, music abandoned); software unverified (U). | Good ("murmur" = speaking quietly). | Not close. | Avoid: crowded, same Windows niche. |
| Aloud Type | None found as dictation product (U, search only). "Aloud" mostly means read-aloud/TTS; a "Talk as I Type" iOS repo (jai/typed-aloud-support) is unrelated. | aloud-type, aloudtype free (V). | aloudtype user free (V); no repo hits. | Unknown (U). "Aloud" is used by Google Aloud (AI dubbing) and TTS products - possible class 9 marks (U). | Weak/ambiguous: "aloud" suggests reading TO you (TTS). | Not close. | Usable, but meaning confusion with TTS. |
| Say Quill | No exact product. BUT "Quill" is crowded: lsai-dev/quill-dictation (Windows, tagline "Say it. It's written."), heyquill.ai, Quill Flow (iOS), xfreeze2/quill, iniyan.pro/quill. | say-quill, sayquill free (V). | `sayquill` GitHub user EXISTS (V, 200); repo "say-quill" 0 hits (V). | Quill marks plausible in software (U). | Good (say + quill = write by speaking). | "Quill Flow" word "Flow" close to "Wispr Flow" ecosystem; Say Quill itself not, but sits next to a Windows Quill dictation app. | Avoid: sits on top of lsai-dev Windows "Quill". |
| Voice Quill | Same Quill crowding; "Voice Quills" is a YouTuber (unrelated). 8 GitHub repos match "voice-quill" (V count only; not inspected). | voice-quill, voicequill free (V). | `voicequill` user EXISTS (V); 8 repo name hits (V). | U. | Very clear. | Not close. | Avoid: taken handles + Quill crowding + generic. |
| Hush Dictate | HEAVY: "Hush" dictation apps: hush.software (Mac, OSS, Wispr alternative), ijon007/hush-voice (Windows, Whisper), khawkins98/Hush (Win/Mac/Linux), Android apps. | hush-dictate, hushdictate free (V). | users free (V). | U (Hush is a common mark). | Moderate (quiet). | Hush marketed as Wispr Flow alternative - same positioning. | Avoid: direct Windows collisions. |
| Quiet Typist | None found (U). | quiet-typist, quiettypist free (V). | quiettypist user free (V); no repo hits (V). | Unknown (U); descriptive-ish. | Weak: reads as keyboard/typing, not speech. | Not close. | Available but does not signal voice. |
| Speak Quill | None exact found (U); Quill crowding applies. "Speak-Quills" is a fantasy item (Wandering Inn); WordQ+SpeakQ (Quillsoft, assistive dictation) is a loosely similar mark (U). | speak-quill, speakquill free (V). | speakquill user free (V); no repo hits (V). | Quillsoft/SpeakQ similarity possible (U). | Good. | Not close. | Best of the Quill set but still near lsai-dev Quill. |

## Notes

- Free-handle results are weak evidence only: crates.io 404 and GitHub user 404 checked 2026-11-22 via API; names can be taken any time. Bare `sotto` and `murmur` crates are taken but unrelated to the two-word forms.
- Trademark: only incidental hits (MURMUR earphones dead; MURMUR Sony abandoned). Not a clearance. Before adopting, run USPTO Trademark Search, EUIPO TMview, WIPO Global Brand DB for classes 9 and 42, and check Microsoft Store name reservation and domain availability (not checked).
- Not checked: domains, Microsoft Store, winget IDs, npm/PyPI.
- Unfetched pages: results rely on search-engine summaries, not full page fetches, so "verified" for existence of competitors means a search listing, and "absent" means not surfaced.

## Sources

- Sotto: https://github.com/stofll/Sotto , https://github.com/smirk-dev/sotto/ , https://sotto.to/ , https://github.com/t5victor/Sotto
- Murmur: https://murmurvt.com/ , https://murmur-app.com/en , https://github.com/dikkadev/murmur , https://github.com/kurenn/murmur , https://murmur.you/
- Hush: https://github.com/ijon007/hush-voice , https://www.hush.software/ , https://github.com/khawkins98/Hush
- Quill: https://github.com/lsai-dev/quill-dictation , https://heyquill.ai/ , https://quillflow.app/ , https://www.iniyan.pro/quill/ , https://en.wikipedia.org/wiki/WordQ%2BSpeakQ
- Aloud context: https://www.yaps.ai/blog/text-to-speech-windows
- TM incidental: https://www.trademarkia.com/murmur-ear-protection-87317893 , https://uspto.report/TM/79154809
- crates.io API: https://crates.io/api/v1/crates/<name> ; GitHub API: https://api.github.com/users/<name>

## Ranking

1. Aloud Type (clean in dictation space, handles free; weakness: TTS ambiguity)
2. Speak Quill (clear meaning, handles free; weakness: Quill neighbours incl. a Windows app)
3. Quiet Typist (clean, handles free; weakness: does not say "voice")

## SpeakAloud

Not legal advice; no formal trademark clearance. Checked 2026-11-22. V = verified by direct fetch/API; S = search-engine summary only; U = unverified/not checked.

### Findings

1. Existing products using the same or near-identical name
   - V: GitHub repo Muhammad-Rajput-IT-Professional/Speakaloud is a Windows desktop app (Python) for text-to-speech (reads PDFs and drafts aloud; Edge neural voices + Kokoro-82M). 0 stars, 25 commits, no license shown, releases v1.0.2 to v1.0.5. Same name, same OS, opposite direction (TTS). https://github.com/Muhammad-Rajput-IT-Professional/Speakaloud
   - S: "Speak Aloud" Windows TTS freeware (text to speech, MP3 export, clipboard reader), listed on Softpedia, old (v2.0.2010.218). https://www.softpedia.com/get/Multimedia/Audio/Audio-Convertors/Speak-Aloud.shtml
   - S: "Speak Aloud" iOS and Android English pronunciation practice app (Aloud Idiomas). https://apps.apple.com/us/app/speak-aloud/id6502767415 , https://play.google.com/store/apps/details?id=com.flutterflow.speakaloudproficiency
   - S: pearl7721User1/SpeakAloud (iOS speaking-practice project, ~2019). https://github.com/pearl7721User1/SpeakAloud
   - S: SpeakLoud (speakloud.app) language-exchange platform; near-name. https://github.com/mohammadDV/speak-loud
   - S: Crowded neighbours: OutSpoken - Speak Aloud (kids), Voice Aloud Reader, TextAloud (NextUp), Out Loud (offline TTS), Speak (speak.com), Speakey (local Windows dictation, direct competitor with near-name), Speakeasy AI Dictation, SpeakPaste. https://apps.apple.com/us/app/outspoken-speak-aloud/id1672797742 , https://nextup.com/ , https://www.out-loud.io/ , https://alternativeto.net/software/speakey/?p=2
   - No dictation product named SpeakAloud surfaced (S).

2. Registry and handle collisions
   - V: crates.io speakaloud: 404 (free). crates.io speak-aloud: 404 (free). https://crates.io/api/v1/crates/speakaloud , https://crates.io/api/v1/crates/speak-aloud
   - V: GitHub user/org speakaloud: 404 (free). speak-aloud: 404 (free). https://api.github.com/users/speakaloud
   - V: Repo name "Speakaloud" already exists under another user (TTS Windows app above); a repo under your own org would not technically collide but would show in search next to it.
   - U: domains, Microsoft Store name, winget ID, npm/PyPI.

3. Trademark
   - S: no registration surfaced in web search; this is weak evidence only. U: USPTO TESS/TSDR, EUIPO, WIPO Global Brand DB (classes 9, 42, 41 for the pronunciation app) not queried. Existing commercial use of "Speak Aloud" in education/app stores and a same-OS TTS app creates common-law-style conflict risk.
   - "speak aloud" is a generic English phrase, so protectability is weak but so is distinctiveness; hard to defend and easy to collide with.

4. Confusion with text-to-speech / read aloud
   - High. "Aloud" and "read aloud" are strongly associated with TTS (Microsoft Edge/Word "Read Aloud", Speak, TextAloud, Out Loud). Search results for "speak aloud" Windows return TTS products first. Likely SEO collision and wrong-audience downloads.

5. Does it read as voice input to a new user?
   - Partly. "Speak" signals voice, and "aloud" implies out loud, which a user can read as "say it out loud and it types." But the same words equally read as the computer speaking aloud (TTS). Ambiguous; dictation is not stated.

### Verdict: AVOID (or at best strong caution)

Reasons: same-name Windows TTS app already exists on GitHub; "Speak Aloud" is used by app-store and Windows freeware titles; strong TTS/"read aloud" confusion and poor searchability; ambiguous as voice input. Free: crates.io and GitHub handles. If kept, use a distinguishing form (e.g. add "Type"/"Dictate") and run USPTO/EUIPO search. Prior ranking in this file (Aloud Type etc.) remains preferable.

Sources: as linked above.


## Batch 3: SpeakType, SpeakKeys, Utter Type

Checked 2026-11-22. Marks: V = verified by fetch/API call, S = web-search snippet only, U = unverified. Not legal advice; no formal trademark clearance.

| Check | SpeakType | SpeakKeys | Utter Type (UtterType) |
|---|---|---|---|
| Same-name software | V/S: many dictation apps named SpeakType: karansinghgit/speaktype (offline OSS, "any OS", mentions signed Windows app), wookat/speaktype (OSS local voice typing for Windows), alexctop/speaktype (push-to-talk Windows), subhojeet-chowdhury/SpeakType, Konoyo-014/SpeakType (macOS), asadeisa/SpeakType (SaaS), speaktype.ai (Mac, paid tiers), tryspeaktype.com (Mac), iOS "SpeakType: AI Voice Keyboard", Mac App Store "SpeakType". | S: SpeakKeys Android voice keyboard (speakkeys.com; GitHub nishantkumar1292/SpeakKeys, GPLv3, HeliBoard fork, cloud STT). Also unrelated: "SpeakKeys" Max for Live instrument (Max for Cats, Oct 2026), smartSpeakKeys NVDA add-on. | V/S: UtterType, a Python Whisper dictation tool (dhruvyad/uttertype, DamianB-BitFlipper/uttertype; hotkey, Windows supported). Neighbouring: "Utter" dictation apps (Windows tryutter.vercel.app, tagline "Utter types."; ductai199x/utter; Mac utter.shantirmizi.com; iOS Utter AI keyboard, Pro $59.99/yr). |
| crates.io (joined / hyphenated) | V: speaktype 404 free; speak-type 404 free | V: speakkeys 404 free; speak-keys 404 free | V: uttertype 404 free; utter-type 404 free |
| GitHub user/org | V: speaktype free; speak-type TAKEN (200) | V: speakkeys free; speak-keys free | V: uttertype free; utter-type free |
| GitHub repo-name collisions | V: 22 repos named like "speaktype" in API search; at least 6 are dictation apps, 3+ for Windows | V: 0 repos with "SpeakKeys" in name (search API); SpeakKeys / SpeakKeys-Next exist under nishantkumar1292 (name search may miss) | V: 2 repos "uttertype" (same tool, fork) |
| Trademark (software) | S: crowded common use across Mac/iOS/Windows dictation; likely strongest common-law conflict. U: USPTO/EUIPO not queried. | S: one live Android voice-keyboard product with own domain; moderate. U: USPTO/EUIPO not queried. | S: one small OSS tool plus "Utter" dictation brand family (iOS app paid, Windows/Mac). U: USPTO/EUIPO not queried (classes 9, 42). |
| Reads as voice input? | Yes, clear ("speak" + "type"). | Yes-ish, but "keys" suggests keyboard or shortcut keys; Max instrument and NVDA key-echo add-on add noise. | Moderate. "Utter" = say; "type" = typed output. Less obvious than Speak; not TTS-like. |
| TTS confusion | Low-moderate ("Speak" is a TTS verb, but "Type" shifts it to input) | Moderate ("Speak keys" can read as "announce the keys", as the NVDA add-on does) | Low |

Sources: https://github.com/karansinghgit/speaktype , https://github.com/wookat/speaktype , https://github.com/alexctop/speaktype , https://speaktype.ai/ , https://tryspeaktype.com/ , https://apps.apple.com/us/app/speaktype-ai-voice-keyboard/id6756912658 , https://www.speakkeys.com/ , https://github.com/nishantkumar1292/SpeakKeys-Next , https://github.com/crucio2211/smartSpeakKeys , https://www.matrixsynth.com/2026/10/speak-and-key-speak-by-max-for-cats.html , https://github.com/dhruvyad/uttertype , https://github.com/DamianB-BitFlipper/uttertype , https://tryutter.vercel.app/ , https://github.com/ductai199x/utter , https://apps.apple.com/app/id6753176013 , https://crates.io/api/v1/crates/speaktype (and the other five slugs), https://api.github.com/users/speak-type , https://api.github.com/search/repositories?q=SpeakType+in:name

Unverified for all three: domains, Microsoft Store name, winget ID, npm/PyPI, USPTO/EUIPO/WIPO, speakkeys.com page contents (only via search summary).

### Verdict
1. SpeakKeys: CAUTION (best of the three on clarity of registry space: no repos by that name besides the Android keyboard). One direct same-name voice-input product (Android, OSS), different platform; "keys" is slightly off for TTS/keyboard-shortcut readings.
2. Utter Type: CAUTION. Registries free, but UtterType is an existing OSS dictation tool (Python, Windows-capable), and the "Utter" dictation family (incl. a Windows app tagged "Utter types.") is very close.
3. SpeakType: AVOID. Clearest meaning but heavily taken: at least 6 dictation repos including two Windows ones, commercial Mac/iOS apps, GitHub handle speak-type taken, and search results would be swamped.
