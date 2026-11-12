#!/usr/bin/env python3
"""Compare pinned ground and waypoint-arrival path samplers with the Rust module.

Path storage, object layers and line-passability answers are wrappers.
The pathfinder, terrain/layers, locomotor and engine scheduler are not executed.
Generated files and both compilers' temporary/cache output stay in the external test output directory.
"""
from pathlib import Path
import reference_support
import json
import os
import subprocess
import tempfile


PROJECT = Path(__file__).resolve().parents[2]
UPSTREAM = reference_support.UPSTREAM
COMMIT = "2bfaea60b3b9d5a798a0d0a1aff7ea61b6cee3ed"
SOURCE = "Core/GameEngine/Source/GameLogic/AI/AIPathfind.cpp"
PATH_HEADER = "Core/GameEngine/Include/GameLogic/Pathfinder/Path.h"
FIND_HEADER = "Core/GameEngine/Include/GameLogic/AIPathfind.h"
COORD2 = "Core/Libraries/Include/Lib/Coord2D.h"
COORD3 = "Core/Libraries/Include/Lib/Coord3D.h"
PROFILE = {
    "CRC": 0, "PATHFINDING": 0, "PATHFINDING_ALLOCATION": 0,
    "CIRCLE_FILL_ALGORITHM": 0, "AIGROUP": 0, "XFER_SAVE": 0,
    "DATA": 1, "NETWORKING": 1,
}

# Nodes are (XYZ, can-optimize). Queries are (XYZ, passability mode).
# Modes: 0 always clear; 1 always blocked; 2 x=60 only; 3 y=0 only.
CORNER = [([0, 0, 2], True), ([10, 0, 3], True), ([10, 10, 4], True)]
CASES = [
    ("corner_probes", CORNER, [([6, 0, 2], 3), ([9.5, 0, 2], 3)]),
    ("nonoptimizable_corner", [(p, i != 1) for i, (p, _) in enumerate(CORNER)],
     [([6, 0, 2], 0), ([9.5, 0, 2], 0)]),
    ("blocked_projection", [([0, 0, 4], True), ([100, 0, 8], True)],
     [([20, 30, 6], 2), ([21, 30, 6], 1), ([21, 12, 6], 1)]),
    ("diagonal_projection", [([-5, 3, 2], True), ([19, 17, 8], True),
                             ([47, -13, 2], True)],
     [([-12, 0, 0], 1), ([13.25, 29.5, 0], 1), ([13.25, 29.5, 1], 0),
      ([47.25, -13.5, 0], 1)]),
    ("cache_expiry_and_height", CORNER,
     [([6, 0, 2], 0)] + [([6.05, 0, 2], 1)] * 21 + [([6.05, 0, 2.11], 0)]),
    ("coincident_nodes", [([0, 0, 0], True), ([0, 0, 0], True),
                           ([10, 0, 0], True)],
     [([0, 0, 0], 0), ([0.2, 0, 0], 1), ([10.1, 0, 0], 0)]),
    ("single_node", [([1, 2, 3], True)], [([10, 20, 30], 0)]),
    ("empty_path", [], [([0, 0, 0], 0)]),
    ("waypoint_pass_and_reverse", [([0, 0, 0], True), ([100, 0, 0], True),
                                  ([100, 100, 0], True), ([200, 100, 0], True)],
     [([101, 101, 0], 0), ([50, 50, 1000], 0), ([201, 113, 0], 0), ([150, 100, 0], 0)]),
    ("technical_final_rally", [([718.60516, 2119.6416, 0], False),
                               ([795, 2075, 0], True), ([825, 2085, 0], True),
                               ([825, 2105, 0], True), ([835, 2105, 0], True),
                               ([865, 2135, 0], True), ([885, 2135, 0], True)],
     [([874.20135, 2141.3933, 0], 0), ([886, 2148, 0], 0)]),
]


# Original ground/bridge layer IDs from Core GameType.h. This exercises the
# verbatim sampler's selected output layer and layer-dependent lookahead.
CASES.append(("bridge_entry_deck_exit", [([0, 0, 0], True), ([10, 0, 0], True),
    ([10, 0, 20], True), ([30, 0, 20], True), ([30, 0, 0], True), ([50, 0, 0], True)],
    [([5, 0, 0], 0), ([10.1, 0, 1], 0), ([20, 0, 20], 0), ([29.5, 0, 20], 0),
     ([40, 0, 0], 0)]))
NODE_LAYERS = {"bridge_entry_deck_exit": [1, 1, 2, 2, 1, 1]}
OBJECT_LAYERS = {"bridge_entry_deck_exit": [1, 2, 2, 2, 1]}


# These source-preserved upper/ground cases distinguish raw getPrevious from
# optimized predecessors and record the layer of every line-passability probe.
CASES.extend([
    ("raw_ground_predecessor_skipped", [([0, 0, 20], True), ([20, 0, 0], True), ([40, 0, 0], True)],
     [([25, 0, 0], 0)]),
    ("midpoint_uses_next_layer", [([0, 0, 20], True), ([10, 0, 0], True), ([20, 0, 0], True),
                                 ([30, 0, 0], True), ([30, 10, 0], True)],
     [([26, 0, 0], 4)]),
])
NODE_LAYERS.update(raw_ground_predecessor_skipped=[2, 1, 1], midpoint_uses_next_layer=[2, 1, 1, 1, 1])
OBJECT_LAYERS.update(raw_ground_predecessor_skipped=[1], midpoint_uses_next_layer=[1])
PREVIOUS_LAYERS = {"raw_ground_predecessor_skipped": [1, 1, 1]}

def previous_layers(name, nodes):
    return PREVIOUS_LAYERS.get(name, [1] + NODE_LAYERS.get(name, [1] * len(nodes))[:-1])


def run(arguments, *, env=None):
    local_env = os.environ.copy() if env is None else dict(env)
    local_env["GIT_OPTIONAL_LOCKS"] = "0"
    result = subprocess.run(arguments, cwd=PROJECT, env=local_env, check=False,
                            capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(f"{arguments[0]} failed ({result.returncode}):\n"
                           f"{result.stderr}\n{result.stdout}")
    return result.stdout.strip()


def original_function(signature):
    source = (UPSTREAM / SOURCE).read_text()
    start = source.index(signature)
    return source[start:source.index("\n}\n", start) + 3]


def original_line(path, marker):
    lines = [line.strip() for line in (UPSTREAM / path).read_text().splitlines()
             if marker in line]
    if len(lines) != 1:
        raise RuntimeError(f"Reference declaration is not unique: {path}: {marker}")
    return lines[0]


def vector(values, opening, closing, suffix=""):
    return opening + ",".join(repr(float(value)) + suffix for value in values) + closing


def cpp_source():
    header = (UPSTREAM / SOURCE).read_text().split("*/", 1)[0] + "*/\n"
    wrappers = r'''
#include <bit>
#include <cmath>
#include <cstdint>
#include <iostream>
#include <vector>
using Real = float;
using Bool = bool;
constexpr int LAYER_GROUND = 1;
constexpr Real PATHFIND_CELL_SIZE_F = 10.0f;
#define CRCDEBUG_LOG(...)
#define DUMPCOORD3D(...)
template<class T> T sqr(T value) { return value * value; }
struct Coord2D { Real x, y; COORD2_LENGTH
    void normalize() { Real len = length(); if (len != 0) { x /= len; y /= len; } }
};
struct Coord3D { Real x, y, z; COORD3_LENGTH
    void zero() { x = 0.0f; y = 0.0f; z = 0.0f; }
};
struct Object { int layer = LAYER_GROUND; int getLayer() const { return layer; } };
struct LocomotorSet { int getValidSurfaces() const { return 1; } };
struct PathNode {
    Coord3D m_pos{};
    Bool m_canOptimize = false;
    int m_layer = LAYER_GROUND;
    PathNode *m_nextOpti = nullptr, *m_prev = nullptr;
    Coord2D m_nextOptiDirNorm2D{};
    Real m_nextOptiDist2D = 0;
    const Coord3D* getPosition() const { return &m_pos; }
    int getLayer() const { return m_layer; }
    const PathNode* getPrevious() const { return m_prev; }
    Bool getCanOptimize() const { return m_canOptimize; }
    const PathNode* getNextOptimized(Coord2D* dir=nullptr, Real* distance=nullptr) const {
        if (dir) *dir = m_nextOptiDirNorm2D;
        if (distance) *distance = m_nextOptiDist2D;
        return m_nextOpti;
    }
    void setNextOptimized(PathNode* node);
};
struct ClosestPointOnPathInfo { Real distAlongPath; Coord3D posOnPath; int layer; };
struct Path {
    CACHE_CONSTANT
    PathNode *m_path = nullptr, *m_pathTail = nullptr;
    const PathNode *m_cpopRecentStart = nullptr;
    Bool m_cpopValid = false;
    int m_cpopCountdown = 0;
    Coord3D m_cpopIn{};
    ClosestPointOnPathInfo m_cpopOut{};
    const PathNode* getLastNode() const { return m_pathTail; }
    const PathNode* getFirstNode() const { return m_path; }
    void computePointOnPath(const Object*, const LocomotorSet&, const Coord3D&, ClosestPointOnPathInfo&);
    Real computeFlightDistToGoal(const Coord3D*, Coord3D&);
};
int mode = 0;
std::vector<Coord3D> probes;
std::vector<int> probeLayers;
struct Pathfinder {
    bool isLinePassable(const Object*, int, int layer, Coord3D, Coord3D to, bool, bool) {
        probes.push_back(to); probeLayers.push_back(layer);
        return mode == 0 || (mode == 2 && to.x == 60.0f) || (mode == 3 && to.y == 0.0f) || (mode == 4 && layer == 2);
    }
    void setDebugPathPosition(const Coord3D*) {}
};
struct AI { Pathfinder finder; Pathfinder* pathfinder() { return &finder; } } ai;
AI* TheAI = &ai;
void bits(Real value) { std::cout << ':' << std::bit_cast<std::uint32_t>(value); }
void position(Coord3D point) { bits(point.x); bits(point.y); bits(point.z); }
'''
    wrappers = wrappers.replace("COORD2_LENGTH", original_line(COORD2, "Real length() const"))
    wrappers = wrappers.replace("COORD3_LENGTH", original_line(COORD3, "Real length() const"))
    wrappers = wrappers.replace("CACHE_CONSTANT", original_line(PATH_HEADER, "enum {MAX_CPOP="))
    parts = [header, "// Extracted from pinned SuperHackers " + COMMIT, wrappers,
             original_line(FIND_HEADER, "#define PATHFIND_CLOSE_ENOUGH "),
             original_function("void PathNode::setNextOptimized("),
             original_function("inline Bool isReallyClose("),
             original_function("void Path::computePointOnPath("),
             original_function("Real Path::computeFlightDistToGoal("), "int main() {"]
    parts.extend(f"static_assert(RETAIL_COMPATIBLE_{key} == {value});"
                 for key, value in PROFILE.items())
    for name, nodes, queries in CASES:
        parts.append(f"{{ std::vector<PathNode> nodes({len(nodes)}), predecessors({len(nodes)}); Path path;")
        for index, (point, optimize) in enumerate(nodes):
            parts.append(f"nodes[{index}].m_pos = {vector(point, '{', '}', 'f')};"
                         f"nodes[{index}].m_canOptimize = {str(optimize).lower()};"
                         f"nodes[{index}].m_layer = {NODE_LAYERS.get(name, [1]*len(nodes))[index]};")
            if index:
                parts.append(f"nodes[{index-1}].setNextOptimized(&nodes[{index}]);"
                             f"nodes[{index}].m_prev = &nodes[{index-1}];")
        if name in PREVIOUS_LAYERS:
            for index, layer in enumerate(previous_layers(name, nodes)):
                parts.append(f"predecessors[{index}].m_layer = {layer}; nodes[{index}].m_prev = &predecessors[{index}];")
        if nodes:
            parts.append("path.m_path = &nodes.front(); path.m_pathTail = &nodes.back();")
        parts.append(f"std::cout << {json.dumps(name + '/initial_route')};"
                     "for (const PathNode* node=path.m_path; node; node=node->getNextOptimized()) {"
                     "bits(node->getPosition()->x); bits(node->getPosition()->y); }"
                     "std::cout << '\\n';")
        for index, (point, query_mode) in enumerate(queries):
            parts.append(f"{{ mode = {query_mode}; object.layer = {OBJECT_LAYERS.get(name,[1]*len(queries))[index]}; probes.clear(); probeLayers.clear(); ClosestPointOnPathInfo result{{}};"
                         f"path.computePointOnPath(&object, locomotor, {vector(point, '{', '}', 'f')}, result);"
                         f"std::cout << {json.dumps(name + '/' + str(index))};"
                         "position(result.posOnPath); bits(result.distAlongPath); std::cout << ':' << result.layer;"
                         "std::cout << ':' << probes.size(); for (std::size_t j=0; j<probes.size(); ++j) { std::cout << ':' << probeLayers[j]; position(probes[j]); }"
                         "std::cout << '\\n'; }")
            parts.append(f"{{ Coord3D point = {vector(point, '{', '}', 'f')}, goal{{}};"
                         "Real distance = path.computeFlightDistToGoal(&point, goal);"
                         f"std::cout << {json.dumps(name + '/flight/' + str(index))};"
                         "position(goal); bits(distance); std::cout << '\\n'; }")
        parts.append(f"std::cout << {json.dumps(name + '/retained_route')};"
                     "for (const PathNode* node=path.m_path; node; node=node->getNextOptimized()) {"
                     "bits(node->getPosition()->x); bits(node->getPosition()->y); }"
                     "std::cout << '\\n';")
        parts.append("}")
    parts.insert(parts.index("int main() {") + 1,
                 "Object object; LocomotorSet locomotor;")
    return "\n".join(parts) + "\n}\n"


def rust_sampler():
    source = (PROJECT / "crates/generals-sim/src/skirmish/path_following.rs").read_text()
    # Keep the production samplers/helpers verbatim. Debug formatting and caller
    # arrival adapters have no native counterpart in this comparison; importing
    # them would require dummy calls or warning suppression, not more coverage.
    for signature in ["    pub(super) fn sampling_details(",
                      "    pub(super) fn last_position(",
                      "    pub(super) fn update_last_position(",
                      "    pub(super) const fn is_direct(",
                      "    pub fn distance_to_goal(",
                      "    pub fn waypoint_distance_to_goal(",
                      "    pub fn arrived("]:
        if source.count(signature) != 1:
            raise RuntimeError(f"Review changed Rust comparison scope: {signature.strip()}")
        start = source.index(signature)
        end = source.index("\n    }\n", start) + len("\n    }\n")
        while start:
            previous = source.rfind("\n", 0, start - 1) + 1
            if not source[previous:start].lstrip().startswith("///"):
                break
            start = previous
        source = source[:start] + source[end:]
    return source


def rust_source():
    parts = ["mod path_following {", rust_sampler(), "}", "fn main() {"]
    for name, nodes, queries in CASES:
        entries = ["path_following::Node { position: " + vector(point, "[", "]")
                   + ", layer: " + str(NODE_LAYERS.get(name,[1]*len(nodes))[index]) + ", previous_layer: " + str(previous_layers(name, nodes)[index]) + ", can_optimize: " + str(optimize).lower() + " }"
                   for index, (point, optimize) in enumerate(nodes)]
        parts.append("{ let mut path = path_following::Path::new(vec![" + ",".join(entries) + "]);")
        parts.append(f"print!(\"{{}}\", {json.dumps(name + '/initial_route')});"
                     "for position in path.positions() { for value in position {"
                     "print!(\":{}\",value.to_bits()); }} println!();")
        for index, (point, mode) in enumerate(queries):
            parts.append("{ let mut probes = Vec::new(); let point = " + vector(point, "[", "]") + ";"
                         + (f"let sample = path.sample_layer(point, {OBJECT_LAYERS[name][index]}, |layer, _, to| {{ probes.push((layer, to)); " if name in OBJECT_LAYERS else "let sample = path.sample_layer(point, 1, |_, _, to| { probes.push((1_u8, to)); ")
                         + ["true", "false", "to[0] == 60.0", "to[1] == 0.0", "layer == 2"][mode] + " });"
                         f"print!(\"{{}}\", {json.dumps(name + '/' + str(index))});"
                         "for value in sample.position { print!(\":{}\", value.to_bits()); }"
                         "print!(\":{}:{}:{}\", sample.distance.to_bits(), sample.layer, probes.len());"
                         "for (layer, point) in probes { print!(\":{}\", layer); for value in point { print!(\":{}\", value.to_bits()); }}"
                         "println!(); }")
            parts.append("{ let point = " + vector(point, "[", "]") + ";"
                         "let sample = path.flight_sample(point);"
                         f"print!(\"{{}}\", {json.dumps(name + '/flight/' + str(index))});"
                         "for value in sample.position { print!(\":{}\",value.to_bits()); }"
                         "println!(\":{}\",sample.distance.to_bits()); }")
        parts.append(f"print!(\"{{}}\", {json.dumps(name + '/retained_route')});"
                     "for position in path.positions() { for value in position {"
                     "print!(\":{}\",value.to_bits()); }} println!();")
        parts.append("}")
    return "\n".join(parts) + "\n}\n"


def main():
    if run(["git", "-C", str(UPSTREAM), "rev-parse", "HEAD"]) != COMMIT:
        raise RuntimeError("Upstream commit changed; review reference extraction first")
    for path in [SOURCE, PATH_HEADER, FIND_HEADER, COORD2, COORD3]:
        if run(["git", "-C", str(UPSTREAM), "status", "--porcelain", "--", path]):
            raise RuntimeError(f"Reference source is modified: {path}")
    cache = reference_support.CACHE / "path-reference"
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="run-", dir=cache) as directory:
        work = Path(directory)
        env = os.environ.copy()
        env.update(TMPDIR=str(work), XDG_CACHE_HOME=str(work / "xdg"),
                   CLANG_MODULE_CACHE_PATH=str(work / "modules"))
        cpp, rust = work / "reference.cpp", work / "port.rs"
        cpp.write_text(cpp_source())
        rust.write_text(rust_source())
        clang = reference_support.compiler("CXX", "clang++")

        rustc = reference_support.compiler("RUSTC", "rustc")
        native, port = work / "reference", work / "port"
        flags = ["-DRTS_GENERALS=0"]
        flags.extend(f"-DRETAIL_COMPATIBLE_{key}={value}" for key, value in PROFILE.items())
        run([clang, "-std=c++20", "-Wall", "-Wextra", "-Werror", "-ffp-contract=off",
             *flags, str(cpp), "-o", str(native)], env=env)
        run([rustc, "--edition=2024", "-Dwarnings", str(rust), "-o", str(port)], env=env)
        original, translated = run([str(native)], env=env), run([str(port)], env=env)
        count = sum(len(queries) for _, _, queries in CASES)
        snapshots = 2 * len(CASES)
        if len(original.splitlines()) != 2 * count + snapshots or len(translated.splitlines()) != 2 * count + snapshots:
            raise RuntimeError("The reference and port did not emit every sample and route snapshot")
        if original != translated:
            differences = [f"C++: {left}\nRust: {right}"
                           for left, right in zip(original.splitlines(), translated.splitlines())
                           if left != right]
            raise RuntimeError("Sampler mismatch:\n" + "\n".join(differences[:8]))
        print(f"PASS: {len(CASES)} ground-path fixtures, {count} samples match original C++ bit-for-bit.")
        print(f"PASS: {count} original flight-distance samples used for ground waypoint arrival match bit-for-bit.")
        print(f"PASS: {snapshots} ordered route snapshots match before and after sampling, including empty/coincident paths.")
        print("Compared goal XYZ, remaining distance and ordered line-passability probe layers and coordinates.")
        print("Scope: verbatim sampler/segment arithmetic, host f32 without contraction;")
        print("fixed profile macros asserted (these extracted functions have no compatibility guards);")
        print("ground-layer/access/storage wrappers only. Pathfinder/layers/physics and full-engine parity are not covered.")


if __name__ == "__main__":
    main()
