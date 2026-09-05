from __future__ import annotations

import importlib.util
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

    def test_windows_placeholder_uses_verified_alternative(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.create_skill(root)
            discovery = root / ".cursor/skills"
            shutil.rmtree(discovery)
            discovery.write_text("../skills", encoding="utf-8")
            shutil.copytree(root / "skills", root / ".agents/skills")
            self.assertEqual(0, skill_doctor.audit_repository(root).errors)
            (root / ".agents/skills/sample-skill/SKILL.md").write_text("stale", encoding="utf-8")
            self.assertIn("cursor-skills-content-drift", {issue.code for issue in skill_doctor.audit_repository(root).issues})

    def test_symlink_discovery_when_supported(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.create_skill(root)
            discovery = root / ".cursor/skills"
            shutil.rmtree(discovery)
            try:
                discovery.symlink_to(Path("..") / "skills", target_is_directory=True)
            except OSError as error:
                self.skipTest(f"Symlink creation unavailable: {error}")
            self.assertEqual(0, skill_doctor.audit_repository(root).errors)


if __name__ == "__main__":
    unittest.main()
