"""Credit the two guarded StateTests without changing earlier coverage rows."""

import argparse
import ast
import copy
import hashlib
import json
import os
from pathlib import Path

from copy_checkpoint_inputs import verify_copies


NAMES = (
    "test_mutation_validates_before_and_after_and_rolls_back_on_failure",
    "test_replay_precedes_state_matrix_for_every_mutation",
)
PREFIX = "tests.test_state.StateTests."


def digest(data):
    return hashlib.sha256(data).hexdigest()


def load(path):
    return json.loads(Path(path).read_bytes())


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    package = Path(__file__).resolve().parent.parent
    artifacts = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve(strict=True)
    output = arguments.output.resolve()
    output.relative_to(artifacts)
    if output.exists():
        raise FileExistsError("Coverage outcome already exists")
    previous_path = artifacts / "parity/registry-10/report.json"
    report_path = artifacts / "parity/registry-13/report.json"
    checkpoint = artifacts / "checkpoints/cli-state-originals-before-02"
    supplement = artifacts / "checkpoints/cli-state-originals-input-supplement-01"
    audit_path = artifacts / "supervisor-registry-13-supplement-audit-01.json"
    previous, report = load(previous_path), load(report_path)
    manifest, run_receipt = load(checkpoint / "before.json"), load(checkpoint / "receipt.json")
    supplement_receipt = load(supplement / "receipt.json")
    mapping = load(supplement / "input-archive.json")["records"]
    assert digest(report_path.read_bytes()) == run_receipt["report_sha256"]
    assert run_receipt["registry_exit"] == 0
    for field in ("source_changes", "input_changes", "runtime_changes", "binary_changes"):
        assert run_receipt[field] == [], field
    assert supplement_receipt["test_exit"] == 0 and supplement_receipt["copy_exit"] == 0
    assert len(mapping) == len(manifest["frozen_inputs"]) == 47
    verify_copies(mapping, supplement / "inputs")
    for index, record in enumerate(mapping):
        original = manifest["frozen_inputs"][index]
        assert record["record_index"] == index
        assert all(record[key] == original[key] for key in ("source", "path", "sha256"))
        assert digest(Path(record["source"]).read_bytes()) == record["sha256"]
    for record in manifest["source"]:
        assert digest((package / record["path"]).read_bytes()) == record["sha256"]
        assert digest((checkpoint / "source" / record["path"]).read_bytes()) == record["sha256"]
    for binary in run_receipt["binaries"]:
        assert digest(Path(binary["path"]).read_bytes()) == binary["sha256"]
        assert digest(Path(binary["copy"]).read_bytes()) == binary["sha256"]
    old_ids = {record["identity"] for record in previous["records"]}
    actual_ids = {record["identity"] for record in report["records"]}
    promoted = {PREFIX + name for name in NAMES}
    assert len(old_ids) == 102 and len(actual_ids) == len(report["records"]) == 104
    assert actual_ids == old_ids | promoted and old_ids.isdisjoint(promoted)
    assert all(record["exit"] == 0 and not record["timeout"] for record in report["records"])
    coverage_path = package / "parity/coverage-map.json"
    before_bytes = coverage_path.read_bytes()
    before = json.loads(before_bytes)
    assert digest(before_bytes) == report["bindings"]["source_files_sha256"]["parity/coverage-map.json"]
    assert {key for key, row in before["test_identities"].items() if row["status"] == "proven"} == old_ids
    source = package.parent.parent / "skills/software-engineering-graph"
    test_path, fixture_path = source / "tests/test_state.py", source / "tests/test_support.py"
    test_bytes, fixture_bytes = test_path.read_bytes(), fixture_path.read_bytes()
    test_class = next(node for node in ast.parse(test_bytes).body if isinstance(node, ast.ClassDef) and node.name == "StateTests")
    methods = {node.name: node for node in test_class.body if isinstance(node, ast.FunctionDef)}
    document = copy.deepcopy(before)
    proofs = [
        "ART/parity/registry-13/report.json",
        "ART/checkpoints/cli-state-originals-before-02/receipt.json",
        "ART/checkpoints/cli-state-originals-input-supplement-01/receipt.json",
        "ART/supervisor-registry-13-supplement-audit-01.json",
    ]
    for name in NAMES:
        node = methods[name]
        symbol = name.removeprefix("test_")
        document["test_identities"][PREFIX + name].update({
            "status": "proven",
            "boundary": "All original assertions and per-identity GraphCase setup/teardown, including actual native in-process execute/parser/dispatch for initialization and plan approval with the frozen argv order.",
            "oracle_path": "skills/software-engineering-graph/tests/test_state.py",
            "oracle_lines": [node.lineno, node.end_lineno],
            "oracle_sha256": digest(test_bytes),
            "native_component": "src/suite/state_originals.rs",
            "native_symbol": symbol,
            "native_assertions": ["src/suite/state_originals.rs::" + symbol, "graph-suite --identity " + PREFIX + name],
            "fixtures": [
                "tests/support/graph_case.rs::GraphFixture::new/tear_down mirrors the original owned repository/policy/hardlink/runtime fixture",
                "tests/support/initialized_graph.rs::initialize/graphctl/execute writes the source task JSON and invokes actual native CLI parsing and dispatch for init and plan approval",
                "Production process-wide shared parser; fresh policy load, verified state open, run-context load, and full semantic mutation validation",
                "Source StateStore.mutate uses a test-only validator/action seam in these exact original assertions; production handlers retain real semantic validation",
            ],
            "proof_artifacts": proofs,
            "negative_proof": ([
                "Before/after validator observations include the incremented revision and temporary action state",
                "Raised post-action validation rolls back run state, revision, operation row and event row",
            ] if name == NAMES[0] else [
                "Original thirteen mutation-command loop replays after terminal state without running either action or semantic callback",
                "Replay preserves the originally recorded revision",
            ]),
            "nondeterministic_relationships": ["Fresh owned temporary paths, SQLite file identity and timestamps; task/policy digests bind their actual bytes; cleanup remains confined to the resolved owned fixture"],
        })
    keysets = ("modules", "definitions", "commands", "mutation_rules", "test_identities")
    assert all(set(document[key]) == set(before[key]) for key in keysets)
    assert document["counts"] == before["counts"]
    assert all(document["test_identities"][key] == row for key, row in before["test_identities"].items() if key not in promoted)
    assert all(document[key] == before[key] for key in before if key != "test_identities")
    assert {key for key, row in document["test_identities"].items() if row["status"] == "proven"} == actual_ids
    assert coverage_path.read_bytes() == before_bytes
    assert test_path.read_bytes() == test_bytes and fixture_path.read_bytes() == fixture_bytes
    audit_hash = digest(audit_path.read_bytes())
    after_bytes = (json.dumps(document, ensure_ascii=True, sort_keys=True, indent=2) + "\n").encode("utf-8")
    output.mkdir(parents=True, exist_ok=False)
    (output / "coverage-before.json").write_bytes(before_bytes)
    (output / "coverage-update-script.py").write_bytes(Path(__file__).read_bytes())
    coverage_path.write_bytes(after_bytes)
    (output / "coverage-after.json").write_bytes(after_bytes)
    receipt = {
        "status": "PARTIAL_ORIGINAL_COVERAGE", "promoted": sorted(promoted),
        "prior_proven": 102, "current_proven": 104, "total_original": 360, "remaining": 256,
        "prior102_parsed_rows_equal": True, "all_other_rows_and_root_values_equal": True,
        "keysets_preserved": {key: len(document[key]) for key in keysets},
        "report_sha256": digest(report_path.read_bytes()),
        "previous_report_sha256": digest(previous_path.read_bytes()),
        "coverage_before_sha256": digest(before_bytes), "coverage_after_sha256": digest(after_bytes),
        "run_receipt_sha256": digest((checkpoint / "receipt.json").read_bytes()),
        "supplement_receipt_sha256": digest((supplement / "receipt.json").read_bytes()),
        "supervisor_audit_sha256": audit_hash,
        "source_test_sha256": digest(test_bytes), "source_fixture_sha256": digest(fixture_bytes),
        "run_source_binding": {"full_authored_files": len(manifest["source"]), "report_filtered_files": len(report["native_source"]), "archive": str(checkpoint / "source")},
        "input_binding": {"records": 47, "historical_copied_paths": 44, "supplement_unique_copies": 47, "original_run_after_observed_hashes_serialized": False, "run_after_evidence": "Retained empty input_changes array from the per-record full-source-path hash mismatch guard; no separately serialized observed hash per input"},
        "metadata_follows_run": "Only coverage/tool metadata changes follow the run; they are not retroactively included in its source binding",
        "skills": ["clean-code", "clean-architecture-code"],
        "reference_invocations": 0, "registry_reruns": 0,
        "remaining_scope": "256 original identities and the remaining full engine/CLI/private/error contracts; no final verification, timing or delivery claim",
    }
    receipt_path = output / "receipt.json"
    receipt_path.write_text(json.dumps(receipt, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"proven": 104, "remaining": 256, "prior102_preserved": True, "coverage_sha256": digest(after_bytes), "receipt_sha256": digest(receipt_path.read_bytes())}))


if __name__ == "__main__":
    main()
