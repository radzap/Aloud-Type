import re, sys, unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ISS = (ROOT / "packaging" / "aloud-type.iss").read_text(encoding="utf-8")


def section(name):
    m = re.search(rf"^\[{name}\]\s*$(.*?)(?=^\[|\Z)", ISS, re.S | re.M)
    return m.group(1) if m else ""


class InstallerScript(unittest.TestCase):
    def test_per_user_install_needs_no_administrator_rights(self):
        s = section("Setup")
        self.assertRegex(s, r"PrivilegesRequired=lowest\b")
        self.assertNotRegex(s, r"PrivilegesRequiredOverridesAllowed=.*dialog")
        self.assertIn("{autopf}", s)  # resolves to %LOCALAPPDATA%\Programs when not admin
        self.assertNotIn("{commonpf", ISS)

    def test_stable_app_id_so_upgrades_replace_in_place(self):
        self.assertRegex(section("Setup"), r"AppId=\{\{[0-9A-F\-]{36}\}")

    def test_start_menu_entry_and_uninstaller(self):
        self.assertIn("{autoprograms}", section("Icons"))
        self.assertIn("UninstallDisplayIcon", section("Setup"))

    def test_launch_at_login_is_one_unchecked_by_default_task_writing_the_run_key(self):
        t = section("Tasks")
        self.assertIn("autostart", t)
        self.assertIn("unchecked", t)
        r = section("Registry")
        self.assertIn(r"Software\Microsoft\Windows\CurrentVersion\Run", r)
        self.assertIn("ValueName: \"AloudType\"", r)
        self.assertIn("Tasks: autostart", r)
        self.assertIn("uninsdeletevalue", r)
        self.assertIn('"""{app}' + chr(92) + 'aloud-type.exe"""', r)  # quoted path, as the app's own toggle writes

    def test_program_files_and_user_data_are_separate_and_installer_never_ships_data(self):
        f = section("Files")
        for needed in ("aloud-type.exe", "aloud-type-settings-ui.exe", "LICENSE", "THIRD-PARTY-NOTICES.txt"):
            self.assertIn(needed, f)
        self.assertNotRegex(f, r"\.(bin|toml)\b")
        self.assertNotIn("{localappdata}", section("InstallDelete") + section("Files"))
        self.assertNotIn("models", section("InstallDelete"))

    def test_uninstaller_asks_before_deleting_settings_and_models_and_defaults_to_keeping(self):
        c = section("Code")
        self.assertIn("CurUninstallStepChanged", c)
        self.assertIn("usPostUninstall", c)
        self.assertIn("MsgBox", c)
        self.assertIn("MB_DEFBUTTON2", c)  # default answer = keep
        self.assertIn("UninstallSilent", c)  # silent uninstall never deletes data
        self.assertIn(r"{localappdata}\AloudType", c)

    def test_running_app_is_closed_for_upgrade(self):
        self.assertRegex(section("Setup"), r"CloseApplications=yes")

    def test_licence_shown_in_the_installer(self):
        self.assertIn("LicenseFile=", section("Setup"))

    def test_installer_makes_no_network_calls(self):
        self.assertNotRegex(ISS.lower(), r"https?://(?!github\.com/radzap)|idp|downloadfile|inetget")


if __name__ == "__main__":
    unittest.main()
