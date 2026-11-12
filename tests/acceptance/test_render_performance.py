import contextlib
import importlib.util
import io
from pathlib import Path
import unittest
from unittest.mock import patch


source = Path(__file__).parent / "compare-render-performance.py"
spec = importlib.util.spec_from_file_location("render_performance", source)
comparison = importlib.util.module_from_spec(spec)
spec.loader.exec_module(comparison)


class RenderPerformanceTests(unittest.TestCase):
    def test_speedup_cannot_hide_changed_geometry(self):
        key = ("models", 4000, "motion")
        before = {key: [{"cpu_ms": "100", "hash": "original"}]}
        after = {key: [{"cpu_ms": "1", "hash": "changed"}]}
        with self.assertRaisesRegex(ValueError, "Output geometry changed"):
            comparison.compare(before, after)

    def test_particle_count_loss_is_rejected_independently(self):
        key = ("particles", 1000, "mixed")
        before = {key: [{"cpu_ms": "100", "hash": "same", "vertices": "4000"}]}
        after = {key: [{"cpu_ms": "10", "hash": "same", "vertices": "3000"}]}
        with self.assertRaisesRegex(ValueError, "Particle vertex count changed"):
            comparison.compare(before, after)

    def test_repeated_samples_use_median_and_require_identical_workloads(self):
        key = ("models", 1000, "idle")
        before = {key: [{"cpu_ms": time, "hash": "same"} for time in ["10", "1000", "12"]]}
        after = {key: [{"cpu_ms": "6", "hash": "same"}]}
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            comparison.compare(before, after)
        self.assertIn("12.000 -> 6.000 ms (2.00x)", output.getvalue())
        with self.assertRaisesRegex(ValueError, "workloads differ"):
            comparison.compare(before, {})

    def test_profile_mismatch_and_incomplete_repetition_are_rejected(self):
        with patch.object(Path, "read_text", side_effect=[
                "render-workload-v1 profile=dev-opt1", "render-workload-v1 profile=release"]):
            with self.assertRaisesRegex(ValueError, "build profiles"):
                comparison.validate_profiles(["before", "after"])
        line = "models count=250 mode=idle cpu_ms=1 hash=same"
        with patch.object(Path, "read_text", side_effect=[line, ""]):
            with self.assertRaisesRegex(ValueError, "Incomplete repeated"):
                comparison.records(["complete", "partial"])


if __name__ == "__main__":
    unittest.main()
