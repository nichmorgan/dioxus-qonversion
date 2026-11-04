"""Local, stdlib-only tests; no iOS command or network request is executed."""
import contextlib
import io
import os
from pathlib import Path
import plistlib
import tempfile
import unittest
from unittest import mock

import ios_remote_config as ci


class DiagnosticsTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.patches = [
            mock.patch.object(ci, "ROOT", self.root),
            mock.patch.object(ci, "SAFE", self.root / "safe"),
            mock.patch.object(ci, "RAW", self.root / "raw"),
            mock.patch.object(ci, "CONFIG", self.root / "ci.toml"),
            mock.patch.dict(os.environ, {"QONVERSION_PROJECT_KEY": "unit-test-project-key", "IOS_TARGET": "aarch64-apple-ios-sim"}),
        ]
        for patch in self.patches:
            patch.start()
        ci.SAFE.mkdir()
        ci.RAW.mkdir()
        ci.CONFIG.write_text('[bundle]\nidentifier="com.example.qonversion.remoteconfig"\n', encoding="utf-8")

    def tearDown(self):
        for patch in reversed(self.patches):
            patch.stop()
        self.temp.cleanup()

    def test_redacts_raw_and_trimmed_key(self):
        with mock.patch.dict(os.environ, {"QONVERSION_PROJECT_KEY": " unit-test-project-key "}):
            self.assertEqual(ci.redact(" unit-test-project-key /unit-test-project-key"), "[REDACTED]/[REDACTED]")

    def test_command_output_is_redacted_before_console_and_file(self):
        import sys
        console = io.StringIO()
        with contextlib.redirect_stdout(console):
            ci.run([sys.executable, "-c", "print('unit-test-project-key')"], "command.log")
        self.assertNotIn("unit-test-project-key", console.getvalue())
        self.assertEqual((ci.SAFE / "command.log").read_text().strip(), "[REDACTED]")

    def test_key_is_not_forwarded_to_maestro_or_inspection_tools(self):
        self.assertNotIn("QONVERSION_PROJECT_KEY", ci.child_env())
        self.assertEqual(ci.child_env(with_key=True)["QONVERSION_PROJECT_KEY"], "unit-test-project-key")

    def test_build_restores_existing_configuration_after_failure(self):
        config = self.root / "Dioxus.toml"
        config.write_bytes(b"original local configuration\r\n")
        with mock.patch.object(ci, "run", side_effect=RuntimeError("simulated build error")):
            with self.assertRaisesRegex(RuntimeError, "simulated build error"):
                ci.build()
        self.assertEqual(config.read_bytes(), b"original local configuration\r\n")

    def test_build_removes_temporary_configuration_after_failure(self):
        with mock.patch.object(ci, "run", side_effect=RuntimeError("simulated build error")):
            with self.assertRaises(RuntimeError):
                ci.build()
        self.assertFalse((self.root / "Dioxus.toml").exists())

    def test_app_selection_requires_matching_bundle_identifier(self):
        app = self.root / "target/dx/demo/Demo.app"
        app.mkdir(parents=True)
        info = {"CFBundleIdentifier": "com.example.qonversion.remoteconfig", "CFBundleExecutable": "Demo"}
        with (app / "Info.plist").open("wb") as output:
            plistlib.dump(info, output)
        self.assertEqual(ci.find_app()[0], app)
        info["CFBundleIdentifier"] = "com.example.other"
        with (app / "Info.plist").open("wb") as output:
            plistlib.dump(info, output)
        with self.assertRaisesRegex(RuntimeError, "found 0"):
            ci.find_app()

    def test_artifact_allowlist_redacts_reports_and_omits_binaries(self):
        (ci.RAW / "report.xml").write_text('<failure>unit-test-project-key</failure>', encoding="utf-8")
        (ci.RAW / "app.zip").write_bytes(b"unit-test-project-key")
        (ci.RAW / "Demo.app").mkdir()
        (ci.RAW / "Demo.app/config.json").write_text('{}', encoding="utf-8")
        (ci.RAW / "screenshot.png").write_bytes(b"\x89PNG\r\nunit-test-screenshot")
        ci.publish_safe_files()
        report = (ci.SAFE / "maestro/report.xml").read_text()
        self.assertIn("[REDACTED]", report)
        self.assertNotIn("unit-test-project-key", report)
        self.assertFalse((ci.SAFE / "maestro/app.zip").exists())
        self.assertFalse((ci.SAFE / "maestro/Demo.app").exists())
        self.assertTrue((ci.SAFE / "maestro/screenshot.png").exists())

    def test_png_with_literal_key_bytes_is_rejected(self):
        (ci.RAW / "bad.png").write_bytes(b"\x89PNG\r\nunit-test-project-key")
        with self.assertRaisesRegex(RuntimeError, "artifact rejected"):
            ci.publish_safe_files()
        self.assertFalse((ci.SAFE / "maestro/bad.png").exists())

    def test_reports_survive_unavailable_simulator(self):
        (ci.RAW / "report.xml").write_text('<testsuite failures="1"/>', encoding="utf-8")
        with mock.patch.object(ci, "simulator", side_effect=RuntimeError("no simulator")):
            ci.collect()
        self.assertTrue((ci.SAFE / "simulator-collection-warning.txt").exists())
        self.assertTrue((ci.SAFE / "maestro/report.xml").exists())


if __name__ == "__main__":
    unittest.main()
