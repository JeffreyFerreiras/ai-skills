import json
import os
import tempfile
import types
import unittest
from pathlib import Path
from unittest.mock import patch

import server


class ServerConfigurationTests(unittest.TestCase):
    def run_server(self, device):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            model = root / "external-model-store" / "english"
            (root / "config.json").write_text(json.dumps({
                "model_path": str(model), "port": 19000, "device": device,
            }))
            (root / "token.txt").write_text("test-token")
            observed = {}
            service = types.ModuleType("laya.serve")
            service.main = lambda: observed.update(os.environ)
            with patch.object(server, "__file__", str(root / "server.py")):
                with patch.dict("sys.modules", {"laya": types.ModuleType("laya"), "laya.serve": service}):
                    with patch.dict(os.environ, {"LAYA_DEVICE": "inherited-device"}, clear=True):
                        server.main()
            return observed, model

    def test_server_uses_configured_model_and_local_authenticated_offline_endpoint(self):
        environment, model = self.run_server("cpu")
        self.assertEqual("127.0.0.1", environment["LAYA_HOST"])
        self.assertEqual("19000", environment["LAYA_PORT"])
        self.assertEqual("test-token", environment["LAYA_API_KEY"])
        self.assertEqual("1", environment["HF_HUB_OFFLINE"])
        self.assertEqual("1", environment["TRANSFORMERS_OFFLINE"])
        self.assertEqual("cpu", environment["LAYA_DEVICE"])
        self.assertEqual({"hook-english": str(model)}, json.loads(environment["LAYA_EXTRA_MODELS"]))
        self.assertEqual(str(model.parent / ".cache"), environment["HF_HOME"])

    def test_automatic_device_does_not_inherit_a_forced_device(self):
        environment, _ = self.run_server("auto")
        self.assertNotIn("LAYA_DEVICE", environment)


if __name__ == "__main__":
    unittest.main()
