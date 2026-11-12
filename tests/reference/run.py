#!/usr/bin/env python3
"""Run bounded comparisons against an external pinned source checkout, serially."""
import os
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
import reference_support
HERE = Path(__file__).resolve().parent


def main():
    checks = sorted(HERE.glob("check-*-reference.py"))
    if not checks:
        print(f"No reference checks found in {HERE}", file=sys.stderr)
        return 1
    reference_support.validate_checkout()
    environment = dict(os.environ, PYTHONDONTWRITEBYTECODE="1")
    failed = []
    for check in checks:
        print(f"reference-check: {check.name}", flush=True)
        result = subprocess.run([sys.executable, "-B", str(check)], env=environment,
                                check=False)
        if result.returncode:
            failed.append(check.name)
    if failed:
        print(f"Failed {len(failed)} reference checks: {', '.join(failed)}", file=sys.stderr)
    print(f"Reference checks: {len(checks) - len(failed)} passed, {len(failed)} failed", flush=True)
    return int(bool(failed))


if __name__ == "__main__":
    sys.exit(main())
