#!/usr/bin/env python3
"""Extract the pinned INFO allocation/release routines and pool lifecycles.

The adapter initializes the native free-list and retained cell metadata. It does
not replace allocateInfo/releaseInfo or the pool pop/push routines.
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
FIXTURE = PROJECT / "tests/reference/path-info-reference.txt"


def extract(text, signature):
    start = text.index(signature)
    return text[start:text.index("\n}\n", start) + 3]


def source():
    native = (reference.UPSTREAM / SOURCE).read_text()
    adapter = r'''
#include <iostream>
#include <vector>
using Bool=bool; using Int=int; using UnsignedInt=unsigned;
constexpr int INVALID_ID=-1;
#define RETAIL_COMPATIBLE_PATHFINDING 0
#define DEBUG_ASSERTCRASH(...)
struct ICoord2D {int x=0,y=0;};
struct PathfindCell;
struct PathfindCellInfo {
 static PathfindCellInfo* s_firstFree;
 PathfindCellInfo *m_pathParent=nullptr,*m_nextOpen=nullptr,*m_prevOpen=nullptr;
 PathfindCell* m_cell=nullptr;ICoord2D m_pos;
 bool m_isFree=true,m_open=false,m_closed=false,m_obstacleIsFence=false,m_obstacleIsTransparent=false,m_blockedByAlly=false;
 unsigned m_costSoFar=0,m_totalCost=0;
 int m_obstacleID=-1,m_goalUnitID=-1,m_posUnitID=-1,m_goalAircraftID=-1;
 static PathfindCellInfo* getACellInfo(PathfindCell*,const ICoord2D&);
 static void releaseACellInfo(PathfindCellInfo*);
};
PathfindCellInfo* PathfindCellInfo::s_firstFree=nullptr;
struct PathfindCell {
 enum CellType {CELL_CLEAR,CELL_WATER,CELL_CLIFF,CELL_RUBBLE,CELL_OBSTACLE,CELL_BRIDGE_IMPASSABLE,CELL_IMPASSABLE};
 enum {NO_UNITS=0,UNIT_PRESENT_FIXED=1};
 CellType m_type=CELL_CLEAR;int m_flags=NO_UNITS;bool m_aircraftGoal=false;
 PathfindCellInfo* m_info=nullptr;
 bool allocateInfo(const ICoord2D&);void releaseInfo();
};
'''
    functions = "".join(extract(native, signature) for signature in [
        "PathfindCellInfo *PathfindCellInfo::getACellInfo(",
        "void PathfindCellInfo::releaseACellInfo(",
        "Bool PathfindCell::allocateInfo(",
        "void PathfindCell::releaseInfo()",
    ])
    main = r'''
int main(){
 std::vector<PathfindCellInfo> pool(CELL_INFOS_TO_ALLOCATE);
 for(unsigned i=0;i+1<pool.size();i++)pool[i].m_pathParent=&pool[i+1];
 PathfindCellInfo::s_firstFree=&pool[0];
 for(int mode=0;mode<8;mode++){
  PathfindCell cell;cell.m_type=(mode&1)?PathfindCell::CELL_OBSTACLE:PathfindCell::CELL_CLEAR;
  cell.m_flags=(mode&2)?PathfindCell::UNIT_PRESENT_FIXED:PathfindCell::NO_UNITS;cell.m_aircraftGoal=mode&4;
  bool first=cell.allocateInfo({3,4});cell.m_info->m_pathParent=&pool.back();
  bool reuse=cell.allocateInfo({8,9});cell.releaseInfo();
  std::cout<<mode<<' '<<first<<' '<<reuse<<' '<<(cell.m_info!=nullptr);
  if(cell.m_info)std::cout<<' '<<cell.m_info->m_pos.x<<' '<<cell.m_info->m_pos.y<<' '<<(cell.m_info->m_pathParent==nullptr);
  else std::cout<<" -1 -1 1";
  cell.m_type=PathfindCell::CELL_CLEAR;cell.m_flags=PathfindCell::NO_UNITS;cell.m_aircraftGoal=false;cell.releaseInfo();
  std::cout<<' '<<(cell.m_info!=nullptr)<<'\n';
 }
 std::vector<PathfindCell> cells(CELL_INFOS_TO_ALLOCATE+1);unsigned allocated=0;
 for(auto& cell:cells)allocated+=cell.allocateInfo({1,2});
 std::cout<<"pool "<<allocated<<' '<<(cells.back().m_info!=nullptr)<<' ';
 cells[0].releaseInfo();std::cout<<cells.back().allocateInfo({7,8})<<'\n';
}
'''
    constant = reference.original_line(SOURCE, "constexpr const UnsignedInt CELL_INFOS_TO_ALLOCATE =")
    return native.split("*/", 1)[0] + "*/\n" + adapter + constant + "\n" + functions + main


def main():
    if reference.run(["git", "-C", str(reference.UPSTREAM), "rev-parse", "HEAD"]) != reference.COMMIT:
        raise RuntimeError("Unexpected pinned source revision")
    temp = reference_support.CACHE
    temp.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, TMPDIR=str(temp), CLANG_MODULE_CACHE_PATH=str(temp / "clang-modules"))
    with tempfile.TemporaryDirectory(prefix="path-info-reference-", dir=temp) as directory:
        folder = Path(directory)
        cpp = folder / "info.cpp"
        cpp.write_text(source())
        binary = folder / "info"
        reference.run([reference_support.compiler("CXX", "clang++"), "-std=c++20", "-O0", str(cpp), "-o", str(binary)], env=env)
        result = reference.run([str(binary)], env=env) + "\n"
    if "--write-fixture" in sys.argv:
        reference_support.write_fixture(FIXTURE, result)
        print("Wrote native INFO lifecycle and 30000-record exhaustion cases; Rust consumer required.")
    elif FIXTURE.read_text() != result:
        raise RuntimeError("Pinned INFO lifecycle fixture changed")
    else:
        print("PASS: pinned INFO allocation/release and bounded pool lifecycles.")


if __name__ == "__main__":
    main()
