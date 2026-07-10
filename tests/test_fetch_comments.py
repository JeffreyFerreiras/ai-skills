from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = REPOSITORY_ROOT / "skills" / "gh-address-comments" / "scripts" / "fetch_comments.py"
SPEC = importlib.util.spec_from_file_location("fetch_comments", MODULE_PATH)
assert SPEC and SPEC.loader
fetch_comments = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fetch_comments)


class FetchCommentsTests(unittest.TestCase):
    def test_parse_pr_url_returns_base_repository(self) -> None:
        result = fetch_comments.parse_pr_url("https://github.com/base-owner/base-repo/pull/42", expected_number=42)
        self.assertEqual(("base-owner", "base-repo", 42), result)

    def test_parse_pr_url_rejects_number_mismatch(self) -> None:
        with self.assertRaisesRegex(ValueError, "does not match"):
            fetch_comments.parse_pr_url("https://github.com/owner/repo/pull/42", expected_number=41)


if __name__ == "__main__":
    unittest.main()
