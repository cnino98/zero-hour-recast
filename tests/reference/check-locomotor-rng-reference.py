#!/usr/bin/env python3
"""Reproduce locomotor constructor RNG fixtures from the pinned native source.

The three random initialization statements and the complete logic RNG functions
are extracted unchanged. This checks initialization values and stream continuity,
not role selection, object creation order or the whole locomotor state machine.
"""
from pathlib import Path
import reference_support
import importlib.util
import os
import sys
import tempfile

sys.dont_write_bytecode = True
PROJECT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("reference", PROJECT / "tests/reference/check-path-reference.py")
reference = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reference)
RANDOM = "Core/GameEngine/Source/Common/RandomValue.cpp"
LOCOMOTOR = "GeneralsMD/Code/GameEngine/Source/GameLogic/Object/Locomotor.cpp"
MACROS = "Core/Libraries/Include/Precompiled/Precompiled/BaseMacros.h"
FIXTURE = PROJECT / "tests/reference/locomotor-rng-reference.txt"


def function(source, signature):
    start = source.index(signature)
    return source[start:source.index("\n}\n", start) + 3]


def native_source():
    random = (reference.UPSTREAM / RANDOM).read_text()
    locomotor = (reference.UPSTREAM / LOCOMOTOR).read_text()
    macros = (reference.UPSTREAM / MACROS).read_text()
    constructor = function(locomotor, "Locomotor::Locomotor(const LocomotorTemplate* tmpl)")
    start = constructor.index("\tm_angleOffset =")
    statements = constructor[start:constructor.index("\tm_donutTimer =", start)]
    pi = next(line for line in macros.splitlines() if line.startswith("#define PI "))
    multiplier = next(line for line in random.splitlines() if line.startswith("static const Real theMultFactor"))
    adc = next(line for line in random.splitlines() if line.startswith("#define ADC("))
    prefix = random.split("*/", 1)[0] + "*/\n"
    wrappers = r'''
#include <bit>
#include <climits>
#include <cstdint>
#include <iostream>
using Real = float;
using UnsignedInt = std::uint32_t;
using Int = std::int32_t;
#define RETAIL_COMPATIBLE_CRC 0
#define DEBUG_ASSERTCRASH(...)
#define GameLogicRandomValueReal(lo, hi) GetGameLogicRandomValueReal(lo, hi, __FILE__, __LINE__)
#define GameLogicRandomValue(lo, hi) GetGameLogicRandomValue(lo, hi, __FILE__, __LINE__)
static UnsignedInt theGameLogicSeed[6];
'''
    functions = "\n".join(function(random, signature) for signature in (
        "static void seedRandom(", "static UnsignedInt randomValue(",
        "Int GetGameLogicRandomValue(", "Real GetGameLogicRandomValueReal(",
    ))
    storage = r'''
struct Template { Real m_wanderLengthFactor; };
struct ConstructorRandomState {
    const Template* m_template;
    Real m_angleOffset, m_offsetIncrement;
    bool increasing = false;
    enum { OFFSET_INCREASING };
    void setFlag(int, bool value) { increasing = value; }
    explicit ConstructorRandomState(const Template* value): m_template(value) {
'''
    main = r'''
    }
};
int main() {
    for (UnsignedInt seed : {1234u, 0u, UINT_MAX, 0x12345678u}) {
        seedRandom(seed, theGameLogicSeed);
        for (Real length : {1.0f, 4.0f, 2.0f, 8.0f}) {
            Template value {length};
            ConstructorRandomState state(&value);
            std::cout << seed << ' ' << length << ' '
                      << std::bit_cast<UnsignedInt>(state.m_angleOffset) << ' '
                      << std::bit_cast<UnsignedInt>(state.m_offsetIncrement) << ' '
                      << state.increasing;
            for (UnsignedInt word : theGameLogicSeed) std::cout << ' ' << word;
            std::cout << '\n';
        }
    }
}
'''
    return prefix + wrappers + "\n".join((pi, multiplier, adc, functions, storage, statements, main))


def main():
    run = reference.run
    if run(["git", "-C", str(reference.UPSTREAM), "rev-parse", "HEAD"]) != reference.COMMIT:
        raise RuntimeError("Unexpected pinned source revision")
    if run(["git", "-C", str(reference.UPSTREAM), "status", "--porcelain", "--", RANDOM, LOCOMOTOR, MACROS]):
        raise RuntimeError("Reference source modified")
    cache = reference_support.CACHE / "locomotor-rng"
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="run-", dir=cache) as directory:
        work = Path(directory)
        env = dict(os.environ, TMPDIR=str(work), XDG_CACHE_HOME=str(work / "xdg"),
                   CLANG_MODULE_CACHE_PATH=str(work / "modules"))
        cpp, binary = work / "reference.cpp", work / "reference"
        cpp.write_text(native_source())
        clang = reference_support.compiler("CXX", "clang++")

        run([clang, "-std=c++20", "-Wall", "-Werror", "-ffp-contract=off",
             str(cpp), "-o", str(binary)], env=env)
        actual = run([str(binary)], env=env) + "\n"
    if len(actual.splitlines()) != 16:
        raise RuntimeError("Missing constructor fixtures")
    if {line.split()[4] for line in actual.splitlines()} != {"0", "1"}:
        raise RuntimeError("Constructor fixtures must exercise both wander directions")
    if "--write-fixture" in sys.argv:
        reference_support.write_fixture(FIXTURE, actual)
        print("Staged 16 native constructor RNG rows; Rust comparison remains required.")
    elif FIXTURE.read_text() != actual:
        raise RuntimeError("Native constructor RNG fixture changed")
    else:
        print("PASS: 16 pinned native constructor RNG states and six-word streams reproduced.")


if __name__ == "__main__":
    main()
