import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SKILL_ROOT = Path(__file__).resolve().parents[1]
SCRIPT = SKILL_ROOT / "scripts" / "search_review_graph.py"
MANIFEST = SKILL_ROOT / "references" / "review-graph.manifest.json"


def run_search(*arguments: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(SCRIPT), *arguments],
        check=False,
        capture_output=True,
        text=True,
    )


class ReviewGraphSearchTests(unittest.TestCase):
    def test_manifest_validates(self) -> None:
        result = run_search("--validate")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Valid review graph", result.stdout)

    def test_behavior_variation_returns_strategy(self) -> None:
        result = run_search(
            "scattered conditional branches choose a payment algorithm that will vary",
            "--json",
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        candidate_ids = [item["id"] for item in payload["candidates"]]
        self.assertIn("pattern.strategy", candidate_ids)

    def test_incompatible_boundary_returns_adapter(self) -> None:
        result = run_search(
            "third party API has an incompatible interface leaking through the boundary",
            "--json",
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        candidate_ids = [item["id"] for item in payload["candidates"]]
        self.assertIn("pattern.adapter", candidate_ids)

    def test_weak_generic_matches_do_not_surface_unrelated_patterns(self) -> None:
        result = run_search(
            "new notification logic directly calls several unrelated consumers "
            "and more consumers will be added",
            "--json",
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        candidate_ids = [item["id"] for item in payload["candidates"]]
        self.assertEqual(candidate_ids, ["pattern.observer"])

    def test_node_budget_is_respected(self) -> None:
        result = run_search(
            "construction behavior state event interface dependency",
            "--max-nodes",
            "4",
            "--json",
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertLessEqual(len(payload["traversal"]), 4)

    def test_unknown_edge_target_fails_validation(self) -> None:
        raw = json.loads(MANIFEST.read_text(encoding="utf-8"))
        raw["edges"][0]["to"] = "pattern.missing"
        with tempfile.TemporaryDirectory() as temporary:
            invalid_manifest = Path(temporary) / "invalid.json"
            invalid_manifest.write_text(json.dumps(raw), encoding="utf-8")
            result = run_search("--manifest", str(invalid_manifest), "--validate")

        self.assertEqual(result.returncode, 2)
        self.assertIn("unknown node", result.stderr)


if __name__ == "__main__":
    unittest.main()
