from __future__ import annotations

import importlib.util
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
GENERATOR = REPOSITORY_ROOT / "scripts/bundle-design-graph.py"
SPEC = importlib.util.spec_from_file_location("bundle_design_graph", GENERATOR)
assert SPEC and SPEC.loader
bundler = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bundler)


class DesignGraphBundleTests(unittest.TestCase):
    def create_repository(self, root: Path) -> None:
        for resource in bundler.RESOURCES:
            target = root / "shared/design-graph" / resource
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(REPOSITORY_ROOT / "shared/design-graph" / resource, target)
        (root / "scripts").mkdir()
        shutil.copyfile(GENERATOR, root / "scripts/bundle-design-graph.py")

    def snapshot(self, root: Path) -> dict[str, tuple[bytes | None, int]]:
        return {
            path.relative_to(root).as_posix(): (
                path.read_bytes() if path.is_file() else None,
                path.stat().st_mtime_ns,
            )
            for path in root.rglob("*")
        }

    def run_generator(self, root: Path, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, "-I", str(root / "scripts/bundle-design-graph.py"), *args],
            cwd=root,
            text=True,
            capture_output=True,
            check=False,
        )

    def test_repository_bundles_match_canonical_bytes(self) -> None:
        self.assertEqual([], bundler.bundle(REPOSITORY_ROOT, check=True))
        for resource in bundler.RESOURCES:
            source = (REPOSITORY_ROOT / "shared/design-graph" / resource).read_bytes()
            for skill in bundler.SKILLS:
                self.assertEqual(source, (REPOSITORY_ROOT / "skills" / skill / resource).read_bytes())

    def test_generation_is_identical_and_idempotent(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.create_repository(root)
            result = self.run_generator(root)
            self.assertEqual(0, result.returncode, result.stderr)
            for resource in bundler.RESOURCES:
                source = (root / "shared/design-graph" / resource).read_bytes()
                for skill in bundler.SKILLS:
                    self.assertEqual(source, (root / "skills" / skill / resource).read_bytes())
            before = self.snapshot(root)
            result = self.run_generator(root)
            self.assertEqual(0, result.returncode, result.stderr)
            self.assertEqual(before, self.snapshot(root))

    def test_check_detects_missing_without_creating_directories(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.create_repository(root)
            before = self.snapshot(root)
            result = self.run_generator(root, "--check")
            self.assertEqual(1, result.returncode)
            for skill in bundler.SKILLS:
                for resource in bundler.RESOURCES:
                    self.assertIn(f"missing: skills/{skill}/{resource.as_posix()}", result.stderr)
            self.assertEqual(before, self.snapshot(root))

    def test_check_detects_drift_and_deleted_bundle_without_writing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.create_repository(root)
            bundler.bundle(root)
            stale = root / "skills/clean-code" / bundler.RESOURCES[0]
            missing = root / "skills/clean-code-review" / bundler.RESOURCES[1]
            stale.write_bytes(b"stale resource\n")
            missing.unlink()
            before = self.snapshot(root)
            result = self.run_generator(root, "--check")
            self.assertEqual(1, result.returncode)
            self.assertIn("stale: skills/clean-code/scripts/search_review_graph.py", result.stderr)
            self.assertIn("missing: skills/clean-code-review/references/review-graph.manifest.json", result.stderr)
            self.assertEqual(before, self.snapshot(root))
            self.assertEqual(0, self.run_generator(root).returncode)
            repaired = self.snapshot(root)
            result = self.run_generator(root, "--check")
            self.assertEqual(0, result.returncode, result.stderr)
            self.assertEqual(repaired, self.snapshot(root))

    def test_missing_source_fails_before_any_bundle_write(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.create_repository(root)
            (root / "shared/design-graph" / bundler.RESOURCES[1]).unlink()
            before = self.snapshot(root)
            for args in ((), ("--check",)):
                result = self.run_generator(root, *args)
                self.assertEqual(1, result.returncode)
                self.assertIn("error:", result.stderr)
                self.assertEqual(before, self.snapshot(root))

    def test_each_skill_searches_from_standalone_profile_installs(self) -> None:
        for profile in (".agents", ".claude", ".cursor"):
            for skill in bundler.SKILLS:
                with self.subTest(profile=profile, skill=skill), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    installed = root / profile / "skills" / skill
                    shutil.copytree(
                        REPOSITORY_ROOT / "skills" / skill,
                        installed,
                        ignore=shutil.ignore_patterns("__pycache__", "*.pyc"),
                    )
                    outside = root / "outside"
                    outside.mkdir()
                    self.assertEqual([skill], [path.name for path in installed.parent.iterdir()])
                    self.assertFalse((root / "shared").exists())
                    script = installed / "scripts/search_review_graph.py"
                    for query, expected_field, expected_id in (
                        ("algorithm variation", "candidates", "pattern.strategy"),
                        ("DIP", "principles", "principle.dependency-inversion"),
                    ):
                        result = subprocess.run(
                            [sys.executable, "-I", str(script), query, "--depth", "1", "--max-nodes", "8", "--json"],
                            cwd=outside,
                            capture_output=True,
                            text=True,
                            check=False,
                        )
                        self.assertEqual(0, result.returncode, result.stderr)
                        payload = json.loads(result.stdout)
                        self.assertEqual(query, payload["query"])
                        self.assertIn(expected_id, [node["id"] for node in payload[expected_field]])
                        self.assertLessEqual(len(payload["traversal"]), 8)
                        self.assertTrue(all(node["depth"] <= 1 for node in payload["traversal"]))
                        if expected_field == "candidates":
                            self.assertTrue(all(node[field] for node in payload[expected_field] for field in ("applicability", "tradeoffs", "avoid_when")))
                        else:
                            self.assertTrue(all(node[field] for node in payload[expected_field] for field in ("cues", "guardrails")))


if __name__ == "__main__":
    unittest.main()
