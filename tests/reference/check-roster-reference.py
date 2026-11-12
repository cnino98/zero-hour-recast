#!/usr/bin/env python3
"""Compare the pinned original faction-selection loop with the Rust port.

Only GUI/data-access wrappers are stubbed. This does not validate INI parsing,
template overlays, other engine systems, or full-game behavioral equivalence.
All generated source, binaries, caches and temporary outputs stay in the external test output directory.
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
GUI = "GeneralsMD/Code/GameEngine/Source/GameNetwork/GUIUtil.cpp"
PERSONA = "Core/GameEngine/Source/GameClient/GUI/ChallengeGenerals.cpp"
STRING = "Core/GameEngine/Include/Common/AsciiString.h"

# (name, ordered players[name, side, starting building, old faction],
# ordered personas[template, enabled], old-only, expected accepted indices)
CASES = [
    ("locked_does_not_reserve_side", [
        ("Boss", "China", "CommandCenter", False),
        ("China", "China", "CommandCenter", True),
        ("Observer", "Observer", "", False),
        ("DuplicateChina", "China", "CommandCenter", False),
    ], [("BOSS", False)], False, [1]),
    ("old_only_case_sensitive_sides", [
        ("General", "China", "CommandCenter", False),
        ("Base", "China", "CommandCenter", True),
        ("DistinctCase", "china", "CommandCenter", True),
    ], [], True, [1, 2]),
    ("first_persona_match_wins", [
        ("General", "China", "CommandCenter", True),
    ], [("GENERAL", False), ("General", True)], False, []),
    ("unmatched_persona_is_unlocked", [
        ("USA", "America", "AmericaCommandCenter", True),
        ("Civilian", "Civilian", "", True),
    ], [("Other", False)], False, [0]),
    ("empty_side_still_deduplicates", [
        ("First", "", "Building", False),
        ("Second", "", "Building", False),
    ], [], False, [0]),
    ("enabled_general", [
        ("General", "ChinaTankGeneral", "Tank_ChinaCommandCenter", False),
    ], [("general", True)], False, [0]),
]


def run(arguments, *, env=None):
    local_env = os.environ.copy() if env is None else dict(env)
    local_env["GIT_OPTIONAL_LOCKS"] = "0"
    result = subprocess.run(arguments, cwd=PROJECT, env=local_env, check=False,
                            capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(f"{arguments[0]} failed ({result.returncode}):\n{result.stderr}\n{result.stdout}")
    return result.stdout.strip()


def original_function(path, signature):
    """Pinned source functions end with an unindented closing brace."""
    source = (UPSTREAM / path).read_text()
    start = source.index(signature)
    end = source.index("\n}\n", start) + 3
    return source[start:end]


def string(value):
    # Fixture strings are ASCII; JSON quoted strings are valid C++/Rust literals.
    return json.dumps(value, ensure_ascii=True)


def cpp_source():
    license_header = (UPSTREAM / GUI).read_text().split("*/", 1)[0] + "*/\n"
    wrappers = r'''
#include <cstdint>
#include <cstring>
#include <strings.h>
#include <iostream>
#include <set>
#include <string>
#include <vector>
using Int = std::intptr_t; // GUI item-data pointer casts only; fixture indices fit original int.
using Bool = bool;
using UnicodeString = std::string;
constexpr Bool TRUE = true;
constexpr Bool FALSE = false;
constexpr Int PLAYERTEMPLATE_RANDOM = -1, PLAYERTEMPLATE_OBSERVER = -2;
constexpr Int NUM_GENERALS = 12;
struct AsciiString {
    std::string value;
    AsciiString() = default;
    AsciiString(const char* text) : value(text) {}
    const char* str() const { return value.c_str(); }
    Bool isEmpty() const { return value.empty(); }
    int compareNoCase(const char* other) const { return strcasecmp(str(), other); }
    void format(const char*, const char* side) { value = std::string("SIDE:") + side; }
};
struct PlayerTemplate {
    AsciiString name, side, building;
    Bool old;
    AsciiString getName() const { return name; }
    AsciiString getSide() const { return side; }
    AsciiString getStartingBuilding() const { return building; }
    Bool isOldFaction() const { return old; }
};
struct PlayerTemplateStore {
    std::vector<PlayerTemplate> players;
    Int getPlayerTemplateCount() const { return static_cast<Int>(players.size()); }
    const PlayerTemplate* getNthPlayerTemplate(Int i) const { return &players.at(i); }
};
struct GeneralPersona {
    AsciiString name;
    Bool enabled = false;
    AsciiString getPlayerTemplateName() const { return name; }
    Bool isStartingEnabled() const { return enabled; }
};
struct ChallengeGenerals {
    GeneralPersona m_position[NUM_GENERALS];
    const GeneralPersona* getGeneralByTemplateName(AsciiString name) const;
};
struct GameInfo { Bool old; Bool oldFactionsOnly() const { return old; } };
struct GameWindow {};
struct MultiplayerColorDefinition { int getColor() const { return 0; } };
struct MultiplayerSettings {
    MultiplayerColorDefinition color;
    MultiplayerColorDefinition* getColor(Int) { return &color; }
};
struct GameText {
    UnicodeString fetch(const char* text) const { return text; }
    UnicodeString fetch(const AsciiString& text) const { return text.str(); }
};
PlayerTemplateStore store;
ChallengeGenerals challenge;
MultiplayerSettings settings;
GameText text;
auto* ThePlayerTemplateStore = &store;
auto* TheChallengeGenerals = &challenge;
auto* TheMultiplayerSettings = &settings;
auto* TheGameText = &text;
std::vector<Int> accepted;
void GadgetComboBoxReset(GameWindow*) { accepted.clear(); }
Int GadgetComboBoxAddEntry(GameWindow*, const UnicodeString&, int) { return 0; }
void GadgetComboBoxSetItemData(GameWindow*, Int, void* value) {
    Int index = reinterpret_cast<Int>(value);
    if (index >= 0) accepted.push_back(index);
}
void GadgetComboBoxSetSelectedPos(GameWindow*, Int) {}
'''
    parts = [license_header, "// Extracted from pinned SuperHackers " + COMMIT + "\n",
             wrappers, original_function(STRING, "inline Bool operator<(const AsciiString&"),
             original_function(PERSONA, "const GeneralPersona* ChallengeGenerals::getGeneralByTemplateName("),
             original_function(GUI, "void PopulatePlayerTemplateComboBox("), "int main() {\n"]
    for name, players, personas, old, _ in CASES:
        rows = ["{" + ",".join([*(string(s) for s in p[:3]), str(p[3]).lower()]) + "}" for p in players]
        parts.append("store.players = {" + ",".join(rows) + "}; challenge = ChallengeGenerals{};\n")
        for index, (template, enabled) in enumerate(personas):
            parts.append(f"challenge.m_position[{index}] = {{{string(template)},{str(enabled).lower()}}};\n")
        parts.append("{ GameWindow window; GameWindow* windows[] = {&window}; GameInfo game{" + str(old).lower() + "};\n")
        parts.append("PopulatePlayerTemplateComboBox(0, windows, &game, false);\n")
        parts.append("std::cout << " + string(name + ":") + "; for (Int i : accepted) std::cout << i << ','; std::cout << '\\n'; }\n")
    return "\n".join(parts) + "}\n"


def rust_source():
    module = string(str(PROJECT / "crates/generals-assets/src/roster.rs"))
    parts = [f"#[path = {module}] mod roster;\nfn main() {{"]
    for name, players, personas, old, _ in CASES:
        rows = ["roster::PlayerTemplate { name:" + string(p[0]) + ".into(), side:" + string(p[1])
                + ".into(), starting_building:" + string(p[2]) + ".into(), old_faction:" + str(p[3]).lower() + "}" for p in players]
        people = ["roster::GeneralPersona { player_template:" + string(n) + ".into(), starts_enabled:" + str(e).lower() + "}" for n, e in personas]
        parts.append("{ let players = [" + ",".join(rows) + "]; let personas = [" + ",".join(people) + "];\n")
        parts.append("print!(\"{}\", " + string(name + ":") + ");\n")
        parts.append("for (i, state) in roster::skirmish_eligibility(&players, &personas, " + str(old).lower() + ").iter().enumerate() { if *state == roster::Eligibility::Selectable { print!(\"{i},\"); } } println!(); }")
    return "\n".join(parts) + "}\n"


def main():
    if run(["git", "-C", str(UPSTREAM), "rev-parse", "HEAD"]) != COMMIT:
        raise RuntimeError("upstream commit changed; review and update reference extraction first")
    for path in [GUI, PERSONA, STRING]:
        if run(["git", "-C", str(UPSTREAM), "status", "--porcelain", "--", path]):
            raise RuntimeError(f"reference source is modified: {path}")
    cache = reference_support.CACHE / "roster-reference"
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
        run([clang, "-std=c++17", "-Wall", "-Wextra", "-Werror", str(cpp), "-o", str(native)], env=env)
        run([rustc, "--edition=2024", "-Dwarnings", str(rust), "-o", str(port)], env=env)
        expected = "\n".join(name + ":" + "".join(f"{i}," for i in selected) for name, _, _, _, selected in CASES)
        original, translated = run([str(native)], env=env), run([str(port)], env=env)
        if original != translated or original != expected:
            raise RuntimeError(f"reference/port/expected mismatch\nC++:\n{original}\nRust:\n{translated}\nExpected:\n{expected}")
        print(f"PASS: {len(CASES)} faction-selection fixtures match extracted original C++ and current Rust.")
        print(original)
        print("Scope: original population loop, persona lookup and string ordering; GUI/data wrappers stubbed. INI resolution and full-engine parity are not covered.")


if __name__ == "__main__":
    main()
