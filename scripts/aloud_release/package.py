"""Portable zip layout and optional code signing for a release."""
import zipfile
from pathlib import Path

TIMESTAMP_URL = "http://timestamp.digicert.com"


def build_zip(out, *, version, exe, settings_exe, licence, notices, readme, extra_files=()):
    top = f"AloudType-{version}"
    files = [Path(exe), Path(settings_exe), Path(licence), Path(notices), Path(readme), *map(Path, extra_files)]
    for f in files:
        if not f.is_file():
            raise FileNotFoundError(f)
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for f in files:
            z.write(f, f"{top}/{f.name}")


# Log prefix compiled in only from ggml-vulkan.cpp: present exactly when the Vulkan backend is linked.
VULKAN_MARKER = b"ggml_vulkan"


def has_vulkan_backend(*binaries):
    """True when the exe (or a ggml-*.dll shipped beside it) contains the Vulkan GPU backend."""
    return any(VULKAN_MARKER in Path(b).read_bytes() for b in binaries)


def sign_command(path, env):
    """signtool command line when a certificate is configured, else None (ship unsigned)."""
    pfx = env.get("ALOUDTYPE_SIGN_PFX")
    if not pfx:
        return None
    cmd = ["signtool", "sign", "/fd", "sha256", "/f", pfx]
    if env.get("ALOUDTYPE_SIGN_PASSWORD"):
        cmd += ["/p", env["ALOUDTYPE_SIGN_PASSWORD"]]
    return cmd + ["/tr", TIMESTAMP_URL, "/td", "sha256", str(path)]
