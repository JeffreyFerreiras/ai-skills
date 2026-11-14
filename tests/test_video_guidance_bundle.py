from __future__ import annotations

import importlib.util
import hashlib
import json
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
            expected_resources = sum(len(resources) for resources in bundler.BUNDLES.values())
            self.assertEqual(expected_resources, len(bundler.bundle(root, check=True)))
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

    def test_video_installs_have_no_sibling_reference_dependencies(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            for skill, resources in bundler.BUNDLES.items():
                standalone = Path(directory) / skill
                shutil.copytree(ROOT / "skills" / skill, standalone)
                for document in standalone.rglob("*.md"):
                    for link in re.findall(r"\]\(([^)]+)\)", document.read_text(encoding="utf-8")):
                        if "://" not in link:
                            target = (document.parent / link.split("#")[0]).resolve()
                            self.assertTrue(target.is_relative_to(standalone.resolve()), link)
                            self.assertTrue(target.is_file(), link)
                if "scene_bridge.py" not in resources:
                    continue
                checker = standalone / "scripts/scene_bridge.py"
                result = subprocess.run([sys.executable, "-B", str(checker), "--help"], capture_output=True, text=True)
                self.assertEqual(0, result.returncode, result.stderr)
                result = subprocess.run([sys.executable, "-B", str(standalone / "scripts/cover_style.py"), "--help"], capture_output=True, text=True)
                self.assertEqual(0, result.returncode, result.stderr)
                project = Path(directory) / (skill + "-project")
                project.mkdir()
                data = "synthetic café".encode("utf-8")
                (project / "source.txt").write_bytes(data)
                prior = b'{"schema_version":1}'
                (project / "prior.json").write_bytes(prior)
                record = {"schema_version": 1, "production_id": "p1", "stage_id": "s1", "generation": 0,
                    "nodes": [{"id": "n1", "kind": "source", "scope_ids": ["s1"], "depends_on": [],
                               "artifacts": [{"path": "source.txt", "sha256": hashlib.sha256(data).hexdigest()}]}],
                    "gates": [{"id": "g1", "prerequisite_nodes": [], "check_ids": ["structure"], "consistency_coverage": ["source"],
                               "budget_checkpoint": {"path": "prior.json", "sha256": hashlib.sha256(prior).hexdigest()},
                               "required": [{"role": "technical", "reviewer_id": "r1", "scope_ids": ["s1"], "criterion_ids": ["C1"]}]}]}
                manifest = project / "context.json"
                manifest.write_text(json.dumps(record, ensure_ascii=False), encoding="utf-8")
                result = subprocess.run([sys.executable, "-B", str(standalone / "scripts/production_context.py"),
                    "check", str(manifest), "--project", str(project)], capture_output=True, text=True, encoding="utf-8")
                self.assertEqual(0, result.returncode, result.stderr + result.stdout)
                self.assertTrue(json.loads(result.stdout)["valid"])
                for name in bundler.CONTEXT_RESOURCES:
                    folder = "scripts" if name.endswith(".py") else "references"
                    self.assertEqual((ROOT / "shared/video-production" / name).read_bytes(),
                                     (standalone / folder / name).read_bytes())


if __name__ == "__main__":
    unittest.main()
