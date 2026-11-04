"""Manual iOS CI helper. Only sanitized diagnostics, never app/build output, are uploaded."""

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
CONFIG = ROOT / ".github/ios-remote-config/Dioxus.toml"
TEMP = Path(os.environ.get("RUNNER_TEMP", os.environ.get("TEMP", "/tmp")))
SAFE = TEMP / "ios-remote-config-safe"
RAW = TEMP / "ios-remote-config-raw"
TEXT_SUFFIXES = {".log", ".txt", ".xml", ".json", ".yaml", ".yml", ".html", ".csv", ".ips", ".crash"}
CALLBACKS = ("dioxus_qonversion_notify_screen_failed", "dioxus_qonversion_notify_screen_event")


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


def run(command, log, *, check=True, with_key=False):
    """Scrub before writing to either the Actions console or the artifact folder."""
    path = SAFE / log
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as output:
        with subprocess.Popen(command, cwd=ROOT, env=child_env(with_key),
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                              text=True, encoding="utf-8", errors="replace") as process:
            for line in process.stdout:
                clean = redact(line)
                output.write(clean)
                print(clean, end="", flush=True)
            code = process.wait()
    if check and code:
        raise RuntimeError(f"{command[0]} failed with exit code {code}; see {log}")
    return code


def capture(command):
    result = subprocess.run(command, cwd=ROOT, env=child_env(), capture_output=True,
                            text=True, encoding="utf-8", errors="replace")
    if result.returncode:
        raise RuntimeError(redact(f"{command[0]} failed with exit code {result.returncode}: {result.stderr or result.stdout}"))
    return result.stdout


def bundle_id():
    return tomllib.loads(CONFIG.read_text(encoding="utf-8"))["bundle"]["identifier"]


def find_app():
    matches = []
    target = ROOT / "target/dx"
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
    matches = [device for group in devices["devices"].values() for device in group
               if device.get("state") == "Booted" and device.get("isAvailable", True)]
    if len(matches) != 1:
        raise RuntimeError(f"Expected exactly one booted simulator, found {len(matches)}")
    return matches[0]["udid"]


def preflight():
    key = os.environ.get("QONVERSION_PROJECT_KEY", "").strip()
    if not key or key.startswith("sk_"):
        raise RuntimeError("Set the SDK Project Key for Jusfine - Rust Tests in the QONVERSION_PROJECT_KEY Actions secret; do not use an administrative Secret Key")
    save("environment.txt", f"Python {platform.python_version()}\nArchitecture: {platform.machine()}\nBundle ID: {bundle_id()}\n")
    run(["xcodebuild", "-version"], "xcode-version.log")
    run(["xcrun", "swift", "--version"], "swift-version.log")


def build():
    # The current example reads option_env! at compile time. Only dx receives
    # the key; raw compiler environment captures stay on the ephemeral runner.
    config = ROOT / "Dioxus.toml"
    previous = config.read_bytes() if config.exists() else None
    try:
        config.write_bytes(CONFIG.read_bytes())
        run(["dx", "build", "--example", "demo", "--platform", "ios",
             "--target", os.environ["IOS_TARGET"]], "build.log", with_key=True)
    finally:
        if previous is None:
            config.unlink(missing_ok=True)
        else:
            config.write_bytes(previous)


def inspect_app():
    app, info = find_app()
    binary = app / info["CFBundleExecutable"]
    save("app-info.json", json.dumps({"bundle_id": info["CFBundleIdentifier"],
         "executable": info["CFBundleExecutable"], "minimum_os": info.get("MinimumOSVersion")}, indent=2))
    run(["file", str(binary)], "app-architecture.log")
    run(["xcrun", "otool", "-L", str(binary)], "app-dependencies.log")
    frameworks = [app / "Frameworks" / f"{name}.framework" / name
                  for name in ("DioxusSwiftPlugins", "DioxusQonversionHost")]
    frameworks = [path for path in frameworks if path.is_file()]
    if not frameworks:
        raise RuntimeError("Host framework missing from app/Frameworks")
    # Diagnose native exports without dumping the binary or strings (contains key).
    exports = capture(["xcrun", "nm", "-gU", str(binary)])
    diagnostics = [f"{name}: {'present in global symbols' if name in exports else 'NOT FOUND in global symbols'}"
                   for name in CALLBACKS]
    diagnostics.append("Global symbols alone do not prove dyld resolution; check native runtime logs.")
    save("rust-callback-symbols.txt", "\n".join(diagnostics))
    host_found = False
    for path in frameworks:
        symbols = capture(["xcrun", "nm", "-gU", str(path)])
        host_found |= "OBJC_CLASS_$_DioxusQonversionHost" in symbols
        save(f"{path.name}-host-symbols.txt", "\n".join(
            line for line in symbols.splitlines() if "DioxusQonversionHost" in line))
        imports = capture(["xcrun", "nm", "-u", str(path)])
        save(f"{path.name}-callback-imports.txt", "\n".join(
            line for line in imports.splitlines() if any(name in line for name in CALLBACKS)))
        run(["xcrun", "otool", "-L", str(path)], f"{path.name}-dependencies.log")
        run(["file", str(path)], f"{path.name}-architecture.log")
    if not host_found:
        raise RuntimeError("Framework exists but DioxusQonversionHost class symbol is absent")


def install():
    app, _ = find_app()
    run(["xcrun", "simctl", "install", simulator(), str(app)], "install.log")


def test():
    _, info = find_app()
    udid = simulator()
    RAW.mkdir(parents=True, exist_ok=True)
    predicate = f'process == "{info["CFBundleExecutable"]}"'
    stream = subprocess.Popen(["xcrun", "simctl", "spawn", udid, "log", "stream",
                               "--level", "debug", "--style", "compact", "--predicate", predicate],
                              env=child_env(), stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                              text=True, encoding="utf-8", errors="replace")

    def drain():
        with (SAFE / "native-stream.log").open("w", encoding="utf-8") as output:
            for line in stream.stdout:
                output.write(redact(line))
                output.flush()

    reader = threading.Thread(target=drain, daemon=True)
    reader.start()
    try:
        run(["maestro", "test", "--format", "junit", "--output", str(RAW / "maestro-report.xml"),
             "--test-output-dir", str(RAW / "maestro"), "--debug-output", str(RAW / "maestro"),
             ".maestro"], "maestro-console.log")
    finally:
        stream.terminate()
        try:
            stream.wait(timeout=10)
        except subprocess.TimeoutExpired:
            stream.kill()
            stream.wait()
        reader.join(timeout=10)
        if not reader.is_alive():
            stream.stdout.close()


def publish_safe_files():
    """Allow-list diagnostic formats; never publish app, target, archives or env captures."""
    copied = []
    if RAW.exists():
        for source in RAW.rglob("*"):
            if not source.is_file() or source.is_symlink():
                continue
            relative_source = source.relative_to(RAW)
            if any(part.endswith((".app", ".framework")) or part == "target"
                   for part in relative_source.parts):
                continue
            suffix = source.suffix.lower()
            relative = Path(redact(str(relative_source)))
            destination = SAFE / "maestro" / relative
            if suffix in TEXT_SUFFIXES:
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_text(redact(source.read_text(encoding="utf-8", errors="replace")), encoding="utf-8")
            elif suffix == ".png":
                data = source.read_bytes()
                key = os.environ.get("QONVERSION_PROJECT_KEY", "").strip().encode()
                if key and key in data:
                    raise RuntimeError("Screenshot contains literal key bytes; artifact rejected")
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(data)
            else:
                continue
            copied.append(str(destination.relative_to(SAFE)))
    save("artifact-manifest.json", json.dumps(copied, indent=2))


def collect():
    # Runs on failure as well, including builds that never produced an app.
    try:
        udid = simulator()
        RAW.mkdir(parents=True, exist_ok=True)
        run(["xcrun", "simctl", "io", udid, "screenshot", str(RAW / "final-screen.png")],
            "screenshot.log", check=False)
        try:
            _, info = find_app()
            name = info["CFBundleExecutable"]
            run(["xcrun", "simctl", "spawn", udid, "log", "show", "--last", "15m",
                 "--style", "compact", "--predicate", f'process == "{name}"'],
                "native-history.log", check=False)
            crash_dirs = [Path.home() / "Library/Logs/DiagnosticReports",
                          Path.home() / f"Library/Developer/CoreSimulator/Devices/{udid}/data/Library/Logs/CrashReporter"]
            for index, directory in enumerate(crash_dirs):
                if directory.exists():
                    for path in directory.glob(f"{name}*"):
                        if path.is_file() and path.suffix in {".ips", ".crash"}:
                            save(f"crashes/{index}-{path.name}", path.read_text(encoding="utf-8", errors="replace"))
        except Exception as error:
            save("native-collection-warning.txt", str(error))
    except Exception as error:
        save("simulator-collection-warning.txt", str(error))
    publish_safe_files()
    # Audit text outputs after redaction, including reports and crash diagnostics.
    key = os.environ.get("QONVERSION_PROJECT_KEY", "").strip()
    if key:
        for path in SAFE.rglob("*"):
            if path.is_file() and path.suffix in TEXT_SUFFIXES and key in path.read_text(encoding="utf-8", errors="replace"):
                path.unlink()
                raise RuntimeError("Unsanitized diagnostic removed; do not upload raw artifacts")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["preflight", "build", "inspect", "install", "test", "collect"])
    args = parser.parse_args()
    SAFE.mkdir(parents=True, exist_ok=True)
    try:
        if sys.platform != "darwin":
            raise RuntimeError("This CI helper requires macOS/Xcode; local Windows validation must not run iOS commands")
        {"preflight": preflight, "build": build, "inspect": inspect_app,
         "install": install, "test": test, "collect": collect}[args.command]()
    except Exception as error:
        message = redact(str(error))
        save(f"{args.command}-error.txt", message)
        print(message, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
