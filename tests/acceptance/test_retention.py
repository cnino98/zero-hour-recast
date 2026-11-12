"""Generated payload rotation preserves active runs, records and user changes."""
import os
from pathlib import Path
import tempfile
import unittest

from retention import Payloads, fcntl, retain_payloads


@unittest.skipIf(fcntl is None, "Cleanup requires POSIX locking")
class PayloadRetentionTests(unittest.TestCase):
    def test_bounds_completed_and_abandoned_payloads_without_touching_evidence(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get("TMPDIR")) as temporary:
            parent = Path(temporary)
            runs = []
            for index in range(6):
                directory = parent / f"run-{index}"
                directory.mkdir()
                payloads = Payloads(directory)
                image = directory / "capture.png"
                image.write_bytes(b"generated capture")
                payloads.add(image)
                (directory / "manifest.json").write_text("failed attempt")
                (directory / "session.log").write_text("failure diagnostics")
                runs.append((payloads, image))
            # First is still active even though oldest. Others mimic a killed runner.
            for payloads, _ in runs[1:]:
                payloads.lock.close()
            runs[1][1].write_bytes(b"user replacement")
            link = runs[2][1]
            link.unlink()
            link.symlink_to(runs[5][1])
            retain_payloads(parent)
            self.assertTrue(runs[0][1].exists())
            self.assertEqual(runs[1][1].read_bytes(), b"user replacement")
            self.assertTrue(link.is_symlink())
            self.assertTrue(runs[5][1].exists())
            runs[0][0].lock.close()
            retain_payloads(parent)
            self.assertFalse(runs[0][1].exists())
            for payloads, _ in runs:
                self.assertEqual((payloads.directory / "manifest.json").read_text(), "failed attempt")
                self.assertEqual((payloads.directory / "session.log").read_text(), "failure diagnostics")

    def test_unmarked_history_and_unregistered_files_are_untouched(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get("TMPDIR")) as temporary:
            parent = Path(temporary)
            legacy = parent / "legacy"
            legacy.mkdir()
            protected = legacy / "generals-client"
            protected.write_bytes(b"preserved build")
            directory = parent / "run"
            directory.mkdir()
            payloads = Payloads(directory)
            generated = directory / "generals-client"
            generated.write_bytes(b"duplicate verification build")
            payloads.add(generated)
            payloads.lock.close()
            retain_payloads(parent, keep=0)
            self.assertFalse(generated.exists())
            self.assertEqual(protected.read_bytes(), b"preserved build")
