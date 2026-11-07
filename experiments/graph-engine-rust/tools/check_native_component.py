"""Freeze authored files and inputs around focused, native-only Cargo checks."""

import argparse
import json
import os
import re
import shutil
import subprocess
from pathlib import Path

from copy_checkpoint_inputs import copy_inputs, sha256, verify_copies


def inventory(root):
    return {path.relative_to(root).as_posix(): sha256(path)
            for path in sorted(root.rglob("*")) if path.is_file()}


def guarded(records, source_key, expected_key="sha256"):
    result = []
    for record in records:
        path = Path(record[source_key])
        observed = sha256(path) if path.is_file() else None
        result.append({"path": str(path), "expected_sha256": record[expected_key],
                       "observed_sha256": observed, "equal": observed == record[expected_key]})
    return result


def freeze_sources(package, output):
    before = inventory(package)
    for relative, expected in before.items():
        destination = output / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        with (package / relative).open("rb") as incoming, destination.open("xb") as copied:
            shutil.copyfileobj(incoming, copied)
        if sha256(destination) != expected:
            raise ValueError("Authored source copy mismatch")
    if inventory(package) != before or inventory(output) != before:
        raise ValueError("Authored inventory changed during copy")
    return before


def write_json(path, value):
    with path.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(value, stream, ensure_ascii=True, sort_keys=True, indent=2)
        stream.write("\n")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--inputs-from", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--test", action="append", required=True)
    parser.add_argument("--exact-filter")
    arguments = parser.parse_args()
    package = Path(__file__).resolve().parent.parent
    worktree = package.parent.parent
    artifacts = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve(strict=True)
    output = arguments.output.resolve()
    output.relative_to(artifacts)
    if output.exists():
        raise FileExistsError("Checkpoint already exists; preserve its outcome")
    for name in arguments.test:
        if not re.fullmatch(r"[a-z][a-z0-9_]*", name):
            raise ValueError("Invalid native test target")
        if not (package / "tests" / (name + ".rs")).is_file():
            raise FileNotFoundError(name)
    if arguments.exact_filter and not re.fullmatch(r"[a-z][a-z0-9_]*", arguments.exact_filter):
        raise ValueError("Invalid exact native test filter")
    inherited = json.loads(arguments.inputs_from.read_bytes())
    coverage = json.loads((package / "parity/coverage-map.json").read_bytes())
    identities = coverage["test_identities"]
    proven = sum(row["status"] == "proven" for row in identities.values())
    remaining = len(identities) - proven
    runtime_before = guarded(inherited["runtime"], "path")
    if not all(record["equal"] for record in runtime_before):
        raise ValueError("Selected runtime differs from its retained binding")
    output.mkdir(parents=True, exist_ok=False)
    source_before = freeze_sources(package, output / "source")
    inputs = copy_inputs(inherited["frozen_inputs"], output / "inputs")
    write_json(output / "input-archive.json", {"records": inputs})
    commands = [
        ["cargo", "check", "--locked", "--manifest-path", "experiments/graph-engine-rust/Cargo.toml", "--all-targets"],
        ["cargo", "test", "--locked", "--manifest-path", "experiments/graph-engine-rust/Cargo.toml"],
        ["cargo", "fmt", "--manifest-path", "experiments/graph-engine-rust/Cargo.toml", "--", "--check"],
    ]
    for name in arguments.test:
        commands[1].extend(["--test", name])
    if arguments.exact_filter:
        commands[1].extend(["--", arguments.exact_filter, "--exact"])
    before = {"source_root": str(package), "source": source_before,
              "source_scope": "All authored files including untracked additions",
              "frozen_inputs": inputs, "runtime": inherited["runtime"],
              "runtime_before": runtime_before, "base_head": inherited["base_head"],
              "skills": inherited["skills"], "commands": commands,
              "reference_invocations": 0, "original_proven": proven, "original_remaining": remaining}
    before["test_evidence_root"] = str(output / "evidence")
    write_json(output / "before.json", before)
    environment = dict(os.environ)
    environment.update(CARGO_HOME=str(artifacts / "build/cargo-home"),
                       CARGO_TARGET_DIR=str(artifacts / "build/target"),
                       GRAPH_RUST_TEST_EVIDENCE_ROOT=str(output / "evidence"),
                       PYTHONDONTWRITEBYTECODE="1")
    outcomes = []
    for index, command in enumerate(commands):
        if index == 1 and outcomes[0]["exit_code"] != 0:
            outcomes.append({"argv": command, "exit_code": None, "skipped": "Compilation failed"})
            continue
        stdout, stderr = output / f"{index}.stdout.bin", output / f"{index}.stderr.bin"
        timed_out = False
        with stdout.open("xb") as outgoing, stderr.open("xb") as errors:
            try:
                completed = subprocess.run(command, cwd=worktree, env=environment,
                                           stdout=outgoing, stderr=errors, timeout=300, check=False)
                exit_code = completed.returncode
            except subprocess.TimeoutExpired:
                exit_code, timed_out = None, True
        outcomes.append({"argv": command, "exit_code": exit_code, "timeout": timed_out,
                         "stdout": stdout.name, "stdout_sha256": sha256(stdout),
                         "stderr": stderr.name, "stderr_sha256": sha256(stderr)})
    binaries = []
    if outcomes[1]["exit_code"] is not None:
        text = (output / "1.stderr.bin").read_text(encoding="utf-8", errors="replace")
        for name in re.findall(r"Running[^\r\n]*\(([^()\r\n]+\.exe)\)", text):
            path = Path(name)
            expected = sha256(path)
            destination = output / "binaries" / path.name
            destination.parent.mkdir(exist_ok=True)
            with path.open("rb") as incoming, destination.open("xb") as copied:
                shutil.copyfileobj(incoming, copied)
            if sha256(destination) != expected:
                raise ValueError("Executed binary copy mismatch")
            binaries.append({"path": str(path), "sha256": expected, "copy": str(destination)})
    source_after = inventory(package)
    input_after = guarded(inputs, "source")
    runtime_after = guarded(inherited["runtime"], "path")
    verify_copies(inputs, output / "inputs")
    write_json(output / "after.json", {"source": source_after, "inputs": input_after,
                                        "runtime": runtime_after, "binaries": binaries})
    unchanged = source_before == source_after and all(record["equal"] for record in input_after + runtime_after)
    passed = unchanged and all(record["exit_code"] == 0 for record in outcomes)
    receipt = {"status": "FOCUSED_NATIVE_COMPONENT", "passed": passed,
               "before_sha256": sha256(output / "before.json"), "after_sha256": sha256(output / "after.json"),
               "source_files": len(source_before), "source_unchanged": source_before == source_after,
               "input_records": len(inputs), "unique_input_copies": len(inputs),
               "inputs_unchanged": all(record["equal"] for record in input_after),
               "runtime_unchanged": all(record["equal"] for record in runtime_after),
               "commands": outcomes, "executed_binaries": binaries,
               "skills": inherited["skills"], "reference_invocations": 0,
               "original_proven": proven, "original_remaining": remaining,
               "limits": "Focused native checks; no original identity promotion or complete-engine claim"}
    write_json(output / "receipt.json", receipt)
    print(json.dumps({"checkpoint": str(output), "passed": passed, "source_files": len(source_before),
                      "input_copies": len(inputs), "commands": [record["exit_code"] for record in outcomes],
                      "executed_binaries": len(binaries), "receipt_sha256": sha256(output / "receipt.json")}))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
