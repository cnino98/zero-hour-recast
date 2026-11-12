"""The public entry point rejects empty scaling matches and repository outputs."""
import importlib.util
import plistlib
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch

PROJECT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("test_entry", PROJECT / "tests/run.py")
ENTRY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ENTRY)


class EntryTests(unittest.TestCase):
    def test_successful_cargo_exit_with_zero_matching_tests_is_incomplete(self):
        self.assertFalse(ENTRY.scaling_complete("test result: ok. 0 passed; 0 failed; 0 ignored; 500 filtered out;\n"))
        self.assertFalse(ENTRY.scaling_complete("test result: ok. 0 passed; 0 failed; 1 ignored; 499 filtered out;\n"))
        self.assertTrue(ENTRY.scaling_complete("test result: ok. 1 passed; 0 failed; 0 ignored; 499 filtered out; finished in 1.0s\n"))

    def test_python_discovery_must_report_at_least_one_test(self):
        self.assertFalse(ENTRY.python_discovered("Ran 0 tests in 0.000s\n\nOK\n"))
        self.assertTrue(ENTRY.python_discovered("Ran 1 test in 0.001s\n\nOK\n"))
        self.assertTrue(ENTRY.python_discovered("Ran 20 tests in 1.024s\n\nOK\n"))

    def test_macos_exports_a_foreground_bundle_with_the_exact_tested_executable(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = root / "generals-client"
            original.write_bytes(b"test executable")
            candidate = ENTRY.export_runtime(original, root / "delivery", "darwin")
            self.assertEqual(candidate.read_bytes(), original.read_bytes())
            info = plistlib.loads((candidate.parents[1] / "Info.plist").read_bytes())
            self.assertEqual(info["CFBundlePackageType"], "APPL")
            self.assertEqual(info["CFBundleExecutable"], candidate.name)
            self.assertFalse(info.get("LSBackgroundOnly", False))
            self.assertFalse(info.get("LSUIElement", False))

    def test_external_output_rejects_repository_children(self):
        with self.assertRaisesRegex(ValueError, "outside the repository"):
            ENTRY.external(PROJECT / "target")
        self.assertEqual(ENTRY.external(PROJECT.parent / "private"), (PROJECT.parent / "private").resolve())

    def test_full_export_registers_ownership_before_copying(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = root / "original"
            original.write_bytes(b"release executable")
            destination = root / "run"
            destination.mkdir()
            payloads = ENTRY.Payloads(destination)

            def interrupted_copy(source, candidate):
                self.assertEqual(payloads.files[candidate.name], ENTRY.digest(source))
                candidate.write_bytes(source.read_bytes())
                raise RuntimeError("abandoned export")

            try:
                with patch.object(ENTRY.shutil, "copy2", side_effect=interrupted_copy):
                    with self.assertRaisesRegex(RuntimeError, "abandoned export"):
                        ENTRY.export_runtime(original, destination, "linux", payloads)
            finally:
                payloads.lock.close()
            self.assertIn(original.name, payloads.files)
