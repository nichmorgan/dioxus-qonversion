"""Opt-in build/runner only. No HTTP client, SDK substitute, or UI automation."""
import argparse
import base64
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import subprocess
import sys
import time
import tempfile
from urllib.parse import quote, quote_plus
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parent


# Exit codes form the public automation contract. A skip is never a pass.
EXIT_CODES = {"PASSED": 0, "FAILED": 1, "SKIPPED": 77}
ACTIVE = None


def secret_forms(key):
    if not key:
        return set()
    return {key, json.dumps(key, ensure_ascii=True)[1:-1], quote(key, safe=""),
            quote_plus(key, safe=""), base64.b64encode(key.encode()).decode(),
            base64.urlsafe_b64encode(key.encode()).decode()}


def sanitize(text, key):
    # Strip terminal escapes before matching (including escapes inside a key).
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text)
    for value in sorted(secret_forms(key), key=len, reverse=True):
        text = text.replace(value, "[REDACTED]")
    return re.sub(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]", "", text)


class Artifacts:
    """Never copy raw reports/binaries into safe/. Only two text files qualify."""
    ALLOWED = {"diagnostics.txt", "result.json"}

    def __init__(self, base, key):
        self.key = key
        self.raw = base / "raw"
        self.safe = base / "safe"
        self.raw.mkdir(parents=True, mode=0o700)
        (self.raw / "tmp").mkdir(mode=0o700)
        self.safe.mkdir(mode=0o700)
        self.index = 0
        (self.safe / "diagnostics.txt").write_text("", encoding="utf-8")

    def capture(self, text):
        self.index += 1
        logs = self.raw / "logs"
        logs.mkdir(exist_ok=True, mode=0o700)
        (logs / f"command-{self.index}.txt").write_text(text, encoding="utf-8")
        with (self.safe / "diagnostics.txt").open("a", encoding="utf-8") as stream:
            stream.write(sanitize(text, self.key) + "\n")

    def validate_safe(self):
        if self.safe.is_symlink():
            raise RuntimeError("unsafe diagnostic directory; publication forbidden")
        for path in self.safe.iterdir():
            if path.name not in self.ALLOWED or path.is_symlink() or not path.is_file():
                raise RuntimeError("unapproved artifact; publication forbidden")
            text = path.read_bytes().decode("utf-8", errors="strict")
            if "\0" in text or any(value in text for value in secret_forms(self.key)):
                raise RuntimeError("unsanitized artifact; publication forbidden")

    @classmethod
    def validate_publication(cls, safe, key):
        # Validate an existing run without creating or overwriting artifacts.
        validator = cls.__new__(cls)
        validator.safe, validator.key = Path(safe), key
        validator.validate_safe()

    def complete(self, status, **details):
        self.validate_safe()
        record = dict(status=status, exit_code=EXIT_CODES[status], **details)
        record["safe_artifacts"] = str(self.safe.resolve())
        content = sanitize(json.dumps(record, ensure_ascii=False, indent=2), self.key)
        (self.safe / "result.json").write_text(content + "\n", encoding="utf-8")
        self.validate_safe()
        return json.loads(content)


def child_environment(args, environment):
    clean = dict(environment)
    if ACTIVE:
        for variable in ("TMPDIR", "TMP", "TEMP"):
            clean[variable] = str((ACTIVE.raw / "tmp").resolve())
    if Path(str(args[0])).stem != "cargo":
        clean.pop("QONVERSION_PROJECT_KEY", None)
    return clean


def command(args, *, cwd=ROOT, env=None, timeout=1200):
    print("Running: " + Path(str(args[0])).name, flush=True)
    clean = child_environment(args, os.environ if env is None else env)
    key = ACTIVE.key if ACTIVE else os.environ.get("QONVERSION_PROJECT_KEY", "").strip()
    try:
        result = subprocess.run(args, cwd=cwd, env=clean, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, text=True, errors="replace", timeout=timeout)
    except subprocess.TimeoutExpired as error:
        # Timeout partial output must use exactly the same sanitization path.
        partial = error.stdout or ""
        if isinstance(partial, bytes):
            partial = partial.decode("utf-8", errors="replace")
        if ACTIVE:
            ACTIVE.capture(partial)
        print(sanitize(partial, key), end="", flush=True)
        raise RuntimeError("native test command exceeded wall timeout (no success recorded)") from None
    if ACTIVE:
        ACTIVE.capture(result.stdout)
    output = sanitize(result.stdout, key)
    print(output, end="", flush=True)
    if result.returncode:
        raise RuntimeError(f"{Path(str(args[0])).name} failed: exit {result.returncode}")
    return output


def build(target):
    env = dict(os.environ, CARGO_BUILD_JOBS="1", CARGO_TARGET_DIR=str(ACTIVE.raw / "target"))
    return command(["cargo", "rustc", "--manifest-path", str(ROOT / "Cargo.toml"),
                    "--lib", "--features", "native-sdk-test", "--target", target,
                    "--", "--print", "native-static-libs"], env=env)


def enable_xctest(plan):
    """Enable exactly the one generated XCTest target; never add credentials."""
    configured = 0
    # Xcode emits either legacy or version-2 xctestrun structure.
    def enable(node):
        nonlocal configured
        if isinstance(node, dict):
            if "TestBundlePath" in node:
                node.setdefault("EnvironmentVariables", {})["QONVERSION_RUN_NATIVE_REMOTE_CONFIG"] = "1"
                configured += 1
            for value in node.values():
                enable(value)
        elif isinstance(node, list):
            for value in node:
                enable(value)
    enable(plan)
    if configured != 1:
        raise RuntimeError("expected exactly one XCTest target")
    return plan


def require_xctest_success(summary):
    expected = dict(totalTestCount=1, passedTests=1, failedTests=0, skippedTests=0)
    if (not isinstance(summary, dict)
            or any(type(summary.get(key)) is not int or summary[key] != value
                   for key, value in expected.items())
            or type(summary.get("expectedFailures", 0)) is not int
            or summary.get("expectedFailures", 0) != 0
            or summary.get("testFailures", []) != []
            or summary.get("testResult", "Passed") != "Passed"):
        raise RuntimeError("XCTest must report exactly one passed test, zero failures and zero skips")


def require_junit_success(reports, started):
    cases = []
    for report in reports.rglob("*.xml"):
        # Fail closed rather than accept a previous run within a tolerance window.
        if report.stat().st_mtime < started:
            raise RuntimeError("stale Android JUnit report; no success recorded")
        root = ET.parse(report).getroot()
        if root.tag not in ("testsuite", "testsuites"):
            raise RuntimeError("unexpected Android JUnit report format")
        for node in root.iter():
            if node.tag in ("skipped", "failure", "error"):
                raise RuntimeError("Android JUnit reports a failure, error or skipped test")
            if node.tag in ("testsuite", "testsuites"):
                # Some JUnit writers omit counters; when present they must agree
                # with the sole passing testcase, including suite-level errors.
                for field, expected in (("tests", 1), ("failures", 0), ("errors", 0),
                                        ("skipped", 0), ("disabled", 0)):
                    if field in node.attrib and node.get(field) != str(expected):
                        raise RuntimeError("inconsistent Android JUnit suite counters")
        for case in root.iter("testcase"):
            # JUnit dialects may use attributes instead of a <skipped> child.
            # Missing attributes are valid in Gradle JUnit XML; declared values
            # must positively describe an executed/passing test.
            allowed = {"status": {"run", "passed"}, "result": {"completed", "passed"}}
            for attribute, values in allowed.items():
                if attribute in case.attrib and case.get(attribute).strip().lower() not in values:
                    raise RuntimeError("Android JUnit case was not executed and passed")
            cases.append(case)
    if (len(cases) != 1
            or cases[0].get("classname") != "io.dioxus.qonversion.nativetest.RemoteConfigTest"
            or cases[0].get("name") != "realRemoteConfig"):
        raise RuntimeError("Android JUnit must report exactly one passed contract test, zero failures and zero skips")


def ios_link_inputs(target, output):
    library = (ACTIVE.raw / "target" / target / "debug" / "libnative_remote_config_test.a").resolve()
    if not library.is_file():
        raise RuntimeError("Cargo did not produce the required Rust static library: " + str(library))
    with library.open("rb") as stream:
        header = stream.read(8)
    if header != b"!<arch>\n" or library.stat().st_size <= 8:
        raise RuntimeError("Rust linker input is not a nonempty static archive: " + str(library))
    libraries = re.findall(r"native-static-libs:\s*(.+)", output)
    if not libraries:
        raise RuntimeError("cargo did not report native-static-libs; refusing to guess linker flags")
    return library, libraries[-1].strip()


def ios(args):
    if platform.system() != "Darwin":
        raise RuntimeError("iOS runner requires macOS, Xcode, XcodeGen and a booted Simulator")
    if not args.device:
        raise RuntimeError("--device must be the booted iOS Simulator UDID")
    target = "aarch64-apple-ios-sim" if platform.machine() == "arm64" else "x86_64-apple-ios"
    output = build(target)
    library, native_libraries = ios_link_inputs(target, output)
    architecture = "arm64" if target == "aarch64-apple-ios-sim" else "x86_64"
    command(["xcrun", "lipo", "-verify_arch", architecture, str(library)])
    diagnostic = sanitize(f"Rust static archive verified: {library} ({library.stat().st_size} bytes)\n", ACTIVE.key)
    ACTIVE.capture(diagnostic)
    print(diagnostic, end="", flush=True)
    command(["xcodegen", "generate"], cwd=ROOT / "ios",
            env=dict(os.environ, RUST_LIBRARY=str(library)))
    derived = ACTIVE.raw / "ios" / "DerivedData"
    common = ["xcodebuild", "-project", "NativeRemoteConfig.xcodeproj", "-scheme", "NativeRemoteConfig",
              "-destination", f"platform=iOS Simulator,id={args.device}", "-derivedDataPath", str(derived)]
    command(common + ["build-for-testing", "RUST_NATIVE_LIBS=" + native_libraries], cwd=ROOT / "ios")
    plans = list((derived / "Build" / "Products").glob("*.xctestrun"))
    if len(plans) != 1:
        raise RuntimeError("expected exactly one generated xctestrun file")
    with plans[0].open("rb") as stream:
        plan = plistlib.load(stream)
    enable_xctest(plan)
    with plans[0].open("wb") as stream:
        plistlib.dump(plan, stream)
    result_bundle = derived / f"NativeRemoteConfig-{time.time_ns()}.xcresult"
    command(["xcodebuild", "test-without-building", "-xctestrun", str(plans[0]),
             "-destination", f"platform=iOS Simulator,id={args.device}",
             "-parallel-testing-enabled", "NO",
             "-only-testing:NativeRemoteConfigTests/RemoteConfigTests/testRealRemoteConfig",
             "-resultBundlePath", str(result_bundle)],
            cwd=ROOT / "ios", timeout=180)
    summary = json.loads(command(["xcrun", "xcresulttool", "get", "test-results", "summary",
                                  "--path", str(result_bundle)]))
    require_xctest_success(summary)


def android(args):
    if not args.device:
        raise RuntimeError("--device must be an Android Emulator/device serial")
    abi = command(["adb", "-s", args.device, "shell", "getprop", "ro.product.cpu.abi"]).strip()
    targets = {"arm64-v8a": "aarch64-linux-android", "x86_64": "x86_64-linux-android"}
    if abi not in targets:
        raise RuntimeError("supported Android ABIs: arm64-v8a, x86_64")
    target = targets[abi]
    ndk = os.environ.get("ANDROID_NDK_HOME")
    if not ndk:
        raise RuntimeError("set ANDROID_NDK_HOME to an installed NDK")
    hosts = {"Windows": "windows-x86_64", "Darwin": "darwin-x86_64", "Linux": "linux-x86_64"}
    toolchain = Path(ndk) / "toolchains" / "llvm" / "prebuilt" / hosts[platform.system()] / "bin"
    linker = toolchain / (target + "24-clang" +
                          (".cmd" if os.name == "nt" else ""))
    # Rust honors the target-specific linker; no global SDKROOT or linker flags.
    os.environ["CARGO_TARGET_" + target.upper().replace("-", "_") + "_LINKER"] = str(linker)
    build(target)
    destination = ACTIVE.raw / "jniLibs" / abi
    destination.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ACTIVE.raw / "target" / target / "debug" / "libnative_remote_config_test.so", destination)
    gradle = shutil.which(os.environ.get("GRADLE", "gradle"))
    if not gradle:
        raise RuntimeError("Gradle 8.6 is required; add it to PATH or set GRADLE to its executable")
    env = dict(os.environ, ANDROID_SERIAL=args.device,
               QONVERSION_TEST_RAW_DIR=str(ACTIVE.raw.resolve()),
               GRADLE_USER_HOME=str(ACTIVE.raw / "gradle-user-home"))
    started = time.time()
    command([gradle, "--no-daemon", "--console=plain", "--max-workers=1", ":app:connectedDebugAndroidTest",
             "-PrunNativeRemoteConfig=true", "--rerun-tasks",
             "--project-cache-dir", str(ACTIVE.raw / "gradle-project-cache")], cwd=ROOT / "android", env=env, timeout=900)
    reports = ACTIVE.raw / "gradle-build" / "app" / "outputs" / "androidTest-results" / "connected"
    require_junit_success(reports, started)


def emit_result(status, *, error=None, artifacts=None, elapsed=0):
    key = os.environ.get("QONVERSION_PROJECT_KEY", "").strip()
    details = {"elapsed_seconds": round(elapsed, 3)}
    if error:
        details["error"] = sanitize(str(error), key)
    record = dict(status=status, exit_code=EXIT_CODES[status], **details)
    if artifacts:
        try:
            record = artifacts.complete(status, **details)
        except (OSError, ValueError, RuntimeError):
            # Do not expose an upload path when final safety validation fails.
            status = "FAILED"
            record = dict(status=status, exit_code=1, error="artifact safety validation failed; publication forbidden")
    if "error" in record:
        print(record["error"], file=sys.stderr)
    print("NATIVE_TEST_RESULT " + json.dumps(record, ensure_ascii=True), flush=True)
    return EXIT_CODES[status]


class Parser(argparse.ArgumentParser):
    def error(self, message):
        raise ValueError(message)


def main(argv=None):
    global ACTIVE
    parser = Parser(description=__doc__)
    parser.add_argument("platform", choices=["ios", "android"])
    parser.add_argument("--run", action="store_true", help="explicitly authorize the real native SDK test")
    parser.add_argument("--device", help="booted Simulator UDID or Android serial")
    started = time.monotonic()
    artifacts = None
    try:
        args = parser.parse_args(argv)
        if not args.run:
            return emit_result("SKIPPED")
        key = os.environ.get("QONVERSION_PROJECT_KEY", "").strip()
        if not key:
            raise ValueError("QONVERSION_PROJECT_KEY must be supplied externally (SDK Project Key only)")
        os.environ["QONVERSION_PROJECT_KEY"] = key
        directory = ROOT / "artifacts"
        directory.mkdir(exist_ok=True, mode=0o700)
        artifacts = Artifacts(Path(tempfile.mkdtemp(prefix="run-", dir=directory)), key)
        ACTIVE = artifacts
        {"ios": ios, "android": android}[args.platform](args)
    except (RuntimeError, OSError, ValueError, ET.ParseError) as error:
        return emit_result("FAILED", error=error, artifacts=artifacts, elapsed=time.monotonic() - started)
    finally:
        ACTIVE = None
    # Reached only after the native runner validated exactly one passing test.
    return emit_result("PASSED", artifacts=artifacts, elapsed=time.monotonic() - started)


if __name__ == "__main__":
    sys.exit(main())
