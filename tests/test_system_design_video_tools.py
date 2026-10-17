from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
import unittest
import wave
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SKILL = ROOT / "skills/system-design-video"
sys.path.insert(0, str(SKILL / "scripts"))
try:
    import video_project as project_tools
    import reference_video
finally:
    sys.path.pop(0)


class VideoProjectTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.project = self.root / "project"

    def test_starter_initializes_without_installing_or_overwriting(self):
        project_tools.initialize(self.project)
        self.assertTrue((self.project / "src/index.jsx").exists())
        self.assertFalse((self.project / "node_modules").exists())
        with self.assertRaises(ValueError):
            project_tools.initialize(self.project)

    def test_snapshot_receipts_detect_changes_and_preserve_previous_checkpoint(self):
        self.project.mkdir()
        artifact = self.project / "script.txt"
        artifact.write_text("original", encoding="utf-8")
        manifest = project_tools.snapshot(self.project, ["script.txt"], {"stage": "script"})
        state = self.root / "state.json"
        entry = {"kind": "validation", "stage": "script", "identity": "verifier", "exit": 0}
        first = project_tools.record(state, manifest, entry)
        second = project_tools.record(state, manifest, entry)
        self.assertEqual(2, second["sequence"])
        self.assertEqual(first, project_tools.read_json(state.with_suffix(".json.previous")))
        artifact.write_text("changed", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "stale"):
            project_tools.record(state, manifest, entry)
        self.assertEqual(second, project_tools.read_json(state))

    def test_deleted_files_and_tampering_are_accounted_for(self):
        self.project.mkdir()
        artifact = self.project / "source.txt"
        artifact.write_text("source", encoding="utf-8")
        baseline = project_tools.snapshot(self.project, ["source.txt"], {})
        artifact.unlink()
        manifest = project_tools.snapshot(self.project, ["source.txt"], {}, baseline)
        self.assertEqual("deleted", manifest["entries"][0]["kind"])
        project_tools.verify_manifest(manifest)
        manifest["plan"] = {"forged": True}
        with self.assertRaisesRegex(ValueError, "integrity"):
            project_tools.verify_manifest(manifest)

    def test_recovery_state_cannot_enter_candidate_scope(self):
        self.project.mkdir()
        (self.project / "assets").mkdir()
        manifest = project_tools.snapshot(self.project, ["assets"], {})
        with self.assertRaisesRegex(ValueError, "outside candidate"):
            project_tools.record(self.project / "assets/state.json", manifest,
                                 {"kind": "checkpoint", "stage": "sample", "identity": "supervisor"})
        with self.assertRaises(ValueError):
            project_tools.snapshot(self.project, ["../outside"], {})

    def test_preflight_does_not_claim_unproven_review_or_listening(self):
        project_tools.initialize(self.project)
        spec = project_tools.read_json(self.project / "video-spec.json")
        with patch.object(shutil, "which", return_value=None):
            report = project_tools.preflight(self.project, spec, {"listening": {"status": "available"}})
        self.assertFalse(report["ready_to_attempt"])
        self.assertIn("independent_review", report["unresolved"])
        self.assertEqual("unknown", report["declared"]["listening"]["status"])

    def test_preview_frames_cover_travel_completion_and_scene_boundaries(self):
        spec = project_tools.read_json(SKILL / "assets/reference-video.json")
        frames = project_tools.preview_frames(spec)
        for frame in (0, 2, 3, 4, 12, 20, 21, 22, 29, 30, 31, 59):
            self.assertIn(frame, frames)
        self.assertTrue(all(0 <= frame < 60 for frame in frames))

    def test_loudness_accepts_trailing_ffmpeg_summary_but_not_silence(self):
        output = 'Parsed_loudnorm\n{"input_i":"-20.26","input_tp":"-3.31"}\n[out#0/null] muxing overhead: unknown'
        self.assertEqual("-3.31", project_tools.parse_loudness(output)["input_tp"])
        with self.assertRaises(ValueError):
            project_tools.parse_loudness('{"input_i":"-inf","input_tp":"-inf"}')

    def test_fixture_retimes_long_speech_without_changing_audio(self):
        voices = self.root / "voices"
        voices.mkdir()
        for name in ("send", "receive"):
            with wave.open(str(voices / f"{name}.wav"), "wb") as clip:
                clip.setparams((1, 2, 8000, 0, "NONE", "not compressed"))
                clip.writeframes(b"\x01\x00" * 16000)
        report = reference_video.prepare(self.project, voices)
        self.assertTrue(any("beyond the scene" in error for error in report["before_retiming_errors"]))
        self.assertEqual([], report["after_retiming_errors"])
        self.assertEqual(5, report["duration_seconds"])
        spec = project_tools.read_json(self.project / "video-spec.json")
        self.assertEqual(2.6, spec["events"][1]["at"])
        for name in ("send", "receive"):
            self.assertEqual(project_tools.file_hash(voices / f"{name}.wav"),
                             project_tools.file_hash(self.project / f"public/audio/{name}.wav"))
        self.assertIn("00:00:02,500 --> 00:00:04,500", (self.project / "subtitles.srt").read_text())

    @unittest.skipUnless(shutil.which("node"), "Node is needed for deterministic renderer-state tests")
    def test_renderer_event_state_contract(self):
        result = subprocess.run([shutil.which("node"), "--test", str(SKILL / "assets/remotion-starter/src/timeline.test.mjs")],
                                capture_output=True, text=True)
        self.assertEqual(0, result.returncode, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
