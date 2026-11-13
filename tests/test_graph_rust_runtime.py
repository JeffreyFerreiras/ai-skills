import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("graph_rust_runtime", ROOT / "skills/software-engineering-graph/scripts/rust-runtime.py")
runtime = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runtime)


class RuntimeTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.skill = self.root / "skill"
        self.package = self.skill / "rust-source" / runtime.PACKAGE
        self.package.mkdir(parents=True)
        (self.package / "Cargo.toml").write_text("fixture")
        (self.skill / "scripts").mkdir()
        (self.skill / "scripts/rust-runtime.py").write_text("fixture launcher")
        self.cache = self.root / "cache"
        env = patch.dict(os.environ, {"GRAPH_RUST_CACHE_HOME": str(self.cache)})
        env.start()
        self.addCleanup(env.stop)

    def build(self, command, **kwargs):
        if command[0] == "cargo":
            target = Path(kwargs["env"]["CARGO_TARGET_DIR"]) / "release"
            target.mkdir(parents=True)
            for name in runtime.COMMANDS:
                suffix = ".exe" if os.name == "nt" else ""
                (target / (name + suffix)).write_bytes(b"native fixture " + name.encode())

    def first_run(self):
        with patch.object(runtime.shutil, "which", return_value="tool"), \
                patch.object(runtime.subprocess, "run", side_effect=self.build) as run:
            result = runtime.ensure_binary(self.skill, "graphctl")
            self.assertEqual(sum(call.args[0][0] == "cargo" for call in run.call_args_list), 1)
            return result

    def test_first_build_then_verified_reuse_without_compiler(self):
        source_before = runtime.source_key(self.skill)
        binary = self.first_run()
        with patch.object(runtime.shutil, "which", return_value=None), \
                patch.object(runtime.subprocess, "run") as run:
            self.assertEqual(runtime.ensure_binary(self.skill, "graphctl"), binary)
            run.assert_not_called()
        self.assertEqual(runtime.source_key(self.skill), source_before)

    def test_corruption_is_declined_without_overwriting_cache(self):
        binary = self.first_run()
        binary.write_bytes(b"corrupted")
        with patch.object(runtime.subprocess, "run") as run:
            with self.assertRaisesRegex(runtime.RuntimeError, "verification failed"):
                runtime.ensure_binary(self.skill, "graphctl")
            run.assert_not_called()
        self.assertEqual(binary.read_bytes(), b"corrupted")

    def test_source_changes_select_new_cache(self):
        binary = self.first_run()
        (self.package / "Cargo.toml").write_text("changed source")
        other = self.first_run()
        self.assertNotEqual(binary, other)
        self.assertTrue(binary.is_file())

    def test_relative_cache_rejected(self):
        with patch.dict(os.environ, {"GRAPH_RUST_CACHE_HOME": "relative"}):
            with self.assertRaisesRegex(runtime.RuntimeError, "absolute"):
                runtime.ensure_binary(self.skill, "graphctl")

    def test_cache_inside_skill_rejected(self):
        with patch.dict(os.environ, {"GRAPH_RUST_CACHE_HOME": str(self.skill / "cache")}):
            with self.assertRaisesRegex(runtime.RuntimeError, "immutable"):
                runtime.ensure_binary(self.skill, "graphctl")

    def test_missing_compiler_does_not_create_cache(self):
        with patch.object(runtime.shutil, "which", return_value=None):
            with self.assertRaisesRegex(runtime.RuntimeError, "first Rust graph run"):
                runtime.ensure_binary(self.skill, "graphctl")
        self.assertFalse(self.cache.exists())

    def test_concurrent_build_is_declined_without_removing_lock(self):
        key = runtime.source_key(self.skill)
        lock = self.cache / (key + ".lock")
        lock.mkdir(parents=True)
        with patch.object(runtime.shutil, "which", return_value="tool"):
            with self.assertRaisesRegex(runtime.RuntimeError, "already locked"):
                runtime.ensure_binary(self.skill, "graphctl")
        self.assertTrue(lock.is_dir())

    def test_failed_build_releases_lock_without_completion_receipt(self):
        with patch.object(runtime.shutil, "which", return_value="tool"), \
                patch.object(runtime.subprocess, "run", side_effect=runtime.subprocess.CalledProcessError(1, "cargo")):
            with self.assertRaises(runtime.subprocess.CalledProcessError):
                runtime.ensure_binary(self.skill, "graphctl")
        self.assertEqual(list(self.cache.glob("*.lock")), [])
        self.assertEqual(list(self.cache.rglob("complete.json")), [])

    def test_failure_has_no_python_engine_fallback(self):
        with patch.object(runtime, "ensure_binary", side_effect=runtime.RuntimeError("missing compiler")), \
                patch.object(runtime.os, "execve") as execute:
            self.assertEqual(runtime.main(["graphctl", "--help"]), 1)
            execute.assert_not_called()


if __name__ == "__main__":
    unittest.main()
