#!/usr/bin/env python3
"""Compare native static-footprint rasterization with the production Rust helper.

Extracts AIPathfind's shape switch verbatim, including its five-unit BOX
sampling and circle arithmetic. Grid/cell storage and geometry accessors are
wrappers; obstacle admission, zone rebuilding and path search are not exercised.
Generated programs and compiler caches stay in the external test output directory.
"""
from pathlib import Path
import reference_support
import importlib.util
import json
import os
import random
import struct
import sys
import tempfile
import native_real_reference


PROJECT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location(
    "path_service_reference", PROJECT / "tests/reference/check-path-service-reference.py"
)
service = importlib.util.module_from_spec(spec)
spec.loader.exec_module(service)
reference = service.reference
SOURCE = "Core/GameEngine/Source/GameLogic/AI/AIPathfind.cpp"
TRIG = "GeneralsMD/Code/GameEngine/Source/Common/System/Trig.cpp"
RASTER = "crates/generals-sim/src/skirmish/movement/obstacles/raster.rs"
MOVEMENT = "crates/generals-sim/src/skirmish/movement.rs"
GRID = "crates/generals-sim/src/grid.rs"
SHAPE = "crates/generals-sim/src/collision_geometry.rs"
FOOTPRINT = "crates/generals-sim/src/skirmish.rs"


def bits(value):
    return struct.unpack("<I", struct.pack("<f", value))[0]


def adjacent(value, offset):
    return struct.unpack("<f", struct.pack("<I", bits(value) + offset))[0]


def cases():
    result = []

    def add(name, shape, position, angle, major, minor, extent=(256, 256)):
        result.append((name, shape, [bits(v) for v in (*position, angle, major, minor)], extent))

    add("arms_dealer_route_cell_83_211", "Box", [775, 2145], -2.3561945, 40, 62)
    for shape in ["Box", "Sphere", "Cylinder"]:
        add(f"last_column/{shape}", shape, [2550, 100], 0, 10, 10)
        add(f"last_row/{shape}", shape, [100, 2550], 0, 10, 10)
    angles = [0, 0.2, -0.2, 0.7853982, 1.5707964, -2.3561945, 3.1415927]
    radii = [(0, 0), (0.01, 0.02), (2.5, 2.5), (5, 12), (26, 26), (40, 62), (80, 55)]
    centers = [[0, 0], [9.5, 9.5], [50, 50], [775, 2145], [2559.5, 2559.5], [-5, 18]]
    for shape in ["Box", "Sphere", "Cylinder"]:
        for major, minor in radii:
            for center_index, position in enumerate(centers):
                for angle_index, angle in enumerate(angles if shape == "Box" else [0, -2.3561945]):
                    add(f"grid/{shape}/{major}/{center_index}/{angle_index}", shape,
                        position, angle, major, minor)
    for shape in ["Box", "Cylinder", "Sphere"]:
        for offset in range(-2, 3):
            for value in [0.5, 9.5, 10.0, 49.5, 50.0, 2559.5]:
                add(f"center_boundary/{shape}/{value}/{offset}", shape,
                    [adjacent(value, offset), 50], 0, 10, 10)
            for radius in [2.5, 5.0, 10.0, 12.0, 40.0, 62.0]:
                add(f"radius_boundary/{shape}/{radius}/{offset}", shape,
                    [50, 50], 0.7853982, adjacent(radius, offset), 12)
    rng = random.Random(4372)
    for index in range(180):
        add(f"varied/{index}", ["Box", "Sphere", "Cylinder"][index % 3],
            [rng.uniform(-80, 2600), rng.uniform(-80, 2600)],
            rng.uniform(-3.1415927, 3.1415927), rng.uniform(0, 90), rng.uniform(0, 90),
            (256, 240))
    return result


def cpp_source(fixtures):
    source = (reference.UPSTREAM / SOURCE).read_text()
    native = service.extract(source, "void Pathfinder::internal_classifyObjectFootprint(")
    switch = "switch(obj->getGeometryInfo().getGeomType())"
    # Preserve the function's position/cellBounds setup and complete switch.
    # The remainder updates zones/occupant callbacks after rasterization.
    raster = native[:native.index(switch)] + service.extract(native, switch) + "\n}\n"
    header = source.split("*/", 1)[0] + "*/\n"
    wrappers = r'''
#include <bit>
#include <cmath>
#include <cstdint>
#include <iostream>
#include <vector>
using Real=float; using Int=int; using Bool=bool;
constexpr int GEOMETRY_SPHERE=0, GEOMETRY_CYLINDER=1, GEOMETRY_BOX=2;
constexpr float PATHFIND_CELL_SIZE_F=10.0f;
#define RTS_ZEROHOUR 1
#define RETAIL_COMPATIBLE_PATHFINDING 0
struct Coord3D { Real x,y,z; };
struct Coord2D { Real x,y; };
struct ICoord2D { Int x,y; };
struct IRegion2D { ICoord2D lo,hi; };
struct Geometry {
    int shape; Real major,minor;
    int getGeomType() const { return shape; }
    Real getMajorRadius() const { return major; }
    Real getMinorRadius() const { return minor; }
};
struct Object {
    Coord3D position; Real angle; Geometry geometry;
    const Coord3D* getPosition() const { return &position; }
    Real getOrientation() const { return angle; }
    const Geometry& getGeometryInfo() const { return geometry; }
};
struct Cell {
    bool occupied=false;
    bool setTypeAsObstacle(Object*,bool,ICoord2D) {
        bool changed=!occupied; occupied=true; return changed;
    }
    bool removeObstacle(Object*) { bool changed=occupied; occupied=false; return changed; }
    void setZone(int) {}
};
struct PathfindZoneManager { static constexpr int UNINITIALIZED_ZONE=0; };
struct Zones { void markZonesDirty() {} };
struct Pathfinder {
    IRegion2D m_extent;
    Zones m_zoneManager;
    std::vector<std::vector<Cell>> m_map;
    // Pathfinder::newMap stores an inclusive high cell index, then allocates hi+1.
    // The extracted classifier deliberately uses strict comparisons against hi.
    Pathfinder(int width,int height)
      :m_extent{{0,0},{width-1,height-1}},m_map(width,std::vector<Cell>(height)) {}
    void internal_classifyObjectFootprint(Object*,Bool);
    void print(const char* name) const {
        std::size_t count=0;
        for(const auto& column:m_map) for(const auto& cell:column) count+=cell.occupied;
        std::cout << name << ' ' << count;
        for(std::size_t x=0;x<m_map.size();++x) for(std::size_t y=0;y<m_map[x].size();++y)
            if(m_map[x][y].occupied) std::cout << ' ' << x << ',' << y;
        std::cout << '\n';
    }
};
'''
    trig = (reference.UPSTREAM / TRIG).read_text()
    parts = [header, native_real_reference.cpp(reference), wrappers, service.extract(trig, "Real Sin(Real x)"),
             service.extract(trig, "Real Cos(Real x)"), raster, "int main() {"]
    for name, shape, values, (width, height) in fixtures:
        x, y, angle, major, minor = [f"std::bit_cast<float>(std::uint32_t({v}))" for v in values]
        parts.append(
            f'{{ Pathfinder path({width},{height}); Object object{{{{{x},{y},0}},'
            f'{angle},{{GEOMETRY_{shape.upper()},{major},{minor}}}}}; '
            f'path.internal_classifyObjectFootprint(&object,true); path.print({json.dumps(name)}); }}'
        )
    return "\n".join(parts) + "\n}\n"


def rust_source(fixtures):
    grid = (PROJECT / GRID).read_text()
    cell_start = grid.index("pub struct Cell")
    cell = grid[grid.rfind("#[derive", 0, cell_start):cell_start] + service.extract(grid, "pub struct Cell")
    accessors = "\n".join(service.extract(grid, f"pub(crate) const fn {name}(")
                          for name in ["width", "height"])
    shape = service.extract((PROJECT / SHAPE).read_text(), "pub enum Shape")
    footprint = service.extract((PROJECT / FOOTPRINT).read_text(), "pub struct Footprint")
    coordinate = service.extract((PROJECT / MOVEMENT).read_text(), "pub(super) fn grid_coordinate(")
    raster = (PROJECT / RASTER).read_text()
    # This harness owns the shape classifier; the separate fence callback is
    # covered by its map-world tests, not the extracted switch below.
    raster = raster.replace(service.extract(
        raster, "pub(in crate::skirmish) fn fence_cells("), "", 1)
    wrappers = r'''
// Only the numeric trait boundary is supplied locally. All tested conversions
// are finite and within i32/u32 range, as required by the production validator.
extern crate self as num_traits;
pub trait ToPrimitive {
    fn to_i32(&self) -> Option<i32> { None }
    fn to_u32(&self) -> Option<u32> { None }
    fn to_f32(&self) -> Option<f32> { None }
}
impl ToPrimitive for f32 {
    fn to_i32(&self) -> Option<i32> { Some(*self as i32) }
    fn to_u32(&self) -> Option<u32> { Some(*self as u32) }
}
impl ToPrimitive for i32 {
    fn to_f32(&self) -> Option<f32> { Some(*self as f32) }
}
struct Grid { width:u16,height:u16 }
'''
    result = [native_real_reference.rust(), wrappers, cell, "impl Grid {", accessors, "}",
              "mod collision_geometry {", shape, "}", "mod skirmish {", footprint,
              "mod movement {", coordinate,
              "pub(super) mod obstacles { use std::collections::BTreeSet; use crate::{Cell,Grid}; use super::super::Footprint;",
              "pub(in crate::skirmish) mod raster {", raster, "}\n}\n}", "pub fn run() {"]
    for name, shape, values, (width, height) in fixtures:
        x, y, angle, major, minor = [f"f32::from_bits({v})" for v in values]
        result.append(
            f'{{ let object=Footprint{{position:[{x},{y}],angle:{angle},major:{major},minor:{minor},'
            f'shape:crate::collision_geometry::Shape::{shape}}}; '
            f'let cells=movement::obstacles::raster::cells(&object,&crate::Grid{{width:{width},height:{height}}}); '
            f'print!("{{}} {{}}",{json.dumps(name)},cells.len()); '
            'for cell in cells {print!(" {},{}",cell.x,cell.y);} println!(); }'
        )
    return "\n".join(result) + "\n}\n}\nfn main() { skirmish::run(); }\n"


def compare(expected, actual, fixtures):
    native, translated = expected.splitlines(), actual.splitlines()
    if len(native) != len(fixtures) or len(translated) != len(fixtures):
        raise RuntimeError(f"Unexpected footprint result counts: {len(native)}, {len(translated)}")
    for index, (left, right) in enumerate(zip(native, translated)):
        if left != right:
            original, port = set(left.split()[2:]), set(right.split()[2:])
            raise RuntimeError(
                f"Obstacle raster mismatch in {fixtures[index][0]}: "
                f"missing={sorted(original-port)[:12]}, extra={sorted(port-original)[:12]}"
            )
        name, _, _, extent = fixtures[index]
        if name.startswith(("last_column/", "last_row/")):
            axis = 0 if name.startswith("last_column/") else 1
            coordinates = {int(cell.split(",")[axis]) for cell in left.split()[2:]}
            if extent[axis] - 1 in coordinates or extent[axis] - 2 not in coordinates:
                raise RuntimeError(f"Original exclusive last-row/column boundary changed: {name}")
    if "83,211" not in native[0].split()[2:]:
        raise RuntimeError("Original Arms Dealer fixture did not block cell [83,211]")


def main():
    run = reference.run
    if run(["git", "-C", str(reference.UPSTREAM), "rev-parse", "HEAD"]) != reference.COMMIT:
        raise RuntimeError("Review changed upstream baseline before obstacle comparison")
    for path in [SOURCE, TRIG]:
        if run(["git", "-C", str(reference.UPSTREAM), "status", "--porcelain", "--", path]):
            raise RuntimeError(f"Modified native obstacle dependency: {path}")
    fixtures = cases()
    cache = reference_support.CACHE / "obstacle-reference"
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
        run([clang, "-std=c++20", "-O1", "-Wall", "-Wextra", "-Werror", "-ffp-contract=off",
             str(cpp), "-o", str(native)], env=env)
        run([rustc, "--edition=2024", "-Dwarnings", "-Copt-level=1", str(rust), "-o", str(port)], env=env)
        compare(run([str(native)], env=env), run([str(port)], env=env), fixtures)
    print(f"PASS: {len(fixtures)} native obstacle footprints match production Rust cell sets.")
    print("BOX rotations/five-unit samples, corrected circle bounds, neighboring-float boundaries, exclusive final row/column and Arms Dealer [83,211]; no zone/search/scheduler claim.")


if __name__ == "__main__":
    main()
