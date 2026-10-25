from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "shared/video-production/scene_bridge.py"
SPEC = importlib.util.spec_from_file_location("bridge_gate", SOURCE)
assert SPEC and SPEC.loader
gate = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gate)


class StoryBridgeTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.project = Path(self.temporary.name).resolve()
        self.record = {
            "schema_version": 1, "episode_id": "synthetic-theatre", "creator_id": "fixture-writer",
            "director_id": "fixture-director", "fps": 10, "frame_count": 20,
            "references": {kind: {"status": "unavailable", "locator": "synthetic fixture",
                                    "observations": "No real reference pixels in this unit fixture"}
                           for kind in ("style", "storytelling")},
            "bridges": [{"id": "poster-change", "visual_form": "contextual_story", "context": "theatre box office",
                "subject": "staff member and audience phone", "story_action": "staff replaces the poster while the phone retains the old one",
                "incoming_idea": "a stored copy can be old", "next_idea": "ask whether the source version changed",
                "semantic_link": "match the same phone into validation", "continuity_anchor": "same poster and phone",
                "return_to_explanation": "the phone becomes the browser-copy view", "incoming_scene_id": "stored-copy",
                "scene_id": "story", "next_scene_id": "validation", "start_frame": 3, "end_frame": 13,
                "sample_frames": {"before": 2, "entry": 3, "action": 7, "exit": 12, "after": 14}}],
        }
        self.metadata = {"fps": 10, "frame_count": 20, "width": 64, "height": 64}

    def reference(self, name, content):
        path = self.project / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
        return {"path": name, "sha256": hashlib.sha256(content).hexdigest()}

    def implemented(self):
        self.record["bridges"][0]["implementation"] = {
            "source": self.reference("render.py", b"def draw_story(): pass\n"),
            "locator": "draw_story, dispatched by scene_id story", "assets": [],
        }

    @staticmethod
    def frame_bytes(index):
        # Synthetic bytes used only with a mocked decoder, not real image-quality evidence.
        return b"\x89PNG\r\n\x1a\n" + str(index).encode()

    def attach_reviews(self, record):
        frames = record["encoded"]["frames"]
        hashes = {f"{identifier}:{phase}": image["sha256"]
                  for identifier, evidence in frames.items() for phase, image in evidence.items()}
        first = {"input_scope": "audience_evidence_only", "reviewer_id": "fixture-logical",
                 "video_sha256": record["encoded"]["video"]["sha256"], "frame_hashes": hashes,
                 "observations": {"poster-change": {key: "Fictional test observation"
                    for key in ("situation", "visible_event", "new_understanding", "return_to_mechanism")}}}
        first_ref = self.reference("blind-reading.json", json.dumps(first).encode())
        observation = {"story_scene_present": True, "introduces_next_idea": True, "continuity_clear": True,
                       "caption_safe": True, "observed_story_action": "Fictional test action",
                       "observed_connection": "Fictional test connection", "audience_first_reading": "Fictional first reading"}
        record["reviews"] = [{"role": role, "reviewer_id": "fixture-" + role, "decision": "APPROVE",
                              "coverage": "sampled_frames", "inspected_frames": sorted(hashes),
                              "observations": {"poster-change": copy.deepcopy(observation)}}
                             for role in ("visual", "logical")]
        record["reviews"][1]["first_reading"] = first_ref
        for review in record["reviews"]:
            review["candidate_digest"] = gate.candidate_digest(record)

    def encoded(self):
        self.implemented()
        self.record["encoded"] = {"video": self.reference("final.mp4", b"synthetic video bytes"), "frames": {}}
        frames = {}
        for phase, index in self.record["bridges"][0]["sample_frames"].items():
            frames[phase] = {**self.reference(f"frames/{phase}.png", self.frame_bytes(index)), "frame": index}
        self.record["encoded"]["frames"]["poster-change"] = frames
        self.attach_reviews(self.record)

    def check(self, stage="encoded", **kwargs):
        with patch.object(gate, "probe_video", return_value=self.metadata), \
             patch.object(gate, "encoded_frame", side_effect=lambda path, index, ffmpeg: self.frame_bytes(index)):
            return gate.check_record(self.record, self.project, stage, **kwargs)

    def assert_blocked(self, phrase, stage="encoded", **kwargs):
        result = self.check(stage, **kwargs)
        self.assertFalse(result["valid"], result)
        self.assertIn(phrase, " ".join(result["errors"]))

    def test_story_plan_cannot_stand_in_for_implementation_or_encode(self):
        self.assertTrue(self.check("plan")["valid"])
        self.assert_blocked("implemented render mapping", "implemented")
        self.implemented()
        self.assertTrue(self.check("implemented")["valid"])
        self.assert_blocked("Actual encoded video")

    def test_missing_or_generic_transition_scene_is_rejected(self):
        for form in ("diagram_transform", "fade", "camera_move", "furniture_backed_diagram"):
            with self.subTest(form=form):
                self.record["bridges"][0]["visual_form"] = form
                self.assert_blocked("storytelling scene", "plan")
        self.record["bridges"] = []
        self.assert_blocked("At least one storytelling", "plan")

    def test_production_regressions_fail_even_when_renamed_and_generically_approved(self):
        cases = json.loads((ROOT / "tests/fixtures/story_bridge_regressions.json").read_text())["cases"]
        self.encoded()
        for case in cases:
            with self.subTest(case=case["id"]):
                for review in self.record["reviews"]:
                    review["observations"]["poster-change"].update({key: case[key]
                        for key in ("story_scene_present", "observed_story_action", "observed_connection")})
                # Relabeling the same diagram as contextual does not override a negative blind review.
                self.record["bridges"][0]["visual_form"] = "contextual_story"
                for review in self.record["reviews"]:
                    review["candidate_digest"] = gate.candidate_digest(self.record)
                self.assert_blocked("story_scene_present")

    def test_accepted_structure_requires_current_independent_recorded_evidence(self):
        self.encoded()
        result = self.check()
        self.assertTrue(result["valid"], result)
        self.assertIn("not automated story quality", result["scope"])
        self.record["reviews"] = []
        self.assert_blocked("independent visual and logical")

    def test_stale_source_movie_or_frame_cannot_reuse_approval(self):
        self.encoded()
        for path in ("render.py", "final.mp4", "frames/action.png"):
            with self.subTest(path=path):
                target = self.project / path
                original = target.read_bytes()
                target.write_bytes(b"changed")
                self.assert_blocked("Stale artifact")
                target.write_bytes(original)

    def test_frame_from_another_video_rejected_even_if_its_own_hash_matches(self):
        self.encoded()
        ref = self.record["encoded"]["frames"]["poster-change"]["action"]
        ref.update(self.reference("frames/action.png", b"different pixels"))
        self.assert_blocked("actual encoded video frame")
        self.encoded()
        self.assert_blocked("different delivery video", expected_video=self.project / "other.mp4")

    def test_no_creator_director_or_combined_reviewer_approval(self):
        self.encoded()
        for identity in ("fixture-writer", "fixture-director", "fixture-visual"):
            self.record["reviews"][1]["reviewer_id"] = identity
            self.assert_blocked("distinct from creator")

    def test_blind_reading_is_required_and_bound_to_current_encoded_samples(self):
        self.encoded()
        del self.record["reviews"][1]["first_reading"]
        self.assert_blocked("Artifact needs path")
        self.encoded()
        first = json.loads((self.project / "blind-reading.json").read_text())
        first["video_sha256"] = "0" * 64
        ref = self.reference("blind-reading.json", json.dumps(first).encode())
        self.record["reviews"][1]["first_reading"] = ref
        self.assert_blocked("another encode")

    def test_plan_or_timing_changes_invalidate_review_and_bad_probe_blocks(self):
        self.encoded()
        self.record["bridges"][0]["story_action"] = "changed teaching action"
        self.assert_blocked("Stale bridge review")
        self.encoded()
        self.metadata["frame_count"] = 21
        self.assert_blocked("Retiming required")

    def test_samples_must_cover_bridge_entry_action_result_and_both_neighbors(self):
        self.record["bridges"][0]["sample_frames"]["after"] = 12
        self.assert_blocked("span the bridge", "plan")

    def test_style_reference_cannot_silently_replace_storytelling_reference(self):
        del self.record["references"]["storytelling"]
        self.assert_blocked("storytelling reference", "plan")

    def test_capture_produces_unapproved_evidence_and_never_overwrites(self):
        self.implemented()
        self.reference("final.mp4", b"synthetic")
        with patch.object(gate, "probe_video", return_value=self.metadata), \
             patch.object(gate, "encoded_frame", side_effect=lambda path, index, ffmpeg: self.frame_bytes(index)):
            result = gate.capture(self.record, self.project, "final.mp4", "samples", "captured.json", "ffmpeg", "ffprobe")
            captured = json.loads((self.project / "captured.json").read_text())
            self.assertEqual([], captured["reviews"])
            self.assertIn("pending", result)
            with self.assertRaisesRegex(ValueError, "never overwrites"):
                gate.capture(self.record, self.project, "final.mp4", "samples", "captured.json", "ffmpeg", "ffprobe")

    def test_escape_and_malformed_records_fail_closed(self):
        self.implemented()
        self.record["bridges"][0]["implementation"]["source"]["path"] = "../outside.py"
        self.assert_blocked("project-relative", "implemented")
        for document in (None, [], {"schema_version": True}, {"schema_version": 1, "fps": float("nan")}):
            result = gate.check_record(document, self.project, "encoded")
            self.assertFalse(result["valid"], result)

    def test_local_timeline_cannot_render_a_bridge_as_an_unnamed_effect(self):
        spec = {"profile": {"fps": 10, "duration_seconds": 2}, "scenes": [
            {"id": "stored-copy", "start": 0, "end": .3}, {"id": "story", "start": .3, "end": 1.3},
            {"id": "validation", "start": 1.3, "end": 2}]}
        self.assertEqual([], gate.check_spec_mapping(self.record, spec))
        spec["scenes"][1]["id"] = "ordinary-diagram"
        self.assertTrue(gate.check_spec_mapping(self.record, spec))

    def test_media_check_cli_requires_bridge_gate_instead_of_technical_only_success(self):
        script = ROOT / "skills/system-design-video/scripts/video_project.py"
        result = subprocess.run([sys.executable, "-B", str(script), "media-check", "missing.json", "missing.mp4"],
                                capture_output=True, text=True)
        self.assertEqual(2, result.returncode)
        self.assertIn("--bridge-record", result.stderr)

    @unittest.skipUnless(os.environ.get("VIDEO_FFMPEG") and os.environ.get("VIDEO_FFPROBE"), "Set configured media executables for synthetic decode integration")
    def test_synthetic_capture_and_redecode_with_real_ffmpeg_is_provenance_only(self):
        self.implemented()
        ffmpeg, ffprobe = os.environ["VIDEO_FFMPEG"], os.environ["VIDEO_FFPROBE"]
        subprocess.run([ffmpeg, "-v", "error", "-f", "lavfi", "-i", "testsrc=size=64x64:rate=10",
                        "-t", "2", "-c:v", "libx264", "-pix_fmt", "yuv420p", str(self.project / "tiny.mp4")],
                       check=True, capture_output=True, timeout=30)
        gate.capture(self.record, self.project, "tiny.mp4", "real-samples", "real.json", ffmpeg, ffprobe)
        record = json.loads((self.project / "real.json").read_text())
        self.attach_reviews(record)  # Fictional receipts exercise plumbing, not semantic production approval.
        result = gate.check_record(record, self.project, "encoded", ffmpeg, ffprobe)
        self.assertTrue(result["valid"], result)
        frame = self.project / record["encoded"]["frames"]["poster-change"]["action"]["path"]
        frame.write_bytes(b"altered")
        self.assertFalse(gate.check_record(record, self.project, "encoded", ffmpeg, ffprobe)["valid"])


if __name__ == "__main__":
    unittest.main()
