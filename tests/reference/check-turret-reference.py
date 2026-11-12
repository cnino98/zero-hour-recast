#!/usr/bin/env python3
"""Compare live Rust turret yaw/pitch primitives with verbatim pinned C++ bodies.

Object model-condition callbacks are wrappers. Full AI state transitions, target
queries, rendering and the native engine scheduler are not run by this comparison.
"""
from pathlib import Path
import reference_support
import importlib.util
import json
import os
import tempfile

PROJECT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("reference", PROJECT / "tests/reference/check-path-reference.py")
reference = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reference)
SOURCE = "GeneralsMD/Code/GameEngine/Source/GameLogic/AI/TurretAI.cpp"
COMMON = "Core/GameEngine/Source/Common/System/GameCommon.cpp"
RUST = PROJECT / "crates/generals-sim/src/skirmish/turret.rs"


def function(source, signature):
    start = source.index(signature)
    opening = source.index("{", start)
    depth = 1
    end = opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


def cases():
    return [(current, desired, rate, modifier, threshold, pitch)
            for current, desired in [(0.0, 0.0), (0.0, 0.5), (0.5, 0.0),
                                     (3.1, -3.1), (-3.1, 3.1), (0.0, -3.1415927410125732)]
            for rate in [0.0, 0.01, 0.5]
            for modifier in [0.5, 1.0]
            for threshold in [0.0, 0.035]
            for pitch in [False, True]]


def cpp_source(fixtures):
    source = (reference.UPSTREAM / SOURCE).read_text()
    common = (reference.UPSTREAM / COMMON).read_text()
    result = r'''
#include <bit>
#include <cmath>
#include <cstdint>
#include <iostream>
using Real=float; using Bool=bool;
constexpr float PI=3.14159265358979323846f;
#define DEBUG_ASSERTCRASH(...)
#define _isnan std::isnan
constexpr int MODELCONDITION_TURRET_ROTATE=1;
struct Object {
    bool rotating=false;
    void clearModelConditionState(int) { rotating=false; }
    void setModelConditionState(int) { rotating=true; }
    void reactToTurretChange(int,float,float) {}
};
struct TurretAI {
    Object owner;
    float m_angle=0, m_pitch=0, rate=0;
    int m_whichTurret=0;
    bool m_playRotSound=false,m_playPitchSound=false;
    Object* getOwner() { return &owner; }
    float getTurretAngle() { return m_angle; }
    float getTurretPitch() { return m_pitch; }
    float getTurnRate() { return rate; }
    float getPitchRate() { return rate; }
    bool isAllowsPitch() { return true; }
    bool friend_turnTowardsAngle(float,float,float);
    bool friend_turnTowardsPitch(float,float);
};
'''
    result += function(common, "Real normalizeAngle(Real angle)") + "\n"
    result += function(source, "Bool TurretAI::friend_turnTowardsAngle(") + "\n"
    result += function(source, "Bool TurretAI::friend_turnTowardsPitch(") + "\n"
    result += "int main() {\n"
    for current, desired, rate, modifier, threshold, pitch in fixtures:
        values = [f"{value:.17g}f" if "." in f"{value:.17g}" else f"{value:.17g}.0f" for value in [current, desired, rate, modifier, threshold]]
        current, desired, rate, modifier, threshold = values
        call = f"t.friend_turnTowardsPitch({desired},{modifier})" if pitch else f"t.friend_turnTowardsAngle({desired},{modifier},{threshold})"
        member = "m_pitch" if pitch else "m_angle"
        result += f'{{ TurretAI t; t.m_angle={current}; t.m_pitch={current}; t.rate={rate}; bool ready={call}; std::cout<<std::bit_cast<uint32_t>(t.{member})<<":"<<ready<<":"<<t.owner.rotating<<"\\n"; }}\n'
    return result + "}\n"


def rust_source(fixtures):
    source = RUST.read_text()
    result = "use std::f32::consts::{PI,TAU};\nstruct State { angle:f32,pitch:f32,rotating:bool }\nstruct Rules { pitch_rate:f32,allows_pitch:bool }\n"
    result += function(source, "fn normalize(") + "\nimpl State {\n"
    result += function(source, "fn turn(") + "\n"
    result += function(source, "fn pitch(") + "\n}\nfn main() {\n"
    for current, desired, rate, modifier, threshold, pitch in fixtures:
        call = f"t.pitch({desired},&Rules{{pitch_rate:{rate},allows_pitch:true}},{modifier})" if pitch else f"t.turn({desired},{rate}*{modifier},{threshold})"
        member = "pitch" if pitch else "angle"
        result += f'{{ let mut t=State{{angle:{current},pitch:{current},rotating:false}}; let ready={call}; println!("{{}}:{{}}:{{}}",t.{member}.to_bits(),u8::from(ready),u8::from(t.rotating)); }}\n'
    return result + "}\n"


def main():
    if reference.run(["git", "-C", str(reference.UPSTREAM), "rev-parse", "HEAD"]) != reference.COMMIT:
        raise RuntimeError("Review changed upstream baseline before turret comparison")
    for path in [SOURCE, COMMON]:
        if reference.run(["git", "-C", str(reference.UPSTREAM), "status", "--porcelain", "--", path]):
            raise RuntimeError(f"Modified reference source: {path}")
    fixtures = cases()
    cache = reference_support.CACHE / "turret-reference"
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
        reference.run([clang,"-std=c++20","-Wall","-Wextra","-Werror","-ffp-contract=off",str(cpp),"-o",str(native)],env=env)
        reference.run([rustc,"--edition=2024","-Dwarnings",str(rust),"-o",str(port)],env=env)
        original, translated = reference.run([str(native)],env=env), reference.run([str(port)],env=env)
        if len(original.splitlines()) != len(fixtures) or original != translated:
            differences = [f"fixture {i}: C++ {a}; Rust {b}" for i,(a,b) in enumerate(zip(original.splitlines(),translated.splitlines())) if a != b]
            raise RuntimeError("Turret primitive mismatch:\n" + "\n".join(differences[:10]))
        print(f"PASS: {len(fixtures)} turret yaw/pitch fixtures match verbatim C++ angle bits, alignment and rotation condition.")
        print("Includes zero rate, exact steps, half-speed recentering, wrap boundaries and original non-normalized alignment comparison.")


if __name__ == "__main__":
    main()
