#!/usr/bin/env python3
"""Native checkPathCost examination/INFO limits on a bounded ground strip.

The allocator availability and existing INFO are explicit fixture inputs. The
native INFO pool routines have their own independent comparator. List adapters
preserve the pinned FIFO ordering for equal total costs; no bridge is present.
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
FIXTURE = PROJECT / "tests/reference/path-cost-reference.txt"


def extract(text, signature):
    start = text.index(signature)
    return text[start:text.index("\n}\n", start) + 3]


def source():
    native = (reference.UPSTREAM / SOURCE).read_text()
    adapter = r'''
#include <algorithm>
#include <cmath>
#include <iostream>
#include <vector>
using Int=int;using UnsignedInt=unsigned;using Bool=bool;using Real=float;
using PathfindLayerEnum=int;
constexpr int LAYER_GROUND=1,COST_ORTHOGONAL=10,COST_DIAGONAL=14,MAX_CELL_COUNT=500;
#define RETAIL_COMPATIBLE_PATHFINDING 0
#define DEBUG_ASSERTCRASH(...)
struct ICoord2D {int x=0,y=0;};struct Coord3D {float x=0,y=0,z=0;};
struct PathfindCell;
struct List {std::vector<PathfindCell*> cells;bool empty()const{return cells.empty();}PathfindCell* getHead(){return cells.front();}void reset(){cells.clear();}};
struct Info {ICoord2D m_pos;};
int available=0;
struct PathfindCell {
 int x=0,y=0;Info storage,*m_info=nullptr;unsigned cost=0,total=0;bool open=false,closed=false,pinched=false;PathfindCell* parent=nullptr;
 bool hasInfo(){return m_info!=nullptr;}bool getOpen(){return open;}bool getClosed(){return closed;}bool getPinched(){return pinched;}
 int getLayer(){return 1;}int getXIndex(){return m_info->m_pos.x;}int getYIndex(){return m_info->m_pos.y;}
 unsigned getCostSoFar(){return cost;}unsigned getTotalCost(){return total;}PathfindCell* getParentCell(){return parent;}
 bool allocateInfo(ICoord2D pos){if(m_info)return true;if(!available)return false;--available;storage.m_pos=pos;m_info=&storage;cost=total=0;return true;}
 void releaseInfo(){if(m_info){++available;m_info=nullptr;parent=nullptr;}}
 void setCostSoFar(unsigned v){cost=v;}void setTotalCost(unsigned v){total=v;}void setBlockedByAlly(bool){}void setParentCell(PathfindCell* p){parent=p;}
 unsigned costSoFar(PathfindCell*);unsigned costToGoal(PathfindCell*);
 void startPathfind(PathfindCell* goal){cost=0;total=costToGoal(goal);parent=nullptr;open=closed=false;}
 void putOnSortedOpenList(List& list){auto at=std::find_if(list.cells.begin(),list.cells.end(),[&](auto c){return c->total>total;});list.cells.insert(at,this);open=true;}
 void putOnClosedList(List& list){list.cells.push_back(this);closed=true;}
 void removeFromOpenList(List& list){std::erase(list.cells,this);open=false;}void removeFromClosedList(List& list){std::erase(list.cells,this);closed=false;}
};
struct Object {int getCrusherLevel(){return 0;}};struct LocomotorSet {int getValidSurfaces()const{return 1;}};
struct Terrain {int getLayerForDestination(const Coord3D*){return 1;}} terrain,*TheTerrainLogic=&terrain;
struct Pathfinder {
 bool m_isMapReady=true,m_isTunneling=false;List m_openList,m_closedList;int cleaned=0;std::vector<PathfindCell> cells;
 Pathfinder():cells(450){for(int y=0;y<3;y++)for(int x=0;x<150;x++){auto& c=cells[y*150+x];c.x=x;c.y=y;}}
 PathfindCell* getCell(int,int x,int y){return x<0||y<0||x>=150||y>=3?nullptr:&cells[y*150+x];}
 PathfindCell* getCell(int layer,const Coord3D* p){return getCell(layer,int(std::floor(p->x/10)),int(std::floor(p->y/10)));}
 PathfindCell* getClippedCell(int layer,const Coord3D* p){return getCell(layer,p);}
 void worldToCell(const Coord3D* p,ICoord2D* out){out->x=int(std::floor(p->x/10));out->y=int(std::floor(p->y/10));}
 void getRadiusAndCenter(Object*,int& radius,bool& center){radius=0;center=true;}
 bool validMovementPosition(bool,int,PathfindCell*,PathfindCell* =nullptr){return true;}
 void checkChangeLayers(PathfindCell*){}
 void cleanOpenAndClosedLists(){for(auto* c:m_openList.cells){++cleaned;c->open=false;c->releaseInfo();}m_openList.reset();for(auto* c:m_closedList.cells){++cleaned;c->closed=false;c->releaseInfo();}m_closedList.reset();}
 int checkPathCost(Object*,const LocomotorSet&,const Coord3D*,const Coord3D*);
};
'''
    functions = "".join(extract(native, signature) for signature in [
        "UnsignedInt PathfindCell::costToGoal(",
        "UnsignedInt PathfindCell::costSoFar(",
        "Int Pathfinder::checkPathCost(",
    ])
    main = r'''
int main(){for(int mode=0;mode<4;mode++){
 Pathfinder path;Object actor;LocomotorSet locomotor;
 available=30000;for(auto& cell:path.cells)cell.allocateInfo({cell.x,cell.y});
 if(mode==1){for(auto& cell:path.cells)cell.m_info=nullptr;path.cells[151].m_info=&path.cells[151].storage;path.cells[153].m_info=&path.cells[153].storage;}
 available=mode==3?30000-450:0;
 if(mode==2)path.cells[152].pinched=true;
 Coord3D from{10,10,0},to{float((mode==0?148:3)*10),10,0};
 int cost=path.checkPathCost(&actor,locomotor,&from,&to);
 std::cout<<mode<<' '<<cost<<' '<<path.cleaned<<'\n';
}}
'''
    return native.split("*/", 1)[0] + "*/\n" + adapter + functions + main


def main():
    if reference.run(["git", "-C", str(reference.UPSTREAM), "rev-parse", "HEAD"]) != reference.COMMIT:
        raise RuntimeError("Unexpected pinned source revision")
    temp = reference_support.CACHE
    temp.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, TMPDIR=str(temp), CLANG_MODULE_CACHE_PATH=str(temp / "clang-modules"))
    with tempfile.TemporaryDirectory(prefix="path-cost-reference-", dir=temp) as directory:
        folder = Path(directory)
        cpp = folder / "cost.cpp"
        cpp.write_text(source())
        binary = folder / "cost"
        reference.run([reference_support.compiler("CXX", "clang++"), "-std=c++20", "-O0", "-ffp-contract=off", str(cpp), "-o", str(binary)], env=env)
        result = reference.run([str(binary)], env=env) + "\n"
    if "--write-fixture" in sys.argv:
        reference_support.write_fixture(FIXTURE, result)
        print("Wrote four native path-cost budget/exhaustion/retained-INFO cases; Rust consumer required.")
    elif FIXTURE.read_text() != result:
        raise RuntimeError("Pinned path-cost fixture changed")
    else:
        print("PASS: pinned checkPathCost budget/exhaustion cases.")


if __name__ == "__main__":
    main()
