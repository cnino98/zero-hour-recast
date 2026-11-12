#!/usr/bin/env python3
"""Compare pinned raw-zone, block/global combiner and partial-update routines.

The adapter supplies original cell kinds, fences, bridge endpoints and bounds.
All classification, equivalence, flattening and query algorithms are unchanged.
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
FIXTURE = PROJECT / "tests/reference/zone-reference.txt"


def extract(text, signature):
    start = text.index(signature)
    return text[start:text.index("\n}\n", start) + 3]


def source():
    native = (reference.UPSTREAM / SOURCE).read_text()
    adapter = (PROJECT / "tests/reference/zone-adapter.cpp").read_text()
    helpers = native[native.index("inline Bool typesMatch("):native.index("//------------------------  ZoneBlock")]
    functions = helpers + "".join(extract(native, signature) for signature in [
        "void ZoneBlock::freeZones(",
        "void ZoneBlock::allocateZones(",
        "void ZoneBlock::blockCalculateZones(",
        "zoneStorageType ZoneBlock::getEffectiveZone(",
        "void PathfindZoneManager::freeZones(",
        "void PathfindZoneManager::allocateZones(",
        "void PathfindZoneManager::calculateZones(",
        "void PathfindZoneManager::updateZonesForModify(",
        "void PathfindZoneManager::setBridge(",
        "Bool PathfindZoneManager::interactsWithBridge(",
        "zoneStorageType PathfindZoneManager::getBlockZone(",
        "zoneStorageType PathfindZoneManager::getEffectiveTerrainZone(",
        "zoneStorageType PathfindZoneManager::getEffectiveZone(",
    ])
    main = (PROJECT / "tests/reference/zone-cases.cpp").read_text()
    return native.split("*/", 1)[0] + "*/\n" + adapter + functions + main


def main():
    if reference.run(["git", "-C", str(reference.UPSTREAM), "rev-parse", "HEAD"]) != reference.COMMIT:
        raise RuntimeError("Unexpected pinned source revision")
    temp = reference_support.CACHE
    temp.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, TMPDIR=str(temp), CLANG_MODULE_CACHE_PATH=str(temp / "clang-modules"))
    with tempfile.TemporaryDirectory(prefix="zone-reference-", dir=temp) as directory:
        folder = Path(directory)
        cpp = folder / "zones.cpp"
        cpp.write_text(source())
        binary = folder / "zones"
        reference.run([reference_support.compiler("CXX", "clang++"), "-std=c++20", "-O0", "-ffp-contract=off", str(cpp), "-o", str(binary)], env=env)
        result = reference.run([str(binary)], env=env) + "\n"
    if "--write-fixture" in sys.argv:
        reference_support.write_fixture(FIXTURE, result)
        print("Wrote native raw-zone fixture; Rust consumer comparison still required.")
    elif FIXTURE.read_text() != result:
        raise RuntimeError("Pinned raw-zone fixture changed")
    else:
        print("PASS: pinned raw-zone, combiner and ordered-update fixture.")


if __name__ == "__main__":
    main()
