#!/usr/bin/env python3
"""Compare repeated renderer workloads, failing on any changed output geometry."""
import argparse
from pathlib import Path
import statistics


def records(paths):
    result = {}
    expected_keys = None
    for path in paths:
        run_keys = set()
        for line in Path(path).read_text().splitlines():
            if not line.startswith(("models count=", "particles emitters=")):
                continue
            kind, *fields = line.split()
            values = dict(field.split("=", 1) for field in fields)
            population = values["count" if kind == "models" else "emitters"]
            key = (kind, int(population), values["mode"])
            if key in run_keys:
                raise ValueError(f"Duplicate workload in {path}: {key}")
            run_keys.add(key)
            result.setdefault(key, []).append(values)
        if expected_keys is not None and run_keys != expected_keys:
            raise ValueError(f"Incomplete repeated workload in {path}")
        expected_keys = run_keys
    if not result:
        raise ValueError("No renderer workload records were found")
    return result


def compare(before, after):
    if before.keys() != after.keys():
        raise ValueError("Before/after workloads differ")
    for key, old in sorted(before.items()):
        new = after[key]
        hashes = {row["hash"] for row in old + new}
        if len(hashes) != 1:
            raise ValueError(f"Output geometry changed for {key}: {sorted(hashes)}")
        if key[0] == "particles":
            if len({row["vertices"] for row in old + new}) != 1:
                raise ValueError(f"Particle vertex count changed for {key}")
        old_ms = statistics.median(float(row["cpu_ms"]) for row in old)
        new_ms = statistics.median(float(row["cpu_ms"]) for row in new)
        print(f"{key[0]} {key[1]} {key[2]}: {old_ms:.3f} -> {new_ms:.3f} ms "
              f"({old_ms / new_ms:.2f}x), identical hash {next(iter(hashes))}")


def validate_profiles(paths):
    headers = []
    for path in paths:
        found = [line for line in Path(path).read_text().splitlines()
                 if line.startswith("render-workload-v")]
        if len(found) != 1:
            raise ValueError(f"Missing or repeated renderer workload header in {path}")
        headers.extend(found)
    if len(set(headers)) != 1:
        raise ValueError("Before/after build profiles or workloads differ")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", nargs="+", required=True)
    parser.add_argument("--after", nargs="+", required=True)
    args = parser.parse_args()
    validate_profiles(args.before + args.after)
    compare(records(args.before), records(args.after))


if __name__ == "__main__":
    main()
