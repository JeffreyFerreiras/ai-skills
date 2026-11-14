from __future__ import annotations

import importlib.util
import io
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = REPOSITORY_ROOT / "skills" / "sync-agent-skills" / "scripts" / "sync_agent_skills.py"
SPEC = importlib.util.spec_from_file_location("sync_agent_skills", MODULE_PATH)
assert SPEC and SPEC.loader
sync_agent_skills = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(sync_agent_skills)


def load_sync_helper(name: str):
    spec = importlib.util.spec_from_file_location(name, MODULE_PATH.with_name(f"{name}.py"))
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@unittest.skipUnless(shutil.which("ssh-keygen"), "OpenSSH is required for signed skill tests")
class SignedSkillTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.master = self.root / "source"
        self.source = self.master / "skills/demo"
        self.source.mkdir(parents=True)
        self.target = self.root / "profile/demo"
        self.key = self.root / "signing_key"
        self.signers = self.root / "allowed_signers"
        self.manifest = self.master / ".skill-signatures.json"
        self.run_command("git", "init", str(self.master))
        self.run_command("ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(self.key))
        self.signers.write_text("ai-skills " + self.key.with_suffix(".pub").read_text(), encoding="ascii")
        (self.source / "SKILL.md").write_text("---\nname: demo\ndescription: demo\n---\n", encoding="utf-8")
        for name in ("obsolete.mp4", "edited.mp4"):
            (self.source / name).write_bytes(b"tracked video")
        self.commit_source()
        self.sign_source()

    def run_command(self, *args, expected=0):
        result = subprocess.run(args, cwd=self.root, capture_output=True, text=True)
        self.assertEqual(expected, result.returncode, result.stdout + result.stderr)
        return result

    def commit_source(self):
        self.run_command("git", "-C", str(self.master), "add", "skills")
        self.run_command("git", "-C", str(self.master), "-c", "user.name=Test", "-c", "user.email=test@example.test",
                         "-c", "commit.gpgsign=false", "commit", "-m", "fixture")

    def sign_source(self):
        self.manifest.unlink(missing_ok=True)
        self.run_command(sys.executable, str(MODULE_PATH), "sign-skills", "--master", str(self.master),
                         "--signing-key", str(self.key), "--output", str(self.manifest))

    def sync(self, *options, expected=0):
        return self.run_command(
            sys.executable, str(MODULE_PATH), "sync-from-master", "--master", str(self.master),
            "--target-root", str(self.target.parent), "--all", "--force", "--json",
            *options, expected=expected,
        )

    def retire_videos(self):
        (self.source / "obsolete.mp4").unlink()
        (self.source / "edited.mp4").unlink()
        self.commit_source()
        self.sign_source()

    def test_prunes_only_unchanged_previously_signed_files(self):
        # Legacy content is retained on the first signed sync.
        self.target.mkdir(parents=True)
        (self.target / "local.mp4").write_bytes(b"local video")
        (self.source / "untracked.mp4").write_bytes(b"untracked video")
        options = ("--allowed-signers", str(self.signers))
        self.sync(*options, "--apply")
        self.assertTrue((self.target / ".skill-signature.json").is_file())
        self.assertFalse((self.target / "untracked.mp4").exists())
        (self.target / "edited.mp4").write_bytes(b"local edit")

        self.retire_videos()
        preview = json.loads(self.sync(*options).stdout)[0]
        self.assertIn("prune signed file removed from source: obsolete.mp4", preview["actions"])
        self.assertIn("kept changed or read-only file removed from source: edited.mp4", preview["actions"])
        self.assertTrue((self.target / "obsolete.mp4").is_file())
        self.sync(*options, "--apply")
        self.assertFalse((self.target / "obsolete.mp4").exists())
        self.assertEqual(b"local video", (self.target / "local.mp4").read_bytes())
        self.assertEqual(b"local edit", (self.target / "edited.mp4").read_bytes())
        self.assertTrue(list(self.target.parent.glob(".sync-agent-skills-backups/demo.*/obsolete.mp4")))
        self.assertFalse(json.loads(self.sync(*options).stdout)[0]["changed"])

    def test_rejects_tampered_or_stale_source_and_unsigned_downgrade(self):
        options = ("--allowed-signers", str(self.signers))
        self.sync(*options, "--apply")
        original = self.manifest.read_bytes()
        changed = json.loads(original)
        changed["demo"]["manifest"]["files"]["SKILL.md"]["sha256"] = "tampered"
        self.manifest.write_text(json.dumps(changed), encoding="utf-8")
        self.assertIn("did not verify", self.sync(*options, "--apply", expected=1).stderr)
        self.manifest.write_bytes(original)
        (self.source / "new.txt").write_text("new tracked file", encoding="utf-8")
        self.commit_source()
        self.assertIn("do not match", self.sync(*options, "--apply", expected=1).stderr)
        self.assertIn("requires signed sync", self.sync("--apply", expected=1).stderr)
        self.assertTrue((self.target / "obsolete.mp4").is_file())
        self.assertFalse((self.target / "new.txt").exists())

    def test_invalid_previous_signature_preserves_obsolete_files(self):
        options = ("--allowed-signers", str(self.signers))
        self.sync(*options, "--apply")
        (self.target / ".skill-signature.json").write_text("{}", encoding="utf-8")
        self.retire_videos()
        result = self.sync(*options, "--apply", "--no-backup")
        self.assertIn("previous skill signature is invalid", result.stdout)
        self.assertTrue((self.target / "obsolete.mp4").is_file())

    def test_preserves_local_pointer_and_repairs_tracked_files_named_tags(self):
        self.target.mkdir(parents=True)
        local_pointer = self.target / "external-source.json"
        local_pointer.write_text("local data", encoding="utf-8")
        (self.source / "tags").write_bytes(b"tracked tags")
        self.commit_source()
        self.sign_source()
        options = ("--allowed-signers", str(self.signers), "--apply")
        self.sync(*options)
        self.assertEqual("local data", local_pointer.read_text(encoding="utf-8"))
        (self.target / "tags").write_bytes(b"edited")
        self.sync(*options)
        self.assertEqual(b"tracked tags", (self.target / "tags").read_bytes())

    def test_prune_allows_tracked_directory_to_become_a_file(self):
        media = self.source / "media"
        media.mkdir()
        (media / "old.mp4").write_bytes(b"tracked file")
        self.commit_source()
        self.sign_source()
        options = ("--allowed-signers", str(self.signers), "--apply")
        self.sync(*options)
        (media / "old.mp4").unlink()
        media.rmdir()
        media.write_bytes(b"replacement file")
        self.commit_source()
        self.sign_source()
        self.sync(*options)
        self.assertEqual(b"replacement file", (self.target / "media").read_bytes())

    def test_executable_mode_is_authenticated(self):
        self.run_command("git", "-C", str(self.master), "update-index", "--chmod=+x", "skills/demo/SKILL.md")
        self.run_command("git", "-C", str(self.master), "-c", "user.name=Test", "-c", "user.email=test@example.test",
                         "-c", "commit.gpgsign=false", "commit", "-m", "change mode")
        result = self.sync("--allowed-signers", str(self.signers), "--apply", expected=1)
        self.assertIn("do not match", result.stderr)
        self.assertFalse(self.target.exists())

    def test_publish_restores_live_skill_when_it_changed_after_staging(self):
        helpers = {"signed_skills": load_sync_helper("signed_skills")}
        if os.name == "nt":
            helpers["windows_streams"] = load_sync_helper("windows_streams")
        with patch.dict(sys.modules, helpers):
            self.target.mkdir(parents=True)
            video = self.target / "obsolete.mp4"
            video.write_bytes(b"original")
            baseline = sync_agent_skills.skill_tree_signature(self.target)
            staged = self.root / "staged"
            staged.mkdir()
            (staged / "SKILL.md").write_bytes(b"new")
            video.write_bytes(b"edit during sync")
            with self.assertRaisesRegex(ValueError, "changed during staging"):
                sync_agent_skills.publish_signed_skill(staged, self.target, baseline, None)
            self.assertEqual(b"edit during sync", video.read_bytes())

    def test_invalid_backup_destination_leaves_profile_unchanged(self):
        options = ("--allowed-signers", str(self.signers), "--apply")
        self.sync(*options)
        self.retire_videos()
        backup = self.root / "invalid-backup"
        backup.write_bytes(b"existing file")
        self.sync(*options, "--backup-root", str(backup), expected=1)
        self.assertEqual(b"tracked video", (self.target / "obsolete.mp4").read_bytes())
        self.assertFalse(list(self.target.parent.glob(".signed-skill-*")))

    def test_preserves_retired_files_with_local_permission_changes(self):
        options = ("--allowed-signers", str(self.signers), "--apply", "--no-backup")
        self.sync(*options)
        installed = self.target / "obsolete.mp4"
        installed.chmod(stat.S_IREAD if os.name == "nt" else 0o600)
        self.addCleanup(installed.chmod, stat.S_IWRITE | stat.S_IREAD)
        self.retire_videos()
        self.sync(*options)
        self.assertEqual(b"tracked video", installed.read_bytes())

    @unittest.skipUnless(os.name == "nt", "Windows alternate data streams")
    def test_preserves_alternate_streams_on_retired_video_without_backup(self):
        options = ("--allowed-signers", str(self.signers), "--apply", "--no-backup")
        self.sync(*options)
        retired = self.target / "obsolete.mp4"
        stream = Path(f"{retired}:local-notes")
        stream.write_bytes(b"unsynced edit")
        directory_stream = Path(f"{self.target}:profile-notes")
        directory_stream.write_bytes(b"local folder metadata")
        nested = self.target / "media"
        nested.mkdir()
        nested_stream = Path(f"{nested}:local-notes")
        nested_stream.write_bytes(b"read-only folder metadata")
        nested.chmod(stat.S_IREAD)
        self.retire_videos()
        result = json.loads(self.sync(*options).stdout)[0]
        self.assertIn("kept file with Windows alternate data streams: obsolete.mp4", result["actions"])
        self.assertEqual(b"tracked video", retired.read_bytes())
        self.assertEqual(b"unsynced edit", stream.read_bytes())
        self.assertEqual(b"local folder metadata", directory_stream.read_bytes())
        self.assertEqual(b"read-only folder metadata", nested_stream.read_bytes())
        self.assertFalse(nested.stat().st_mode & stat.S_IWRITE)
        self.assertFalse((self.target / "edited.mp4").exists())
        self.assertFalse((self.target.parent / ".sync-agent-skills-backups").exists())

    def test_unsigned_copy_rejects_receipt_in_source(self):
        source = self.root / "unsigned"
        source.mkdir()
        receipt = json.loads(self.manifest.read_bytes())["demo"]
        (source / ".skill-signature.json").write_text(json.dumps(receipt), encoding="utf-8")
        result = self.run_command(
            sys.executable, str(MODULE_PATH), "sync", "--source", str(source),
            "--target-root", str(self.target.parent), "--apply", expected=1,
        )
        self.assertIn("receipts cannot be copied", result.stderr)
        self.assertFalse((self.target.parent / "unsigned").exists())

    def test_unsigned_copy_rejects_standalone_nested_and_renamed_receipts(self):
        self.sync("--allowed-signers", str(self.signers), "--apply")
        receipt = self.target / ".skill-signature.json"
        innocent = self.root / "receipt.txt"
        shutil.copy2(receipt, innocent)
        target_root = self.root / "unsigned"
        for source, target_name in (
            (receipt, "ordinary.json"),
            (self.target.parent, "nested"),
            (innocent, ".SKILL-SIGNATURE.JSON"),
        ):
            with self.subTest(source=source, target_name=target_name):
                result = self.run_command(
                    sys.executable, str(MODULE_PATH), "sync", "--source", str(source),
                    "--target-root", str(target_root), "--target-name", target_name,
                    "--apply", expected=1,
                )
                self.assertIn("receipts cannot be copied", result.stderr)
                self.assertFalse(target_root.exists())
        result = self.run_command(
            sys.executable, str(MODULE_PATH), "sync", "--source", str(self.source.parent),
            "--target-root", str(self.root), "--target-name", self.target.parent.name,
            "--force", "--apply", expected=1,
        )
        self.assertIn("requires signed sync", result.stderr)
        self.assertTrue(receipt.exists())

    def test_exports_signed_install_for_publication_without_copying_receipt(self):
        self.sync("--allowed-signers", str(self.signers), "--apply")
        receipt = self.target / ".skill-signature.json"
        original = receipt.read_bytes()
        (self.target / "SKILL.md").write_text("local edit", encoding="utf-8")
        (self.target / "external-source.json").write_text("local data", encoding="utf-8")
        command = (
            sys.executable, str(MODULE_PATH), "sync", "--source", str(self.target),
            "--target-root", str(self.source.parent), "--strip-install-receipt", "--force", "--no-backup",
        )
        self.run_command(*command)
        self.assertNotEqual("local edit", (self.source / "SKILL.md").read_text(encoding="utf-8"))
        self.run_command(*command, "--apply")
        self.assertEqual("local edit", (self.source / "SKILL.md").read_text(encoding="utf-8"))
        self.assertEqual("local data", (self.source / "external-source.json").read_text(encoding="utf-8"))
        self.assertFalse((self.source / receipt.name).exists())
        self.assertEqual(original, receipt.read_bytes())

    def test_rejects_git_export_omissions_before_pruning(self):
        options = ("--allowed-signers", str(self.signers), "--apply")
        self.sync(*options)
        (self.source / ".gitattributes").write_text("*.mp4 export-ignore\n", encoding="ascii")
        self.commit_source()
        result = self.sync(*options, expected=1)
        self.assertIn("Git archive omitted tracked skill file", result.stderr)
        self.assertTrue((self.target / "obsolete.mp4").is_file())


class SyncAgentSkillsTests(unittest.TestCase):
    def test_codex_cleanup_previews_and_moves_only_legacy_duplicates(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            profile = Path(directory)
            shared = profile / ".agents/skills"
            codex = profile / "custom-codex-home"
            legacy = codex / "skills"
            archive = codex / ".retired-profile-skills"
            for folder in (shared / "catch-up", legacy / "catch-up", legacy / "only-local", legacy / ".system"):
                folder.mkdir(parents=True)
                (folder / "SKILL.md").write_text(
                    f"---\nname: {folder.name}\ndescription: example\n---\n", encoding="utf-8",
                )
            (legacy / "catch-up/local-state.json").write_text("preserve", encoding="utf-8")
            (legacy / "catch-up/SKILL.md").write_text(
                "---\nname: catch-up # legacy copy\ndescription: example\n---\n", encoding="utf-8",
            )

            catalog = {
                legacy / "catch-up/SKILL.md": ("catch-up", True),
                shared / "catch-up/SKILL.md": ("catch-up", True),
            }
            with (
                patch.dict(os.environ, {"CODEX_HOME": str(codex)}),
                patch.object(sync_agent_skills, "codex_profile_catalogs", return_value=[catalog]),
            ):
                preview = sync_agent_skills.retire_codex_duplicates(shared, apply=False)
                self.assertEqual(["catch-up"], [item["skill"] for item in preview if item["changed"]])
                self.assertFalse(archive.exists())
                self.assertTrue((legacy / "catch-up").is_dir())
                with patch.object(Path, "rename", side_effect=OSError("move interrupted")):
                    failed = sync_agent_skills.retire_codex_duplicates(shared, apply=True)
                self.assertFalse(failed[0]["changed"])
                self.assertTrue((legacy / "catch-up").is_dir())
                self.assertFalse((archive / "catch-up").exists())
                applied = sync_agent_skills.retire_codex_duplicates(shared, apply=True)
                self.assertEqual(["catch-up"], [item["skill"] for item in applied if item["changed"]])
                self.assertFalse(any(item["changed"] for item in sync_agent_skills.retire_codex_duplicates(shared, True)))
                (legacy / "catch-up").mkdir()
                (legacy / "catch-up/SKILL.md").write_text("---\nname: catch-up\n---\n", encoding="utf-8")
                collision = sync_agent_skills.retire_codex_duplicates(shared, apply=True, skill_names=["catch-up"])

            self.assertFalse(collision[0]["changed"])
            self.assertIn("archive destination already exists", collision[0]["actions"][0])
            self.assertEqual("preserve", (archive / "catch-up/local-state.json").read_text())
            self.assertTrue((shared / "catch-up/SKILL.md").is_file())
            self.assertTrue((legacy / "only-local/SKILL.md").is_file())
            self.assertTrue((legacy / ".system/SKILL.md").is_file())

    def test_master_sync_cleans_only_the_shared_profile_and_requested_skills(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            profile = Path(directory)
            master = profile / "master/skills"
            shared = profile / ".agents/skills"
            other = profile / "other"
            codex = profile / ".codex"
            legacy = codex / "skills"
            for root in (master, legacy):
                for name in ("first", "second"):
                    skill = root / name
                    skill.mkdir(parents=True)
                    (skill / "SKILL.md").write_text(
                        f"---\nname: {name}\ndescription: example\n---\n", encoding="utf-8",
                    )

            def sync(target_root: Path, apply: bool, name: str = "first") -> list[dict]:
                argv = ["sync-agent-skills", "sync-from-master", "--master", str(master),
                        "--target-root", str(target_root), "--skill", name, "--json"]
                if apply:
                    argv.append("--apply")
                output = io.StringIO()
                with patch.object(sys, "argv", argv), redirect_stdout(output):
                    self.assertEqual(0, sync_agent_skills.main())
                return json.loads(output.getvalue())

            catalog = {
                legacy / "first/SKILL.md": ("first", True),
                shared / "first/SKILL.md": ("first", True),
            }
            with (
                patch.object(sync_agent_skills, "home", return_value=profile),
                patch.object(sync_agent_skills, "codex_profile_catalogs", return_value=[catalog]),
            ):
                with patch.dict(os.environ, {"CODEX_HOME": str(codex)}):
                    sync(other, apply=True)
                    self.assertTrue((legacy / "first").exists())
                    preview = sync(shared, apply=False)
                    self.assertIn("if Codex discovers the synced replacement", preview[1]["actions"][0])
                    self.assertFalse(shared.exists())
                    self.assertTrue((legacy / "first").exists())
                    sync(shared, apply=True)
                with patch.dict(os.environ, {"CODEX_HOME": str(shared.parent)}):
                    results = sync(shared, apply=True, name="second")
                    self.assertTrue(results[0]["changed"])
                    self.assertIn("overlaps", results[1]["actions"][0])

            self.assertFalse((legacy / "first").exists())
            self.assertTrue((legacy / "second").exists())

    def test_codex_cleanup_keeps_legacy_without_an_enabled_match_in_every_profile(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            profile = Path(directory)
            codex = profile / ".codex"
            legacy = codex / "skills/example"
            shared_root = profile / ".agents/skills"
            shared = shared_root / "example"
            for skill in (legacy, shared):
                skill.mkdir(parents=True)
                (skill / "SKILL.md").write_text(
                    "---\nname: example\ndescription: fixture\n---\n", encoding="utf-8",
                )
            old = legacy / "SKILL.md"
            new = shared / "SKILL.md"
            enabled = {old: ("example", True), new: ("example", True)}
            profile_catalogs = {
                "replacement missing": [{old: ("example", True)}],
                "different name": [{old: ("example", True), new: ("different", True)}],
                "legacy missing": [{new: ("example", True)}],
                "disabled in named profile": [enabled, {**enabled, new: ("example", False)}],
                "catalog unavailable": [],
            }
            for case, catalogs in profile_catalogs.items():
                with (
                    self.subTest(case=case),
                    patch.dict(os.environ, {"CODEX_HOME": str(codex)}),
                    patch.object(sync_agent_skills, "codex_profile_catalogs", return_value=catalogs),
                ):
                    result = sync_agent_skills.retire_codex_duplicates(shared_root, apply=True)
                    self.assertFalse(result[0]["changed"])
                    self.assertIn("could not confirm an enabled replacement", result[0]["actions"][0])
                    self.assertTrue(old.is_file())
                    self.assertFalse((codex / ".retired-profile-skills").exists())

            with (
                patch.dict(os.environ, {"CODEX_HOME": str(codex)}),
                patch.object(sync_agent_skills, "codex_profile_catalogs", side_effect=ValueError("discovery timed out")),
            ):
                result = sync_agent_skills.retire_codex_duplicates(shared_root, apply=True)
                self.assertFalse(result[0]["changed"])
                self.assertIn("discovery timed out", result[0]["actions"][0])
                self.assertTrue(old.is_file())

    def test_cleanup_checks_default_and_named_profiles(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            profile = Path(directory)
            codex = profile / ".codex"
            codex.mkdir()
            for name in ("config.toml", "lab.config.toml"):
                (codex / name).touch()
            (codex / "work.config.toml").write_text(
                '[[skills.config]]\nname = "example"\nenabled = false\n'
                '[[skills.config]]\npath = "../.agents/skills/example/SKILL.md"\nenabled = true\n',
                encoding="utf-8",
            )
            with (
                patch.object(sync_agent_skills.shutil, "which", return_value="codex"),
                patch.object(sync_agent_skills, "codex_skill_catalog", return_value={}) as catalog,
            ):
                sync_agent_skills.codex_profile_catalogs(codex, profile / ".agents/skills")
                replacement = json.dumps(str(profile.resolve() / ".agents/skills/example/SKILL.md"))
                overrides = 'skills.config=[{enabled = false, name = "example"}, {enabled = true, path = '
                self.assertEqual([
                    ("codex", codex, profile),
                    ("codex", codex, profile, overrides + replacement + "}]"),
                ], [call.args for call in catalog.call_args_list])

    def test_profile_lock_blocks_shared_sync_and_cleanup_until_released(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            profile = Path(directory)
            shared = profile / ".agents/skills"
            source = profile / "example"
            source.mkdir()
            (source / "SKILL.md").write_text("fixture", encoding="utf-8")
            commands = (
                ["sync", "--source", str(source), "--target-root", str(shared), "--apply"],
                ["sync-from-master", "--master", str(profile), "--target-root", str(shared), "--all", "--apply"],
                ["cleanup-codex-duplicates", "--apply"],
            )
            with patch.object(sync_agent_skills, "home", return_value=profile):
                with sync_agent_skills.profile_write_lock(shared):
                    for command in commands:
                        with patch.object(sys, "argv", ["sync-agent-skills", *command]):
                            with self.assertRaisesRegex(SystemExit, "locked by another operation"):
                                sync_agent_skills.main()
                    self.assertFalse(shared.exists())
                with sync_agent_skills.profile_write_lock(shared):
                    self.assertFalse(shared.exists())

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

    def test_no_backup_replaces_target_without_retained_copy(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            (source / "SKILL.md").write_text("new", encoding="utf-8")
            target_root = root / "installed"
            target = target_root / "source"
            target.mkdir(parents=True)
            (target / "SKILL.md").write_text("old", encoding="utf-8")

            result = sync_agent_skills.copy_source(source, target_root, None, True, True, no_backup=True)

            self.assertTrue(result["changed"])
            self.assertEqual("new", (target / "SKILL.md").read_text(encoding="utf-8"))
            self.assertEqual(["source"], [item.name for item in target_root.iterdir()])
            self.assertFalse((target_root / ".sync-agent-skills-backups").exists())

    def test_no_backup_restores_target_when_copy_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            (source / "SKILL.md").write_text("new", encoding="utf-8")
            target_root = root / "installed"
            target = target_root / "source"
            target.mkdir(parents=True)
            (target / "SKILL.md").write_text("old", encoding="utf-8")

            with patch.object(sync_agent_skills.shutil, "copytree", side_effect=OSError("copy failed")):
                with self.assertRaisesRegex(OSError, "copy failed"):
                    sync_agent_skills.copy_source(source, target_root, None, True, True, no_backup=True)

            self.assertEqual("old", (target / "SKILL.md").read_text(encoding="utf-8"))
            self.assertEqual(["source"], [item.name for item in target_root.iterdir()])

    def test_invalid_external_pointer_preserves_installation(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "master/graph"
            source.mkdir(parents=True)
            (source / "SKILL.md").write_text("stub", encoding="utf-8")
            (source / "external-source.json").write_text('{}', encoding="utf-8")
            installed = root / "target/graph"
            installed.mkdir(parents=True)
            (installed / "SKILL.md").write_text("full implementation", encoding="utf-8")
            with self.assertRaises(ValueError):
                sync_agent_skills.sync_skills_from_master(source.parent, installed.parent, apply=True, force=True)
            self.assertEqual("full implementation", (installed / "SKILL.md").read_text(encoding="utf-8"))

    def make_external_fixture(self, root: Path):
        upstream = root / "upstream"
        upstream.mkdir()
        subprocess.run(["git", "init", str(upstream)], check=True, capture_output=True)
        (upstream / "SKILL.md").write_text("---\nname: graph\ndescription: full graph\n---\n", encoding="utf-8")
        (upstream / "engine.py").write_text("VERSION = 1\n", encoding="utf-8")
        nested = upstream / ".agents/skills/duplicate"
        nested.mkdir(parents=True)
        (nested / "SKILL.md").write_text("development skill", encoding="utf-8")
        self.commit_fixture(upstream)
        pointer = root / "master/graph"
        pointer.mkdir(parents=True)
        (pointer / "SKILL.md").write_text("pointer only", encoding="utf-8")
        manifest = {"repository": "https://example.test/graph.git", "management": "external",
                    "required_files": ["SKILL.md", "engine.py"]}
        (pointer / "external-source.json").write_text(json.dumps(manifest), encoding="utf-8")
        real_git = sync_agent_skills.run_git

        def local_git(*args):
            return real_git(*(str(upstream) if arg == manifest["repository"] else arg for arg in args))

        return pointer, upstream, local_git

    def commit_fixture(self, upstream: Path):
        subprocess.run(["git", "-C", str(upstream), "add", "."], check=True, capture_output=True)
        subprocess.run(["git", "-C", str(upstream), "-c", "user.name=Test", "-c", "user.email=test@example.test",
                        "-c", "commit.gpgsign=false", "commit", "-m", "fixture"], check=True, capture_output=True)

    def test_external_sync_resolves_latest_with_backup_and_no_nested_skills(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            pointer, upstream, local_git = self.make_external_fixture(root)
            target = root / "installed/graph"
            target.mkdir(parents=True)
            (target / "SKILL.md").write_text("old installation", encoding="utf-8")
            backups = root / "backups"
            with patch.object(sync_agent_skills, "run_git", side_effect=local_git):
                dry = sync_agent_skills.copy_source(pointer, target.parent, None, False, True, backups)
                self.assertTrue(dry["changed"])
                self.assertEqual("old installation", (target / "SKILL.md").read_text())
                self.assertFalse(backups.exists())
                refused = sync_agent_skills.copy_source(pointer, target.parent, None, True, False, backups)
                self.assertFalse(refused["changed"])
                self.assertEqual("old installation", (target / "SKILL.md").read_text())
                first = sync_agent_skills.copy_source(pointer, target.parent, None, True, True, backups)
                self.assertTrue(first["changed"])
                self.assertTrue((target / "engine.py").is_file())
                self.assertFalse((target / ".agents").exists())
                self.assertFalse((target / ".git").exists())
                self.assertFalse((target / "external-source.json").exists())
                self.assertEqual("old installation", next(backups.glob("graph.*/SKILL.md")).read_text())
                self.assertFalse(sync_agent_skills.copy_source(pointer, target.parent, None, True, True, backups)["changed"])
                (upstream / "engine.py").write_text("VERSION = 2\n", encoding="utf-8")
                self.commit_fixture(upstream)
                manifest_file = pointer / "external-source.json"
                manifest = json.loads(manifest_file.read_text())
                manifest["revision"] = first["external_source"]["revision"]
                manifest_file.write_text(json.dumps(manifest), encoding="utf-8")
                pinned = sync_agent_skills.copy_source(pointer, target.parent, None, True, True, backups)
                self.assertFalse(pinned["changed"])
                manifest.pop("revision")
                manifest_file.write_text(json.dumps(manifest), encoding="utf-8")
                updated = sync_agent_skills.copy_source(pointer, target.parent, None, True, True, backups)
                self.assertNotEqual(first["external_source"]["revision"], updated["external_source"]["revision"])
                self.assertEqual("VERSION = 2\n", (target / "engine.py").read_text())
                self.assertEqual(updated["external_source"], json.loads((target / ".skill-source.json").read_text()))
                self.assertTrue((pointer / "external-source.json").is_file())

    def test_missing_external_resource_leaves_existing_skill_untouched(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            pointer, upstream, local_git = self.make_external_fixture(root)
            (upstream / "engine.py").unlink()
            self.commit_fixture(upstream)
            target = root / "installed/graph"
            target.mkdir(parents=True)
            (target / "SKILL.md").write_text("preserve", encoding="utf-8")
            with patch.object(sync_agent_skills, "run_git", side_effect=local_git), self.assertRaisesRegex(ValueError, "missing required"):
                sync_agent_skills.copy_source(pointer, target.parent, None, True, True)
            self.assertEqual("preserve", (target / "SKILL.md").read_text())
            self.assertFalse((target.parent / ".sync-agent-skills-backups").exists())

    def test_external_manifest_rejects_escaping_requirements_and_self_installation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            pointer, _, _ = self.make_external_fixture(root)
            manifest_file = pointer / "external-source.json"
            manifest = json.loads(manifest_file.read_text())
            manifest["required_files"] = ["../outside"]
            manifest_file.write_text(json.dumps(manifest), encoding="utf-8")
            with patch.object(sync_agent_skills, "run_git") as git:
                with self.assertRaisesRegex(ValueError, "stay inside"):
                    sync_agent_skills.copy_source(pointer, root / "target", None, True, True)
                git.assert_not_called()
            with self.assertRaisesRegex(ValueError, "canonical pointer"):
                sync_agent_skills.copy_source(pointer, pointer.parent, None, True, True)

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

    def test_doctor_adds_only_agents_skill_location(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            settings_path = Path(temporary_directory) / "settings.json"
            settings_path.write_text(
                '{"chat.agentSkillsLocations": {"custom/skills": true}}',
                encoding="utf-8",
            )
            result = sync_agent_skills.doctor_vscode(settings_path, apply=False)
            locations = result["effective_agent_skills_locations"]
            self.assertEqual({"custom/skills": True, "~/.agents/skills": True}, locations)
            self.assertNotIn("~/.codex/skills", locations)

    def test_default_roots_use_agents_not_codex(self) -> None:
        with patch.dict(os.environ, {"CODEX_HOME": r"C:\codex-home"}, clear=False):
            roots = sync_agent_skills.default_roots()
        self.assertEqual(sync_agent_skills.home() / ".agents" / "skills", roots["agents-skills"])
        self.assertNotIn("codex-skills", roots)
        self.assertTrue(all(".codex" not in Path(path).parts for path in roots.values()))

    def test_doctor_refuses_to_rewrite_jsonc(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            settings_path = Path(temporary_directory) / "settings.json"
            settings_path.write_text('{\n  // comment\n  "chat.useAgentSkills": false,\n}\n', encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "refusing to rewrite JSONC"):
                sync_agent_skills.doctor_vscode(settings_path, apply=True)

    def test_cli_rejects_repository_sync_option(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "sync-from-master", "--master", str(root),
                 "--target-root", str(root / "profile"), "--target-repo", str(root / "project"),
                 "--apply"],
                capture_output=True, text=True,
            )
            self.assertEqual(2, result.returncode)
            self.assertIn("unrecognized arguments: --target-repo", result.stderr)
            self.assertEqual([], list(root.iterdir()))

    def test_cli_updates_only_explicit_profile_root(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            master = root / "master"
            profile = root / "user" / ".agents" / "skills"
            project = root / "project" / ".cursor" / "skills"
            for skills_root, version in ((master / "skills", "new"), (project, "keep")):
                skill_dir = skills_root / "sample-skill"
                skill_dir.mkdir(parents=True)
                (skill_dir / "SKILL.md").write_text(
                    f"---\nname: sample-skill\ndescription: {version}\n---\n",
                    encoding="utf-8",
                )
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "sync-from-master", "--master", str(master),
                 "--target-root", str(profile), "--all", "--apply", "--json"],
                capture_output=True, text=True, cwd=root,
            )
            self.assertEqual(0, result.returncode, result.stderr)
            self.assertEqual(1, len(json.loads(result.stdout)))
            self.assertIn("new", (profile / "sample-skill" / "SKILL.md").read_text(encoding="utf-8"))
            self.assertIn("keep", (project / "sample-skill" / "SKILL.md").read_text(encoding="utf-8"))

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
