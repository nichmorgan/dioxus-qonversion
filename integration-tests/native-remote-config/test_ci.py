"""CI protocol/path fixtures only; no native SDK mock or simulated payload."""
import json
import ast
import re
import shlex
from pathlib import Path
import tempfile
import unittest
import ci
import run as runner


class PublicationTests(unittest.TestCase):
    KEY = "ci-canary-not-a-real-project-key"

    def fixture(self, folder, status="PASSED"):
        root = Path(folder) / "source"
        artifacts = runner.Artifacts(root / "artifacts" / "run-fixture", self.KEY)
        artifacts.capture("sanitized diagnostic")
        record = artifacts.complete(status)
        return root, artifacts, record, Path(folder) / "published"

    def test_pass_and_failure_preserve_the_exit_code_and_copy_only_authorized_text(self):
        for status, code in (("PASSED", 0), ("FAILED", 1)):
            with self.subTest(status=status), tempfile.TemporaryDirectory() as folder:
                root, artifacts, record, destination = self.fixture(folder, status)
                (artifacts.raw / "private.so").write_bytes(self.KEY.encode())
                self.assertEqual(ci.stage_result(record, code, root, self.KEY, destination), code)
                self.assertEqual({p.name for p in destination.iterdir()}, runner.Artifacts.ALLOWED)

    def test_skip_missing_result_and_mismatched_exit_codes_are_failures(self):
        for record, code in ((None, 0), ({"status": "SKIPPED", "exit_code": 77}, 77),
                             ({"status": "PASSED", "exit_code": 0}, 1),
                             ({"status": "PASSED", "exit_code": True}, 0),
                             ({"status": "PASSED", "exit_code": 0}, 0)):
            with self.subTest(record=record), tempfile.TemporaryDirectory() as folder:
                with self.assertRaises(RuntimeError):
                    ci.stage_result(record, code, Path(folder), self.KEY, Path(folder) / "publish")

    def test_outside_root_raw_and_traversal_paths_are_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            root, artifacts, record, destination = self.fixture(folder)
            for value in (str(Path(folder)), str(artifacts.raw), str(artifacts.safe / ".." / "safe")):
                with self.subTest(value=value), self.assertRaises(RuntimeError):
                    ci.stage_result(dict(record, safe_artifacts=value), 0, root, self.KEY, destination)

    def test_missing_reports_extra_files_and_unsanitized_text_block_publication(self):
        for scenario in ("missing", "extra", "key"):
            with self.subTest(scenario=scenario), tempfile.TemporaryDirectory() as folder:
                root, artifacts, record, destination = self.fixture(folder)
                if scenario == "missing":
                    (artifacts.safe / "result.json").unlink()
                elif scenario == "extra":
                    (artifacts.safe / "private.so").write_bytes(b"binary")
                else:
                    (artifacts.safe / "diagnostics.txt").write_text(self.KEY)
                with self.assertRaises(RuntimeError):
                    ci.stage_result(record, 0, root, self.KEY, destination)
                self.assertFalse(destination.exists())

    def test_modified_result_cannot_turn_failure_into_success(self):
        with tempfile.TemporaryDirectory() as folder:
            root, artifacts, record, destination = self.fixture(folder, "FAILED")
            with self.assertRaises(RuntimeError):
                ci.stage_result(dict(record, status="PASSED", exit_code=0), 0, root, self.KEY, destination)


class WorkflowBuildRegressionTests(unittest.TestCase):
    def workflow(self):
        return (runner.ROOT.parents[1] / ".github/workflows/native-remote-config.yml").read_text(encoding="utf-8")

    def test_android_setup_overrides_removed_legacy_tools(self):
        configuration = self.workflow().split("uses: android-actions/setup-android@v3", 1)[1].split("      - ", 1)[0]
        match = re.search(r"packages:\s*([^\n]+)", configuration)
        self.assertIsNotNone(match, "action default includes removed tools")
        self.assertEqual(match[1].split(), ["platform-tools"])

    def test_emulator_owns_boot_and_runs_the_native_test_without_a_secret(self):
        workflow = self.workflow()
        step = workflow.split("uses: reactivecircus/android-emulator-runner@v2", 1)[1].split("      - name:", 1)[0]
        self.assertNotIn("QONVERSION_PROJECT_KEY", step)
        self.assertIn("-no-window", step)
        self.assertIn("get-state", step)
        self.assertIn(":app:connectedDebugAndroidTest -PrunNativeRemoteConfig=true", step)
        self.assertIn("&&", step.split("connectedDebugAndroidTest", 1)[1].split("verify-android", 1)[0])
        self.assertNotIn("- name: Stop Emulator", workflow)

    def test_xcode_links_the_exact_archive_path_including_spaces(self):
        project = (runner.ROOT / "ios/project.yml").read_text(encoding="utf-8")
        self.assertNotIn("framework: ${RUST_LIBRARY}", project)
        block = re.search(r"OTHER_LDFLAGS:\n((?:[ ]+-[^\n]+\n)+)", project)
        self.assertIsNotNone(block, "Rust archive must be an explicit linker input")
        values = []
        for line in block[1].splitlines():
            value = line.strip()[2:]
            values.append(ast.literal_eval(value) if value.startswith(("'", '\"')) else value)
        library = "/tmp/path with spaces/libnative_remote_config_test.a"
        flags = " ".join(values).replace("$(inherited)", "").replace("${RUST_LIBRARY}", library)
        flags = flags.replace("$(RUST_NATIVE_LIBS)", "-lSystem -framework Foundation")
        self.assertEqual(shlex.split(flags), [library, "-lSystem", "-framework", "Foundation"])
