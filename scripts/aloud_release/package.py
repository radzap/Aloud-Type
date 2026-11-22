"""Portable zip layout and optional code signing for a release."""
import zipfile
from pathlib import Path

TIMESTAMP_URL = "http://timestamp.digicert.com"


def build_zip(out, *, version, exe, licence, notices, readme, extra_files=()):
    top = f"AloudType-{version}"
    files = [Path(exe), Path(licence), Path(notices), Path(readme), *map(Path, extra_files)]
    for f in files:
        if not f.is_file():
            raise FileNotFoundError(f)
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for f in files:
            z.write(f, f"{top}/{f.name}")


def sign_command(path, env):
    """signtool command line when a certificate is configured, else None (ship unsigned)."""
    pfx = env.get("ALOUDTYPE_SIGN_PFX")
    if not pfx:
        return None
    cmd = ["signtool", "sign", "/fd", "sha256", "/f", pfx]
    if env.get("ALOUDTYPE_SIGN_PASSWORD"):
        cmd += ["/p", env["ALOUDTYPE_SIGN_PASSWORD"]]
    return cmd + ["/tr", TIMESTAMP_URL, "/td", "sha256", str(path)]
