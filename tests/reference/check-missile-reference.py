#!/usr/bin/env python3
"""Compare native missile steering primitives with the actual Rust functions.

The original C++ bodies are extracted verbatim. Object/physics storage and basic
vector operators are wrappers. No missile state machine or scheduler is run.
"""
from pathlib import Path
import reference_support
import importlib.util
import json
import os
import struct
import tempfile

PROJECT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("reference", PROJECT / "tests/reference/check-path-reference.py")
reference = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reference)
UPSTREAM = reference.UPSTREAM
LOCOMOTOR = "GeneralsMD/Code/GameEngine/Source/GameLogic/Object/Locomotor.cpp"
VECTOR = "Core/Libraries/Source/WWVegas/WWMath/vector3.h"
MATRIX = "Core/Libraries/Source/WWVegas/WWMath/matrix3d.h"
MATRIX_CPP = "Core/Libraries/Source/WWVegas/WWMath/matrix3d.cpp"
WWMATH = "Core/Libraries/Source/WWVegas/WWMath/wwmath.h"
TRIG = "GeneralsMD/Code/GameEngine/Source/Common/System/Trig.cpp"
RUST = PROJECT / "crates/generals-sim/src/skirmish/projectiles/missile/thrust.rs"


def function(path, signature):
    source = (UPSTREAM / path).read_text()
    start = source.index(signature)
    return source[start:source.index("\n}\n", start) + 3]


def f32(value):
    return struct.unpack("<f", struct.pack("<f", value))[0]


def fixtures():
    frame = f32(1.0 / 30.0)
    gravity = f32(-64.0 * f32(frame * frame))
    acceleration = f32(675.0 * f32(frame * frame))
    launch_speed = f32(150.0 * frame)
    turn_rate = f32(100.0 * f32(frame * f32(f32(3.141592653589793) / 180.0)))
    cases = []
    for angle in [0.0, 0.0009, 0.001, turn_rate, f32(25.0 * f32(f32(3.141592653589793) / 180.0)), f32(45.0 * f32(f32(3.141592653589793) / 180.0)), -0.5]:
        for name, current, goal in [
            ("aligned", [1, 0, 0], [2, 0, 0]),
            ("right_angle", [1, 0, 0], [0, 1, 0]),
            ("opposite", [1, 0, 0], [-1, 0, 0]),
            ("vertical", [0, 0, 1], [1, 0, -0.2]),
            ("oblique", [0.9, 0.23, 0.17], [13, -19, 27]),
            ("zero_goal", [1, 0, 0], [0, 0, 0]),
            ("tiny_turn", [2, 0, 0], [2, 0.00001, 0]),
        ]:
            cases.append((f"rotation_{name}_{angle}", "rotate", angle, current, goal, 0.0))
    for acceleration in [0.0, acceleration, -acceleration, launch_speed]:
        for name, velocity, goal in [
            ("rest", [0, 0, 0], [200, 0, 0]),
            ("launch", [launch_speed, 0, gravity], [200, 0, 0]),
            ("crossing", [3, 4, 0.5], [100, 80, 20]),
            ("overshot", [7.5, 0, 0], [-10, -20, 0]),
            ("at_goal", [5, 0, 0], [0, 0, 0]),
            ("near_goal", [0, 0, 0], [0.01, 0, 0]),
            ("denominator_zero", [0, 0, acceleration], [20, 30, 40]),
        ]:
            case_gravity = 0.0 if name == "denominator_zero" else gravity
            cases.append((f"thrust_{name}_{acceleration}", "thrust", acceleration, velocity, goal, case_gravity))
    return cases


def cpp_source(cases):
    wrappers = r'''
#include <bit>
#include <cmath>
#include <cstdint>
#include <iostream>
#include <cassert>
using Real=float; using Bool=bool;
#define WWINLINE inline
#define FALSE false
#define TRUE true
template<class T> T sqr(T value) { return value*value; }
template<class T> T clamp(T low,T value,T high) { return value<low?low:(value>high?high:value); }
struct WWMath { static float Inv_Sqrt(float); };
struct Vector3 {
    float X=0,Y=0,Z=0;
    Vector3()=default;
    Vector3(float x,float y,float z):X(x),Y(y),Z(z){}
    float operator[](int i) const { return i==0?X:(i==1?Y:Z); }
    Vector3 operator-(const Vector3& r) const { return {X-r.X,Y-r.Y,Z-r.Z}; }
    float Length2() const;
    static float Dot_Product(const Vector3&,const Vector3&);
    static void Normalized_Cross_Product(const Vector3&,const Vector3&,Vector3*);
    void Normalize();
};
struct Row4 {
    float values[4];
    float operator[](int i) const { return values[i]; }
    void Set(float x,float y,float z,float w) { values[0]=x;values[1]=y;values[2]=z;values[3]=w; }
};
struct Matrix3D {
    Row4 Row[3];
    Vector3 forward;
    Matrix3D(const Vector3& axis,float angle) { Set(axis,angle); }
    explicit Matrix3D(Vector3 direction):forward(direction){}
    void Set(const Vector3&,float);
    void Set(const Vector3&,float,float);
    Vector3 Rotate_Vector(const Vector3&) const;
    Vector3 Get_X_Vector() const { return forward; }
};
struct Coord3D { float x,y,z; };
struct Object {
    Coord3D position; Matrix3D transform;
    const Coord3D* getPosition() const { return &position; }
    const Matrix3D* getTransformMatrix() const { return &transform; }
};
struct PhysicsBehavior {
    Coord3D velocity;
    const Coord3D* getVelocity() const { return &velocity; }
};
struct GlobalData { float m_gravity; } data;
GlobalData* TheGlobalData=&data;
'''
    parts = [wrappers]
    for path, signature in [
        (WWMATH, "WWINLINE float WWMath::Inv_Sqrt(float val)"),
        (VECTOR, "WWINLINE float Vector3::Length2() const"),
        (VECTOR, "WWINLINE float Vector3::Dot_Product("),
        (VECTOR, "WWINLINE void Vector3::Normalize()"),
        (VECTOR, "WWINLINE void Vector3::Normalized_Cross_Product("),
        (MATRIX, "WWINLINE void Matrix3D::Set(const Vector3 & axis,float angle)"),
        (MATRIX, "WWINLINE void Matrix3D::Set(const Vector3 & axis,float s,float c)"),
        (MATRIX_CPP, "Vector3 Matrix3D::Rotate_Vector("),
        (TRIG, "Real ACos(Real x)"),
        (LOCOMOTOR, "inline Bool isNearlyZero(Real a)"),
        (LOCOMOTOR, "static Real tryToRotateVector3D("),
        (LOCOMOTOR, "static void calcDirectionToApplyThrust("),
    ]:
        parts.append(function(path, signature))
    parts.append("int main() {")
    for name, kind, value, first, second, gravity in cases:
        v1 = reference.vector(first, "{", "}", "f")
        v2 = reference.vector(second, "{", "}", "f")
        parts.append(f'{{ Vector3 result; Real angle=0; std::cout << {json.dumps(name)};')
        if kind == "rotate":
            parts.append(f"angle=tryToRotateVector3D({value}f,Vector3{v1},Vector3{v2},result);")
        else:
            parts.append(f"Object object{{{{0,0,0}},Matrix3D(Vector3{{1,0,0}})}}; PhysicsBehavior physics{{{v1}}}; data.m_gravity={gravity}f; calcDirectionToApplyThrust(&object,&physics,Coord3D{v2},{value}f,result);")
        parts.append("for(float value:{result.X,result.Y,result.Z,angle}) std::cout << ':' << std::bit_cast<std::uint32_t>(value); std::cout << '\\n'; }")
    return "\n".join(parts) + "\n}\n"


def rust_source(cases):
    source = (PROJECT / "crates/generals-sim/src/skirmish/projectiles.rs").read_text()
    start = source.index("fn direction_axes(")
    axes = source[start:source.index("\n}\n", start) + 3]
    wrappers = r'''
mod skirmish { pub mod projectiles {
AXES
pub struct Motion { position:[f32;3],velocity:[f32;3],axes:[[f32;3];3] }
impl Motion { fn motive_force(&mut self,_force:[f32;3],_rules:&missile::Physics,_frame:u32) {} }
pub mod missile {
    use super::Motion;
    const IS_BRAKING:u32=1;
    const PRECISE_Z_POS:u32=1 << 3;
    pub(super) struct Physics { mass:f32, gravity:f32 }
    struct ThrustRules { max_speed:f32,min_speed:f32,acceleration:f32,braking:f32,
        turn_rate:f32,max_thrust_angle:f32,preferred_height:f32,height_damping:f32,airborne_motive:bool }
    struct MissileRules { physics:Physics,thrust:ThrustRules }
    struct MoveGoal { maintain:Option<[f32;3]> }
    struct Missile { goal:MoveGoal, maximum_speed:f32,acceleration_cap:f32,turn_cap:f32,locomotor_flags:u32 }
    mod thrust {
        include!(RUST_PATH);
        pub fn report(kind:bool,value:f32,first:[f32;3],second:[f32;3],gravity:f32) {
            let _=Missile::move_towards;
            let (result,angle)=if kind { rotate(value,first,second) } else {
                let motion=super::Motion {position:[0.0;3],velocity:first,axes:[[1.0,0.0,0.0],[0.0,1.0,0.0],[0.0,0.0,1.0]]};
                (Missile::thrust_direction(&motion,second,value,gravity),0.0)
            };
            for value in [result[0],result[1],result[2],angle] { print!(":{}",value.to_bits()); }
            println!();
        }
    }
    pub fn report(kind:bool,value:f32,first:[f32;3],second:[f32;3],gravity:f32) { thrust::report(kind,value,first,second,gravity); }
}
}}
fn main() {
'''
    parts = [wrappers.replace("AXES", axes).replace("RUST_PATH", json.dumps(str(RUST)))]
    for name, kind, value, first, second, gravity in cases:
        a = reference.vector(first, "[", "]")
        b = reference.vector(second, "[", "]")
        parts.append(f'print!("{{}}",{json.dumps(name)}); skirmish::projectiles::missile::report({str(kind == "rotate").lower()},{value},{a},{b},{gravity});')
    return "\n".join(parts) + "\n}\n"


def main():
    if reference.run(["git", "-C", str(UPSTREAM), "rev-parse", "HEAD"]) != reference.COMMIT:
        raise RuntimeError("Upstream commit changed; review missile extraction first")
    for path in [LOCOMOTOR,VECTOR,MATRIX,MATRIX_CPP,WWMATH,TRIG]:
        if reference.run(["git", "-C", str(UPSTREAM), "status", "--porcelain", "--", path]):
            raise RuntimeError(f"Reference source modified: {path}")
    cases = fixtures()
    cache = reference_support.CACHE / "missile-reference"
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="run-", dir=cache) as directory:
        work = Path(directory)
        env = os.environ.copy()
        env.update(TMPDIR=str(work),XDG_CACHE_HOME=str(work / "xdg"),CLANG_MODULE_CACHE_PATH=str(work / "modules"))
        cpp, rust = work / "reference.cpp", work / "port.rs"
        cpp.write_text(cpp_source(cases))
        rust.write_text(rust_source(cases))
        clang = reference_support.compiler("CXX", "clang++")

        rustc = reference_support.compiler("RUSTC", "rustc")
        native, port = work / "reference", work / "port"
        reference.run([clang,"-std=c++20","-Wall","-Wextra","-Werror","-DNDEBUG","-ffp-contract=off",str(cpp),"-o",str(native)],env=env)
        reference.run([rustc,"--edition=2024","-Dwarnings",str(rust),"-o",str(port)],env=env)
        original, translated = reference.run([str(native)],env=env), reference.run([str(port)],env=env)
        if len(original.splitlines()) != len(cases) or len(translated.splitlines()) != len(cases):
            raise RuntimeError("Missile comparison omitted fixtures")
        if original != translated:
            differences = [f"C++: {a}\nRust: {b}" for a,b in zip(original.splitlines(),translated.splitlines()) if a != b]
            raise RuntimeError("Missile primitive mismatch:\n" + "\n".join(differences[:10]))
        print(f"PASS: {len(cases)} missile steering fixtures match verbatim C++ in all four f32 outputs.")
        print("Actual Rust rotate/thrust-direction functions; original vector normalization, axis rotation and quadratic interception.")
        print("Scope excludes whole THRUST force output, state transitions, target lookup, physics, contacts and engine scheduling.")


if __name__ == "__main__":
    main()
