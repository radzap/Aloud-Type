# Speech model lineup and multilingual behaviour
Type: research
Status: resolved

## Question
Which Whisper model variants (tiny/base/small/medium, English-only vs multilingual, quantized) should Settings offer, with sizes, download sources, licenses, accuracy/speed trade-offs, and how fixed-language vs Auto detection behaves, including English mixed with a second language?

## Answer
Offer a short quantised list of Whisper ggml models (MIT, from Hugging Face `ggerganov/whisper.cpp`): `base.en` q5_1 (~60 MB, fast English), `small` q5_1 (~190 MB, balanced multilingual default; `small.en` for English-only accuracy), and `large-v3-turbo` q5_0/q8_0 (~574/874 MB, accurate, best with GPU). Skip medium and full large. `.en` models beat multilingual at tiny/base, nearly tie at small+, but are English-only, so selecting one disables the language setting. Fixed language (English default) conditions the decoder; Auto detects one language per utterance (30 s windows, weaker on short clips). Mid-sentence English plus second-language switching is undocumented and unsupported, so no guarantee (matches map scope). Gaps: real CPU latency per model and q5/q8 accuracy loss need a benchmark spike. Full findings and sources: `../research/06-models-and-languages.md`.
