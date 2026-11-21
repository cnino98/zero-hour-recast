"""Run the registered acceptance inventory serially without replacing delivered builds."""
import sys

sys.dont_write_bytecode = True

import argparse
from datetime import datetime, timezone
try:
    import fcntl
except ModuleNotFoundError:
    fcntl = None
import hashlib
import json
import os
import plistlib
from pathlib import Path
import re
import shutil
import subprocess
import threading
import time
from xml.parsers.expat import ExpatError

from run_logged import run_logged
from retention import Payloads


PROJECT = Path(__file__).resolve().parents[2]
REPOSITORY = PROJECT
HEADERS = {
    "Installed gameplay checks:": "installed",
    "Performance checks:": "performance",
    "Native controls, art and audio checks (require a visible game window):": "native",
}
AUDIO_COMPLETE = "Audio verification PASSED: original click/voice/weapon events and all three authored initial music tracks reached advancing output sinks."
SCALING_TEST = "skirmish::tests::performance::simulation_scaling"
COUNTS = {"installed": 31, "native": 143, "benchmarks": 3, "profiles": 3}
REQUIRED_IMAGES = {
    "native-smoke-internet-center": ("p03-internet-satellite-one-mines.png", "internet-center.png"),
    "native-p03-recon-capture": ("p03-recon-cia.png", "p03-recon-satellite.png", "p03-recon-drone-grid.png", "p03-recon-drone.png", "p03-recon-cleared.png"),
    'native-p03-usa-capture': ('p03-particle-ready.png', 'p03-particle-beam.png', 'p03-particle-scorch.png'),
    'native-p03-china-capture': ('p03-nuke-ready.png', 'p03-nuke-flight.png', 'p03-nuke-blast.png'),
    'native-p03-gla-capture': ('p03-scud-ready.png', 'p03-scud-flight.png', 'p03-scud-toxin.png'),
    'native-p03-a10-capture': ('p03-a10-ready.png', 'p03-a10-approach.png', 'p03-a10-payload.png', 'p03-a10-effect.png', 'p03-a10-cleared.png'),
    'native-p03-paradrop-capture': ('p03-paradrop-ready.png', 'p03-paradrop-approach.png', 'p03-paradrop-payload.png', 'p03-paradrop-effect.png', 'p03-paradrop-cleared.png'),
    'native-p03-daisy-capture': ('p03-daisy-ready.png', 'p03-daisy-approach.png', 'p03-daisy-payload.png', 'p03-daisy-effect.png', 'p03-daisy-cleared.png'),
    'native-p03-moab-capture': ('p03-moab-ready.png', 'p03-moab-approach.png', 'p03-moab-payload.png', 'p03-moab-effect.png', 'p03-moab-cleared.png'),
    'native-p03-leaflet-capture': ('p03-leaflet-ready.png', 'p03-leaflet-approach.png', 'p03-leaflet-payload.png', 'p03-leaflet-effect.png', 'p03-leaflet-cleared.png'),
    'native-p03-spectre-capture': ('p03-spectre-ready.png', 'p03-spectre-approach.png', 'p03-spectre-payload.png', 'p03-spectre-effect.png', 'p03-spectre-cleared.png'),
    'native-p03-china-artillery-capture': ('p03-china-artillery.png',),
    'native-p03-china-cluster-capture': ('p03-china-cluster.png',),
    'native-p03-china-carpet-capture': ('p03-china-carpet.png',),
    'native-p03-china-emp-capture': ('p03-china-emp.png',),
    'native-p03-china-frenzy-capture': ('p03-china-frenzy.png',),
    'native-p03-china-cannon-capture': ('p03-china-cannon-nuclear.png', 'p03-china-cannon-neutron.png'),
    'native-p03-gla-ambush-capture': ('p03-gla-ambush.png',),
    'native-p03-gla-gps-capture': ('p03-gla-gps.png',),
    'native-p03-gla-sneak-capture': ('p03-gla-sneak.png',),
    'native-p03-gla-anthrax-capture': ('p03-gla-anthrax.png',),
    'native-p03-gla-radar-capture': ('p03-gla-radar.png',),
    'native-p03-gla-booby-capture': ('p03-gla-booby.png',),

    "native-parachute-terrain-capture": ("parachute-bridge-canopy.png", "parachute-bridge-landed.png", "parachute-water-canopy.png", "parachute-water-destruction.png"),
    "native-neutral-damage-capture": ("neutral-tech-damaged.png", "neutral-tech-death.png"),
    "native-hijacker-airborne-capture": ("hijacker-airborne-canopy.png", "hijacker-airborne-landed.png"),
    "native-pilot-ejection-capture": ("ejected-pilot-canopy.png", "ejected-pilot-landed.png"),
    "native-comanche-countermeasures-capture": ("comanche-countermeasures.png",),
    "native-mig-crash-capture": ("mig-crash-falling.png", "mig-crash-hulk.png"),
    "native-burton-capture": ("burton-knife.png", "burton-c4.png", "burton-c4-detonation.png"),
    "native-bomb-truck-capture": ("bomb-truck-payload.png", "bomb-truck-disguise.png", "bomb-truck-tank-treads.png", "bomb-truck-halftrack-treads.png", "bomb-truck-detonation.png"),
    "native-combat-bike-capture": ("combat-bike-rebel.png", "combat-bike-tracks.png", "combat-bike-terrorist.png", "combat-bike-hulk.png"),
    "native-smoke-shell-flow": ("shell-flow-menu.png", "shell-flow-options.png", "shell-flow-setup.png", "shell-flow-error.png", "shell-flow-selected.png", "shell-flow-loading.png", "shell-flow-paid-build.png", "shell-flow.png"),
    "native-lightpulse-capture": ("lightpulse-before.png", "lightpulse-active.png", "lightpulse.png"),
    "native-bridge-destruction-capture": ("bridge-intact-radar.png", "bridge-hydrant-water.png", "bridge-hydrant-tail.png", "bridge-hydrant-retired.png", "bridge-rubble-radar-retained.png", "bridge-destroyed.png"),
    "native-reinforcement-pad-usa-capture": ("reinforcement-pad-plane.png", "reinforcement-pad-descent.png", "reinforcement-pad.png"),
    "native-reinforcement-pad-china-capture": ("reinforcement-pad-plane.png", "reinforcement-pad-descent.png", "reinforcement-pad.png"),
    "native-reinforcement-pad-gla-capture": ("reinforcement-pad-plane.png", "reinforcement-pad-descent.png", "reinforcement-pad.png"),
    "native-supply-drop-zone-capture": ("supply-drop-zone-countdown.png", "supply-drop-zone.png"),
    "native-world-rain-capture": ("world-rain-initial.png", "world-rain-lightning.png", "world-rain-restarted.png", "world-rain.png", "world-tree-before.png", "world-tree-after.png", "world-tree-initial-fogged.png", "world-tree-fogged.png", "world-tree-revealed.png"),
    "native-world-bridge-capture": ("world-bridge-crossing.png", "world-bridge.png"),
    "native-structure-collapse-capture": ("structure-collapse-falling.png", "structure-collapse.png"),
    "native-structure-topple-capture": ("structure-topple-falling.png", "structure-topple.png"),
    "native-ecm-capture": ("ecm-disabled-vehicle.png", "ecm-jammed-missile.png", "ecm-pulse-first.png", "ecm.png"),
    "native-microwave-capture": ("microwave-disabled-building.png", "microwave-garrison-cleared.png",
                                 "microwave-emitter-first.png", "microwave-emitter.png"),
    "native-repair-bay-capture": ("repair-bay.png",),
    "native-repair-pad-capture": ("repair-pad.png",),
    "native-angry-mob-capture": ("angry-mob-paid-original-bodies.png", "angry-mob-normal-weapons.png", "angry-mob-normal-rock.png", "angry-mob-normal-molotov.png", "angry-mob-upgraded-molotov.png", "angry-mob.png"),
    "native-battle-bus-capture": ("battle-bus-loaded.png", "battle-bus-thrown.png", "battle-bus-held.png", "battle-bus.png"),
    "native-stealth-fighter-capture": ("stealth-fighter-parked.png", "stealth-fighter-airborne.png", "stealth-fighter.png"),
    "native-aurora-capture": ("aurora-parked.png", "aurora-airborne.png", "aurora.png"),
    "native-neutral-expanded-capture": ("neutral-expanded.png",),
    "native-capture-fake-buildings": ("fake-barracks-before-conversion.png", "fake-buildings.png"),
    "native-custom-map-capture": ("map-selection.png", "custom-map.png"),
    "native-options-capture": ("options-before.png", "options.png"),
    "native-helix-capture": ("helix-attachments.png", "helix-bomb.png", "helix-firestorm.png", "helix-spiral.png", "helix.png"),
    "native-hijacker-capture": ("hijacker-before-command.png", "hijacker.png"),
    "native-china-aircraft-capture": ("china-airfield.png", "china-mig.png", "china-aircraft.png"),
    "native-map-selection-two-capture": ("map-selection.png", "map-selection-two.png"),
    "native-map-selection-four-capture": ("map-selection.png", "map-selection-four.png"),
    "native-map-selection-six-capture": ("map-selection.png", "map-selection-six.png"),
    "native-map-selection-eight-capture": ("map-selection.png", "map-selection-eight.png"),

    "native-sabotage-capture": ("sabotage-cliff.png", "sabotage-before-command.png", "sabotage.png"),
    "native-smoke-barracks-healing": ("barracks-healing-entry.png", "barracks-healing-contained.png", "barracks-healing.png"),
    "native-smoke-chinook": ("chinook-landing.png", "chinook.png"),
    "native-smoke-scout": ("scout-body.png", "scout.png"),
    "native-smoke-disarm": ("disarm-dig.png", "disarm-cleared.png", "disarm.png"),
    "native-smoke-aircraft-crash": (
        "aircraft-crash-falling.png",
        "aircraft-crash-falling-view-1.png",
        "aircraft-crash-falling-view-2.png",
        "aircraft-crash-falling-view-3.png",
        "aircraft-crash.png",
        "aircraft-crash-hulk-view-0.png",
        "aircraft-crash-hulk-view-1.png",
        "aircraft-crash-hulk-view-2.png",
        "aircraft-crash-hulk.png",
    ),
    "native-smoke-outpost": ("outpost-loaded.png", "outpost-route.png", "outpost-combat.png",
                             "outpost-dying.png", "outpost.png"),
    "native-smoke-sentry": ("sentry-concealed.png", "sentry-deployed.png", "sentry.png"),
}
RUNTIME_ROOT = Path(os.environ.get("GENERALS_GAME_ROOT", PROJECT.parent / "private")).resolve()

GROUPS = ("validation", "installed", "benchmarks", "scaling", "native", "profiles")


def external_runtime(value):
    root = Path(value).resolve() if value is not None else RUNTIME_ROOT
    for path in (root, root / "output", root / ".cache"):
        resolved = path.resolve()
        if resolved == REPOSITORY or REPOSITORY in resolved.parents:
            raise ValueError("Acceptance runtime and outputs must be outside the repository")
    return root


def prepare_environment(project):
    for name, directory in {"TMPDIR": "tmp", "XDG_CACHE_HOME": "xdg",
                            "XDG_CONFIG_HOME": "config", "XDG_DATA_HOME": "data"}.items():
        path = project / ".cache" / directory
        if path.resolve() == REPOSITORY or REPOSITORY in path.resolve().parents:
            raise ValueError("Acceptance caches must be outside the repository")
        path.mkdir(parents=True, exist_ok=True)
        os.environ[name] = str(path)
    os.environ["PYTHONDONTWRITEBYTECODE"] = "1"
    os.environ["GENERALS_GAME_ROOT"] = str(project)
    target = Path(os.environ.get("CARGO_TARGET_DIR", project / ".cache/target")).resolve()
    if target == REPOSITORY or REPOSITORY in target.parents:
        raise ValueError("Cargo build output must be outside the repository")
    os.environ["CARGO_TARGET_DIR"] = str(target)
    os.environ.setdefault("CARGO_HOME", str(project / ".cache/cargo"))
    os.environ.setdefault("RUSTFLAGS", "-D warnings")


def digest(path):
    with path.open("rb") as source:
        value = hashlib.sha256()
        while block := source.read(1024 * 1024):
            value.update(block)
        return value.hexdigest()


def bundle_identity(binary):
    """Identify a real macOS bundle without importing resources or launch helpers."""
    contents = binary.parent.parent
    bundle = contents.parent
    if binary.parent.name != "MacOS" or contents.name != "Contents" or bundle.suffix != ".app":
        return None
    info = contents / "Info.plist"
    if info.is_symlink() or not info.is_file() or info.stat().st_size > 128 * 1024:
        raise ValueError("Candidate bundle requires a regular Info.plist of at most 128 KiB")
    payload = info.read_bytes()
    try:
        metadata = plistlib.loads(payload)
    except (plistlib.InvalidFileException, ExpatError, ValueError, TypeError, OverflowError) as error:
        raise ValueError("Candidate bundle Info.plist is invalid") from error
    if (not isinstance(metadata, dict)
            or metadata.get("CFBundleExecutable") != binary.name
            or metadata.get("CFBundlePackageType") != "APPL"
            or not isinstance(metadata.get("CFBundleIdentifier"), str)
            or not metadata["CFBundleIdentifier"].strip()
            or metadata.get("LSUIElement", False) is not False
            or metadata.get("LSBackgroundOnly", False) is not False):
        raise ValueError("Candidate bundle must identify this executable as a foreground application")
    return {"root": str(bundle), "identifier": metadata["CFBundleIdentifier"],
            "info_plist_sha256": hashlib.sha256(payload).hexdigest()}


def bundle_unchanged(binary, expected):
    try:
        return bundle_identity(binary) == expected
    except (OSError, ValueError):
        return False


def preserve_runtime(executable, destination, identity, payloads=None):
    bundle = bundle_identity(executable)
    if bundle is None:
        snapshot = destination / "generals-client"
    else:
        snapshot = destination / Path(bundle["root"]).name / "Contents/MacOS" / executable.name
        snapshot.parent.mkdir(parents=True, exist_ok=False)
        shutil.copy2(executable.parent.parent / "Info.plist", snapshot.parent.parent / "Info.plist")
    if payloads is not None:
        payloads.add(snapshot, identity)
    shutil.copy2(executable, snapshot)
    frozen_bundle = bundle_identity(snapshot)
    if (digest(snapshot) != identity or digest(executable) != identity
            or not bundle_unchanged(executable, bundle)
            or (bundle is not None and frozen_bundle["info_plist_sha256"] != bundle["info_plist_sha256"])):
        raise ValueError("Runtime changed while preserving the acceptance snapshot")
    return snapshot, frozen_bundle


def source_identity(project):
    paths = {project / "Cargo.toml", project / "Cargo.lock"}
    paths.update(path for path in (project / "crates").rglob("*")
                 if path.is_file() and path.suffix in (".rs", ".wgsl", ".toml"))
    paths.update(path for path in (project / "tests").rglob("*")
                 if path.is_file() and path.suffix != ".pyc"
                 and "__pycache__" not in path.relative_to(project / "tests").parts)
    configuration = project / ".cargo/config.toml"
    if configuration.is_file():
        paths.add(configuration)
    files = {str(path.relative_to(project)): digest(path) for path in sorted(paths)
             if path.name != ".DS_Store" and not path.name.startswith("._")}
    combined = hashlib.sha256()
    for name, identity in files.items():
        combined.update(name.encode() + b"\0" + identity.encode() + b"\n")
    return {"sha256": combined.hexdigest(), "files": files}


def overlay_identity(project, path):
    """Freeze explicitly selected runtime data independently from source code."""
    if path is None:
        return None
    root = Path(path).resolve(strict=True)
    allowed = (project / "output/content-overlays").resolve(strict=True)
    root.relative_to(allowed)
    if root == allowed or not (root / "manifest.tsv").is_file():
        raise ValueError("Select a content overlay containing manifest.tsv")
    files = {}
    for entry in sorted(root.rglob("*")):
        if entry.is_symlink():
            raise ValueError("Content overlay may not contain symlinks")
        if entry.is_file():
            files[str(entry.relative_to(root))] = digest(entry)
    return {"root": str(root), "manifest_sha256": files["manifest.tsv"], "files": files}


def overlay_unchanged(project, path, expected):
    try:
        return overlay_identity(project, path) == expected
    except (OSError, ValueError):
        return False


def inventory(help_text, source):
    """Reject drift instead of silently treating unrecognized help as an empty suite."""
    sections = {name: [] for name in HEADERS.values()}
    section = None
    for line in help_text.splitlines():
        if line in HEADERS:
            section = HEADERS[line]
        elif line.startswith("  --"):
            if section is None or not re.fullmatch(r"  --[a-z0-9-]+", line):
                raise ValueError(f"Unrecognized acceptance help entry: {line!r}")
            sections[section].append(line.strip())
    found = {
        "installed": sections["installed"],
        "native": sections["native"],
        "benchmarks": [flag for flag in sections["performance"] if flag.startswith("--benchmark-")],
        "profiles": [flag for flag in sections["performance"] if flag.startswith("--profile-")],
    }
    for anchor in ("fn run()", "fn validate_arguments", "const SCREEN_CAPTURES:", "fn screen_capture"):
        if source.count(anchor) != 1:
            raise ValueError(f"Acceptance source registry changed: {anchor}")
    dispatch = source.split("fn run()", 1)[1].split("fn validate_arguments", 1)[0]
    captures = source.split("const SCREEN_CAPTURES:", 1)[1].split("fn screen_capture", 1)[0]
    expected = {
        "installed": set(re.findall(r'"(--verify-[a-z0-9-]+)"', dispatch)),
        "native": set(re.findall(r'"(--[a-z0-9-]+)"', captures)),
        "benchmarks": set(re.findall(r'"(--benchmark-[a-z0-9-]+)"', dispatch)),
        "profiles": set(re.findall(r'"(--profile-[a-z0-9-]+)"', dispatch)),
    }
    for group, flags in found.items():
        if len(flags) != COUNTS[group] or len(set(flags)) != len(flags) or set(flags) != expected[group]:
            raise ValueError(f"Acceptance inventory drift in {group}: help={flags}, source={sorted(expected[group])}")
    if set(sections["performance"]) != set(found["benchmarks"] + found["profiles"]):
        raise ValueError("Unclassified performance help entry")
    if "--verify-audio" not in found["native"] or "--verify-audio" in found["installed"]:
        raise ValueError("Audio acceptance must remain a native case")
    cases = []
    for group in GROUPS:
        if group == "validation":
            entries = [("validate", [sys.executable, "-B", str(PROJECT / "tests/run.py")])]
        elif group == "scaling":
            entries = [("simulation-scaling", ["cargo", "test", "--manifest-path", str(PROJECT / "Cargo.toml"),
                        "-p", "generals-sim", "--lib", "--all-features", "--locked", "--offline", SCALING_TEST,
                        "--", "--exact", "--ignored", "--nocapture", "--test-threads=1"])]
        else:
            entries = [(flag.removeprefix("--"), [flag]) for flag in found[group]]
        for name, arguments in entries:
            # Startup uses the normal match capture. The other profiles request
            # their captures after recording the fixed timing window.
            if arguments == ["--profile-performance"]:
                arguments.append("--smoke")
            cases.append({"id": f"{group}-{name}", "group": group, "arguments": arguments,
                          "runtime": group not in ("validation", "scaling"),
                          "requires_image": (group == "native" and arguments != ["--verify-audio"])
                                            or arguments == ["--profile-performance", "--smoke"]
                                            or arguments in (["--profile-defenses"], ["--profile-skirmish"])})
    return cases


def save_json(path, value):
    temporary = path.with_suffix(".next")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


def images(project):
    return {path.name: (path.stat().st_mtime_ns, path.stat().st_size, digest(path))
            for path in (project / "output").glob("*.png") if path.is_file()}


def complete_profile(log):
    summaries = [line for line in log.splitlines() if line.startswith("live-frames ")]
    return len(summaries) == 1 and re.fullmatch(
        r"live-frames count=300 p50_ms=\d+\.\d{3} p95_ms=\d+\.\d{3} max_ms=\d+\.\d{3} "
        r"over_60hz=\d+ over_30hz=\d+ over_100ms=\d+", summaries[0]) is not None


def progress(stop, case, started):
    while not stop.wait(30):
        print(f"acceptance-progress case={case} elapsed_seconds={time.monotonic() - started:.1f}",
              file=sys.stderr, flush=True)


def run_case(project, binary, identity, case, destination, bundle=None, payloads=None):
    prepare_environment(project)
    destination.mkdir()
    if digest(binary) != identity or not bundle_unchanged(binary, bundle):
        raise ValueError("Preserved runtime changed during acceptance")
    before = images(project)
    arguments = case["arguments"]
    command = [str(binary), *arguments] if case["runtime"] else arguments
    logged_executable = binary if case["runtime"] else Path(command[0])
    started = time.monotonic()
    stop = threading.Event()
    reporter = threading.Thread(target=progress, args=(stop, case["id"], started), daemon=True)
    print(f"acceptance-start case={case['id']} runtime_sha256={identity}", flush=True)
    reporter.start()
    errors = []
    session = destination / "session.log"
    try:
        options = {"session_path": session}
        if case.get("timeout_seconds") is not None:
            options["timeout"] = case["timeout_seconds"]
        status = run_logged(project, logged_executable, command, **options)
    except OSError as error:
        status = 127
        errors.append(f"Session logging failed: {error}")
    finally:
        stop.set()
        reporter.join()
    if session.is_file():
        log = session.read_text(errors="replace")
        if re.search(r"(?m)^.*\bERROR (?:bevy_render|bevy_pbr|wgpu)(?:::|:)", log):
            errors.append("Rendering backend reported an error; inspect the preserved session log")
        if case["id"] == "native-verify-audio" and AUDIO_COMPLETE not in log.splitlines():
            errors.append("Audio case did not complete its original event and music sink checks")
        if case["id"] == "native-smoke-shell-flow":
            if not re.search(r"(?m)^SHELL_FLOW_RENDERED actual_mouse=true options=true metadata_without_match=true rejected_map_dialog=true rejected_map_retry=true original_loading_screen=true resolved_slots=true original_models=[1-9]\d*(?:\s|$)", log):
                errors.append("Shell flow did not verify its menu, failure, retry and original-model transition")
            if "SHELL_FLOW_PAID_BUILD actual_mouse=true faction=USA template=AmericaPowerPlant " not in log:
                errors.append("Fresh skirmish did not demonstrate a paid build through the control bar")
            if not re.search(r"(?m)^SHELL_FLOW_BUILD_FINISHED template=AmericaPowerPlant object=\d+ tick=[1-9]\d*$", log):
                errors.append("Paid Power Plant never reported completion")
            if "SHELL_FLOW_LOADING original_layout=MultiplayerLoadScreen.wnd" not in log:
                errors.append("Shell flow did not verify the original skirmish loading layout")
        if case["group"] == "profiles":
            if not complete_profile(log):
                errors.append("Profile did not report exactly one complete 300-frame timing summary")
            if f"Verified screenshot saved: {project / 'output/playtest.png'}" not in log.splitlines():
                errors.append("Profile did not save its verified playtest screenshot")
        if case["group"] == "scaling" and not re.search(r"(?m)^test result: ok\. 1 passed; 0 failed; 0 ignored;", log):
            errors.append("Ignored simulation scaling check did not run and pass its one test")
    else:
        errors.append("Session log was not created")
    captured = []
    for name, fingerprint in images(project).items():
        if before.get(name) == fingerprint:
            continue
        image = destination / name
        if payloads is not None:
            payloads.add(image, fingerprint[2])
        shutil.copy2(project / "output" / name, image)
        captured.append({"file": name, "sha256": digest(image)})
    if digest(binary) != identity or not bundle_unchanged(binary, bundle):
        errors.append("Preserved runtime changed during acceptance")
    if status == 0 and case["requires_image"] and not captured:
        errors.append("Native case exited successfully without a fresh image")
    if status == 0:
        required = ("playtest.png",) if case["group"] == "profiles" else REQUIRED_IMAGES.get(case["id"], ())
        missing = set(required) - {image["file"] for image in captured}
        if missing:
            errors.append(f"Native case is missing required fresh images: {sorted(missing)}")
    result = {**case, "exit_status": status, "elapsed_seconds": round(time.monotonic() - started, 3),
              "runtime_sha256": identity, "log": str(destination / "session.log"), "images": captured,
              "errors": errors, "process_gates": "pass" if status == 0 and not errors else "fail",
              "visual_review": "pending" if captured else "not_recorded",
              "physical_audio_review": "not_recorded"}
    save_json(destination / "result.json", result)
    print(f"acceptance-finish case={case['id']} process_gates={result['process_gates']} "
          f"seconds={result['elapsed_seconds']} log={result['log']}", flush=True)
    return result


def run(arguments):
    project = external_runtime(getattr(arguments, "runtime_root", None))
    prepare_environment(project)
    executable = Path(arguments.executable).resolve(strict=True)
    frozen_source = source_identity(PROJECT)
    help_result = subprocess.run([str(executable), "--help"], cwd=project,
                                 capture_output=True, text=True, check=True, timeout=30)
    source = (PROJECT / "crates/generals-client/src/main.rs").read_text()
    cases = inventory(help_result.stdout, source)
    groups = arguments.group or (["all"] if arguments.list else [])
    selected = [case for case in cases if "all" in groups or case["group"] in groups]
    if arguments.case:
        available = {case["id"]: case for case in selected}
        unknown = set(arguments.case) - available.keys()
        if unknown:
            raise ValueError(f"Unknown or unselected cases: {sorted(unknown)}")
        selected = [available[case_id] for case_id in dict.fromkeys(arguments.case)]
    identity = digest(executable)
    if arguments.list:
        print(json.dumps({"runtime_sha256": identity, "source_sha256": frozen_source["sha256"],
                          "counts": counts(selected), "cases": selected}, indent=2))
        return 0
    if not selected:
        raise ValueError("Select at least one acceptance group")
    timeout = getattr(arguments, "case_timeout", None)
    if timeout is not None:
        if timeout <= 0:
            raise ValueError("Case timeout must be positive")
        for case in selected:
            case["timeout_seconds"] = timeout
    frozen_overlay = overlay_identity(project, getattr(arguments, "content_overlay", None))
    if frozen_overlay is not None:
        if any(case["group"] not in ("native", "profiles") or case["arguments"] == ["--verify-audio"] for case in selected):
            raise ValueError("Content overlays support native window and profile cases only")
        for case in selected:
            case["arguments"].append(f"--content-overlay={frozen_overlay['root']}")
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    destination = (project / "output/acceptance" / f"{stamp}-{identity[:12]}"
                   if arguments.output is None else Path(arguments.output).resolve())
    destination.relative_to(project / "output")
    destination.mkdir(parents=True, exist_ok=False)
    payloads = Payloads(destination)
    try:
        snapshot, frozen_bundle = preserve_runtime(executable, destination, identity, payloads)
        (destination / "runtime-help.txt").write_text(help_result.stdout)
        manifest = {"runtime_sha256": identity, "runtime": str(snapshot), "original_runtime": str(executable),
                    "main_source_sha256": digest(PROJECT / "crates/generals-client/src/main.rs"),
                    "source": frozen_source, "content_overlay": frozen_overlay, "runtime_bundle": frozen_bundle,
                    "counts": counts(selected),
                    "cases": selected, "results": [], "not_run": [case["id"] for case in selected],
                    "visual_and_physical_audio_review": "separate; never implied by process success"}
        save_json(destination / "manifest.json", manifest)
        for case in selected:
            if source_identity(PROJECT) != frozen_source:
                manifest["source_error"] = "Source changed before the next acceptance case; remaining cases were not run"
                save_json(destination / "manifest.json", manifest)
                break
            if not overlay_unchanged(project, getattr(arguments, "content_overlay", None), frozen_overlay):
                manifest["data_error"] = "Content overlay changed before acceptance case"
                save_json(destination / "manifest.json", manifest)
                break
            result = run_case(project, snapshot, identity, case, destination / case["id"], frozen_bundle, payloads)
            source_changed = source_identity(PROJECT) != frozen_source
            overlay_changed = not overlay_unchanged(project, getattr(arguments, "content_overlay", None), frozen_overlay)
            if overlay_changed:
                result["errors"].append("Content overlay changed during acceptance")
                result["process_gates"] = "fail"
                save_json(destination / case["id"] / "result.json", result)
            if source_changed:
                result["errors"].append("Source changed during acceptance; frozen-source validation is invalid")
                result["process_gates"] = "fail"
                save_json(destination / case["id"] / "result.json", result)
                print(f"acceptance-source-changed case={case['id']}; remaining cases will not run", file=sys.stderr)
            manifest["results"].append(result)
            manifest["not_run"].remove(case["id"])
            save_json(destination / "manifest.json", manifest)
            if source_changed or overlay_changed or result["exit_status"] >= 128 or arguments.stop_on_failure and result["process_gates"] != "pass":
                break
        return int(bool(manifest["not_run"]) or any(result["process_gates"] != "pass" for result in manifest["results"]))
    finally:
        payloads.close()


def counts(cases):
    return {**{group: sum(case["group"] == group for case in cases) for group in GROUPS},
            "total": len(cases), "cases_requiring_images": sum(case["requires_image"] for case in cases)}


def main():
    if fcntl is None:
        raise ValueError("Private acceptance requires POSIX file locking")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", required=True, help="Candidate executable; an unchanged snapshot is used for acceptance")
    parser.add_argument("--runtime-root", help="External private root containing GameData, UserData and output")
    parser.add_argument("--group", action="append", choices=(*GROUPS, "all"), default=[])
    parser.add_argument("--case", action="append", default=[], help="Exact manifest case ID; repeat for selected retries")
    parser.add_argument("--list", action="store_true", help="Print checked inventory without launching acceptance cases")
    parser.add_argument("--output", help="New directory under the external runtime output; existing evidence is never replaced")
    parser.add_argument("--stop-on-failure", action="store_true")
    parser.add_argument("--case-timeout", type=float, default=900,
                        help="Seconds per case before termination; default 900, increase for long workloads")
    parser.add_argument("--content-overlay", help="Explicit overlay under output/content-overlays, for native windows/profiles")
    arguments = parser.parse_args()
    lock_path = external_runtime(arguments.runtime_root) / ".cache/acceptance.lock"
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    with lock_path.open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            return run(arguments)
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            print(f"Acceptance stopped: {error}", file=sys.stderr)
            return 1


if __name__ == "__main__":
    sys.exit(main())
