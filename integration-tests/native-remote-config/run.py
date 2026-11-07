"""Verify the Android Remote Config report and print sanitized logs."""

import argparse
import base64
import json
import os
from pathlib import Path
import re
import sys
from urllib.parse import quote, quote_plus
import xml.etree.ElementTree as ET


def secret_forms(key):
    if not key:
        return set()
    return {
        key,
        json.dumps(key, ensure_ascii=True)[1:-1],
        quote(key, safe=""),
        quote_plus(key, safe=""),
        base64.b64encode(key.encode()).decode(),
        base64.urlsafe_b64encode(key.encode()).decode(),
    }


def sanitize(text, key):
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text)
    for value in sorted(secret_forms(key), key=len, reverse=True):
        text = text.replace(value, "[REDACTED]")
    return re.sub(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]", "", text)


def require_junit_success(reports, started):
    cases = []
    for report in reports.rglob("*.xml"):
        if report.stat().st_mtime < started:
            raise RuntimeError("stale Android JUnit report; no success recorded")
        root = ET.parse(report).getroot()
        if root.tag not in ("testsuite", "testsuites"):
            raise RuntimeError("unexpected Android JUnit report format")
        for node in root.iter():
            if node.tag in ("skipped", "failure", "error"):
                raise RuntimeError("Android JUnit reports a failure, error or skipped test")
            if node.tag in ("testsuite", "testsuites"):
                for field, expected in (
                    ("tests", 1),
                    ("failures", 0),
                    ("errors", 0),
                    ("skipped", 0),
                    ("disabled", 0),
                ):
                    if field in node.attrib and node.get(field) != str(expected):
                        raise RuntimeError("inconsistent Android JUnit suite counters")
        for case in root.iter("testcase"):
            allowed = {"status": {"run", "passed"}, "result": {"completed", "passed"}}
            for attribute, values in allowed.items():
                if attribute in case.attrib and case.get(attribute).strip().lower() not in values:
                    raise RuntimeError("Android JUnit case was not executed and passed")
            cases.append(case)
    if (
        len(cases) != 1
        or cases[0].get("classname") != "io.dioxus.qonversion.nativetest.RemoteConfigTest"
        or cases[0].get("name") != "realRemoteConfig"
    ):
        raise RuntimeError(
            "Android JUnit must report exactly one passed contract test, zero failures and zero skips"
        )


def logs(directory):
    key = os.environ.get("QONVERSION_PROJECT_KEY", "").strip()
    for path in sorted(directory.glob("*.log")):
        print(f"--- {path.name} (sanitized tail) ---")
        content = sanitize(path.read_text(encoding="utf-8", errors="replace"), key)
        for line in content[-12000:].splitlines():
            print("  | " + line)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["verify-android", "logs"])
    parser.add_argument("path", type=Path)
    args = parser.parse_args(argv)
    try:
        if args.command == "verify-android":
            require_junit_success(args.path, 0)
            print("PASSED: one native JUnit test validated Remote Config")
        else:
            logs(args.path)
    except (OSError, RuntimeError, ET.ParseError) as error:
        print(f"Native test {args.command} failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
