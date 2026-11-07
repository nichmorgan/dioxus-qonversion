"""Report checks for the Android Remote Config job. They never load an SDK."""

import base64
from pathlib import Path
import tempfile
import unittest

import run as runner

RUNNER = Path(__file__).with_name("run.py")


class ResultTests(unittest.TestCase):
    def test_junit_rejects_suite_errors_wrong_identity_and_recent_stale_reports(self):
        with tempfile.TemporaryDirectory() as folder:
            reports = Path(folder)
            path = reports / "result.xml"
            case = '<testcase classname="io.dioxus.qonversion.nativetest.RemoteConfigTest" name="realRemoteConfig"/>'
            for xml in (
                '<testsuite failures="1">' + case + "</testsuite>",
                '<testsuite errors="1">' + case + "</testsuite>",
                '<testsuite skipped="1">' + case + "</testsuite>",
                '<testsuite tests="0">' + case + "</testsuite>",
                "<testsuite>" + case + "<error/></testsuite>",
                "<testsuite>" + case.replace("realRemoteConfig", "anotherTest") + "</testsuite>",
            ):
                path.write_text(xml)
                with self.subTest(xml=xml), self.assertRaises(RuntimeError):
                    runner.require_junit_success(reports, 0)
            path.write_text("<testsuite>" + case + "</testsuite>")
            with self.assertRaises(RuntimeError):
                runner.require_junit_success(reports, path.stat().st_mtime + 1)
            path.write_text("<invalid")
            with self.assertRaises(runner.ET.ParseError):
                runner.require_junit_success(reports, 0)

    def test_junit_requires_fresh_success_of_the_actual_contract_test(self):
        with tempfile.TemporaryDirectory() as folder:
            reports = Path(folder)
            with self.assertRaises(RuntimeError):
                runner.require_junit_success(reports, 0)
            path = reports / "result.xml"
            case = '<testcase classname="io.dioxus.qonversion.nativetest.RemoteConfigTest" name="realRemoteConfig">{}</testcase>'
            path.write_text("<testsuite>" + case.format("") + "</testsuite>")
            runner.require_junit_success(reports, 0)
            with self.assertRaises(RuntimeError):
                runner.require_junit_success(reports, path.stat().st_mtime + 10)
            for content in ("<skipped/>", "<failure/>", "<error/>"):
                path.write_text("<testsuite>" + case.format(content) + "</testsuite>")
                with self.assertRaises(RuntimeError):
                    runner.require_junit_success(reports, 0)
            path.write_text("<testsuite>" + case.format("") * 2 + "</testsuite>")
            with self.assertRaises(RuntimeError):
                runner.require_junit_success(reports, 0)

    def test_junit_rejects_unexecuted_status_and_result_attributes(self):
        with tempfile.TemporaryDirectory() as folder:
            reports = Path(folder)
            path = reports / "report.xml"
            for attribute in ("status", "result"):
                for state in ("notrun", "disabled", "skipped", "failed", "unknown"):
                    path.write_text(
                        '<testsuite tests="1" failures="0" errors="0" skipped="0">'
                        '<testcase classname="io.dioxus.qonversion.nativetest.RemoteConfigTest" '
                        'name="realRemoteConfig" ' + attribute + '="' + state + '"/></testsuite>'
                    )
                    with self.subTest(attribute=attribute, state=state), self.assertRaises(RuntimeError):
                        runner.require_junit_success(reports, 0)

    def test_junit_accepts_explicit_executed_success_attributes(self):
        with tempfile.TemporaryDirectory() as folder:
            reports = Path(folder)
            (reports / "report.xml").write_text(
                '<testsuite tests="1" failures="0" errors="0" skipped="0">'
                '<testcase classname="io.dioxus.qonversion.nativetest.RemoteConfigTest" '
                'name="realRemoteConfig" status="run" result="completed"/></testsuite>'
            )
            runner.require_junit_success(reports, 0)


class SecurityRegressionTests(unittest.TestCase):
    KEY = "regression-canary-not-a-real-project-key"

    def test_redaction_handles_console_escapes_and_common_encoded_forms(self):
        encoded = base64.b64encode(self.KEY.encode()).decode()
        text = "\x1b[31m" + self.KEY + "\x1b[0m " + encoded
        cleaned = runner.sanitize(text, self.KEY)
        self.assertNotIn(self.KEY, cleaned)
        self.assertNotIn(encoded, cleaned)
        self.assertNotIn("\x1b", cleaned)

    def test_readme_is_utf8(self):
        text = RUNNER.with_name("README.md").read_bytes().decode("utf-8")
        self.assertNotIn("\ufffd", text)


if __name__ == "__main__":
    unittest.main()
