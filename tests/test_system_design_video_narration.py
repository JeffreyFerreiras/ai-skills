from __future__ import annotations

import importlib.util
import subprocess
import sys
import unittest
from pathlib import Path
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[1] / "skills/system-design-video/scripts/narration_probe.py"
SPEC = importlib.util.spec_from_file_location("narration_probe", SCRIPT)
probe_module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(probe_module)


class DeviceDiscoveryTests(unittest.TestCase):
    def test_detecting_npu_never_claims_execution_or_gate_pass(self):
        class FakeCore:
            available_devices = ["CPU", "NPU", "GPU.0"]

            def get_property(self, device, key):
                if key == "DRIVER_VERSION":
                    raise RuntimeError("property unsupported")
                return {"CPU": "Intel CPU", "NPU": "Intel AI Boost", "GPU.0": "Intel GPU"}[device]

        report = probe_module.probe(FakeCore)
        self.assertEqual("discovered", report["status"])
        self.assertEqual(["CPU", "NPU", "GPU.0"], [item["id"] for item in report["devices"]])
        self.assertIn("DRIVER_VERSION", report["devices"][1]["property_errors"])
        self.assertIsNone(report["execution_device"])
        self.assertFalse(report["cpu_fallback_executed"])
        self.assertEqual("pending", report["npu_validation"])

    def test_plugin_failure_is_unavailable_without_inventing_cpu_fallback(self):
        def broken_core():
            raise RuntimeError("driver failed")

        report = probe_module.probe(broken_core)
        self.assertEqual("unavailable", report["status"])
        self.assertEqual([], report["devices"])
        self.assertEqual("driver failed", report["error"])
        self.assertFalse(report["cpu_fallback_executed"])

    def test_missing_runtime_is_reported_without_installing(self):
        with patch.dict(sys.modules, {"openvino": None}):
            report = probe_module.probe()
        self.assertEqual("unavailable", report["status"])
        self.assertIsNone(report["execution_device"])

    def test_timeout_is_bounded_and_leaves_hardware_validation_pending(self):
        with patch.object(subprocess, "run", side_effect=subprocess.TimeoutExpired("probe", 3)) as run:
            report = probe_module.bounded_probe(3)
        self.assertEqual(3, run.call_args.kwargs["timeout"])
        self.assertEqual("error", report["status"])
        self.assertEqual("pending", report["npu_validation"])
        self.assertFalse(report["cpu_fallback_executed"])

    def test_invalid_worker_output_is_an_error(self):
        result = subprocess.CompletedProcess([], 0, stdout="driver output", stderr="")
        with patch.object(subprocess, "run", return_value=result):
            self.assertEqual("error", probe_module.bounded_probe(3)["status"])

    def test_cli_rejects_unbounded_deadlines(self):
        result = subprocess.run([sys.executable, str(SCRIPT), "--timeout", "0"],
                                capture_output=True, text=True, timeout=10)
        self.assertEqual(2, result.returncode)
        self.assertIn("between 1 and 60", result.stderr)


if __name__ == "__main__":
    unittest.main()
