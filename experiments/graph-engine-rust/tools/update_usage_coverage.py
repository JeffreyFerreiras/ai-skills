"""Guarded component metadata update; this never promotes original test identities."""

import copy
import hashlib
import json
import os
from pathlib import Path


EXPECTED_RAW_SHA256 = "9dffe72a5a6c1b9d273677b1f15944628d67901a85213e783f66c049b967e52a"
PREFIX = "experiments/graph-engine-rust/"
IMPLEMENTATIONS = {
    "usage.Source": ("src/domain/usage.rs", "Source"),
    "usage.Source.checkpoint": ("src/domain/usage_contracts.rs", "Source::checkpoint"),
    "usage._add_diagnostic": ("src/domain/usage.rs", "Interval::add_diagnostic"),
    "usage._advance": ("src/domain/usage.rs", "Interval::advance"),
    "usage._association": ("src/application/usage_bindings.rs", "association"),
    "usage._bindings": ("src/application/usage_bindings.rs", "load_bindings"),
    "usage._bounded_json": ("src/domain/usage.rs", "bounded_json"),
    "usage._context": ("src/domain/usage.rs", "Context::from_value"),
    "usage._counters": ("src/domain/usage.rs", "Counters::from_value"),
    "usage._empty_totals": ("src/domain/usage.rs", "Counters::zero"),
    "usage._identity": ("src/infrastructure/filesystem.rs", "FileState::identity"),
    "usage._integer": ("src/domain/usage_contracts.rs", "integer"),
    "usage._read_source": ("src/infrastructure/usage_files.rs", "read_prefix_observed"),
    "usage._safe_components": ("src/infrastructure/usage_files.rs", "safe_components"),
    "usage._source_counters": ("src/domain/usage.rs", "Counters::from_source"),
    "usage._sum_into": ("src/domain/usage.rs", "Counters::sum_into"),
    "usage._summary": ("src/domain/usage.rs", "summarize"),
    "usage._validate_binding": ("src/domain/usage_contracts.rs", "validate_binding"),
    "usage.checkpoint": ("src/application/usage_source.rs", "checkpoint"),
    "usage.read_source": ("src/application/usage_source.rs", "read_source"),
    "usage.record": ("src/application/usage_record.rs", "prepare+PreparedUsage::apply+PreparedUsage::execute"),
    "usage.report": ("src/application/usage_report.rs+src/domain/usage_report.rs", "report"),
    "usage.validate_checkpoint": ("src/domain/usage_contracts.rs", "validate_checkpoint"),
}
PROOFS = [
    "ART/parity/usage-native-02/report.json",
    "ART/parity/usage-db-comparison-01/report.json",
    "ART/parity/usage-buffer-native-01/report.json",
    "ART/parity/usage-buffer-append-native-01/report.json",
    "ART/build/usage-durable-tests-03.log",
    "ART/build/usage-library-tests-01.log",
]
ASSERTIONS = [
    PREFIX + "tests/usage.rs::usage_component_replays_exact_prior_response_after_new_appends_and_rejects_conflicting_close",
    PREFIX + "tests/usage.rs::usage_component_losing_prepared_checkpoint_cannot_replace_winner_or_advance_revision",
    PREFIX + "tests/usage.rs::usage_component_fault_rolls_back_observation_operation_and_revision_together",
    PREFIX + "tests/usage.rs::usage_component_json_corruption_preempts_later_bad_sqlite_text_but_storage_failure_still_propagates",
    PREFIX + "tests/usage.rs::usage_component_large_totals_remain_exact_and_optional_missing_source_keeps_prior_coverage_partial",
    PREFIX + "tests/usage.rs::usage_component_historical_checkpoint_and_all_phase_contexts_preserve_exact_attribution",
    PREFIX + "tests/usage.rs::usage_component_supervisor_and_generation_groups_keep_unobserved_scoping_partial",
    PREFIX + "tests/usage.rs::usage_component_negative_open_faults_reject_sources_and_close_real_descriptors",
]
NEGATIVES = [
    "ART/parity/usage-pure-01/records.jsonl: malformed counters, context mappings, binding/checkpoint values and nesting",
    "ART/parity/usage-db-01/report.json: overlap/conflicting replay/CAS, prefix reset/corruption, missing source and lazy bad-JSON-before-bad-TEXT order",
    "ART/parity/usage-buffer-append-01/report.json: actual cached-prefix behavior at 1/8191 and changed-prefix rejection at 8192/8193",
    ASSERTIONS[2], ASSERTIONS[7],
]


def main():
    repo = Path(__file__).resolve().parents[3]
    path = repo / PREFIX / "parity/coverage-map.json"
    artifact = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"])
    before_path = artifact / "coverage-usage-before-01.json"
    receipt_path = artifact / "coverage-usage-update-01.json"
    if before_path.exists() or receipt_path.exists():
        raise RuntimeError("immutable coverage update destination already exists")
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != EXPECTED_RAW_SHA256:
        raise RuntimeError("coverage raw byte guard failed")
    data = json.loads(raw)
    before = copy.deepcopy(data)
    sizes = {"modules": 17, "definitions": 383, "commands": 39, "mutation_rules": 25, "test_identities": 360}
    for section, size in sizes.items():
        if len(data[section]) != size:
            raise RuntimeError("coverage identity count changed: " + section)
    if sum(row["status"] == "proven" for row in data["test_identities"].values()) != 91:
        raise RuntimeError("existing promotion count changed")
    if {key for key in data["definitions"] if key.startswith("usage.")} != IMPLEMENTATIONS.keys():
        raise RuntimeError("usage definition identities changed")
    components = sorted({PREFIX + path for path, _ in IMPLEMENTATIONS.values()})
    module = data["modules"]["usage"]
    module.update(status="partial", native_component="+".join(components), native_assertions=ASSERTIONS,
                  proof_artifacts=PROOFS, negative_proof=NEGATIVES,
                  fixtures=["ART/parity/usage-pure-01/records.jsonl", "ART/parity/usage-db-01/actions.json"],
                  proven_scope="Written native boundaries and recorded component evidence only. Source/binary bindings are those in each immutable report; current-source rebinding, all28 full-fixture originals, branch association and CLI integration remain pending.")
    for key, (component, symbol) in IMPLEMENTATIONS.items():
        row = data["definitions"][key]
        partial = key in {"usage._association", "usage._read_source", "usage._safe_components", "usage.read_source", "usage.record", "usage.report"}
        row.update(status="partial" if partial else "implemented", native_component=PREFIX + component,
                   native_symbol=symbol, native_assertions=ASSERTIONS, proof_artifacts=PROOFS,
                   negative_proof=NEGATIVES, fixtures=module["fixtures"],
                   proven_scope="Native counterpart written; listed component evidence retains its own source/binary bindings. Full original-identity/CLI integration credit is pending.")
    for command, component in [("graphctl record usage", "src/application/usage_record.rs"),
                                ("graphctl usage checkpoint", "src/application/usage_source.rs")]:
        data["commands"][command].update(status="partial", backend_component=PREFIX + component,
                                         native_assertions=ASSERTIONS, proof_artifacts=PROOFS, negative_proof=NEGATIVES,
                                         proven_scope="Backend API component only; native graphctl parser/help/error/exit and full CLI workload remain pending.")
    mutation = data["mutation_rules"]["record.usage"]
    mutation["native_component"] += "+src/application/usage_record.rs+src/application/usage_store.rs+src/infrastructure/usage_database.rs"
    mutation["native_assertions"] += ASSERTIONS
    mutation["proof_artifacts"] += PROOFS
    mutation["negative_proof"] += NEGATIVES
    mutation["proven_scope"] = "Admission/replay/conflict matrix plus scoped actual usage transaction/CAS/rollback component evidence; private harness no-op semantic callback, full graphctl/core-semantic/full-fixture closure remains pending."
    for section in sizes:
        if set(data[section]) != set(before[section]):
            raise RuntimeError("coverage identity keyset changed: " + section)
    if data["test_identities"] != before["test_identities"] or data["counts"] != before["counts"]:
        raise RuntimeError("original identities/promotions or inventory counts changed")
    allowed = {"modules": {"usage"}, "definitions": set(IMPLEMENTATIONS),
               "commands": {"graphctl record usage", "graphctl usage checkpoint"}, "mutation_rules": {"record.usage"}}
    for section, keys in allowed.items():
        if any(data[section][key] != value for key, value in before[section].items() if key not in keys):
            raise RuntimeError("unrelated coverage row changed")
    encoded = (json.dumps(data, sort_keys=True, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii")
    if path.read_bytes() != raw:
        raise RuntimeError("coverage changed concurrently")
    before_path.write_bytes(raw)
    path.write_bytes(encoded)
    receipt = {"before_raw_sha256": EXPECTED_RAW_SHA256, "after_raw_sha256": hashlib.sha256(encoded).hexdigest(),
               "before_snapshot": str(before_path), "definitions_bound": len(IMPLEMENTATIONS),
               "original_promotions_preserved": 91, "new_original_promotions": 0, "identity_counts": sizes,
               "scope": "usage component metadata only; all28 original usage-associated identities remain pending"}
    receipt_path.write_bytes((json.dumps(receipt, indent=2) + "\n").encode("ascii"))
    print(json.dumps(receipt))


if __name__ == "__main__":
    main()
