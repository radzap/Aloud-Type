# 12 - License compatibility for the app

Researched 2026-11-22. This is research, not legal advice. Tags: **[Doc]** stated by the primary source; **[Inference]** my reading or reasoning; **[Unverified]** not checked.

## Conclusion

- Nothing planned prevents releasing AloudType under **AGPL-3.0, GPL-3.0, MIT or Apache-2.0**. Every planned code dependency is MIT, Apache-2.0, MIT OR Apache-2.0, or Unlicense. All of these are GPLv3-compatible per the FSF. [Doc for each license and the FSF compatibility statements; Inference for the "nothing prevents" roll-up]
- Model weights (Whisper, ggml conversions, Silero VAD) are labelled MIT. They are downloaded by the user at run time and are never part of our source or binary, so they add no copyleft question. They add at most a notice courtesy. [Doc for the labels; Inference for the consequence]
- The only real obligations from permissive dependencies are **notice and licence-text preservation** when we ship a binary: a THIRD-PARTY-NOTICES file bundled in the installer or reachable from the app. [Doc, MIT and Apache clauses]
- AGPL's network clause (section 13) is not triggered by a fully local app. The obligations that do apply are the ordinary copyleft ones on distribution. [Doc for the clause text; Inference for "not triggered"]
- Recommendation [Inference]: **GPL-3.0-or-later or AGPL-3.0-or-later are both viable. For a fully local desktop app I'd pick GPL-3.0-or-later**, since AGPL adds nothing for this app. Choose AGPL only if a hosted or server component might appear later. MIT/Apache-2.0 is also fully viable if you want maximum adoption and don't mind proprietary forks. The decision is a project-values choice, not a technical constraint.

## 1. Dependency and asset table

| Component | License | Source | Blocks AGPL/GPL? | Obligation when we ship |
|---|---|---|---|---|
| whisper.cpp (incl. ggml, vendored) | MIT, "Copyright (c) 2023-2026 The ggml authors" [Doc] | [LICENSE](https://raw.githubusercontent.com/ggml-org/whisper.cpp/master/LICENSE) | No | Include copyright + permission notice "in all copies or substantial portions" (statically linked into our exe, so the exe distribution needs the notice) |
| `whisper-rs` 0.16.0 | Unlicense (public-domain dedication) [Doc] | [crates.io](https://crates.io/api/v1/crates/whisper-rs), repo https://codeberg.org/tazz4843/whisper-rs | No. FSF: "public domain works and the lax license provided by the Unlicense are compatible with the GNU GPL" [Doc] | None required |
| OpenAI Whisper weights (original) | MIT: "Whisper's code and model weights are released under the MIT License" [Doc]; LICENSE "Copyright (c) 2022 OpenAI" [Doc] | [README](https://raw.githubusercontent.com/openai/whisper/main/README.md), [LICENSE](https://raw.githubusercontent.com/openai/whisper/main/LICENSE) | No | Notice if we redistribute. We don't (download on demand) |
| Hugging Face card `openai/whisper-small` | Metadata says **apache-2.0** [Doc] | [HF](https://huggingface.co/openai/whisper-small) | No | Discrepancy with GitHub (MIT). Both are permissive; note in notices. Cite the GitHub MIT statement as authoritative since it is the original release [Inference] |
| ggml conversions (`ggerganov/whisper.cpp` on HF; the files we download, e.g. small-q5_1 ~181 MiB, base.en-q5_1 ~57 MiB) | Card metadata `license: mit`; no separate licence text beyond that [Doc] | [HF](https://huggingface.co/ggerganov/whisper.cpp); download URL pattern `https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-<model>.bin` from [download-ggml-model.sh](https://raw.githubusercontent.com/ggml-org/whisper.cpp/master/models/download-ggml-model.sh) | No | None for us while we only point to them. The conversion is a derivative of OpenAI's MIT weights, so MIT is consistent [Inference]. The HF card does not itself reproduce the OpenAI copyright line [Doc]; if we ever mirror or bundle, include both notices |
| Silero VAD (upstream) | MIT, "Copyright (c) 2020-present Silero Team" [Doc] | [LICENSE](https://raw.githubusercontent.com/snakers4/silero-vad/master/LICENSE) | No | Notice if redistributed |
| Silero VAD ggml conversion (`ggml-org/whisper-vad`: `ggml-silero-v5.1.2.bin`, `ggml-silero-v6.2.0.bin`) | Card metadata `license: mit`; card says models "converted to ggml format"; does not name upstream [Doc] | [HF](https://huggingface.co/ggml-org/whisper-vad), [download-vad-model.sh](https://raw.githubusercontent.com/ggml-org/whisper.cpp/master/models/download-vad-model.sh) | No | Same as above. Note: card does not credit Silero; the Silero copyright line should be in our notices [Inference] |
| `cpal` 0.18.2 | Apache-2.0 [Doc] | [crates.io](https://crates.io/api/v1/crates/cpal), [LICENSE](https://raw.githubusercontent.com/RustAudio/cpal/master/LICENSE) | No. FSF: Apache-2.0 compatible with GPLv3 [Doc] | Give recipients the licence text; keep any NOTICE content (the repo shows no NOTICE file in what I read [Unverified for transitive crates]) |
| `tray-icon` 0.21.2 | Apache-2.0 OR MIT [Doc] | [crates.io](https://crates.io/api/v1/crates/tray-icon) | No | Pick MIT (or Apache) and carry its notice |
| `muda` 0.26.1 | MIT OR Apache-2.0 [Doc] | [crates.io](https://crates.io/api/v1/crates/muda) | No | Same |
| `iced` 0.14.0 | MIT [Doc] | [crates.io](https://crates.io/api/v1/crates/iced) | No | MIT notice. Iced's own repo lists no bundled fonts at top level and its DEPENDENCIES.md covers system deps only, not licences [Doc]; transitive crates (wgpu, tiny-skia, cosmic-text, etc.) are **[Unverified]**: audit with `cargo deny` / `cargo about` |
| `windows-sys` 0.61.2 | MIT OR Apache-2.0 [Doc] | [crates.io](https://crates.io/api/v1/crates/windows-sys) | No | Notice |
| sherpa-onnx (fallback) 2.0.2 | Apache-2.0 [Doc] | [crates.io](https://crates.io/api/v1/crates/sherpa-onnx); also repo licence API cited in research 01 | No code conflict. Its **models** carry their own licences (varied, per model) **[Unverified]**; check per model before offering any in Settings | Apache-2.0 notice. It would also pull in ONNX Runtime (MIT, **[Unverified]**) |
| Vulkan loader/driver, GPU driver | System components on the user's machine | not shipped by us | Not applicable [Inference] | We don't redistribute them. whisper.cpp's Vulkan build uses Vulkan headers/shader tooling at build time only; Vulkan headers are Apache-2.0 **[Unverified here]** |
| Tray icon art (OpenArt generation at implementation) | OpenArt "makes no claims of ownership or copyright of AI-generated Output"; **commercial use needs Plus tier or above**; free-plan output may carry a watermark that must not be removed [Doc] | [OpenArt terms 4.1, 4.5](https://openart.ai/terms) | See below | See below |

Reading note [Inference]: the ticket's list is satisfied; I did not enumerate the full transitive crate graph.

## 2. Compatibility reasoning

- **Permissive into copyleft is one-way.** The FSF states Apache-2.0 is "compatible with version 3 of the GNU GPL" but not GPLv2; MIT/Expat, BSD-3 and the Unlicense are "compatible with the GNU GPL" ([FSF license list](https://www.gnu.org/licenses/license-list.html)). The ASF says the same direction: Apache code can go into GPLv3 projects, but "GPLv3 software cannot be included in Apache projects" ([ASF page](https://www.apache.org/licenses/GPL-compatibility.html)). [Doc]
- **AGPL and GPL together.** AGPLv3 section 13 expressly lets you link or combine with GPLv3 work; the AGPL terms then apply to the AGPL part and the GPL terms to the rest ([agpl-3.0.txt](https://www.gnu.org/licenses/agpl-3.0.txt)). The FSF list (as I read it) does not address AGPL-specific compatibility for these licenses; that AGPL behaves like GPLv3 for permissive inbound code is **[Inference]** from section 13 plus GPLv3 compatibility.
- **Implication for choice.** If the app is AGPL or GPL, all the above inbound licenses work. If the app is MIT/Apache, also fine (they're all permissive). Only a GPLv2-only or non-commercial/field-of-use dependency would break this, and none is planned. [Inference]
- **If a model or crate later turns out to be non-commercial or restrictive**, it would not be a clean fit for an open licence. Check each added model's card; the Whisper-family ones here are not restricted.

## 3. Model weights, download-on-demand, and notices

- The app does not bundle weights; the user's machine fetches them from Hugging Face. [Doc, map decision and download scripts] So we do not "convey" the weights ourselves. [Inference] Even if conveyed, MIT only requires the notice.
- Practical step: Settings download UI or the notices file should name the model's origin and licence (MIT, OpenAI; Silero Team) as courtesy and for traceability. [Inference]
- OpenAI's model card has no licence text and no usage restriction beyond cautions: it advises against transcribing people "taken without their consent" and against high-risk or subjective-classification use ([model-card.md](https://raw.githubusercontent.com/openai/whisper/main/model-card.md)). These are cautions, not licence terms. [Doc] The card says the training data was "collected from the internet" and doesn't list sources; provenance of training data is outside the licence question and I did not research it. [Doc / out of scope]
- Hosting risk: Hugging Face URLs could move or the repo owner change terms. Pin by file revision/hash in the downloader (hash values are visible on the HF files page, e.g. small-q5_1 `6fe57ddc...`, base.en-q5_1 `d26d7ce5...` as of this reading) and verify after download. [Doc for hashes shown, Inference for the recommendation]

## 4. AGPL network clause for a local app

- Section 13: it applies "if you modify the Program" and the modified version "supports interaction with users remotely through a computer network"; then those users must be offered the Corresponding Source from a network server. [Doc, summarised; full text at the link above]
- AloudType is on-device only, with no listening server and no cloud. A user talking into the mic is a local user, not a remote one. So section 13 has nothing to attach to. [Inference]
- AGPL does still impose the normal copyleft duties on **distribution** (section 6): when we hand out the exe we must make Corresponding Source available (bundled, a three-year written offer, or a network location). A public repository link per release satisfies the practical need. [Doc for section 6 options; Inference for the repo-link approach]
- If the app ever gained a local HTTP API or remote control, section 13 could start to matter for modified versions. [Inference]
- Side effect: some companies ban AGPL outright, which may limit contributions or enterprise adoption even though the app's behaviour is identical. [Inference, common industry view; not sourced]

## 5. AGPL vs GPL vs permissive for this project

| | AGPL-3.0 | GPL-3.0 | MIT / Apache-2.0 |
|---|---|---|---|
| Inbound deps | All planned OK | All planned OK | All planned OK |
| Network clause relevance | None for local app | n/a | n/a |
| Forks | Must stay open; also covers any hosted service built from the code | Must stay open when distributed; a SaaS fork could keep modifications private | Can be closed and sold |
| Corporate adoption | Weakest | Moderate | Strongest |
| Patent grant | Yes (GPLv3-family) | Yes | Apache: yes; MIT: none stated |
| Our burden | Source offer on distribution; notices | Same | Notices only |
| Relicensing later | Needs all contributors' consent unless you hold copyright (CLA/DCO choice matters) | Same | Same |

[Doc for the licence mechanics; the pros/cons columns are Inference.]

Pros of copyleft: protects against a closed commercial repackaging of a free tool, which matters because the project goal is "free, fully local". Cons: scares off some contributors/packagers, and no Microsoft Store or enterprise redistribution barrier exists in the licence itself, but corporate legal teams often add one in practice. Permissive pros: easiest adoption and zero compatibility friction. Cons: a company can ship a closed, paid fork. If you want the option to relicense (for example to dual-license), keep copyright with the project via a CLA, or accept contributions only under the chosen licence with DCO sign-off. [Inference]

## 6. Tray icon art

- OpenArt claims no ownership of Output. Commercial use requires Plus tier or higher, and free-plan watermarks cannot be removed. [Doc, [terms](https://openart.ai/terms)] Since the app is free and open, commercial status is arguable, but a release that others may redistribute commercially is safer with a paid-tier generation or a non-AI-sourced icon. [Inference]
- Open-licence compatibility: for AGPL/GPL the art can be a separately licensed asset (for example CC0 or CC-BY) in a `assets/` folder; the code licence need not cover it. [Inference]
- Copyrightability of purely AI-generated images is uncertain: the US Copyright Office's Part 2 report (January 2025) is the primary source; I could only confirm it exists ([copyright.gov/ai](https://www.copyright.gov/ai/)), not its conclusions, **[Unverified here]**. Practical effect: you may not be able to enforce copyright over the icon, which doesn't block release. [Inference]
- Hand-drawn or procedurally drawn icon states (the UI stack ticket already draws the overlay in Win32) avoid all of this. [Inference]

## 7. Practical to-do list for implementation tickets [Inference]

1. Pick the project licence; add `LICENSE` and SPDX headers (`GPL-3.0-or-later` or `AGPL-3.0-or-later`; "or later" gives future flexibility).
2. Add `cargo-deny` (licence allowlist: MIT, Apache-2.0, Unlicense, BSD-3, ISC, Zlib, Unicode, MPL-2.0 if reviewed) and `cargo-about` to generate THIRD-PARTY-NOTICES for the release.
3. Hand-write notices for non-crate items: whisper.cpp/ggml (MIT), Silero VAD (MIT), OpenAI Whisper (MIT).
4. Record the vendored whisper.cpp commit (already in ticket 01) so notices match what ships.
5. Pin model download URLs by revision and hash.
6. Vet any sherpa-onnx model licences individually before listing them.
7. Decide inbound-contribution policy (DCO or CLA).

## Sources

- whisper.cpp LICENSE: https://raw.githubusercontent.com/ggml-org/whisper.cpp/master/LICENSE
- whisper.cpp README: https://raw.githubusercontent.com/ggml-org/whisper.cpp/master/README.md
- download scripts: https://raw.githubusercontent.com/ggml-org/whisper.cpp/master/models/download-ggml-model.sh ; https://raw.githubusercontent.com/ggml-org/whisper.cpp/master/models/download-vad-model.sh
- OpenAI Whisper README/LICENSE/model card: https://raw.githubusercontent.com/openai/whisper/main/README.md , .../LICENSE , .../model-card.md
- HF cards: https://huggingface.co/ggerganov/whisper.cpp ; https://huggingface.co/openai/whisper-small ; https://huggingface.co/ggml-org/whisper-vad
- Silero LICENSE: https://raw.githubusercontent.com/snakers4/silero-vad/master/LICENSE
- crates.io API: whisper-rs, cpal, tray-icon, muda, iced, windows-sys, sherpa-onnx (https://crates.io/api/v1/crates/<name>)
- cpal LICENSE: https://raw.githubusercontent.com/RustAudio/cpal/master/LICENSE
- FSF license list: https://www.gnu.org/licenses/license-list.html
- AGPL-3.0 text: https://www.gnu.org/licenses/agpl-3.0.txt
- ASF GPL compatibility: https://www.apache.org/licenses/GPL-compatibility.html
- OpenArt terms: https://openart.ai/terms
- US Copyright Office AI page: https://www.copyright.gov/ai/
