#!/usr/bin/env python3
"""Compare native isSamePosition with the production ground-attack predicate.

Only the finite XY tolerance arithmetic is extracted. Rust integration tests
separately exercise the ten-frame check, forced repath and queued requests.
"""
from pathlib import Path
import reference_support
import importlib.util
import os
import random
import struct
import sys
import tempfile

PROJECT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location(
    "path_service_reference", PROJECT / "tests/reference/check-path-service-reference.py"
)
service = importlib.util.module_from_spec(spec)
spec.loader.exec_module(service)
reference = service.reference
SOURCE = "GeneralsMD/Code/GameEngine/Source/GameLogic/AI/AIStates.cpp"
RUST = "crates/generals-sim/src/skirmish/ground_movement/attack.rs"


def bits(value):
    return struct.unpack("<I", struct.pack("<f", value))[0]


def cases():
    result = []
    for current in [1.0, 10.0, 100.0, 1000.0, 30000.0]:
        boundary = bits(current * 0.9)
        for offset in range(-3, 4):
            result.append([bits(0.0), bits(0.0), boundary + offset, bits(0.0), bits(current), bits(0.0)])
    rng = random.Random(3751)
    for _ in range(300):
        source = [rng.uniform(-30000, 30000) for _ in range(2)]
        current = [rng.uniform(-30000, 30000) for _ in range(2)]
        previous = [point + rng.uniform(-5000, 5000) for point in current]
        result.append([bits(value) for value in source + previous + current])
    result.append([bits(2.0), bits(3.0)] * 3)
    return result


def cpp_source():
    text = (reference.UPSTREAM / SOURCE).read_text()
    header = text.split("*/", 1)[0] + "*/\n"
    function = service.extract(text, "static Bool isSamePosition(")
    lines = [header, "#include <bit>\n#include <cstdint>\n#include <iostream>",
             "using Bool=bool; using Real=float; struct Coord3D {float x,y,z;};", function,
             "int main() {"]
    for values in cases():
        points = ["{" + ",".join(f"std::bit_cast<float>(std::uint32_t({value}))" for value in values[start:start+2]) + ",0}" for start in [0, 2, 4]]
        lines.append("{ Coord3D a=" + points[0] + ",b=" + points[1] + ",c=" + points[2] + '; std::cout << isSamePosition(&a,&b,&c) << "\\n"; }')
    return "\n".join(lines) + "\n}\n"


def rust_source():
    function = service.extract((PROJECT / RUST).read_text(), "fn same_position(")
    lines = [function, "fn main() {"]
    for values in cases():
        points = ["[" + ",".join(f"f32::from_bits({value})" for value in values[start:start+2]) + "]" for start in [0, 2, 4]]
        lines.append('println!("{}",u8::from(same_position(' + ",".join(points) + ")));")
    return "\n".join(lines) + "\n}\n"


def main():
    run = reference.run
    upstream = reference.UPSTREAM
    if run(["git", "-C", str(upstream), "rev-parse", "HEAD"]) != reference.COMMIT:
        raise RuntimeError("Pinned source changed")
    if run(["git", "-C", str(upstream), "status", "--porcelain", "--", SOURCE]):
        raise RuntimeError("Native AIStates.cpp was modified")
    cache = reference_support.CACHE / "attack-approach-reference"
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="run-", dir=cache) as directory:
        work = Path(directory)
        env = os.environ.copy()
        env.update(TMPDIR=str(work), XDG_CACHE_HOME=str(work / "xdg"), CLANG_MODULE_CACHE_PATH=str(work / "modules"))
        cpp, rust = work / "reference.cpp", work / "port.rs"
        cpp.write_text(cpp_source())
        rust.write_text(rust_source())
        clang = reference_support.compiler("CXX", "clang++")

        rustc = reference_support.compiler("RUSTC", "rustc")
        native, port = work / "reference", work / "port"
        run([clang, "-std=c++20", "-Wall", "-Wextra", "-Werror", "-ffp-contract=off", str(cpp), "-o", str(native)], env=env)
        run([rustc, "--edition=2024", "-Dwarnings", str(rust), "-o", str(port)], env=env)
        expected, actual = run([str(native)], env=env), run([str(port)], env=env)
        if expected != actual:
            raise RuntimeError("Native finite-XY attack tolerance differs from production Rust")
        print(f"PASS: {len(cases())} extracted native isSamePosition cases match production Rust.")
        print("Exact and neighboring-float boundaries, coincident points and varied map-scale finite coordinates; no pursuit or full AI scheduler claim.")


if __name__ == "__main__":
    main()
