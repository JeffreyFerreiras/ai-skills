"""Build and freeze a lease packet, executing only its native preflight."""

import argparse
import json
import os
import shutil
import sys
from pathlib import Path

from check_native_component import freeze_sources, guarded, inventory, write_json
from copy_checkpoint_inputs import copy_inputs, sha256, verify_copies

PACKAGE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(PACKAGE / "parity"))
import lease_expiry
import run_sorting
from run_lease_expiry import STREAM_CAP, TIMEOUT_SECONDS


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--inputs-from", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    artifacts = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve(strict=True)
    output = arguments.output.resolve()
    output.relative_to(artifacts)
    assert not output.exists(), "preserve any prior or unknown preparation outcome"
    inherited = json.loads(arguments.inputs_from.read_bytes())
    assert all(record["equal"] for record in guarded(inherited["runtime"], "path"))
    output.mkdir(parents=True, exist_ok=False)
    manifest = lease_expiry.prepare(output)  # Pure generation, no frozen-source import.
    requests = output / "requests.jsonl"
    manifest_path = output / "manifest.json"
    source_before = freeze_sources(PACKAGE, output / "source")
    assert not any(path.is_symlink() for path in PACKAGE.rglob("*"))
    runtime = list(inherited["runtime"])
    datetime_wrapper = Path("C:/Python312/Lib/datetime.py")
    runtime.append({"path": str(datetime_wrapper), "sha256": sha256(datetime_wrapper)})
    records = list(inherited["frozen_inputs"])
    existing = {str(Path(record["source"]).resolve()) for record in records}
    source_root = PACKAGE.parent.parent / "skills/software-engineering-graph"
    additional = list((source_root / "graph_engine").glob("*.py")) + [datetime_wrapper, requests, manifest_path]
    for path in additional:
        if str(path.resolve()) not in existing:
            records.append({"source": str(path), "path": str(path), "sha256": sha256(path)})
            existing.add(str(path.resolve()))
    inputs = copy_inputs(records, output / "inputs")
    write_json(output / "input-archive.json", {"records": inputs})
    environment = dict(os.environ, PYTHONDONTWRITEBYTECODE="1", PYTHONHASHSEED="0",
                       CARGO_HOME=str(artifacts / "build/cargo-home"),
                       CARGO_TARGET_DIR=str(artifacts / "build/target"))
    build_directory = output / "build"
    build_directory.mkdir()
    run_sorting.MAX_OUTPUT = STREAM_CAP
    build_argv = ["cargo", "build", "--locked", "--manifest-path", str(PACKAGE / "Cargo.toml"), "--bin", "lease-expiry-probe"]
    build = run_sorting.run_child(build_argv, input_bytes=None, directory=build_directory,
                                 environment=environment, prefix="build", timeout=90)
    write_json(build_directory / "report.json", build)
    assert build["exit"] == 0 and build["raw_complete"] and not build["timed_out"], "retained native build failed"
    target = artifacts / "build/target/debug/lease-expiry-probe.exe"
    binaries = output / "binaries"
    binaries.mkdir()
    binary = binaries / target.name
    with target.open("rb") as incoming, binary.open("xb") as outgoing:
        shutil.copyfileobj(incoming, outgoing)
    assert sha256(binary) == sha256(target)
    binding = {"source_root": str(PACKAGE), "source_archive": str(output / "source"),
               "source": [{"path": path, "sha256": digest} for path, digest in source_before.items()],
               "source_scope": "all authored files, including untracked additions and Cargo.lock",
               "frozen_inputs": inputs, "runtime": runtime, "base_head": inherited["base_head"],
               "skills": inherited["skills"], "binary": {"path": str(binary), "sha256": sha256(binary)},
               "target_binary": {"path": str(target), "sha256": sha256(target)},
               "build": build, "environment": {key: environment[key] for key in ("CARGO_HOME", "CARGO_TARGET_DIR", "PYTHONDONTWRITEBYTECODE", "PYTHONHASHSEED")},
               "python_reference_calls": 0}
    binding_path = output / "binding.json"
    write_json(binding_path, binding)
    packet = {"cases": manifest["cases"], "binding": {"path": str(binding_path), "sha256": sha256(binding_path)},
              "requests": {"path": str(requests), "sha256": sha256(requests)},
              "manifest": {"path": str(manifest_path), "sha256": sha256(manifest_path)},
              "reference_argv": ["C:/Python312/python.exe", "-I", "-B", str(PACKAGE / "parity/lease_expiry.py"), "reference", str(requests), str(source_root)],
              "native_argv": [str(binary)], "output": str(output / "matrix01"),
              "driver": str(PACKAGE / "parity/run_lease_expiry.py"),
              "bounds": {"source_children": 1, "native_children": 1, "source_function_calls": manifest["cases"],
                         "per_child_seconds": TIMEOUT_SECONDS, "live_cap_bytes_per_stream": STREAM_CAP,
                         "retries": 0, "handler_state_locks_git_network_profile_controller_calls": 0},
              "protocol": "exact ASCII escaped sorted compact JSON plus binary LF; no payload normalization",
              "source_scope": "original cli._lease_expired with substituted fixed aware UTC clock; observed datetime.now count and injected clock error",
              "limitations": manifest["limitations"], "original_promotions": 0}
    before = run_sorting.guarded_files(binding, packet)
    preflight = output / "native-preflight01"
    preflight.mkdir()
    native = run_sorting.run_child(packet["native_argv"], input_bytes=requests.read_bytes(), directory=preflight,
                                  environment=environment, prefix="native", timeout=TIMEOUT_SECONDS)
    rows = [json.loads(line) for line in (preflight / "native.stdout.bin").read_bytes().splitlines()]
    cases = [json.loads(line) for line in requests.read_bytes().splitlines()]
    assert [row["id"] for row in rows] == [case["id"] for case in cases]
    assert all(type(row["clock_reads"]) is int and 0 <= row["clock_reads"] <= 1 for row in rows)
    after = run_sorting.guarded_files(binding, packet)
    verify_copies(inputs, output / "inputs")
    assert inventory(PACKAGE) == source_before and inventory(output / "source") == source_before
    assert before == after and not Path(packet["output"]).exists()
    passed = native["exit"] == 0 and native["raw_complete"] and not native["timed_out"] and native["input"]["complete"]
    receipt = {"passed": passed, "cases": len(cases), "records": len(rows), "native": native,
               "source_file_count": len(source_before), "unique_input_copies": len(inputs), "runtime_count": len(runtime),
               "guards_before": before, "guards_after": after, "binding_sha256": sha256(binding_path),
               "reference_invocations": 0, "original_promotions": 0,
               "limitations": manifest["limitations"]}
    receipt_path = preflight / "receipt.json"
    write_json(receipt_path, receipt)
    assert passed, "retained native preflight failed"
    packet["preflight"] = {"path": str(receipt_path), "sha256": sha256(receipt_path)}
    packet_path = output / "packet01.json"
    write_json(packet_path, packet)
    print(json.dumps({"packet": str(packet_path), "packet_sha256": sha256(packet_path),
                      "binding_sha256": sha256(binding_path), "preflight_sha256": sha256(receipt_path),
                      "binary_sha256": sha256(binary), "source_files": len(source_before),
                      "input_copies": len(inputs), "runtime": len(runtime), "cases": len(cases),
                      "reference_invocations": 0}))


if __name__ == "__main__":
    main()
