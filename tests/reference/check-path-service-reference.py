#!/usr/bin/env python3
"""Compare extracted native queue/service functions against the production Rust.

Search callbacks are controlled fixtures. This verifies FIFO, capacity, callbacks,
zone suspension and whole-query cell budgets, not the complete C++ path search.
"""
from pathlib import Path
import reference_support
import importlib.util
import os
import sys
import tempfile
import native_real_reference

PROJECT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("path_reference", PROJECT / "tests/reference/check-path-reference.py")
reference = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reference)
UPSTREAM = reference.UPSTREAM
SOURCE = reference.SOURCE


def extract(text, signature):
    start = text.index(signature)
    opening = text.index("{", start)
    depth = 1
    cursor = opening + 1
    while depth:
        if text[cursor] == "{":
            depth += 1
        elif text[cursor] == "}":
            depth -= 1
        cursor += 1
    return text[start:cursor]


def operations():
    result = [("queue", 0, 4999), ("queue", 1, 2), ("queue", 2, 1), ("frame",), ("frame",)]
    result += [("queue", 3, 5000), ("queue", 4, 0), ("frame",), ("frame",)]
    result += [("queue", 5, 9000), ("queue", 6, 1), ("frame",), ("frame",)]
    result += [("queue", 7, 2000), ("queue", 8, 3000), ("cancel", 7), ("frame",)]
    result += [("queue", 9, 5000), ("queue", 10, 2), ("queue", 9, 3), ("zone",), ("frame",), ("frame",)]
    result += [("queue", 11, 3000), ("repeat", 11), ("queue", 12, 1), ("frame",), ("frame",)]
    result += [("queue", 13, 5000), ("kill", 13), ("queue", 14, 1), ("frame",)]
    result += [("no_ai", 15), ("queue", 15, 10), ("queue", 16, 1), ("frame",)]
    # Capacity, canceled tombstones, and FIFO wrap; each queue uses a live ID.
    result += [("queue", index, 11) for index in range(100, 612)]
    result += [("queue", 200, 12), ("cancel", 205), ("frame",)]
    result += [("queue", index, 17) for index in range(620, 720)]
    result += [("frame",), ("frame",), ("frame",)]
    return result


def cpp_source():
    source = (UPSTREAM / SOURCE).read_text()
    header = source.split("*/", 1)[0] + "*/\n"
    queue_constant = reference.original_line(reference.FIND_HEADER, "enum { PATHFIND_QUEUE_LEN=")
    budget_constant = reference.original_line(SOURCE, "constexpr const UnsignedInt PATHFIND_CELLS_PER_FRAME =")
    stubs = r'''
#include <array>
#include <cmath>
#include <iostream>
#include <vector>
using Bool = bool;
using Int = int;
using ObjectID = int;
constexpr int INVALID_ID = -1;
constexpr float PATHFIND_CELL_SIZE_F = 10.0f;
#define DEBUG_CRASH(...)
#define PROFILER_PLOT(...)
struct Coord { float x=0, y=0, z=0; };
struct Region3D { Coord lo, hi; };
struct ICoord2D { int x=0, y=0; };
struct IRegion2D { ICoord2D lo, hi; };
struct Terrain { void getExtent(Region3D* value) { value->hi.x=1000; value->hi.y=1000; } };
Terrain terrain;
Terrain* TheTerrainLogic = &terrain;
struct Zones {
    bool dirty=false;
    bool needToCalculateZones() { return dirty; }
    void calculateZones(int,int,int) { dirty=false; }
};
struct Pathfinder;
struct AIUpdateInterface {
    int id=0, cells=0;
    bool waiting=false, repeat=false;
    void doPathfind(Pathfinder*);
};
struct Object {
    bool alive=true, hasAI=true;
    AIUpdateInterface ai;
    AIUpdateInterface* getAIUpdateInterface() { return hasAI ? &ai : nullptr; }
};
struct Logic {
    unsigned frame=0;
    unsigned getFrame() const { return frame; }
    std::array<Object, 1024> objects;
    Object* findObjectByID(int id) { return id>=0 && objects[id].alive ? &objects[id] : nullptr; }
};
Logic logic;
Logic* TheGameLogic = &logic;
#define MIN(a,b) ((a)<(b)?(a):(b))
struct PathfindZoneManager {
    unsigned m_nextFrameToCalculateZones=~0u;
    void markZonesDirty();
};
struct Pathfinder {
    std::array<int, PATHFIND_QUEUE_LEN> m_queuedPathfindRequests;
    int m_queuePRHead=0, m_queuePRTail=0;
    unsigned int m_cumulativeCellsAllocated=0;
    bool m_isMapReady=true;
    Zones m_zoneManager;
    int m_map=0, m_layers=0, m_extent=0;
    IRegion2D m_logicalExtent;
    std::vector<int> served;
    Pathfinder() { m_queuedPathfindRequests.fill(INVALID_ID); }
    Bool queueForPath(ObjectID);
    void processPathfindQueue();
};
void AIUpdateInterface::doPathfind(Pathfinder* path) {
    if (!waiting) return;
    waiting=false;
    path->served.push_back(id);
    path->m_cumulativeCellsAllocated += cells;
    if (repeat) { repeat=false; waiting=true; path->queueForPath(id); }
}
'''
    body = []
    for op in operations():
        if op[0] == "queue":
            _, entity, cells = op
            body.append(f'logic.objects[{entity}].ai.waiting=true; logic.objects[{entity}].ai.cells={cells}; std::cout << "Q " << path.queueForPath({entity}) << "\\n";')
        elif op[0] == "frame":
            body.append('path.served.clear(); path.processPathfindQueue(); std::cout << "F " << path.m_queuePRHead << " " << path.m_queuePRTail << " " << path.m_cumulativeCellsAllocated; for(int id:path.served) std::cout << " " << id; std::cout << "\\n";')
        elif op[0] == "zone":
            body.append('path.m_zoneManager.dirty=true;')
        elif op[0] == "cancel":
            body.append(f'logic.objects[{op[1]}].ai.waiting=false;')
        elif op[0] == "kill":
            body.append(f'logic.objects[{op[1]}].alive=false;')
        elif op[0] == "repeat":
            body.append(f'logic.objects[{op[1]}].ai.repeat=true;')
        elif op[0] == "no_ai":
            body.append(f'logic.objects[{op[1]}].hasAI=false;')
    for frames in [(0, 1, 2), (20, 10, 30), (2, 0, 2), (1000, 1001)]:
        body.append("{ PathfindZoneManager zones;")
        for frame in frames:
            body.append(f'logic.frame={frame}; zones.markZonesDirty(); std::cout << "Z " << zones.m_nextFrameToCalculateZones << "\\n";')
        body.append("}")
    main = 'int main() { Pathfinder path; for(int id=0;id<1024;++id) logic.objects[id].ai.id=id;\n' + '\n'.join(body) + '\n}'
    return header + native_real_reference.cpp(reference) + 'using UnsignedInt = unsigned int;\n' + queue_constant + '\n' + budget_constant + '\n' + stubs + reference.original_line(SOURCE, 'constexpr const UnsignedInt ZONE_UPDATE_FREQUENCY =') + '\n' + extract(source, 'void PathfindZoneManager::markZonesDirty()') + '\n' + extract(source, 'Bool Pathfinder::queueForPath(') + '\n' + extract(source, 'void Pathfinder::processPathfindQueue()') + '\n' + main


def rust_source():
    source = (PROJECT / 'crates/generals-sim/src/skirmish/path_service.rs').read_text()
    service = extract(source, 'pub(super) struct Service') + '\n' + extract(source, 'impl Service')
    process = extract(source, '    pub(super) fn process_path_service(')
    stubs = r'''
struct Request { cells: usize }
struct State { pending: Option<Request> }
enum UnitAiUpdate {Absent,Ordinary}
struct Ai {update:UnitAiUpdate}
struct Unit {ai:Ai}
struct ObjectAi { path_request: State }
struct Entity { ai: ObjectAi, alive: bool, repeat: bool,type_index:usize }
impl Entity { fn exists(&self) -> bool { self.alive } }
struct Grid;
impl Grid {fn recalculate_source_zones(&mut self) {}}
struct Skirmish { path_service: Service, entities: Vec<Entity>, served: Vec<usize>,catalog:Vec<Unit>,tick:u64,navigation:Grid,cliff_navigation:Option<Grid> }
impl Skirmish {
    fn rebuild_rubble(&mut self) {}
    // Same domain callback boundary as native AIUpdateInterface::doPathfind.
    // Each fixture completes one whole request and may enqueue a replacement.
    fn do_ground_pathfind(&mut self,id:usize,request:Request) {
        self.served.push(id);
        self.path_service.cumulative_cells+=request.cells;
        if self.entities[id].repeat {
            self.entities[id].repeat=false;
            self.entities[id].ai.path_request.pending=Some(Request {cells:request.cells});
            self.path_service.queue(id);
        }
    }
}
'''
    # process_path_service's super::Entity resolves to this enclosing test fixture.
    types, impl = stubs.split('impl Skirmish {', 1)
    text = 'mod fixture {\n' + types + '\nmod queue { use super::*;\n' + service + '\nimpl Skirmish {' + impl + '\nimpl Skirmish {\n' + process + '\n}\n'
    body = []
    for op in operations():
        if op[0] == 'queue':
            _, entity, cells = op
            body.append(f'game.entities[{entity}].ai.path_request.pending=Some(Request{{cells:{cells}}}); println!("Q {{}}",usize::from(game.path_service.queue({entity})));')
        elif op[0] == 'frame':
            body.append('game.served.clear(); game.process_path_service(); print!("F {} {} {}", game.path_service.head,game.path_service.tail,game.path_service.cumulative_cells); for id in &game.served {print!(" {id}");} println!();')
        elif op[0] == 'zone':
            body.append('game.path_service.zone_recalculate_at=Some(game.tick);')
        elif op[0] == 'cancel':
            body.append(f'game.entities[{op[1]}].ai.path_request.pending=None;')
        elif op[0] == 'kill':
            body.append(f'game.entities[{op[1]}].alive=false;')
        elif op[0] == 'repeat':
            body.append(f'game.entities[{op[1]}].repeat=true;')
        elif op[0] == 'no_ai':
            body.append(f'game.entities[{op[1]}].type_index=1;')
    for frames in [(0, 1, 2), (20, 10, 30), (2, 0, 2), (1000, 1001)]:
        body.append("{ let mut zones=Service::new();")
        for frame in frames:
            body.append(f'zones.mark_zones_dirty({frame}); println!("Z {{}}", zones.zone_recalculate_at.unwrap());')
        body.append("}")
    text += 'pub fn run() {let mut game=Skirmish{path_service:Service::new(), entities:(0..1024).map(|_|Entity{ai:ObjectAi{path_request:State{pending:None}},alive:true,repeat:false,type_index:0}).collect(),served:Vec::new(),catalog:vec![Unit{ai:Ai{update:UnitAiUpdate::Ordinary}},Unit{ai:Ai{update:UnitAiUpdate::Absent}}],tick:0,navigation:Grid,cliff_navigation:None};\n' + '\n'.join(body) + '\n}}\nuse queue::Service; pub fn run() { queue::run(); }\n}\nfn main() { fixture::run(); }'
    return text


def main():
    run = reference.run
    if run(['git', '-C', str(UPSTREAM), 'rev-parse', 'HEAD']) != reference.COMMIT:
        raise RuntimeError('Pinned source changed')
    for path in [SOURCE, reference.FIND_HEADER]:
        if run(['git', '-C', str(UPSTREAM), 'status', '--porcelain', '--', path]):
            raise RuntimeError(f'Reference modified: {path}')
    cache = reference_support.CACHE / 'path-service-reference'
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='run-', dir=cache) as directory:
        work = Path(directory)
        env = os.environ.copy()
        env.update(TMPDIR=str(work), XDG_CACHE_HOME=str(work/'xdg'), CLANG_MODULE_CACHE_PATH=str(work/'modules'))
        cpp, rust = work/'reference.cpp', work/'port.rs'
        cpp.write_text(cpp_source())
        rust.write_text(rust_source())
        clang = reference_support.compiler("CXX", "clang++")

        rustc = reference_support.compiler("RUSTC", "rustc")
        native, port = work/'reference', work/'port'
        run([clang, '-std=c++20', '-Wall', '-Wextra', '-Werror', str(cpp), '-o', str(native)], env=env)
        run([rustc, '--edition=2024', '-Dwarnings', str(rust), '-o', str(port)], env=env)
        expected, actual = run([str(native)], env=env), run([str(port)], env=env)
        if expected != actual:
            differences = [f'C++: {a}\nRust: {b}' for a,b in zip(expected.splitlines(), actual.splitlines()) if a != b]
            raise RuntimeError('Native queue mismatch:\n' + '\n'.join(differences[:10]))
        print(f'PASS: {len(operations())} native queue/service operations match the production Rust extraction.')
        print('Capacity, FIFO wrap, duplicate replacement, canceled/deleted/AI-less objects, callback requeue, zone suspension/deadlines and 4999/5000/overshoot boundaries match.')
        print('Search callbacks supply observed-cell costs; path search geometry and complete engine scheduling are outside this fixture.')


if __name__ == '__main__':
    main()
