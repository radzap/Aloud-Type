"""Build a release: portable zip and (when Inno Setup is installed) the per-user installer.

  python scripts/release.py [--skip-build] [--target x86_64-pc-windows-msvc]

Environment: LIBCLANG_PATH for the build; ALOUDTYPE_SIGN_PFX / ALOUDTYPE_SIGN_PASSWORD to sign
(otherwise the release is unsigned); ISCC to point at ISCC.exe.
"""
import argparse
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from aloud_release import audit, notices, package  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]


def workspace_version():
    m = re.search(r'^version\s*=\s*"([^"]+)"', (ROOT / "Cargo.toml").read_text(encoding="utf-8"), re.M)
    return m.group(1)


def sign(path):
    cmd = package.sign_command(path, os.environ)
    if cmd is None:
        print(f"unsigned (no certificate configured): {path.name}")
    else:
        subprocess.run(cmd, check=True)


def find_iscc():
    for c in (os.environ.get("ISCC"), shutil.which("iscc"),
              str(Path(os.environ.get("LOCALAPPDATA", "")) / "Programs/Inno Setup 6/ISCC.exe"),
              r"C:\Program Files (x86)\Inno Setup 6\ISCC.exe"):
        if c and Path(c).is_file():
            return c
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--skip-build", action="store_true")
    ap.add_argument("--target", default="x86_64-pc-windows-msvc")
    a = ap.parse_args()
    version = workspace_version()

    bad = audit.scan_sources(ROOT) + [f"resident process links network crate: {c}"
                                      for c in audit.network_crates(audit.cargo_tree(ROOT, "aloud-type-app"))]
    if bad:
        sys.exit("no-network audit failed:\n" + "\n".join(bad))

    if not a.skip_build:
        subprocess.run(["cargo", "build", "--release", "--locked", "-p", "aloud-type-app", "-p", "aloud-type-settings-ui",
                        "--target", a.target],
                       cwd=ROOT, check=True)
    exe_src = ROOT / "target" / a.target / "release" / "aloud-type.exe"
    settings_src = exe_src.with_name("aloud-type-settings-ui.exe")
    for built in (exe_src, settings_src):
        if not built.is_file():
            sys.exit(f"missing {built}")

    dist = ROOT / "dist"
    stage = dist / "stage"
    shutil.rmtree(dist, ignore_errors=True)
    stage.mkdir(parents=True)
    exe = shutil.copy2(exe_src, stage / "aloud-type.exe")
    sign(Path(exe))
    settings_exe = shutil.copy2(settings_src, stage / settings_src.name)
    sign(Path(settings_exe))
    shutil.copy2(ROOT / "LICENSE", stage / "LICENSE")
    (stage / "THIRD-PARTY-NOTICES.txt").write_text(notices.generate(notices.cargo_metadata(ROOT, a.target)),
                                                   encoding="utf-8", newline="\n")
    readme = (ROOT / "packaging" / "README.txt").read_text(encoding="utf-8").replace("{version}", version)
    (stage / "README.txt").write_text(readme, encoding="utf-8", newline="\r\n")
    gpu_libs = sorted(p for p in exe_src.parent.glob("ggml-*.dll"))  # GPU backend library, if the build produces one
    extra = [Path(shutil.copy2(p, stage / p.name)) for p in gpu_libs]
    for p in extra:
        sign(p)

    zip_path = dist / f"AloudType-{version}-portable.zip"
    package.build_zip(zip_path, version=version, exe=stage / "aloud-type.exe", settings_exe=stage / "aloud-type-settings-ui.exe", licence=stage / "LICENSE",
                      notices=stage / "THIRD-PARTY-NOTICES.txt", readme=stage / "README.txt", extra_files=extra)
    print("wrote", zip_path)

    iscc = find_iscc()
    if iscc is None:
        print("Inno Setup (ISCC.exe) not found: installer NOT built. Install Inno Setup 6 or set ISCC.")
        return
    subprocess.run([iscc, f"/DAppVersion={version}", f"/DStageDir={stage}", str(ROOT / "packaging" / "aloud-type.iss")],
                   check=True)
    setup = dist / f"AloudType-{version}-setup.exe"
    sign(setup)
    print("wrote", setup)


if __name__ == "__main__":
    main()
