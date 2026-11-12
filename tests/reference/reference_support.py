"""External source and scratch paths shared by the bounded reference checks."""
import os
from pathlib import Path
import shutil
import subprocess

PROJECT = Path(__file__).resolve().parents[2]
UPSTREAM = Path(os.environ.get("GENERALS_REFERENCE_SOURCE",
                              PROJECT.parent / "private/references/GeneralsGameCode")).resolve()
CACHE = Path(os.environ.get("GENERALS_TEST_OUTPUT",
                           PROJECT.parent / "private/tests")).resolve() / "reference"
for name, path in (("reference checkout", UPSTREAM), ("test output", CACHE)):
    if path == PROJECT or PROJECT in path.parents:
        raise ValueError(f"The {name} must be outside the repository: {path}")
COMMIT = "2bfaea60b3b9d5a798a0d0a1aff7ea61b6cee3ed"


def validate_checkout():
    environment = dict(os.environ, GIT_OPTIONAL_LOCKS="0")
    revision = subprocess.check_output(["git", "-C", str(UPSTREAM), "rev-parse", "HEAD"],
                                       text=True, env=environment).strip()
    changes = subprocess.check_output(["git", "-C", str(UPSTREAM), "status", "--porcelain", "--untracked-files=all"],
                                      text=True, env=environment)
    if revision != COMMIT or changes:
        raise RuntimeError(f"Expected clean GeneralsGameCode checkout at {COMMIT}: {UPSTREAM}")


def compiler(variable, default):
    value = os.environ.get(variable, default)
    found = shutil.which(value)
    if found is None:
        raise RuntimeError(f"Missing {default}; install it separately or set {variable}")
    return found


def write_fixture(fixture, content):
    """Stage regenerated values for review without changing checked-in fixtures."""
    destination = CACHE / "regenerated" / fixture.name
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(content)
    print(f"Regenerated fixture for review: {destination}")
