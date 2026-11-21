"""Exercise retained evidence, child failures, signal forwarding and concurrent launches."""
try:
    import fcntl
except ModuleNotFoundError:
    fcntl = None
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import sys
import tempfile
import time
import unittest


PROJECT = Path(__file__).resolve().parents[2]
SCRIPT = PROJECT / "tests/acceptance" / "run_logged.py"
SPEC = importlib.util.spec_from_file_location("run_logged", SCRIPT)
LOGGER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(LOGGER)
HARNESS = """
import importlib.util
from pathlib import Path
import sys
spec = importlib.util.spec_from_file_location("run_logged", sys.argv[1])
logger = importlib.util.module_from_spec(spec)
spec.loader.exec_module(logger)
sys.exit(logger.run_logged(Path(sys.argv[2]), Path(sys.executable), sys.argv[3:]))
"""


BATCH_HARNESS = r"""
import argparse
import importlib.util
from pathlib import Path
import subprocess
import sys
from unittest.mock import patch
sys.path.insert(0, str(Path(sys.argv[1]).parent))
spec = importlib.util.spec_from_file_location("run_acceptance", Path(sys.argv[1]).with_name("run.py"))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
project, ready, advanced = map(Path, sys.argv[2:])
candidate = project / "candidate"
candidate.write_bytes(b"retained candidate identity")
child = "import signal, sys, time; from pathlib import Path; signal.signal(signal.SIGINT, lambda *_: sys.exit(0)); Path(sys.argv[1]).write_text('ready'); print('interrupt handler ready', flush=True); time.sleep(30)"
cases = [
    {"id": "profiles-first", "group": "profiles", "runtime": False, "requires_image": False,
     "arguments": [sys.executable, "-c", child, str(ready)]},
    {"id": "profiles-second", "group": "profiles", "runtime": False, "requires_image": False,
     "arguments": [sys.executable, "-c", "from pathlib import Path; import sys; Path(sys.argv[1]).write_text('advanced')", str(advanced)]},
]
arguments = argparse.Namespace(executable=str(candidate), runtime_root=str(project),
                               group=["profiles"], case=[], list=False, output=None,
                               stop_on_failure=False, content_overlay=None)
with patch.object(runner.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "fixture help", "")), \
        patch.object(runner, "inventory", return_value=cases):
    sys.exit(runner.run(arguments))
"""


@unittest.skipUnless(os.name == "posix", "POSIX child signals, locks and log retention")
class LaunchLoggingTests(unittest.TestCase):
    def setUp(self):
        temporary = Path(os.environ.get("GENERALS_TEST_OUTPUT", PROJECT.parent / "private/tests")) / "tmp"
        temporary.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="launch-logging-", dir=temporary)
        self.addCleanup(self.temporary.cleanup)
        self.project = Path(self.temporary.name)
        self.logs = self.project / "output" / "logs"

    def start(self, source, *arguments):
        environment = os.environ.copy()
        environment["PYTHONDONTWRITEBYTECODE"] = "1"
        return subprocess.Popen([sys.executable, "-c", HARNESS, str(SCRIPT), str(self.project),
                                 sys.executable, "-c", source, *arguments],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=environment)

    def finish(self, process):
        try:
            output, errors = process.communicate(timeout=15)
        except subprocess.TimeoutExpired:
            process.kill()
            process.communicate()
            self.fail("logging subprocess did not finish")
        return process.returncode, output, errors

    def latest(self):
        return (self.project / "output" / "playtest-run.log").resolve()

    def test_timeout_terminates_child_and_preserves_bound_evidence(self):
        evidence = self.project / "timeout.log"
        status = LOGGER.run_logged(self.project, Path(sys.executable),
                                   [sys.executable, "-c", "import time; print('before timeout', flush=True); time.sleep(30)"],
                                   session_path=evidence, timeout=0.1)
        self.assertEqual(status, 143)
        log = evidence.read_text()
        self.assertIn("before timeout", log)
        self.assertIn("acceptance_timeout_seconds: 0.1", log)
        self.assertIn("exit_status: 143", log)

    def test_success_records_metadata_arguments_and_merged_output(self):
        source = "import os, sys; print(os.environ['RUST_BACKTRACE'], os.environ['BEVY_BACKTRACE'], os.environ['NO_COLOR']); print('stderr-marker', file=sys.stderr)"
        arguments = ("spaces and 'quotes'", "$(not-a-command)", "line\nbreak")
        code, output, _ = self.finish(self.start(source, *arguments))
        self.assertEqual(code, 0)
        self.assertIn(b"stderr-marker", output)
        data = self.latest().read_text()
        self.assertIn("full full 1", data)
        self.assertIn("stderr-marker", data)
        self.assertIn("started_utc: ", data)
        self.assertIn("finished_utc: ", data)
        self.assertIn("exit_status: 0", data)
        self.assertIn(hashlib.sha256(Path(sys.executable).read_bytes()).hexdigest(), data)
        command = data.split("command: ", 1)[1].split("\nexecutable_sha256:", 1)[0]
        self.assertEqual(shlex.split(command), [sys.executable, "-c", source, *arguments])

    def test_failed_session_survives_restart_with_exact_exit_status(self):
        code, _, _ = self.finish(self.start("import sys; print('panic evidence'); sys.exit(101)"))
        self.assertEqual(code, 101)
        failed = self.latest()
        before = failed.read_bytes()
        self.assertIn(b"exit_status: 101", before)
        self.assertEqual(self.finish(self.start("print('next game')"))[0], 0)
        self.assertNotEqual(failed, self.latest())
        self.assertEqual(failed.read_bytes(), before)

    def test_child_library_capture_disabled_with_failure_diagnostics_retained(self):
        source = """
import os
import sys
print('panic_backtrace=' + os.environ['RUST_BACKTRACE'], flush=True)
print('library_backtrace=' + os.environ['RUST_LIB_BACKTRACE'], flush=True)
print('bevy_backtrace=' + os.environ['BEVY_BACKTRACE'], flush=True)
raise RuntimeError('retained child stack evidence')
"""
        code, output, _ = self.finish(self.start(source))
        self.assertEqual(code, 1)
        data = self.latest().read_text()
        for marker in ("panic_backtrace=full", "library_backtrace=0", "bevy_backtrace=full",
                       "Traceback (most recent call last):", "RuntimeError: retained child stack evidence"):
            self.assertIn(marker, data)
            self.assertIn(marker.encode(), output)
        self.assertIn("rust_backtrace: full\nrust_lib_backtrace: 0\nbevy_backtrace: full\n", data)
        self.assertIn("exit_status: 1", data)

    def test_child_kill_records_signal_and_preserves_prior_output(self):
        source = "import os, signal; print('before termination', flush=True); os.kill(os.getpid(), signal.SIGKILL)"
        self.assertEqual(self.finish(self.start(source))[0], 137)
        data = self.latest().read_text()
        self.assertIn("before termination", data)
        self.assertIn("termination_signal: 9 (SIGKILL)", data)
        self.assertIn("exit_status: 137", data)

    def test_termination_of_supervisor_is_forwarded_and_logged(self):
        ready = self.project / "child-ready"
        source = "from pathlib import Path; import sys, time; Path(sys.argv[1]).write_text('ready'); time.sleep(30)"
        process = self.start(source, str(ready))
        deadline = time.monotonic() + 10
        while not ready.exists() and process.poll() is None and time.monotonic() < deadline:
            time.sleep(0.01)
        if not ready.exists():
            process.kill()
            process.communicate()
            self.fail("child never became ready")
        process.send_signal(signal.SIGTERM)
        self.assertEqual(self.finish(process)[0], 143)
        self.assertIn("termination_signal: 15 (SIGTERM)", self.latest().read_text())

    def test_external_interrupt_stops_batch_even_when_child_handles_it_and_exits_zero(self):
        ready = self.project / "child-ready"
        advanced = self.project / "second-case-ran"
        environment = os.environ.copy()
        environment["PYTHONDONTWRITEBYTECODE"] = "1"
        process = subprocess.Popen(
            [sys.executable, "-c", BATCH_HARNESS, str(SCRIPT), str(self.project),
             str(ready), str(advanced)],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=environment,
        )
        deadline = time.monotonic() + 10
        while not ready.exists() and process.poll() is None and time.monotonic() < deadline:
            time.sleep(0.01)
        if not ready.exists():
            process.kill()
            process.communicate()
            self.fail("first acceptance child never installed its signal handler")
        process.send_signal(signal.SIGINT)
        self.assertEqual(self.finish(process)[0], 1)
        data = self.latest().read_text()
        self.assertIn("interruption_requested: 2 (SIGINT)", data)
        self.assertIn("child_exit_status: 0", data)
        self.assertIn("exit_status: 130", data)
        manifest = json.loads(next((self.project / "output/acceptance").glob("*/manifest.json")).read_text())
        self.assertEqual(len(manifest["results"]), 1)
        self.assertEqual(manifest["results"][0]["exit_status"], 130)
        self.assertEqual(manifest["not_run"], ["profiles-second"])
        self.assertFalse(advanced.exists())

    def test_concurrent_launches_have_separate_complete_logs(self):
        source = "import sys, time; print(sys.argv[1], flush=True); time.sleep(0.1); print('finished')"
        first = self.start(source, "first-session")
        second = self.start(source, "second-session")
        self.assertEqual(self.finish(first)[0], 0)
        self.assertEqual(self.finish(second)[0], 0)
        paths = list(self.logs.glob("playtest-*.log"))
        self.assertEqual(len(paths), 2)
        self.assertIn(self.latest(), paths)
        for path in paths:
            data = path.read_text()
            self.assertIn("exit_status: 0", data)
            self.assertIn("\nfinished\n", data)
            self.assertNotEqual("first-session" in data, "second-session" in data)

    def test_retention_preserves_active_sessions_and_protected_crashes(self):
        self.logs.mkdir(parents=True)
        protected = self.project / "output" / "crashes" / "saved-crash.log"
        protected.parent.mkdir()
        protected.write_text("preserve this evidence")
        for index in range(45):
            path = self.logs / f"playtest-{index:02}.log"
            path.write_text("completed")
            os.utime(path, ns=(index + 1, index + 1))
        active = self.logs / "playtest-active.log"
        with active.open("wb") as handle:
            fcntl.flock(handle, fcntl.LOCK_EX)
            os.utime(active, ns=(0, 0))
            LOGGER.retain_logs(self.logs)
            self.assertTrue(active.exists())
            self.assertEqual(len(list(self.logs.glob("*.log"))), 41)
        LOGGER.retain_logs(self.logs)
        self.assertEqual(len(list(self.logs.glob("*.log"))), 40)
        self.assertEqual(protected.read_text(), "preserve this evidence")

    def test_old_single_log_is_archived_before_latest_link_replacement(self):
        latest = self.project / "output" / "playtest-run.log"
        latest.parent.mkdir()
        latest.write_text("old crash trace")
        self.assertEqual(self.finish(self.start("print('new session')"))[0], 0)
        self.assertTrue(latest.is_symlink())
        archived = list(self.logs.glob("playtest-legacy-*.log"))
        self.assertEqual(len(archived), 1)
        self.assertEqual(archived[0].read_text(), "old crash trace")

    def test_missing_child_still_leaves_a_completed_failure_log(self):
        environment = os.environ.copy()
        environment["PYTHONDONTWRITEBYTECODE"] = "1"
        process = subprocess.Popen([sys.executable, "-c", HARNESS, str(SCRIPT), str(self.project),
                                    str(self.project / "missing-executable")],
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=environment)
        self.assertEqual(self.finish(process)[0], 127)
        data = self.latest().read_text()
        self.assertIn("launch_error:", data)
        self.assertIn("exit_status: 127", data)

    def test_closed_output_pipe_does_not_lose_log_or_override_status(self):
        process = self.start("import time; time.sleep(0.1); print('retained after pipe closed')")
        process.stdout.close()
        try:
            self.assertEqual(process.wait(timeout=15), 0)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            process.stderr.close()
        self.assertIn("retained after pipe closed", self.latest().read_text())
        self.assertIn("exit_status: 0", self.latest().read_text())


if __name__ == "__main__":
    unittest.main()
