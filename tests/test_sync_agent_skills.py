from __future__ import annotations

import importlib.util
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
