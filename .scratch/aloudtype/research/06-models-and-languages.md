# 06 Speech model lineup and multilingual behaviour

Researched 2026-11-22. Scope: Windows, Rust, CPU baseline with optional GPU, fully offline after the one-time download. Assumes the whisper.cpp runtime (ggml model files); other runtimes belong to the stack ticket.

## 1. What the models are (primary: OpenAI Whisper)

- Sizes and variants (OpenAI README table): tiny 39M, base 74M, small 244M, medium 769M, large 1550M, turbo 809M. `.en` variants exist only for tiny/base/small/medium; large and turbo are multilingual only. Source: https://github.com/openai/whisper (README, "Available models and languages")
- Licence: code and weights are MIT (same README; turbo card https://huggingface.co/openai/whisper-large-v3-turbo says MIT). whisper.cpp is MIT too: https://github.com/ggml-org/whisper.cpp
- English-only quality: README states the ".en models ... tend to perform better, especially for the tiny.en and base.en models ... the difference becomes less significant for the small.en and medium.en models."
- Turbo is a pruned large-v3 with decoder layers cut from 32 to 4, "way faster, at the expense of a minor quality degradation"; it is not trained for translation (irrelevant to dictation). Sources: https://huggingface.co/openai/whisper-large-v3-turbo and the README above.
- Training mix (model card): about 65% English audio, 18% non-English audio with English transcripts, 17% non-English audio with matching transcripts, covering 98 non-English languages; accuracy in a language tracks its training data. Source: https://github.com/openai/whisper/blob/main/model-card.md
- Known failure modes: hallucinated text not in the audio, and repetition loops (model card). Implication: gate with VAD/silence detection so silence is never sent to the model.

## 2. Download sources and sizes (primary: ggml files on Hugging Face)

Hosted at https://huggingface.co/ggerganov/whisper.cpp/tree/main as `ggml-<name>.bin`. Sizes as listed there:

| Model | f16 (default) | q8_0 | q5 |
|---|---|---|---|
| tiny / tiny.en | 77.7 MB | 43.5 MB | 32.2 MB (q5_1) |
| base / base.en | 148 MB | 81.8 MB | 59.7 MB (q5_1) |
| small / small.en | 488 MB | 264 MB | 190 MB (q5_1) |
| medium / medium.en | 1.53 GB | 823 MB | 539 MB (q5_0) |
| large-v3 | 3.1 GB | not listed | 1.08 GB (q5_0) |
| large-v3-turbo | 1.62 GB | 874 MB | 574 MB (q5_0) |

Runtime memory (whisper.cpp README, f16): tiny ~273 MB, base ~388 MB, small ~852 MB, medium ~2.1 GB, large ~3.9 GB. Quantised models need less memory and disk and "depending on the hardware can be processed more efficiently"; the README gives no accuracy figures for quantisation. Source: https://github.com/ggml-org/whisper.cpp (README, Memory and Quantization). The accuracy cost of q5/q8 is therefore NOT established by a primary source; verify with our own test clips before fixing defaults.

Relative speed (OpenAI README, A100, English): tiny ~10x, base ~7x, small ~4x, medium ~2x, large 1x, turbo ~8x. Real CPU speed on user hardware was not measured; needs a benchmark spike.

GPU: whisper.cpp supports Vulkan (cross-vendor, `-DGGML_VULKAN=1`) and CUDA (`-DGGML_CUDA=1`) on Windows, plus CPU. Source: whisper.cpp README. The CLI exposes `-ng` (no GPU) and `-dev N` (device id), and flash attention is on by default: https://github.com/ggml-org/whisper.cpp/blob/master/examples/cli/README.md

## 3. Language behaviour

- CLI: `-l` defaults to `en`; `-l auto` auto-detects; `-dl` detects and exits (CLI README above).
- API: `language` in whisper_full_params: "for auto-detection, set to nullptr, "" or "auto""; `whisper_lang_auto_detect(ctx, offset_ms, n_threads, lang_probs)` detects from mel data at an offset; `no_context` and `single_segment` exist (the latter noted as useful for streaming). Source: https://github.com/ggml-org/whisper.cpp/blob/master/include/whisper.h
- Whisper works on 30-second windows and picks one language per window (paper: https://arxiv.org/abs/2212.04356 , full text https://ar5iv.labs.arxiv.org/html/2212.04356). Training pairs whose spoken and transcript languages disagree are excluded from ASR training; mixed-language utterances are not addressed by the paper.
- Language ID is imperfect: the paper reports zero-shot Fleurs language ID of 64.5, and 80.3% for the best model on the 82 overlapping languages. Short dictation clips (a few seconds) give less evidence than 30 s, so Auto should be less reliable on short utterances (inference). Community report that detection can latch onto accent: https://github.com/openai/whisper/discussions/2385 (secondary, corroborating only).
- Code-switching: not documented as supported by OpenAI or whisper.cpp. A whisper.cpp issue shows French-then-German audio output as French translation even with language forced to German; no maintainer answer, closed as not planned: https://github.com/ggml-org/whisper.cpp/issues/1800 (anecdotal). Community view that Whisper assumes one language per window: https://github.com/openai/whisper/discussions/49 (secondary).
- Practical implication for English plus a second language (inference from the above, not a documented guarantee): with a fixed language the decoder is conditioned on that language, so foreign words may be transliterated and a wholly foreign utterance may be mangled. With Auto, one language is chosen per utterance, so a mostly-English sentence with a few foreign words stays English, a mostly-foreign one goes foreign, and a true mid-sentence switch is not guaranteed. This matches the map's "no per-sentence mixed-language guarantees" scope line.
- `.en` models cannot transcribe other languages, so choosing one forces English and the language setting should be hidden or disabled.

## 4. Recommended Settings lineup (decision input)

Offer a short list, defaulting to a quantised multilingual model so one download serves every language option.

1. Fast, English only: `base.en` q5_1 (59.7 MB); `tiny.en` q5_1 (32.2 MB) only for very weak CPUs. The `.en` gain is largest at tiny/base.
2. Balanced default, multilingual: `small` q5_1 (190 MB). `small.en` q5_1 (190 MB) for English-only users wanting accuracy.
3. Accurate, multilingual: `large-v3-turbo` q5_0 (574 MB) or q8_0 (874 MB). Likely too slow on CPU alone because the encoder is still large; suggest when a GPU is detected or the user opts in.
4. Omit `medium` (q5_0 539 MB is dominated by turbo q5_0 at similar size), full `large-v3`, and the f16 files.

Language setting: English default; fixed-language list limited to what the chosen model supports; Auto offered only for multilingual models; disabled for `.en` models.

## 5. Open items and gaps

- Measured CPU latency per model on typical Windows laptops (needs a spike); the README speed table is A100, English only.
- Accuracy cost of q5 vs q8 vs f16 (no primary source found).
- Whether Auto re-detects per Dictation (suggested, since each Dictation is one utterance); cost not measured.
- Alternative runtimes or non-Whisper/distilled models were not evaluated here.
