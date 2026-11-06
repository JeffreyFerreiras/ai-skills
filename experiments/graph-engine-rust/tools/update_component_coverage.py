"""Emit a bounded patch for completed component mappings; keep workload credit separate."""

import hashlib
import argparse
import json
import os
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--receipt", default="coverage-update-01")
    parser.add_argument("--evidence", action="store_true")
    parser.add_argument("--expected-sha256", required=True)
    args = parser.parse_args()
    if not args.receipt.startswith("coverage-update-") or not args.receipt.removeprefix("coverage-update-").isdigit():
        raise ValueError("INVALID_COVERAGE_RECEIPT_NAME")
    package = Path(__file__).resolve().parent.parent
    path = package / "parity/coverage-map.json"
    original_bytes = path.read_bytes()
    if hashlib.sha256(original_bytes).hexdigest() != args.expected_sha256:
        raise ValueError("COVERAGE_EXPECTED_BYTE_HASH_MISMATCH")
    original = original_bytes.decode("utf-8")
    data = json.loads(original)
    expected = {"modules": 17, "definitions": 383, "commands": 39, "mutation_rules": 25, "test_identities": 360}
    if any(len(data[key]) != count for key, count in expected.items()):
        raise ValueError("COVERAGE_INVENTORY_COUNT_CHANGED")
    promoted = sum(record["status"] == "proven" for record in data["test_identities"].values())
    if promoted != 57:
        raise ValueError("UNEXPECTED_ORIGINAL_PROMOTION_STATE")
    before = {key: set(data[key]) for key in ("modules", "definitions", "commands", "mutation_rules", "test_identities")}
    def update(key, component, assertions, proofs, negatives=()):
        record = data["definitions"][key]
        record.update(status="implemented", native_component=component, native_assertions=list(assertions), proof_artifacts=list(proofs), negative_proof=list(negatives))
    private = ["ART/parity/config-private-02/report.json"]
    loader = ["ART/parity/policy-loader-02/report.json", "ART/parity/policy-02/report.json"]
    for name in ("_has_location_duplicates", "_location_key", "_roots_overlap", "_under_configured_root", "branch_lease_seconds", "role_capability_allowed"):
        native = name.lstrip("_")
        update("config." + name, "src/domain/policy_validation.rs::" + native, ["parity/config_private.py::records/oracle"], private, ["ART/parity/config-private-02/requests.ndjson"])
    for source, native in [("_prove_capability_case_identity", "prove_capability_case_identity"), ("validate_task_capability_paths", "validate_task_capability_paths")]:
        update("config." + source, "src/application/policy.rs::" + native, ["parity/config_private.py", "tests/policy.rs::every_projected_role_pair_requires_its_own_case_identity"], private, ["ART/parity/config-private-02/requests.ndjson", "ART/build/config-consumers-02.log"])
    data["modules"]["config"].update(status="implemented", native_component="src/application/policy.rs;src/domain/policy.rs;src/domain/policy_validation.rs;src/infrastructure/policy_files.rs", native_assertions=["tests/policy.rs", "parity/policy.py", "parity/policy_loader.py", "parity/config_private.py"], proof_artifacts=loader + private, negative_proof=["ART/parity/policy-01/report.json", "ART/parity/config-private-02/requests.ndjson"], limitations=["Windows runtime proof; Unix branches not run", "case-sensitive sibling OS fixture Access denied; identical inode observed; fault equivalent and junction/ordinary alias checks proved", "raw generic I/O error-message fidelity remains pending adversarial completion"])
    store_proof = ["ART/parity/store-04/report.json", "ART/parity/store-02/report.json", "ART/build/persistent-store-order-01.log"]
    store_assertions = ["parity/store.py", "tests/persistent_store.rs"]
    application = {
        "StateStore": "StateStore", "StateStore.__init__": "StateStore::new", "StateStore._directory_sync": "StateStore::directory_sync", "StateStore._incomplete_database_request_digest": "StateStore::incomplete_database_request_digest", "StateStore._insert_node": "node_values", "StateStore._mkdir_private": "StateStore::mkdir_private", "StateStore._verified_connection": "StateStore::open_run/with_run", "StateStore._verify_private_directory": "StateStore::private_directory", "StateStore._write_incomplete_marker": "StateStore::write_incomplete_marker", "StateStore.db_path": "StateStore::db_path", "StateStore.inbox_root": "StateStore::inbox_root", "StateStore.initialize": "StateStore::initialize/initialize_locked/insert_initial_rows", "StateStore.open_run": "StateStore::open_run/with_run", "StateStore.run_root": "StateStore::run_root", "StateStore.verify_state_paths": "StateStore::verify_state_paths", "repository_identity": "StateStore::repository_identity",
    }
    for source, native in application.items():
        update("state." + source, "src/application/persistent_store.rs::" + native, store_assertions, store_proof, ["ART/parity/store-01/report.json", "ART/build/persistent-store-tests-02.log"])
    for source, native in [("_process_alive", "process_alive"), ("current_actor", "current_actor"), ("current_host_identity", "host_identity"), ("local_filesystem_identity", "locality"), ("utc_now", "utc_now")]:
        update("state." + source, "src/infrastructure/state_platform.rs::" + native, store_assertions, store_proof)
    update("state.StateStore._probe_sqlite_locking", "src/infrastructure/state_database.rs::NativeStateDatabase::locking_probe", ["tests/persistent_store.rs::initialize_reopen_and_identical_operation_replay_persist_authoritative_identity"], store_proof)
    update("state.StateStore.connect", "src/infrastructure/sqlite.rs::SqliteSession::connect_existing/connection_uri", ["parity/store.py::sqlite_uri cases", "tests/persistent_store.rs"], store_proof + ["ART/parity/sqlite-paths-02/report.json"], ["ART/parity/sqlite-paths-01/report.json"])
    update("state.StateStore.row_dict", "src/infrastructure/sqlite.rs::row_value", ["tests/sqlite_values.rs::row_dictionary_uses_first_ascii_case_insensitive_lookup/every_selected_cell_is_decoded_before_dictionary_duplicate_elision", "parity/sqlite_values.py"], ["ART/parity/sql-values-03/report.json"])
    for source, component in [("StateStore.insert_join", "src/application/persistent_store.rs::insert_join;src/infrastructure/state_database.rs::insert_join_row/insert_join_member"), ("StateStore.verify_inbox_manifest", "src/application/persistent_store.rs::StateStore::verify_inbox_manifest"), ("_linux_filesystem_type", "src/infrastructure/state_platform.rs::linux_filesystem_type")]:
        update("state." + source, component, [], [], ["pending focused assertion; Unix runtime unproved where applicable"])
    data["modules"]["state"].update(status="partial", native_component="src/application/state.rs;src/application/persistent_store.rs;src/domain/state.rs;src/infrastructure/state_database.rs;src/infrastructure/state_platform.rs;src/infrastructure/sqlite.rs", native_assertions=store_assertions + ["tests/state_transactions.rs", "tests/sqlite_values.rs"], proof_artifacts=store_proof, limitations=["No original StateTests identity promoted before full GraphCase/CLI/semantic workload", "installed_codex_home/utc_after and remaining adversarial/private assertions pending", "Unix runtime not run"])
    updated_modules = ["config", "state"]
    if args.evidence:
        evidence_proofs = ["ART/parity/artifact-pure-03/report.json", "ART/parity/artifact-files-04/report.json", "ART/build/crt-snapshot-consumers-02.log"]
        evidence_assertions = ["tests/artifacts.rs", "tests/snapshots.rs", "parity/artifacts.py", "parity/artifact_files.py"]
        pure = {"VerifiedArtifact": "VerifiedArtifact", "VerifiedArtifact.as_input": "VerifiedArtifact::as_input", "_kind_policy": "kind_policy", "_within_roots": "within_roots", "artifact_size_limit": "artifact_size_limit", "canonical_ledger_artifact": "canonical_ledger_artifact", "enforce_artifact_size": "enforce_artifact_size", "policy_path_allowed": "policy_path_allowed"}
        for source, native in pure.items():
            update("evidence." + source, "src/domain/evidence.rs::" + native, evidence_assertions, evidence_proofs)
        for name in ("persist_artifact", "resolve_reference", "resolve_unhashed_reference", "reverify_artifact"):
            update("evidence." + name, "src/application/artifacts.rs::" + ("" if name == "persist_artifact" else "ArtifactResolver::") + name + ";src/infrastructure/artifact_ledger.rs", evidence_assertions, evidence_proofs, ["ART/parity/artifact-files-01/report.json", "ART/build/crt-snapshot-consumers-01.log"])
        data["modules"]["evidence"].update(status="implemented", native_component="src/domain/evidence.rs;src/application/artifacts.rs;src/infrastructure/artifact_ledger.rs;src/infrastructure/artifact_files.rs", native_assertions=evidence_assertions, proof_artifacts=evidence_proofs, limitations=["Component assertions and differential scenarios do not promote original identities", "Source-fault final lstat identity proof is separate from real Windows CRT delete-sharing denial; Unix physical final swap branch not run", "Canonical native identifier is a PyString; original caller/test workload and remaining unexpected-type coverage still pending", "OS formatting proved for exercised WinAPI/CRT calls; no blanket OS/error-metadata equivalence claim"])
        updated_modules.append("evidence")
    for key, identities in before.items():
        if set(data[key]) != identities:
            raise ValueError("COVERAGE_IDENTITIES_CHANGED:" + key)
    if sum(record["status"] == "proven" for record in data["test_identities"].values()) != promoted:
        raise ValueError("ORIGINAL_PROMOTION_CHANGED")
    revised = json.dumps(data, separators=(",", ":"), sort_keys=True) + "\n"
    root = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"])
    report_path = root / (args.receipt + ".json")
    if report_path.exists():
        raise ValueError("IMMUTABLE_COVERAGE_UPDATE_EXISTS")
    (root / ("coverage-map-before-" + args.receipt.removeprefix("coverage-") + ".json.txt")).write_bytes(original_bytes)
    before_hash = hashlib.sha256(original_bytes).hexdigest()
    if path.read_bytes() != original_bytes:
        raise ValueError("COVERAGE_CHANGED_DURING_UPDATE")
    path.write_bytes(revised.encode("ascii"))
    verified = json.loads(path.read_text(encoding="ascii"))
    if any(set(verified[key]) != identities for key, identities in before.items()):
        raise ValueError("COVERAGE_WRITE_IDENTITY_MISMATCH")
    report = dict(before_sha256=before_hash, after_sha256=hashlib.sha256(path.read_bytes()).hexdigest(), identities=expected, original_promotions_unchanged=promoted, updated_modules=updated_modules, scope="authored component metadata; no new original-test credit")
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="ascii")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
