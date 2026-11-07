"""Observe original buffered prefix checks using only owned generated ART files."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import sys
from unittest.mock import patch


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--append-before-read", action="store_true")
    args = parser.parse_args()
    artifact = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve()
    output = args.output.resolve()
    output.relative_to(artifact)
    output.mkdir(parents=True, exist_ok=False)
    fixtures = (artifact / "fixtures").resolve()
    owned = fixtures / ("usage-buffer-" + secrets.token_hex(12))
    owned.mkdir()
    owned.resolve().relative_to(fixtures)
    repo = Path(__file__).resolve().parents[3]
    sys.path.insert(0, str(repo / "skills/software-engineering-graph"))
    from graph_engine import usage
    original_fdopen, original_fstat = os.fdopen, os.fstat
    records = []
    sizes = [1, 8191, 8192, 8193] if args.append_before_read else [0, 1, 8191, 8192, 8193, 16383, 16384, 16385, 1048576, 1048577]
    for size in sizes:
        for mutate in (False, True):
            path = owned / (str(len(records)) + ".generated")
            initial = b"A" * size
            path.write_bytes(initial)
            events = []

            class Proxy:
                def __init__(self, handle):
                    self.handle = handle
                    self.first_read = True

                def __enter__(self):
                    self.handle.__enter__()
                    return self

                def __exit__(self, *arguments):
                    events.append({"operation": "close"})
                    return self.handle.__exit__(*arguments)

                def fileno(self):
                    return self.handle.fileno()

                def read(self, amount):
                    if self.first_read and args.append_before_read:
                        with path.open("ab", buffering=0) as writer:
                            writer.write(b"C" * 16)
                        events.append({"operation": "append_before_first_read", "length": 16})
                    self.first_read = False
                    result = self.handle.read(amount)
                    events.append({"operation": "read", "requested": amount, "length": len(result),
                                   "sha256": hashlib.sha256(result).hexdigest()})
                    return result

                def seek(self, offset):
                    if mutate:
                        if size:
                            with path.open("r+b", buffering=0) as writer:
                                writer.write(b"B")
                            events.append({"operation": "overwrite_first_sampled_byte", "offset": 0})
                        else:
                            events.append({"operation": "not_applicable_no_sampled_byte"})
                    result = self.handle.seek(offset)
                    events.append({"operation": "seek", "requested": offset, "result": result})
                    return result

            def fdopen(descriptor, mode):
                events.append({"operation": "fdopen", "mode": mode})
                return Proxy(original_fdopen(descriptor, mode))

            def fstat(descriptor):
                result = original_fstat(descriptor)
                events.append({"operation": "fstat", "size": result.st_size, "mode": result.st_mode,
                               "device": result.st_dev, "inode": result.st_ino})
                return result

            with patch.object(usage.os, "fdopen", fdopen), patch.object(usage.os, "fstat", fstat):
                data, diagnostics = usage._read_source(str(path))
            post = path.read_bytes()
            records.append({"index": len(records), "size": size, "mutate_before_seek": mutate,
                            "input_sha256": hashlib.sha256(initial).hexdigest(),
                            "returned_length": len(data), "returned_sha256": hashlib.sha256(data).hexdigest(),
                            "diagnostics": diagnostics, "post_length": len(post),
                            "post_sha256": hashlib.sha256(post).hexdigest(), "events": events})
            path.resolve().relative_to(owned.resolve())
            path.unlink()
    owned.resolve().relative_to(fixtures)
    owned.rmdir()
    report = {"cases": len(records), "append_before_read": args.append_before_read, "records": records, "oracle_python": sys.executable,
              "python_version": sys.version, "oracle_script": str(Path(usage.__file__).resolve()),
              "oracle_script_sha256": hashlib.sha256(Path(usage.__file__).read_bytes()).hexdigest(),
              "generator_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "fixture_scope": str(owned), "teardown": "closed descriptors; contained generated files removed"}
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"cases": len(records), "summary": [{
        "size": item["size"], "mutate": item["mutate_before_seek"],
        "returned_length": item["returned_length"], "diagnostics": item["diagnostics"]} for item in records]}))


if __name__ == "__main__":
    main()
