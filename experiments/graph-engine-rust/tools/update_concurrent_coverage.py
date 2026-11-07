"""Credit only the guarded original concurrent-claims identity."""

import argparse
import ast
import copy
import hashlib
import json
import os
from pathlib import Path


IDENTITY = "tests.test_state.StateTests.test_concurrent_claims_have_one_winner_and_one_conflict"


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def load(path):
    return json.loads(Path(path).read_bytes())


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    package = Path(__file__).resolve().parent.parent
    artifacts = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve(strict=True)
    output = args.output.resolve()
    output.relative_to(artifacts)
    if output.exists():
        raise FileExistsError("Coverage outcome already exists")
    checkpoint = artifacts / "checkpoints/concurrent-registry-before-01"
    before, receipt = load(checkpoint / "before.json"), load(checkpoint / "receipt.json")
    report_path, previous_path = artifacts / "parity/registry-14/report.json", artifacts / "parity/registry-13/report.json"
    report, previous = load(report_path), load(previous_path)
    assert receipt["build_exit"] == receipt["registry_exit"] == 0 and receipt["guards_unchanged"]
    assert sha(report_path) == receipt["report_sha256"]
    for record in before["source"]:
        assert sha(package / record["path"]) == record["sha256"]
        assert sha(Path(before["source_contents"]) / record["path"]) == record["sha256"]
    for record in before["frozen_inputs"]:
        assert sha(record["source"]) == record["sha256"]
        assert sha(Path(before["input_contents"]) / record["archive"]) == record["sha256"]
    for binary in receipt["binaries"]:
        assert sha(binary["path"]) == sha(binary["copy"]) == binary["sha256"]
    old_ids = {record["identity"] for record in previous["records"]}
    actual_ids = {record["identity"] for record in report["records"]}
    assert len(old_ids) == 104 and len(report["records"]) == len(actual_ids) == 105
    assert actual_ids == old_ids | {IDENTITY} and IDENTITY not in old_ids
    assert all(record["exit"] == 0 and not record["timeout"] for record in report["records"])
    coverage_path = package / "parity/coverage-map.json"
    original_bytes = coverage_path.read_bytes()
    original = json.loads(original_bytes)
    assert hashlib.sha256(original_bytes).hexdigest() == report["bindings"]["source_files_sha256"]["parity/coverage-map.json"]
    assert {key for key, row in original["test_identities"].items() if row["status"] == "proven"} == old_ids
    oracle = package.parent.parent / "skills/software-engineering-graph/tests"
    test_path, fixture_path = oracle / "test_state.py", oracle / "test_support.py"
    test_hash, fixture_hash = sha(test_path), sha(fixture_path)
    source_class = next(node for node in ast.parse(test_path.read_bytes()).body if isinstance(node, ast.ClassDef) and node.name == "StateTests")
    method = next(node for node in source_class.body if isinstance(node, ast.FunctionDef) and node.name == IDENTITY.rsplit(".", 1)[1])
    document = copy.deepcopy(original)
    document["test_identities"][IDENTITY].update({
        "status": "proven", "oracle_path": "skills/software-engineering-graph/tests/test_state.py",
        "oracle_lines": [method.lineno, method.end_lineno], "oracle_sha256": test_hash,
        "native_component": "src/suite/state_originals.rs", "native_symbol": "concurrent_claims_have_one_winner_and_one_conflict",
        "boundary": "Full original GraphCase setup/teardown, initialization/parser/dispatch, two claim threads, entered signal, 200ms before_commit hold, two-second wait, joins and sorted outcome assertion.",
        "native_assertions": ["src/suite/state_originals.rs::concurrent_claims_have_one_winner_and_one_conflict", "graph-suite --identity " + IDENTITY],
        "fixtures": [
            "tests/support/graph_case.rs::GraphFixture::new/tear_down",
            "tests/support/initialized_graph.rs actual in-process init and approval argv/parser/dispatch",
            "Native StateStore constructor for first_store, with the source runtime root and before_commit hook",
            "tests/support/concurrent_graph.rs safe per-thread views of the source store roots and stateless native adapters; no constructor IO repeats inside workers",
        ],
        "negative_proof": ["Exactly one CLAIMED and one CLAIM_CONFLICT outcome under the source entered/hold schedule"],
        "nondeterministic_relationships": ["Fresh owned fixture paths, timestamps, native OS entropy and actual SQLite file identity; preserve the exact source 200ms hold and two-second event wait"],
        "proof_artifacts": ["ART/parity/registry-14/report.json", "ART/checkpoints/concurrent-claim-original-before-01/receipt.json", "ART/checkpoints/concurrent-registry-before-01/receipt.json", "ART/supervisor-concurrent-claim-audit-01.json", "ART/supervisor-registry-14-audit-01.json"],
    })
    keysets = ("modules", "definitions", "commands", "mutation_rules", "test_identities")
    assert all(set(document[key]) == set(original[key]) for key in keysets)
    assert all(document[key] == original[key] for key in original if key != "test_identities")
    assert all(document["test_identities"][key] == row for key, row in original["test_identities"].items() if key != IDENTITY)
    assert {key for key, row in document["test_identities"].items() if row["status"] == "proven"} == actual_ids
    assert sha(test_path) == test_hash and sha(fixture_path) == fixture_hash
    assert coverage_path.read_bytes() == original_bytes
    audits = {name: sha(artifacts / name) for name in ["supervisor-concurrent-claim-audit-01.json", "supervisor-registry-14-audit-01.json", "supervisor-registry-14-log-manifest-01.json"]}
    output.mkdir(parents=True, exist_ok=False)
    (output / "coverage-before.json").write_bytes(original_bytes)
    (output / "coverage-update-script.py").write_bytes(Path(__file__).read_bytes())
    updated_bytes = (json.dumps(document, ensure_ascii=True, sort_keys=True, indent=2) + "\n").encode("utf-8")
    coverage_path.write_bytes(updated_bytes)
    (output / "coverage-after.json").write_bytes(updated_bytes)
    result = {"status": "PARTIAL_ORIGINAL_COVERAGE", "promoted": [IDENTITY], "prior_proven": 104,
              "current_proven": 105, "remaining": 255, "total_original": 360,
              "prior104_parsed_rows_equal": True, "all_other_rows_and_root_values_equal": True,
              "keysets": {key: len(document[key]) for key in keysets},
              "coverage_before_sha256": hashlib.sha256(original_bytes).hexdigest(), "coverage_after_sha256": sha(coverage_path),
              "report_sha256": sha(report_path), "previous_report_sha256": sha(previous_path),
              "run_receipt_sha256": sha(checkpoint / "receipt.json"), "audits": audits,
              "source_test_sha256": test_hash, "source_fixture_sha256": fixture_hash,
              "source_binding": {"full_authored_files": len(before["source"]), "report_filtered_files": len(report["native_source"]), "full_contents": before["source_contents"]},
              "input_binding": {"records": 59, "unique_indexed_copies": 59, "observed_after_hashes_serialized": True},
              "raw_log_hash_limit": "Supervisor log-manifest hashes are post-run per-file hashes, not claimed as during-run hashes",
              "metadata_follows_run": "Only coverage/tool metadata changes follow the run; not retroactively included in its source binding",
              "reference_invocations": 0, "registry_reruns": 0, "skills": ["clean-code", "clean-architecture-code"],
              "limits": "One supported original identity; not final independent review, complete engine, timing or delivery proof"}
    (output / "receipt.json").write_text(json.dumps(result, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"proven": 105, "remaining": 255, "prior104_preserved": True,
                      "coverage_sha256": sha(coverage_path), "receipt_sha256": sha(output / "receipt.json")}))


if __name__ == "__main__":
    main()
