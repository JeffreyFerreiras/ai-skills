"""Bind the three executed original timing cases without changing other identities."""

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import sys

PACKAGE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(PACKAGE / "parity"))
from run_registry import native_source_snapshot


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for item in value.values():
            yield from strings(item)
    elif isinstance(value, list):
        for item in value:
            yield from strings(item)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--registry", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--expected-sha256", required=True)
    args = parser.parse_args()
    artifact = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve(strict=True)
    registry = args.registry.resolve(strict=True)
    binary = args.binary.resolve(strict=True)
    if not registry.is_relative_to(artifact) or not binary.is_relative_to(artifact):
        raise ValueError("PROOF_OUTSIDE_ART")
    path = PACKAGE / "parity/coverage-map.json"
    before_bytes = path.read_bytes()
    if hashlib.sha256(before_bytes).hexdigest() != args.expected_sha256:
        raise ValueError("COVERAGE_EXPECTED_RAW_HASH_MISMATCH")
    before = json.loads(before_bytes)
    data = copy.deepcopy(before)
    expected = {"modules": 17, "definitions": 383, "commands": 39, "mutation_rules": 25, "test_identities": 360}
    keys = {key: set(before[key]) for key in expected}
    if any(len(keys[key]) != count for key, count in expected.items()):
        raise ValueError("COVERAGE_KEYS_CHANGED")
    prior = {key: row for key, row in before["test_identities"].items() if row["status"] == "proven"}
    if len(prior) != 91:
        raise ValueError("PRIOR_PROMOTION_STATE_CHANGED")
    proof_bytes = registry.read_bytes()
    proof = json.loads(proof_bytes)
    if proof["registered"] != 94 or proof["native_source"] != native_source_snapshot():
        raise ValueError("REGISTRY_SOURCE_OR_COUNT_MISMATCH")
    if sha256(binary) not in set(strings(proof["bindings"])):
        raise ValueError("PRIMARY_BINARY_BINDING_MISMATCH")
    for auxiliary in proof["auxiliary_binaries"]:
        if sha256(Path(auxiliary["path"])) != auxiliary["sha256"]:
            raise ValueError("AUXILIARY_BINARY_CHANGED")
    names = ["exact_overlap_slowest_and_critical_path_metrics", "retry_lifecycle_and_microsecond_flooring", "incomplete_metrics_are_null_and_equal_ties_are_lexical"]
    targets = {"tests.test_graph_hardening.TimingMetricTests.test_" + name: name for name in names}
    executed = set()
    for index, record in enumerate(proof["records"]):
        if record["exit"] != 0 or record["timeout"]:
            raise ValueError("REGISTRY_FAILURE")
        identity = record["identity"]
        if identity in executed:
            raise ValueError("DUPLICATE_REGISTRY_IDENTITY")
        executed.add(identity)
        raw = (registry.parent / (str(index) + ".stdout")).read_bytes()
        if raw not in {(f"PASS {identity}\n").encode(), (f"PASS {identity}\r\n").encode()} or (registry.parent / (str(index) + ".stderr")).read_bytes():
            raise ValueError("REGISTRY_RAW_RESULT_MISMATCH")
    if executed != set(prior) | set(targets):
        raise ValueError("REGISTRY_IDENTITY_SET_MISMATCH")
    for identity, name in targets.items():
        if data["test_identities"][identity]["status"] != "not_started":
            raise ValueError("TIMING_IDENTITY_ALREADY_CHANGED")
        data["test_identities"][identity].update(
            status="proven", boundary="native equivalent of private SQLite compute_timing assertions",
            native_component="src/application/timing.rs;src/infrastructure/graph_ledger.rs",
            native_assertions=["src/suite/timing.rs::" + name, "tests/timing_originals.rs::" + name],
            fixtures=["src/suite/timing.rs::fixture: original seven-table in-memory SQLite schema and original insert/teardown sequence", "skills/software-engineering-graph/tests/test_graph_hardening.py:2222-2315"],
            proof_artifacts=["ART/" + registry.relative_to(artifact).as_posix()],
            negative_proof=["tests/timing_originals.rs::critical_path_rejects_cycles_and_keeps_unbounded_weights", "tests/timing_originals.rs::negative_microseconds_use_python_floor_division"],
            nondeterministic_relationships=["No current-time calls or elapsed-duration matching; all timestamps are the original fixed fixture values."])
    if any(set(data[key]) != keyset for key, keyset in keys.items()):
        raise ValueError("COVERAGE_IDENTITY_SET_CHANGED")
    if any(data["test_identities"][identity] != row for identity, row in prior.items()):
        raise ValueError("PRIOR_PROVEN_RECORD_CHANGED")
    changed = {identity for identity in before["test_identities"] if before["test_identities"][identity] != data["test_identities"][identity]}
    if changed != set(targets) or any(before[key] != data[key] for key in expected if key != "test_identities"):
        raise ValueError("UNAUTHORIZED_COVERAGE_CHANGE")
    output = artifact / "coverage-timing-update-01"
    output.mkdir(exist_ok=False)
    (output / "before.json.txt").write_bytes(before_bytes)
    revised = (json.dumps(data, separators=(",", ":"), sort_keys=True) + "\n").encode("ascii")
    if path.read_bytes() != before_bytes or registry.read_bytes() != proof_bytes or proof["native_source"] != native_source_snapshot():
        raise ValueError("COVERAGE_OR_PROOF_CHANGED_DURING_UPDATE")
    path.write_bytes(revised)
    if path.read_bytes() != revised:
        raise ValueError("COVERAGE_WRITE_MISMATCH")
    receipt = dict(before_sha256=hashlib.sha256(before_bytes).hexdigest(), after_sha256=sha256(path), identities=expected,
                   prior_proven_preserved=91, promoted=sorted(targets), total_proven=94,
                   registry_sha256=hashlib.sha256(proof_bytes).hexdigest(), primary_binary_sha256=sha256(binary),
                   proof_bindings=proof["bindings"], auxiliary_binaries=proof["auxiliary_binaries"], native_source=proof["native_source"])
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({key: receipt[key] for key in ("before_sha256", "after_sha256", "prior_proven_preserved", "total_proven", "registry_sha256", "primary_binary_sha256")}))


if __name__ == "__main__":
    main()
