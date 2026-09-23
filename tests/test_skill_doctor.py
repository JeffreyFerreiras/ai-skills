from __future__ import annotations

import importlib.util
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = REPOSITORY_ROOT / "skills" / "skill-doctor" / "scripts" / "skill_doctor.py"
SPEC = importlib.util.spec_from_file_location("skill_doctor", MODULE_PATH)
assert SPEC and SPEC.loader
skill_doctor = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = skill_doctor
SPEC.loader.exec_module(skill_doctor)
SYNC_SPEC = importlib.util.spec_from_file_location("sync_discovery", REPOSITORY_ROOT / "scripts" / "sync-discovery.py")
assert SYNC_SPEC and SYNC_SPEC.loader
sync_discovery = importlib.util.module_from_spec(SYNC_SPEC)
SYNC_SPEC.loader.exec_module(sync_discovery)


class SkillDoctorTests(unittest.TestCase):
    def create_skill(self, root: Path, description: str = "Validate a sample skill.") -> None:
        skill_dir = root / "skills" / "sample-skill"
        (skill_dir / "agents").mkdir(parents=True)
        (skill_dir / "SKILL.md").write_text(
            f"---\nname: sample-skill\ndescription: {description!r}\n---\n\n# Sample Skill\n\nRun the sample workflow.\n",
            encoding="utf-8",
        )
        (skill_dir / "agents" / "openai.yaml").write_text(
            "interface:\n"
            "  display_name: Sample Skill\n"
            "  short_description: Validate a sample skill\n"
            '  default_prompt: "Use $sample-skill to validate this sample."\n',
            encoding="utf-8",
        )
        cursor_skills = root / ".cursor" / "skills"
        shutil.copytree(root / "skills", cursor_skills, dirs_exist_ok=True)

    def test_valid_skill_has_no_errors(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            self.create_skill(root)
            audit = skill_doctor.audit_repository(root)
            self.assertEqual(0, audit.errors)

    def test_angle_bracket_in_description_is_an_error(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            self.create_skill(root, "Reach >=95 percent coverage.")
            audit = skill_doctor.audit_repository(root)
            self.assertIn("description-angle-bracket", {issue.code for issue in audit.issues})

    def test_missing_cursor_skills_discovery_is_an_error(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            self.create_skill(root)
            shutil.rmtree(root / ".cursor" / "skills")
            audit = skill_doctor.audit_repository(root)
            self.assertIn("missing-cursor-skills-discovery", {issue.code for issue in audit.issues})

    def test_cursor_skills_discovery_drift_is_an_error(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            self.create_skill(root)
            shutil.rmtree(root / ".cursor" / "skills")
            drifted = root / ".cursor" / "skills" / "other-skill"
            drifted.mkdir(parents=True)
            (drifted / "SKILL.md").write_text(
                "---\nname: other-skill\ndescription: Drifted skill.\n---\n\n# Other\n",
                encoding="utf-8",
            )
            audit = skill_doctor.audit_repository(root)
            self.assertIn("cursor-skills-discovery-drift", {issue.code for issue in audit.issues})

    def test_copied_skill_content_drift_is_detected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.create_skill(root)
            (root / ".cursor/skills/sample-skill/SKILL.md").write_text("stale", encoding="utf-8")
            audit = skill_doctor.audit_repository(root)
            self.assertIn("cursor-skills-content-drift", {issue.code for issue in audit.issues})

    def test_nested_reference_and_cycle(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "references").mkdir()
            (root / "references/guide.md").write_text("[self](guide.md) [missing](missing.md)", encoding="utf-8")
            issues = []
            skill_doctor.validate_resource_references(root, "[guide](references/guide.md)", issues)
            self.assertEqual(["missing-resource"], [issue.code for issue in issues])

    def test_missing_entrypoint_is_detected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.create_skill(root)
            (root / "skills/incomplete").mkdir()
            self.assertIn("missing-skill-entrypoint", {issue.code for issue in skill_doctor.audit_repository(root).issues})

    def test_text_placeholder_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.create_skill(root)
            discovery = root / ".cursor/skills"
            shutil.rmtree(discovery)
            discovery.write_text("../skills", encoding="utf-8")
            shutil.copytree(root / "skills", root / ".agents/skills")
            self.assertIn("invalid-cursor-skills-discovery", {issue.code for issue in skill_doctor.audit_repository(root).issues})

    def test_symlink_discovery_is_rejected_when_supported(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.create_skill(root)
            discovery = root / ".cursor/skills"
            shutil.rmtree(discovery)
            try:
                discovery.symlink_to(Path("..") / "skills", target_is_directory=True)
            except OSError as error:
                self.skipTest(f"Symlink creation unavailable: {error}")
            self.assertIn("invalid-cursor-skills-discovery", {issue.code for issue in skill_doctor.audit_repository(root).issues})

    def test_sync_discovery_copies_files_and_excludes_graph_runtime(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.create_skill(root)
            graph = root / "skills/software-engineering-graph"
            (graph / "policies").mkdir(parents=True)
            (graph / "state").mkdir()
            (graph / "SKILL.md").write_text("graph", encoding="utf-8")
            (graph / "policies/repo.json").write_text("{}", encoding="utf-8")
            (graph / "state/ledger.json").write_text("{}", encoding="utf-8")

            self.assertEqual([], sync_discovery.sync_discovery(root, check_only=False))
            self.assertTrue((root / ".cursor/skills/software-engineering-graph/SKILL.md").is_file())
            self.assertFalse((root / ".cursor/skills/software-engineering-graph/policies").exists())
            self.assertFalse((root / ".cursor/skills/software-engineering-graph/state").exists())
            self.assertEqual([], sync_discovery.sync_discovery(root, check_only=True))

            (root / "skills/sample-skill/SKILL.md").write_text("changed", encoding="utf-8")
            self.assertIn("changed: sample-skill/SKILL.md", sync_discovery.sync_discovery(root, check_only=True))
            self.assertEqual([], sync_discovery.sync_discovery(root, check_only=False))

    def test_sync_discovery_removes_only_previously_managed_stale_files(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.create_skill(root)
            source = root / "skills/sample-skill"
            mirror = root / ".cursor/skills/sample-skill"
            (source / "old.md").write_text("old", encoding="utf-8")
            self.assertEqual([], sync_discovery.sync_discovery(root, check_only=False))
            (mirror / "my-notes.md").write_text("keep", encoding="utf-8")
            runtime = root / ".cursor/skills/software-engineering-graph/policies"
            runtime.mkdir(parents=True)
            (runtime / "repo.json").write_text("{}", encoding="utf-8")

            (source / "old.md").rename(source / "new.md")
            self.assertEqual(
                ["missing: sample-skill/new.md", "extra: sample-skill/my-notes.md", "extra: sample-skill/old.md"],
                sync_discovery.sync_discovery(root, check_only=True),
            )
            self.assertEqual(["extra: sample-skill/my-notes.md"], sync_discovery.sync_discovery(root, check_only=False))
            self.assertFalse((mirror / "old.md").exists())
            self.assertEqual("old", (mirror / "new.md").read_text(encoding="utf-8"))
            self.assertEqual("keep", (mirror / "my-notes.md").read_text(encoding="utf-8"))
            self.assertTrue((runtime / "repo.json").is_file())
            self.assertEqual(["extra: sample-skill/my-notes.md"], sync_discovery.sync_discovery(root, check_only=True))

    def test_sync_discovery_handles_managed_file_directory_type_changes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.create_skill(root)
            source = root / "skills/sample-skill"
            mirror = root / ".cursor/skills/sample-skill"
            (source / "item").write_text("file", encoding="utf-8")
            sync_discovery.sync_discovery(root, check_only=False)

            (source / "item").unlink()
            (source / "item").mkdir()
            (source / "item/child.md").write_text("child", encoding="utf-8")
            self.assertEqual([], sync_discovery.sync_discovery(root, check_only=False))
            self.assertEqual("child", (mirror / "item/child.md").read_text(encoding="utf-8"))

            (source / "item/child.md").unlink()
            (source / "item").rmdir()
            (source / "item").write_text("replacement", encoding="utf-8")
            self.assertEqual([], sync_discovery.sync_discovery(root, check_only=False))
            self.assertEqual("replacement", (mirror / "item").read_text(encoding="utf-8"))

    def test_generated_dotnet_outputs_are_removed_from_managed_mirror(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.create_skill(root)
            source = root / "skills/sample-skill"
            mirror = root / ".cursor/skills/sample-skill"
            generated = ("obj/Debug/generated.py", "bin/Release/sample.dll")
            for relative in generated:
                path = source / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("generated source", encoding="utf-8")

            self.assertEqual([], sync_discovery.sync_discovery(root, check_only=False))
            manifest_path = root / ".cursor/sync-discovery-manifest.json"
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            self.assertFalse(any("/obj/" in name or "/bin/" in name for name in manifest["files"]))
            for relative in generated:
                stale = mirror / relative
                stale.parent.mkdir(parents=True, exist_ok=True)
                stale.write_text("old generated output", encoding="utf-8")
                manifest["files"].append("sample-skill/" + relative)
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

            self.assertEqual([], sync_discovery.sync_discovery(root, check_only=False))
            self.assertTrue(all(not (mirror / relative).exists() for relative in generated))
            self.assertFalse(any("/obj/" in name or "/bin/" in name for name in json.loads(manifest_path.read_text(encoding="utf-8"))["files"]))
            self.assertEqual(skill_doctor.tree_digest(source), skill_doctor.tree_digest(mirror))
            self.assertEqual([], sync_discovery.sync_discovery(root, check_only=True))

    def test_sync_discovery_rejects_manifest_traversal_without_deleting_data(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.create_skill(root)
            outside = root / "keep.txt"
            outside.write_text("keep", encoding="utf-8")
            manifest = root / ".cursor/sync-discovery-manifest.json"
            manifest.write_text('{"version": 1, "files": ["../../keep.txt"]}', encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "Invalid discovery manifest path"):
                sync_discovery.sync_discovery(root, check_only=False)
            self.assertEqual("keep", outside.read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
