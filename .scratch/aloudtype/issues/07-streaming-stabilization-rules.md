# Streaming insertion stability and latency rules
Type: grilling
Status: resolved
Blocked by: 01, 03

## Question
Given the runtime findings, what exactly counts as stable text to type, what latency budget between speech and typed text do we accept, and how are corrections, trailing partial words, punctuation and capitalisation handled in Stable typing mode? Decide whether Live-correct typing mode ships in v1.

## Answer

Decided with the owner:
- **Commit policy**: confirmed-prefix streaming. While Dictation runs, the growing utterance is re-decoded as fast as the machine allows and only the leading words that two consecutive decodes agree on are typed; the rest stays pending. If the machine cannot keep up, fall back automatically to typing the whole utterance when the speaker pauses.
- **Latency target**: words appear about 1-2 s behind speech on a reasonable machine; on ending the Dictation all remaining words are typed within about 1 s. Slower machines may lag more rather than lose accuracy. Ticket on performance budgets may tighten the numbers once benchmarks exist.
- **Trailing partial word**: never typed until a later decode confirms it or the Dictation ends (end flushes everything).
- **Punctuation, capitals, spacing**: model text as-is. Only spacing is managed: no leading space on the first chunk, one space before each later chunk, nothing added at the end. Trailing-space/Enter options are out of v1.
- **Live-correct typing mode: not in v1.** Stable is the only mode and the Typing mode setting is removed. The spec may note it as a possible later opt-in experiment. This reverses the earlier "configurable typing mode" decision.
- **Silence and hallucinations**: VAD gates audio so non-speech never reaches the decoder; segments with high no-speech probability or matching a short blocklist of known hallucinations are dropped. Thresholds are tuned in implementation.
- **Focus changes**: text goes to whichever window has focus at insertion time, like a keyboard; real keystrokes the user types do not cancel Dictation. Exception: an elevated window gets the notice instead of typing.

Sources: research/01-whisper-runtime-streaming.md, research/03-windows-input-capture-injection.md.
