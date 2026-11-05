"""CI adapter: run the existing SDK runner and stage only validated text."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

import run as runner


def stage_result(record, code, root, key, destination):
    if not isinstance(record, dict) or record.get("status") not in ("PASSED", "FAILED"):
        raise RuntimeError("missing, skipped or invalid native result")
    expected = runner.EXIT_CODES[record["status"]]
    if type(record.get("exit_code")) is not int or record["exit_code"] != expected or code != expected:
        raise RuntimeError("native exit code and result disagree")
    value = record.get("safe_artifacts")
    if not isinstance(value, str) or not value:
        raise RuntimeError("native runner did not authorize artifacts")
    safe = Path(value)
    base = root.resolve() / "artifacts"
    if not safe.is_absolute() or ".." in safe.parts or base.is_symlink():
        raise RuntimeError("unsafe native artifact path")
    try:
        relative = safe.relative_to(base)
    except ValueError:
        raise RuntimeError("native artifacts are outside the allowed root") from None
    if len(relative.parts) != 2 or not relative.parts[0].startswith("run-") or relative.parts[1] != "safe":
        raise RuntimeError("only a run-specific safe directory is allowed")
    if safe.parent.is_symlink() or safe.is_symlink() or safe.resolve() != safe:
        raise RuntimeError("linked artifact directories are forbidden")
    runner.Artifacts.validate_publication(safe, key)
    if {p.name for p in safe.iterdir()} != runner.Artifacts.ALLOWED:
        raise RuntimeError("required sanitized reports are absent")
    if json.loads((safe / "result.json").read_text(encoding="utf-8")) != record:
        raise RuntimeError("native console and report disagree")
    # Copy exactly two validated files. No globs, caches, binaries or raw reports.
    destination.mkdir(parents=True, exist_ok=False, mode=0o700)
    for name in sorted(runner.Artifacts.ALLOWED):
        shutil.copyfile(safe / name, destination / name)
    runner.Artifacts.validate_publication(destination, key)
    if json.loads((destination / "result.json").read_text(encoding="utf-8")) != record:
        raise RuntimeError("staged native result changed; upload forbidden")
    return expected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("platform", choices=["ios", "android"])
    parser.add_argument("--device", required=True)
    args = parser.parse_args()
    key = os.environ.get("QONVERSION_PROJECT_KEY", "").strip()
    try:
        if not key:
            raise RuntimeError("QONVERSION_PROJECT_KEY is required from Environment test")
        temporary = Path(os.environ["RUNNER_TEMP"]).resolve()
        result = subprocess.run([sys.executable, "-B", str(runner.ROOT / "run.py"),
                                 args.platform, "--run", "--device", args.device],
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                text=True, errors="replace", timeout=4200)
        print(runner.sanitize(result.stdout, key), end="", flush=True)
        lines = [line.removeprefix("NATIVE_TEST_RESULT ") for line in result.stdout.splitlines()
                 if line.startswith("NATIVE_TEST_RESULT ")]
        if len(lines) != 1:
            raise RuntimeError("expected exactly one final native result")
        record = json.loads(lines[0])
        destination = temporary / "native-remote-config-publish" / args.platform
        code = stage_result(record, result.returncode, runner.ROOT, key, destination)
        output = os.environ.get("GITHUB_OUTPUT")
        if output:
            with open(output, "a", encoding="utf-8") as stream:
                stream.write(f"safe_artifacts={destination}\n")
        # FAILED remains a failed job even when sanitized evidence is available.
        return code
    except subprocess.TimeoutExpired:
        print("Native runner wall timeout; no artifacts authorized", file=sys.stderr)
        return 1
    except (RuntimeError, OSError, ValueError, KeyError) as error:
        print(runner.sanitize("CI FAILED: " + str(error), key), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
