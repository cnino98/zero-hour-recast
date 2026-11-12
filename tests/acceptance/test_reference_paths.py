"""Reference scratch paths and regenerated fixtures never modify the repository."""
import importlib.util
import os
import contextlib
import io
import sys
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

PROJECT = Path(__file__).resolve().parents[2]
SUPPORT = PROJECT / "tests/reference/reference_support.py"


def support():
    spec = importlib.util.spec_from_file_location("reference_support_test", SUPPORT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ReferencePathTests(unittest.TestCase):
    def test_empty_reference_directory_fails_instead_of_reporting_success(self):
        spec = importlib.util.spec_from_file_location("reference_runner", SUPPORT.parent / "run.py")
        runner = importlib.util.module_from_spec(spec)
        with patch.dict(sys.modules, reference_support=support()):
            spec.loader.exec_module(runner)
        errors = io.StringIO()
        with patch.object(runner, "HERE", PROJECT / "tests/acceptance"), contextlib.redirect_stderr(errors):
            self.assertEqual(runner.main(), 1)
        self.assertIn("No reference checks found", errors.getvalue())

    def test_repository_source_and_output_are_rejected(self):
        for name in ("GENERALS_REFERENCE_SOURCE", "GENERALS_TEST_OUTPUT"):
            with self.subTest(name=name), patch.dict(os.environ, {name: str(PROJECT / "nested")}):
                with self.assertRaisesRegex(ValueError, "outside the repository"):
                    support()

    def test_fixture_regeneration_stages_external_output_for_review(self):
        directory = Path(os.environ.get("GENERALS_TEST_OUTPUT", PROJECT.parent / "private/tests")) / "tmp"
        directory.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=directory) as temporary:
            with patch.dict(os.environ, {"GENERALS_TEST_OUTPUT": temporary}):
                module = support()
            fixture = Path(temporary) / "source.txt"
            fixture.write_text("checked fixture")
            module.write_fixture(fixture, "new result")
            self.assertEqual(fixture.read_text(), "checked fixture")
            self.assertEqual((module.CACHE / "regenerated/source.txt").read_text(), "new result")
