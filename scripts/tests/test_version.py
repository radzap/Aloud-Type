import sys, unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from aloud_release import version  # noqa: E402


class NextVersion(unittest.TestCase):
    def test_first_release_uses_the_workspace_version(self):
        self.assertEqual(version.next_version("0.1.0", []), "0.1.0")

    def test_each_release_bumps_the_patch_after_the_newest_tag(self):
        self.assertEqual(version.next_version("0.1.0", ["v0.1.0", "v0.1.2", "v0.1.1"]), "0.1.3")

    def test_a_hand_bumped_workspace_version_wins(self):
        self.assertEqual(version.next_version("0.2.0", ["v0.1.7"]), "0.2.0")

    def test_tags_compare_numerically_and_odd_tags_are_ignored(self):
        self.assertEqual(version.next_version("0.1.0", ["v0.1.9", "v0.1.10", "nightly", "v1.0"]), "0.1.11")


class CargoToml(unittest.TestCase):
    TEXT = '[workspace]\nmembers = ["crates/*"]\n\n[workspace.package]\nversion = "0.1.0"\nedition = "2021"\n' \
           '\n[workspace.dependencies]\nfoo = { version = "1.2.3" }\n'

    def test_reads_and_rewrites_only_the_workspace_version(self):
        self.assertEqual(version.workspace_version(self.TEXT), "0.1.0")
        out = version.set_workspace_version(self.TEXT, "0.1.4")
        self.assertIn('version = "0.1.4"\nedition', out)
        self.assertIn('foo = { version = "1.2.3" }', out)


if __name__ == "__main__":
    unittest.main()
