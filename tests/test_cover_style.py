from __future__ import annotations

import copy
import hashlib
import json
import os
import shutil
import struct
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "shared/video-production"))
import cover_style as gate

REFERENCES = ROOT / "shared/video-production/cover-references"


class CoverStyleTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.project = Path(self.temporary.name).resolve()
        self.cover = self.project / "cover.png"
        self.cover.write_bytes(b"synthetic cover bytes, not production QA")
        self.pixels = b"\x89PNG\r\n\x1a\nsynthetic thumbnail"
        self.decoder = patch.object(gate, "thumbnail_bytes", return_value=self.pixels)
        self.decoder.start()
        self.addCleanup(self.decoder.stop)
        self.record = gate.capture("cover.png", "evidence/thumb.png", "cover-record.json", self.project,
                                   "fixture-only", "fixture-writer")

    def approve_fixture(self):
        # Fictional receipts test mechanics only, never actual style acceptance.
        assets = gate.reference_assets(REFERENCES)
        self.record["reference_inspections"] = [
            {"reference_id": key, "sha256": value["sha256"], "viewer_id": "fixture-writer",
             "pixels_viewed": True, "observations": "Fictional unit-test inspection"}
            for key, value in assets.items()]
        self.record["reviews"] = [{"role": "cover_visual", "reviewer_id": "fixture-reviewer",
            "decision": "APPROVE", "candidate_digest": gate.candidate_digest(self.record),
            "inspected_images": {key: self.record[key]["sha256"] for key in ("cover", "thumbnail")},
            "reference_hashes": {key: value["sha256"] for key, value in assets.items()},
            "observations": {**dict.fromkeys(gate.CHECKS, True), "observed_hook": "Fixture only",
                             "observed_illustration": "Fixture only"}}]

    def test_capture_is_pending_and_preserves_existing_evidence(self):
        self.assertEqual([], self.record["reference_inspections"])
        self.assertEqual([], self.record["reviews"])
        with self.assertRaisesRegex(ValueError, "inspect both"):
            gate.check(self.record, self.project, REFERENCES)
        original = (self.project / "cover-record.json").read_bytes()
        with self.assertRaisesRegex(ValueError, "never overwrites"):
            gate.capture("cover.png", "evidence/thumb.png", "cover-record.json", self.project, "e", "c")
        self.assertEqual(original, (self.project / "cover-record.json").read_bytes())

    def test_complete_fictional_fixture_and_generic_approval_rejection(self):
        self.approve_fixture()
        gate.check(self.record, self.project, REFERENCES)
        self.record["reviews"][0]["observations"] = {}
        with self.assertRaisesRegex(ValueError, "rough_chalk_lettering"):
            gate.check(self.record, self.project, REFERENCES)

    def test_negative_style_findings_cannot_pass_even_with_approve_label(self):
        self.approve_fixture()
        for check in ("rough_chalk_lettering", "not_lesson_slide", "thumbnail_readable", "minimal_labels"):
            with self.subTest(check=check):
                record = copy.deepcopy(self.record)
                record["reviews"][0]["observations"][check] = False
                with self.assertRaisesRegex(ValueError, check):
                    gate.check(record, self.project, REFERENCES)

    def test_missing_stale_or_description_only_reference_inspection_fails(self):
        self.approve_fixture()
        for key, value in (("sha256", "0" * 64), ("pixels_viewed", False), ("viewer_id", "someone-else")):
            record = copy.deepcopy(self.record)
            record["reference_inspections"][0][key] = value
            with self.assertRaises(ValueError):
                gate.check(record, self.project, REFERENCES)
        self.record["reference_inspections"].pop()
        with self.assertRaisesRegex(ValueError, "inspect both"):
            gate.check(self.record, self.project, REFERENCES)

    def test_self_review_stale_candidate_and_incomplete_viewing_fail(self):
        self.approve_fixture()
        for key, value in (("reviewer_id", "fixture-writer"), ("candidate_digest", "0" * 64),
                           ("inspected_images", {}), ("reference_hashes", {})):
            record = copy.deepcopy(self.record)
            record["reviews"][0][key] = value
            with self.assertRaises(ValueError):
                gate.check(record, self.project, REFERENCES)

    def test_stale_cover_thumbnail_and_unrelated_thumbnail_fail(self):
        self.approve_fixture()
        self.cover.write_bytes(b"new cover")
        with self.assertRaisesRegex(ValueError, "Stale artifact"):
            gate.check(self.record, self.project, REFERENCES)
        self.record["cover"]["sha256"] = gate.file_hash(self.cover)
        (self.project / "evidence/thumb.png").write_bytes(b"different thumbnail")
        with self.assertRaisesRegex(ValueError, "Stale artifact"):
            gate.check(self.record, self.project, REFERENCES)
        self.record["thumbnail"]["sha256"] = gate.file_hash(self.project / "evidence/thumb.png")
        with self.assertRaisesRegex(ValueError, "not the current cover"):
            gate.check(self.record, self.project, REFERENCES)

    def test_original_reference_hashes_sizes_and_pixel_dimensions(self):
        assets = gate.reference_assets(REFERENCES)
        for reference in assets.values():
            pixels = (REFERENCES / reference["path"]).read_bytes()
            self.assertEqual((941, 1672), struct.unpack(">II", pixels[16:24]))
            self.assertEqual((reference["width"], reference["height"]), struct.unpack(">II", pixels[16:24]))
        copied = self.project / "references"
        shutil.copytree(REFERENCES, copied)
        (copied / "safe-rollbacks-tiktok-cover.png").write_bytes(b"substitute")
        with self.assertRaisesRegex(ValueError, "Stale artifact"):
            gate.reference_assets(copied)

    @unittest.skipUnless(os.environ.get("VIDEO_FFMPEG"), "Configured FFmpeg required")
    def test_real_thumbnail_from_original_bytes_is_bound_to_current_cover(self):
        self.decoder.stop()
        shutil.copyfile(REFERENCES / "logical-clocks-tiktok-cover.png", self.project / "original.png")
        record = gate.capture("original.png", "real/thumb.png", "real/record.json", self.project,
                              "provenance-fixture", "fixture-writer", os.environ["VIDEO_FFMPEG"])
        pixels = (self.project / "real/thumb.png").read_bytes()
        width, height = struct.unpack(">II", pixels[16:24])
        self.assertEqual(240, width)
        self.assertAlmostEqual(941 / 1672, width / height, places=2)
        self.assertEqual(record["thumbnail"]["sha256"], hashlib.sha256(pixels).hexdigest())
        self.assertEqual(pixels, gate.thumbnail_bytes(self.project / "original.png", os.environ["VIDEO_FFMPEG"]))
        self.assertEqual([], record["reviews"])


if __name__ == "__main__":
    unittest.main()
