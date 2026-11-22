"""Third-party notices generated from the Cargo dependency tree (`cargo metadata`)."""
import hashlib
import json
import subprocess
import sys
from pathlib import Path

LICENCE_FILE_PREFIXES = ("license", "licence", "copying", "unlicense", "notice")

MODEL_SECTION = """\
Speech models (not distributed)
-------------------------------
Aloud Type never ships a Speech model. The user downloads them on demand:
  - Whisper models converted to ggml (ggerganov/whisper.cpp on Hugging Face), derived from
    OpenAI Whisper weights, MIT licence, Copyright (c) 2022 OpenAI.
  - The Silero voice-activity model (ggml-org/whisper-vad on Hugging Face), derived from
    Silero VAD, MIT licence, Copyright (c) 2020-present Silero Team.
whisper.cpp / ggml (statically linked through whisper-rs): MIT licence,
Copyright (c) 2023-2026 The ggml authors.
"""


def shipped_packages(meta):
    """Non-workspace packages reachable from the workspace through normal or build dependencies."""
    by_id = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    seen, stack = set(), list(meta["workspace_members"])
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for dep in nodes.get(pid, {}).get("deps", []):
            if any(k.get("kind") in (None, "build") for k in dep.get("dep_kinds", [])):
                stack.append(dep["pkg"])
    members = set(meta["workspace_members"])
    out = [by_id[i] for i in seen if i not in members and by_id[i].get("source")]
    return sorted(out, key=lambda p: (p["name"], p["version"]))


def _licence_texts(pkg):
    root = Path(pkg["manifest_path"]).parent
    texts = []
    for f in sorted(root.iterdir()) if root.is_dir() else []:
        if f.is_file() and f.name.lower().startswith(LICENCE_FILE_PREFIXES):
            try:
                texts.append(f.read_text(encoding="utf-8", errors="replace").strip())
            except OSError:
                pass
    return texts


def generate(meta):
    groups = {}  # key -> (text or None, licence exprs, [pkgs])
    for p in shipped_packages(meta):
        texts = _licence_texts(p)
        body = "\n\n".join(texts) if texts else None
        key = (hashlib.sha256(body.encode()).hexdigest() if body else "none:" + (p.get("license") or "unknown"))
        entry = groups.setdefault(key, [body, set(), []])
        entry[1].add(p.get("license") or "unknown")
        entry[2].append(p)
    lines = ["Aloud Type third-party notices", "=" * 30, "",
             "Aloud Type is free software under the GNU AGPL v3 or later (see LICENSE).",
             "It includes the following third-party Rust crates, grouped by licence text.", "",
             MODEL_SECTION]
    for key in sorted(groups):
        body, exprs, pkgs = groups[key]
        lines += ["-" * 72, "Licence: " + "; ".join(sorted(exprs)), "Used by:"]
        for p in pkgs:
            repo = p.get("repository") or "no repository listed"
            lines.append(f"  {p['name']} {p['version']} ({repo})")
        lines.append("")
        lines.append(body if body else "(no licence file in the published crate; licence per SPDX expression above)")
        lines.append("")
    return "\n".join(lines) + "\n"


def cargo_metadata(root, target="x86_64-pc-windows-msvc"):
    out = subprocess.run(["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", target],
                         cwd=root, capture_output=True, text=True, encoding="utf-8", check=True).stdout
    return json.loads(out)


if __name__ == "__main__":
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path.cwd()
    dest = Path(sys.argv[2]) if len(sys.argv) > 2 else root / "THIRD-PARTY-NOTICES.txt"
    dest.write_text(generate(cargo_metadata(root)), encoding="utf-8", newline="\n")
    print(f"wrote {dest}")
