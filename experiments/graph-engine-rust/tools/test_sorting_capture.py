"""Exercise raw bounded capture with three native process-fixture children only."""

import json
import os
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "parity"))
import run_sorting


class CaptureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root = Path(os.environ["GRAPH_RUST_CAPTURE_TEST_OUTPUT"])
        assert not cls.root.exists(), "preserve prior outputs"
        cls.root.mkdir()
        cls.fixture = os.environ["GRAPH_RUST_PROCESS_FIXTURE"]

    def capture(self, mode, *, limit, timeout=5):
        directory = self.root / mode
        directory.mkdir()
        with patch.object(run_sorting, "MAX_OUTPUT", limit):
            result = run_sorting.run_child([self.fixture, mode], input_bytes=None,
                                          directory=directory, environment=dict(os.environ),
                                          prefix="native", timeout=timeout)
        (directory / "report.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        return result, directory

    def test_binary_streams_and_nonzero_exit_are_preserved(self):
        result, directory = self.capture("emit", limit=1024)
        self.assertEqual(result["exit"], 7)
        self.assertTrue(result["raw_complete"])
        self.assertFalse(result["timed_out"])
        self.assertEqual((directory / "native.stdout.bin").read_bytes(), b"out\0\xff")
        self.assertEqual((directory / "native.stderr.bin").read_bytes(), b"err\0\xfe")

    def test_live_overflow_kills_child_and_marks_incomplete(self):
        result, directory = self.capture("flood", limit=1024)
        self.assertFalse(result["raw_complete"])
        self.assertFalse(result["timed_out"])
        self.assertTrue(result["capture"]["stdout"]["overflow"])
        self.assertEqual((directory / "native.stdout.bin").read_bytes(), b"x" * 1024)
        sample = (directory / "native.stdout.overflow.bin").read_bytes()
        self.assertGreater(len(sample), 0)
        self.assertLessEqual(len(sample), 65536)
        self.assertNotEqual(result["exit"], 0)

    def test_timeout_retains_partial_raw_bytes(self):
        result, directory = self.capture("delay", limit=1024, timeout=1)
        self.assertTrue(result["timed_out"])
        self.assertNotEqual(result["exit"], 0)
        self.assertTrue((directory / "native.stdout.bin").read_bytes().startswith(b"ready:"))
        self.assertEqual((directory / "native.stderr.bin").read_bytes(), b"waiting")


if __name__ == "__main__":
    unittest.main()
