// Environmental adapter for the unchanged pinned hierarchy routines.
#include <algorithm>
#include <cmath>
#include <iostream>
#include <list>
#include <map>
#include <set>
#include <tuple>
#include <vector>
using Int=int; using UnsignedInt=unsigned; using Real=float; using Bool=bool;
using PathfindLayerEnum=int; using LocomotorSurfaceTypeMask=int; using zoneStorageType=unsigned;
constexpr int LAYER_GROUND=1,LAYER_LAST=3,COST_ORTHOGONAL=10,LOCOMOTORSURFACE_GROUND=1;
constexpr float PATHFIND_CELL_SIZE_F=10,HUGE_DIST=1000000;
#define FALSE false
#define TRUE true
#define RTS_GENERALS 0
#define RETAIL_COMPATIBLE_PATHFINDING 0
#define DEBUG_ASSERTCRASH(...)
#define DEBUG_LOG(...)
#define DEBUG_CRASH(...)
#define REAL_TO_INT_FLOOR(value) int(std::floor(value))
#define IABS(value) std::abs(value)
#define newInstance(type) new type
inline float sqr(float value){return value*value;}
struct ICoord2D {int x=0,y=0;};
struct Coord3D {float x=0,y=0,z=0;};
struct Region {ICoord2D lo,hi;};
struct PathfindCell;
using Key=std::tuple<int,int,int>;
struct Info {
 ICoord2D m_pos; unsigned m_costSoFar=0,m_totalCost=0;
 Info *m_nextOpen=nullptr,*m_prevOpen=nullptr,*m_pathParent=nullptr;
 bool m_open=false,m_closed=false;
};
struct PathfindCellList {
 std::list<PathfindCell*> cells;
 bool empty()const{return cells.empty();}
 void reset(){cells.clear();}
 PathfindCell* getHead(){return cells.front();}
};
struct PathfindCell {
 enum {CELL_CLEAR,CELL_CLIFF};
 int layer=1,x=0,y=0;unsigned zone=0;bool pinched=false,retained=false;
 Info storage,*m_info=nullptr;PathfindCell* parent=nullptr;
 static int available;static std::map<Key,Info> cleanup;
 Key key()const{return {layer,x,y};}
 bool hasInfo()const{return m_info!=nullptr;}
 bool allocateInfo(ICoord2D pos){if(m_info)return true;if(available==0)return false;--available;storage={};storage.m_pos=pos;m_info=&storage;return true;}
 void releaseInfo(){if(!m_info)return;cleanup[key()]=*m_info;parent=nullptr;if(!retained){m_info=nullptr;++available;}}
 int getXIndex()const{return m_info->m_pos.x;}int getYIndex()const{return m_info->m_pos.y;}
 int getLayer()const{return layer;}unsigned getZone()const{return zone;}int getType()const{return CELL_CLEAR;}
 bool getOpen()const{return m_info->m_open;}bool getClosed()const{return m_info->m_closed;}
 bool getPinched()const{return pinched;}bool isBlockedByAlly()const{return false;}
 unsigned getCostSoFar()const{return m_info->m_costSoFar;}
 void setCostSoFar(unsigned cost){m_info->m_costSoFar=cost;}void setTotalCost(unsigned total){m_info->m_totalCost=total;}
 void setParentCellHierarchical(PathfindCell* cell){parent=cell;}PathfindCell* getParentCell(){return parent;}
 void clearParentCell(){parent=nullptr;}
 bool startPathfind(PathfindCell*);unsigned costToGoal(PathfindCell*);unsigned costToHierGoal(PathfindCell*);
 void putOnSortedOpenList(PathfindCellList& list){m_info->m_open=true;m_info->m_closed=false;auto it=list.cells.begin();while(it!=list.cells.end()&&(*it)->m_info->m_totalCost<=m_info->m_totalCost)++it;list.cells.insert(it,this);}
 void removeFromOpenList(PathfindCellList& list){list.cells.remove(this);m_info->m_open=false;}
 void putOnClosedList(PathfindCellList& list){m_info->m_closed=true;list.cells.push_front(this);}
};
int PathfindCell::available=30000;std::map<Key,Info> PathfindCell::cleanup;
struct PathNode {
 Coord3D position;int layer=1;
 Coord3D* getPosition(){return &position;}int getLayer(){return layer;}
 void setLayer(int value){layer=value;}void setCanOptimize(bool){}
};
struct Path {
 std::list<PathNode> nodes;PathNode* getFirstNode(){return nodes.empty()?nullptr:&nodes.front();}
 void prependNode(const Coord3D* pos,int layer){nodes.push_front({*pos,layer});}
 void setBlockedByAlly(bool){}
};
struct Layer {
 bool active=false,destroyed=false;unsigned zone=0;ICoord2D first,last;
 bool isUnused(){return !active;}bool isDestroyed(){return destroyed;}
 void getStartCellIndex(ICoord2D* out){*out=first;}void getEndCellIndex(ICoord2D* out){*out=last;}
 unsigned getZone(){return zone;}
};
struct PathfindZoneManager {
 static constexpr int ZONE_BLOCK_SIZE=10;static constexpr unsigned UNINITIALIZED_ZONE=0;
 std::set<std::pair<int,int>> bridge,passable;
 bool disconnected=false;
 unsigned getEffectiveZone(int,bool,unsigned zone){return disconnected&&zone==3?2:1;}
 unsigned getBlockZone(int,bool,int x,int y,PathfindCell** map){return map[x][y].getZone();}
 bool interactsWithBridge(int x,int y){return bridge.contains({x/10,y/10});}
 void getExtent(ICoord2D& out){out={3,1};}
 void setPassable(int x,int y,bool){passable.insert({x/10,y/10});}
};
struct Terrain {int getLayerForDestination(const Coord3D* pos){return pos->z>0?2:1;}};
Terrain terrain,*TheTerrainLogic=&terrain;
struct Pathfinder {
 PathfindCell ground[30][10],upper[30][10];PathfindCell* m_map[30];
 Layer m_layers[4];PathfindZoneManager m_zoneManager;PathfindCellList m_openList,m_closedList;
 bool m_isMapReady=true,m_isTunneling=false;int m_cumulativeCellsAllocated=0;
 Region m_extent{{0,0},{29,9}},logical{{0,0},{29,9}};
 Pathfinder(){for(int x=0;x<30;x++){m_map[x]=ground[x];for(int y=0;y<10;y++){ground[x][y].x=x;ground[x][y].y=y;ground[x][y].zone=x/10+1;upper[x][y].x=x;upper[x][y].y=y;upper[x][y].layer=2;upper[x][y].zone=4;}}}
 void worldToCell(const Coord3D* pos,ICoord2D* out){*out={std::clamp(int(std::floor(pos->x/10)),0,29),std::clamp(int(std::floor(pos->y/10)),0,9)};}
 void clip(Coord3D* from,Coord3D* to){from->x=std::clamp(from->x,0.0f,299.0f);from->y=std::clamp(from->y,0.0f,99.0f);to->x=std::clamp(to->x,0.0f,299.0f);to->y=std::clamp(to->y,0.0f,99.0f);}
 PathfindCell* getCell(int layer,int x,int y){if(x<0||y<0||x>=30||y>=10)return nullptr;return layer==2&&m_layers[2].active?&upper[x][y]:&ground[x][y];}
 PathfindCell* getClippedCell(int layer,Coord3D* pos){ICoord2D cell;worldToCell(pos,&cell);return getCell(layer,cell.x,cell.y);}
 bool checkCellOutsideExtents(ICoord2D cell){return cell.x<logical.lo.x||cell.y<logical.lo.y||cell.x>logical.hi.x||cell.y>logical.hi.y;}
 void adjustCoordToCell(int x,int y,bool,Coord3D& pos,int layer){pos={x*10.0f+5,y*10.0f+5,layer==1?0.0f:10.0f};}
 void cleanOpenAndClosedLists(){for(auto* list:{&m_openList,&m_closedList}){for(auto* cell:list->cells){++m_cumulativeCellsAllocated;cell->m_info->m_open=false;cell->m_info->m_closed=false;cell->releaseInfo();}list->reset();}}
 void prependCells(Path*,const Coord3D*,PathfindCell*,bool);
 Path* buildHierarchicalPath(const Coord3D*,PathfindCell*);
 void processHierarchicalCell(const ICoord2D&,const ICoord2D&,PathfindCell*,PathfindCell*,unsigned,unsigned*,int&,bool,int&);
 Path* internal_findHierarchicalPath(bool,int,const Coord3D*,const Coord3D*,bool,bool);
};
