from __future__ import annotations

import copy
import importlib.util
import json
import struct
import subprocess
import sys
import tempfile
import unittest
import wave
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
SKILL_ROOT = REPOSITORY_ROOT / "skills/system-design-video"
SCRIPT = SKILL_ROOT / "scripts/check_video_spec.py"
EXAMPLE = SKILL_ROOT / "assets/cache-miss-example.json"
SPEC = importlib.util.spec_from_file_location("check_video_spec", SCRIPT)
assert SPEC and SPEC.loader
checker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checker)


class SystemDesignVideoTests(unittest.TestCase):
    def setUp(self) -> None:
        self.plan = json.loads(EXAMPLE.read_text(encoding="utf-8"))
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.project = Path(self.directory.name)

    def errors(self, plan: dict | None = None, ready: bool = False) -> list[str]:
        return checker.validate_spec(self.plan if plan is None else plan, self.project, ready)

    def assert_issue(self, expected: str, ready: bool = False) -> None:
        self.assertTrue(any(expected in error for error in self.errors(ready=ready)), self.errors(ready=ready))

    def write_audio(
        self, name: str = "voice.wav", seconds: float = 1.0, *, channels: int = 1, sample_width: int = 2
    ) -> None:
        with wave.open(str(self.project / name), "wb") as clip:
            clip.setnchannels(channels)
            clip.setsampwidth(sample_width)
            clip.setframerate(8000)
            clip.writeframes(b"\x00" * channels * sample_width * round(seconds * 8000))

    def measured_plan(self) -> None:
        self.write_audio()
        for scene in self.plan["scenes"]:
            scene["narration"].update(file="voice.wav", timing_basis="measured", duration_seconds=1)

    def test_bundled_plan_passes_without_claiming_measured_audio(self) -> None:
        self.assertEqual([], self.errors())
        self.assert_issue("require measured narration", ready=True)

    def test_measured_clips_pass_and_checker_is_read_only(self) -> None:
        self.measured_plan()
        before_plan = copy.deepcopy(self.plan)
        before_files = {path.name: path.read_bytes() for path in self.project.iterdir()}
        self.assertEqual([], self.errors(ready=True))
        self.assertEqual(before_plan, self.plan)
        self.assertEqual(before_files, {path.name: path.read_bytes() for path in self.project.iterdir()})

    def test_supported_and_inferred_claims_require_known_sources(self) -> None:
        for kind in ("supported", "inference"):
            with self.subTest(kind=kind):
                self.plan["claims"][0]["kind"] = kind
                self.plan["claims"][0]["source_ids"] = []
                self.assert_issue("source_ids: must not be empty")
        self.plan["claims"][0]["source_ids"] = ["not-inspected"]
        self.assert_issue("unknown reference 'not-inspected'")

    def test_illustrative_models_can_have_no_external_source(self) -> None:
        self.plan["sources"] = []
        for claim in self.plan["claims"]:
            claim.update(kind="illustrative", source_ids=[], rationale="Synthetic evaluation model.")
        self.assertEqual([], self.errors())

    def test_duplicate_ids_and_missing_claim_references_fail(self) -> None:
        self.plan["nodes"].append(copy.deepcopy(self.plan["nodes"][0]))
        self.plan["edges"][0]["claim_ids"] = ["invented-claim"]
        self.assert_issue("duplicate id 'client'")
        self.assert_issue("unknown reference 'invented-claim'")

    def test_packet_endpoints_and_unknown_nodes_fail(self) -> None:
        self.plan["events"][1]["node_ids"] = ["client"]
        self.plan["edges"][0]["to"] = "invented-node"
        self.assert_issue("packet participants must include both edge endpoints")
        self.assert_issue("unknown reference 'invented-node'")

    def test_forward_dependencies_and_reversed_event_order_fail(self) -> None:
        self.plan["events"][1]["after"] = ["db-result"]
        self.plan["events"][2]["at"] = 12
        self.assert_issue("unknown reference 'db-result'")
        self.assert_issue("events must be chronological")

    def test_event_cannot_be_at_exclusive_scene_end(self) -> None:
        self.plan["events"][0]["at"] = 12
        self.assert_issue("event must occur inside its scene window")

    def test_duration_aware_dependencies_wait_for_completion(self) -> None:
        self.plan = json.loads((SKILL_ROOT / "assets/reference-video.json").read_text(encoding="utf-8"))
        self.plan["events"][1].update(at=0.5, scene_id="send")
        self.assert_issue("starts before dependency")
        self.plan["events"][1].update(at=1.1, scene_id="receive")
        self.assertEqual([], self.errors())

    def test_v2_rejects_missing_duration_state_and_overflow(self) -> None:
        self.plan = json.loads((SKILL_ROOT / "assets/reference-video.json").read_text(encoding="utf-8"))
        del self.plan["events"][0]["duration_seconds"]
        self.plan["events"][1]["state_after"] = {"unknown": "done"}
        self.assert_issue("duration_seconds")
        self.assert_issue("must participate")
        self.plan["events"][0]["duration_seconds"] = 2
        self.assert_issue("completion extends beyond")

    def test_caption_order_and_video_fit_are_checked(self) -> None:
        self.plan["captions"] = [{"start": 1, "end": 3, "text": "First"},
                                 {"start": 2, "end": 91, "text": "Second"}]
        self.assert_issue("nonoverlapping")
        self.assert_issue("cue extends beyond")

    def test_scene_gaps_overlaps_and_incomplete_duration_fail(self) -> None:
        for start in (11, 13):
            with self.subTest(start=start):
                self.plan["scenes"][1]["start"] = start
                self.assert_issue("scene windows must be contiguous")
        self.plan["scenes"][1]["start"] = 12
        self.plan["profile"]["duration_seconds"] = 91
        self.assert_issue("last scene must end")

    def test_invalid_numeric_values_and_boolean_dimensions_fail(self) -> None:
        for value in (True, float("nan"), float("inf"), -1, 10**400):
            with self.subTest(value=str(value)[:20]):
                self.plan["profile"]["fps"] = value
                self.assert_issue("profile.fps")
        self.plan["profile"]["width"] = True
        self.assert_issue("profile.width")
        self.plan["schema_version"] = True
        self.assert_issue("schema_version")

    def test_malformed_arrays_and_nested_fields_fail_without_crashing(self) -> None:
        for key in ("sources", "claims", "nodes", "edges", "scenes", "events"):
            for value in (None, "wrong", [None], [{"id": []}]):
                with self.subTest(key=key, value=value):
                    plan = copy.deepcopy(self.plan)
                    plan[key] = value
                    self.assertTrue(self.errors(plan))
        self.plan["events"][0].update(scene_id=[], node_ids=None, after={})
        self.plan["scenes"][0]["narration"] = []
        self.assertTrue(self.errors())
        self.assertTrue(checker.validate_spec([], self.project))

    def test_missing_and_invalid_audio_fail_ready_check(self) -> None:
        self.measured_plan()
        (self.project / "voice.wav").unlink()
        self.assert_issue("cannot measure PCM WAV", ready=True)
        (self.project / "voice.wav").write_text("not a wave file", encoding="utf-8")
        self.assert_issue("cannot measure PCM WAV", ready=True)

    def test_truncated_pcm_payloads_fail_ready_check(self) -> None:
        self.measured_plan()
        clip_path = self.project / "voice.wav"
        for seconds, missing_bytes in ((1, 16000), (1, 8000), (1, 1), (9, 8000)):
            with self.subTest(seconds=seconds, missing_bytes=missing_bytes):
                self.write_audio(seconds=seconds)
                for scene in self.plan["scenes"]:
                    scene["narration"]["duration_seconds"] = seconds
                clip_path.write_bytes(clip_path.read_bytes()[:-missing_bytes])
                self.assert_issue("cannot measure PCM WAV", ready=True)

    def test_incomplete_pcm_frame_fails_ready_check(self) -> None:
        self.measured_plan()
        clip_path = self.project / "voice.wav"
        encoded = bytearray(clip_path.read_bytes())
        data_header = encoded.index(b"data")
        data_bytes = len(encoded) - data_header - 8
        # Add one sample fragment and its separate RIFF padding byte.
        encoded.extend(b"\x00\x00")
        struct.pack_into("<I", encoded, 4, len(encoded) - 8)
        struct.pack_into("<I", encoded, data_header + 4, data_bytes + 1)
        clip_path.write_bytes(encoded)
        self.assert_issue("incomplete PCM frame", ready=True)

    def test_complete_longer_pcm_clips_pass_ready_check(self) -> None:
        self.measured_plan()
        for channels, sample_width in ((1, 1), (1, 2), (2, 3)):
            with self.subTest(channels=channels, sample_width=sample_width):
                self.write_audio(seconds=9, channels=channels, sample_width=sample_width)
                for scene in self.plan["scenes"]:
                    scene["narration"]["duration_seconds"] = 9
                self.assertEqual([], self.errors(ready=True))

    def test_audio_declared_duration_and_actual_scene_fit_are_checked(self) -> None:
        self.measured_plan()
        self.plan["scenes"][0]["narration"]["duration_seconds"] = 2
        self.assert_issue("declares 2s but WAV measures 1s", ready=True)
        self.plan["scenes"][0]["narration"].update(duration_seconds=1, offset=11.05)
        self.assert_issue("narration extends beyond the scene", ready=True)
        self.plan["scenes"][0]["narration"]["offset"] = 11
        self.write_audio(seconds=1.05)
        self.assert_issue("measured WAV extends beyond the scene", ready=True)

    def test_audio_paths_cannot_escape_project(self) -> None:
        self.measured_plan()
        for name in ("../outside.wav", str(self.project / "voice.wav"), "invalid\x00.wav"):
            with self.subTest(name=name):
                self.plan["scenes"][0]["narration"]["file"] = name
                self.assertTrue(self.errors(ready=True))

    def test_symlink_audio_paths_cannot_escape_project(self) -> None:
        self.measured_plan()
        with tempfile.TemporaryDirectory() as outside:
            (Path(outside) / "voice.wav").write_bytes((self.project / "voice.wav").read_bytes())
            try:
                (self.project / "escaped.wav").symlink_to(Path(outside) / "voice.wav")
            except (OSError, NotImplementedError) as error:
                self.skipTest(f"Symlink creation unavailable: {error}")
            self.plan["scenes"][0]["narration"]["file"] = "escaped.wav"
            self.assert_issue("must stay inside", ready=True)

    def test_explicit_silent_plan_needs_no_voice_clips(self) -> None:
        self.plan["profile"]["narrated"] = False
        for scene in self.plan["scenes"]:
            del scene["narration"]
        self.assertEqual([], self.errors(ready=True))

    def test_cli_works_from_standalone_install_and_rejects_bad_json(self) -> None:
        script = self.project / "check_video_spec.py"
        script.write_bytes(SCRIPT.read_bytes())
        spec = self.project / "video-spec.json"
        spec.write_text(json.dumps(self.plan), encoding="utf-8")
        before = spec.read_bytes()
        result = subprocess.run([sys.executable, str(script), str(spec), "--json"], capture_output=True, text=True)
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual({"valid": True, "stage": "plan", "errors": []}, json.loads(result.stdout))
        self.assertEqual(before, spec.read_bytes())
        result = subprocess.run([sys.executable, str(script), str(spec), "--ready", "--json"], capture_output=True, text=True)
        self.assertEqual(1, result.returncode)
        self.assertEqual("animation-inputs", json.loads(result.stdout)["stage"])
        spec.write_text("{broken", encoding="utf-8")
        result = subprocess.run([sys.executable, str(script), str(spec), "--json"], capture_output=True, text=True)
        self.assertEqual(1, result.returncode)
        self.assertFalse(json.loads(result.stdout)["valid"])


if __name__ == "__main__":
    unittest.main()
