"""Bind the current native original-identity subset to archived source and inputs."""

import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

from check_native_component import freeze_sources, guarded, inventory, write_json
from copy_checkpoint_inputs import copy_inputs, sha256, verify_copies


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--inputs-from", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--registry-output", type=Path, required=True)
    arguments = parser.parse_args()
    package = Path(__file__).resolve().parent.parent
    worktree = package.parent.parent
    artifacts = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve(strict=True)
    output, registry_output = arguments.output.resolve(), arguments.registry_output.resolve()
    output.relative_to(artifacts)
    registry_output.relative_to(artifacts)
    if output.exists() or registry_output.exists():
        raise FileExistsError("Preserve the existing registry checkpoint outcome")
    inherited = json.loads(arguments.inputs_from.read_bytes())
    runtime_before = guarded(inherited["runtime"], "path")
    if not all(record["equal"] for record in runtime_before):
        raise ValueError("Selected runtime differs from its retained binding")
    coverage = json.loads((package / "parity/coverage-map.json").read_bytes())
    proven = sum(row["status"] == "proven" for row in coverage["test_identities"].values())
    inventory_path = artifacts / "oracle-inventory.json"
    inventory_hash = sha256(inventory_path)
    output.mkdir(parents=True, exist_ok=False)
    sources = freeze_sources(package, output / "source")
    inputs = copy_inputs(inherited["frozen_inputs"], output / "inputs")
    write_json(output / "input-archive.json", {"records": inputs})
    target = artifacts / "build/target/debug"
    suite, helper = target / "graph-suite.exe", target / "helper-register.exe"
    commands = [
        ["cargo", "build", "--locked", "--manifest-path", "experiments/graph-engine-rust/Cargo.toml",
         "--bin", "graph-suite", "--bin", "helper-register"],
        [sys.executable, "-B", "experiments/graph-engine-rust/parity/run_registry.py",
         "--rust-suite", str(suite), "--inventory", str(inventory_path),
         "--output", str(registry_output), "--auxiliary-binary", str(helper)],
    ]
    before = {"source_root": str(package), "source": sources,
              "source_scope": "All authored files including untracked additions",
              "frozen_inputs": inputs, "runtime": inherited["runtime"],
              "runtime_before": runtime_before, "base_head": inherited["base_head"],
              "skills": inherited["skills"], "commands": commands,
              "inventory_sha256": inventory_hash, "original_proven_before": proven,
              "reference_invocations": 0, "test_evidence_root": str(output / "evidence")}
    write_json(output / "before.json", before)
    environment = dict(os.environ)
    fixtures = artifacts / "fixtures"
    fixtures.mkdir(exist_ok=True)
    environment.update(CARGO_HOME=str(artifacts / "build/cargo-home"),
                       CARGO_TARGET_DIR=str(artifacts / "build/target"),
                       GRAPH_RUST_TEST_EVIDENCE_ROOT=str(output / "evidence"),
                       GRAPH_RUST_HELPER_BINARY=str(helper),
                       GRAPH_RUST_ORACLE_PYTHON=sys.executable,
                       TEMP=str(fixtures), TMP=str(fixtures), TMPDIR=str(fixtures),
                       PYTHONDONTWRITEBYTECODE="1")
    outcomes, binaries, binary_guards = [], [], []
    for index, command in enumerate(commands):
        if index == 1 and outcomes[0]["exit_code"] != 0:
            outcomes.append({"argv": command, "exit_code": None, "skipped": "Build failed"})
            continue
        if index == 1:
            for path in (suite, helper):
                destination = output / "binaries" / path.name
                destination.parent.mkdir(exist_ok=True)
                expected = sha256(path)
                with path.open("rb") as incoming, destination.open("xb") as copied:
                    shutil.copyfileobj(incoming, copied)
                if sha256(destination) != expected:
                    raise ValueError("Native binary archive mismatch")
                binaries.append({"path": str(path), "sha256": expected, "copy": str(destination)})
        stdout, stderr = output / f"{index}.stdout.bin", output / f"{index}.stderr.bin"
        timed_out = False
        with stdout.open("xb") as outgoing, stderr.open("xb") as errors:
            try:
                completed = subprocess.run(command, cwd=worktree, env=environment,
                                           stdout=outgoing, stderr=errors,
                                           timeout=1200 if index else 300, check=False)
                exit_code = completed.returncode
            except subprocess.TimeoutExpired:
                exit_code, timed_out = None, True
        outcomes.append({"argv": command, "exit_code": exit_code, "timeout": timed_out,
                         "stdout": stdout.name, "stdout_sha256": sha256(stdout),
                         "stderr": stderr.name, "stderr_sha256": sha256(stderr)})
    if binaries:
        binary_guards = guarded(binaries, "path")
    source_after = inventory(package)
    input_after, runtime_after = guarded(inputs, "source"), guarded(inherited["runtime"], "path")
    verify_copies(inputs, output / "inputs")
    raw_logs = []
    report_path = registry_output / "report.json"
    if report_path.is_file():
        report = json.loads(report_path.read_bytes())
        for index, record in enumerate(report["records"]):
            raw_logs.append({"identity": record["identity"],
                             "stdout_sha256": sha256(registry_output / f"{index}.stdout"),
                             "stderr_sha256": sha256(registry_output / f"{index}.stderr")})
    write_json(output / "after.json", {"source": source_after, "inputs": input_after,
                                        "runtime": runtime_after, "binaries": binary_guards,
                                        "registry_log_hashes": raw_logs})
    unchanged = (sources == source_after and inventory_hash == sha256(inventory_path)
                 and all(row["equal"] for row in input_after + runtime_after + binary_guards))
    passed = unchanged and all(row["exit_code"] == 0 for row in outcomes)
    receipt = {"status": "NATIVE_REGISTRY_PENDING_ORIGINAL_ASSESSMENT", "passed": passed,
               "guards_unchanged": unchanged, "commands": outcomes,
               "before_sha256": sha256(output / "before.json"),
               "after_sha256": sha256(output / "after.json"),
               "source_files": len(sources), "input_records": len(inputs),
               "unique_input_copies": len(inputs), "binaries": binaries,
               "report_sha256": sha256(report_path) if report_path.is_file() else None,
               "raw_log_hashes_observed": "After child registry exits, within this guarded checkpoint",
               "registered": len(raw_logs), "original_proven_before": proven,
               "reference_invocations": 0, "skills": inherited["skills"],
               "limits": "Current original subset only; no promotion, final verification or timing claim"}
    write_json(output / "receipt.json", receipt)
    print(json.dumps({"passed": passed, "registered": len(raw_logs), "source_files": len(sources),
                      "receipt_sha256": sha256(output / "receipt.json")}))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
