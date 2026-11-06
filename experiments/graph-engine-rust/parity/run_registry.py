"""Execute every currently registered native original identity, without parity claims."""

import argparse
import json
import hashlib
import os
import subprocess
from pathlib import Path
from bindings import capture


def native_source_snapshot():
    package = Path(__file__).resolve().parent.parent
    return {path.relative_to(package).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in sorted(package.rglob("*")) if path.is_file()
            and (path.suffix in {".rs", ".toml", ".manifest", ".sql"} or path.name == "Cargo.lock")}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--rust-suite", type=Path, required=True)
    parser.add_argument("--inventory", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--auxiliary-binary", type=Path, action="append", default=[])
    args = parser.parse_args()
    inventory = json.loads(args.inventory.read_text(encoding="utf-8"))
    original = {identity for group in inventory["test_groups"].values() for identity in group}
    binary = str(args.rust_suite.resolve())
    auxiliary = [{"path": str(path.resolve(strict=True)), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()} for path in args.auxiliary_binary]
    native_source = native_source_snapshot()
    listing = subprocess.run([binary, "--list"], capture_output=True, check=True)
    identities = listing.stdout.decode("utf-8").splitlines()
    if identities != sorted(set(identities)) or any(identity not in original for identity in identities):
        raise ValueError("REGISTRY_IDENTITY_MISMATCH")
    args.output.mkdir(parents=True, exist_ok=False)
    (args.output / "list.stdout").write_bytes(listing.stdout)
    records = []
    for index, identity in enumerate(identities):
        argv = [binary, "--identity", identity]
        with subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE) as process:
            timed_out = False
            try:
                stdout, stderr = process.communicate(timeout=120)
            except subprocess.TimeoutExpired:
                timed_out = True
                process.kill()
                stdout, stderr = process.communicate()
            (args.output / (str(index)+".stdout")).write_bytes(stdout)
            (args.output / (str(index)+".stderr")).write_bytes(stderr)
            records.append(dict(identity=identity, argv=argv, pid=process.pid, exit=process.returncode, timeout=timed_out))
    report = {"scope": "currently implemented original-identity subset, not full-workload parity",
              "registered": len(identities), "original_count": len(original),
              "remaining_original": sorted(original-set(identities)), "records": records,
              "bindings": capture(args.rust_suite), "native_source": native_source, "auxiliary_binaries": auxiliary,
              "environment": {key: os.environ.get(key) for key in ("TEMP", "TMP", "TMPDIR", "GRAPH_RUST_RUN_ARTIFACT_ROOT", "GRAPH_RUST_ORACLE_PYTHON", "GRAPH_RUST_HELPER_BINARY", "COLUMNS", "PYTHONUTF8", "PYTHONIOENCODING")}}
    if any(hashlib.sha256(Path(item["path"]).read_bytes()).hexdigest() != item["sha256"] for item in auxiliary):
        raise ValueError("AUXILIARY_BINARY_CHANGED_DURING_RUN")
    if native_source_snapshot() != native_source:
        raise ValueError("NATIVE_SOURCE_CHANGED_DURING_RUN")
    (args.output / "report.json").write_text(json.dumps(report, indent=2)+"\n", encoding="utf-8")
    failed = sum(record["exit"] != 0 or record["timeout"] for record in records)
    print(json.dumps(dict(registered=len(identities), failures=failed, remaining_original=len(report["remaining_original"]))))
    return int(bool(failed))


if __name__ == "__main__":
    raise SystemExit(main())
