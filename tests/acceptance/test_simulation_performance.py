import contextlib
import importlib.util
import io
from pathlib import Path
import unittest
from unittest.mock import patch


source = Path(__file__).parent / "compare-simulation-performance.py"
spec = importlib.util.spec_from_file_location("simulation_performance", source)
comparison = importlib.util.module_from_spec(spec)
spec.loader.exec_module(comparison)


def log(time=100, profile="test", metadata=""):
    lines = [f"    Finished `{profile}` profile [optimized] target(s) in 1s",
             f"SIMULATION_BENCHMARK fixture_catalog grid=400x400 frames=600 warmup=30{metadata}"]
    for case, count in sorted(comparison.WORKLOADS):
        retired = count * 4 if case == "casualties" else 0
        lines.append(f"SIMULATION_PERF case={case} units={count} retired={retired} "
                     f"command_us=0 p50_us={time} p95_us={time + 1} max_us={time + 2} "
                     "trace=0123456789abcdef shots=0 rng=[1, 2, 3, 4, 5, 6]")
    return "\n".join(lines)


def records(*texts):
    with patch.object(Path, "read_text", side_effect=texts):
        return comparison.records([f"sample{index}" for index in range(len(texts))])


class SimulationPerformanceTests(unittest.TestCase):
    def test_complete_repeated_workloads_use_medians_without_speed_threshold(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            comparison.compare(records(log(10), log(1000), log(12)), records(log(24)))
        self.assertIn("p50_us 12 -> 24", output.getvalue())
        self.assertEqual(output.getvalue().count("identical trace/shots/RNG"), 16)

    def test_speedup_cannot_hide_changed_trace_event_count_or_rng(self):
        for before, after, field in [
                ("0123456789abcdef", "fedcba9876543210", "trace"),
                ("shots=0", "shots=1", "shots"),
                ("[1, 2, 3, 4, 5, 6]", "[1, 2, 3, 4, 5, 7]", "rng")]:
            with self.subTest(field=field), contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaisesRegex(ValueError, f"{field} changed"):
                    comparison.compare(records(log()), records(log(1).replace(before, after, 1)))

    def test_changed_repetition_is_rejected_even_if_last_run_matches(self):
        changed = log().replace("shots=0", "shots=1", 1)
        with self.assertRaisesRegex(ValueError, "shots changed"):
            comparison.compare(records(changed, log()), records(log()))

    def test_missing_duplicate_or_changed_population_is_rejected(self):
        complete = log()
        first_row = complete.splitlines()[2]
        for invalid in [
                complete.replace(first_row + "\n", ""),
                complete + "\n" + first_row,
                complete.replace("units=64", "units=65", 1),
                complete.replace("retired=256", "retired=255", 1)]:
            with self.subTest(invalid=invalid[:40]), self.assertRaises(ValueError):
                records(invalid)
        with self.assertRaisesRegex(ValueError, "Incomplete 16-case"):
            records(complete, complete.replace(first_row + "\n", ""))

    def test_header_and_recorded_build_changes_are_rejected(self):
        for old, new in [("frames=600", "frames=300"), ("warmup=30", "warmup=0"),
                         ("grid=400x400", "grid=200x200"), ("`test`", "`release`")]:
            with self.subTest(new=new), self.assertRaisesRegex(ValueError, "differ"):
                comparison.compare(records(log()), records(log().replace(old, new)))
        with self.assertRaisesRegex(ValueError, "headers differ"):
            comparison.compare(records(log(metadata=" build=dev-opt1")),
                               records(log(metadata=" build=release")))

    def test_missing_profile_is_reported_and_never_assumed(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            comparison.compare(records(log()), records("\n".join(log().splitlines()[1:])))
        self.assertIn("no build-profile parity claim", output.getvalue())

    def test_trace_schema_mismatch_is_rejected_even_when_hashes_match(self):
        for metadata in ("", " trace_schema=weapons2"):
            with self.subTest(metadata=metadata), self.assertRaisesRegex(ValueError, "trace schemas differ"):
                comparison.compare(records(log(metadata=metadata)),
                                   records(log(metadata=" trace_schema=weapons3")))

    def test_malformed_records_cannot_pass_as_measurements(self):
        valid = log()
        for invalid in [valid.replace("frames=600", "frames=0"),
                        valid.replace("rng=[1, 2, 3, 4, 5, 6]", "rng=[1, 2]", 1),
                        valid.replace("rng=[1, 2, 3, 4, 5, 6]", "rng=[1, 2, 3, 4, 5, true]", 1),
                        valid.replace("command_us=0", "command_us=-1", 1),
                        valid.replace("trace=0123456789abcdef", "trace=unknown", 1),
                        valid.replace("p50_us=100", "p50_us=104", 1),
                        valid.replace("shots=0", "shots=0 shots=0", 1),
                        valid + "\n" + valid.splitlines()[1]]:
            with self.subTest(invalid=invalid[:40]), self.assertRaises(ValueError):
                records(invalid)


if __name__ == "__main__":
    unittest.main()
