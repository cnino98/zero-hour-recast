#!/usr/bin/env python3
"""Compare native radius buckets with the live outward-query numeric adapter.

This checks calcMinRadius for every represented-map-sized offset, including
fractional-cell probes. COI ordering/admission are covered by Rust regressions;
this isolated primitive does not run the original partition manager.
"""
from pathlib import Path
import reference_support
import importlib.util
import os
import tempfile
import native_real_reference

PROJECT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("turret_reference", PROJECT / "tests/reference/check-turret-reference.py")
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)
reference = helper.reference
SOURCE = "GeneralsMD/Code/GameEngine/Source/GameLogic/Object/PartitionManager.cpp"
RUST = PROJECT / "crates/generals-sim/src/skirmish/partition/query.rs"


def main():
    if reference.run(["git", "-C", str(reference.UPSTREAM), "rev-parse", "HEAD"]) != reference.COMMIT:
        raise RuntimeError("Review changed upstream baseline before partition comparison")
    if reference.run(["git", "-C", str(reference.UPSTREAM), "status", "--porcelain", "--", SOURCE]):
        raise RuntimeError("Modified reference partition source")
    source = (reference.UPSTREAM / SOURCE).read_text()
    cpp = r'''
#include <cmath>
#include <iostream>
using Real=float; using Int=int;
struct Coord3D { Real x,y,z; };
struct ICoord2D { Int x,y; };
struct PartitionManager { Real m_cellSize; Int calcMinRadius(const ICoord2D&); };
'''
    cpp += native_real_reference.cpp(reference)
    cpp += helper.function(source, "Int PartitionManager::calcMinRadius(")
    cpp += '''\nint main() {
for(float size : {10.0f,32.0f,40.0f,64.0f,0.3f}) {
 PartitionManager manager{size};
 for(int y=-256;y<=256;++y) for(int x=-256;x<=256;++x)
  std::cout << manager.calcMinRadius({x,y}) << "\\n";
}}
'''
    # Standalone extraction supplies precisely the numeric trait boundary;
    # the production code uses the already-locked num_traits checked adapter.
    rust = native_real_reference.rust()
    rust += "trait ToPrimitive { fn to_f32(self) -> Option<f32>; }\nimpl ToPrimitive for f64 { fn to_f32(self) -> Option<f32> { Some(self as f32) } }\n"
    rust += helper.function(RUST.read_text(), "fn min_radius(")
    rust += '''\nfn main() {
for size in [10.0,32.0,40.0,64.0,0.3] {
 for y in -256_i16..=256 { for x in -256_i16..=256 {
  println!("{}", min_radius([f32::from(x),f32::from(y)], size));
 }}
}}
'''
    cache = reference_support.CACHE / "partition-reference"
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="run-", dir=cache) as directory:
        work = Path(directory)
        env = os.environ.copy()
        env.update(TMPDIR=str(work), XDG_CACHE_HOME=str(work / "xdg"), CLANG_MODULE_CACHE_PATH=str(work / "modules"))
        source_cpp, source_rust = work / "reference.cpp", work / "port.rs"
        source_cpp.write_text(cpp)
        source_rust.write_text(rust)
        clang = reference_support.compiler("CXX", "clang++")

        rustc = reference_support.compiler("RUSTC", "rustc")
        native, port = work / "reference", work / "port"
        reference.run([clang, "-std=c++20", "-O1", "-ffp-contract=off", str(source_cpp), "-o", str(native)], env=env)
        reference.run([rustc, "--edition=2024", "-Dwarnings", "-Copt-level=1", str(source_rust), "-o", str(port)], env=env)
        original = reference.run([str(native)], env=env).splitlines()
        translated = reference.run([str(port)], env=env).splitlines()
        if len(original) != 5 * 513 * 513 or original != translated:
            differences = [(index, left, right) for index, (left,right) in enumerate(zip(original,translated)) if left != right]
            raise RuntimeError(f"Partition bucket mismatch: {differences[:12]}")
        print(f"PASS: {len(original)} partition radius buckets match verbatim C++ calcMinRadius, including all offsets through +/-256 cells.")

if __name__ == "__main__":
    main()
