"""Read only explicit generated ART paths under a scoped working directory."""

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
    args = parser.parse_args()
    artifact = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve()
    output = args.output.resolve()
    output.relative_to(artifact)
    output.mkdir(parents=True, exist_ok=False)
    fixtures = (artifact / "fixtures").resolve()
    root = fixtures / ("usage-paths-source-" + secrets.token_hex(12))
    root.mkdir()
    nested = root / "nested"
    nested.mkdir()
    source = root / "synthetic-source.generated"
    source.write_bytes(b"synthetic-owned-prefix\n")
    root.resolve().relative_to(fixtures)
    repo = Path(__file__).resolve().parents[3]
    sys.path.insert(0, str(repo / "skills/software-engineering-graph"))
    from graph_engine import usage
    drive = source.drive
    inputs = [str(source), source.as_posix(), "../synthetic-source.generated", "..\\synthetic-source.generated",
              "./..//synthetic-source.generated", "../missing/../synthetic-source.generated",
              drive + "..\\synthetic-source.generated", drive + "..\\missing\\..\\synthetic-source.generated",
              str(source)[len(drive):], "", ".", drive, drive + ".", "missing-source.generated"]
    if os.name != "nt" or len(inputs) != 14:
        raise RuntimeError("this bounded reference requires selected Windows Python")
    previous = Path.cwd()
    records = []
    original_safe = usage._safe_components
    try:
        os.chdir(nested)
        for value in inputs:
            absolute = os.path.abspath(value)
            Path(absolute).resolve().relative_to(root.resolve())
            observed = []

            def safe(path):
                observed.append(str(path))
                return original_safe(path)

            with patch.object(usage, "_safe_components", side_effect=safe):
                data, diagnostics = usage._read_source(value)
            records.append({"input": value, "absolute": absolute, "safe_component_arguments": observed,
                            "returned_length": len(data), "returned_sha256": hashlib.sha256(data).hexdigest(),
                            "diagnostics": diagnostics})
    finally:
        os.chdir(previous)
    report = {"cases": len(records), "root": str(root), "cwd": str(nested), "records": records,
              "oracle_python": sys.executable, "python_version": sys.version,
              "oracle_usage_sha256": hashlib.sha256(Path(usage.__file__).read_bytes()).hexdigest(),
              "generator_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "scope": "all resolved inputs in owned ART fixture; working directory and patches restored; fixture retained"}
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"cases": len(records), "summary": [{"index": index, "length": item["returned_length"], "diagnostics": item["diagnostics"]} for index, item in enumerate(records)]}))


if __name__ == "__main__":
    main()
