#!/usr/bin/env python3
"""Regenerate/check pinned native floating-road geometry fixtures.

Executes the unedited loadFloat4PtSection and getMaxCellHeight functions.
Vector/container adapters and synthetic terrain surround the native routines;
lighting and road-network topology are covered separately. The client regression
executes its real Bevy mesh consumer against this fixture, including exact indices.
"""
from pathlib import Path
import reference_support
import importlib.util
import os
import sys
import tempfile

PROJECT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('reference', PROJECT / 'tests/reference/check-path-reference.py')
reference = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reference)
UPSTREAM = reference.UPSTREAM
ROAD = 'GeneralsMD/Code/GameEngineDevice/Source/W3DDevice/GameClient/W3DRoadBuffer.cpp'
TERRAIN = 'Core/GameEngineDevice/Source/W3DDevice/GameClient/BaseHeightMap.cpp'
FIXTURE = PROJECT / 'tests/reference/road-geometry-reference.txt'


def extract(path, signature):
    text = (UPSTREAM / path).read_text()
    start = text.index(signature)
    return text[start:text.index('\n}\n', start) + 3]


def source():
    header = (UPSTREAM / ROAD).read_text().split('*/', 1)[0] + '*/\n'
    wrapper = r'''
#include <algorithm>
#include <bit>
#include <cmath>
#include <cstdint>
#include <iostream>
#include <vector>
using Real=float; using Int=int; using Bool=bool;
using UnsignedShort=unsigned short; using UnsignedByte=unsigned char;
constexpr float MAP_HEIGHT_SCALE=.625f, MAP_XY_FACTOR=10.f;
constexpr int MAX_SEG_VERTEX=500, MAX_SEG_INDEX=2000;
enum { bottomLeft,bottomRight,topLeft,topRight };
#define __max std::max
struct Vector2 {
 float X=0,Y=0; Vector2()=default; Vector2(float x,float y):X(x),Y(y){}
 void Set(float x,float y){X=x;Y=y;}
 float Length() const { return std::sqrt(X*X+Y*Y); }
 void Normalize(){float len2=X*X+Y*Y;if(len2!=0){float r=1.0f/std::sqrt(len2);X*=r;Y*=r;}}
 static float Dot_Product(Vector2 a,Vector2 b){return a.X*b.X+a.Y*b.Y;}
 Vector2 operator-(Vector2 b) const{return {X-b.X,Y-b.Y};}
};
struct Vector3 {
 float X=0,Y=0,Z=0; Vector3()=default; Vector3(float x,float y,float z):X(x),Y(y),Z(z){}
 Vector3 operator+(Vector3 b)const{return {X+b.X,Y+b.Y,Z+b.Z};}
 Vector3 operator-(Vector3 b)const{return {X-b.X,Y-b.Y,Z-b.Z};}
 Vector3 operator*(float b)const{return {X*b,Y*b,Z*b};}
 Vector3& operator+=(Vector3 b){X+=b.X;Y+=b.Y;Z+=b.Z;return *this;}
};
struct VertexFormatXYZDUV1 {float x,y,z;unsigned diffuse;float u1,v1;};
struct TRoadSegInfo {Vector2 loc,roadNormal,roadVector,corners[4];float uOffset,vOffset,scale;};
struct RoadSegment {
 std::vector<VertexFormatXYZDUV1> vertices;std::vector<UnsignedShort> indices;
 void SetRoadSegInfo(TRoadSegInfo*){}
 void SetVertexBuffer(VertexFormatXYZDUV1* p,int n){vertices.assign(p,p+n);}
 void SetIndexBuffer(UnsignedShort* p,int n){indices.assign(p,p+n);}
};
struct WorldHeightMap {
 UnsignedByte samples[64*64]{};
 int getMaxHeightValue(){return 255;} int getMinHeightValue(){return 0;}
 int getXExtent(){return 64;} int getYExtent(){return 64;}
 int getBorderSizeInline(){return 2;} UnsignedByte* getDataPtr(){return samples;}
};
struct TerrainVisual { WorldHeightMap* getLogicHeightMap(){return nullptr;} };
TerrainVisual* TheTerrainVisual=nullptr;
struct BaseHeightMapRenderObjClass {WorldHeightMap* m_map;Real getMaxCellHeight(Real,Real)const;};
BaseHeightMapRenderObjClass* TheTerrainRenderObject;
struct W3DRoadBuffer {
 WorldHeightMap* m_map;
 void loadFloat4PtSection(RoadSegment*,Vector2,Vector2,Vector2,Vector2*,Real,Real,Real,Real);
};
'''
    main = r'''
int main(){
 WorldHeightMap map;BaseHeightMapRenderObjClass terrain{&map};TheTerrainRenderObject=&terrain;W3DRoadBuffer roads{&map};
 for(int mode=0;mode<5;++mode){
  for(int y=0;y<64;++y)for(int x=0;x<64;++x){
   int h=mode==0?7:mode==1?x+y:mode==2?(x==9?23:6):mode==3?x/4:(x*7+y*11)%31;
   map.samples[y*64+x]=h;
  }
  for(int shape=0;shape<4;++shape){
   Vector2 origin(12.25f,35.5f),normal(0,16),direction(175.5f,0);
   Vector2 corners[4]={{12.25f,19.5f},{187.75f,19.5f},{12.25f,51.5f},{187.75f,51.5f}};
   if(shape==1){normal={-13.25f,9.5f};direction={123.75f,94.25f};corners[0]={4.5f,3.25f};corners[1]={138.5f,91.75f};corners[2]={-14.5f,29.75f};corners[3]={106.25f,122.5f};}
   if(shape==2){origin={-31.5f,-21.25f};direction={651.75f,81.5f};normal={-1.25f,8.5f};corners[0]={-33.5f,-25.75f};corners[1]={625.5f,44.25f};corners[2]={-31.5f,-3.25f};corners[3]={619.75f,60.25f};}
   if(shape==3){origin={20,20};direction={.25f,0};normal={0,4};corners[0]={20,16};corners[1]={20.25f,16};corners[2]={20,24};corners[3]={20.25f,24};}
   std::cout<<mode;
   auto input=[&](Vector2 v){std::cout<<' '<<v.X<<' '<<v.Y;};
   input(origin);input(normal);input(direction);for(auto p:corners)input(p);
   std::cout<<" 0.375 0.125 32 24\n";
   RoadSegment segment;roads.loadFloat4PtSection(&segment,origin,normal,direction,corners,.375f,.125f,32.f,24.f);
   std::cout<<segment.vertices.size()<<' '<<segment.indices.size()<<'\n';
   for(auto v:segment.vertices)std::cout<<std::bit_cast<uint32_t>(v.x)<<' '<<std::bit_cast<uint32_t>(v.y)<<' '<<std::bit_cast<uint32_t>(v.z)<<' '<<std::bit_cast<uint32_t>(v.u1)<<' '<<std::bit_cast<uint32_t>(v.v1)<<'\n';
   for(auto i:segment.indices)std::cout<<i<<' ';std::cout<<'\n';
  }
 }
}
'''
    return header + wrapper + extract(TERRAIN, 'Real BaseHeightMapRenderObjClass::getMaxCellHeight(') + extract(ROAD, 'void W3DRoadBuffer::loadFloat4PtSection(') + main


def main():
    if reference.run(['git','-C',str(UPSTREAM),'rev-parse','HEAD']) != reference.COMMIT:
        raise RuntimeError('Unexpected source revision')
    temp = reference_support.CACHE
    temp.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, TMPDIR=str(temp), CLANG_MODULE_CACHE_PATH=str(temp / 'clang-modules'))
    with tempfile.TemporaryDirectory(prefix='road-reference-', dir=temp) as directory:
        folder = Path(directory)
        cpp = folder / 'road.cpp'
        cpp.write_text(source())
        binary = folder / 'road'
        reference.run([reference_support.compiler("CXX", "clang++"),'-std=c++20','-O0','-ffp-contract=off',str(cpp),'-o',str(binary)], env=env)
        result = reference.run([str(binary)], env=env) + '\n'
    if '--write-fixture' in sys.argv:
        reference_support.write_fixture(FIXTURE, result)
        print('Wrote 20 extracted native road meshes; client parity test still required.')
    elif FIXTURE.read_text() != result:
        raise RuntimeError('Native road fixture differs from pinned-source output')
    else:
        print('PASS: 20 native road fixtures reproduce from pinned source; run focused Rust road geometry test to compare production.')


if __name__ == '__main__':
    main()
