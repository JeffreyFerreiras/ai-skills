from __future__ import annotations

import importlib.util
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("bundle_video", ROOT / "scripts/bundle-video-guidance.py")
assert SPEC and SPEC.loader
bundler = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bundler)


class VideoGuidanceBundleTests(unittest.TestCase):
    def test_repository_bundles_are_current(self) -> None:
        self.assertEqual([], bundler.bundle(ROOT, check=True))

    def test_check_reports_drift_without_repair_and_generation_preserves_extras(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            canonical = root / "shared/video-production"
            canonical.mkdir(parents=True)
            for name in {name for resources in bundler.BUNDLES.values() for name in resources}:
                (canonical / name).parent.mkdir(parents=True, exist_ok=True)
                (canonical / name).write_text(f"# {name}\n", encoding="utf-8")
            self.assertEqual(28, len(bundler.bundle(root, check=True)))
            self.assertFalse((root / "skills").exists())
            bundler.bundle(root)
            target = root / "skills/notebooklm-short-video/references/scene-direction.md"
            extra = target.parent / "custom-notes.txt"
            extra.write_text("Keep me", encoding="utf-8")
            target.write_text("stale", encoding="utf-8")
            self.assertEqual(1, len(bundler.bundle(root, check=True)))
            self.assertEqual("stale", target.read_text(encoding="utf-8"))
            bundler.bundle(root)
            self.assertEqual((canonical / target.name).read_bytes(), target.read_bytes())
            self.assertEqual("Keep me", extra.read_text(encoding="utf-8"))
            timestamp = target.stat().st_mtime_ns
            bundler.bundle(root)
            self.assertEqual(timestamp, target.stat().st_mtime_ns)

    def test_missing_source_does_not_partly_write_bundles(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            canonical = root / "shared/video-production"
            canonical.mkdir(parents=True)
            (canonical / "scene-direction.md").write_text("direction", encoding="utf-8")
            with self.assertRaises(FileNotFoundError):
                bundler.bundle(root)
            self.assertFalse((root / "skills").exists())

    def test_notebooklm_installs_have_no_sibling_reference_dependencies(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            for skill in ("notebooklm-cinematic-video", "notebooklm-short-video"):
                standalone = Path(directory) / skill
                shutil.copytree(ROOT / "skills" / skill, standalone)
                checker = standalone / "scripts/scene_bridge.py"
                result = subprocess.run([sys.executable, "-B", str(checker), "--help"], capture_output=True, text=True)
                self.assertEqual(0, result.returncode, result.stderr)
                result = subprocess.run([sys.executable, "-B", str(standalone / "scripts/cover_style.py"), "--help"], capture_output=True, text=True)
                self.assertEqual(0, result.returncode, result.stderr)
                for document in standalone.rglob("*.md"):
                    for link in re.findall(r"\]\(([^)]+)\)", document.read_text(encoding="utf-8")):
                        if "://" not in link:
                            self.assertTrue((document.parent / link.split("#")[0]).is_file(), link)


if __name__ == "__main__":
    unittest.main()
