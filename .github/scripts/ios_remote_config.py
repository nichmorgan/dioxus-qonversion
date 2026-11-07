"""Build and drive the iOS Remote Config test app. Upload only sanitized diagnostics."""

import argparse
import json
import os
from pathlib import Path
import plistlib
import platform
import subprocess
import sys
import threading
import tomllib

ROOT = Path(__file__).resolve().parents[2]
APP = ROOT / "integration-tests/remote-config-app"
CONFIG = APP / "Dioxus.toml"
TEMP = Path(os.environ.get("RUNNER_TEMP", os.environ.get("TEMP", "/tmp")))
SAFE = TEMP / "remote-config-e2e-safe"
RAW = TEMP / "remote-config-e2e-raw"
TEXT_SUFFIXES = {".log", ".txt", ".xml", ".json", ".yaml", ".yml", ".html", ".csv", ".ips", ".crash"}


def redact(text):
    key = os.environ.get("QONVERSION_PROJECT_KEY", "")
    for value in sorted({key, key.strip()} - {""}, key=len, reverse=True):
        text = text.replace(value, "[REDACTED]")
    return text


def child_env(with_key=False):
    env = os.environ.copy()
    if not with_key:
        env.pop("QONVERSION_PROJECT_KEY", None)
    return env


def save(name, text):
    path = SAFE / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(redact(text), encoding="utf-8")


def run(command, log, *, check=True, with_key=False, cwd=None):
    """Scrub before writing to the Actions console or the artifact folder."""
    path = SAFE / log
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as output:
        with subprocess.Popen(
            command,
            cwd=cwd or ROOT,
            env=child_env(with_key),
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
        ) as process:
            for line in process.stdout:
                clean = redact(line)
                output.write(clean)
                print(clean, end="", flush=True)
            code = process.wait()
    if check and code:
        raise RuntimeError(f"{command[0]} failed with exit code {code}; see {log}")
    return code


def capture(command):
    result = subprocess.run(
        command,
        cwd=ROOT,
        env=child_env(),
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    if result.returncode:
        raise RuntimeError(
            redact(f"{command[0]} failed with exit code {result.returncode}: {result.stderr or result.stdout}")
        )
    return result.stdout


def bundle_id():
    return tomllib.loads(CONFIG.read_text(encoding="utf-8"))["bundle"]["identifier"]


def find_app():
    matches = []
    target = APP / "target" / "dx"
    if target.exists():
        for path in target.rglob("Info.plist"):
            if path.parent.suffix != ".app":
                continue
            with path.open("rb") as source:
                info = plistlib.load(source)
            if info.get("CFBundleIdentifier") == bundle_id():
                matches.append((path.parent, info))
    if len(matches) != 1:
        raise RuntimeError(f"Expected one simulator app with bundle id {bundle_id()}, found {len(matches)}")
    return matches[0]


def simulator():
    devices = json.loads(capture(["xcrun", "simctl", "list", "devices", "booted", "--json"]))
    matches = [
        device
        for group in devices["devices"].values()
        for device in group
        if device.get("state") == "Booted" and device.get("isAvailable", True)
    ]
    if len(matches) != 1:
        raise RuntimeError(f"Expected exactly one booted simulator, found {len(matches)}")
    return matches[0]["udid"]


def preflight():
    key = os.environ.get("QONVERSION_PROJECT_KEY", "").strip()
    if not key or key.startswith("sk_"):
        raise RuntimeError(
            "Set the SDK Project Key in the QONVERSION_PROJECT_KEY Actions secret; do not use a Secret Key"
        )
    save(
        "environment.txt",
        f"Python {platform.python_version()}\nArchitecture: {platform.machine()}\nBundle ID: {bundle_id()}\n",
    )
    run(["xcodebuild", "-version"], "xcode-version.log")
    run(["xcrun", "swift", "--version"], "swift-version.log")


def build():
    run(
        ["dx", "build", "--platform", "ios", "--target", os.environ["IOS_TARGET"]],
        "build.log",
        with_key=True,
        cwd=APP,
    )


def inspect_app():
    app, info = find_app()
    binary = app / info["CFBundleExecutable"]
    save(
        "app-info.json",
        json.dumps(
            {
                "bundle_id": info["CFBundleIdentifier"],
                "executable": info["CFBundleExecutable"],
                "minimum_os": info.get("MinimumOSVersion"),
            },
            indent=2,
        ),
    )
    run(["file", str(binary)], "app-architecture.log")
    run(["xcrun", "otool", "-L", str(binary)], "app-dependencies.log")
    host = app / "Frameworks" / "DioxusSwiftPlugins.framework" / "DioxusSwiftPlugins"
    if not host.is_file():
        raise RuntimeError("DioxusSwiftPlugins.framework is missing from the app")
    symbols = capture(["xcrun", "nm", "-gU", str(host)])
    if "OBJC_CLASS_$_DioxusQonversionHost" not in symbols:
        raise RuntimeError("Framework exists but DioxusQonversionHost is absent")
    save(
        "host-symbols.txt",
        "\n".join(line for line in symbols.splitlines() if "DioxusQonversionHost" in line),
    )
    run(["xcrun", "otool", "-L", str(host)], "host-dependencies.log")
    run(["file", str(host)], "host-architecture.log")


def install():
    app, _ = find_app()
    run(["xcrun", "simctl", "install", simulator(), str(app)], "install.log")


def test():
    _, info = find_app()
    udid = simulator()
    RAW.mkdir(parents=True, exist_ok=True)
    predicate = f'process == "{info["CFBundleExecutable"]}"'
    stream = subprocess.Popen(
        [
            "xcrun",
            "simctl",
            "spawn",
            udid,
            "log",
            "stream",
            "--level",
            "debug",
            "--style",
            "compact",
            "--predicate",
            predicate,
        ],
        env=child_env(),
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
    )

    def drain():
        with (SAFE / "native-stream.log").open("w", encoding="utf-8") as output:
            for line in stream.stdout:
                output.write(redact(line))

    reader = threading.Thread(target=drain, daemon=True)
    reader.start()
    try:
        run(
            [
                "maestro",
                "test",
                "--format",
                "junit",
                "--output",
                str(RAW / "maestro-report.xml"),
                "--test-output-dir",
                str(RAW / "maestro"),
                "--debug-output",
                str(RAW / "maestro"),
                str(APP / "maestro"),
            ],
            "maestro-console.log",
        )
    finally:
        stream.terminate()
        try:
            stream.wait(timeout=10)
        except subprocess.TimeoutExpired:
            stream.kill()
            stream.wait()
        reader.join(timeout=10)
        if not reader.is_alive() and stream.stdout is not None:
            stream.stdout.close()


def publish_safe_files():
    """Copy allow-listed diagnostics. Do not publish the app, target, or archives."""
    copied = []
    if RAW.exists():
        for source in RAW.rglob("*"):
            if not source.is_file() or source.is_symlink():
                continue
            relative_source = source.relative_to(RAW)
            if any(part.endswith((".app", ".framework")) or part == "target" for part in relative_source.parts):
                continue
            suffix = source.suffix.lower()
            relative = Path(redact(str(relative_source)))
            destination = SAFE / "maestro" / relative
            if suffix in TEXT_SUFFIXES:
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_text(
                    redact(source.read_text(encoding="utf-8", errors="replace")),
                    encoding="utf-8",
                )
            elif suffix == ".png":
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(source.read_bytes())
            else:
                continue
            copied.append(str(destination.relative_to(SAFE)))
    save("artifact-manifest.json", json.dumps(copied, indent=2))


def collect():
    try:
        udid = simulator()
        RAW.mkdir(parents=True, exist_ok=True)
        run(
            ["xcrun", "simctl", "io", udid, "screenshot", str(RAW / "final-screen.png")],
            "screenshot.log",
            check=False,
        )
        try:
            _, info = find_app()
            name = info["CFBundleExecutable"]
            run(
                [
                    "xcrun",
                    "simctl",
                    "spawn",
                    udid,
                    "log",
                    "show",
                    "--last",
                    "15m",
                    "--style",
                    "compact",
                    "--predicate",
                    f'process == "{name}"',
                ],
                "native-history.log",
                check=False,
            )
        except (RuntimeError, OSError) as error:
            save("native-collection-warning.txt", str(error))
    except (RuntimeError, OSError, subprocess.SubprocessError) as error:
        save("simulator-collection-warning.txt", str(error))
    publish_safe_files()
    key = os.environ.get("QONVERSION_PROJECT_KEY", "").strip()
    if not key:
        return
    for path in SAFE.rglob("*"):
        if path.is_file() and path.suffix in TEXT_SUFFIXES and key in path.read_text(encoding="utf-8", errors="replace"):
            path.unlink()
            raise RuntimeError("Unsanitized diagnostic removed; do not upload raw artifacts")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["preflight", "build", "inspect", "install", "test", "collect"])
    args = parser.parse_args()
    SAFE.mkdir(parents=True, exist_ok=True)
    if sys.platform != "darwin":
        raise RuntimeError("This helper requires macOS")
    try:
        {"preflight": preflight, "build": build, "inspect": inspect_app, "install": install, "test": test, "collect": collect}[
            args.command
        ]()
    except RuntimeError as error:
        message = redact(str(error))
        save(f"{args.command}-error.txt", message)
        print(message, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
