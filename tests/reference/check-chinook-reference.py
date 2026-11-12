#!/usr/bin/env python3
"""Compare the live Chinook lift primitive with the pinned C++ body, by float bits."""
from pathlib import Path
import reference_support
import importlib.util
import os
import tempfile

PROJECT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("turret", PROJECT / "tests/reference/check-turret-reference.py")
turret = importlib.util.module_from_spec(spec)
spec.loader.exec_module(turret)
reference = turret.reference
SOURCE = "GeneralsMD/Code/GameEngine/Source/GameLogic/Object/Locomotor.cpp"
RUST = PROJECT / "crates/generals-sim/src/skirmish/flight/physics.rs"


def cases():
    return [(height, velocity, target, damaged, precise, limit)
            for height, target in [(100.0, 0.0), (0.0, 100.0), (2.9, 0.0), (97.0, 100.0), (100.0, 100.0)]
            for velocity in [-5.0, -0.01, 0.0, 0.01, 5.0]
            for damaged in [False, True]
            for precise in [False, True]
            for limit in [1.0, 999999.0]]


def cpp_source(fixtures):
    result = r'''
#include <bit>
#include <cmath>
#include <cstdint>
#include <iostream>
using Real=float; using BodyDamageType=bool;
constexpr bool ULTRA_ACCURATE=true;
float sqr(float value) { return value*value; }
struct Global { float m_gravity=-64.0f/900.0f; } globals;
Global* TheGlobalData=&globals;
struct Object { bool damaged; Object* getBodyModule(){return this;} bool getDamageState(){return damaged;} };
struct Coord { float z; };
struct PhysicsBehavior { Coord velocity; const Coord* getVelocity(){return &velocity;} };
struct Template { float m_speedLimitZ; };
struct Locomotor {
    Template* m_template; bool precise;
    float getMaxLift(bool damaged){return damaged ? 80.0f/900.0f : 120.0f/900.0f;}
    bool getFlag(bool){return precise;}
    float calcLiftToUseAtPt(Object*,PhysicsBehavior*,float,float,float);
};
'''
    result += turret.function((reference.UPSTREAM / SOURCE).read_text(), "Real Locomotor::calcLiftToUseAtPt(")
    result += "\nint main(){\n"
    for height, velocity, target, damaged, precise, limit in fixtures:
        result += f"{{ Object obj{{{str(damaged).lower()}}}; PhysicsBehavior physics{{{{{velocity}f}}}}; Template data{{{limit}f}}; Locomotor loco{{&data,{str(precise).lower()}}}; std::cout<<std::bit_cast<uint32_t>(loco.calcLiftToUseAtPt(&obj,&physics,{height}f,0.0f,{target}f))<<'\\n'; }}\n"
    return result + "}\n"


def rust_source(fixtures):
    result = ""
    result += turret.function(RUST.read_text(), "pub(in crate::skirmish) fn lift(").replace("pub(in crate::skirmish) ", "")
    result += "\nfn main(){\n"
    for height, velocity, target, damaged, precise, limit in fixtures:
        result += f"{{println!(\"{{}}\",lift({height},{velocity},{target},{80.0 if damaged else 120.0}/900.0,-64.0/900.0,{limit},{str(precise).lower()}).to_bits());}}\n"
    return result + "}\n"


def main():
    if reference.run(["git", "-C", str(reference.UPSTREAM), "rev-parse", "HEAD"]) != reference.COMMIT:
        raise RuntimeError("Review changed source pin before comparing Chinook lift")
    if reference.run(["git", "-C", str(reference.UPSTREAM), "status", "--porcelain", "--", SOURCE]):
        raise RuntimeError("Modified source reference")
    fixtures = cases()
    cache = reference_support.CACHE / "chinook-reference"
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="run-", dir=cache) as directory:
        work = Path(directory)
        env = os.environ.copy()
        env.update(TMPDIR=str(work), XDG_CACHE_HOME=str(work / "xdg"), CLANG_MODULE_CACHE_PATH=str(work / "modules"))
        cpp, rust = work / "reference.cpp", work / "port.rs"
        cpp.write_text(cpp_source(fixtures))
        rust.write_text(rust_source(fixtures))
        clang = reference_support.compiler("CXX", "clang++")

        rustc = reference_support.compiler("RUSTC", "rustc")
        native, port = work / "reference", work / "port"
        reference.run([clang, "-std=c++20", "-Wall", "-Werror", "-ffp-contract=off", str(cpp), "-o", str(native)], env=env)
        reference.run([rustc, "--edition=2024", "-Dwarnings", str(rust), "-o", str(port)], env=env)
        original = reference.run([str(native)], env=env).splitlines()
        translated = reference.run([str(port)], env=env).splitlines()
        if len(original) != len(fixtures) or len(translated) != len(fixtures) or original != translated:
            differences = [f"case {i} {fixtures[i]}: C++ {left}; Rust {right}" for i, (left, right) in enumerate(zip(original, translated)) if left != right]
            raise RuntimeError("Chinook lift differs:\n" + "\n".join(differences[:10]))
        print(f"PASS: {len(fixtures)} Chinook lift cases match verbatim C++ float bits (landing/takeoff, braking, damage, precise-mode clamps, speed-limit branch).")
        print("This comparison covers the lift primitive; Rust scenario tests cover container/state integration.")


if __name__ == "__main__":
    main()
