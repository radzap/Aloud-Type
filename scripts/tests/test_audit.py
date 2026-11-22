import sys, tempfile, unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from aloud_release import audit  # noqa: E402


class SourceScan(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)

    def write(self, rel, text):
        p = self.root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text)

    def test_clean_tree_has_no_findings(self):
        self.write("crates/a/src/lib.rs", "fn main() { println!(\"hi\"); }")
        self.assertEqual(audit.scan_sources(self.root), [])

    def test_socket_use_outside_the_downloader_is_a_finding(self):
        self.write("crates/a/src/lib.rs", "use std::net::TcpStream;")
        f = audit.scan_sources(self.root)
        self.assertEqual(len(f), 1)
        self.assertIn("crates/a/src/lib.rs", f[0])

    def test_http_client_use_is_allowed_only_in_the_download_module(self):
        self.write("crates/settings-ui/src/download.rs", "let a: ureq::Agent;")
        self.write("crates/a/src/x.rs", "let a: ureq::Agent;")
        f = audit.scan_sources(self.root)
        self.assertEqual(len(f), 1)
        self.assertIn("crates/a/src/x.rs", f[0])

    def test_urls_are_findings_unless_on_the_allow_list(self):
        self.write("crates/models/src/lib.rs", 'u("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/x")')
        self.write("crates/settings-ui/src/items.rs", 'c("https://github.com/radzap/Aloud-Type")')
        self.write("crates/a/src/lib.rs", 'u("https://telemetry.example.com/ping")')
        f = audit.scan_sources(self.root)
        self.assertEqual(len(f), 1)
        self.assertIn("telemetry.example.com", f[0])

    def test_update_check_endpoints_are_findings(self):
        self.write("crates/a/src/lib.rs", 'u("https://api.github.com/repos/x/y/releases/latest")')
        self.assertTrue(audit.scan_sources(self.root))

    def test_tests_directories_are_not_scanned(self):
        self.write("crates/a/tests/t.rs", "use std::net::TcpListener;")
        self.assertEqual(audit.scan_sources(self.root), [])


class ResidentDependencies(unittest.TestCase):
    def test_http_crates_in_the_resident_tree_are_findings(self):
        tree = "aloud-type-app v0.1.0\ncpal v0.18.2\nureq v3.5.2\nnative-tls v0.2.18\n"
        self.assertEqual(sorted(audit.network_crates(tree)), ["native-tls", "ureq"])

    def test_clean_tree(self):
        self.assertEqual(audit.network_crates("aloud-type-app v0.1.0\ncpal v0.18.2\n"), [])


class RealRepository(unittest.TestCase):
    def test_sources_make_no_network_calls_other_than_the_model_download(self):
        self.assertEqual(audit.scan_sources(ROOT), [])

    def test_resident_process_crate_links_no_network_crates(self):
        self.assertEqual(audit.network_crates(audit.cargo_tree(ROOT, "aloud-type-app")), [])


if __name__ == "__main__":
    unittest.main()
