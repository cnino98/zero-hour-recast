#!/usr/bin/env python3
"""Compare native fast transport assignments for represented small team sizes.

The native size-only std::sort comparator leaves equal-size identity ordering
STL-dependent. This checks the local C++ implementation through twelve entries
(the observed base-USA transport teams have at most ten nontransport members),
not every compiler's sort or the original complete team scheduler.
"""
from pathlib import Path
import reference_support
import importlib.util
import os
import tempfile

PROJECT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("turret_reference", PROJECT / "tests/reference/check-turret-reference.py")
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)
reference = helper.reference
SOURCE = "GeneralsMD/Code/GameEngine/Source/Common/PartitionSolver.cpp"
RUST = PROJECT / "crates/generals-sim/src/skirmish/opponent/transports.rs"


def main():
    if reference.run(["git", "-C", str(reference.UPSTREAM), "rev-parse", "HEAD"]) != reference.COMMIT:
        raise RuntimeError("Review changed upstream baseline before transport comparison")
    if reference.run(["git", "-C", str(reference.UPSTREAM), "status", "--porcelain", "--", SOURCE]):
        raise RuntimeError("Modified reference transport source")
    source = (reference.UPSTREAM / SOURCE).read_text()
    cpp = r'''
#include <algorithm>
#include <iostream>
#include <vector>
using Bool=bool; using Int=int;
using PairObjectIDAndUInt=std::pair<unsigned,unsigned>;
using EntriesVec=std::vector<PairObjectIDAndUInt>;
using SpacesVec=EntriesVec;
constexpr int PREFER_FAST_SOLUTION=0;
#define DEBUG_CRASH(...)
struct PartitionSolver {
 EntriesVec m_data, m_spacesForData, m_bestSolution, m_currentSolution;
 unsigned m_currentSolutionLeftovers=0;
 int m_howToSolve=0;
 void solve();
};
'''
    cpp += helper.function(source, "static Bool greater_than(")
    cpp += helper.function(source, "void PartitionSolver::solve()")
    cpp += r'''
int main() {
 for(unsigned n=0;n<=12;++n) for(unsigned m=0;m<=4;++m) for(unsigned seed=0;seed<64;++seed) {
  PartitionSolver p;
  for(unsigned i=0;i<n;++i) p.m_data.push_back({i*17%101,(seed+i*(seed%5))%9});
  for(unsigned i=0;i<m;++i) p.m_spacesForData.push_back({100+i,(seed+i*(seed%7))%13});
  p.solve();
  std::cout<<n<<":"<<m<<":"<<seed<<"|";
  for(auto [unit,host]:p.m_bestSolution) std::cout<<unit<<":"<<host<<",";
  std::cout<<"\n";
 }
}
'''
    rust = helper.function(RUST.read_text(), "fn partition(")
    rust += r'''
fn main() {
 for n in 0..=12 { for m in 0..=4 { for seed in 0..64 {
  let entries=(0..n).map(|i| (i*17%101,u32::try_from((seed+i*(seed%5))%9).unwrap())).collect();
  let spaces=(0..m).map(|i| (100+i,u32::try_from((seed+i*(seed%7))%13).unwrap())).collect();
  print!("{n}:{m}:{seed}|");
  for (unit,host) in partition(entries,spaces) { print!("{unit}:{host},"); }
  println!();
 }}}
}
'''
    cache = reference_support.CACHE / "transport-reference"
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
        reference.run([clang, "-std=c++20", "-O1", str(source_cpp), "-o", str(native)], env=env)
        reference.run([rustc, "--edition=2024", "-Dwarnings", "-Copt-level=1", str(source_rust), "-o", str(port)], env=env)
        original = reference.run([str(native)], env=env).splitlines()
        translated = reference.run([str(port)], env=env).splitlines()
        if len(original) != 13 * 5 * 64 or original != translated:
            differences = [(index, left, right) for index, (left, right) in enumerate(zip(original, translated)) if left != right]
            raise RuntimeError(f"Transport assignment mismatch: {differences[:12]}")
        print(f"PASS: {len(original)} native fast transport assignment fixtures match, including equal-size small teams and insufficient/zero capacities.")


if __name__ == "__main__":
    main()
