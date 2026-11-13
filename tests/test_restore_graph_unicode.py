import importlib.util
import tempfile
import unittest
import unicodedata
from pathlib import Path
from unittest.mock import patch


SOURCE = Path(__file__).resolve().parents[1] / "experiments/graph-engine-rust/tools/restore_unicode.py"
SPEC = importlib.util.spec_from_file_location("restore_unicode", SOURCE)
restore = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(restore)


class RestoreUnicodeTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)

    def test_rejects_modified_manifest_before_reconstruction(self):
        manifest = self.root / "manifest.json"
        manifest.write_bytes(restore.DEFAULT_MANIFEST.read_bytes() + b"\n")
        with patch.object(restore, "reconstruct") as reconstruct:
            with self.assertRaisesRegex(ValueError, "MANIFEST_DIGEST_MISMATCH"):
                restore.restore(manifest, self.root)
            reconstruct.assert_not_called()

    def test_rejects_incompatible_table_without_writing(self):
        with patch.object(restore, "reconstruct", return_value=b"wrong"):
            with self.assertRaisesRegex(ValueError, "INPUT_DIGEST_MISMATCH"):
                restore.restore(restore.DEFAULT_MANIFEST, self.root)
        self.assertFalse((self.root / "compat").exists())

    def test_preserves_existing_invalid_artifact(self):
        output = self.root / restore.INPUT_PATH
        output.parent.mkdir(parents=True)
        output.write_bytes(b"keep this")
        with self.assertRaisesRegex(ValueError, "INPUT_DIGEST_MISMATCH"):
            restore.restore(restore.DEFAULT_MANIFEST, self.root)
        self.assertEqual(output.read_bytes(), b"keep this")

    @unittest.skipUnless(unicodedata.unidata_version == "15.0.0", "Requires frozen Unicode version")
    def test_restores_frozen_bytes_and_reuses_verified_output(self):
        output = restore.restore(restore.DEFAULT_MANIFEST, self.root)
        self.assertEqual(output.stat().st_size, 2253079)
        with patch.object(restore, "reconstruct") as reconstruct:
            self.assertEqual(restore.restore(restore.DEFAULT_MANIFEST, self.root), output)
            reconstruct.assert_not_called()

    def test_rejects_relative_root(self):
        with self.assertRaisesRegex(ValueError, "ABSOLUTE_REQUIRED"):
            restore.restore(restore.DEFAULT_MANIFEST, Path("relative"))


if __name__ == "__main__":
    unittest.main()
