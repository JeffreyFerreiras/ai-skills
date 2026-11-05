"""Recheck two frozen JSON-input failures using only the native helper CLI."""

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def sources(package):
    result = {}
    for path in sorted(package.rglob("*")):
        if path.is_symlink() or path.is_junction():
            raise ValueError("Refuse linked entries in an authored-source snapshot")
        if path.is_file():
            result[path.relative_to(package).as_posix()] = digest(path)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--native", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        raise FileExistsError("Refuse to replace native traceback evidence")
    package = Path(__file__).resolve().parent.parent
    generator = package / "parity/helper_cli.py"
    original = json.loads(args.report.read_bytes())
    assert digest(generator) == original["generator_sha256"], "Frozen generator binding changed"
    source_before, binary_before = sources(package), digest(args.native)
    reference_before = {"report": digest(args.report)}
    for index in (23, 24):
        reference_before[str(index)] = digest(args.report.parent / f"{index}.oracle.stderr")
    args.output.mkdir(parents=True)
    records = []
    constructors = (
        (23, "context_integer_digit_limit", b"1" * 4301),
        (24, "context_recursion_limit", b"[" * 3005 + b"0" + b"]" * 3005),
    )
    for index, label, payload in constructors:
        expected = original["records"][index]
        assert expected["label"] == label
        context = args.output / f"{index}.context.json"
        context.write_bytes(payload)
        before = digest(context)
        unused_register = args.output / "unused-register.json"
        unused_lock = unused_register.with_suffix(".lock")
        assert not unused_register.exists() and not unused_lock.exists()
        command = [str(args.native), "status", "--register", str(unused_register), "--context", str(context)]
        completed = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=15, check=False)
        (args.output / f"{index}.native.stdout").write_bytes(completed.stdout)
        (args.output / f"{index}.native.stderr").write_bytes(completed.stderr)
        assert digest(context) == before and not unused_register.exists() and not unused_lock.exists()
        source_stderr = base64.b64decode(expected["oracle"]["stderr_base64"])
        assert source_stderr == (args.report.parent / f"{index}.oracle.stderr").read_bytes()
        differences = []
        if completed.returncode != expected["oracle"]["exit_code"]:
            differences.append("exit_code")
        if expected["oracle"]["timeout"]:
            differences.append("timeout")
        if completed.stdout != base64.b64decode(expected["oracle"]["stdout_base64"]):
            differences.append("stdout_bytes")
        if completed.stderr != source_stderr:
            differences.append("stderr_bytes")
        records.append({
            "index": index, "label": label, "command": command,
            "exit_code": completed.returncode, "timeout": False,
            "input_sha256_before": before, "input_sha256_after": digest(context),
            "input_bytes": len(payload), "original_per_case_input_sha256": None,
            "input_binding": "Exact literal constructor from hash-matching frozen generator; the old report did not retain a per-case input hash and reused one overwritten path.",
            "source_stderr_sha256": hashlib.sha256(source_stderr).hexdigest(),
            "native_stderr_sha256": hashlib.sha256(completed.stderr).hexdigest(),
            "source_stderr_bytes": len(source_stderr), "native_stderr_bytes": len(completed.stderr),
            "register_and_lock_unchanged_absent": True,
            "differences": differences,
        })
    assert sources(package) == source_before, "Native source changed during replay"
    assert digest(args.native) == binary_before, "Native binary changed during replay"
    assert digest(args.report) == reference_before["report"]
    for index in (23, 24):
        assert digest(args.report.parent / f"{index}.oracle.stderr") == reference_before[str(index)]
    result = {
        "schema_version": 1, "scope": "Two reconstructed context JSON-input failures; exact retained source stdout/stderr/exit without normalization. No Python engine execution.",
        "records": records, "source_before_after_equal": True,
        "source_sha256": source_before, "native_binary_sha256_before_after": binary_before,
        "reference_sha256_before_after": reference_before,
        "environment": {key: os.environ.get(key) for key in ("TEMP", "TMP", "COLUMNS", "PYTHONIOENCODING", "PYTHONUTF8")},
        "generator_sha256": digest(generator), "replay_sha256": digest(__file__),
        "original_promotions": 0,
        "remaining": "Other helper input sites and other unexpected traceback families are not proved by these two context cases; full GraphCase fixtures remain separate.",
    }
    (args.output / "report.json").write_text(json.dumps(result, ensure_ascii=True, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    count = sum(bool(record["differences"]) for record in records)
    print(json.dumps({"comparisons": len(records), "differences": count, "oracle_calls": 0}))
    return int(bool(count))


if __name__ == "__main__":
    raise SystemExit(main())
