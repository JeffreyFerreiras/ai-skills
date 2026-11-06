"""Promote source-bound helper equivalents while preserving every frozen identity."""

import argparse
import hashlib
import json
import os
from pathlib import Path

EXPECTED = {"modules": 17, "definitions": 383, "commands": 39, "mutation_rules": 25, "test_identities": 360}
IMPLEMENTATIONS = {
    "HelperRegister": "src/application/helper.rs::HelperRegister",
    "HelperRegister.__init__": "src/application/helper.rs::HelperRegister;src/bin/helper-register.rs::execute",
    "HelperRegisterError": "src/domain/helper_register.rs::HelperRegisterError",
    "HelperRegisterError.__init__": "src/domain/helper_register.rs::HelperRegisterError::new",
    "HelperRegisterPersistence": "src/application/helper_repository.rs::HelperRegisterPersistence",
    "HelperRegisterPersistence.read_bound": "src/application/helper_repository.rs::HelperRegisterPersistence::read_bound",
    "HelperRegisterPersistence.transaction": "src/application/helper_repository.rs::HelperRegisterPersistence::transaction",
    "HelperRegisterRepository": "src/application/helper_repository.rs::HelperRegisterRepository",
    "HelperRegisterRepository.__init__": "src/application/helper_repository.rs::HelperRegisterRepository;src/bin/helper-register.rs::execute",
    "HelperReservationLedger": "src/application/helper_ledger.rs::HelperReservationLedger",
    "HelperReservationLedger.__init__": "src/application/helper_ledger.rs::HelperReservationLedger",
    "RegisterAccountingInvariantChecker": "src/application/helper_validation.rs::check_accounting",
    "RegisterAccountingInvariantChecker.check": "src/application/helper_validation.rs::check_accounting",
    "RegisterRecordValidator": "src/application/helper_validation.rs::validate_record",
    "RegisterRecordValidator.validate": "src/application/helper_validation.rs::validate_record",
    "RegisterTransaction": "src/application/helper_repository.rs::RegisterTransaction",
    "RegisterTransaction.commit": "src/application/helper_repository.rs::RegisterTransaction::commit",
    "_FileRegisterTransaction": "src/application/helper_repository.rs::FileRegisterTransaction",
    "_FileRegisterTransaction.__init__": "src/application/helper_repository.rs::FileRegisterTransaction",
    "_FileRegisterTransaction.commit": "src/application/helper_repository.rs::FileRegisterTransaction::commit",
    "_allowance_location": "src/application/helper_repository.rs::HelperRegisterRepository::allowance_location",
    "_exclusive_lock": "src/infrastructure/helper_files.rs::NativeHelperFileTransactions::exclusive_lock",
    "_private_directory": "src/application/helper_repository.rs::HelperRegisterRepository::private_directory",
    "_test_mode_metadata": "src/application/helper_repository.rs::test_mode_metadata",
    "_verify_checkpoint": "src/application/helper_ledger.rs::HelperReservationLedger::verify_checkpoint",
    "_verify_test_repository": "src/application/helper_repository.rs::HelperRegisterRepository::verify_test_repository",
    "preflight_record": "src/application/helper_ledger.rs::HelperReservationLedger::preflight_record",
    "validate_allowance": "src/domain/helper_register.rs::validate_allowance",
    "validate_host_observation": "src/domain/helper_register.rs::validate_host_observation",
    "_integer": "src/domain/helper_register.rs::integer",
    "_unique_ids": "src/domain/helper_register.rs::unique_ids",
    "_scope_refs": "src/domain/helper_register.rs::scope_refs",
    "_limits": "src/domain/helper_register.rs::limits",
    "_commands": "src/domain/helper_register.rs::commands",
    "_parent_capabilities": "src/domain/helper_register.rs::parent_capabilities",
    "_ref_within": "src/domain/helper_register.rs::ref_within",
    "_trusted_observation_source": "src/domain/helper_register.rs::trusted_observation_source",
    "_validate_test_mode": "src/domain/helper_register.rs::validate_test_mode",
    "_assignment_for": "src/application/helper_validation.rs::assignment_for",
    "_filesystem_identity": "src/application/helper_validation.rs::filesystem_identity",
    "_register_key": "src/application/helper_validation.rs::register_key",
    "_required_amounts": "src/application/helper_validation.rs::required_amounts",
    "_validate_context": "src/application/helper_validation.rs::validate_context",
    "_validate_plan": "src/application/helper_validation.rs::validate_plan",
    "_validate_request": "src/application/helper_validation.rs::validate_request",
    "HelperRegister.initialize": "src/application/helper.rs::HelperRegister::initialize",
    "HelperRegister.preflight": "src/application/helper.rs::HelperRegister::preflight",
    "HelperRegister.reserve": "src/application/helper.rs::HelperRegister::reserve",
    "HelperRegister.settle": "src/application/helper.rs::HelperRegister::settle",
    "HelperRegister.status": "src/application/helper.rs::HelperRegister::status",
    "HelperReservationLedger.preflight": "src/application/helper_ledger.rs::HelperReservationLedger::preflight",
    "HelperReservationLedger.reserve": "src/application/helper_ledger.rs::HelperReservationLedger::reserve",
    "HelperReservationLedger.settle": "src/application/helper_ledger.rs::HelperReservationLedger::settle",
    "HelperReservationLedger.status": "src/application/helper_ledger.rs::HelperReservationLedger::status",
    "HelperRegisterRepository._bound_path": "src/application/helper_repository.rs::HelperRegisterRepository::bound_path",
    "HelperRegisterRepository._bound_record": "src/application/helper_repository.rs::HelperRegisterRepository::bound_record",
    "HelperRegisterRepository._read": "src/application/helper_repository.rs::HelperRegisterRepository::read",
    "HelperRegisterRepository._write": "src/application/helper_repository.rs::HelperRegisterRepository::write",
    "HelperRegisterRepository.initialize": "src/application/helper_repository.rs::HelperRegisterRepository::initialize",
    "HelperRegisterRepository.read_bound": "src/application/helper_repository.rs::HelperRegisterRepository::read_bound",
    "HelperRegisterRepository.transaction": "src/application/helper_repository.rs::HelperRegisterRepository::transaction"
}

def sha256(payload):
    return hashlib.sha256(payload).hexdigest()

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--expected-sha256", required=True)
    parser.add_argument("--registry", type=Path, required=True)
    parser.add_argument("--receipt", required=True)
    args = parser.parse_args()
    if not args.receipt.startswith("coverage-update-") or not args.receipt.removeprefix("coverage-update-").isdigit():
        raise ValueError("INVALID_RECEIPT_NAME")
    package = Path(__file__).resolve().parent.parent
    root = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve(strict=True)
    path = package / "parity/coverage-map.json"
    original = path.read_bytes()
    if sha256(original) != args.expected_sha256:
        raise ValueError("COVERAGE_EXPECTED_BYTE_HASH_MISMATCH")
    data = json.loads(original)
    before = {key: set(data[key]) for key in EXPECTED}
    if any(len(before[key]) != count for key, count in EXPECTED.items()):
        raise ValueError("COVERAGE_INVENTORY_COUNT_CHANGED")
    promoted = {key: value for key, value in data["test_identities"].items() if value["status"] == "proven"}
    if len(promoted) != 57:
        raise ValueError("UNEXPECTED_EXISTING_PROMOTIONS")
    helper_ids = {key for key in data["test_identities"] if key.startswith("tests.test_graph_hardening.HelperRegisterTests.")}
    if len(helper_ids) != 34 or helper_ids & promoted.keys():
        raise ValueError("HELPER_IDENTITY_SET_INVALID")
    registry_path = args.registry.resolve(strict=True)
    registry_path.relative_to(root)
    registry = json.loads(registry_path.read_bytes())
    records = {row["identity"]: row for row in registry["records"]}
    if len(records) != len(registry["records"]) or set(records) != set(promoted) | helper_ids or any(row["exit"] != 0 or row.get("timeout", False) for row in records.values()):
        raise ValueError("CURRENT_REGISTRY_ASSERTIONS_NOT_PROVED")
    if registry["registered"] != 91 or registry["original_count"] != 360 or len(registry["remaining_original"]) != 269:
        raise ValueError("REGISTRY_SCOPE_MISMATCH")
    if set(registry["native_source"]) != {candidate.relative_to(package).as_posix() for candidate in package.rglob("*") if candidate.is_file() and (candidate.suffix in {".rs", ".toml", ".manifest", ".sql"} or candidate.name == "Cargo.lock")}:
        raise ValueError("NATIVE_SOURCE_SET_CHANGED")
    for relative, digest in registry["native_source"].items():
        if sha256((package / relative).read_bytes()) != digest:
            raise ValueError("NATIVE_SOURCE_BINDING_CHANGED:" + relative)
    if len(registry["auxiliary_binaries"]) != 1:
        raise ValueError("HELPER_BINARY_NOT_BOUND")
    for binary in registry["auxiliary_binaries"]:
        candidate = Path(binary["path"]).resolve(strict=True)
        candidate.relative_to(root)
        if sha256(candidate.read_bytes()) != binary["sha256"]:
            raise ValueError("HELPER_BINARY_CHANGED")
    for key in ("TEMP", "TMP", "TMPDIR"):
        if Path(registry["environment"][key]).resolve(strict=True) != root / "fixtures":
            raise ValueError("FIXTURE_TEMP_ENVIRONMENT_MISMATCH:" + key)
    proof = "ART/" + registry_path.relative_to(root).as_posix()
    for identity in helper_ids:
        name = identity.rsplit(".", 1)[1]
        data["test_identities"][identity].update(
            status="proven", native_component="src/suite/helper_originals.rs", native_symbol=name,
            native_assertions=["src/suite/helper_originals.rs::" + name, "graph-suite --identity " + identity],
            fixtures=["src/suite/helper_fixture.rs::Fixture::prepare/new/request", "src/suite/task_fixture.rs::TaskFixture", "unchanged skills/software-engineering-graph/tests/fixtures/engineering-graph.json", "source-equivalent policy copy/hardlink/text setup and verified ART teardown"],
            proof_artifacts=[proof, "ART/build/helper-originals-04.log"],
            boundary="native equivalent of original import/private/fault assertions; real native CLI for process-bound cases",
            negative_proof=["src/suite/helper_originals.rs::" + name],
            nondeterministic_relationships=["fresh ART fixture paths and current native device/inode identities", "hashes bind actual fixture bytes", "concurrent reservation winner unspecified; result pair and cumulative counters exact"])
    definitions = {"helper_register." + name: native for name, native in IMPLEMENTATIONS.items()}
    if set(definitions) != {key for key in data["definitions"] if key.startswith("helper_register.")}:
        raise ValueError("HELPER_DEFINITION_SET_MISMATCH")
    for key, native in definitions.items():
        data["definitions"][key].update(status="implemented", native_component=native,
            native_assertions=["src/suite/helper_originals.rs::CASES", "parity/helper_policy.py"],
            proof_artifacts=[proof, "ART/parity/helper-policy-03/report.json"],
            fixtures=["src/suite/helper_fixture.rs", "src/suite/task_fixture.rs"],
            negative_proof=["ART/parity/helper-policy-03/requests.ndjson", "src/suite/helper_originals.rs"])
    limitations = ["Native tempfile candidate selection/cache/name-collision exhaustion remains incomplete", "Uncaught CLI traceback formatting, other codecs/interactive console and full five-command external scenarios remain pending", "Windows runtime proof; Unix source branches not run"]
    data["modules"]["helper_register"].update(status="partial",
        native_component="src/domain/helper_register.rs;src/application/helper_validation.rs;src/application/helper_repository.rs;src/application/helper_ledger.rs;src/application/helper.rs;src/infrastructure/helper_files.rs",
        native_assertions=["src/suite/helper_originals.rs::CASES", "tests/helper_register.rs", "parity/helper_policy.py"],
        fixtures=["src/suite/helper_fixture.rs", "src/suite/task_fixture.rs"], proof_artifacts=[proof, "ART/parity/helper-policy-03/report.json"],
        negative_proof=["src/suite/helper_originals.rs", "ART/parity/helper-stream-encodings-01/report.json"], limitations=limitations)
    for command in ("initialize", "preflight", "reserve", "settle", "status"):
        data["commands"]["helper-register " + command].update(status="partial",
            native_component="src/bin/helper-register.rs::execute;src/compat/argparse.rs::helper_arguments",
            native_assertions=["src/bin/helper-parser-probe.rs", "src/suite/helper_originals.rs"],
            fixtures=["tools/probe_helper_parser.py", "tools/probe_helper_streams.py"],
            proof_artifacts=[proof, "ART/parity/helper-parser-native-02/report.json", "ART/parity/helper-parser-widths-native-01/20.json", "ART/parity/helper-parser-widths-native-01/40.json", "ART/parity/helper-parser-widths-native-01/120.json", "ART/parity/helper-stream-encodings-02/report.json"],
            negative_proof=["ART/parity/helper-parser-native-01/report.json", "ART/parity/helper-streams-01/report.json", "ART/parity/helper-stream-encodings-01/report.json"], limitations=limitations)
    if any(set(data[key]) != keys for key, keys in before.items()) or any(data["test_identities"][identity] != value for identity, value in promoted.items()):
        raise ValueError("COVERAGE_EXISTING_IDENTITIES_CHANGED")
    if {key for key, value in data["test_identities"].items() if value["status"] == "proven"} != set(promoted) | helper_ids:
        raise ValueError("COVERAGE_PROMOTION_SET_MISMATCH")
    receipt = root / (args.receipt + ".json")
    backup = root / ("coverage-map-before-" + args.receipt.removeprefix("coverage-") + ".json.txt")
    if receipt.exists() or backup.exists():
        raise ValueError("IMMUTABLE_RECEIPT_EXISTS")
    revised = (json.dumps(data, sort_keys=True, separators=(",", ":")) + "\n").encode("ascii")
    if path.read_bytes() != original:
        raise ValueError("COVERAGE_CHANGED_DURING_UPDATE")
    backup.write_bytes(original)
    path.write_bytes(revised)
    if path.read_bytes() != revised:
        raise ValueError("COVERAGE_WRITE_MISMATCH")
    report = {"before_sha256": sha256(original), "after_sha256": sha256(revised), "counts": EXPECTED, "previous_proven": 57, "new_proven": 91, "promoted_identities": sorted(helper_ids), "native_definition_mappings": len(definitions), "registry": str(registry_path), "registry_sha256": sha256(registry_path.read_bytes()), "scope": "34 executed native original equivalents,61 implementation mappings; helper module andfiveCLI rows remainpartial"}
    receipt.write_bytes((json.dumps(report, indent=2) + "\n").encode("ascii"))
    print(json.dumps({key: value for key, value in report.items() if key != "promoted_identities"}, sort_keys=True))

if __name__ == "__main__":
    main()
