// Environmental storage adapter; all zone algorithms below are pinned source.
#include <algorithm>
#include <cstdint>
#include <iostream>
#include <vector>
using Int=int;using UnsignedInt=unsigned;using UnsignedShort=std::uint16_t;
using Bool=bool;using zoneStorageType=UnsignedShort;using LocomotorSurfaceTypeMask=int;
constexpr int LAYER_GROUND=1,LAYER_LAST=15;
constexpr int LOCOMOTORSURFACE_GROUND=1,LOCOMOTORSURFACE_WATER=2,LOCOMOTORSURFACE_CLIFF=4,LOCOMOTORSURFACE_AIR=8,LOCOMOTORSURFACE_RUBBLE=16;
#define RTS_GENERALS 0
#define RETAIL_COMPATIBLE_PATHFINDING 0
#define FALSE false
#define TRUE true
#define __fastcall
#define DEBUG_ASSERTCRASH(...)
#define DEBUG_CRASH(...)
#define DEBUG_LOG(...)
#define MSGNEW(...) new
struct ICoord2D {
 int x=0,y=0;
 void updateMin(const ICoord2D& other){x=std::min(x,other.x);y=std::min(y,other.y);}
};
struct IRegion2D {
 ICoord2D lo,hi;
 void intersectWith(const IRegion2D& other){lo.x=std::max(lo.x,other.lo.x);lo.y=std::max(lo.y,other.lo.y);hi.updateMin(other.hi);}
};
struct PathfindCell {
 enum CellType {CELL_CLEAR=0,CELL_WATER=1,CELL_CLIFF=2,CELL_RUBBLE=3,CELL_OBSTACLE=4,CELL_BRIDGE_IMPASSABLE=5,CELL_IMPASSABLE=6};
 CellType kind=CELL_CLEAR;zoneStorageType zone:14=0;bool fence=false;int connection=0;
 CellType getType()const{return kind;}zoneStorageType getZone()const{return zone;}
 void setZone(zoneStorageType value){zone=value;}bool isObstacleFence()const{return fence;}
 int getConnectLayer()const{return connection;}
};
struct PathfindLayer {
 zoneStorageType zone=0;bool active=false,destroyed=false;ICoord2D start,end;
 void setZone(zoneStorageType value){zone=value;}zoneStorageType getZone()const{return zone;}
 void applyZone(){} // No upper cell table is queried by zone calculation.
 bool isUnused()const{return !active;}bool isDestroyed()const{return destroyed;}
 void getStartCellIndex(ICoord2D* out)const{*out=start;}void getEndCellIndex(ICoord2D* out)const{*out=end;}
};
struct ZoneBlock {
 ICoord2D m_cellOrigin;
 unsigned m_firstZone=0,m_numZones=0,m_zonesAllocated=0;
 bool m_interactsWithBridge=false;
 zoneStorageType *m_groundCliffZones=nullptr,*m_groundWaterZones=nullptr,*m_groundRubbleZones=nullptr,*m_crusherZones=nullptr;
 void setInteractsWithBridge(bool value){m_interactsWithBridge=value;}bool getInteractsWithBridge()const{return m_interactsWithBridge;}
 void blockCalculateZones(PathfindCell**,PathfindLayer[],const IRegion2D&);
 zoneStorageType getEffectiveZone(LocomotorSurfaceTypeMask,Bool,zoneStorageType)const;
 void allocateZones();void freeZones();
 ~ZoneBlock(){freeZones();}
};
struct PathfindZoneManager {
 static constexpr int ZONE_BLOCK_SIZE=10,INITIAL_ZONES=1024;
 static constexpr zoneStorageType UNINITIALIZED_ZONE=0;
 ICoord2D m_zoneBlockExtent{3,2};ZoneBlock m_zoneBlocks[3][2];
 UnsignedShort m_maxZone=0,m_zonesAllocated=0;unsigned m_nextFrameToCalculateZones=0;
 zoneStorageType *m_groundCliffZones=nullptr,*m_groundWaterZones=nullptr,*m_groundRubbleZones=nullptr,*m_terrainZones=nullptr,*m_crusherZones=nullptr,*m_hierarchicalZones=nullptr;
 void allocateZones();void freeZones();
 void calculateZones(PathfindCell**,PathfindLayer[],const IRegion2D&);
 void updateZonesForModify(PathfindCell**,PathfindLayer[],const IRegion2D&,const IRegion2D&);
 void setBridge(Int,Int,Bool);Bool interactsWithBridge(Int,Int)const;
 zoneStorageType getBlockZone(LocomotorSurfaceTypeMask,Bool,Int,Int,PathfindCell**)const;
 zoneStorageType getEffectiveTerrainZone(zoneStorageType)const;
 zoneStorageType getEffectiveZone(LocomotorSurfaceTypeMask,Bool,zoneStorageType)const;
 ~PathfindZoneManager(){freeZones();}
};
