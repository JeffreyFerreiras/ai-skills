"""Owned fixtures for checkpoint input identity and collision checks."""

import os
import tempfile
import unittest
from pathlib import Path

from copy_checkpoint_inputs import copy_inputs, sha256, verify_copies


class CheckpointCopyTests(unittest.TestCase):
    def setUp(self):
        parent = (Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]) / "fixtures").resolve(strict=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="checkpoint-copy-", dir=parent)
        self.root = Path(self.temporary.name).resolve(strict=True)
        self.assertEqual(self.root.parent, parent)
        self.parent = parent

    def tearDown(self):
        self.assertEqual(self.root.resolve(strict=True).parent, self.parent)
        self.temporary.cleanup()

    def records(self, values):
        records = []
        for index, value in enumerate(values):
            directory = self.root / str(index)
            directory.mkdir()
            source = directory / "source.stdout.bin"
            source.write_bytes(value)
            records.append({"source": str(source), "path": source.name, "sha256": sha256(source)})
        return records

    def test_duplicate_basenames_preserve_distinct_and_identical_bytes(self):
        originals = self.records([b"sorting\n", b"tuple\n", b"", b""])
        destination = self.root / "archive"
        archived = copy_inputs(originals, destination)
        self.assertEqual(len(archived), 4)
        self.assertEqual(len({record["archive"] for record in archived}), 4)
        self.assertEqual(
            [(destination / record["archive"]).read_bytes() for record in archived],
            [b"sorting\n", b"tuple\n", b"", b""],
        )
        verify_copies(archived, destination)

    def test_final_verification_rejects_collisions_and_late_changes(self):
        archived = copy_inputs(self.records([b"first", b"second"]), self.root / "archive")
        collision = [dict(record) for record in archived]
        collision[1]["archive"] = collision[0]["archive"]
        with self.assertRaisesRegex(ValueError, "collision"):
            verify_copies(collision, self.root / "archive")
        (self.root / "archive" / archived[0]["archive"]).write_bytes(b"later")
        with self.assertRaisesRegex(ValueError, "hash mismatch"):
            verify_copies(archived, self.root / "archive")

    def test_existing_archive_is_preserved_and_not_overwritten(self):
        records = self.records([b"original"])
        destination = self.root / "archive"
        archived = copy_inputs(records, destination)
        with self.assertRaises(FileExistsError):
            copy_inputs(records, destination)
        self.assertEqual((destination / archived[0]["archive"]).read_bytes(), b"original")


if __name__ == "__main__":
    unittest.main()
