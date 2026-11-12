#!/usr/bin/env python3
"""Execute pinned hierarchy search with explicit zone/bridge/INFO fixture inputs.

This adapter supplies explicit raw-zone query results and cell storage; it does
not validate the independently implemented raw-zone cache. processHierarchicalCell, internal_findHierarchicalPath,
prependCells, buildHierarchicalPath and both cost functions are copied unchanged.
"""
from pathlib import Path
import reference_support
import importlib.util
import os
import sys
import tempfile

PROJECT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("reference", PROJECT / "tests/reference/check-path-reference.py")
reference = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reference)
SOURCE = "Core/GameEngine/Source/GameLogic/AI/AIPathfind.cpp"
FIXTURE = PROJECT / "tests/reference/hierarchy-reference.txt"


def extract(text, signature):
    start = text.index(signature)
    return text[start:text.index("\n}\n", start) + 3]


def source():
    native = (reference.UPSTREAM / SOURCE).read_text()
    adapter = (PROJECT / "tests/reference/hierarchy-adapter.cpp").read_text()
    functions = "".join(extract(native, signature) for signature in [
        "UnsignedInt PathfindCell::costToGoal(",
        "UnsignedInt PathfindCell::costToHierGoal(",
        "Bool PathfindCell::startPathfind(",
        "void Pathfinder::prependCells(",
        "Path *Pathfinder::buildHierarchicalPath(",
        "void Pathfinder::processHierarchicalCell(",
        "Path *Pathfinder::internal_findHierarchicalPath(",
    ])
    main = (PROJECT / "tests/reference/hierarchy-cases.cpp").read_text()
    return native.split("*/", 1)[0] + "*/\n" + adapter + functions + main


def main():
    if reference.run(["git", "-C", str(reference.UPSTREAM), "rev-parse", "HEAD"]) != reference.COMMIT:
        raise RuntimeError("Unexpected pinned source revision")
    temp = reference_support.CACHE
    temp.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, TMPDIR=str(temp), CLANG_MODULE_CACHE_PATH=str(temp / "clang-modules"))
    with tempfile.TemporaryDirectory(prefix="hierarchy-reference-", dir=temp) as directory:
        folder = Path(directory)
        cpp = folder / "hierarchy.cpp"
        cpp.write_text(source())
        binary = folder / "hierarchy"
        reference.run([reference_support.compiler("CXX", "clang++"), "-std=c++20", "-O0", "-ffp-contract=off", str(cpp), "-o", str(binary)], env=env)
        result = reference.run([str(binary)], env=env) + "\n"
    if "--write-fixture" in sys.argv:
        reference_support.write_fixture(FIXTURE, result)
        print("Wrote native hierarchy fixture; Rust consumer comparison still required.")
    elif FIXTURE.read_text() != result:
        raise RuntimeError("Pinned hierarchy fixture changed")
    else:
        print("PASS: pinned hierarchy boundary, bridge and INFO cleanup fixture.")


if __name__ == "__main__":
    main()
