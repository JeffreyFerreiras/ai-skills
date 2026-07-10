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


if __name__ == "__main__":
    unittest.main()
