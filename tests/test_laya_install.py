import importlib.util
import json
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[1]
PACKAGE = REPOSITORY / "hooks" / "laya"
SPEC = importlib.util.spec_from_file_location("laya_install", PACKAGE / "install.py")
installer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(installer)


@unittest.skipUnless(os.name == "nt", "Windows installer")
class LayaInstallTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.home = self.root / "codex home"
        self.runtime = self.home / "laya-hook"
        self.model = self.root / "model store" / "english"
        for name in ("model.safetensors", "rl_agent_config.json", "encoder/config.json", "tokenizer/tokenizer.json"):
            path = self.model / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fixture", encoding="utf-8")

    def arguments(self, *extra):
        return installer.parse_args([
            "--model-path", str(self.model), "--codex-home", str(self.home),
            "--powershell", sys.executable, "--skip-dependencies", *extra,
        ])

    def prepare_environment(self):
        python = self.runtime / ".venv" / "Scripts" / "python.exe"
        python.parent.mkdir(parents=True)
        python.write_text("not executed with --skip-dependencies", encoding="utf-8")

    def install(self, *extra):
        with redirect_stdout(StringIO()):
            installer.install(self.arguments(*extra))

    def test_dry_run_does_not_create_runtime_or_configuration(self):
        self.install("--dry-run")
        self.assertFalse(self.home.exists())
        self.assertEqual("fixture", (self.model / "model.safetensors").read_text())

    def test_install_preserves_unrelated_hooks_and_keeps_weights_outside_runtime(self):
        self.prepare_environment()
        unrelated = {"type": "command", "command": "keep-this-hook"}
        (self.home / "hooks.json").write_text(json.dumps({
            "description": "keep metadata", "hooks": {"UserPromptSubmit": [{"hooks": [unrelated]}]},
        }))
        self.install()
        config = json.loads((self.runtime / "config.json").read_text())
        self.assertEqual(str(self.model.resolve()), config["model_path"])
        self.assertEqual("advisory", config["mode"])
        self.assertFalse((self.runtime / "model.safetensors").exists())
        self.assertTrue((self.runtime / "token.txt").read_text())
        hooks = json.loads((self.home / "hooks.json").read_text())
        self.assertEqual("keep metadata", hooks["description"])
        self.assertEqual(unrelated, hooks["hooks"]["UserPromptSubmit"][0]["hooks"][0])
        prompt = hooks["hooks"]["UserPromptSubmit"][1]["hooks"][0]
        self.assertFalse(prompt["async"])
        self.assertEqual(256, prompt["additionalContextLimit"])
        self.assertIn('"', prompt["command"])
        self.assertTrue(hooks["hooks"]["SessionStart"][0]["hooks"][0]["async"])

    def test_repeat_install_preserves_local_state_and_does_not_duplicate_hooks(self):
        self.prepare_environment()
        self.install()
        token = (self.runtime / "token.txt").read_text()
        (self.runtime / "routing.jsonl").write_text("keep-log")
        config_path = self.runtime / "config.json"
        config = json.loads(config_path.read_text())
        config.update({"enabled": False, "mode": "observe", "port": 19000, "device": "cuda", "custom": "keep"})
        config_path.write_text(json.dumps(config))
        self.install()
        self.assertEqual(token, (self.runtime / "token.txt").read_text())
        self.assertEqual("keep-log", (self.runtime / "routing.jsonl").read_text())
        self.assertEqual(config, json.loads(config_path.read_text()))
        hooks = json.loads((self.home / "hooks.json").read_text())["hooks"]
        self.assertEqual(1, len(hooks["UserPromptSubmit"]))
        self.assertEqual(1, len(hooks["SessionStart"]))

    def test_explicit_settings_override_preserved_defaults(self):
        self.prepare_environment()
        self.install()
        self.install("--mode", "observe", "--device", "cpu", "--port", "19001")
        config = json.loads((self.runtime / "config.json").read_text())
        self.assertEqual(("observe", "cpu", 19001), (config["mode"], config["device"], config["port"]))

    def test_subagent_install_preserves_hooks_and_adds_instructions_once(self):
        self.prepare_environment()
        hooks = '{"hooks":{"Stop":[]}}'
        (self.home / "hooks.json").write_text(hooks)
        instructions = self.home / "AGENTS.md"
        instructions.write_text("Existing guidance.\n", encoding="utf-8")
        self.install("--integration", "subagents")
        self.install("--integration", "subagents")
        self.assertEqual(hooks, (self.home / "hooks.json").read_text())
        guidance = instructions.read_text(encoding="utf-8")
        self.assertTrue(guidance.startswith("Existing guidance.\n"))
        self.assertEqual(1, guidance.count("<!-- laya-subagents:start -->"))
        self.assertIn(self.runtime.as_posix(), guidance)
        self.assertNotIn("__LAYA_", guidance)
        config = json.loads((self.runtime / "config.json").read_text())
        self.assertTrue(config["subagent_routing_enabled"])
        self.assertFalse(config["enabled"])
        self.assertTrue((self.runtime / "route_subagent.py").is_file())

    def test_subagent_install_uses_existing_nonempty_override(self):
        self.prepare_environment()
        override = self.home / "AGENTS.override.md"
        override.write_text("Override instructions.\n")
        self.install("--integration", "subagents")
        self.assertIn("laya-subagents:start", override.read_text())
        self.assertFalse((self.home / "AGENTS.md").exists())
        self.assertFalse((self.home / "hooks.json").exists())

    def test_malformed_markers_reject_before_runtime_writes(self):
        self.prepare_environment()
        instructions = self.home / "AGENTS.md"
        instructions.write_text("<!-- laya-subagents:start -->")
        with self.assertRaisesRegex(ValueError, "Malformed"):
            self.install("--integration", "subagents")
        self.assertFalse((self.runtime / "config.json").exists())

    def test_missing_checkpoint_rejects_before_writes(self):
        (self.model / "model.safetensors").unlink()
        with self.assertRaisesRegex(ValueError, "checkpoint"):
            self.install()
        self.assertFalse(self.home.exists())

    def test_invalid_port_rejects_before_writes(self):
        with self.assertRaisesRegex(ValueError, "Port"):
            self.install("--port", "65536")
        self.assertFalse(self.home.exists())

    def test_source_repository_cannot_be_runtime_storage(self):
        args = self.arguments()
        args.codex_home = REPOSITORY / "not-a-runtime"
        with self.assertRaisesRegex(ValueError, "outside"):
            installer.install(args)
        self.assertFalse(args.codex_home.exists())

    def test_skipping_dependencies_requires_existing_environment(self):
        with self.assertRaisesRegex(ValueError, "existing runtime"):
            self.install()
        self.assertFalse(self.home.exists())


class LayaRegistrationTests(unittest.TestCase):
    def test_merge_preserves_other_handlers_in_the_same_group(self):
        runtime = Path("C:/example/runtime")
        registrations = installer.hook_registrations(runtime, Path("C:/PowerShell/pwsh.exe"))
        own = registrations["UserPromptSubmit"]["hooks"][0]
        unrelated = {"type": "command", "command": "python other.py"}
        document = {"hooks": {"UserPromptSubmit": [{"hooks": [own, unrelated]}]}}
        merged = installer.merge_hooks(document, registrations, runtime)
        self.assertEqual([unrelated], merged["hooks"]["UserPromptSubmit"][0]["hooks"])
        self.assertEqual(2, len(merged["hooks"]["UserPromptSubmit"]))

    def test_handler_mentioning_a_laya_script_is_not_claimed_as_owned(self):
        runtime = Path("C:/example/runtime")
        registrations = installer.hook_registrations(runtime, Path("C:/PowerShell/pwsh.exe"))
        unrelated = {"type": "command", "command": "python audit.py C:/example/runtime/hook.py"}
        document = {"hooks": {"UserPromptSubmit": [{"hooks": [unrelated]}]}}
        merged = installer.merge_hooks(document, registrations, runtime)
        self.assertEqual([unrelated], merged["hooks"]["UserPromptSubmit"][0]["hooks"])


if __name__ == "__main__":
    unittest.main()
