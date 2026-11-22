"""Release version: the next version after the newest `vX.Y.Z` tag, or the workspace version if it is higher.

  python scripts/aloud_release/version.py <repo root>   # prints the next version and writes it into Cargo.toml

Bumping major or minor is done by hand in Cargo.toml (`[workspace.package] version`); every release
otherwise gets the next patch number automatically.
"""
import re
import subprocess
import sys
from pathlib import Path

TAG_RE = re.compile(r"^v(\d+)\.(\d+)\.(\d+)$")
VERSION_LINE_RE = re.compile(r'^(version\s*=\s*")(\d+\.\d+\.\d+)(")', re.M)


def parse(v):
    return tuple(int(x) for x in v.split("."))


def next_version(workspace_version, tags):
    released = [tuple(map(int, m.groups())) for m in map(TAG_RE.match, tags) if m]
    own = parse(workspace_version)
    if not released:
        return workspace_version
    latest = max(released)
    if own > latest:
        return workspace_version
    return f"{latest[0]}.{latest[1]}.{latest[2] + 1}"


def workspace_version(cargo_toml_text):
    return VERSION_LINE_RE.search(cargo_toml_text).group(2)


def set_workspace_version(cargo_toml_text, version):
    return VERSION_LINE_RE.sub(lambda m: m.group(1) + version + m.group(3), cargo_toml_text, count=1)


if __name__ == "__main__":
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path.cwd()
    manifest = root / "Cargo.toml"
    text = manifest.read_text(encoding="utf-8")
    tags = subprocess.run(["git", "tag", "--list", "v*"], cwd=root, capture_output=True, text=True,
                          check=True).stdout.split()
    version = next_version(workspace_version(text), tags)
    manifest.write_text(set_workspace_version(text, version), encoding="utf-8", newline="\n")
    print(version)
