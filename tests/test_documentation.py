from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("build_docs", REPOSITORY_ROOT / "scripts/build-docs.py")
assert SPEC and SPEC.loader
build_docs = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(build_docs)


class DocumentationTests(unittest.TestCase):
    def create_skill(self, root: Path, name: str, summary: str = "Test a capability") -> None:
        skill = root / "skills" / name
        (skill / "agents").mkdir(parents=True)
        (skill / "SKILL.md").write_text(
            f"---\nname: {name}\ndescription: Test when asked.\n---\n# Example\n",
            encoding="utf-8",
        )
        (skill / "agents/openai.yaml").write_text(
            f"interface:\n  short_description: {summary!r}\n", encoding="utf-8"
        )

    def test_catalog_discovers_new_skills_and_metadata_changes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.create_skill(root, "zulu")
            original = build_docs.catalog(root)
            self.create_skill(root, "alpha", "Check pipes | and brackets [carefully]")
            updated = build_docs.catalog(root)
            self.assertNotEqual(original, updated)
            self.assertLess(updated.index("[alpha]"), updated.index("[zulu]"))
            self.assertIn("skills/alpha/SKILL.md", updated)
            self.assertIn(r"\|", updated)
            (root / "skills/zulu/agents/openai.yaml").write_text(
                "interface:\n  short_description: Updated purpose\n", encoding="utf-8"
            )
            self.assertIn("Updated purpose", build_docs.catalog(root))
            self.assertNotEqual(updated, build_docs.catalog(root))

    def test_incomplete_skill_cannot_silently_disappear(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "skills/incomplete").mkdir(parents=True)
            with self.assertRaises(ValueError):
                build_docs.catalog(root)

    def test_generation_preserves_handwritten_content(self) -> None:
        source = f"Intro\n{build_docs.START}\nold\n{build_docs.END}\nFooter\n"
        expected = f"Intro\n{build_docs.START}\nnew\n{build_docs.END}\nFooter\n"
        self.assertEqual(expected, build_docs.replace_catalog(source, "new"))
        with self.assertRaises(ValueError):
            build_docs.replace_catalog("Missing delimiters", "new")

    def test_links_check_images_fragments_and_ignore_code_examples(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            document = root / "README.md"
            (root / "guide.md").write_text("# Working example\n", encoding="utf-8")
            document.write_text(
                '[guide](guide.md#working-example)\n[bad](guide.md#missing)\n'
                '<picture><source srcset="missing.png"><img src="also-missing.png" alt="Cycle"></picture>\n'
                '```markdown\n[example](not-a-file.md)\n```\n', encoding="utf-8"
            )
            errors = build_docs.check_links(root, [document])
            self.assertEqual(3, len(errors))
            self.assertTrue(any("#missing" in error for error in errors))
            self.assertFalse(any("not-a-file" in error for error in errors))

    def test_repository_catalog_is_current(self) -> None:
        readme = (REPOSITORY_ROOT / "README.md").read_text(encoding="utf-8")
        self.assertEqual(readme, build_docs.replace_catalog(readme, build_docs.catalog(REPOSITORY_ROOT)))

    def test_repository_documentation_links_resolve(self) -> None:
        self.assertEqual([], build_docs.check_links(REPOSITORY_ROOT, build_docs.documents(REPOSITORY_ROOT)))


if __name__ == "__main__":
    unittest.main()
