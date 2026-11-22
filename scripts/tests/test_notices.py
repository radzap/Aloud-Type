import json, os, subprocess, sys, tempfile, unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from aloud_release import notices  # noqa: E402


def pkg(pid, name, version, lic, manifest_dir, source="registry+x"):
    return {"id": pid, "name": name, "version": version, "license": lic, "source": source,
            "repository": f"https://example.com/{name}", "manifest_path": str(Path(manifest_dir) / "Cargo.toml")}


def meta(packages, nodes, members):
    return {"packages": packages, "workspace_members": members, "resolve": {"nodes": nodes}}


class NoticesFromMetadata(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)

    def crate(self, name, files):
        d = Path(self.tmp.name) / name
        d.mkdir()
        for f, text in files.items():
            (d / f).write_text(text)
        return d

    def test_lists_shipped_dependencies_with_licence_text_and_skips_workspace_and_dev_only(self):
        a = self.crate("a", {"LICENSE-MIT": "MIT TEXT A"})
        b = self.crate("b", {"LICENSE": "MIT TEXT A"})  # same text as a: grouped
        c = self.crate("c", {"LICENSE-APACHE": "APACHE TEXT"})
        dev = self.crate("devonly", {"LICENSE": "DEV TEXT"})
        ws = self.crate("ws", {})
        m = meta(
            [pkg("ws", "ws", "0.1.0", "AGPL-3.0-or-later", ws, source=None),
             pkg("a", "a", "1.0.0", "MIT", a), pkg("b", "b", "2.0.0", "MIT", b),
             pkg("c", "c", "3.0.0", "Apache-2.0", c), pkg("devonly", "devonly", "1.0.0", "MIT", dev)],
            [{"id": "ws", "deps": [{"pkg": "a", "dep_kinds": [{"kind": None}]},
                                    {"pkg": "devonly", "dep_kinds": [{"kind": "dev"}]}]},
             {"id": "a", "deps": [{"pkg": "b", "dep_kinds": [{"kind": None}]},
                                  {"pkg": "c", "dep_kinds": [{"kind": "build"}]}]},
             {"id": "b", "deps": []}, {"id": "c", "deps": []}, {"id": "devonly", "deps": []}],
            ["ws"])
        text = notices.generate(m)
        for needle in ("a 1.0.0", "b 2.0.0", "c 3.0.0", "MIT TEXT A", "APACHE TEXT", "Apache-2.0"):
            self.assertIn(needle, text)
        self.assertNotIn("devonly", text)
        self.assertNotIn("ws 0.1.0", text)
        self.assertEqual(text.count("MIT TEXT A"), 1)  # grouped by identical text

    def test_package_without_licence_file_is_flagged_not_silently_dropped(self):
        a = self.crate("a", {})
        m = meta([pkg("ws", "ws", "0.1.0", "MIT", a, source=None), pkg("a", "a", "1.0.0", "MIT OR Apache-2.0", a)],
                 [{"id": "ws", "deps": [{"pkg": "a", "dep_kinds": [{"kind": None}]}]}, {"id": "a", "deps": []}], ["ws"])
        text = notices.generate(m)
        self.assertIn("a 1.0.0", text)
        self.assertIn("MIT OR Apache-2.0", text)
        self.assertIn("no licence file", text.lower())

    def test_output_is_deterministic(self):
        a = self.crate("a", {"LICENSE": "T"})
        m = meta([pkg("ws", "ws", "0.1.0", "MIT", a, source=None), pkg("a", "a", "1.0.0", "MIT", a)],
                 [{"id": "ws", "deps": [{"pkg": "a", "dep_kinds": [{"kind": None}]}]}, {"id": "a", "deps": []}], ["ws"])
        self.assertEqual(notices.generate(m), notices.generate(m))


class NoticesFromRealDependencyTree(unittest.TestCase):
    def test_every_third_party_crate_in_the_real_tree_appears(self):
        m = notices.cargo_metadata(ROOT)
        text = notices.generate(m)
        shipped = notices.shipped_packages(m)
        self.assertGreater(len(shipped), 50)
        for p in shipped:
            self.assertIn(f"{p['name']} {p['version']}", text)
        self.assertIn("whisper-rs", text)
        self.assertIn("Whisper", text)  # model/ggml courtesy section
        self.assertIn("Silero", text)


if __name__ == "__main__":
    unittest.main()
