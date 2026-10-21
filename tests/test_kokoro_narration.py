from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch


SCRIPT = Path(__file__).resolve().parents[1] / "skills/system-design-video/scripts/kokoro_narration.py"
SPEC = importlib.util.spec_from_file_location("kokoro_narration", SCRIPT)
narration = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(narration)


class KokoroNarrationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.model = self.root / "model"
        (self.model / "voices").mkdir(parents=True)
        (self.model / "voices/af_heart.bin").write_bytes(b"embedding")
        self.config = self.root / "config.json"
        self.value = {"engine": "kokoro-openvino", "device": "CPU", "python": sys.executable,
                      "model_dir": str(self.model), "voice": None}
        self.save_config()

    def save_config(self):
        self.config.write_text(json.dumps(self.value), encoding="utf-8")

    def settings(self):
        return narration.settings_from_file(self.config, voice="af_heart")

    def test_voice_is_required_and_no_model_level_approval_is_inferred(self):
        with self.assertRaisesRegex(ValueError, "explicit Kokoro voice"):
            narration.settings_from_file(self.config)
        self.assertEqual("af_heart", self.settings()["voice"])
        with self.assertRaisesRegex(ValueError, "Missing Kokoro voice"):
            narration.settings_from_file(self.config, voice="af_bella")

    def test_non_cpu_devices_and_invalid_resource_bounds_are_rejected(self):
        for update in ({"device": "AUTO"}, {"device": "NPU"}, {"device": "GPU"},
                       {"threads": 0}, {"threads": 5}, {"threads": True},
                       {"timeout_seconds": 0}, {"timeout_seconds": 3601}, {"speed": float("nan")}, {"speed": True}):
            with self.subTest(update=update):
                original = self.value.copy()
                self.value.update(update)
                self.save_config()
                with self.assertRaises(ValueError):
                    self.settings()
                self.value = original

    def test_connected_take_is_default_and_gap_mode_uses_explicit_boundaries(self):
        text = "Use HTTP one point one.\n\nThen send a response."
        self.assertEqual([text], narration.passages(text, None))
        self.assertEqual(["Use HTTP one point one.", "Then send a response."], narration.passages(text, 550))
        for text in (" ", "a" * 10001):
            with self.assertRaises(ValueError):
                narration.passages(text, None)

    def test_asset_manifest_detects_modified_model_or_voice_bytes(self):
        names = ["config.json", "openvino_model.xml", "openvino_model.bin", "voices/af_heart.bin"]
        for name in names[:3]:
            (self.model / name).write_bytes(name.encode())
        manifest = {"files": [{"path": name, "sha256": narration.sha256(self.model / name)} for name in names]}
        (self.model / "asset-manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
        self.assertEqual(manifest, narration.verify_assets(self.model))
        (self.model / "voices/af_heart.bin").write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "asset missing, changed"):
            narration.verify_assets(self.model)

    def test_existing_output_and_active_lock_are_preserved(self):
        output = self.root / "existing"
        output.mkdir()
        marker = output / "native.wav"
        marker.write_bytes(b"approved")
        with self.assertRaises(FileExistsError):
            narration.run(self.settings(), "A reviewed sentence.", output)
        self.assertEqual(b"approved", marker.read_bytes())
        lock = self.config.with_suffix(".lock")
        lock.write_text("another owner", encoding="utf-8")
        with self.assertRaises(FileExistsError):
            narration.run(self.settings(), "A reviewed sentence.", self.root / "new")
        self.assertEqual("another owner", lock.read_text(encoding="utf-8"))

    def test_timeout_has_no_retry_and_releases_only_owned_lock(self):
        output = self.root / "timeout"
        process = Mock(pid=123)
        process.wait.side_effect = [subprocess.TimeoutExpired("worker", 120), -9]
        def launch(*args, **kwargs):
            kwargs["stderr"].write(b"partial")
            return process
        with patch.object(subprocess, "Popen", side_effect=launch) as worker:
            with self.assertRaisesRegex(RuntimeError, "killed/waited; no retry"):
                narration.run(self.settings(), "A reviewed sentence.", output)
        self.assertEqual(1, worker.call_count)
        self.assertEqual(120, process.wait.call_args_list[0].kwargs["timeout"])
        process.kill.assert_called_once()
        self.assertEqual(15, process.wait.call_args_list[1].kwargs["timeout"])
        self.assertEqual("", worker.call_args.kwargs["env"]["CUDA_VISIBLE_DEVICES"])
        self.assertFalse(self.config.with_suffix(".lock").exists())
        self.assertEqual("partial", (output / "worker-stderr.log").read_text(encoding="utf-8"))
        receipt = json.loads((output / "supervision.json").read_text(encoding="utf-8"))
        self.assertEqual("timed_out_killed_waited", receipt["outcome"])
        self.assertEqual(0, receipt["automatic_retries"])

    def test_failed_cpu_worker_is_not_retried_or_relabelled_as_success(self):
        process = Mock(pid=123)
        process.wait.return_value = 1
        with patch.object(subprocess, "Popen", return_value=process) as worker:
            with self.assertRaisesRegex(RuntimeError, "CPU worker exited 1"):
                narration.run(self.settings(), "A reviewed sentence.", self.root / "failure")
        self.assertEqual(1, worker.call_count)

    def test_cancellation_retains_direct_logs_and_records_settled_outcome(self):
        output = self.root / "cancelled"
        process = Mock(pid=123)
        process.wait.side_effect = [KeyboardInterrupt(), -9]
        def launch(*args, **kwargs):
            kwargs["stdout"].write(b"diagnostic before cancellation")
            return process
        with patch.object(subprocess, "Popen", side_effect=launch):
            with self.assertRaises(KeyboardInterrupt):
                narration.run(self.settings(), "A reviewed sentence.", output)
        self.assertEqual(b"diagnostic before cancellation", (output / "worker-stdout.log").read_bytes())
        process.kill.assert_called_once()
        self.assertFalse(self.config.with_suffix(".lock").exists())
        receipt = json.loads((output / "supervision.json").read_text(encoding="utf-8"))
        self.assertEqual("interrupted_killed_waited", receipt["outcome"])

    def test_interrupted_launch_without_handle_retains_lock_and_records_uncertainty(self):
        output = self.root / "launch-interrupted"
        with patch.object(subprocess, "Popen", side_effect=KeyboardInterrupt()) as worker:
            with self.assertRaisesRegex(RuntimeError, "launch outcome is uncertain; lock retained"):
                narration.run(self.settings(), "A reviewed sentence.", output)
        self.assertEqual(1, worker.call_count)
        self.assertTrue(self.config.with_suffix(".lock").exists())
        receipt = json.loads((output / "supervision.json").read_text(encoding="utf-8"))
        self.assertEqual("unknown_lock_retained", receipt["outcome"])
        self.assertIsNone(receipt["worker_pid"])
        self.assertEqual(0, receipt["automatic_retries"])
        with self.assertRaises(FileExistsError):
            narration.run(self.settings(), "A reviewed sentence.", self.root / "retry")

    def test_uncertain_worker_termination_retains_lock_and_prevents_retry(self):
        output = self.root / "unknown"
        process = Mock(pid=123)
        process.wait.side_effect = subprocess.TimeoutExpired("worker", 120)
        process.kill.side_effect = OSError("termination failed")
        with patch.object(subprocess, "Popen", return_value=process) as worker:
            with self.assertRaisesRegex(RuntimeError, "outcome is uncertain; lock retained"):
                narration.run(self.settings(), "A reviewed sentence.", output)
        self.assertTrue(self.config.with_suffix(".lock").exists())
        with self.assertRaises(FileExistsError):
            narration.run(self.settings(), "A reviewed sentence.", self.root / "retry")
        self.assertEqual(1, worker.call_count)
        receipt = json.loads((output / "supervision.json").read_text(encoding="utf-8"))
        self.assertEqual("unknown_lock_retained", receipt["outcome"])


if __name__ == "__main__":
    unittest.main()
