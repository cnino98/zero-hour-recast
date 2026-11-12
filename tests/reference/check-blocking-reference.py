#!/usr/bin/env python3
"""Compare pinned AI blocking decisions with the actual Rust policy functions.

Object/AI/locomotor field accessors are wrappers. All fixtures have AI+physics;
formation IDs, state dispatch, contact discovery and pathfinding are not run.
"""
from pathlib import Path
import reference_support
import importlib.util
import json
import os
import tempfile

PROJECT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("path_reference", PROJECT / "tests/reference/check-path-reference.py")
reference = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reference)
UPSTREAM = reference.UPSTREAM
SOURCE = "GeneralsMD/Code/GameEngine/Source/GameLogic/Object/Update/AIUpdate.cpp"
PARTITION = "GeneralsMD/Code/GameEngine/Source/GameLogic/Object/PartitionManager.cpp"
TRIG = "GeneralsMD/Code/GameEngine/Source/Common/System/Trig.cpp"
MACROS = "Core/Libraries/Include/Precompiled/Precompiled/BaseMacros.h"
TIMINGS = "GeneralsMD/Code/GameEngine/Include/Common/UnitTimings.h"
RUST = PROJECT / "crates/generals-sim/src/skirmish/blocking/policy.rs"


def function(path, signature):
    source = (UPSTREAM / path).read_text()
    start = source.index(signature)
    return source[start:source.index("\n}\n", start) + 3]


def cases():
    actor = dict(id=1, position=[0, 0], facing=0, kind="Vehicle", dozer=False,
                 ground=True, reversing=False, goal_active=True, dead=False,
                 goal=[100, 100], cell=[10, 10], blocked=0, velocity=[1, 0], cap=10)
    other = dict(actor, id=2, position=[10, 0], velocity=[0.5, 0])
    result = []

    def add(name, a=None, b=None, crush=False):
        result.append((name, dict(actor, **(a or {})), dict(other, **(b or {})), crush))

    add("following")
    add("coincident", b=dict(position=[0, 0]))
    add("coincident_reversed_ids", a=dict(id=9), b=dict(position=[0, 0]))
    add("near_coincident", b=dict(position=[0.0999, 0]))
    add("outside_coincident", b=dict(position=[0.1001, 0]))
    add("ahead", b=dict(position=[-10, 0]))
    add("other_facing_us", b=dict(facing=3.1415927))
    add("self_facing_away", a=dict(facing=3.1415927))
    add("orthogonal_directions", b=dict(facing=1.5707964))
    add("dozer_wins", a=dict(dozer=True), b=dict(dozer=False))
    add("other_dozer_wins", a=dict(dozer=False), b=dict(dozer=True))
    add("vehicle_over_infantry", b=dict(kind="Infantry"))
    add("infantry_below_vehicle", a=dict(kind="Infantry"))
    add("both_infantry_aligned", a=dict(kind="Infantry"), b=dict(kind="Infantry"))
    add("both_infantry_stationary", a=dict(kind="Infantry"), b=dict(kind="Infantry", goal_active=False))
    add("both_infantry_crossing", a=dict(kind="Infantry"), b=dict(kind="Infantry", facing=1.5707964))
    add("other_airborne", b=dict(ground=False))
    add("reversing", a=dict(reversing=True))
    add("crushing_allowed", crush=True)
    add("dead_blocker", b=dict(dead=True))
    add("dead_coincident_priority", b=dict(dead=True, position=[0, 0]))
    add("near_goal", a=dict(goal=[9.999, 9.999], cell=[1, 1]))
    add("goal_boundary", a=dict(goal=[10, 0], cell=[1, 1]))
    add("zero_goal_cell_x", a=dict(goal=[1, 1], cell=[0, 1]))
    add("zero_goal_cell_y", a=dict(goal=[1, 1], cell=[1, 0]))
    add("no_goal_cell", a=dict(cell=None, goal=[1, 1]))
    add("blocked_thirty", a=dict(blocked=30), b=dict(facing=3.1415927))
    add("blocked_thirty_one", a=dict(blocked=31), b=dict(facing=3.1415927))
    add("stationary_in_front", b=dict(goal_active=False, velocity=[0, 0]))
    add("stationary_off_angle", b=dict(position=[10, 8], goal_active=False))
    add("stationary_angle_boundary_low", b=dict(position=[10, 6.6817], goal_active=False))
    add("stationary_angle_boundary_high", b=dict(position=[10, 6.6819], goal_active=False))
    add("moving_angle_boundary_low", b=dict(position=[10, 9.999]))
    add("moving_angle_boundary_high", b=dict(position=[10, 10.001]))
    add("same_cap", a=dict(cap=0.25))
    add("zero_speed", b=dict(velocity=[0, 0]))
    add("zero_cap", a=dict(cap=0))
    for index, (position, facing, velocity) in enumerate([
        ([3, 7], 0.2, [0.9, 0.3]), ([19, 13], 0.7, [0.3, 1.7]),
        ([0.3, 0.7], -0.2, [-0.01, 0.27]), ([123.25, 91.75], 0.8, [3.37, 8.25]),
        ([1.1, -9.7], -1.4, [0.004, -0.9]),
    ]):
        add(f"diagonal_normalization_{index}", a=dict(facing=facing / 2),
            b=dict(position=position, facing=facing, velocity=velocity))
    for angle in [0.0, 0.2, 0.7, -0.7, 1.4, 2.8]:
        add(f"crossing_{angle}", a=dict(facing=angle), b=dict(position=[8, 5], facing=-angle))
    return result


def cpp_source(fixtures):
    source = (UPSTREAM / SOURCE).read_text()
    wrappers = r'''
#include <bit>
#include <cmath>
#include <cstdint>
#include <iostream>
using Real=float;
using Bool=bool;
using Int=int;
using ObjectID=unsigned;
constexpr int KINDOF_DOZER=1, KINDOF_VEHICLE=2, KINDOF_INFANTRY=4;
constexpr int NONE=0, TEST_CRUSH_OR_SQUISH=0, NO_FORMATION_ID=0;
constexpr int LOGICFRAMES_PER_SECOND=30;
constexpr Real PATHFIND_CELL_SIZE_F=10.0f;
#define TRUE true
#define FALSE false
template<class T> T sqr(T value) { return value*value; }
struct Coord2D { Real x,y; COORD2_LENGTH COORD2_NORMALIZE };
struct Coord3D { Real x,y,z; COORD3_LENGTH };
struct ICoord2D { int x,y; };
struct PhysicsBehavior { Coord3D velocity; const Coord3D* getVelocity() const { return &velocity; } };
struct Locomotor { bool reverse; bool isMovingBackwards() const { return reverse; } };
struct Machine { Coord3D goal; const Coord3D* getGoalPosition() const { return &goal; } };
struct AIUpdateInterface;
struct Object {
    ObjectID id;
    Coord3D position, direction;
    int kind;
    bool crush;
    AIUpdateInterface* ai=nullptr;
    PhysicsBehavior physics;
    ObjectID getID() const { return id; }
    bool isKindOf(int flag) const { return (kind & flag)!=0; }
    const Coord3D* getPosition() const { return &position; }
    const Coord3D* getUnitDirectionVector2D() const { return &direction; }
    bool canCrushOrSquish(Object*, int) const { return crush; }
    AIUpdateInterface* getAI() const { return ai; }
    PhysicsBehavior* getPhysics() { return &physics; }
    int getFormationID() const { return NO_FORMATION_ID; }
};
struct AIUpdateInterface {
    Object* object;
    Machine machine;
    ICoord2D goal_cell;
    bool ground, dead;
    Locomotor locomotor;
    unsigned frames;
    int m_locomotorGoalType;
    Real m_curMaxBlockedSpeed;
    Object* getObject() const { return object; }
    const Machine* getStateMachine() const { return &machine; }
    const ICoord2D* getPathfindGoalCell() const { return &goal_cell; }
    bool isDoingGroundMovement() const { return ground; }
    Locomotor* getCurLocomotor() { return &locomotor; }
    unsigned getNumFramesBlocked() const { return frames; }
    bool isAiInDeadState() const { return dead; }
    Bool hasHigherPathPriority(AIUpdateInterface*) const;
    Real calculateMaxBlockedSpeed(Object*) const;
    Bool blockedBy(Object*);
};
struct PartitionManager { Real getRelativeAngle2D(const Object*,const Coord3D*); } partition;
PartitionManager* ThePartitionManager=&partition;
'''
    for name, path in [("COORD2", reference.COORD2), ("COORD3", reference.COORD3)]:
        wrappers = wrappers.replace(name + "_LENGTH", reference.original_line(path, "Real length() const"))
    coord = (UPSTREAM / reference.COORD2).read_text()
    start = coord.index("\tvoid normalize()")
    wrappers = wrappers.replace("COORD2_NORMALIZE", coord[start:coord.index("\n\t}\n", start) + 4])
    parts = [source.split("*/", 1)[0] + "*/\n", wrappers,
             reference.original_line(MACROS, "#define PI "),
             reference.original_line(reference.FIND_HEADER, "#define INFANTRY_MOVES_THROUGH_INFANTRY"),
             function(TRIG, "Real Sin(Real x)"), function(TRIG, "Real Cos(Real x)"),
             function(TRIG, "Real ACos(Real x)"),
             function(PARTITION, "Real PartitionManager::getRelativeAngle2D( const Object *obj, const Coord3D *pos )"),
             function(SOURCE, "Bool AIUpdateInterface::hasHigherPathPriority("),
             function(SOURCE, "Real AIUpdateInterface::calculateMaxBlockedSpeed("),
             function(SOURCE, "Bool AIUpdateInterface::blockedBy("), "int main() {"]
    parts.extend(f"static_assert(RETAIL_COMPATIBLE_{key} == {value});" for key, value in reference.PROFILE.items())
    for name, a, b, crush in fixtures:
        parts.append("{")
        for key, obj in [("a", a), ("b", b)]:
            kind = {"Other": 0, "Vehicle": 2, "Infantry": 4}[obj["kind"]] | int(obj["dozer"])
            position = reference.vector(obj["position"] + [0], "{", "}", "f")
            velocity = reference.vector(obj["velocity"] + [0], "{", "}", "f")
            parts.append(f"Object {key}{{{obj['id']},{position},{{Cos({float(obj['facing'])}f),Sin({float(obj['facing'])}f),0}}, {kind}, {str(crush).lower()},nullptr,{{{velocity}}}}};")
            cell = "{-1,-1}" if obj["cell"] is None else "{" + ",".join(map(str, obj["cell"])) + "}"
            goal = reference.vector(obj["goal"] + [0], "{", "}", "f")
            parts.append(f"AIUpdateInterface {key}_ai{{&{key},{{{goal}}},{cell},{str(obj['ground']).lower()},{str(obj['dead']).lower()},{{{str(obj['reversing']).lower()}}},{obj['blocked']},{int(obj['goal_active'])},{float(obj['cap'])}f}}; {key}.ai=&{key}_ai;")
        parts.append(f"std::cout << {json.dumps(name)};")
        for left, right in [("a", "b"), ("b", "a")]:
            parts.append(f"std::cout << ':' << {left}_ai.hasHigherPathPriority(&{right}_ai) << ':' << {left}_ai.blockedBy(&{right}) << ':' << std::bit_cast<std::uint32_t>({left}_ai.calculateMaxBlockedSpeed(&{right})) << ':' << std::bit_cast<std::uint32_t>(partition.getRelativeAngle2D(&{left},&{right}.position));")
        parts.append("std::cout << '\\n'; }")
    return "\n".join(parts) + "\n}\n"


def rust_source(fixtures):
    wrappers = r'''
mod skirmish {
    pub enum GroupKind { Infantry, Vehicle }
    pub struct GroundRules { pub is_dozer:bool }
    pub struct Locomotor { pub ground:Option<GroundRules> }
    pub struct UnitType { pub locomotor:Locomotor, pub group_kind:GroupKind }
    #[derive(Clone, Copy)]
    pub struct Cell { pub x:u16, pub y:u16 }
    pub struct Blocking { pub frames:u32, pub max_speed:f32 }
    impl Blocking {
        pub const fn command_goal(&self, fallback:Option<[f32;2]>)->Option<[f32;2]> { fallback }
    }
    pub struct Motion { pub velocity:[f32;2], pub reverse:bool, pub goal:bool }
    impl Motion {
        pub const fn moving_backwards(&self)->bool { self.reverse }
        pub const fn has_goal(&self)->bool { self.goal }
    }
    pub struct Command { pub formation_id:Option<usize> }
    pub struct Ai { pub path_goal:Option<Cell>, pub path_endpoint:Option<[f32;2]>,
        pub motion:Motion, pub blocking:Blocking, pub command:Command }
    pub struct Entity {
        pub position:[f32;2], pub facing:f32, pub ai:Ai, pub dead:bool,
    }
    impl Entity { pub const fn alive(&self)->bool { !self.dead } }
    pub mod blocking {
        #[path=POLICY_PATH] mod policy;
        pub fn report(a:&super::Entity, au:&super::UnitType, b:&super::Entity, bu:&super::UnitType, ids:(usize,usize), crush:bool) {
            let priority=policy::has_higher_priority(a,au,b,bu,ids);
            let blocked=policy::blocked_by(a,au,b,bu,crush,priority);
            let speed=policy::max_blocked_speed(a,b);
            let angle=policy::relative_angle(a.position,policy::direction(a),b.position);
            print!(":{}:{}:{}:{}",u8::from(priority),u8::from(blocked),speed.to_bits(),angle.to_bits());
        }
    }
}
use skirmish::*;
fn main() {
'''
    parts = [wrappers.replace("POLICY_PATH", json.dumps(str(RUST)))]
    for name, a, b, crush in fixtures:
        parts.append("{")
        for key, obj in [("a", a), ("b", b)]:
            ground = "None" if not obj["ground"] else f"Some(GroundRules{{is_dozer:{str(obj['dozer']).lower()}}})"
            parts.append(f"let {key}_unit=UnitType{{locomotor:Locomotor{{ground:{ground}}},group_kind:GroupKind::{obj['kind']}}};")
            cell = "None" if obj["cell"] is None else f"Some(Cell{{x:{obj['cell'][0]},y:{obj['cell'][1]}}})"
            position = reference.vector(obj["position"], "[", "]")
            velocity = reference.vector(obj["velocity"], "[", "]")
            goal = reference.vector(obj["goal"], "[", "]")
            parts.append(f"let {key}=Entity{{position:{position},facing:{float(obj['facing'])},ai:Ai{{path_goal:{cell},path_endpoint:Some({goal}),motion:Motion{{velocity:{velocity},reverse:{str(obj['reversing']).lower()},goal:{str(obj['goal_active']).lower()}}},blocking:Blocking{{frames:{obj['blocked']},max_speed:{float(obj['cap'])}}},command:Command{{formation_id:None}}}},dead:{str(obj['dead']).lower()}}};")
        parts.append(f"print!(\"{{}}\",{json.dumps(name)});")
        parts.append(f"blocking::report(&a,&a_unit,&b,&b_unit,({a['id']},{b['id']}),{str(crush).lower()});")
        parts.append(f"blocking::report(&b,&b_unit,&a,&a_unit,({b['id']},{a['id']}),{str(crush).lower()}); println!(); }}")
    return "\n".join(parts) + "\n}\n"


def main():
    if reference.run(["git", "-C", str(UPSTREAM), "rev-parse", "HEAD"]) != reference.COMMIT:
        raise RuntimeError("Upstream commit changed; review blocking extraction first")
    for path in [SOURCE, PARTITION, TRIG, MACROS, TIMINGS, reference.FIND_HEADER, reference.COORD2, reference.COORD3]:
        if reference.run(["git", "-C", str(UPSTREAM), "status", "--porcelain", "--", path]):
            raise RuntimeError(f"Reference source is modified: {path}")
    fixtures = cases()
    cache = reference_support.CACHE / "blocking-reference"
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="run-", dir=cache) as directory:
        work = Path(directory)
        env = os.environ.copy()
        env.update(TMPDIR=str(work), XDG_CACHE_HOME=str(work / "xdg"), CLANG_MODULE_CACHE_PATH=str(work / "modules"))
        cpp, rust = work / "reference.cpp", work / "port.rs"
        cpp.write_text(cpp_source(fixtures))
        rust.write_text(rust_source(fixtures))
        clang = reference_support.compiler("CXX", "clang++")

        rustc = reference_support.compiler("RUSTC", "rustc")
        flags = [f"-DRETAIL_COMPATIBLE_{key}={value}" for key, value in reference.PROFILE.items()]
        native, port = work / "reference", work / "port"
        reference.run([clang, "-std=c++20", "-Wall", "-Wextra", "-Werror", "-ffp-contract=off", *flags, str(cpp), "-o", str(native)], env=env)
        reference.run([rustc, "--edition=2024", "-Dwarnings", str(rust), "-o", str(port)], env=env)
        original, translated = reference.run([str(native)], env=env), reference.run([str(port)], env=env)
        if len(original.splitlines()) != len(fixtures) or len(translated.splitlines()) != len(fixtures):
            raise RuntimeError("Blocking comparison did not emit every fixture")
        if original != translated:
            differences = [f"C++: {left}\nRust: {right}" for left, right in zip(original.splitlines(), translated.splitlines()) if left != right]
            raise RuntimeError("Blocking mismatch:\n" + "\n".join(differences[:10]))
        print(f"PASS: {len(fixtures)} blocking fixtures, {len(fixtures)*2} directed decisions match original C++.")
        print("Compared priority/blocking booleans and exact f32 speed-cap/relative-angle bits.")
        print("Source INFANTRY_MOVES_THROUGH_INFANTRY enabled; formation IDs absent on both sides.")
        print("Scope: actual Rust policy functions vs verbatim pinned C++; field accessors only.")
        print("Contact dispatch, temporary states, recovery/pathfinding and full-engine parity are not covered.")


if __name__ == "__main__":
    main()
