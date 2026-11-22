"""Build a release: portable zip and (when Inno Setup is installed) the per-user installer.

  python scripts/release.py [--skip-build] [--cpu-only] [--target x86_64-pc-windows-msvc]

Environment: LIBCLANG_PATH for the build; VULKAN_SDK (set by the Vulkan SDK installer) for the
GPU backend, which release builds include unless --cpu-only; ALOUDTYPE_TARGET_DIR for the Cargo
target folder (default: <drive>:\aloudtype-build, because the Vulkan build's nested CMake
paths exceed the 260-character Windows path limit under the repo's own target folder);
ALOUDTYPE_SIGN_PFX / ALOUDTYPE_SIGN_PASSWORD to sign (otherwise the release is unsigned); ISCC to
point at ISCC.exe.
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


# The Vulkan build nests CMake scratch files 228 characters below Cargo's profile folder
# (<target-dir>/release), and MSBuild cannot write paths over 259 characters.
MAX_PROFILE_DIR_LEN = 31


def target_dir():
    return Path(os.environ.get("ALOUDTYPE_TARGET_DIR") or Path(ROOT.anchor) / "aloudtype-build")


def host_triple():
    out = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=True).stdout
    return re.search(r"^host:\s*(\S+)", out, re.M).group(1)


def profile_dir(target, host):
    """Where Cargo puts the release build. For the host target `--target` is left out, which saves
    the `<triple>` folder level (23 characters of the path budget)."""
    return target_dir() / "release" if target == host else target_dir() / target / "release"


def build_command(target, host, gpu=True):
    cmd = ["cargo", "build", "--release", "--locked", "-p", "aloud-type-app", "-p", "aloud-type-settings-ui",
           "--target-dir", str(target_dir())]
    if target != host:
        cmd += ["--target", target]
    if gpu:
        cmd += ["--features", "aloud-type-app/vulkan"]
    return cmd


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
    ap.add_argument("--cpu-only", action="store_true", help="build without the Vulkan GPU backend")
    ap.add_argument("--require-gpu", action="store_true",
                    help="fail unless the built exe contains the Vulkan GPU backend (CI releases)")
    ap.add_argument("--require-installer", action="store_true",
                    help="fail when Inno Setup is missing instead of building only the zip (CI releases)")
    a = ap.parse_args()
    if a.require_gpu and a.cpu_only:
        sys.exit("--require-gpu and --cpu-only contradict each other")
    version = workspace_version()
    host = host_triple()

    bad = audit.scan_sources(ROOT) + [f"resident process links network crate: {c}"
                                      for c in audit.network_crates(audit.cargo_tree(ROOT, "aloud-type-app"))]
    if bad:
        sys.exit("no-network audit failed:\n" + "\n".join(bad))

    if not a.skip_build:
        if not a.cpu_only and not os.environ.get("VULKAN_SDK"):
            sys.exit("VULKAN_SDK is not set: install the Vulkan SDK (winget install KhronosGroup.VulkanSDK) and "
                     "open a new terminal, or pass --cpu-only for a build without GPU acceleration.")
        if not a.cpu_only and len(str(profile_dir(a.target, host))) > MAX_PROFILE_DIR_LEN:
            sys.exit(f"build folder {profile_dir(a.target, host)} is too long for the Vulkan build (over "
                     f"{MAX_PROFILE_DIR_LEN} characters): set ALOUDTYPE_TARGET_DIR to a short folder such as C:\\atb.")
        subprocess.run(build_command(a.target, host, gpu=not a.cpu_only), cwd=ROOT, check=True)
    exe_src = profile_dir(a.target, host) / "aloud-type.exe"
    settings_src = exe_src.with_name("aloud-type-settings-ui.exe")
    for built in (exe_src, settings_src):
        if not built.is_file():
            sys.exit(f"missing {built}")
    gpu_libs = sorted(p for p in exe_src.parent.glob("ggml-*.dll"))  # GPU backend library, if the build produces one
    if package.has_vulkan_backend(exe_src, *gpu_libs):
        print("Vulkan GPU backend: included")
    elif a.require_gpu:
        sys.exit(f"{exe_src} has no Vulkan GPU backend: the build did not enable the vulkan feature")
    else:
        print("Vulkan GPU backend: NOT included (CPU-only build)")

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
    extra = [Path(shutil.copy2(p, stage / p.name)) for p in gpu_libs]
    for p in extra:
        sign(p)

    zip_path = dist / f"AloudType-{version}-portable.zip"
    package.build_zip(zip_path, version=version, exe=stage / "aloud-type.exe", settings_exe=stage / "aloud-type-settings-ui.exe", licence=stage / "LICENSE",
                      notices=stage / "THIRD-PARTY-NOTICES.txt", readme=stage / "README.txt", extra_files=extra)
    print("wrote", zip_path)

    iscc = find_iscc()
    if iscc is None:
        if a.require_installer:
            sys.exit("Inno Setup (ISCC.exe) not found: install Inno Setup 6 or set ISCC.")
        print("Inno Setup (ISCC.exe) not found: installer NOT built. Install Inno Setup 6 or set ISCC.")
        return
    subprocess.run([iscc, f"/DAppVersion={version}", f"/DStageDir={stage}", str(ROOT / "packaging" / "aloud-type.iss")],
                   check=True)
    setup = dist / f"AloudType-{version}-setup.exe"
    sign(setup)
    print("wrote", setup)


if __name__ == "__main__":
    main()
