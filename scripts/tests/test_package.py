import sys, tempfile, unittest, zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from aloud_release import package  # noqa: E402


class PortableZip(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.d = Path(self.tmp.name)
        (self.d / "aloud-type.exe").write_bytes(b"MZ")
        (self.d / "aloud-type-settings-ui.exe").write_bytes(b"MZ")
        (self.d / "LICENSE").write_text("AGPL")
        (self.d / "THIRD-PARTY-NOTICES.txt").write_text("notices")
        (self.d / "README.txt").write_text("readme")

    def build(self, **kw):
        out = self.d / "out.zip"
        package.build_zip(out, version="0.1.0", exe=self.d / "aloud-type.exe",
                          settings_exe=self.d / "aloud-type-settings-ui.exe", licence=self.d / "LICENSE",
                          notices=self.d / "THIRD-PARTY-NOTICES.txt", readme=self.d / "README.txt", **kw)
        return zipfile.ZipFile(out).namelist()

    def test_zip_holds_both_exes_licence_notices_readme_under_one_versioned_folder(self):
        names = self.build()
        self.assertCountEqual(names, [
            "AloudType-0.1.0/aloud-type.exe", "AloudType-0.1.0/aloud-type-settings-ui.exe", "AloudType-0.1.0/LICENSE",
            "AloudType-0.1.0/THIRD-PARTY-NOTICES.txt", "AloudType-0.1.0/README.txt"])

    def test_gpu_backend_libraries_are_included_when_the_build_has_them(self):
        (self.d / "ggml-vulkan.dll").write_bytes(b"x")
        names = self.build(extra_files=[self.d / "ggml-vulkan.dll"])
        self.assertIn("AloudType-0.1.0/ggml-vulkan.dll", names)

    def test_zip_never_contains_user_data_or_models(self):
        for n in self.build():
            self.assertFalse(n.endswith((".bin", ".toml", ".gguf")), n)

    def test_missing_licence_or_notices_refuses_to_build(self):
        (self.d / "LICENSE").unlink()
        with self.assertRaises(FileNotFoundError):
            self.build()

    def test_zip_has_no_portable_marker_so_data_location_is_shared_with_the_installer(self):
        self.assertFalse([n for n in self.build() if "portable" in n.lower() and n.endswith(".ini")])


class Signing(unittest.TestCase):
    def test_unsigned_without_a_certificate(self):
        self.assertIsNone(package.sign_command(Path("a.exe"), {}))

    def test_signs_with_signtool_when_a_pfx_is_configured(self):
        cmd = package.sign_command(Path("a.exe"), {"ALOUDTYPE_SIGN_PFX": "c.pfx", "ALOUDTYPE_SIGN_PASSWORD": "pw"})
        self.assertEqual(cmd[0], "signtool")
        self.assertIn("c.pfx", cmd)
        self.assertEqual(cmd[-1], "a.exe")
        self.assertIn("/tr", cmd)  # timestamped

    def test_certificate_without_password_is_still_attempted_only_when_pfx_set(self):
        self.assertIsNone(package.sign_command(Path("a.exe"), {"ALOUDTYPE_SIGN_PASSWORD": "pw"}))


if __name__ == "__main__":
    unittest.main()
