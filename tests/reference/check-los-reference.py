#!/usr/bin/env python3
"""Compare live Hacker approach LOS against the original Bresenham implementation."""
from pathlib import Path
import reference_support
import importlib.util
import os
import tempfile
import native_real_reference

PROJECT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("turret", PROJECT / "tests/reference/check-turret-reference.py")
turret = importlib.util.module_from_spec(spec)
spec.loader.exec_module(turret)
reference = turret.reference
SOURCE = "Core/GameEngineDevice/Source/W3DDevice/GameClient/BaseHeightMap.cpp"
RUST = PROJECT / "crates/generals-sim/src/skirmish/projectiles/terrain.rs"


def fixtures():
    points = [(-25.0, -25.0), (-10.0, 30.0), (0.0, 0.0), (9.9, 20.0),
              (30.0, 9.9), (50.0, 70.0), (89.0, 89.0), (120.0, 10.0)]
    return [(pattern, (a[0], a[1], z), (b[0], b[1], w))
            for pattern in range(4) for a in points for b in points
            for z, w in [(0.0, 0.0), (20.0, 20.0), (20.0, 90.0),
                         (90.0, 20.0), (159.0, 161.0)]]


def heightmap(pattern):
    return [(0 if pattern == 0 else 32 if pattern == 1 else
             128 if (x == 5 if pattern == 2 else (x + y) % 5 == 0) else 0)
            for y in range(12) for x in range(12)]


def cpp_source(cases):
    text = native_real_reference.cpp(reference) + r'''
#include <algorithm>
#include <cmath>
#include <iostream>
using Real=float; using Int=int; using Bool=bool; using UnsignedByte=unsigned char;
#define MAP_XY_FACTOR 10.0f
#define MAP_HEIGHT_SCALE 0.625f
#define __max(a,b) std::max(float(a),float(b))
struct Coord3D {float x,y,z;};
struct WorldHeightMap {
    unsigned char data[144];
    int getBorderSizeInline() { return 2; }
    const unsigned char* getDataPtr(){return data;}
    int getXExtent(){return 12;} int getYExtent(){return 12;}
};
struct Visual { WorldHeightMap* getLogicHeightMap(){return nullptr;} };
Visual* TheTerrainVisual=nullptr;
struct BaseHeightMapRenderObjClass {
    WorldHeightMap* m_map;
    float getMaxHeight() const {return *std::max_element(m_map->data,m_map->data+144)*0.625f;}
    bool isClearLineOfSight(const Coord3D&,const Coord3D&) const;
};
'''
    text += turret.function((reference.UPSTREAM / SOURCE).read_text(), "Bool BaseHeightMapRenderObjClass::isClearLineOfSight(")
    text += "\nint main(){\n"
    for pattern in range(4):
        data = ",".join(map(str, heightmap(pattern)))
        text += f"WorldHeightMap map{pattern}{{{{{data}}}}}; BaseHeightMapRenderObjClass view{pattern}{{&map{pattern}}};\n"
    for pattern, start, end in cases:
        a = ",".join(f"{value}f" for value in start)
        b = ",".join(f"{value}f" for value in end)
        text += f"std::cout<<view{pattern}.isClearLineOfSight({{{a}}},{{{b}}})<<'\\n';\n"
    return text + "}\n"


def rust_source(cases):
    source = RUST.read_text()
    text = native_real_reference.rust() + "struct Terrain {width:u16,height:u16,origin:[f32;2],samples:Vec<f32>}\nimpl Terrain {\n"
    text += turret.function(source, "pub(in crate::skirmish) fn line_of_sight(").replace("pub(in crate::skirmish) ", "") + "}\n"
    text += turret.function(source, "pub(super) fn bounded_index(").replace("pub(super) ", "")
    text += turret.function(source, "fn signed_index(")
    text += "\nfn main(){\n"
    for pattern in range(4):
        data = ",".join(str(value * 0.625) for value in heightmap(pattern))
        text += f"let map{pattern}=Terrain{{width:12,height:12,origin:[-20.0;2],samples:vec![{data}]}};\n"
    for pattern, start, end in cases:
        a = ",".join(map(str, start)); b = ",".join(map(str, end))
        text += f'println!("{{}}",u8::from(map{pattern}.line_of_sight([{a}],[{b}])));\n'
    return text + "}\n"


def main():
    if reference.run(["git", "-C", str(reference.UPSTREAM), "rev-parse", "HEAD"]) != reference.COMMIT:
        raise RuntimeError("Review changed source pin before comparing terrain LOS")
    if reference.run(["git", "-C", str(reference.UPSTREAM), "status", "--porcelain", "--", SOURCE]):
        raise RuntimeError("Modified source reference")
    cases = fixtures()
    cache = reference_support.CACHE / "los-reference"
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="run-", dir=cache) as directory:
        work = Path(directory)
        env = os.environ.copy()
        env.update(TMPDIR=str(work), XDG_CACHE_HOME=str(work / "xdg"), CLANG_MODULE_CACHE_PATH=str(work / "modules"))
        cpp, rust = work / "reference.cpp", work / "port.rs"
        cpp.write_text(cpp_source(cases)); rust.write_text(rust_source(cases))
        clang = reference_support.compiler("CXX", "clang++")

        rustc = reference_support.compiler("RUSTC", "rustc")
        native, port = work / "reference", work / "port"
        reference.run([clang, "-std=c++20", "-ffp-contract=off", str(cpp), "-o", str(native)], env=env)
        reference.run([rustc, "--edition=2024", "-Dwarnings", str(rust), "-o", str(port)], env=env)
        original = reference.run([str(native)], env=env).splitlines()
        translated = reference.run([str(port)], env=env).splitlines()
        if len(original) != len(cases) or original != translated:
            differences = [f"{cases[i]}: C++ {left}, Rust {right}" for i, (left, right) in enumerate(zip(original, translated)) if left != right]
            raise RuntimeError("Terrain LOS differs:\n" + "\n".join(differences[:10]))
        print(f"PASS: {len(cases)} terrain LOS fixtures match the extracted original DO_BRESENHAM body: flat/ridge/checkered terrain, border/off-map/reversed/same-cell lines and uphill/downhill visibility.")
        print("Scope: this comparison covers the exact heightmap line predicate, not the caller endpoint construction or full special-ability scheduler.")


if __name__ == "__main__":
    main()
