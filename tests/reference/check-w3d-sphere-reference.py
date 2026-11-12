#!/usr/bin/env python3
"""Compare ordered HLOD unions using the pinned SphereClass::Add_Sphere body."""
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
FIXTURE = PROJECT / "tests/reference/w3d-sphere-reference.txt"


def source():
    native = (reference.UPSTREAM / "Core/Libraries/Source/WWVegas/WWMath/sphere.h").read_text()
    start = native.index("inline void SphereClass::Add_Sphere(")
    body = native[start:native.index("\n}\n", start) + 3]
    return r'''
#include <array>
#include <bit>
#include <cmath>
#include <iostream>
struct Vector3 {
 float X,Y,Z;
 Vector3 operator-(const Vector3& s)const{return {X-s.X,Y-s.Y,Z-s.Z};}
 Vector3 operator+(const Vector3& s)const{return {X+s.X,Y+s.Y,Z+s.Z};}
 Vector3 operator*(float s)const{return {X*s,Y*s,Z*s};}
 float Length()const{return std::sqrt(X*X+Y*Y+Z*Z);}
};
struct SphereClass {
 Vector3 Center;float Radius;
 bool Is_Valid()const{return Radius>0.0f;}
 void Init(Vector3 c,float r){Center=c;Radius=r;}
 void Add_Sphere(const SphereClass&);
};
''' + body + r'''
int main(){
 const std::array<SphereClass,4> spheres={SphereClass{{0,0,0},1},{{10,0,0},1},{{0,10,0},1},{{-10,-10,0},2}};
 const std::array<std::array<int,4>,3> orders={{{0,1,2,-1},{2,1,0,-1},{0,1,2,3}}};
 for(auto order:orders){
  auto result=spheres[order[0]];
  for(int i=1;i<4&&order[i]>=0;i++)result.Add_Sphere(spheres[order[i]]);
  std::cout<<std::bit_cast<unsigned>(result.Center.X)<<' '<<std::bit_cast<unsigned>(result.Center.Y)<<' '<<std::bit_cast<unsigned>(result.Center.Z)<<' '<<std::bit_cast<unsigned>(result.Radius)<<'\n';
 }
}
'''


def main():
    if reference.run(["git", "-C", str(reference.UPSTREAM), "rev-parse", "HEAD"]) != reference.COMMIT:
        raise RuntimeError("Unexpected pinned source revision")
    temp = reference_support.CACHE
    temp.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, TMPDIR=str(temp), CLANG_MODULE_CACHE_PATH=str(temp / "clang-modules"))
    with tempfile.TemporaryDirectory(prefix="w3d-sphere-reference-", dir=temp) as directory:
        folder = Path(directory)
        cpp = folder / "sphere.cpp"
        cpp.write_text(source())
        binary = folder / "sphere"
        reference.run([reference_support.compiler("CXX", "clang++"), "-std=c++20", "-O0", "-ffp-contract=off", str(cpp), "-o", str(binary)], env=env)
        result = reference.run([str(binary)], env=env) + "\n"
    if "--write-fixture" in sys.argv:
        reference_support.write_fixture(FIXTURE, result)
        print("Wrote three ordered native sphere unions; Rust consumer required.")
    elif FIXTURE.read_text() != result:
        raise RuntimeError("Pinned ordered sphere fixture changed")
    else:
        print("PASS: pinned ordered SphereClass::Add_Sphere unions.")


if __name__ == "__main__":
    main()
