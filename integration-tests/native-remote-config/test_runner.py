"""Runner opt-in guards only; these tests never load or simulate either SDK."""
import os
import base64
import contextlib
import io
import json
from pathlib import Path
import subprocess
import sys
import unittest
import tempfile
import run as runner

RUNNER = Path(__file__).with_name("run.py")


class OptInTests(unittest.TestCase):
    def invoke(self, *args):
        env = dict(os.environ)
        env.pop("QONVERSION_PROJECT_KEY", None)
        return subprocess.run([sys.executable, "-B", str(RUNNER), *args], env=env,
                              capture_output=True, text=True, timeout=10)

    def test_disabled_for_both_platforms_without_native_tools_or_credentials(self):
        for platform in ("ios", "android"):
            with self.subTest(platform=platform):
                result = self.invoke(platform)
                self.assertEqual(result.returncode, 77, result.stderr)
                self.assertIn("SKIPPED", result.stdout)
                self.assertEqual(json.loads(result.stdout.split("NATIVE_TEST_RESULT ")[-1])["status"], "SKIPPED")
                self.assertNotIn("Running:", result.stdout)

    def test_opt_in_requires_external_key_before_starting_native_tools(self):
        result = self.invoke("ios", "--run")
        self.assertEqual(result.returncode, 1)
        self.assertIn("QONVERSION_PROJECT_KEY must be supplied externally", result.stderr)
        self.assertNotIn("Running:", result.stdout)


class ResultTests(unittest.TestCase):
    # These are report-format fixtures, not SDK mocks or simulated payloads.
    def test_xctest_rejects_skipped_failed_missing_or_extra_results(self):
        runner.require_xctest_success(dict(totalTestCount=1, passedTests=1, failedTests=0, skippedTests=0))
        for summary in ({}, dict(totalTestCount=1, passedTests=0, failedTests=0, skippedTests=1),
                        dict(totalTestCount=2, passedTests=1, failedTests=1, skippedTests=0),
                        dict(totalTestCount=2, passedTests=2, failedTests=0, skippedTests=0)):
            with self.subTest(summary=summary), self.assertRaises(RuntimeError):
                runner.require_xctest_success(summary)

    def test_xctest_rejects_inconsistent_counts_and_non_integer_values(self):
        valid = dict(totalTestCount=1, passedTests=1, failedTests=0, skippedTests=0,
                     expectedFailures=0, testFailures=[], testResult="Passed")
        runner.require_xctest_success(valid)
        for changes in (dict(totalTestCount=2), dict(expectedFailures=1),
                        dict(testFailures=[{"failureText": "runner failed"}]),
                        dict(testResult="Failed"), dict(passedTests=True),
                        dict(passedTests=1.0)):
            with self.subTest(changes=changes), self.assertRaises(RuntimeError):
                runner.require_xctest_success(dict(valid, **changes))
        for summary in (None, [], "invalid"):
            with self.subTest(summary=summary), self.assertRaises(RuntimeError):
                runner.require_xctest_success(summary)

    def test_junit_rejects_suite_errors_wrong_identity_and_recent_stale_reports(self):
        with tempfile.TemporaryDirectory() as folder:
            reports = Path(folder)
            path = reports / "result.xml"
            case = '<testcase classname="io.dioxus.qonversion.nativetest.RemoteConfigTest" name="realRemoteConfig"/>'
            for xml in ('<testsuite failures="1">' + case + '</testsuite>',
                        '<testsuite errors="1">' + case + '</testsuite>',
                        '<testsuite skipped="1">' + case + '</testsuite>',
                        '<testsuite tests="0">' + case + '</testsuite>',
                        '<testsuite>' + case + '<error/></testsuite>',
                        '<testsuite>' + case.replace('realRemoteConfig', 'anotherTest') + '</testsuite>'):
                path.write_text(xml)
                with self.subTest(xml=xml), self.assertRaises(RuntimeError):
                    runner.require_junit_success(reports, 0)
            path.write_text('<testsuite>' + case + '</testsuite>')
            with self.assertRaises(RuntimeError):
                runner.require_junit_success(reports, path.stat().st_mtime + 1)
            path.write_text('<invalid')
            with self.assertRaises(runner.ET.ParseError):
                runner.require_junit_success(reports, 0)

    def test_xctest_opt_in_supports_both_plan_formats_and_rejects_ambiguity(self):
        for plan in ({"Tests": {"TestBundlePath": "test.xctest"}},
                     {"TestConfigurations": [{"TestTargets": [{"TestBundlePath": "test.xctest"}]}]}):
            enabled = runner.enable_xctest(plan)
            self.assertIn("QONVERSION_RUN_NATIVE_REMOTE_CONFIG", str(enabled))
            self.assertNotIn("QONVERSION_PROJECT_KEY", str(enabled))
        for plan in ({}, {"A": {"TestBundlePath": "a"}, "B": {"TestBundlePath": "b"}}):
            with self.assertRaises(RuntimeError):
                runner.enable_xctest(plan)

    def test_junit_requires_fresh_success_of_the_actual_contract_test(self):
        with tempfile.TemporaryDirectory() as folder:
            reports = Path(folder)
            with self.assertRaises(RuntimeError):
                runner.require_junit_success(reports, 0)
            path = reports / "result.xml"
            case = '<testcase classname="io.dioxus.qonversion.nativetest.RemoteConfigTest" name="realRemoteConfig">{}</testcase>'
            path.write_text('<testsuite>' + case.format('') + '</testsuite>')
            runner.require_junit_success(reports, 0)
            with self.assertRaises(RuntimeError):
                runner.require_junit_success(reports, path.stat().st_mtime + 10)
            for content in ('<skipped/>', '<failure/>', '<error/>'):
                path.write_text('<testsuite>' + case.format(content) + '</testsuite>')
                with self.assertRaises(RuntimeError):
                    runner.require_junit_success(reports, 0)
            path.write_text('<testsuite>' + case.format('') * 2 + '</testsuite>')
            with self.assertRaises(RuntimeError):
                runner.require_junit_success(reports, 0)


class SecurityRegressionTests(unittest.TestCase):
    # A canary tests redaction only; it is not a credential or SDK response.
    KEY = "regression-canary-not-a-real-project-key"

    def test_raw_and_safe_artifacts_are_separate_and_sensitive_text_is_redacted(self):
        with tempfile.TemporaryDirectory() as folder:
            artifacts = runner.Artifacts(Path(folder), self.KEY)
            binary = artifacts.raw / "library.so"
            binary.write_bytes(b"\x7fELF" + self.KEY.encode())
            encoded = base64.b64encode(self.KEY.encode()).decode()
            artifacts.capture("native output " + self.KEY + " " + encoded)
            result = artifacts.complete("FAILED", error=self.KEY)
            self.assertEqual(result["status"], "FAILED")
            self.assertEqual({p.name for p in artifacts.safe.iterdir()}, {"diagnostics.txt", "result.json"})
            for path in artifacts.safe.iterdir():
                self.assertNotIn(self.KEY.encode(), path.read_bytes())
                self.assertNotIn(encoded.encode(), path.read_bytes())
            self.assertIn("[REDACTED]", (artifacts.safe / "diagnostics.txt").read_text())
            self.assertIn(self.KEY.encode(), binary.read_bytes())

    def test_safe_directory_rejects_unapproved_files_and_unsanitized_content(self):
        with tempfile.TemporaryDirectory() as folder:
            artifacts = runner.Artifacts(Path(folder), self.KEY)
            forbidden = artifacts.safe / "library.so"
            forbidden.write_bytes(b"binary")
            with self.assertRaises(RuntimeError):
                artifacts.validate_safe()
            forbidden.unlink()
            (artifacts.safe / "diagnostics.txt").write_text(self.KEY, encoding="utf-8")
            with self.assertRaises(RuntimeError):
                artifacts.validate_safe()

    def test_only_cargo_receives_the_key_environment_variable(self):
        environment = dict(QONVERSION_PROJECT_KEY=self.KEY, PATH="tool-path")
        self.assertEqual(runner.child_environment(["cargo"], environment)["QONVERSION_PROJECT_KEY"], self.KEY)
        for executable in ("xcodebuild", "gradle.bat", "adb", sys.executable):
            with self.subTest(executable=executable):
                self.assertNotIn("QONVERSION_PROJECT_KEY", runner.child_environment([executable], environment))

    def test_redaction_handles_console_escapes_and_common_encoded_forms(self):
        encoded = base64.b64encode(self.KEY.encode()).decode()
        text = "\x1b[31m" + self.KEY + "\x1b[0m " + encoded
        cleaned = runner.sanitize(text, self.KEY)
        self.assertNotIn(self.KEY, cleaned)
        self.assertNotIn(encoded, cleaned)
        self.assertNotIn("\x1b", cleaned)

    def test_readme_has_legible_utf8_without_mojibake(self):
        text = RUNNER.with_name("README.md").read_bytes().decode("utf-8", errors="strict")
        for marker in ("\ufffd", "\u00c3\u00a3", "\u00c3\u00a7", "\u00e2\u2020\u2019"):
            self.assertNotIn(marker, text)


    def test_real_subprocess_output_is_sanitized_in_console_and_safe_files(self):
        with tempfile.TemporaryDirectory() as folder:
            artifacts = runner.Artifacts(Path(folder), self.KEY)
            previous = runner.ACTIVE
            runner.ACTIVE = artifacts
            output = io.StringIO()
            script = "import os; assert 'QONVERSION_PROJECT_KEY' not in os.environ; print(" + repr(self.KEY) + ")"
            try:
                with contextlib.redirect_stdout(output):
                    runner.command([sys.executable, "-B", "-c", script],
                                   env=dict(os.environ, QONVERSION_PROJECT_KEY=self.KEY))
            finally:
                runner.ACTIVE = previous
            self.assertNotIn(self.KEY, output.getvalue())
            self.assertIn("[REDACTED]", output.getvalue())
            artifacts.complete("FAILED", error="diagnostic-only regression")
            self.assertNotIn(self.KEY, (artifacts.safe / "diagnostics.txt").read_text())
            self.assertIn(self.KEY, next((artifacts.raw / "logs").iterdir()).read_text())

    def test_result_codes_and_machine_readable_statuses_are_distinct(self):
        # Exercise only the status serializer, not a native test or SDK response.
        for status, expected in (("PASSED", 0), ("FAILED", 1), ("SKIPPED", 77)):
            with self.subTest(status=status):
                output = io.StringIO()
                with contextlib.redirect_stdout(output):
                    code = runner.emit_result(status)
                result = json.loads(output.getvalue().split("NATIVE_TEST_RESULT ")[-1])
                self.assertEqual((code, result["exit_code"], result["status"]), (expected, expected, status))

    def test_artifact_safety_failure_never_returns_an_upload_path_or_pass(self):
        with tempfile.TemporaryDirectory() as folder:
            artifacts = runner.Artifacts(Path(folder), self.KEY)
            (artifacts.safe / "forbidden.so").write_bytes(b"binary")
            output = io.StringIO()
            with contextlib.redirect_stdout(output), contextlib.redirect_stderr(io.StringIO()):
                code = runner.emit_result("PASSED", artifacts=artifacts)
            result = json.loads(output.getvalue().split("NATIVE_TEST_RESULT ")[-1])
            self.assertEqual((code, result["status"]), (1, "FAILED"))
            self.assertNotIn("safe_artifacts", result)


class AdditionalResultRegressionTests(unittest.TestCase):
    def test_junit_rejects_unexecuted_status_and_result_attributes(self):
        with tempfile.TemporaryDirectory() as folder:
            reports = Path(folder)
            path = reports / "report.xml"
            for attribute in ("status", "result"):
                for state in ("notrun", "disabled", "skipped", "failed", "unknown"):
                    path.write_text('<testsuite tests="1" failures="0" errors="0" skipped="0">'
                                    '<testcase classname="io.dioxus.qonversion.nativetest.RemoteConfigTest" '
                                    'name="realRemoteConfig" ' + attribute + '="' + state + '"/></testsuite>')
                    with self.subTest(attribute=attribute, state=state), self.assertRaises(RuntimeError):
                        runner.require_junit_success(reports, 0)

    def test_junit_accepts_explicit_executed_success_attributes(self):
        with tempfile.TemporaryDirectory() as folder:
            reports = Path(folder)
            (reports / "report.xml").write_text(
                '<testsuite tests="1" failures="0" errors="0" skipped="0">'
                '<testcase classname="io.dioxus.qonversion.nativetest.RemoteConfigTest" '
                'name="realRemoteConfig" status="run" result="completed"/></testsuite>')
            runner.require_junit_success(reports, 0)

    def test_xctest_rejects_unexecuted_status_even_with_passing_counts(self):
        for status in ("NotRun", "Skipped", "Disabled", "Running", "Failed"):
            with self.subTest(status=status), self.assertRaises(RuntimeError):
                runner.require_xctest_success(dict(totalTestCount=1, passedTests=1, failedTests=0,
                                                  skippedTests=0, testResult=status))


class IosLinkRegressionTests(unittest.TestCase):
    def test_native_static_libraries_from_remote_diagnostic(self):
        flags = "-lSystem -lobjc -framework Foundation -lobjc -liconv -lSystem -lc -lm"
        for prefix in ("note: ", "    note: ", "2026-11-05T19:15:08.8755070Z   | note: ",
                       "\x1b[1m\x1b[92mnote\x1b[0m: "):
            with self.subTest(prefix=prefix):
                self.assertEqual(runner.rust_native_libraries(
                    f"Compiling test\n{prefix}native-static-libs: {flags}\r\nFinished\n"
                ), flags)


    # Archive-format fixtures validate build inputs, never simulate an SDK reply.
    def inputs(self, folder, data=None, output="note: native-static-libs: -lSystem -lobjc -framework Foundation"):
        previous = runner.ACTIVE
        runner.ACTIVE = type("Build", (), {"raw": Path(folder)})()
        library = Path(folder) / "target" / "aarch64-apple-ios-sim" / "debug" / "libnative_remote_config_test.a"
        if data is not None:
            library.parent.mkdir(parents=True)
            library.write_bytes(data)
        try:
            return runner.ios_link_inputs("aarch64-apple-ios-sim", output)
        finally:
            runner.ACTIVE = previous

    def test_missing_or_wrong_rust_archive_is_rejected_before_xcode(self):
        for data in (None, b"not a static archive", b"!<arch>\n"):
            with self.subTest(data=data), tempfile.TemporaryDirectory() as folder:
                with self.assertRaises(RuntimeError):
                    self.inputs(folder, data)

    def test_existing_archive_uses_the_same_absolute_path_as_cargo_target_dir(self):
        with tempfile.TemporaryDirectory(prefix="qonversion path with spaces ") as folder:
            library, libraries = self.inputs(folder, b"!<arch>\narchive-format-fixture")
            self.assertEqual(library, (Path(folder) / "target/aarch64-apple-ios-sim/debug/libnative_remote_config_test.a").resolve())
            self.assertEqual(libraries, "-lSystem -lobjc -framework Foundation")

    def test_missing_native_dependencies_are_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaises(RuntimeError):
                self.inputs(folder, b"!<arch>\narchive-format-fixture", output="Finished dev profile")


if __name__ == "__main__":
    unittest.main()
