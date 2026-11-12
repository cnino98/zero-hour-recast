"""Acceptance inventory drift and evidence retention, without launching a game."""
import argparse
import importlib.util
import hashlib
import json
import os
import plistlib
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


PROJECT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(PROJECT / "tests/acceptance"))
try:
    SPEC = importlib.util.spec_from_file_location("run_acceptance", PROJECT / "tests/acceptance/run.py")
    RUNNER = importlib.util.module_from_spec(SPEC)
    SPEC.loader.exec_module(RUNNER)
finally:
    sys.path.pop(0)
SOURCE = (PROJECT / "crates/generals-client/src/main.rs").read_text()


def help_fixture():
    """Model the actual print_help sections, independently from run() dispatch."""
    printed = SOURCE.split("fn print_help()", 1)[1]
    installed = printed.split("for flag in [", 1)[1].split("]", 1)[0]
    performance = printed.split('"\\nPerformance checks:', 1)[1].split('"', 1)[0]
    capture_registry = SOURCE.split("const SCREEN_CAPTURES:", 1)[1].split("fn screen_capture", 1)[0]
    return "\n".join([
        "Generals Rust translation", "Installed gameplay checks:",
        *(f"  --verify-{name}" for name in re.findall(r'"([a-z-]+)"', installed)),
        "Performance checks:", *performance.split("\\n")[1:],
        "Native controls, art and audio checks (require a visible game window):",
        *(f"  {flag}" for flag in re.findall(r'"(--[a-z-]+)"', capture_registry)),
    ])


class AcceptanceInventoryTests(unittest.TestCase):
    def test_runtime_rejects_repository_and_nested_output_paths(self):
        for path in (PROJECT, PROJECT / "output", PROJECT / "private"):
            with self.subTest(path=path), self.assertRaisesRegex(ValueError, "outside the repository"):
                RUNNER.external_runtime(path)
        external = PROJECT.parent / "private"
        self.assertEqual(RUNNER.external_runtime(external), external.resolve())

    def test_complete_registered_inventory_keeps_audio_native_and_scaling_explicit(self):
        cases = RUNNER.inventory(help_fixture(), SOURCE)
        self.assertEqual(RUNNER.counts(cases), {
            "validation": 1, "installed": 31, "benchmarks": 3, "scaling": 1,
            "native": 110, "profiles": 3, "total": 149, "cases_requiring_images": 112,
        })
        native = [case for case in cases if case["group"] == "native"]
        self.assertEqual(sum(case["requires_image"] for case in native), 109)
        self.assertTrue(all(len(case["arguments"]) == 1 for case in native))
        audio = next(case for case in native if case["arguments"] == ["--verify-audio"])
        self.assertFalse(audio["requires_image"])
        scaling = next(case for case in cases if case["group"] == "scaling")
        self.assertIn("--ignored", scaling["arguments"])
        self.assertIn("--exact", scaling["arguments"])
        self.assertIn("skirmish::tests::performance::simulation_scaling", scaling["arguments"])
        profile = next(case for case in cases if case["id"] == "profiles-profile-performance")
        self.assertEqual(profile["arguments"], ["--profile-performance", "--smoke"])
        defenses = next(case for case in cases if case["id"] == "profiles-profile-defenses")
        self.assertTrue(defenses["requires_image"])
        skirmish = next(case for case in cases if case["id"] == "profiles-profile-skirmish")
        self.assertTrue(skirmish["requires_image"])

    def test_missing_duplicate_renamed_and_combined_flags_fail_closed(self):
        original = help_fixture()
        for changed in [original.replace("  --smoke-pathfinder\n", ""),
                        original + "\n  --smoke-pathfinder",
                        original.replace("--smoke-pathfinder", "--smoke-new-name"),
                        original.replace("  --smoke-pathfinder", "  --smoke-pathfinder --smoke-scout")]:
            with self.subTest(help=changed[-80:]), self.assertRaises(ValueError):
                RUNNER.inventory(changed, SOURCE)

    def test_source_registry_drift_is_not_hidden_by_old_runtime_help(self):
        changed = SOURCE.replace('("--smoke-pathfinder", ScreenCapture::Pathfinder)',
                                 '("--smoke-new-name", ScreenCapture::Pathfinder)')
        self.assertNotEqual(changed, SOURCE)
        with self.assertRaisesRegex(ValueError, "inventory drift in native"):
            RUNNER.inventory(help_fixture(), changed)
        with self.assertRaisesRegex(ValueError, "source registry changed"):
            RUNNER.inventory(help_fixture(), SOURCE.replace("const SCREEN_CAPTURES:", "const CAPTURES:"))


@unittest.skipUnless(os.name == "posix", "POSIX private runtime acceptance evidence")
class AcceptanceEvidenceTests(unittest.TestCase):
    def test_explicit_case_order_is_preserved_and_duplicates_are_deduplicated(self):
        source_path = self.project / "crates/generals-client/src/main.rs"
        source_path.parent.mkdir(parents=True)
        source_path.write_text(SOURCE)
        requested = ["profiles-profile-performance", "native-smoke-menu",
                     "profiles-profile-performance", "native-smoke-shell-flow"]
        expected = ["profiles-profile-performance", "native-smoke-menu", "native-smoke-shell-flow"]
        arguments = argparse.Namespace(executable=str(self.binary), group=["native", "profiles"],
                                       case=requested, list=False, output=None,
                                       stop_on_failure=False, content_overlay=None)
        completed_help = subprocess.CompletedProcess([str(self.binary), "--help"], 0, help_fixture(), "")
        launched = []

        def passed_case(_project, _executable, _identity, case, _evidence, _bundle=None, _payloads=None):
            launched.append(case["id"])
            return {**case, "exit_status": 0, "process_gates": "pass", "errors": [], "images": []}

        with patch.object(RUNNER, "PROJECT", self.project), \
                patch.object(RUNNER.subprocess, "run", return_value=completed_help), \
                patch.object(RUNNER, "run_case", side_effect=passed_case):
            self.assertEqual(RUNNER.run(arguments), 0)
        manifest = json.loads(next((self.output / "acceptance").glob("*/manifest.json")).read_text())
        self.assertEqual([case["id"] for case in manifest["cases"]], expected)
        self.assertEqual([result["id"] for result in manifest["results"]], expected)
        self.assertEqual(launched, expected)
        self.assertEqual(manifest["not_run"], [])

    def test_overlay_change_stops_after_first_native_case_and_retains_failed_evidence(self):
        source_path = self.project / "crates/generals-client/src/main.rs"
        source_path.parent.mkdir(parents=True)
        source_path.write_text(SOURCE)
        overlay = self.output / "content-overlays" / "selected"
        overlay.mkdir(parents=True)
        (overlay / "manifest.tsv").write_text("original identity\n")
        arguments = argparse.Namespace(executable=str(self.binary), group=["native"],
                                       case=["native-smoke-menu", "native-smoke-skirmish"],
                                       list=False, output=None, stop_on_failure=False,
                                       content_overlay=str(overlay))
        completed_help = subprocess.CompletedProcess([str(self.binary), "--help"], 0, help_fixture(), "")

        def changed_overlay(project, _executable, command, session_path=None):
            self.assertEqual(command[-1], f"--content-overlay={overlay}")
            session_path.write_text("exit_status: 0\n")
            (project / "output/native.png").write_bytes(b"fresh rendered fixture")
            (overlay / "manifest.tsv").write_text("edited during native check\n")
            return 0

        with patch.object(RUNNER, "PROJECT", self.project), \
                patch.object(RUNNER.subprocess, "run", return_value=completed_help), \
                patch.object(RUNNER, "run_logged", side_effect=changed_overlay) as launch:
            self.assertEqual(RUNNER.run(arguments), 1)
        launch.assert_called_once()
        manifest = json.loads(next((self.output / "acceptance").glob("*/manifest.json")).read_text())
        self.assertEqual(manifest["results"][0]["process_gates"], "fail")
        self.assertIn("Content overlay changed during acceptance", manifest["results"][0]["errors"])
        self.assertEqual(len(manifest["not_run"]), 1)
        self.assertEqual(manifest["content_overlay"]["manifest_sha256"],
                         hashlib.sha256(b"original identity\n").hexdigest())

    def test_overlay_identity_detects_changed_or_deleted_payload_and_rejects_symlinks(self):
        overlay = self.output / "content-overlays" / "selected"
        payload = overlay / "payload/data/ini/weapon.ini"
        payload.parent.mkdir(parents=True)
        payload.write_bytes(b"selected runtime data")
        (overlay / "manifest.tsv").write_text("test manifest identity\n")
        identity = RUNNER.overlay_identity(self.project, overlay)
        self.assertEqual(identity["manifest_sha256"], RUNNER.digest(overlay / "manifest.tsv"))
        self.assertTrue(RUNNER.overlay_unchanged(self.project, overlay, identity))
        payload.write_bytes(b"changed during native run")
        self.assertFalse(RUNNER.overlay_unchanged(self.project, overlay, identity))
        payload.unlink()
        self.assertFalse(RUNNER.overlay_unchanged(self.project, overlay, identity))
        payload.symlink_to(overlay / "manifest.tsv")
        with self.assertRaises(ValueError):
            RUNNER.overlay_identity(self.project, overlay)

    def setUp(self):
        environment = patch.dict(os.environ)
        environment.start()
        self.addCleanup(environment.stop)
        temporary = Path(os.environ.get("GENERALS_TEST_OUTPUT", PROJECT.parent / "private/tests")) / "tmp"
        temporary.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="acceptance-runner-", dir=temporary)
        self.addCleanup(self.temporary.cleanup)
        self.project = Path(self.temporary.name)
        runtime = patch.object(RUNNER, "external_runtime", return_value=self.project)
        runtime.start()
        self.addCleanup(runtime.stop)
        self.output = self.project / "output"
        self.output.mkdir()
        self.binary = self.project / "candidate"
        self.binary.write_bytes(b"preserved candidate identity")
        (self.project / "Cargo.toml").write_text("[workspace]\n")
        (self.project / "Cargo.lock").write_text("version = 4\n")
        self.identity = RUNNER.digest(self.binary)
        self.case = {"id": "native-smoke-cleanup", "group": "native", "arguments": ["--smoke-cleanup"],
                     "runtime": True, "requires_image": True}

    def bundled_candidate(self):
        binary = self.project / "Playtest.app/Contents/MacOS/generals-client"
        binary.parent.mkdir(parents=True)
        binary.write_bytes(self.binary.read_bytes())
        binary.chmod(0o755)
        info = binary.parent.parent / "Info.plist"
        info.write_bytes(plistlib.dumps({"CFBundleExecutable": binary.name,
                                        "CFBundlePackageType": "APPL",
                                        "CFBundleIdentifier": "com.generalsrust.playtest",
                                        "CFBundleName": "Generals Rust Playtest"}))
        return binary, info

    def test_backend_error_rejects_successful_process_with_fresh_image(self):
        for index, (diagnostic, expected) in enumerate([
            ("ERROR bevy_render::slab_allocator: Use-after-free: attempted to copy element data for an unallocated key", "fail"),
            ("ERROR wgpu::backend: invalid render attachment", "fail"),
            ("WARN bevy_render::renderer: optional adapter feature unavailable", "pass"),
        ]):
            with self.subTest(diagnostic=diagnostic):
                def launch(project, executable, command, session_path=None):
                    session_path.write_text(f"2026-10-13T04:18:00Z {diagnostic}\nexit_status: 0\n")
                    (project / "output/cleanup.png").write_bytes(f"fresh frame {index}".encode())
                    return 0

                with patch.object(RUNNER, "run_logged", side_effect=launch):
                    result = RUNNER.run_case(self.project, self.binary, self.identity, self.case,
                                             self.output / f"backend-{index}")
                self.assertEqual(result["exit_status"], 0)
                self.assertEqual(result["process_gates"], expected)
                self.assertEqual(len(result["images"]), 1)
                if expected == "fail":
                    self.assertIn("Rendering backend reported an error", result["errors"][0])

    def test_bundle_snapshot_keeps_identity_supervision_and_fresh_image_gate(self):
        binary, info = self.bundled_candidate()
        unused = binary.parent.parent / "Resources"
        unused.mkdir()
        (unused / "unrelated-resource").write_bytes(b"do not import")
        source_path = self.project / "crates/generals-client/src/main.rs"
        source_path.parent.mkdir(parents=True)
        source_path.write_text(SOURCE)
        arguments = argparse.Namespace(executable=str(binary), group=["native"],
                                       case=[self.case["id"]], list=False,
                                       output=None, stop_on_failure=False)
        completed_help = subprocess.CompletedProcess([str(binary), "--help"], 0, help_fixture(), "")

        def launch(project, executable, command, session_path=None):
            self.assertEqual(executable.relative_to(session_path.parent.parent),
                             Path("Playtest.app/Contents/MacOS/generals-client"))
            self.assertEqual(command[0], str(executable))
            self.assertEqual(command[-2:], [str(executable), "--smoke-cleanup"])
            self.assertEqual(executable.stat().st_mode & 0o777, 0o755)
            self.assertEqual(RUNNER.digest(executable), self.identity)
            self.assertEqual((executable.parent.parent / "Info.plist").read_bytes(), info.read_bytes())
            self.assertFalse((executable.parent.parent / "Resources").exists())
            session_path.write_text("exit_status: 0\n")
            # Process success still fails the native acceptance without a fresh image.
            return 0

        with patch.object(RUNNER, "PROJECT", self.project), \
                patch.object(RUNNER.subprocess, "run", return_value=completed_help), \
                patch.object(RUNNER, "run_logged", side_effect=launch) as launched:
            self.assertEqual(RUNNER.run(arguments), 1)
        launched.assert_called_once()
        manifest_path = next((self.output / "acceptance").glob("*/manifest.json"))
        manifest = json.loads(manifest_path.read_text())
        self.assertEqual(manifest["runtime_sha256"], self.identity)
        self.assertEqual(manifest["runtime_bundle"]["identifier"], "com.generalsrust.playtest")
        self.assertEqual(manifest["runtime_bundle"]["info_plist_sha256"], RUNNER.digest(info))
        self.assertIn("without a fresh image", manifest["results"][0]["errors"][0])
        self.assertEqual(RUNNER.digest(binary), self.identity)

    def test_bundle_metadata_drift_fails_before_launch_or_after_supervised_case(self):
        binary, _info = self.bundled_candidate()
        destination = self.output / "snapshot"
        destination.mkdir()
        snapshot, bundle = RUNNER.preserve_runtime(binary, destination, self.identity)
        info = snapshot.parent.parent / "Info.plist"
        original = info.read_bytes()
        info.write_bytes(original.replace(b"com.generalsrust.playtest", b"com.generalsrust.changedx"))
        with patch.object(RUNNER, "run_logged") as launch, self.assertRaisesRegex(ValueError, "runtime changed"):
            RUNNER.run_case(self.project, snapshot, self.identity, self.case, self.output / "before", bundle)
        launch.assert_not_called()
        info.write_bytes(original)

        def changed(project, executable, command, session_path=None):
            session_path.write_text("exit_status: 0\n")
            (project / "output/cleanup.png").write_bytes(b"fresh model")
            info.write_bytes(original.replace(b"com.generalsrust.playtest", b"com.generalsrust.changedx"))
            return 0

        with patch.object(RUNNER, "run_logged", side_effect=changed):
            result = RUNNER.run_case(self.project, snapshot, self.identity, self.case, self.output / "during", bundle)
        self.assertEqual(result["process_gates"], "fail")
        self.assertIn("Preserved runtime changed during acceptance", result["errors"])
        self.assertEqual(len(result["images"]), 1)

    def test_bundle_snapshot_rejects_wrong_executable_and_symlinked_metadata(self):
        binary, info = self.bundled_candidate()
        original = info.read_bytes()
        info.write_bytes(original.replace(b"generals-client", b"../other-client"))
        destination = self.output / "invalid"
        destination.mkdir()
        with self.assertRaises(ValueError):
            RUNNER.preserve_runtime(binary, destination, self.identity)
        self.assertEqual(list(destination.iterdir()), [])
        info.write_bytes(b'<?xml version="1.0"?><plist><dict>')
        with self.assertRaisesRegex(ValueError, "Info.plist is invalid"):
            RUNNER.preserve_runtime(binary, destination, self.identity)
        self.assertEqual(list(destination.iterdir()), [])
        info.unlink()
        target = self.project / "external.plist"
        target.write_bytes(original)
        info.symlink_to(target)
        with self.assertRaises(ValueError):
            RUNNER.preserve_runtime(binary, destination, self.identity)
        self.assertEqual(list(destination.iterdir()), [])

    def fake_session(self, status, new_images=()):
        def launch(project, executable, command, session_path=None):
            self.assertEqual(project, self.project)
            self.assertTrue(executable.is_file())
            self.assertEqual(command[-1], self.case["arguments"][-1])
            session_path.write_text(f"child failure details\nexit_status: {status}\n")
            for name, data in new_images:
                (project / "output" / name).write_bytes(data)
            return status
        return launch

    def test_failed_case_keeps_unrelated_evidence_and_captures_only_fresh_images(self):
        (self.output / "cleanup.png").write_bytes(b"old verified image")
        logs = self.output / "logs"
        logs.mkdir()
        (logs / "playtest-failure.log").write_text("prior failure")
        (self.output / "playtest-run.log").write_text("latest prior failure")
        evidence = self.output / "new-case"
        with patch.object(RUNNER, "run_logged", self.fake_session(1, [("cleanup-radius.png", b"partial gate")])):
            result = RUNNER.run_case(self.project, self.binary, self.identity, self.case, evidence)
        self.assertEqual(result["process_gates"], "fail")
        self.assertEqual([image["file"] for image in result["images"]], ["cleanup-radius.png"])
        self.assertEqual((self.output / "cleanup.png").read_bytes(), b"old verified image")
        self.assertEqual((logs / "playtest-failure.log").read_text(), "prior failure")
        self.assertEqual((self.output / "playtest-run.log").read_text(), "latest prior failure")
        self.assertIn("exit_status: 1", (evidence / "session.log").read_text())
        self.assertEqual(json.loads((evidence / "result.json").read_text())["runtime_sha256"], self.identity)
        self.assertEqual(result["visual_review"], "pending")
        self.assertEqual(result["physical_audio_review"], "not_recorded")
        with self.assertRaises(FileExistsError):
            RUNNER.run_case(self.project, self.binary, self.identity, self.case, evidence)

    def test_abandoned_case_copy_is_registered_before_the_case_returns(self):
        from retention import retain_payloads
        run = self.output / "abandoned"
        run.mkdir()
        payloads = RUNNER.Payloads(run)
        evidence = run / "case"
        copy = RUNNER.shutil.copy2

        def interrupted_copy(source, destination):
            copy(source, destination)
            raise RuntimeError("interrupted after copying, before returning a result")

        try:
            with patch.object(RUNNER, "run_logged", self.fake_session(1, [("cleanup-radius.png", b"partial gate")])), \
                    patch.object(RUNNER.shutil, "copy2", side_effect=interrupted_copy):
                with self.assertRaisesRegex(RuntimeError, "interrupted"):
                    RUNNER.run_case(self.project, self.binary, self.identity, self.case,
                                    evidence, payloads=payloads)
        finally:
            # A killed process releases its lock without any completion callback.
            payloads.lock.close()
        self.assertTrue((evidence / "cleanup-radius.png").exists())
        retain_payloads(self.output, keep=0)
        self.assertFalse((evidence / "cleanup-radius.png").exists())
        self.assertIn("exit_status: 1", (evidence / "session.log").read_text())

    def test_identical_image_rewritten_this_case_is_fresh_but_old_image_is_not(self):
        image = self.output / "cleanup.png"
        image.write_bytes(b"same drawable bytes")
        os.utime(image, ns=(1, 1))
        with patch.object(RUNNER, "run_logged", self.fake_session(0, [(image.name, image.read_bytes())])):
            result = RUNNER.run_case(self.project, self.binary, self.identity, self.case, self.output / "fresh")
        self.assertEqual(result["process_gates"], "pass")
        self.assertEqual(len(result["images"]), 1)
        with patch.object(RUNNER, "run_logged", self.fake_session(0)):
            result = RUNNER.run_case(self.project, self.binary, self.identity, self.case, self.output / "stale")
        self.assertEqual(result["process_gates"], "fail")
        self.assertIn("without a fresh image", result["errors"][0])

    def test_audio_requires_the_final_sink_check_even_after_a_clean_exit(self):
        self.case.update(id="native-verify-audio", arguments=["--verify-audio"], requires_image=False)
        for label, output, expected in [
            ("early-close", "", "fail"),
            ("events-only", "Audio verification PASSED: original GUI, catalog building selection, unit voice, primary-weapon and seven scripted advisor/cheer recordings reached advancing system-output sinks; human audibility is not inferred.", "fail"),
            ("completed", RUNNER.AUDIO_COMPLETE, "pass"),
        ]:
            with self.subTest(case=label):
                def launch(project, _executable, _command, session_path=None):
                    session_path.write_text(f"{output}\nexit_status: 0\n")
                    return 0
                with patch.object(RUNNER, "run_logged", launch):
                    result = RUNNER.run_case(self.project, self.binary, self.identity, self.case, self.output / label)
                self.assertEqual(result["process_gates"], expected)
                self.assertEqual(result["physical_audio_review"], "not_recorded")

    def test_shell_flow_requires_transition_evidence_even_with_all_images(self):
        self.case.update(id="native-smoke-shell-flow", arguments=["--smoke-shell-flow"])
        images = [(name, name.encode()) for name in RUNNER.REQUIRED_IMAGES[self.case["id"]]]
        launcher = self.fake_session(0, images)
        with patch.object(RUNNER, "run_logged", launcher):
            result = RUNNER.run_case(self.project, self.binary, self.identity, self.case, self.output / "missing-flow")
        self.assertEqual(result["process_gates"], "fail")
        self.assertEqual(len(result["errors"]), 4)

        def paid(project, executable, command, session_path=None):
            status = launcher(project, executable, command, session_path)
            with session_path.open("a") as log:
                log.write("SHELL_FLOW_PAID_BUILD actual_mouse=true faction=USA template=AmericaPowerPlant builder=15 site=[123.0, 456.0] cost=800 cash_before=10000 cash_after=9200\n")
                log.write("SHELL_FLOW_LOADING original_layout=MultiplayerLoadScreen.wnd\n")
                log.write("SHELL_FLOW_RENDERED actual_mouse=true options=true metadata_without_match=true rejected_map_dialog=true rejected_map_retry=true original_loading_screen=true resolved_slots=true original_models=7\n")
            return status

        with patch.object(RUNNER, "run_logged", paid):
            result = RUNNER.run_case(self.project, self.binary, self.identity, self.case, self.output / "foundation-only")
        self.assertEqual(result["errors"], ["Paid Power Plant never reported completion"])

        def verified(project, executable, command, session_path=None):
            status = paid(project, executable, command, session_path)
            with session_path.open("a") as log:
                log.write("SHELL_FLOW_BUILD_FINISHED template=AmericaPowerPlant object=40 tick=900\n")
            return status

        with patch.object(RUNNER, "run_logged", verified):
            result = RUNNER.run_case(self.project, self.binary, self.identity, self.case, self.output / "complete-flow")
        self.assertEqual(result["process_gates"], "pass")
        self.assertEqual(result["visual_review"], "pending")

    def test_lightpulse_requires_fresh_inactive_active_and_final_images(self):
        self.case.update(id="native-lightpulse-capture", arguments=["--lightpulse-capture"])
        before = ("lightpulse-before.png", b"inactive lightpulse state")
        active = ("lightpulse-active.png", b"active lightpulse state")
        final = ("lightpulse.png", b"final lightpulse capture")
        with patch.object(RUNNER, "run_logged", self.fake_session(0, [before, final])):
            result = RUNNER.run_case(self.project, self.binary, self.identity, self.case, self.output / "missing-active")
        self.assertEqual(result["process_gates"], "fail")
        self.assertTrue(any("lightpulse-active.png" in error for error in result["errors"]))
        with patch.object(RUNNER, "run_logged", self.fake_session(0, [before, active, final])):
            result = RUNNER.run_case(self.project, self.binary, self.identity, self.case, self.output / "complete-lightpulse")
        self.assertEqual(result["process_gates"], "pass")
        self.assertEqual({image["file"] for image in result["images"]},
                         {before[0], active[0], final[0]})

    def test_aircraft_crash_requires_fresh_all_views_and_hulk_images(self):
        self.case.update(id="native-smoke-aircraft-crash", arguments=["--smoke-aircraft-crash"])
        names = (
            "aircraft-crash-falling.png",
            "aircraft-crash-falling-view-1.png",
            "aircraft-crash-falling-view-2.png",
            "aircraft-crash-falling-view-3.png",
            "aircraft-crash.png",
            "aircraft-crash-hulk-view-0.png",
            "aircraft-crash-hulk-view-1.png",
            "aircraft-crash-hulk-view-2.png",
            "aircraft-crash-hulk.png",
        )
        complete = [(name, f"fresh {name}".encode()) for name in names]
        partial = [(names[0], b"falling model and debris")]
        with patch.object(RUNNER, "run_logged", self.fake_session(0, partial)):
            result = RUNNER.run_case(self.project, self.binary, self.identity, self.case, self.output / "partial-falling")
        self.assertEqual(result["process_gates"], "fail")
        self.assertTrue(any("aircraft-crash.png" in error for error in result["errors"]))
        stale_hulk = self.output / "aircraft-crash-hulk.png"
        stale_hulk.write_bytes(b"old hulk image")
        os.utime(stale_hulk, ns=(1, 1))
        missing_hulk = [image for image in complete if image[0] != "aircraft-crash-hulk.png"]
        with patch.object(RUNNER, "run_logged", self.fake_session(0, missing_hulk)):
            result = RUNNER.run_case(self.project, self.binary, self.identity, self.case, self.output / "missing-hulk")
        self.assertEqual(result["process_gates"], "fail")
        self.assertTrue(any("aircraft-crash-hulk.png" in error for error in result["errors"]))
        self.assertNotIn("aircraft-crash-hulk.png", [image["file"] for image in result["images"]])
        missing_quarter = [image for image in complete if image[0] != "aircraft-crash-falling-view-2.png"]
        with patch.object(RUNNER, "run_logged", self.fake_session(0, missing_quarter)):
            result = RUNNER.run_case(self.project, self.binary, self.identity, self.case, self.output / "missing-quarter")
        self.assertEqual(result["process_gates"], "fail")
        self.assertTrue(any("aircraft-crash-falling-view-2.png" in error for error in result["errors"]))
        self.assertNotIn("aircraft-crash-falling-view-2.png", [image["file"] for image in result["images"]])
        missing_hulk = [image for image in complete if image[0] != "aircraft-crash-hulk-view-1.png"]
        with patch.object(RUNNER, "run_logged", self.fake_session(0, missing_hulk)):
            result = RUNNER.run_case(self.project, self.binary, self.identity, self.case, self.output / "missing-hulk-view")
        self.assertEqual(result["process_gates"], "fail")
        self.assertTrue(any("aircraft-crash-hulk-view-1.png" in error for error in result["errors"]))
        with patch.object(RUNNER, "run_logged", self.fake_session(0, complete)):
            result = RUNNER.run_case(self.project, self.binary, self.identity, self.case, self.output / "complete-aircraft")
        self.assertEqual(result["process_gates"], "pass")
        self.assertEqual({image["file"] for image in result["images"]}, set(names))
        self.assertEqual(result["visual_review"], "pending")
        self.assertEqual(result["physical_audio_review"], "not_recorded")

    def test_paid_captures_require_fresh_intermediate_and_final_images(self):
        cases = {
            "microwave": ("microwave-disabled-building.png", "microwave-garrison-cleared.png", "microwave-emitter-first.png", "microwave-emitter.png"),
            "angry-mob": ("angry-mob-paid-original-bodies.png", "angry-mob-normal-weapons.png", "angry-mob-normal-rock.png", "angry-mob-normal-molotov.png", "angry-mob-upgraded-molotov.png", "angry-mob.png"),
            "ecm": ("ecm-disabled-vehicle.png", "ecm-jammed-missile.png", "ecm-pulse-first.png", "ecm.png"),
            "structure-collapse": ("structure-collapse-falling.png", "structure-collapse.png"),
            "structure-topple": ("structure-topple-falling.png", "structure-topple.png"),
            "battle-bus": ("battle-bus-loaded.png", "battle-bus-thrown.png", "battle-bus-held.png", "battle-bus.png"),
            "stealth-fighter": ("stealth-fighter-parked.png", "stealth-fighter-airborne.png", "stealth-fighter.png"),
        }
        for name, files in cases.items():
            with self.subTest(capture=name):
                self.case.update(id=f"native-{name}-capture", arguments=[f"--{name}-capture"])
                final = (files[-1], b"final screenshot")
                with patch.object(RUNNER, "run_logged", self.fake_session(0, [final])):
                    result = RUNNER.run_case(self.project, self.binary, self.identity, self.case, self.output / f"partial-{name}")
                self.assertEqual(result["process_gates"], "fail")
                self.assertTrue(any(files[0] in error for error in result["errors"]))
                images = [(file, b"fresh source art screenshot") for file in files]
                with patch.object(RUNNER, "run_logged", self.fake_session(0, images)):
                    result = RUNNER.run_case(self.project, self.binary, self.identity, self.case, self.output / f"complete-{name}")
                self.assertEqual(result["process_gates"], "pass")
                self.assertEqual({image["file"] for image in result["images"]}, set(files))

    def test_profile_requires_one_complete_timing_summary_and_its_verified_image(self):
        self.case.update(id="profiles-profile-defenses", group="profiles",
                         arguments=["--profile-defenses"])
        timing = ("live-frames count=300 p50_ms=10.000 p95_ms=15.500 max_ms=20.000 "
                  "over_60hz=1 over_30hz=0 over_100ms=0\n")
        for index, (lines, save, file, expected) in enumerate([
            ("", True, "playtest.png", "fail"),
            (timing.replace("300", "299"), True, "playtest.png", "fail"),
            (timing * 2, True, "playtest.png", "fail"),
            ("live-frames count=300\n", True, "playtest.png", "fail"),
            (timing, False, "playtest.png", "fail"),
            (timing, True, "other.png", "fail"),
            (timing, True, "playtest.png", "pass"),
        ]):
            def launch(project, _executable, _command, session_path=None):
                proof = f"Verified screenshot saved: {project / 'output/playtest.png'}\n" if save else ""
                session_path.write_text(lines + proof + "exit_status: 0\n")
                (project / "output" / file).write_bytes(f"frame {index}".encode())
                return 0
            with self.subTest(case=index), patch.object(RUNNER, "run_logged", launch):
                result = RUNNER.run_case(self.project, self.binary, self.identity, self.case,
                                         self.output / f"profile-{index}")
                self.assertEqual(result["process_gates"], expected)

    def test_ignored_scaling_filter_cannot_succeed_by_running_zero_tests(self):
        self.case.update(id="scaling-simulation-scaling", group="scaling",
                         arguments=["cargo-local.sh", "simulation_scaling"], runtime=False, requires_image=False)
        for passed in [0, 1]:
            def launch(project, _executable, _command, session_path=None):
                session_path.write_text(
                    f"test result: ok. {passed} passed; 0 failed; 0 ignored; 600 filtered out\n")
                return 0
            with patch.object(RUNNER, "run_logged", launch):
                result = RUNNER.run_case(self.project, self.binary, self.identity, self.case,
                                         self.output / f"scaling-{passed}")
            self.assertEqual(result["process_gates"], "pass" if passed else "fail")

    def test_contained_environment_is_set_before_help_and_inherited_by_case(self):
        source_path = self.project / "crates/generals-client/src/main.rs"
        source_path.parent.mkdir(parents=True)
        source_path.write_text(SOURCE)
        arguments = argparse.Namespace(executable=str(self.binary), group=["native"],
                                       case=["native-verify-audio"], list=False,
                                       output=None, stop_on_failure=False)

        def assert_environment():
            self.assertTrue(sys.dont_write_bytecode)
            self.assertEqual(os.environ["PYTHONDONTWRITEBYTECODE"], "1")
            for name, directory in [("TMPDIR", "tmp"), ("XDG_CACHE_HOME", "xdg"),
                                    ("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data")]:
                value = self.project / ".cache" / directory
                self.assertEqual(os.environ[name], str(value))
                self.assertTrue(value.is_dir())

        def help_command(command, **_options):
            assert_environment()
            self.assertEqual(command, [str(self.binary), "--help"])
            return subprocess.CompletedProcess(command, 0, help_fixture(), "")

        def launch(project, _executable, _command, session_path=None):
            assert_environment()
            session_path.write_text(f"{RUNNER.AUDIO_COMPLETE}\nexit_status: 0\n")
            return 0

        with patch.object(RUNNER, "PROJECT", self.project), \
                patch.object(RUNNER.subprocess, "run", side_effect=help_command), \
                patch.object(RUNNER, "run_logged", side_effect=launch):
            self.assertEqual(RUNNER.run(arguments), 0)

    def test_missing_bound_session_fails_even_when_shared_alias_exists(self):
        self.case.update(id="native-verify-audio", arguments=["--verify-audio"], requires_image=False)
        (self.output / "playtest-run.log").write_text("unrelated session\nexit_status: 0\n")
        with patch.object(RUNNER, "run_logged", return_value=0):
            result = RUNNER.run_case(self.project, self.binary, self.identity, self.case,
                                     self.output / "missing-session")
        self.assertEqual(result["process_gates"], "fail")
        self.assertEqual(result["errors"], ["Session log was not created"])

    def test_case_retains_exact_session_when_latest_alias_is_renamed_or_removed(self):
        self.case.update(id="installed-log-binding", group="installed", runtime=False, requires_image=False)
        for operation in ["rename", "remove"]:
            with self.subTest(operation=operation):
                source = """
from pathlib import Path
import sys
latest = Path('output/playtest-run.log')
bound = Path(sys.argv[1])
assert bound.is_file(), 'session evidence must be bound before the child starts'
assert bound.samefile(latest), 'session evidence must identify this exact log'
if sys.argv[2] == 'rename':
    latest.rename(latest.with_name('playtest-run 9.log'))
else:
    latest.unlink()
latest.write_text('different session: must never be accepted as this case log')
print('exact accepted child output')
"""
                evidence = self.output / f"alias-{operation}"
                self.case["arguments"] = [sys.executable, "-c", source,
                                          str(evidence / "session.log"), operation]
                result = RUNNER.run_case(self.project, self.binary, self.identity, self.case, evidence)
                self.assertEqual(result["process_gates"], "pass", result["errors"])
                self.assertEqual(result["exit_status"], 0)
                session = evidence / "session.log"
                log = session.read_text()
                self.assertIn("\nexact accepted child output\n", log)
                self.assertIn("exit_status: 0", log)
                self.assertNotIn("\ndifferent session: must never be accepted as this case log", log)
                immutable = Path(log.split("session_log: ", 1)[1].split("\n", 1)[0])
                self.assertTrue(session.samefile(immutable))
                immutable.unlink()
                self.assertEqual(session.read_text(), log)

    def test_changed_runtime_is_rejected_before_any_launch(self):
        self.binary.write_bytes(b"replacement binary")
        with patch.object(RUNNER, "run_logged") as launch, self.assertRaisesRegex(ValueError, "runtime changed"):
            RUNNER.run_case(self.project, self.binary, self.identity, self.case, self.output / "changed")
        launch.assert_not_called()

    def test_signal_stops_batch_and_retains_all_unrun_cases_and_original_binary(self):
        source_path = self.project / "crates/generals-client/src/main.rs"
        source_path.parent.mkdir(parents=True)
        source_path.write_text(SOURCE)
        arguments = argparse.Namespace(executable=str(self.binary), group=["native"], case=[],
                                       list=False, output=None, stop_on_failure=False)
        completed_help = subprocess.CompletedProcess([str(self.binary), "--help"], 0, help_fixture(), "")

        def interrupted(project, executable, command, session_path=None):
            self.assertNotEqual(executable, self.binary)
            self.assertEqual(RUNNER.digest(executable), self.identity)
            self.assertEqual(command[-1], "--smoke-infantry-capture")
            session_path.write_text("termination_signal: 15\nexit_status: 143\n")
            return 143

        with patch.object(RUNNER, "PROJECT", self.project), \
                patch.object(RUNNER.subprocess, "run", return_value=completed_help), \
                patch.object(RUNNER, "run_logged", side_effect=interrupted) as launch:
            self.assertEqual(RUNNER.run(arguments), 1)
        launch.assert_called_once()
        manifest_path = next((self.output / "acceptance").glob("*/manifest.json"))
        manifest = json.loads(manifest_path.read_text())
        self.assertEqual(len(manifest["results"]), 1)
        native = [case["id"] for case in RUNNER.inventory(help_fixture(), SOURCE)
                  if case["group"] == "native"]
        self.assertEqual(manifest["not_run"], native[1:])
        self.assertEqual(manifest["results"][0]["exit_status"], 143)
        self.assertEqual(RUNNER.digest(self.binary), self.identity)

    def test_source_change_during_case_fails_even_when_process_succeeds(self):
        source_path = self.project / "crates/generals-client/src/main.rs"
        source_path.parent.mkdir(parents=True)
        source_path.write_text(SOURCE)
        arguments = argparse.Namespace(executable=str(self.binary), group=["installed"], case=[],
                                       list=False, output=None, stop_on_failure=False)
        completed_help = subprocess.CompletedProcess([str(self.binary), "--help"], 0, help_fixture(), "")

        def changed_source(project, _executable, _command, session_path=None):
            session_path.write_text("exit_status: 0\n")
            (source_path.parent / "new_module.rs").write_text("// saved during acceptance\n")
            return 0

        with patch.object(RUNNER, "PROJECT", self.project), \
                patch.object(RUNNER.subprocess, "run", return_value=completed_help), \
                patch.object(RUNNER, "run_logged", side_effect=changed_source) as launch:
            self.assertEqual(RUNNER.run(arguments), 1)
        launch.assert_called_once()
        manifest_path = next((self.output / "acceptance").glob("*/manifest.json"))
        manifest = json.loads(manifest_path.read_text())
        installed = [case["id"] for case in RUNNER.inventory(help_fixture(), SOURCE)
                     if case["group"] == "installed"]
        self.assertEqual(manifest["not_run"], installed[1:])
        self.assertEqual(manifest["results"][0]["process_gates"], "fail")
        self.assertIn("Source changed during acceptance", manifest["results"][0]["errors"][0])
        self.assertIn("Cargo.lock", manifest["source"]["files"])

    def test_source_identity_covers_shaders_and_scripts_but_not_generated_bytecode(self):
        crate = self.project / "crates/example"
        scripts = self.project / "tests"
        crate.mkdir(parents=True)
        scripts.mkdir()
        (crate / "source_material.wgsl").write_text("shader")
        (scripts / "validate.sh").write_text("checks")
        before = RUNNER.source_identity(self.project)
        bytecode = scripts / "__pycache__"
        bytecode.mkdir()
        (bytecode / "generated.pyc").write_bytes(b"cache")
        self.assertEqual(RUNNER.source_identity(self.project), before)
        (crate / "source_material.wgsl").write_text("changed shader")
        changed_shader = RUNNER.source_identity(self.project)
        self.assertNotEqual(changed_shader["sha256"], before["sha256"])
        (scripts / "validate.sh").write_text("changed checks")
        self.assertNotEqual(RUNNER.source_identity(self.project)["sha256"], changed_shader["sha256"])


if __name__ == "__main__":
    unittest.main()
