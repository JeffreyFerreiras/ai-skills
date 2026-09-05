from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(os.name == "nt", "PowerShell deletion example")
class RemovalGuardTests(unittest.TestCase):
    def test_example_rejects_root_and_sibling_but_removes_child(self) -> None:
        content = (ROOT / "skills/remove-agent-skill/SKILL.md").read_text(encoding="utf-8")
        example = content.split("```powershell\n", 2)[2].split("```", 1)[0]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            allowed = root / "skills"
            allowed.mkdir()
            sibling = root / "skills-other"
            sibling.mkdir()
            child = allowed / "sample"
            child.mkdir()
            script = root / "guard.ps1"
            script.write_text("param($allowedRoot, $target)\n$ErrorActionPreference = 'Stop'\n" + example, encoding="utf-8")
            for target in (allowed, sibling):
                result = subprocess.run(["powershell", "-NoProfile", "-File", str(script), str(allowed), str(target)], capture_output=True)
                self.assertNotEqual(0, result.returncode)
                self.assertTrue(target.exists())
            result = subprocess.run(["powershell", "-NoProfile", "-File", str(script), str(allowed), str(child)], capture_output=True)
            self.assertEqual(0, result.returncode, result.stderr.decode(errors="replace"))
            self.assertFalse(child.exists())


if __name__ == "__main__":
    unittest.main()
