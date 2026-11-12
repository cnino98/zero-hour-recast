#!/usr/bin/env python3
"""Execute pinned unedited road insertion/topology against bounded fixture graphs.

The wrapper supplies containers, vector operations and texture identity only.
Generated native TRoadPt fields left uninitialized by RoadSegment are zeroed in
this harness, matching Rust's explicit deterministic adaptation of undefined data.
"""
from pathlib import Path
import reference_support
import importlib.util
import os
import sys
import tempfile

PROJECT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('geometry_reference', PROJECT / 'tests/reference/check-road-geometry-reference.py')
geometry = importlib.util.module_from_spec(spec)
spec.loader.exec_module(geometry)
reference = geometry.reference
ROAD = geometry.ROAD
LINE = 'Core/Libraries/Source/WWVegas/WWMath/lineseg.cpp'
FIXTURE = PROJECT / 'tests/reference/road-network-reference.txt'


def source():
    wrapper = r'''
#include <algorithm>
#include <bit>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <iostream>
using Real=float;using Int=int;using Bool=bool;
constexpr float PI=3.14159265358979323846f, TEE_WIDTH_ADJUSTMENT=1.03f;
#define DEBUG_ASSERTLOG(...)
#define DEBUG_LOG(...)
#define CHECK_SEGMENTS {if(m_numRoads>=m_maxRoadSegments) return;}
struct WWMath {static float Acos(float x){return float(acos(double(x)));}};
struct Vector2 {
 float X=0,Y=0;Vector2()=default;Vector2(float x,float y):X(x),Y(y){}
 void Set(float x,float y){X=x;Y=y;}void Set(Vector2 v){*this=v;}
 float Length()const{return sqrtf(X*X+Y*Y);}
 void Normalize(){float l=X*X+Y*Y;if(l!=0){float r=1.f/sqrtf(l);X*=r;Y*=r;}}
 void Rotate(float a){float s=float(sin(double(a))),c=float(cos(double(a)));float x=X*c+Y*-s;Y=X*s+Y*c;X=x;}
 static float Dot_Product(Vector2 a,Vector2 b){return a.X*b.X+a.Y*b.Y;}
 Vector2 operator+(Vector2 v)const{return {X+v.X,Y+v.Y};}Vector2 operator-(Vector2 v)const{return {X-v.X,Y-v.Y};}
 Vector2 operator*(float s)const{return {X*s,Y*s};}Vector2 operator/(float s)const{float r=1.f/s;return {X*r,Y*r};}Vector2 operator-()const{return {-X,-Y};}
 Vector2& operator+=(Vector2 v){X+=v.X;Y+=v.Y;return *this;}Vector2& operator-=(Vector2 v){X-=v.X;Y-=v.Y;return *this;}
 Vector2& operator*=(float s){X*=s;Y*=s;return *this;}bool operator==(Vector2 v)const{return X==v.X&&Y==v.Y;}bool operator!=(Vector2 v)const{return !(*this==v);}
};
Vector2 operator*(float s,Vector2 v){return v*s;}
struct Vector3 {
 float X=0,Y=0,Z=0;Vector3()=default;Vector3(float x,float y,float z):X(x),Y(y),Z(z){}
 void Set(float x,float y,float z){X=x;Y=y;Z=z;}
 Vector3 operator+(Vector3 v)const{return {X+v.X,Y+v.Y,Z+v.Z};}Vector3 operator-(Vector3 v)const{return {X-v.X,Y-v.Y,Z-v.Z};}
 Vector3 operator*(float s)const{return {X*s,Y*s,Z*s};}
 float Length()const{return sqrtf(X*X+Y*Y+Z*Z);}void Normalize(){float l=X*X+Y*Y+Z*Z;if(l!=0){float r=1.f/sqrtf(l);X*=r;Y*=r;Z*=r;}}
 void Rotate_Z(float a){Vector2 v(X,Y);v.Rotate(a);X=v.X;Y=v.Y;}
 static float Dot_Product(Vector3 a,Vector3 b){return a.X*b.X+a.Y*b.Y+a.Z*b.Z;}
 static float Cross_Product_Z(Vector3 a,Vector3 b){return a.X*b.Y-a.Y*b.X;}
 static void Cross_Product(Vector3 a,Vector3 b,Vector3* out){*out={a.Y*b.Z-a.Z*b.Y,a.Z*b.X-a.X*b.Z,a.X*b.Y-a.Y*b.X};}
 static float Distance(Vector3 a,Vector3 b){return (a-b).Length();}
};
Vector3 operator*(float s,Vector3 v){return v*s;}
class LineSegClass {
 Vector3 P0,P1,DP,Dir;float Length;
 public:LineSegClass(Vector3 a,Vector3 b){Set(a,b);}void Set(Vector3 a,Vector3 b){P0=a;P1=b;DP=b-a;Dir=DP;Dir.Normalize();Length=DP.Length();}
 Vector3 Get_Dir()const{return Dir;}
 Vector3 Find_Point_Closest_To(const Vector3&)const;
 bool Find_Intersection(const LineSegClass&,Vector3*,float*,Vector3*,float*)const;
};
struct TRoadPt {Vector2 loc,top,bottom;int count=0;bool last=false,multi=false,isAngled=false,isJoin=false;};
enum TCorner {SEGMENT,CURVE,TEE,FOUR_WAY,THREE_WAY_Y,THREE_WAY_H,THREE_WAY_H_FLIP,ALPHA_JOIN};
struct RoadSegment {TRoadPt m_pt1,m_pt2;float m_curveRadius=0;TCorner m_type=SEGMENT;float m_scale=1,m_widthInTexture=1;int m_uniqueID=0;};
struct RoadType {int id,stack=0;int getUniqueID(){return id;}int getStacking(){return stack;}void setStacking(int s){stack=s;}};
struct Region2D {struct XY{float x,y;}lo,hi;};
struct W3DRoadBuffer {
 RoadSegment m_roads[512];RoadType m_roadTypes[3]={{1},{2},{3}};int m_numRoads=0,m_maxRoadSegments=512,m_maxRoadTypes=3;
 void flipTheRoad(RoadSegment* p){TRoadPt t=p->m_pt1;p->m_pt1=p->m_pt2;p->m_pt2=t;}
 void moveRoadSegTo(int,int);void checkLinkBefore(int);void checkLinkAfter(int);void addMapObject(RoadSegment*,bool);void updateCounts(RoadSegment*);void updateCountsAndFlags();
 void insertTee(Vector2,int,float);bool insertY(Vector2,int,float);void offset3Way(TRoadPt*,TRoadPt*,TRoadPt*,Vector2,Vector2,Vector2,float);
 void offsetH(TRoadPt*,TRoadPt*,TRoadPt*,Vector2,Vector2,Vector2,bool,bool,float);void offsetY(TRoadPt*,TRoadPt*,TRoadPt*,Vector2,Vector2,float);
 void offset4Way(TRoadPt*,TRoadPt*,TRoadPt*,TRoadPt*,TRoadPt*,Vector2,Vector2,float);void insert4Way(Vector2,int,float);void insertTeeIntersections();void insertCurveSegments();
 int findCrossTypeJoinVector(Vector2,Vector2*,int);void adjustStacking(int,int);void insertCrossTypeJoins();void miter(int,int);void insertCurveSegmentAt(int,int);void rotateAbout(Vector2*,Vector2,float);
};
'''
    signatures = ['moveRoadSegTo','checkLinkBefore','checkLinkAfter','addMapObject','updateCounts','updateCountsAndFlags','insertTee','insertY','offset3Way','offsetH','offsetY','offset4Way','insert4Way','insertTeeIntersections','insertCurveSegments','findCrossTypeJoinVector','adjustStacking','insertCrossTypeJoins','miter','insertCurveSegmentAt','rotateAbout']
    native = geometry.extract(ROAD, 'static Int xpSign(')
    native += geometry.extract(LINE, 'Vector3 LineSegClass::Find_Point_Closest_To(')
    native += 'bool\n' + geometry.extract(LINE, 'LineSegClass::Find_Intersection\n')
    for name in signatures:
        result = 'Bool' if name == 'insertY' else 'Int' if name == 'findCrossTypeJoinVector' else 'void'
        native += geometry.extract(ROAD, f'{result} W3DRoadBuffer::{name}(')
    main = r'''
int main(){int cases;std::cin>>cases;while(cases--){W3DRoadBuffer b;int n;std::cin>>n;std::cout<<n<<'\n';
 for(int i=0;i<n;i++){RoadSegment s;int flags;std::cin>>s.m_pt1.loc.X>>s.m_pt1.loc.Y>>s.m_pt2.loc.X>>s.m_pt2.loc.Y>>s.m_uniqueID>>flags;
 std::cout<<s.m_pt1.loc.X<<' '<<s.m_pt1.loc.Y<<' '<<s.m_pt2.loc.X<<' '<<s.m_pt2.loc.Y<<' '<<s.m_uniqueID<<' '<<flags<<'\n';
 s.m_scale=10;s.m_widthInTexture=1;s.m_curveRadius=(flags&64)?.5f:1.5f;
 s.m_pt1.isAngled=s.m_pt2.isAngled=(flags&8)!=0;s.m_pt1.isJoin=(flags&128)!=0;s.m_pt2.isJoin=(flags&256)!=0;b.addMapObject(&s,true);}
 RoadSegment copy[512];int count=b.m_numRoads;for(int i=0;i<count;i++)copy[i]=b.m_roads[i];b.m_numRoads=0;for(int i=0;i<count;i++)b.addMapObject(&copy[i],false);
 b.updateCountsAndFlags();b.insertTeeIntersections();b.insertCurveSegments();b.insertCrossTypeJoins();
 std::cout<<b.m_numRoads<<'\n';for(int i=0;i<b.m_numRoads;i++){auto s=b.m_roads[i];std::cout<<int(s.m_type)<<' '<<s.m_uniqueID<<' '<<std::bit_cast<uint32_t>(s.m_curveRadius)<<' '<<std::bit_cast<uint32_t>(s.m_scale)<<' '<<std::bit_cast<uint32_t>(s.m_widthInTexture);
 for(auto p:{s.m_pt1,s.m_pt2}){for(auto v:{p.loc,p.top,p.bottom})std::cout<<' '<<std::bit_cast<uint32_t>(v.X)<<' '<<std::bit_cast<uint32_t>(v.Y);std::cout<<' '<<p.count<<' '<<(int(p.last)|int(p.multi)*2|int(p.isAngled)*4|int(p.isJoin)*8);}std::cout<<'\n';}
 for(auto t:b.m_roadTypes)std::cout<<t.stack<<' ';std::cout<<'\n';}}
'''
    header = (reference.UPSTREAM / ROAD).read_text().split("*/", 1)[0] + "*/\n"
    return header + wrapper + native + main


def inputs():
    graphs = []
    center = (100, 100)
    for arms in [[(200,100),(0,100),(100,200)],[(200,100),(0,100),(150,180)],[(100,0),(30,170),(170,170)],[(200,100),(0,100),(100,200),(100,0)]]:
        graphs.append([(*center,*arm,1,0) for arm in arms])
    for end in [(100,200),(160,160),(180,110),(100,105)]:
        for flags in [0,8,64]:
            graphs.append([(0,100,100,100,1,flags),(100,100,*end,1,flags)])
    graphs += [[(0,100,200,100,1,0),(100,100,100,200,2,128)],[(0,100,200,100,1,0),(100,200,100,100,2,256)],[(0,100,100,100,1,0),(0,100,100,100,1,0)],[(0,0,100,0,1,0),(100,0,100,100,1,0),(100,100,0,100,1,0),(0,100,0,0,1,0)]]
    return str(len(graphs))+'\n'+''.join(str(len(graph))+'\n'+''.join(' '.join(map(str,line))+'\n' for line in graph) for graph in graphs)


def main():
    if reference.run(['git','-C',str(reference.UPSTREAM),'rev-parse','HEAD']) != reference.COMMIT:
        raise RuntimeError('Unexpected pinned source revision')
    temp = reference_support.CACHE
    temp.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, TMPDIR=str(temp), CLANG_MODULE_CACHE_PATH=str(temp/'clang-modules'))
    with tempfile.TemporaryDirectory(prefix='road-network-reference-',dir=temp) as directory:
        folder=Path(directory);cpp=folder/'network.cpp';cpp.write_text(source());binary=folder/'network'
        reference.run([reference_support.compiler("CXX", "clang++"),'-std=c++20','-O0','-ffp-contract=off',str(cpp),'-o',str(binary)],env=env)
        # Input graphs surround the unchanged native topology routines.
        import subprocess
        result=subprocess.run([str(binary)],input=inputs(),text=True,check=True,capture_output=True,env=env).stdout
    if '--write-fixture' in sys.argv:
        reference_support.write_fixture(FIXTURE, result)
        print('Wrote 20 extracted native road topology graphs; Rust consumer checks remain required.')
    elif FIXTURE.read_text()!=result:
        raise RuntimeError('Pinned native road topology fixture changed')
    else:
        print('PASS: 20 pinned native road topology graphs.')

if __name__=='__main__':
    main()
