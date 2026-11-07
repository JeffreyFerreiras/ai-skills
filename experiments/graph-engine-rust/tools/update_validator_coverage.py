"""Promote only the eight guarded original ValidatorTests identities."""

import argparse
import ast
import hashlib
import json
import os
from pathlib import Path


NAMES = [
    "test_decision_precedence_is_order_independent",
    "test_delivery_dominance",
    "test_out_of_scope_findings_do_not_raise_delivery_outcome",
    "test_stored_cross_phase_dispositions_remain_valid_on_reverification",
    "test_only_runs_from_earlier_engines_keep_shared_dispositions_on_reverification",
    "test_delivery_rejects_in_scope_approve_disposition",
    "test_unknown_finding_scope_is_rejected",
    "test_duplicate_and_conflicting_finding_ids_are_rejected",
]
PREFIX = "tests.test_validator.ValidatorTests."


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--previous-report", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    package = Path(__file__).resolve().parent.parent
    artifacts = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve(strict=True)
    args.output.resolve().relative_to(artifacts)
    if args.output.exists():
        raise FileExistsError("Refuse to replace a coverage receipt")
    report_bytes, previous_bytes = args.report.read_bytes(), args.previous_report.read_bytes()
    report, previous = json.loads(report_bytes), json.loads(previous_bytes)
    old_ids = {record["identity"] for record in previous["records"]}
    actual = {record["identity"] for record in report["records"]}
    new_ids = {PREFIX + name for name in NAMES}
    assert len(old_ids) == 94 and len(actual) == 102
    assert actual == old_ids | new_ids and not old_ids & new_ids
    assert len(report["records"]) == 102 and all(record["exit"] == 0 and not record["timeout"] for record in report["records"])
    coverage_path = package / "parity/coverage-map.json"
    before_bytes = coverage_path.read_bytes()
    assert sha(before_bytes) == report["bindings"]["source_files_sha256"]["parity/coverage-map.json"]
    before = json.loads(before_bytes)
    document = json.loads(before_bytes)
    assert {identity for identity, row in before["test_identities"].items() if row["status"] == "proven"} == old_ids
    oracle = package.parent.parent / "skills/software-engineering-graph"
    tests_path, support_path = oracle / "tests/test_validator.py", oracle / "tests/test_support.py"
    test_bytes, support_bytes = tests_path.read_bytes(), support_path.read_bytes()
    test_class = next(node for node in ast.parse(test_bytes).body if isinstance(node, ast.ClassDef) and node.name == "ValidatorTests")
    methods = {node.name: node for node in test_class.body if isinstance(node, ast.FunctionDef)}
    assert all(name in methods for name in NAMES)
    fixtures = [
        "tests/support/graph_case.rs::GraphFixture::new/tear_down, real owned files and native StateStore constructor",
        "Source GraphCase.setUp/tearDown: resolved temporary root, .codex/docs/artifacts, exact platform text bytes, data-only policy copy and profile hardlink, fake skill/runtime roots, counter0 and cleanup",
        "Source TaskCase.task and ValidatorTests._review_branch/_review_result data helpers where called",
        "Unchanged skills/software-engineering-graph/tests/fixtures/engineering-graph.json",
    ]
    proofs = ["ART/parity/registry-10/report.json", "ART/build/consolidation-originals-tests-01.log", "ART/build/graphcase-fixture-tests-03.log"]
    for name in NAMES:
        identity = PREFIX + name
        symbol = name.removeprefix("test_")
        node = methods[name]
        document["test_identities"][identity].update({
            "status": "proven",
            "boundary": "All original assertions with per-identity GraphCase setup/teardown; pure import-level validation, no graph CLI operation is called by these source methods.",
            "oracle_path": "skills/software-engineering-graph/tests/test_validator.py",
            "oracle_lines": [node.lineno, node.end_lineno], "oracle_sha256": sha(test_bytes),
            "native_component": "src/suite/validator_originals.rs", "native_symbol": symbol,
            "native_assertions": ["src/suite/validator_originals.rs::" + symbol, "graph-suite --identity " + identity],
            "fixtures": fixtures, "proof_artifacts": proofs,
            "negative_proof": ["Original error, precedence and rejection assertions retained where present in this exact source identity"],
            "nondeterministic_relationships": ["Fresh owned fixture paths/device/inode; exact policy bytes and hardlink relationship; guarded cleanup removes only the resolved owned root"],
        })
    definitions = {
        "compute_design_outcome": "src/domain/consolidation.rs::compute_design_outcome",
        "compute_delivery_outcome": "src/domain/consolidation.rs::compute_delivery_outcome",
        "_source_finding_dispositions": "src/domain/consolidation.rs::source_finding_dispositions",
        "_delegated_issue_sources": "src/domain/consolidation.rs::delegated_issue_sources",
        "_delegated_terminal_precedence": "src/domain/consolidation.rs::delegated_terminal_precedence",
        "_delivery_outcome_without_delegated_echoes": "src/domain/consolidation.rs::delivery_outcome_without_delegated_echoes",
        "validate_consolidation_manifest": "src/domain/consolidation.rs::validate_consolidation_manifest",
        "_predates_phase_dispositions": "src/application/semantic_nodes.rs::predates_phase_dispositions",
    }
    for name, component in definitions.items():
        document["definitions"]["validator." + name].update({
            "status": "implemented", "native_component": component,
            "proof_artifacts": proofs,
            "limitations": ["Authored implementation; eight original identities are separately proved, not universal malformed/private-input or delegated-collection parity."],
        })
    document["modules"]["validator"].update({
        "status": "partial",
        "native_component": "src/domain/consolidation.rs;src/application/semantic_nodes.rs;src/application/semantic_joins.rs;src/application/review_binding.rs;src/application/execution_validation.rs;src/application/state_history.rs;src/application/timing.rs",
        "fixtures": fixtures, "native_assertions": ["src/suite/validator_originals.rs::CASES"], "proof_artifacts": proofs,
        "limitations": ["Full semantic orchestration, topology/fanout/delegation checks, seven remaining persisted ValidatorTests and broader private-input/exception contracts remain incomplete."],
    })
    for key in ("modules", "definitions", "commands", "mutation_rules", "test_identities"):
        assert set(document[key]) == set(before[key]), key
    assert document["counts"] == before["counts"]
    assert all(document["test_identities"][identity] == before["test_identities"][identity] for identity in old_ids)
    assert {identity for identity, row in document["test_identities"].items() if row["status"] == "proven"} == actual
    assert test_bytes == tests_path.read_bytes() and support_bytes == support_path.read_bytes()
    after_bytes = (json.dumps(document, ensure_ascii=True, sort_keys=True, indent=2) + "\n").encode("utf-8")
    args.output.mkdir(parents=True)
    (args.output / "coverage-before.json").write_bytes(before_bytes)
    coverage_path.write_bytes(after_bytes)
    (args.output / "coverage-after.json").write_bytes(after_bytes)
    receipt = {
        "schema_version": 1, "status": "PARTIAL_ORIGINAL_COVERAGE", "promoted": sorted(new_ids),
        "prior_proven": 94, "current_proven": 102, "total_original": 360, "remaining": 258,
        "prior94_parsed_records_equal": True,
        "preserved_record_scope": "Coverage test_identities rows compared as parsed JSON values; prior registry artifacts are immutable, and new process PIDs are not claimed identical.",
        "report_sha256": sha(report_bytes), "previous_report_sha256": sha(previous_bytes),
        "coverage_before_sha256": sha(before_bytes), "coverage_after_sha256": sha(after_bytes),
        "source_test_sha256": sha(test_bytes), "source_fixture_sha256": sha(support_bytes),
        "native_source_binding": {"entries": len(report["native_source"]), "scope": "Filtered Rust/build/lock/SQL/Windows-manifest source before/after only; this is not an all-authored-file snapshot.", "separate_bindings": "report.bindings captures its declared formats after the run; exact frozen source artifacts and prior receipts remain unchanged."},
        "coverage_metadata_follows_test": "Only coverage metadata/tooling is added after the guarded registry run; no retroactive source-binding claim.",
        "keysets_preserved": {key: len(document[key]) for key in ("modules", "definitions", "commands", "mutation_rules", "test_identities")},
        "skills": ["clean-code", "clean-architecture-code"],
        "remaining_scope": "Complete native engine/semantic/CLI work and258 original identities; no full parity, independent final verification, benchmark, publication or delivery claim.",
    }
    (args.output / "receipt.json").write_text(json.dumps(receipt, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"proven": 102, "remaining": 258, "promoted": 8, "prior94_preserved": True, "coverage_sha256": sha(after_bytes), "receipt_sha256": sha((args.output / "receipt.json").read_bytes())}))


if __name__ == "__main__":
    main()
