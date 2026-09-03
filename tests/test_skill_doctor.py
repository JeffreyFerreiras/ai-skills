from __future__ import annotations

import importlib.util
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
        cursor_skills.parent.mkdir(parents=True, exist_ok=True)
        if not cursor_skills.exists():
            cursor_skills.symlink_to(Path("..") / "skills", target_is_directory=True)

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
            (root / ".cursor" / "skills").unlink()
            audit = skill_doctor.audit_repository(root)
            self.assertIn("missing-cursor-skills-discovery", {issue.code for issue in audit.issues})

    def test_cursor_skills_discovery_drift_is_an_error(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            self.create_skill(root)
            (root / ".cursor" / "skills").unlink()
            drifted = root / ".cursor" / "skills" / "other-skill"
            drifted.mkdir(parents=True)
            (drifted / "SKILL.md").write_text(
                "---\nname: other-skill\ndescription: Drifted skill.\n---\n\n# Other\n",
                encoding="utf-8",
            )
            audit = skill_doctor.audit_repository(root)
            self.assertIn("cursor-skills-discovery-drift", {issue.code for issue in audit.issues})


if __name__ == "__main__":
    unittest.main()
