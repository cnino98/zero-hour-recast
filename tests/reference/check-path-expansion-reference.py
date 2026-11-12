#!/usr/bin/env python3
"""Pinned native neighbor expansion/cost fixtures, not whole-A* admission.

Executes unchanged examineNeighboringCells and PathfindCell::costSoFar.
The fixture deliberately has no heuristic goal, so the separate goal-line and
hierarchical algorithms are not replaced by wrappers or claimed covered here.
"""
from pathlib import Path
import reference_support
import importlib.util
import os
import sys
import tempfile
PROJECT=Path(__file__).resolve().parents[2]
sys.dont_write_bytecode=True
spec=importlib.util.spec_from_file_location('reference',PROJECT/'tests/reference/check-path-reference.py')
reference=importlib.util.module_from_spec(spec);spec.loader.exec_module(reference)
SOURCE='Core/GameEngine/Source/GameLogic/AI/AIPathfind.cpp'
FIXTURE=PROJECT/'tests/reference/path-expansion-reference.txt'


def extract(signature):
    value=(reference.UPSTREAM/SOURCE).read_text();start=value.index(signature)
    return value[start:value.index('\n}\n',start)+3]


def source():
    wrapper=r'''
#include <cmath>
#include <iostream>
#include <algorithm>
using Int=int;using UnsignedInt=unsigned;using Real=float;using Bool=bool;using ObjectID=int;using LocomotorSurfaceTypeMask=int;
constexpr int LAYER_GROUND=1,COST_ORTHOGONAL=10,COST_DIAGONAL=14,NO_ATTACK=-1,ENEMIES=0,KINDOF_DOZER=1,KINDOF_VEHICLE=2;constexpr float PATHFIND_CELL_SIZE_F=10;
#define DEBUG_ASSERTCRASH(...)
#define RETAIL_COMPATIBLE_PATHFINDING 0
#define RTS_GENERALS 0
struct ICoord2D{int x=0,y=0;};struct Coord3D{float x=0,y=0,z=0;};
struct Info{ICoord2D m_pos;};
struct PathfindCell {
 enum CellType{CELL_CLEAR,CELL_CLIFF,CELL_OBSTACLE};int x=0,y=0;CellType kind=CELL_CLEAR;
 Info storage,*m_info=nullptr;bool pinched=false,open=false,closed=false,fixed=false,moving=false,enemy=false;
 unsigned cost=0,total=0;PathfindCell* parent=nullptr;
 bool hasInfo(){return m_info!=nullptr;}int getXIndex(){return x;}int getYIndex(){return y;}CellType getType(){return kind;}
 bool getPinched(){return pinched;}bool getOpen(){return open;}bool getClosed(){return closed;}unsigned getCostSoFar(){return cost;}
 PathfindCell* getParentCell(){return parent;}bool allocateInfo(ICoord2D p){storage.m_pos=p;m_info=&storage;return true;}
 void setCostSoFar(unsigned c){cost=c;}void setTotalCost(unsigned c){total=c;}void setParentCell(PathfindCell* p){parent=p;}
 void setBlockedByAlly(bool){}void putOnSortedOpenList(int&){open=true;}void removeFromClosedList(int&){closed=false;}void removeFromOpenList(int&){open=false;}
 unsigned costSoFar(PathfindCell*);unsigned costToGoal(PathfindCell*){return 0;}
};
struct AI{bool canPathThroughUnits()const{return false;}};
struct Object {bool dozer=false;AI ai;int getCrusherLevel()const{return 0;}bool isKindOf(int k)const{return k==KINDOF_DOZER&&dozer;}const AI* getAIUpdateInterface()const{return &ai;}int getRelationship(Object*)const{return 2;}};
struct LocomotorSet{bool downhill=false;bool isDownhillOnly()const{return downhill;}int getValidSurfaces()const{return 5;}};
struct Terrain{float heights[49]{};float getGroundHeight(float x,float y){return heights[int(y/10)*7+int(x/10)];}};
Terrain terrain,*TheTerrainLogic=&terrain;
struct TCheckMovementInfo{ICoord2D cell;int layer=1,radius=0,acceptableSurfaces=0,allyFixedCount=0;bool centerInCell=true,considerTransient=false,enemyFixed=false,allyMoving=false,allyGoal=false;};
struct Pathfinder;
struct ExamineCellsStruct{Pathfinder* thePathfinder;const LocomotorSet* theLoco;bool centerInCell,isHuman,isCrusher;int radius;const Object* obj;PathfindCell* goalCell;};
struct Zone{bool isPassable(int,int){return true;}};
struct Pathfinder {
 PathfindCell cells[49];bool m_isTunneling=false;int m_ignoreObstacleID=0,m_openList=0,m_closedList=0;Zone m_zoneManager;
 Pathfinder(){for(int y=0;y<7;y++)for(int x=0;x<7;x++){auto& c=cells[y*7+x];c.x=x;c.y=y;}}
 PathfindCell* getCell(int,int x,int y){return x<0||y<0||x>=7||y>=7?nullptr:&cells[y*7+x];}
 bool checkCellOutsideExtents(ICoord2D p){return p.x<0||p.y<0||p.x>=7||p.y>=7;}
 bool validMovementPosition(bool,int,PathfindCell* c,PathfindCell*){return c&&c->kind!=PathfindCell::CELL_OBSTACLE;}
 bool checkForMovement(const Object*,TCheckMovementInfo& i){auto c=getCell(1,i.cell.x,i.cell.y);if(!c)return false;i.enemyFixed=c->enemy;i.allyFixedCount=c->fixed;i.allyMoving=c->moving;return true;}
 static int examineCellsCallback(Pathfinder*,PathfindCell*,PathfindCell*,int,int,void*){return 0;}
 void iterateCellsAlongLine(ICoord2D,ICoord2D,int,int(*)(Pathfinder*,PathfindCell*,PathfindCell*,int,int,void*),void*){}
 int examineNeighboringCells(PathfindCell*,PathfindCell*,const LocomotorSet&,bool,bool,int,const ICoord2D&,const Object*,int);
};
struct Logic{Object obstacle;Object* findObjectByID(int){return &obstacle;}};
Logic logic,*TheGameLogic=&logic;
'''
    # getObstacleID/getLayer are metadata accessors, not algorithm substitutes.
    wrapper=wrapper.replace('bool getPinched(){','int getObstacleID(){return 1;}int getLayer(){return 1;}bool getPinched(){')
    main=r'''
int main(){for(int mode=0;mode<32;mode++){
 Pathfinder p;Object actor;LocomotorSet locomotor;actor.dozer=(mode&1)!=0;locomotor.downhill=(mode&2)!=0;p.m_isTunneling=(mode&4)!=0;
 for(int i=0;i<49;i++)terrain.heights[i]=0;
 auto* east=p.getCell(1,3,2);auto* north=p.getCell(1,2,3);
 east->kind=(mode&8)?PathfindCell::CELL_OBSTACLE:PathfindCell::CELL_CLIFF;
 east->pinched=(mode&16)!=0;north->kind=(mode&8)?PathfindCell::CELL_OBSTACLE:PathfindCell::CELL_CLEAR;
 terrain.heights[2*7+3]=(mode&2)?10.0f:9.5f;
 auto* parent=p.getCell(1,2,2);parent->allocateInfo({2,2});parent->open=true;
 std::cout<<mode<<'\n';p.examineNeighboringCells(parent,nullptr,locomotor,true,true,0,{2,2},&actor,NO_ATTACK);
 for(auto d:{ICoord2D{1,0},ICoord2D{0,1},ICoord2D{-1,0},ICoord2D{0,-1},ICoord2D{1,1},ICoord2D{-1,1},ICoord2D{-1,-1},ICoord2D{1,-1}}){auto* c=p.getCell(1,2+d.x,2+d.y);std::cout<<(c->hasInfo()?int(c->cost):-1)<<' ';}std::cout<<'\n';}}
'''
    native=extract('UnsignedInt PathfindCell::costSoFar(')+extract('Int Pathfinder::examineNeighboringCells(')
    header=(reference.UPSTREAM/SOURCE).read_text().split('*/',1)[0]+'*/\n'
    return header+wrapper+native+main


def main():
    if reference.run(['git','-C',str(reference.UPSTREAM),'rev-parse','HEAD'])!=reference.COMMIT:raise RuntimeError('Unexpected pinned source revision')
    temp=reference_support.CACHE;temp.mkdir(parents=True,exist_ok=True)
    env=dict(os.environ,TMPDIR=str(temp),CLANG_MODULE_CACHE_PATH=str(temp/'clang-modules'))
    with tempfile.TemporaryDirectory(prefix='path-expansion-reference-',dir=temp) as directory:
        folder=Path(directory);cpp=folder/'expansion.cpp';cpp.write_text(source());binary=folder/'expansion'
        reference.run([reference_support.compiler("CXX", "clang++"),'-std=c++20','-O0','-ffp-contract=off',str(cpp),'-o',str(binary)],env=env)
        result=reference.run([str(binary)],env=env)+'\n'
    if '--write-fixture' in sys.argv:
        reference_support.write_fixture(FIXTURE, result);print('Wrote 32 native neighbor expansion cases; real Rust consumer check required.')
    elif FIXTURE.read_text()!=result:raise RuntimeError('Pinned expansion fixture changed')
    else:print('PASS: 32 pinned native neighbor expansion cases.')

if __name__=='__main__':main()
