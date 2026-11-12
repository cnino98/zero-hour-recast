#!/usr/bin/env python3
"""Compare native ground-group width admission with production layer helper.

The fixture cell lookup supplies the same bounded ground/upper rectangles and
raw fixed-occupant flags to both implementations. The diameter algorithm itself
is extracted unchanged; this does not claim to compare complete A* routes.
"""
from pathlib import Path
import reference_support
import importlib.util
import os
import sys
import tempfile

PROJECT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("service", PROJECT / "tests/reference/check-path-service-reference.py")
service = importlib.util.module_from_spec(spec)
spec.loader.exec_module(service)
reference = service.reference
SOURCE = "Core/GameEngine/Source/GameLogic/AI/AIPathfind.cpp"
RUST = "crates/generals-sim/src/grid/group.rs"
BLOCKED = [(1, 2, 4), (1, 8, 7), (2, 7, 5), (2, 9, 6)]
FIXED = [(1, 5, 3), (1, 12, 8), (2, 6, 6), (2, 10, 4)]


def cases():
    return [(layer, x, y, diameter) for layer in [1, 2]
            for x, y in [(0, 0), (2, 4), (4, 3), (5, 4), (6, 6), (8, 5), (11, 8), (12, 8), (15, 11)]
            for diameter in range(9)]


def cpp_source():
    text = (reference.UPSTREAM / SOURCE).read_text()
    header = text.split("*/", 1)[0] + "*/\n"
    algorithm = service.extract(text, "Int Pathfinder::clearCellForDiameter(")
    wrapper = r'''
#include <iostream>
using Int=int; using Bool=bool; using PathfindLayerEnum=int;
struct Object { int getCrushableLevel() const {return 1;} };
struct Logic { Object object; Object* findObjectByID(int) {return &object;} };
Logic logic; Logic* TheGameLogic=&logic;
struct PathfindCell {
    enum {CELL_CLEAR=0,CELL_OBSTACLE=1,UNIT_PRESENT_FIXED=1};
    int type=0, flags=0;
    int getType() const {return type;}
    int getFlags() const {return flags;}
    int getPosUnit() const {return 1;}
    bool isObstacleFence() const {return false;}
};
struct Pathfinder {
    PathfindCell cells[3][16][12];
    PathfindCell* getCell(int layer,int x,int y) {
        if(x<0||x>=16||y<0||y>=12) return nullptr;
        if(layer==2 && !(x>=4&&x<12&&y>=3&&y<9)) layer=1;
        return &cells[layer][x][y];
    }
    int clearCellForDiameter(bool,int,int,int,int);
};
'''
    lines = [header, wrapper, algorithm, "int main(){ Pathfinder grid;"]
    for layer, x, y in BLOCKED:
        lines.append(f"grid.cells[{layer}][{x}][{y}].type=1;")
    for layer, x, y in FIXED:
        lines.append(f"grid.cells[{layer}][{x}][{y}].flags=1;")
    for layer, x, y, diameter in cases():
        lines.append(f'std::cout<<grid.clearCellForDiameter(false,{x},{y},{layer},{diameter})<<"\\n";')
    return "\n".join(lines) + "\n}\n"


def rust_source():
    algorithm = service.extract((PROJECT / RUST).read_text(), "fn clear_diameter(")
    wrapper = r'''
use std::collections::{BTreeMap,BTreeSet};
#[derive(Clone,Copy,Eq,PartialEq,Ord,PartialOrd)] struct Cell {x:u16,y:u16}
#[derive(Clone,Copy,Eq,PartialEq,Ord,PartialOrd)] struct LayerCell {cell:Cell,layer:u8}
struct RouteUnits {width_blockers:BTreeSet<Cell>}
struct Grid {blocked:BTreeSet<LayerCell>}
impl Grid {
    fn signed_cell(&self,x:i32,y:i32)->Option<Cell> {
        (x>=0&&x<16&&y>=0&&y<12).then(||Cell {x:u16::try_from(x).unwrap(),y:u16::try_from(y).unwrap()})
    }
    fn resolve_layer(&self,node:LayerCell)->LayerCell {
        if node.layer==2 && !(node.cell.x>=4&&node.cell.x<12&&node.cell.y>=3&&node.cell.y<9) {
            LayerCell {layer:1,..node}
        } else {node}
    }
    fn layer_blocked(&self,node:LayerCell)->bool {self.blocked.contains(&self.resolve_layer(node))}
}
'''
    blocked = ",".join(f"LayerCell {{cell:Cell {{x:{x},y:{y}}},layer:{layer}}}" for layer, x, y in BLOCKED)
    lines = [wrapper, algorithm, f"fn main(){{let grid=Grid {{blocked:BTreeSet::from([{blocked}])}};"]
    for layer in [1, 2]:
        fixed = ",".join(f"Cell {{x:{x},y:{y}}}" for fixed_layer, x, y in FIXED if fixed_layer == layer)
        lines.append(f"let layer{layer}=RouteUnits {{width_blockers:BTreeSet::from([{fixed}])}};")
    lines.append("let units=BTreeMap::from([(1,&layer1),(2,&layer2)]);")
    for layer, x, y, diameter in cases():
        lines.append(f'println!("{{}}",clear_diameter(&grid,LayerCell {{cell:Cell {{x:{x},y:{y}}},layer:{layer}}},{diameter},&units));')
    return "\n".join(lines) + "\n}\n"


def main():
    run = reference.run
    if run(["git", "-C", str(reference.UPSTREAM), "rev-parse", "HEAD"]) != reference.COMMIT:
        raise RuntimeError("Pinned source changed")
    if run(["git", "-C", str(reference.UPSTREAM), "status", "--porcelain", "--", SOURCE]):
        raise RuntimeError("Native AIPathfind.cpp was modified")
    cache = reference_support.CACHE / "path-width-reference"
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="run-", dir=cache) as directory:
        work = Path(directory)
        env = os.environ.copy()
        env.update(TMPDIR=str(work), XDG_CACHE_HOME=str(work / "xdg"), CLANG_MODULE_CACHE_PATH=str(work / "modules"))
        cpp, rust = work / "reference.cpp", work / "port.rs"
        cpp.write_text(cpp_source())
        rust.write_text(rust_source())
        clang = reference_support.compiler("CXX", "clang++")

        rustc = reference_support.compiler("RUSTC", "rustc")
        native, port = work / "reference", work / "port"
        run([clang, "-std=c++20", "-Wall", "-Wextra", "-Werror", "-ffp-contract=off", str(cpp), "-o", str(native)], env=env)
        run([rustc, "--edition=2024", "-Dwarnings", str(rust), "-o", str(port)], env=env)
        expected, actual = run([str(native)], env=env), run([str(port)], env=env)
        if expected != actual:
            differences = [f"{case}: native {left}, Rust {right}" for case, left, right in zip(cases(), expected.splitlines(), actual.splitlines()) if left != right]
            raise RuntimeError("Native width admission differs:\n" + "\n".join(differences[:10]))
        print(f"PASS: {len(cases())} extracted native group-width cases match production Rust.")
        print("Diameters zero through eight, bounded map edges, bridge rectangle fallback, fixed crushable occupants and cut corners.")


if __name__ == "__main__":
    main()
