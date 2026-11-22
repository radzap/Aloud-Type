# License compatibility for the app

Type: research
Status: resolved

## Question

Which open-source licenses can the app itself be released under (in particular AGPL-3.0, GPL-3.0, MIT/Apache-2.0)? Check every planned dependency and asset against it: whisper.cpp, `whisper-rs`, the OpenAI Whisper model weights and the ggml conversions hosted for download, the Silero VAD model, cpal, tray-icon/muda, iced, windows-sys, and any fallback engine (sherpa-onnx). Does anything (code licence, weights licence, download-on-demand model delivery, generated tray icon art) prevent AGPL, or force attribution/notice obligations? What does AGPL's network clause mean for a fully local app, and what are the practical pros and cons of AGPL vs permissive for this project? Note this is research, not legal advice.

## Answer

Nothing planned blocks AGPL-3.0, GPL-3.0, MIT or Apache-2.0. Every code dependency (whisper.cpp MIT, whisper-rs Unlicense, cpal Apache-2.0, tray-icon/muda/windows-sys MIT OR Apache-2.0, iced MIT, sherpa-onnx Apache-2.0) is GPLv3-compatible, and the Whisper, ggml-converted and Silero VAD weights are labelled MIT and are downloaded on demand, so they are never part of our distribution. The only obligation is shipping a THIRD-PARTY-NOTICES file with licence texts. AGPL section 13 does not trigger for a fully local app; only the normal source-on-distribution duty applies, so GPL-3.0-or-later is the simpler fit unless a server component may appear. Tray art from OpenArt needs a Plus-or-higher tier for commercial use, and a hand-drawn icon avoids the question. Not legal advice. Transitive crate licences (iced's tree) and sherpa-onnx model licences are unaudited; use `cargo-deny`/`cargo-about`. Findings: [research/12-license-compatibility.md](../research/12-license-compatibility.md)
