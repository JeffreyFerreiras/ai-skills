"""Bounded tempfile reference calls with every filesystem candidate fixed below ART."""

import contextlib
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
from unittest.mock import patch


def outcome(action):
    try:
        return {"ok": True, "result": action()}
    except Exception as error:
        return {"ok": False, "error": {"kind": type(error).__name__, "message": str(error), "errno": getattr(error, "errno", None), "winerror": getattr(error, "winerror", None), "filename": getattr(error, "filename", None)}}


class Names:
    def __init__(self, values):
        self.values, self.calls = values, 0

    def __next__(self):
        value = self.values[min(self.calls, len(self.values) - 1)]
        self.calls += 1
        return value


@contextlib.contextmanager
def environment(values):
    previous = {key: os.environ.get(key) for key in values}
    try:
        for key, value in values.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value
        yield
    finally:
        for key, value in previous.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value


def main():
    artifact = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve(strict=True)
    output = artifact / "parity/helper-tempfile-01/report.json"
    if output.exists():
        raise ValueError("IMMUTABLE_REFERENCE_EXISTS")
    root = artifact / "fixtures" / ("helper-tempfile-reference-" + str(os.getpid()) + "-" + os.urandom(8).hex())
    root.mkdir(parents=True, mode=0o700)
    original_cwd = Path.cwd()
    records = []
    try:
        os.chdir(artifact)
        base_env = {"TMPDIR": str(root / "A"), "TEMP": str(root / "B"), "TMP": str(root / "C"), "USERPROFILE": str(root / "home"), "SYSTEMROOT": str(root / "system"), "HOMEDRIVE": "C:", "HOMEPATH": str(root / "alternative-home")[2:]}
        for changes in ({}, {"TMPDIR": "", "TEMP": ""}, {"TMPDIR": ".", "USERPROFILE": "", "SYSTEMROOT": None}, {"USERPROFILE": None}):
            selected = {**base_env, **changes}
            with environment(selected):
                expected = {"ok": True, "result": tempfile._candidate_tempdir_list(), "attempts": 0}
            records.append({"input": {"op": "candidates", "environment": selected}, "expected": expected})

        not_directory = root / "file"; not_directory.write_bytes(b"sentinel")
        good = root / "good"; good.mkdir(); (good / "collide0").write_bytes(b"sentinel"); (good / "occupy00").mkdir()
        first, second = root / "first", root / "second"; first.mkdir(); second.mkdir(); (first / "collide0").write_bytes(b"sentinel")
        unicode_dir = root / "unicode Σ"; unicode_dir.mkdir()
        cases = [([], ["probe000"]), ([str(root / "missing")], ["probe000"]), ([str(not_directory)], ["probe000"]), ([str(good)], ["collide0", "occupy00", "probe000"]), ([str(first), str(second)], ["collide0"]), ([str(unicode_dir) + os.sep + "."], ["probe000"])]

        def validate(paths):
            for path in paths:
                if os.path.commonpath((str(artifact), os.path.abspath(path))) != str(artifact):
                    raise ValueError("CANDIDATE_OUTSIDE_ART")

        for paths, values in cases:
            validate(paths); names = Names(values)
            with patch.object(tempfile, "_candidate_tempdir_list", return_value=paths), patch.object(tempfile, "_RandomNameSequence", return_value=names):
                expected = outcome(tempfile._get_default_tempdir)
            expected["attempts"] = names.calls
            records.append({"input": {"op": "default", "paths": paths, "names": values}, "expected": expected})

        for path_groups in ([[str(second)], [str(unicode_dir)]], [[], [str(second)]]):
            for paths in path_groups:
                validate(paths)
            names = Names(["probe000"]); results = []; selected = []
            with patch.object(tempfile, "tempdir", None), patch.object(tempfile, "_candidate_tempdir_list", side_effect=lambda: selected), patch.object(tempfile, "_RandomNameSequence", return_value=names):
                for paths in path_groups:
                    selected = paths
                    results.append(outcome(tempfile._gettempdir))
            records.append({"input": {"op": "cache", "path_groups": path_groups, "names": ["probe000"]}, "expected": {"ok": True, "result": results, "attempts": names.calls}})
    finally:
        os.chdir(original_cwd)
    attempts = sum(record["expected"]["attempts"] for record in records)
    if len(records) != 12 or attempts > 210:
        raise ValueError("REFERENCE_SCOPE_EXCEEDED")
    result = {"scope": "12 direct in-process cases; filesystem candidate lists patched explicitly to generated ART paths; no global OS fallback or subprocess", "oracle": {"executable": sys.executable, "version": sys.version, "tempfile_source_sha256_previously_verified": "561adb9a999bfe6a98af76c7acb41d3e811d5a0bafbf86e78899548e00479d4b", "bin_flags": tempfile._bin_openflags, "TMP_MAX": tempfile.TMP_MAX}, "generator_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), "fixture_root": str(root), "cwd": str(artifact), "attempts": attempts, "records": records}
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes((json.dumps(result, ensure_ascii=True, sort_keys=True, indent=2) + "\n").encode("ascii"))
    print(json.dumps({"output": str(output), "cases": len(records), "attempts": attempts, "errors": sum(not record["expected"]["ok"] for record in records), "sha256": hashlib.sha256(output.read_bytes()).hexdigest()}, sort_keys=True))


if __name__ == "__main__":
    main()
