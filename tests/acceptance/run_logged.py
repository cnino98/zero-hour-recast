"""Keep each launch's diagnostics, including launches that fail or receive a signal."""
from datetime import datetime, timezone
try:
    import fcntl
except ModuleNotFoundError:
    fcntl = None
import hashlib
import os
from pathlib import Path
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import threading


def utc_now():
    return datetime.now(timezone.utc).isoformat(timespec="microseconds")


def mirror_output(block):
    while block:
        try:
            written = os.write(sys.stdout.fileno(), block)
        except BrokenPipeError:
            return False
        block = block[written:]
    return True


def retain_logs(directory):
    """Keep forty completed sessions; a locked, active session is never removed."""
    completed = 0
    history = []
    for path in directory.glob("playtest-*.log"):
        try:
            history.append((path.stat().st_mtime_ns, path))
        except FileNotFoundError:
            continue
    for _, path in sorted(history, reverse=True):
        try:
            with path.open("rb") as previous:
                try:
                    fcntl.flock(previous, fcntl.LOCK_EX | fcntl.LOCK_NB)
                except BlockingIOError:
                    continue
                completed += 1
                if completed > 40:
                    path.unlink(missing_ok=True)
        except FileNotFoundError:
            # Another launcher may finish and rotate the same history concurrently.
            continue


def run_logged(project, executable, command, session_path=None, timeout=None):
    directory = project / "output" / "logs"
    directory.mkdir(parents=True, exist_ok=True)
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    descriptor, name = tempfile.mkstemp(prefix=f"playtest-{stamp}-", suffix=".log", dir=directory)
    path = Path(name)
    latest = project / "output" / "playtest-run.log"
    child = None
    pending_signals = []
    handlers = {}
    deadline = None
    termination_deadline = None
    timed_out = threading.Event()

    def forward(signum, _frame):
        if child is None:
            pending_signals.append(signum)
        else:
            try:
                os.killpg(child.pid, signum)
            except ProcessLookupError:
                pass

    with os.fdopen(descriptor, "wb", buffering=0) as log:
        fcntl.flock(log, fcntl.LOCK_EX)
        if session_path is not None:
            # Bind retained evidence before launch; the latest alias is shared by all sessions.
            os.link(path, session_path)
        if latest.exists() and not latest.is_symlink():
            shutil.copyfile(latest, directory / f"playtest-legacy-{path.stem}.log")
        temporary_link = latest.with_name(f".{path.stem}.latest")
        temporary_link.symlink_to(path.relative_to(latest.parent))
        temporary_link.replace(latest)

        def write(message):
            log.write(message.encode("utf-8", errors="replace"))

        write(f"session_log: {path}\nstarted_utc: {utc_now()}\nlauncher_pid: {os.getpid()}\n")
        write(f"executable: {executable}\ncommand: {shlex.join(command)}\n")
        try:
            digest = hashlib.sha256()
            with executable.open("rb") as binary:
                while block := binary.read(1024 * 1024):
                    digest.update(block)
            write(f"executable_sha256: {digest.hexdigest()}\n")
        except OSError as error:
            write(f"executable_sha256: unavailable ({error})\n")
        environment = os.environ.copy()
        environment.update(RUST_BACKTRACE="full", RUST_LIB_BACKTRACE="0", BEVY_BACKTRACE="full", NO_COLOR="1",
                           CLICOLOR="0", CLICOLOR_FORCE="0", RUST_LOG_STYLE="never")
        write("rust_backtrace: full\nrust_lib_backtrace: 0\nbevy_backtrace: full\n--- game output ---\n")
        mirror = mirror_output(f"Game session log: {path}\n".encode())
        status = 127
        try:
            for signum in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
                handlers[signum] = signal.signal(signum, forward)
            child = subprocess.Popen(command, cwd=project, env=environment,
                                     stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                     start_new_session=True)
            def expire():
                nonlocal termination_deadline
                timed_out.set()
                forward(signal.SIGTERM, None)
                termination_deadline = threading.Timer(5, forward, args=(signal.SIGKILL, None))
                termination_deadline.daemon = True
                termination_deadline.start()

            if timeout is not None:
                deadline = threading.Timer(timeout, expire)
                deadline.daemon = True
                deadline.start()
            write(f"child_pid: {child.pid}\n")
            for signum in pending_signals:
                forward(signum, None)
            with child.stdout:
                while block := child.stdout.read1(65536):
                    log.write(block)
                    if mirror:
                        mirror = mirror_output(block)
            code = child.wait()
            if timed_out.is_set():
                write(f"\nacceptance_timeout_seconds: {timeout}\n")
                code = -signal.SIGTERM if code == 0 else code
            status = code if code >= 0 else 128 - code
            if code < 0:
                write(f"\ntermination_signal: {-code} ({signal.Signals(-code).name})\n")
        except OSError as error:
            write(f"\nlaunch_error: {error}\n")
        finally:
            if deadline is not None:
                deadline.cancel()
            if termination_deadline is not None:
                termination_deadline.cancel()
            for signum, handler in handlers.items():
                signal.signal(signum, handler)
            write(f"\n--- session ended ---\nfinished_utc: {utc_now()}\nexit_status: {status}\n")
            os.fsync(log.fileno())
    try:
        retain_logs(directory)
    except OSError as error:
        print(f"Could not rotate session logs: {error}", file=sys.stderr)
    if status:
        print(f"The game stopped with status {status}. Details: {path}", file=sys.stderr)
    return status



def main():
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-root", type=Path, required=True)
    parser.add_argument("executable", type=Path)
    parser.add_argument("arguments", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    project = args.runtime_root.resolve()
    repository = Path(__file__).resolve().parents[2]
    if project == repository or repository in project.parents:
        parser.error("Runtime output must be outside the repository")
    os.environ["GENERALS_GAME_ROOT"] = str(project)
    executable = args.executable.resolve(strict=True)
    return run_logged(project, executable, [str(executable), *args.arguments])


if __name__ == "__main__":
    sys.exit(main())
