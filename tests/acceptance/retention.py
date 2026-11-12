"""Bound disposable generated payloads while preserving evidence and active runs."""
try:
    import fcntl
except ModuleNotFoundError:
    fcntl = None
import hashlib
import json
from pathlib import Path


def fingerprint(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


class Payloads:
    """Only registered, unchanged files may expire; manifests/logs are never registered."""
    def __init__(self, directory):
        self.directory = directory
        self.lock = (directory / ".payload-lock").open("xb")
        if fcntl is not None:
            fcntl.flock(self.lock, fcntl.LOCK_EX)
        self.files = {}
        self.record()

    def record(self):
        pending = self.directory / ".payloads.next"
        pending.write_text(json.dumps(self.files) + "\n")
        pending.replace(self.directory / ".payloads.json")

    def add(self, path, identity=None):
        self.files[str(path.relative_to(self.directory))] = identity or fingerprint(path)
        self.record()

    def close(self):
        self.lock.close()
        retain_payloads(self.directory.parent)


def retain_payloads(parent, keep=3):
    """Keep three inactive runs' payloads, including failed or abandoned runs.

    Unknown historical directories, modified files and active locked runs survive.
    POSIX locks are required for cleanup; other hosts keep all evidence.
    """
    if fcntl is None:
        return
    inactive = 0
    candidates = []
    for directory in parent.iterdir():
        try:
            candidates.append(((directory / ".payloads.json").stat().st_mtime_ns, directory))
        except OSError:
            continue
    for _, directory in sorted(candidates, reverse=True):
        marker = directory / ".payloads.json"
        lock_path = directory / ".payload-lock"
        if directory.is_symlink() or marker.is_symlink() or lock_path.is_symlink():
            continue
        try:
            with lock_path.open("rb") as lock:
                try:
                    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                except BlockingIOError:
                    continue
                files = json.loads(marker.read_text())
                if not isinstance(files, dict):
                    continue
                inactive += 1
                if inactive <= keep:
                    continue
                for relative, identity in files.items():
                    path = directory / relative
                    # Refuse traversal and symlinked parents, including replaced bundles.
                    if Path(relative).is_absolute() or ".." in Path(relative).parts:
                        continue
                    if any(part.is_symlink() for part in (path, *path.parents)):
                        continue
                    if path.is_file() and fingerprint(path) == identity:
                        path.unlink()
        except (OSError, ValueError, TypeError):
            continue
