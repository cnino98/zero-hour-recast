#!/usr/bin/env python3
"""Run source checks; --full also requires private game data and a visible desktop."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import sys
import uuid

sys.dont_write_bytecode = True
PROJECT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PROJECT / "tests/acceptance"))
from retention import Payloads
SCALING = "skirmish::tests::performance::simulation_scaling"
STRICT = ["--workspace", "--all-targets", "--all-features", "--locked"]


def external(path):
    resolved = Path(path).resolve()
    if resolved == PROJECT or PROJECT in resolved.parents:
        raise ValueError(f"Test output must be outside the repository: {resolved}")
    return resolved


def environment(runtime, output):
    env = os.environ.copy()
    for key, name in (("CARGO_HOME", "cargo"), ("CARGO_TARGET_DIR", "target"),
                      ("TMPDIR", "tmp"), ("TMP", "tmp"), ("TEMP", "tmp"),
                      ("XDG_CACHE_HOME", "xdg"), ("XDG_CONFIG_HOME", "config"),
                      ("XDG_DATA_HOME", "data")):
        configured = env.get(key) if key in ("CARGO_HOME", "CARGO_TARGET_DIR") else None
        path = external(configured or runtime / "cache" / name)
        path.mkdir(parents=True, exist_ok=True)
        env[key] = str(path)
    flags = env.get("RUSTFLAGS", "")
    env.update(RUSTFLAGS=(flags + " -D warnings").strip(), CARGO_INCREMENTAL="0",
               PYTHONDONTWRITEBYTECODE="1", GENERALS_TEST_OUTPUT=str(output),
               GENERALS_GAME_ROOT=str(runtime), CARGO_TERM_COLOR="never")
    return env


def scaling_complete(output):
    """Cargo can return success after a misspelled filter matched zero tests."""
    return bool(re.search(r"(?m)^test result: ok\. 1 passed; 0 failed; 0 ignored;", output))


def python_discovered(output):
    return bool(re.search(r"(?m)^Ran [1-9]\d* tests? in \S+s$", output))


def gates(full):
    test = ["cargo", "test", "--no-fail-fast", *STRICT]
    if full:
        test += ["--", "--include-ignored", "--test-threads=1"]
    return [
        ("fmt", ["cargo", "fmt", "--all", "--", "--check"]),
        ("check", ["cargo", "check", *STRICT]),
        ("clippy", ["cargo", "clippy", *STRICT, "--", "-D", "warnings", "-D", "clippy::all",
                    "-D", "clippy::pedantic", "-D", "clippy::nursery"]),
        ("test", test),
        ("linked-build", ["cargo", "build", *STRICT]),
        ("python", [sys.executable, "-B", "-m", "unittest", "discover", "-s",
                    str(PROJECT / "tests/acceptance"), "-p", "test_*.py"]),
        ("simulation-scaling", ["cargo", "test", "-p", "generals-sim", "--lib", "--all-features", "--locked",
                                SCALING, "--", "--exact", "--ignored", "--nocapture", "--test-threads=1"]),
    ]


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def export_runtime(original, destination, platform, payloads=None):
    candidate = destination / original.name
    if platform == "darwin":
        contents = destination / "Zero Hour Recast.app" / "Contents"
        candidate = contents / "MacOS" / original.name
        candidate.parent.mkdir(parents=True)
        metadata = {"CFBundleExecutable": original.name, "CFBundleIdentifier": "org.zero-hour-recast.skirmish",
                    "CFBundleName": "Zero Hour Recast", "CFBundlePackageType": "APPL",
                    "CFBundleVersion": "1", "NSHighResolutionCapable": True}
        (contents / "Info.plist").write_bytes(plistlib.dumps(metadata))
    if payloads is not None:
        payloads.add(candidate, digest(original))
    shutil.copy2(original, candidate)
    if digest(candidate) != digest(original):
        raise ValueError("Exported runtime differs from the release build")
    return candidate


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--full", action="store_true", help="Run ignored installed tests, references and every runtime acceptance case")
    parser.add_argument("--references", action="store_true", help="Also run comparisons against the external pinned source checkout")
    parser.add_argument("--runtime-root", type=Path, default=Path(os.environ.get("GENERALS_GAME_ROOT", PROJECT.parent / "private")))
    parser.add_argument("--output", type=Path, help="External test result directory; every run creates a new child")
    parser.add_argument("--executable", type=Path, help="Explicit private runtime candidate; by default export the freshly built release")
    parser.add_argument("--case-timeout", type=float, default=900, help="Private runtime acceptance timeout per case in seconds")
    args = parser.parse_args()
    if args.full and os.name != "posix":
        parser.error("Private full acceptance currently requires a POSIX desktop")
    runtime = external(args.runtime_root)
    output = external(args.output or os.environ.get("GENERALS_TEST_OUTPUT", runtime / "test-results"))
    env = environment(runtime, output)
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    destination = output / f"{stamp}-{uuid.uuid4().hex[:8]}"
    destination.mkdir(parents=True, exist_ok=False)
    payloads = Payloads(destination)
    try:
        manifest = {"full": args.full, "source": str(PROJECT), "runtime_root": str(runtime), "steps": []}

        def save():
            pending = destination / "manifest.next"
            pending.write_text(json.dumps(manifest, indent=2) + "\n")
            pending.replace(destination / "manifest.json")

        def run(name, command):
            log = destination / f"{len(manifest['steps']) + 1:02d}-{name}.log"
            print(f"test-gate: {name}; log: {log}", flush=True)
            status = 127
            with log.open("xb") as stream:
                stream.write((json.dumps(command) + "\n").encode())
                try:
                    process = subprocess.Popen(command, cwd=PROJECT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
                    with process.stdout:
                        while block := process.stdout.read1(65536):
                            stream.write(block)
                            sys.stdout.buffer.write(block)
                            sys.stdout.buffer.flush()
                    status = process.wait()
                except OSError as error:
                    stream.write(f"Launch failed: {error}\n".encode())
            if name == "simulation-scaling" and status == 0 and not scaling_complete(log.read_text(errors="replace")):
                with log.open("ab") as stream:
                    stream.write(b"FAIL: expected one completed scaling test; a zero-test match is invalid.\n")
                status = 1
            if name == "python" and status == 0 and not python_discovered(log.read_text(errors="replace")):
                with log.open("ab") as stream:
                    stream.write(b"FAIL: Python discovery must run at least one test.\n")
                status = 1
            manifest["steps"].append({"name": name, "command": command, "status": status, "log": str(log)})
            save()
            return status == 0

        save()
        for name, command in gates(args.full):
            run(name, command)
        if args.full or args.references:
            run("reference", [sys.executable, "-B", str(PROJECT / "tests/reference/run.py")])
        if args.full:
            built = run("release-build", ["cargo", "build", "--release", *STRICT])
            if args.executable is not None:
                candidate = external(args.executable)
            elif built:
                target = Path(env["CARGO_TARGET_DIR"])
                if env.get("CARGO_BUILD_TARGET"):
                    target /= env["CARGO_BUILD_TARGET"]
                original = target / "release" / ("generals-client.exe" if os.name == "nt" else "generals-client")
                candidate = export_runtime(original, destination, sys.platform, payloads)
                manifest["runtime_sha256"] = digest(candidate)
                save()
            else:
                candidate = None
            if candidate is not None:
                command = [sys.executable, "-B", str(PROJECT / "tests/acceptance/run.py"),
                           "--runtime-root", str(runtime), "--executable", str(candidate),
                           "--case-timeout", str(args.case_timeout)]
                for group in ("installed", "benchmarks", "scaling", "native", "profiles"):
                    command += ["--group", group]
                run("runtime-acceptance", command)
        failed = [step["name"] for step in manifest["steps"] if step["status"] != 0]
        print(f"Test results: {destination}; failed gates: {', '.join(failed) or 'none'}", flush=True)
        return int(bool(failed))
    finally:
        payloads.close()


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError) as error:
        print(f"Tests stopped: {error}", file=sys.stderr)
        sys.exit(1)
