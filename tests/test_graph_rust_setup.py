"""Fail-closed setup checks; integration builds are exercised separately."""

import hashlib
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("graph_rust_setup", ROOT / "skills/software-engineering-graph/scripts/setup-rust.py")
setup = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(setup)


class SetupTests(unittest.TestCase):
    def test_canonical_manifest_verifies(self):
        manifest = setup.verify_manifest(ROOT / setup.PACKAGE)
        self.assertEqual(manifest["input"]["size_bytes"], 2253079)

    def test_frozen_line_endings_require_exact_digest(self):
        raw = b"a\nb\n"
        historical = b"a\r\nb\r\n"
        self.assertEqual(setup.frozen_bytes(raw, hashlib.sha256(historical).hexdigest()), historical)
        with self.assertRaises(setup.SetupError):
            setup.frozen_bytes(raw, "0" * 64)

    def test_existing_destination_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "skill"
            target.mkdir()
            marker = target / "keep"
            marker.write_text("unchanged")
            with self.assertRaises(setup.SetupError):
                setup.install(ROOT, target)
            self.assertEqual(marker.read_text(), "unchanged")

    def test_bad_input_does_not_publish(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            supplied = root / "bad.bin"
            supplied.write_bytes(b"bad")
            manifest = setup.verify_manifest(ROOT / setup.PACKAGE)
            with self.assertRaises(setup.SetupError):
                setup.prepare_input(ROOT / setup.PACKAGE, root / "artifacts", manifest, supplied)
            self.assertFalse((root / "artifacts").exists())

    def test_no_input_calls_restorer(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            blob = b"checked"
            manifest = {"input": {"path": "compat/blob", "size_bytes": len(blob),
                                   "sha256": hashlib.sha256(blob).hexdigest()}}
            def restore(command):
                self.assertEqual(command[-2:], ["--artifact-root", str(root)])
                target = root / "compat/blob"
                target.parent.mkdir()
                target.write_bytes(blob)
            with patch.object(setup, "run", side_effect=restore):
                setup.prepare_input(ROOT / setup.PACKAGE, root, manifest, None)

    def test_native_wrappers_never_import_python_engine(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "scripts").mkdir()
            (root / "references").mkdir()
            (root / "SKILL.md").write_text("Run python <skill>/scripts/graphctl.py")
            setup.native_dispatch(root, "")
            wrapper = (root / "scripts/graphctl.py").read_text()
            self.assertIn("os.execv", wrapper)
            self.assertNotIn("graph_engine", wrapper)
            self.assertEqual((root / "SKILL.md").read_text(), "Run <skill>/scripts/graphctl")

    def test_missing_prerequisite_does_not_create_destination(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "skill"
            with patch.object(setup.shutil, "which", return_value=None):
                with self.assertRaises(setup.SetupError):
                    setup.install(ROOT, target)
            self.assertFalse(target.exists())

    def test_recursive_destination_is_rejected_before_copy(self):
        for subtree in (setup.SKILL, setup.PACKAGE):
            with self.subTest(subtree=subtree), patch.object(setup.shutil, "copytree") as copy:
                with self.assertRaisesRegex(setup.SetupError, "outside"):
                    setup.install(ROOT, ROOT / subtree / "new-install")
                copy.assert_not_called()

    def test_dangling_symlink_destination_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "skill"
            missing = root / "missing"
            try:
                target.symlink_to(missing, target_is_directory=True)
            except (OSError, NotImplementedError):
                self.skipTest("symlink creation is unavailable")
            with patch.object(setup.shutil, "copytree") as copy:
                with self.assertRaisesRegex(setup.SetupError, "symlink"):
                    setup.install(ROOT, target)
                copy.assert_not_called()
            self.assertTrue(target.is_symlink())
            self.assertFalse(missing.exists())

    def test_build_failure_cleans_staging_and_does_not_publish(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "skill"
            failure = setup.subprocess.CalledProcessError(1, "cargo")
            with patch.object(setup.shutil, "which", return_value="found"), \
                    patch.object(setup, "stage_oracle"), \
                    patch.object(setup, "prepare_input"), \
                    patch.object(setup, "run", side_effect=failure):
                with self.assertRaises(setup.subprocess.CalledProcessError):
                    setup.install(ROOT, target)
            self.assertFalse(target.exists())
            self.assertEqual(list(root.iterdir()), [])

    def test_success_packages_licenses_and_native_wrappers(self):
        if os.name == "nt":
            self.skipTest("fixture executable uses a POSIX shebang")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "skill-arbitrary-id"
            real_run = setup.run
            def fake_build(command, **kwargs):
                if command[0] != "cargo":
                    return real_run(command, **kwargs)
                release = Path(kwargs["env"]["CARGO_TARGET_DIR"]) / "release"
                release.mkdir(parents=True)
                for name in ("graphctl", "helper-register"):
                    binary = release / name
                    binary.write_text("#!/bin/sh\nexit 0\n")
                    binary.chmod(0o755)
            with patch.object(setup.shutil, "which", return_value="found"), \
                    patch.object(setup, "stage_oracle"), \
                    patch.object(setup, "prepare_input"), \
                    patch.object(setup, "run", side_effect=fake_build):
                self.assertEqual(setup.install(ROOT, target), target)
            self.assertTrue((target / "licenses/CPython-3.12.10-NOTICE.md").is_file())
            self.assertIn("os.execv", (target / "scripts/graphctl.py").read_text())
            self.assertEqual(list(root.iterdir()), [target])

    def test_source_package_does_not_build_or_include_native_binary(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "source-skill"
            def oracle(source, build, manifest):
                (build / setup.SKILL / "graph_engine").mkdir(parents=True)
            with patch.object(setup.shutil, "which", return_value="found"), \
                    patch.object(setup, "stage_oracle", side_effect=oracle), \
                    patch.object(setup, "run") as run:
                self.assertEqual(setup.install(ROOT, target, source_only=True), target)
                run.assert_not_called()
            self.assertTrue((target / "rust-source" / setup.PACKAGE / "Cargo.lock").is_file())
            self.assertIn("rust-runtime.py", (target / "scripts/graphctl.py").read_text())
            self.assertTrue((target / "scripts/graphctl").read_bytes().startswith(b"#!/usr/bin/env python3"))


if __name__ == "__main__":
    unittest.main()
