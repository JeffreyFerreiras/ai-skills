"""Compare actual helper CLI streams and durable bytes on one isolated fixture."""

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

from bindings import capture
from helper_policy import materials
from run_registry import native_source_snapshot


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--native", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    artifact = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve(strict=True)
    temporary = artifact / "fixtures"; temporary.mkdir(parents=True, exist_ok=True)
    for key in ("TEMP", "TMP", "TMPDIR"):
        os.environ[key] = str(temporary)
    output = args.output.resolve(); output.relative_to(artifact); output.mkdir(parents=True, exist_ok=False)
    native = args.native.resolve(strict=True); native.relative_to(artifact)
    oracle_root = Path(__file__).resolve().parents[3] / "skills/software-engineering-graph"
    sys.path.insert(0, str(oracle_root))
    from tests.test_graph_hardening import HelperRegisterTests

    case = HelperRegisterTests("test_plan_binds_allowance_and_register_context_is_immutable")
    case.setUp()
    root = case.root.resolve(strict=True)
    if root.parent != temporary or root.is_symlink():
        raise ValueError("FIXTURE_OUTSIDE_ART")
    records = []
    try:
        _registry, initialized, _command, _plan, allowance = case._materials()
        assert allowance == materials(case.repo)[0]
        register = Path(initialized["register_path"])
        lock = register.parent / "register.lock"
        context = root / "context.json"; context.write_bytes(canonical(initialized["context"]))
        request_value = case._request("external-request")
        request = root / "request.json"; request.write_bytes(canonical(request_value))
        settlement = root / "settlement.json"; settlement.write_bytes(canonical(case._settlement(request_value)))
        script = oracle_root / "scripts/helper-register.py"
        programs = {"oracle": [sys.executable, "-B", str(script)], "native": [str(native)]}
        source_before = native_source_snapshot()

        def guarded(path):
            path.resolve(strict=False).relative_to(root)
            if path.exists() and os.lstat(path).st_file_attributes & 0x400:
                raise ValueError("REPARSE_FIXTURE_FILE")
            return path

        def snapshot():
            return {name: guarded(path).read_bytes() if path.exists() else None for name, path in (("register", register), ("lock", lock))}

        def save(index, side, state):
            for name, payload in state.items():
                if payload is not None:
                    (output / (str(index) + "." + side + "." + name + ".txt")).write_bytes(payload)

        def restore(state):
            for name, path in (("register", register), ("lock", lock)):
                guarded(path)
                if state[name] is None:
                    if path.exists():
                        path.unlink()
                else:
                    path.write_bytes(state[name])

        def compare(label, argv):
            index = len(records); before = snapshot(); save(index, "before", before)
            record = {"label": label, "argv": [str(value) for value in argv]}
            for side in ("oracle", "native"):
                if side == "native":
                    restore(before)
                command = programs[side] + record["argv"]
                try:
                    completed = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=15, check=False)
                    stdout, stderr, code, timeout = completed.stdout, completed.stderr, completed.returncode, False
                except subprocess.TimeoutExpired as error:
                    stdout, stderr, code, timeout = error.stdout or b"", error.stderr or b"", None, True
                (output / (str(index) + "." + side + ".stdout")).write_bytes(stdout)
                (output / (str(index) + "." + side + ".stderr")).write_bytes(stderr)
                after = snapshot(); save(index, side + ".after", after)
                record[side] = {"command": command, "exit_code": code, "timeout": timeout, "stdout_base64": base64.b64encode(stdout).decode("ascii"), "stderr_base64": base64.b64encode(stderr).decode("ascii"), "after": {key: None if value is None else hashlib.sha256(value).hexdigest() for key, value in after.items()}}
            comparable = lambda value: {key: value[key] for key in ("exit_code", "timeout", "stdout_base64", "stderr_base64", "after")}
            record["matches"] = comparable(record["oracle"]) == comparable(record["native"])
            records.append(record)

        initialize = ["initialize", "--state-root", root / "host-state", "--repo", case.repo, "--run-id", "RUN-HELPERS", "--plan", case.repo / "docs/helper-plan.json", "--allowance", case.repo / "docs/helper-allowance.json", "--host-observation", case.repo / "docs/host-observation.json"]
        save("bootstrap", "source", snapshot())
        restore({"register": None, "lock": None})
        compare("initialize_fresh", initialize)
        compare("initialize_replay", initialize)
        bound = ["--register", register, "--context", context]
        compare("preflight", ["preflight", *bound, "--request", request])
        compare("reserve", ["reserve", *bound, "--request", request])
        compare("reserve_replay", ["reserve", *bound, "--request", request])
        compare("settle", ["settle", *bound, "--settlement", settlement])
        compare("settle_replay", ["settle", *bound, "--settlement", settlement])
        compare("status", ["status", *bound])

        malformed = root / "malformed-context.json"
        for label, payload in (("context_invalid_json", b"{"), ("context_invalid_utf8", b"\xff"), ("context_utf8_bom", b"\xef\xbb\xbf{}"), ("context_null", b"null"), ("context_list", b"[]"), ("context_empty", b"{}"), ("context_sensitive", b'{"api_key":"synthetic"}'), ("context_oversize", b" " * (256 * 1024 + 1))):
            malformed.write_bytes(payload)
            compare(label, ["status", "--register", register, "--context", malformed])
        compare("context_missing", ["status", "--register", register, "--context", root / "missing-context.json"])
        directory = root / "context-directory"; directory.mkdir()
        compare("context_directory", ["status", "--register", register, "--context", directory])
        relative = [str(value) for value in initialize]; relative[relative.index("--state-root") + 1] = "relative-root"
        compare("state_root_relative", relative)
        external = root / "external"; external.mkdir(); external_register = external / "register.json"; external_register.write_bytes(b"external register sentinel")
        compare("external_register_path", ["reserve", "--register", external_register, "--context", context, "--request", request])
        assert external_register.read_bytes() == b"external register sentinel" and not (external / "register.lock").exists()
        bad_request = root / "bad-request.json"; bad_request.write_bytes(b"[")
        compare("request_invalid_json", ["reserve", *bound, "--request", bad_request])
        bad_settlement = root / "bad-settlement.json"; bad_settlement.write_bytes(b"[")
        compare("settlement_invalid_json", ["settle", *bound, "--settlement", bad_settlement])
        correct = register.read_bytes(); broken = json.loads(correct); broken["usage"] = None; register.write_bytes(canonical(broken))
        compare("register_usage_null", ["status", *bound]); register.write_bytes(correct)
        malformed.write_bytes(b"1" * 4301)
        compare("context_integer_digit_limit", ["status", "--register", register, "--context", malformed])
        malformed.write_bytes(b"[" * 3005 + b"0" + b"]" * 3005)
        compare("context_recursion_limit", ["status", "--register", register, "--context", malformed])
        if len(records) != 25 or native_source_snapshot() != source_before:
            raise ValueError("CORPUS_SCOPE_OR_SOURCE_CHANGED")
        report = {"scope": "25actual external CLI comparisons; synthetic owned ART fixture only; stored validation argv never executed", "records": records, "differences": sum(not record["matches"] for record in records), "environment": {key: os.environ.get(key) for key in ("TEMP", "TMP", "TMPDIR", "COLUMNS", "PYTHONIOENCODING", "PYTHONUTF8")}, "oracle_executable": sys.executable, "oracle_script_sha256": hashlib.sha256(script.read_bytes()).hexdigest(), "native_source": source_before, "bindings": capture(native), "generator_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
        (output / "report.json").write_bytes((json.dumps(report, ensure_ascii=True, sort_keys=True, indent=2) + "\n").encode("ascii"))
        print(json.dumps({"comparisons": len(records), "differences": report["differences"], "failed_labels": [record["label"] for record in records if not record["matches"]]}, sort_keys=True))
        return int(bool(report["differences"]))
    finally:
        case.root.resolve(strict=True).relative_to(temporary)
        case.tearDown()
        case.doCleanups()


if __name__ == "__main__":
    raise SystemExit(main())
