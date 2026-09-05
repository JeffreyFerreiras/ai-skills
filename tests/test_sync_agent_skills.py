from __future__ import annotations

import importlib.util
import os
import subprocess
import tempfile
import unittest
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = REPOSITORY_ROOT / "skills" / "sync-agent-skills" / "scripts" / "sync_agent_skills.py"
SPEC = importlib.util.spec_from_file_location("sync_agent_skills", MODULE_PATH)
assert SPEC and SPEC.loader
sync_agent_skills = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(sync_agent_skills)


class SyncAgentSkillsTests(unittest.TestCase):
    def test_rejects_escaped_target_names_before_writes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.write_text("keep", encoding="utf-8")
            for name in ("../outside", "..\\outside", "/absolute", "C:\\absolute", "C:relative", "", ".", "..", ".. ", "name.", "NUL", "nested/name"):
                with self.subTest(name=name), self.assertRaises(ValueError):
                    sync_agent_skills.copy_source(source, root / "targets", name, True, True)
            self.assertFalse((root / "targets").exists())
            self.assertEqual("keep", source.read_text(encoding="utf-8"))

    def test_rejects_overlapping_trees(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            child = source / "child"
            child.mkdir()
            with self.assertRaisesRegex(ValueError, "overlap"):
                sync_agent_skills.copy_source(source, source, "nested", True, True)
            with self.assertRaisesRegex(ValueError, "overlap"):
                sync_agent_skills.copy_source(child, root, "source", True, True)
            self.assertTrue(child.exists())

    def test_external_skill_is_never_mirrored_over_installation(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "master/graph"
            source.mkdir(parents=True)
            (source / "SKILL.md").write_text("stub", encoding="utf-8")
            (source / "external-source.json").write_text('{}', encoding="utf-8")
            installed = root / "target/graph"
            installed.mkdir(parents=True)
            (installed / "SKILL.md").write_text("full implementation", encoding="utf-8")
            result = sync_agent_skills.sync_skills_from_master(source.parent, installed.parent, apply=True, force=True)
            self.assertFalse(result[0]["changed"])
            self.assertEqual("full implementation", (installed / "SKILL.md").read_text(encoding="utf-8"))

    def test_linked_target_and_source_descendant_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            outside = root / "outside"
            outside.mkdir()
            target = root / "targets"
            target.mkdir()
            link = target / "source"
            try:
                link.symlink_to(outside, target_is_directory=True)
            except OSError as error:
                self.skipTest(f"Symlinks unavailable: {error}")
            with self.assertRaises(ValueError):
                sync_agent_skills.copy_source(source, target, None, True, True)
            link.unlink()
            (source / "linked").symlink_to(outside, target_is_directory=True)
            with self.assertRaises(ValueError):
                sync_agent_skills.copy_source(source, target, None, True, True)
            self.assertEqual([], list(outside.iterdir()))

    @unittest.skipUnless(os.name == "nt", "Windows junction behavior")
    def test_windows_junction_target_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            outside = root / "outside"
            outside.mkdir()
            (outside / "keep").write_text("preserved", encoding="utf-8")
            targets = root / "targets"
            targets.mkdir()
            link = targets / "source"
            quoted_link = str(link).replace("'", "''")
            quoted_outside = str(outside).replace("'", "''")
            subprocess.run(["powershell", "-NoProfile", "-Command", f"New-Item -ItemType Junction -Path '{quoted_link}' -Target '{quoted_outside}' | Out-Null"], check=True)
            try:
                with self.assertRaises(ValueError):
                    sync_agent_skills.copy_source(source, targets, None, True, True)
                self.assertEqual("preserved", (outside / "keep").read_text(encoding="utf-8"))
            finally:
                link.rmdir()

    def test_load_json_object_accepts_jsonc(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            settings_path = Path(temporary_directory) / "settings.json"
            settings_path.write_text(
                '{\n  // Keep this comment\n  "custom.url": "https://example.com/a//b",\n}\n',
                encoding="utf-8",
            )
            settings, is_jsonc = sync_agent_skills.load_json_object(settings_path)
            self.assertTrue(is_jsonc)
            self.assertEqual("https://example.com/a//b", settings["custom.url"])

    def test_doctor_adds_only_codex_skill_location(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            settings_path = Path(temporary_directory) / "settings.json"
            settings_path.write_text(
                '{"chat.agentSkillsLocations": {"custom/skills": true}}',
                encoding="utf-8",
            )
            result = sync_agent_skills.doctor_vscode(settings_path, apply=False)
            locations = result["effective_agent_skills_locations"]
            self.assertEqual({"custom/skills": True, "~/.codex/skills": True}, locations)

    def test_doctor_refuses_to_rewrite_jsonc(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            settings_path = Path(temporary_directory) / "settings.json"
            settings_path.write_text('{\n  // comment\n  "chat.useAgentSkills": false,\n}\n', encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "refusing to rewrite JSONC"):
                sync_agent_skills.doctor_vscode(settings_path, apply=True)

    def test_find_installed_repo_skill_roots(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            repo_root = Path(temporary_directory)
            cursor_skills = repo_root / ".cursor" / "skills"
            cursor_skills.mkdir(parents=True)
            skill_dir = cursor_skills / "sample-skill"
            skill_dir.mkdir()
            (skill_dir / "SKILL.md").write_text("---\nname: sample-skill\ndescription: test\n---\n", encoding="utf-8")

            discovered = sync_agent_skills.find_installed_repo_skill_roots(repo_root)
            self.assertEqual([cursor_skills], discovered)

    def test_sync_skills_from_master_dry_run_and_apply(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            temp_path = Path(temporary_directory)
            master_skills = temp_path / "master" / "skills"
            master_skills.mkdir(parents=True)
            master_skill = master_skills / "my-skill"
            master_skill.mkdir()
            (master_skill / "SKILL.md").write_text("---\nname: my-skill\ndescription: v2\n---\n", encoding="utf-8")

            target_root = temp_path / "target" / ".cursor" / "skills"
            target_root.mkdir(parents=True)
            target_skill = target_root / "my-skill"
            target_skill.mkdir()
            (target_skill / "SKILL.md").write_text("---\nname: my-skill\ndescription: v1\n---\n", encoding="utf-8")

            # Dry-run test
            dry_results = sync_agent_skills.sync_skills_from_master(
                master_skills_dir=master_skills,
                target_root=target_root,
                apply=False,
                force=True,
            )
            self.assertEqual(1, len(dry_results))
            self.assertTrue(dry_results[0]["changed"])
            self.assertIn("v1", (target_skill / "SKILL.md").read_text(encoding="utf-8"))

            # Apply test
            apply_results = sync_agent_skills.sync_skills_from_master(
                master_skills_dir=master_skills,
                target_root=target_root,
                apply=True,
                force=True,
            )
            self.assertEqual(1, len(apply_results))
            self.assertTrue(apply_results[0]["changed"])
            self.assertIn("v2", (target_skill / "SKILL.md").read_text(encoding="utf-8"))
            backups = list((target_root / ".sync-agent-skills-backups").glob("my-skill.*"))
            self.assertEqual(1, len(backups))
            self.assertIn("v1", (backups[0] / "SKILL.md").read_text(encoding="utf-8"))

            # No-op re-run test
            noop_results = sync_agent_skills.sync_skills_from_master(
                master_skills_dir=master_skills,
                target_root=target_root,
                apply=True,
                force=True,
            )
            self.assertEqual(1, len(noop_results))
            self.assertFalse(noop_results[0]["changed"])
            self.assertIn("target already matches source", noop_results[0]["actions"])


if __name__ == "__main__":
    unittest.main()
