"""Compare the four helper JSON-input sites on fresh, guarded failure inputs."""

import argparse
import base64
import json
import os
from pathlib import Path
import re
import subprocess
import sys

from replay_helper_traceback import digest, sources


FRAME = re.compile(rb'^  File "(.*)", line ([0-9]+), in ([^\r\n]*)(\r?\n)$')
FUNCTIONS = [b"<module>", b"main", b"execute", b"_input", b"safe_json_snapshot", b"loads", b"decode", b"raw_decode"]
SUFFIXES = [b"scripts/helper-register.py"] * 4 + [b"graph_engine/contracts.py", b"json/__init__.py", b"json/decoder.py", b"json/decoder.py"]


def traceback_parts(stderr):
    lines, paths, functions = [], [], []
    valid = stderr.startswith(b"Traceback (most recent call last):")
    carets = 0
    for line in stderr.splitlines(keepends=True):
        matched = FRAME.fullmatch(line)
        if matched:
            path, number, function, newline = matched.groups()
            index = len(paths)
            valid = valid and index < len(SUFFIXES) and path.replace(b"\\", b"/").endswith(b"/" + SUFFIXES[min(index, len(SUFFIXES) - 1)])
            paths.append(base64.b64encode(path).decode("ascii"))
            functions.append(function)
            lines.append(b'  File "<variable-source-prefix>/' + SUFFIXES[min(index, len(SUFFIXES) - 1)] + b'", line ' + number + b", in " + function + newline)
        else:
            lines.append(line)
        stripped = line.strip()
        if stripped and b"^" in stripped and not stripped.strip(b"^~"):
            carets += 1
    valid = valid and functions == FUNCTIONS and carets == 8
    return b"".join(lines), paths, valid


def fixture_snapshot(directory):
    result = {}
    for path in sorted(directory.rglob("*")):
        if path.is_file():
            stat = path.stat()
            result[path.relative_to(directory).as_posix()] = {
                "sha256": digest(path), "bytes": stat.st_size, "mtime_ns": stat.st_mtime_ns,
                "device": stat.st_dev, "inode": stat.st_ino,
            }
        elif path.is_dir():
            result[path.relative_to(directory).as_posix() + "/"] = "directory"
        else:
            raise ValueError("Unexpected fixture file type")
    return result


def run(command, environment):
    try:
        completed = subprocess.run(command, env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=15, check=False)
        return completed.stdout, completed.stderr, completed.returncode, False
    except subprocess.TimeoutExpired as error:
        return error.stdout or b"", error.stderr or b"", None, True


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle", type=Path, required=True)
    parser.add_argument("--native", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    package = Path(__file__).resolve().parent.parent
    artifacts = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve(strict=True)
    oracle = args.oracle.resolve(strict=True)
    native = args.native.resolve(strict=True)
    output = args.output.resolve()
    assert oracle == (package.parent.parent / "skills/software-engineering-graph").resolve(strict=True)
    assert native == (artifacts / "build/target/debug/helper-register.exe").resolve(strict=True)
    assert Path(sys.executable).resolve(strict=True) == Path("C:/Python312/python.exe").resolve(strict=True)
    output.relative_to(artifacts)
    if output.exists():
        raise FileExistsError("Refuse an unknown or repeated reference run")
    constructors = [("integer_digit_limit", b"1" * 4301, "ValueError"), ("recursion_limit", b"[" * 3005 + b"0" + b"]" * 3005, "RecursionError")]
    cases = [(site, label, payload, kind) for site in ("context", "preflight_request", "reserve_request", "settle_settlement") for label, payload, kind in constructors]
    assert len(cases) == 8 and all(len(case[2]) <= 256 * 1024 for case in cases)
    script = oracle / "scripts/helper-register.py"
    oracle_before = {path.relative_to(oracle).as_posix(): digest(path) for path in sorted((oracle / "graph_engine").glob("*.py"))}
    oracle_before["scripts/helper-register.py"] = digest(script)
    python_before, native_before = digest(sys.executable), digest(native)
    source_before = sources(package)
    environment = os.environ.copy()
    environment["PYTHONDONTWRITEBYTECODE"] = "1"
    environment["PYTHONINTMAXSTRDIGITS"] = "4300"
    environment.pop("PYTHONNODEBUGRANGES", None)
    output.mkdir(parents=True)
    logs = output / "streams"
    logs.mkdir()
    records, source_calls, native_calls = [], 0, 0
    planned = {"cases": [{"site": site, "label": label, "payload_bytes": len(payload), "kind": kind} for site, label, payload, kind in cases], "maximum_source_calls": 8, "maximum_native_calls": 8, "python": sys.executable, "python_sha256": python_before, "script_sha256": oracle_before["scripts/helper-register.py"], "source_sha256": source_before}
    (output / "planned.json").write_text(json.dumps(planned, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    for index, (site, label, payload, kind) in enumerate(cases):
        fixture = output / f"fixture-{index}"
        fixture.mkdir()
        context, failure, register = fixture / "context.json", fixture / "failure.json", fixture / "register.json"
        context.write_bytes(payload if site == "context" else b"{}")
        if site != "context":
            failure.write_bytes(payload)
        if os.name != "nt":
            for path in fixture.iterdir():
                path.chmod(0o600)
        argv = ["status" if site == "context" else site.split("_")[0], "--register", str(register), "--context", str(context)]
        if site != "context":
            argv.extend(["--settlement" if site == "settle_settlement" else "--request", str(failure)])
        before = fixture_snapshot(fixture)
        observations = {}
        for side, command in (("oracle", [sys.executable, "-B", str(script), *argv]), ("native", [str(native), *argv])):
            if side == "oracle":
                source_calls += 1
            else:
                native_calls += 1
            assert source_calls <= 8 and native_calls <= 8
            stdout, stderr, code, timeout = run(command, environment)
            (logs / f"{index}.{side}.stdout").write_bytes(stdout)
            (logs / f"{index}.{side}.stderr").write_bytes(stderr)
            after = fixture_snapshot(fixture)
            _, paths, framing = traceback_parts(stderr)
            error_lines = stderr.splitlines()
            expected_failure = code == 1 and not timeout and not stdout and bool(error_lines) and error_lines[-1].startswith((kind + ": ").encode("ascii"))
            observations[side] = {"command": command, "exit_code": code, "timeout": timeout, "stdout_base64": base64.b64encode(stdout).decode("ascii"), "stderr_base64": base64.b64encode(stderr).decode("ascii"), "input_before": before, "input_after": after, "source_paths_base64": paths, "required_framing": framing, "expected_failure": expected_failure}
            if after != before:
                (output / "unexpected-fixture-write.json").write_text(json.dumps({"index": index, "side": side, "before": before, "after": after}, indent=2) + "\n", encoding="utf-8")
                raise ValueError("Reference/native exceeded read-only fixture scope")
        source, rust = observations["oracle"], observations["native"]
        source_error = base64.b64decode(source["stderr_base64"])
        native_error = base64.b64decode(rust["stderr_base64"])
        differences = [field for field in ("exit_code", "timeout", "stdout_base64", "input_after") if source[field] != rust[field]]
        if not source["required_framing"] or not rust["required_framing"] or not source["expected_failure"] or not rust["expected_failure"]:
            differences.append("required_reference_and_native_failure_shape")
        classification = "raw_exact"
        if source_error != native_error:
            left, left_paths, left_valid = traceback_parts(source_error)
            right, right_paths, right_valid = traceback_parts(native_error)
            if left_valid and right_valid and left == right:
                classification = "variable_source_path_diagnostics_only"
            else:
                classification = "required_mismatch"
                differences.append("required_stderr_bytes_outside_variable_source_prefixes")
        records.append({"index": index, "site": site, "label": label, "argv": argv, "observations": observations, "stderr_classification": classification, "required_mismatches": differences})
        (output / "progress.json").write_text(json.dumps({"source_calls": source_calls, "native_calls": native_calls, "records": records}, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    oracle_after = {path.relative_to(oracle).as_posix(): digest(path) for path in sorted((oracle / "graph_engine").glob("*.py"))}
    oracle_after["scripts/helper-register.py"] = digest(script)
    assert oracle_before == oracle_after and source_before == sources(package)
    assert python_before == digest(sys.executable) and native_before == digest(native)
    assert source_calls == native_calls == 8
    result = {"schema_version": 1, "scope": "Eight fresh guarded read-only JSON failures; only variable traceback source prefixes may be classified as diagnostics, all raw streams retained.", "source_calls": source_calls, "native_calls": native_calls, "other_driver_subprocesses": 0, "git_subprocesses_expected": 0, "handler_calls_expected": 0, "nested_process_observation": "No independent OS process trace; zero nested/Git launches follows the JSON failure before handler invocation.", "oracle_sha256_before_after": oracle_before, "native_source_sha256_before_after": source_before, "python_sha256_before_after": python_before, "native_sha256_before_after": native_before, "child_environment": {key: environment.get(key) for key in ("PYTHONDONTWRITEBYTECODE", "PYTHONINTMAXSTRDIGITS", "PYTHONNODEBUGRANGES", "PYTHONIOENCODING", "PYTHONUTF8", "COLUMNS", "TEMP", "TMP")}, "records": records, "original_promotions": 0, "remaining": "Other unexpected traceback families and complete original GraphCase workflows are separate proof obligations."}
    (output / "report.json").write_text(json.dumps(result, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    failures = sum(bool(record["required_mismatches"]) for record in records)
    print(json.dumps({"comparisons": len(records), "source_calls": source_calls, "native_calls": native_calls, "required_mismatches": failures, "raw_exact": sum(record["stderr_classification"] == "raw_exact" for record in records)}))
    return int(bool(failures))


if __name__ == "__main__":
    raise SystemExit(main())
