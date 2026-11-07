"""Record selected Python/native SQLite path behavior without parity claims."""

import argparse
import json
import sqlite3
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "parity"))
from bindings import capture


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--rust", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    connection = sqlite3.connect(":memory:")
    options = [row[0] for row in connection.execute("PRAGMA compile_options")]
    requests = [dict(op="sqlite_rows", sql="PRAGMA compile_options", parameters=[])]
    source = []
    for index, length in enumerate((240, 259, 260, 280, 320)):
        parent = args.output.resolve() / ("p" * max(1, length - len(str(args.output.resolve())) - 11))
        path = parent / "test.db"
        parent.mkdir()
        result = dict(path=str(path), length=len(str(path)))
        try:
            database = sqlite3.connect(path)
            database.execute("CREATE TABLE actual_path_probe(value)")
            database.close()
            connection.execute(f"ATTACH DATABASE ? AS p{index}", (str(path),))
            result["ok"] = True
        except Exception as error:
            result.update(ok=False, error=dict(kind=type(error).__name__, message=str(error)))
        source.append(result)
        requests.append(dict(op="sqlite_rows", sql=f"ATTACH DATABASE ? AS p{index}", parameters=[dict(type="value", value=str(path))]))
    connection.close()
    encoded = b"".join((json.dumps(request, sort_keys=True) + "\n").encode("ascii") for request in requests)
    completed = subprocess.run([str(args.rust.resolve())], input=encoded, capture_output=True, timeout=30)
    (args.output / "requests.ndjson").write_bytes(encoded)
    (args.output / "stdout.ndjson").write_bytes(completed.stdout)
    (args.output / "stderr.bin").write_bytes(completed.stderr)
    native = [json.loads(line) for line in completed.stdout.splitlines()]
    report = dict(scope="isolated ART SQLite path and compile-option diagnostic, no workload credit", source_compile_options=options, source=source, native=native, native_exit=completed.returncode, bindings=capture(args.rust))
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="ascii")
    print(json.dumps(dict(native_exit=completed.returncode, source=[dict(length=item["length"], ok=item["ok"]) for item in source], native_ok=[item["ok"] for item in native])))
    return int(completed.returncode != 0 or len(native) != len(requests))


if __name__ == "__main__":
    raise SystemExit(main())
