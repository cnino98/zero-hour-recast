#!/usr/bin/env python3
"""Compare actual Rust geometry/coverage with pinned PartitionManager bodies.

Storage/accessor and generously sized COI wrappers preserve ordered output;
candidate discovery, response, terrain and the scheduler are not run.
"""
from pathlib import Path
import reference_support
import importlib.util
import json
import os
import tempfile
import native_real_reference


PROJECT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("path_reference", PROJECT / "tests/reference/check-path-reference.py")
reference = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reference)
UPSTREAM = reference.UPSTREAM
SOURCE = "GeneralsMD/Code/GameEngine/Source/GameLogic/Object/PartitionManager.cpp"
GEOMETRY = "GeneralsMD/Code/GameEngine/Source/Common/System/Geometry.cpp"
GEOMETRY_HEADER = "GeneralsMD/Code/GameEngine/Include/Common/Geometry.h"
PARTITION_HEADER = "GeneralsMD/Code/GameEngine/Include/GameLogic/PartitionManager.h"
TRIG = "GeneralsMD/Code/GameEngine/Source/Common/System/Trig.cpp"
RUST = PROJECT / "crates/generals-sim/src/collision_geometry.rs"


def function(path, signature):
    source = (UPSTREAM / path).read_text()
    start = source.index(signature)
    while ";" in source[start:source.index("{", start)]:
        start = source.index(signature, start + len(signature))
    return source[start:source.index("\n}\n", start) + 3]


def cases():
    # Shape, raw major/minor/height. Cylinder minor is deliberately unequal:
    # the native close-enough shortcut reads it before the circle-box wrapper.
    shapes = [("Sphere", 5, 3, 20), ("Cylinder", 6, 2, 12), ("Box", 8, 2, 12)]
    positions = [
        [0, 0, 0], [3, 4, 0], [10, 0, 0], [0, 10, 0], [12, 0, 0],
        [12.0001, 0, 0], [0, 0, 12], [0, 0, 12.0001], [0, 0, -8],
        [5, 5, 16], [5, 5, -3], [9, 3, 3],
    ]
    result = []
    for a in shapes:
        for b in shapes:
            for angle in [0, 0.6, 1.5707964]:
                for index, position in enumerate(positions):
                    name = f"{a[0]}-{b[0]}/{angle}/{index}"
                    result.append((name, (a, [0, 0, 0], angle), (b, position, -angle)))
    # Native box corner testing is not SAT; this crossing has no contained
    # corner once the strict minimum-radius short-circuit stops applying.
    box = ("Box", 10, 1, 5)
    result.extend([
        ("crossed_box_coincident", (box, [0, 0, 0], 0), (box, [0, 0, 0], 1.5707964)),
        ("crossed_box_at_shortcut_limit", (box, [0, 0, 0], 0), (box, [1, 0, 0], 1.5707964)),
        ("sphere_touching_top", (("Sphere", 5, 1, 2), [0, 0, 17], 0), (shapes[1], [0, 0, 0], 0)),
        ("sphere_above_top", (("Sphere", 5, 1, 2), [0, 0, 17.0001], 0), (shapes[1], [0, 0, 0], 0)),
        ("sphere_touching_bottom", (("Sphere", 5, 1, 2), [0, 0, -5], 0), (shapes[1], [0, 0, 0], 0)),
        ("zero_radius_point", (("Cylinder", 0, 0, 0), [8, 0, 5], 0), (shapes[2], [0, 0, 0], 0)),
        ("large_translated_rotated", (box, [1000.125, -2200.75, 32], 0.37), (shapes[1], [1008.125, -2197.75, 35], -0.9)),
    ])
    return result


def coverage_cases():
    result = []
    # Includes the installed cell size 30 and smaller partitions at map edges.
    for size in [10, 30]:
        for shape in ["Sphere", "Cylinder", "Box"]:
            for radius, minor in [(0, 0), (2.5, 1), (5, 5), (30, 12), (80, 55)]:
                for index, (point, angle) in enumerate([
                    ([0, 0, 0], 0), ([size, size, 0], 0),
                    ([size - 0.0001, size + 0.0001, 0], 0.6),
                    ([-1, size, 0], -0.7), ([size * 8, size * 6, 0], 1.5707964),
                    ([size * 9, size * 7, 0], 0.2),
                ]):
                    for small in [False, True]:
                        result.append((f"coverage/{size}/{shape}/{radius}/{index}/{int(small)}",
                                       ((shape, radius, minor, 10), point, angle), small, size, [8, 6]))
    return result


def cpp_source(fixtures, fills):
    source = (UPSTREAM / SOURCE).read_text()
    copyright_header = source.split("*/", 1)[0] + "*/\n"
    wrappers = r'''
#include <cmath>
#include <iostream>
#include <algorithm>
#include <array>
#include <vector>
using Real = float;
using Bool = bool;
using Int = int;
using GeometryType = int;
constexpr GeometryType GEOMETRY_SPHERE=0, GEOMETRY_CYLINDER=1, GEOMETRY_BOX=2;
constexpr int GEOMETRY_FIRST=0, GEOMETRY_NUM_TYPES=3, KINDOF_NO_COLLIDE=1;
#define FALSE false
#define DEBUG_ASSERTCRASH(...)
#define DEBUG_CRASH(...)
#define __min std::min
template<class T> T sqr(T value) { return value * value; }
struct Coord2D { Real x, y; };
struct Coord3D { Real x, y, z; COORD3_LENGTH COORD3_NORMALIZE };
struct GeometryInfo {
    GeometryType m_type;
    Real m_majorRadius, m_minorRadius, m_height;
    GeometryType getGeomType() const { return m_type; }
    Real getMajorRadius() const { return m_majorRadius; }
    Real getMinorRadius() const { return m_minorRadius; }
    void setMajorRadius(Real value) { m_majorRadius=value; }
    void setMinorRadius(Real value) { m_minorRadius=value; }
    Real getMaxHeightAbovePosition() const;
    Real getMaxHeightBelowPosition() const;
};
struct CollideLocAndNormal { Coord3D loc, normal; };
struct Object {
    GeometryInfo geometry;
    Coord3D position;
    Real angle;
    bool isKindOf(int) const { return false; }
    const Coord3D* getPosition() const { return &position; }
    const GeometryInfo& getGeometryInfo() const { return geometry; }
    Real getOrientation() const { return angle; }
};
struct PartitionCell { Int x, y; };
struct PartitionData;
struct CellAndObjectIntersection {
    PartitionCell* cell=nullptr;
    PartitionCell* getCell() const { return cell; }
    void addCoverage(PartitionCell* value, PartitionData*) { cell=value; }
};
struct PartitionManager {
    Real m_cellSize, m_cellSizeInv;
    Int m_cellCountX, m_cellCountY;
    struct { Coord3D lo; } m_worldExtents{{0,0,0}};
    std::vector<PartitionCell> m_cells;
    PartitionManager(Real size, Int x, Int y)
      :m_cellSize(size),m_cellSizeInv((Real)(1.0 / size)),m_cellCountX(x),m_cellCountY(y) {
        for (Int cy=0;cy<y;++cy) for (Int cx=0;cx<x;++cx) m_cells.push_back({cx,cy});
    }
    Real getCellSize() { return m_cellSize; }
    void worldToCell(Real,Real,Int*,Int*) const;
    PartitionCell* getCellAt(Int,Int);
};
PartitionManager* ThePartitionManager=nullptr;
struct PartitionData {
    Object object;
    Int m_coiArrayCount=4096,m_coiInUseCount=0;
    std::vector<CellAndObjectIntersection> coi_storage=std::vector<CellAndObjectIntersection>(4096);
    CellAndObjectIntersection* m_coiArray=coi_storage.data();
    const Object* getObject() const { return &object; }
    Bool collidesWith(const PartitionData*, CollideLocAndNormal*) const;
    void addSubPixToCoverage(PartitionCell*);
    void doRectFill(Real,Real,Real,Real,Real);
    void doCircleFillPrecise(Real,Real,Real);
    void doSmallFill(Real,Real,Real);
};
'''
    coord = (UPSTREAM / reference.COORD3).read_text()
    start = coord.index("\tvoid normalize()")
    normalize = coord[start:coord.index("\n\t}\n", start) + 4]
    wrappers = wrappers.replace("COORD3_LENGTH", reference.original_line(reference.COORD3, "Real length() const"))
    wrappers = wrappers.replace("COORD3_NORMALIZE", normalize)
    start = source.index("struct CollideInfo\n")
    info = source[start:source.index("\n};", start) + 3]
    start = source.index("inline void projectCoord3D(")
    end = source.index("//-----------------------------------------------------------------------------\nstatic Bool distCalcProc_CenterAndCenter_2D(", start)
    collisions = source[start:end]
    start = source.index("static CollideTestProc theCollideTestProcs[] =")
    table = source[start:source.index("\n};", start) + 3]
    parts = [copyright_header, "// Pinned source: " + reference.COMMIT, native_real_reference.cpp(reference), wrappers, info,
             "using CollideTestProc = Bool (*)(const CollideInfo*, const CollideInfo*, CollideLocAndNormal*);",
             "static Bool xy_collideTest_Rect_Rect(const CollideInfo*, const CollideInfo*, CollideLocAndNormal*);",
             "static Bool xy_collideTest_Rect_Circle(const CollideInfo*, const CollideInfo*, CollideLocAndNormal*);",
             function(TRIG, "Real Sin(Real x)"), function(TRIG, "Real Cos(Real x)"),
             function(GEOMETRY, "Real GeometryInfo::getMaxHeightAbovePosition() const"),
             function(GEOMETRY, "Real GeometryInfo::getMaxHeightBelowPosition() const")]
    for signature in ["inline void vecDiff_2D(", "inline void vecDiff_3D(",
                      "inline Real calcSqrDist_2D(", "inline Real calcSqrDist_3D("]:
        parts.append(function(SOURCE, signature))
    parts.extend([collisions, table, function(SOURCE, "Bool PartitionData::collidesWith(")])
    for path, signature in [
        (PARTITION_HEADER, "inline void PartitionManager::worldToCell("),
        (PARTITION_HEADER, "inline PartitionCell *PartitionManager::getCellAt("),
        (SOURCE, "void PartitionData::addSubPixToCoverage("),
        (SOURCE, "void PartitionData::doRectFill("),
        (SOURCE, "static Bool doesCircleOverlapCell("),
        (SOURCE, "void PartitionData::doCircleFillPrecise("),
        (SOURCE, "void PartitionData::doSmallFill("),
    ]:
        parts.append(function(path, signature))
    parts.append("int main() {")
    parts.extend(f"static_assert(RETAIL_COMPATIBLE_{key} == {value});" for key, value in reference.PROFILE.items())
    for name, a, b in fixtures:
        parts.append("{")
        for variable, (shape, position, angle) in [("a", a), ("b", b)]:
            geometry = f"{{GEOMETRY_{shape[0].upper()},{float(shape[1])}f,{float(shape[2])}f,{float(shape[3])}f}}"
            parts.append(f"PartitionData {variable}{{{{{geometry},{reference.vector(position, '{', '}', 'f')},{float(angle)}f}}}};")
        parts.append(f"std::cout << {json.dumps(name)} << ':' << a.collidesWith(&b,nullptr) << ':' << b.collidesWith(&a,nullptr) << '\\n'; }}")
    for name, (shape, position, angle), small, size, dimensions in fills:
        parts.append("{")
        parts.append(f"PartitionManager manager({float(size)}f,{dimensions[0]},{dimensions[1]}); ThePartitionManager=&manager;")
        parts.append("PartitionData a{};")
        args = f"{float(position[0])}f,{float(position[1])}f,{float(shape[1])}f"
        if small:
            parts.append(f"a.doSmallFill({args});")
        elif shape[0] == "Box":
            parts.append(f"a.doRectFill({args},{float(shape[2])}f,{float(angle)}f);")
        else:
            parts.append(f"a.doCircleFillPrecise({args});")
        parts.append(f"std::cout << {json.dumps(name)};")
        parts.append("for(Int i=0;i<a.m_coiInUseCount;++i) { auto* cell=a.m_coiArray[i].getCell(); std::cout << ':' << cell->x << ',' << cell->y; }")
        parts.append("std::cout << '\\n'; }")
    return "\n".join(parts) + "\n}\n"


def rust_source(fixtures, fills):
    # Verbatim local source snapshots retain Rust's ordinary nested-module
    # resolution; #[path] on the outer file would change coverage.rs lookup.
    parts = [native_real_reference.rust(), "mod collision_geometry;",
             "use collision_geometry::{Body, Geometry, Shape, intersects, coverage};", "fn main() {"]
    for name, a, b in fixtures:
        parts.append("{")
        for variable, (shape, position, angle) in [("a", a), ("b", b)]:
            parts.append(f"let {variable}_geometry = Geometry {{ shape: Shape::{shape[0]}, major:{float(shape[1])}, minor:{float(shape[2])}, height:{float(shape[3])}, is_small:false }};")
            parts.append(f"let {variable} = Body {{ geometry:&{variable}_geometry, position:{reference.vector(position, '[', ']')}, angle:{float(angle)} }};")
        parts.append(f"println!(\"{{}}:{{}}:{{}}\", {json.dumps(name)}, u8::from(intersects(&a,&b)), u8::from(intersects(&b,&a))); }}")
    for name, (shape, position, angle), small, size, dimensions in fills:
        parts.append("{")
        parts.append(f"let geometry=Geometry {{shape:Shape::{shape[0]},major:{float(shape[1])},minor:{float(shape[2])},height:{float(shape[3])},is_small:{str(small).lower()}}};")
        parts.append(f"let body=Body {{geometry:&geometry,position:{reference.vector(position, '[', ']')},angle:{float(angle)}}};")
        parts.append(f"print!(\"{{}}\",{json.dumps(name)});")
        parts.append(f"for [x,y] in coverage(&body,body.geometry.is_small,{float(size)},[{dimensions[0]},{dimensions[1]}]) {{print!(\":{{x}},{{y}}\");}} println!(); }}")
    return "\n".join(parts) + "\n}\n"


def main():
    if reference.run(["git", "-C", str(UPSTREAM), "rev-parse", "HEAD"]) != reference.COMMIT:
        raise RuntimeError("Upstream commit changed; review collision extraction first")
    for path in [SOURCE, GEOMETRY, GEOMETRY_HEADER, PARTITION_HEADER, TRIG, reference.COORD3]:
        if reference.run(["git", "-C", str(UPSTREAM), "status", "--porcelain", "--", path]):
            raise RuntimeError(f"Reference source is modified: {path}")
    fixtures = cases()
    fills = coverage_cases()
    cache = reference_support.CACHE / "collision-reference"
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="run-", dir=cache) as directory:
        work = Path(directory)
        env = os.environ.copy()
        env.update(TMPDIR=str(work), XDG_CACHE_HOME=str(work / "xdg"), CLANG_MODULE_CACHE_PATH=str(work / "modules"))
        cpp, rust = work / "reference.cpp", work / "port.rs"
        cpp.write_text(cpp_source(fixtures, fills))
        rust.write_text(rust_source(fixtures, fills))
        # The standalone test exercises contact predicates and coverage. The
        # separate potential-contact wrapper has no caller in this harness.
        geometry = RUST.read_text()
        geometry = geometry.replace(native_real_reference.function(
            geometry, "pub(crate) fn would_collide("), "", 1)
        (work / "collision_geometry.rs").write_text(geometry)
        (work / "collision_geometry").mkdir()
        (work / "collision_geometry/coverage.rs").write_bytes((RUST.parent / "collision_geometry/coverage.rs").read_bytes())
        clang = reference_support.compiler("CXX", "clang++")

        rustc = reference_support.compiler("RUSTC", "rustc")
        flags = [f"-DRETAIL_COMPATIBLE_{key}={value}" for key, value in reference.PROFILE.items()]
        native, port = work / "reference", work / "port"
        reference.run([clang, "-std=c++20", "-Wall", "-Wextra", "-Werror", "-ffp-contract=off",
                       *flags, str(cpp), "-o", str(native)], env=env)
        reference.run([rustc, "--edition=2024", "-Dwarnings", str(rust), "-o", str(port)], env=env)
        original, translated = reference.run([str(native)], env=env), reference.run([str(port)], env=env)
        expected = len(fixtures) + len(fills)
        if len(original.splitlines()) != expected or len(translated.splitlines()) != expected:
            raise RuntimeError("Collision comparison did not emit every fixture")
        if original != translated:
            differences = [f"C++: {left}\nRust: {right}" for left, right in zip(original.splitlines(), translated.splitlines()) if left != right]
            raise RuntimeError("Collision mismatch:\n" + "\n".join(differences[:10]))
        print(f"PASS: {len(fixtures)} collision fixtures, {len(fixtures)*2} directed shape tests match original C++.")
        print(f"PASS: {len(fills)} ordered coverage fixtures match original C++ small, rectangle and precise-circle fills.")
        print("Compared source sphere/cylinder/rotated-box, Z admission and native intersection shortcuts.")
        print("Scope: verbatim current Rust files vs pinned native bodies; storage/accessor wrappers and ample COI capacity.")
        print("Candidate broadphase, NO_COLLIDE filtering, response/yielding and full-engine parity are not covered.")


if __name__ == "__main__":
    main()
