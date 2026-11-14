"""Contracts for the Jeffrit chalk TikTok series guidance and bundled example."""

from __future__ import annotations

import hashlib
import json
import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SKILL = ROOT / "skills/system-design-video"
SERIES = SKILL / "references/chalk-tiktok-series.md"
BRAND = SKILL / "assets/brand/jeffrit"
EXAMPLE = SKILL / "references/examples/transactional-outbox"
PIPELINE = SKILL / "scripts/reference/transactional-outbox"

REQUIRED_SERIES_PHRASES = (
    "under 2 minutes",
    "-14 LUFS",
    "-2 dBTP",
    "0.35 s to 0.45 s",
    "0.68 s",
    "af_bella",
    "full-precision",
    "int8",
    "Chatterbox",
    "sample 1",
    "am_fenrir",
    "speed 1.20",
    "171",
    "Entity Framework Core",
    "sequel",
    "polls",
    "pulls",
    "at least once",
    "at least one's",
    "360x640",
    "340",
    "y 240 to 1680",
    "y 1670",
    "240 px",
    "230 Hz",
    "300 Hz",
    "2.9 s",
    "17, 25, 29",
    "QA-REPORT.md",
    "SHA256SUMS",
    "Video Janitor",
    "directly to the vault's `main` branch",
)

CASE_IDS = (
    "system-design-video-chalk-tiktok-workflow",
    "system-design-video-chalk-tiktok-narration-qa",
    "system-design-video-chalk-tiktok-not-universal",
)


class ChalkTikTokSeriesTests(unittest.TestCase):
    def test_skill_routes_the_series_without_replacing_other_defaults(self) -> None:
        skill = (SKILL / "SKILL.md").read_text(encoding="utf-8")
        self.assertIn("references/chalk-tiktok-series.md", skill)
        self.assertIn("references/diagram-qa.md", skill)
        self.assertIn("capped under 2 minutes", skill)
        self.assertIn("one TTS\ntake per sentence", skill)
        self.assertIn("af_heart or af_bella", skill)
        self.assertIn("-18 LUFS", (SKILL / "references/audio-and-delivery.md").read_text(encoding="utf-8"))

    def test_series_reference_keeps_the_decided_numbers(self) -> None:
        text = SERIES.read_text(encoding="utf-8")
        for phrase in REQUIRED_SERIES_PHRASES:
            self.assertIn(phrase, text, phrase)
        self.assertIn("do not ask for a style", text.lower())
        self.assertIn("Jeffrey uploads to TikTok himself", text)

    def test_behavioral_cases_cover_the_series_and_its_boundary(self) -> None:
        cases = json.loads((ROOT / "tests/behavioral/cases.json").read_text(encoding="utf-8"))
        found = {case["id"]: case for case in cases if case["id"] in CASE_IDS}
        self.assertEqual(set(CASE_IDS), set(found))
        for case in found.values():
            self.assertEqual("system-design-video", case["skill"])
            self.assertTrue(case["prompt"].strip())
            self.assertTrue(case["expected"].strip())
        self.assertIn("Do not upload", found["system-design-video-chalk-tiktok-workflow"]["expected"])
        self.assertIn("-14 LUFS", found["system-design-video-chalk-tiktok-narration-qa"]["expected"])
        self.assertIn("-18 LUFS", found["system-design-video-chalk-tiktok-not-universal"]["expected"])
        self.assertIn("Jeffrit end card", found["system-design-video-chalk-tiktok-not-universal"]["expected"])

    def test_brand_manifest_matches_shipped_bytes(self) -> None:
        manifest = (BRAND / "sha256-manifest.txt").read_text(encoding="utf-8").splitlines()
        listed = {}
        for line in manifest:
            digest, rel = line.split("  ", 1)
            listed[rel] = digest
            path = BRAND / rel.removeprefix("./")
            self.assertTrue(path.is_file(), rel)
            self.assertEqual(digest, hashlib.sha256(path.read_bytes()).hexdigest(), rel)
        shipped = {
            "./" + path.relative_to(BRAND).as_posix()
            for path in BRAND.rglob("*")
            if path.is_file() and path.name != "sha256-manifest.txt"
        }
        self.assertEqual(shipped, set(listed))
        for name in (
            "jeffrit-bust.png",
            "jeffrit-badge.png",
            "jeffrit-logo.png",
            "jeffrit-wordmark.png",
            "jeffrit-endcard-preview.png",
            "jeffrit-endcard.mp4",
            "endcard.py",
            "build_assets.py",
            "src/jeffrey-meme-original.png",
        ):
            self.assertIn("./" + name, listed)
        endcard = (BRAND / "endcard.py").read_text(encoding="utf-8")
        self.assertIn("LEN=2.9", endcard)
        self.assertIn("T_IN=0.60", endcard)
        self.assertIn("RING=(0.30,0.95)", endcard)
        self.assertIn("POP=(0.60,1.00)", endcard)
        self.assertIn("WORD=(1.02,1.95)", endcard)
        self.assertIn("GLINT=2.20", endcard)
        self.assertNotIn("/workspace/", endcard)

    def test_outbox_example_and_pipeline_are_present_and_clean(self) -> None:
        narration = (EXAMPLE / "canonical-narration.txt").read_text(encoding="utf-8")
        self.assertIn("Entity Framework Core", narration)
        self.assertNotIn("EF Core", narration)
        self.assertNotRegex(narration, r"\bSQL\b")
        caption = (EXAMPLE / "tiktok-caption.txt").read_text(encoding="utf-8").strip()
        self.assertNotIn("Caption:", caption)
        hashtags = re.findall(r"#\w+", caption)
        self.assertGreaterEqual(len(hashtags), 3)
        self.assertTrue(all(tag.lower() == tag for tag in hashtags))
        for card in (EXAMPLE / "code").glob("*.cs"):
            for line in card.read_text(encoding="utf-8").splitlines():
                self.assertLessEqual(len(line), 34, f"{card.name}: {line}")
        self.assertTrue((EXAMPLE / "transactional-outbox-tiktok-cover-1080x1920-v2.png").is_file())
        self.assertTrue((EXAMPLE / "QA-REPORT.md").is_file())
        for name in ("render.py", "part2.py", "timeline.py", "chalkkit.py", "build_audio.py",
                     "master.py", "measure_pauses.py", "asr_mp4.py", "enc_qa.py", "events.json"):
            self.assertTrue((PIPELINE / name).is_file(), name)
        self.assertIn("CHALK_SOUND_HELPERS", (PIPELINE / "build_audio.py").read_text(encoding="utf-8"))
        self.assertIn("-14.0", (PIPELINE / "master.py").read_text(encoding="utf-8"))

    def test_new_guidance_has_no_dashes_or_private_paths(self) -> None:
        roots = (
            SERIES,
            SKILL / "references/diagram-qa.md",
            EXAMPLE,
            PIPELINE,
            BRAND,
            SKILL / "assets/episode-tracker.csv",
        )
        needles = ("/workspace/", "Z:\\", "C:\\Users", "C:\\dev", "\u2014", "\u2013")
        for root in roots:
            paths = [root] if root.is_file() else [path for path in root.rglob("*") if path.is_file()]
            for path in paths:
                if path.suffix.lower() not in {".md", ".py", ".txt", ".cs", ".json", ".csv"}:
                    continue
                text = path.read_text(encoding="utf-8")
                for needle in needles:
                    self.assertNotIn(needle, text, f"{needle} in {path.relative_to(ROOT)}")
                self.assertNotIn("__pycache__", str(path))


if __name__ == "__main__":
    unittest.main()
