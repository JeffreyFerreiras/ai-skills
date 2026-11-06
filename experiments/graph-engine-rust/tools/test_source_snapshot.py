"""Keep non-Rust build assets in source-bound verification evidence."""

import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "parity"))
from replay_helper_traceback import sources


class SourceSnapshotTests(unittest.TestCase):
    def test_sql_windows_manifest_and_unlisted_extensions_are_guarded(self):
        fixtures = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve(strict=True) / "fixtures"
        fixtures.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=fixtures) as temporary:
            root = Path(temporary).resolve(strict=True)
            root.relative_to(fixtures.resolve(strict=True))
            names = ["Cargo.lock", "src/lib.rs", "src/infrastructure/state_schema.sql", "compat/windows.manifest", "new-build-input.unlisted"]
            for name in names:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(name.encode("ascii"))
            before = sources(root)
            self.assertEqual(set(before), set(names))
            for name in names[2:]:
                (root / name).write_bytes(b"modified input")
                self.assertNotEqual(sources(root)[name], before[name])


if __name__ == "__main__":
    unittest.main()
