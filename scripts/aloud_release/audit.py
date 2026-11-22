"""No-network audit: the only network code is the Speech model download."""
import re
import subprocess
import sys
from pathlib import PurePosixPath, Path

# Crates that can talk to the network. The resident process must not link any of them.
NETWORK_CRATES = {"ureq", "ureq-proto", "native-tls", "reqwest", "hyper", "curl", "isahc", "attohttpc",
                  "minreq", "tungstenite", "tokio-tungstenite", "surf", "rustls", "openssl", "schannel"}
# Only the model downloader may use an HTTP client.
CLIENT_ALLOWED = {"crates/settings-ui/src/download.rs"}
CLIENT_RE = re.compile(r"\bureq\b|\breqwest\b|\bhyper::|native_tls|\bwinhttp\b|\bwininet\b|WinHttp|InternetOpen", re.I)
SOCKET_RE = re.compile(r"TcpStream|TcpListener|UdpSocket|ToSocketAddrs|WSAStartup|\bconnect\(\s*socket")
URL_RE = re.compile(r"https?://[^\s\"')>]+")
# URLs the app may contain: the model host (downloaded on request) and links the user opens in a browser.
URL_ALLOWED_PREFIXES = {
    "crates/models/src/lib.rs": ("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/",
                                 "https://huggingface.co/ggml-org/whisper-vad/resolve/main/"),
    "crates/settings-ui/src/items.rs": ("https://github.com/radzap/Aloud-Type",),
}
UPDATE_RE = re.compile(r"releases/latest|/update|check[_-]?for[_-]?update", re.I)
SKIP_DIRS = {"tests", "target", ".git", "examples"}


def _rel(root, path):
    return PurePosixPath(path.relative_to(root).as_posix()).as_posix()


def scan_sources(root):
    root = Path(root)
    findings = []
    for path in sorted((root / "crates").rglob("*.rs")):
        rel = _rel(root, path)
        if SKIP_DIRS & set(path.relative_to(root).parts):
            continue
        for n, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
            where = f"{rel}:{n}"
            if SOCKET_RE.search(line):
                findings.append(f"{where}: raw socket use: {line.strip()}")
            if CLIENT_RE.search(line) and rel not in CLIENT_ALLOWED:
                findings.append(f"{where}: HTTP client outside the model downloader: {line.strip()}")
            for url in URL_RE.findall(line):
                if not url.startswith(URL_ALLOWED_PREFIXES.get(rel, ())) or UPDATE_RE.search(url):
                    findings.append(f"{where}: URL not on the allow list: {url}")
    return findings


def network_crates(tree_text):
    names = set()
    for line in tree_text.splitlines():
        m = re.match(r"\s*([A-Za-z0-9_\-]+) v\d", line)
        if m and m.group(1) in NETWORK_CRATES:
            names.add(m.group(1))
    return sorted(names)


def cargo_tree(root, package):
    return subprocess.run(["cargo", "tree", "-p", package, "-e", "normal", "--prefix", "none",
                           "--target", "x86_64-pc-windows-msvc", "--locked"],
                          cwd=root, capture_output=True, text=True, encoding="utf-8", check=True).stdout


if __name__ == "__main__":
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path.cwd()
    bad = scan_sources(root) + [f"resident process links network crate: {c}"
                                for c in network_crates(cargo_tree(root, "aloud-type-app"))]
    print("\n".join(bad) or "no-network audit passed")
    sys.exit(1 if bad else 0)
