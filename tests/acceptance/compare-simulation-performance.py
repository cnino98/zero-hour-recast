#!/usr/bin/env python3
"""Compare complete simulation workloads; reject changed traces, events or RNG."""
import argparse
import json
from pathlib import Path
import re
import statistics


CASES = {"idle", "movement", "contacts", "casualties"}
COUNTS = {64, 128, 256, 512}
WORKLOADS = {(case, count) for case in CASES for count in COUNTS}
TIMINGS = ("command_us", "p50_us", "p95_us", "max_us")
FIELDS = {"case", "units", "retired", "trace", "shots", "rng", *TIMINGS}


def fields(text):
    result = {}
    position = 0
    for match in re.finditer(r"([a-z][a-z0-9_]*)=(\[[^\]]*\]|\S+)", text):
        if text[position:match.start()].strip() or match[1] in result:
            raise ValueError("Malformed or duplicate workload field")
        result[match[1]] = match[2]
        position = match.end()
    if text[position:].strip():
        raise ValueError("Malformed workload field")
    return result


def natural(value):
    if not re.fullmatch(r"[0-9]+", value):
        raise ValueError(f"Expected a nonnegative integer, found {value!r}")
    return int(value)


def parse(path):
    text = Path(path).read_text()
    headers = [line for line in text.splitlines()
               if line.startswith("SIMULATION_BENCHMARK")]
    if len(headers) != 1 or not headers[0].startswith("SIMULATION_BENCHMARK fixture_catalog "):
        raise ValueError(f"Missing, repeated or unknown workload header in {path}")
    header = fields(headers[0].removeprefix("SIMULATION_BENCHMARK fixture_catalog "))
    if not {"grid", "frames", "warmup"} <= header.keys():
        raise ValueError(f"Incomplete workload header in {path}")
    if not re.fullmatch(r"[1-9][0-9]*x[1-9][0-9]*", header["grid"]):
        raise ValueError(f"Invalid grid dimensions in {path}")
    if natural(header["frames"]) == 0:
        raise ValueError(f"Empty measured window in {path}")
    natural(header["warmup"])
    profiles = set(re.findall(r"Finished `([^`]+)` profile \[([^\]]+)\]", text))
    if len(profiles) > 1:
        raise ValueError(f"Mixed build profiles in {path}")
    rows = {}
    for line in text.splitlines():
        if not line.startswith("SIMULATION_PERF"):
            continue
        values = fields(line.removeprefix("SIMULATION_PERF "))
        if values.keys() != FIELDS:
            raise ValueError(f"Missing or unexpected result fields in {path}")
        for name in ("units", "retired", "shots", *TIMINGS):
            values[name] = natural(values[name])
        key = (values["case"], values["units"])
        if key not in WORKLOADS or key in rows:
            raise ValueError(f"Unexpected or duplicate workload in {path}: {key}")
        retired = key[1] * 4 if key[0] == "casualties" else 0
        if values["retired"] != retired:
            raise ValueError(f"Retired population changed in {path}: {key}")
        if not re.fullmatch(r"[0-9a-fA-F]{16}", values["trace"]):
            raise ValueError(f"Malformed trace in {path}: {key}")
        values["trace"] = int(values["trace"], 16)
        values["rng"] = json.loads(values["rng"])
        if not isinstance(values["rng"], list) or len(values["rng"]) != 6 or any(
                type(word) is not int or not 0 <= word <= 0xffffffff for word in values["rng"]):
            raise ValueError(f"Malformed six-word RNG in {path}: {key}")
        values["rng"] = tuple(values["rng"])
        if not values["p50_us"] <= values["p95_us"] <= values["max_us"]:
            raise ValueError(f"Invalid timing quantiles in {path}: {key}")
        rows[key] = values
    if rows.keys() != WORKLOADS:
        raise ValueError(f"Incomplete 16-case workload in {path}")
    return header, profiles, rows


def records(paths):
    if not paths:
        raise ValueError("No simulation logs provided")
    runs = [parse(path) for path in paths]
    if any(run[0] != runs[0][0] for run in runs[1:]):
        raise ValueError("Repeated workload headers differ")
    profiles = set().union(*(run[1] for run in runs))
    if len(profiles) > 1:
        raise ValueError("Recorded build profiles differ")
    rows = {key: [run[2][key] for run in runs] for key in WORKLOADS}
    return runs[0][0], profiles, all(run[1] for run in runs), rows


def compare(before, after):
    if before[0].get("trace_schema") != after[0].get("trace_schema"):
        raise ValueError("Before/after trace schemas differ; hashes cannot establish behavior parity")
    if before[0] != after[0]:
        raise ValueError("Before/after workload headers differ")
    profiles = before[1] | after[1]
    if len(profiles) > 1:
        raise ValueError("Before/after recorded build profiles differ")
    if not before[2] or not after[2]:
        print("Build profile unavailable in one or more logs; no build-profile parity claim.")
    for key in sorted(WORKLOADS):
        old, new = before[3][key], after[3][key]
        for field in ("trace", "shots", "rng"):
            if len({row[field] for row in old + new}) != 1:
                raise ValueError(f"Simulation {field} changed for {key}")
        changes = "; ".join(
            f"{field} {statistics.median(row[field] for row in old):g} -> "
            f"{statistics.median(row[field] for row in new):g}" for field in TIMINGS)
        print(f"{key[0]} units={key[1]}: {changes}; identical trace/shots/RNG")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", nargs="+", required=True)
    parser.add_argument("--after", nargs="+", required=True)
    args = parser.parse_args()
    try:
        compare(records(args.before), records(args.after))
    except (OSError, ValueError) as error:
        parser.exit(1, f"Simulation comparison failed: {error}\n")


if __name__ == "__main__":
    main()
