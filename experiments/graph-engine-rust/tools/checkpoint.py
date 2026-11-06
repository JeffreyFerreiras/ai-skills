"""Preserve a candid resumable implementation checkpoint under ART."""

import argparse
import datetime
import json
import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "parity"))
from bindings import capture


def main():
    parser = argparse.ArgumentParser()
    for field in ("reads", "read-ceiling", "discoveries", "discovery-ceiling"):
        parser.add_argument("--"+field, type=int, required=True)
    args = parser.parse_args()
    package = Path(__file__).resolve().parent.parent
    root = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve()
    binary = Path(os.environ["CARGO_TARGET_DIR"]) / "debug/graph-parity.exe"
    report = {
        "schema_version": 1, "timestamp_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "status": "IN_PROGRESS", "full_native_engine_implemented": False,
        "worktree": str(package.parent.parent), "artifact_root": str(root),
        "source_edit_scope": "experiments/graph-engine-rust/ only",
        "phase": "config and row/Bytes component parity verified; owned persistent-store implementation remains next",
        "skills": ["C:/Users/sephn/.agents/skills/clean-code/SKILL.md", "C:/Users/sephn/.agents/skills/clean-architecture-code/SKILL.md"],
        "skill_actions": ["Domain policies do not import SQLite/process/filesystem adapters.", "Snapshot application port is owned inward and implemented by native infrastructure.", "Fault observations are explicit native test seams; production CLI bypasses are not added.", "Names and control flow preserve source validation order and keep unrelated source untouched."],
        "inspection": {"reads_used": args.reads, "read_ceiling": args.read_ceiling,
                       "discoveries_used": args.discoveries, "discovery_ceiling": args.discovery_ceiling,
                       "unused_reserved_TaskCase_read": 1,
                       "reserved_reads": ["TaskCase body1", "planning.py slice1", "execution API slice1"]},
        "bindings": capture(binary),
        "source_binary_equivalence": "Use each check report's source hashes; a checkpoint hash alone does not assert current source was tested.",
        "sqlite_refinement": {"old_rusqlite": "0.32.1", "old_libsqlite3_sys": "0.30.1", "old_sqlite": "3.46.0", "new_rusqlite": "0.34.0 exact", "new_libsqlite3_sys": "0.32.0", "new_and_oracle_sqlite": "3.49.1", "version_assertion": "parity/sqlite-versions-02.json", "all_current_native_tests": "build/sqlite-3491-consumers-01.log", "pre_refinement_source_binary_input_snapshot": "checkpoints/config-component-01"},
        "sql_bytes_proof": {"scalar": "parity/sql-values-03/report.json", "scalar_requests": 67, "scalar_differences": 0, "byte_ingress": "parity/byte-json-02/report.json", "byte_requests": 250, "byte_differences": 0, "native_tests": "build/sql-response-tests-02.log", "comparison_regression": "build/comparison-tests-01.log", "retained_failure": "byte-json-driver-repro-01: native test-wrapper serialization exit101 after247/250 responses plus driver recursive-comparison traceback; original/raw data retained", "repair": "Iterative exact comparison preserves strict types, bits, keys and lengths; parsed max-depth values use lossless CLI JSON string test transport; direct native library test proves2997 acceptance/clone/equality/drop and2998 rejection", "remaining": "per-call SQLite URI/filesystem codecs and persistent initialization/recovery remain pending"},
        "policy_proof": {"pure": "parity/policy-02/report.json", "pure_requests": 2320, "pure_differences": 0, "all_decimal_codepoints": 680, "loader": "parity/policy-loader-01/report.json", "loader_requests": 2467, "loader_differences": 0, "native_tests": "build/policy-native-tests-01.log", "scope": "actual Windows ART files, schema1/2/3, field/error order, raw policy bytes/digests/size/parsed snapshot and native file identities; complete adversarial filesystem/error/lifecycle proof remains pending", "retained_failure": "parity/policy-01:53 malformed-policy differences; fixed in fresh02", "compiler_repair": "policy-loader-compile-01 E0502 borrowed slice clone changed to owned to_vec; subsequent native tests/build pass", "frozen_inputs_changed": False},
        "git_status": subprocess.check_output(["git", "status", "--short"], cwd=package.parent.parent, text=True),
        "component_evidence": ["parity/foundation-01", "parity/foundation-02", "parity/foundation-03", "parity/foundation-04", "parity/foundation-05", "parity/foundation-06", "parity/foundation-07", "parity/foundation-08", "parity/foundation-09", "parity/planning-01", "parity/planning-02", "parity/planning-03", "parity/planning-04", "parity/registry-01", "parity/frozen-inputs-01", "build/foundation-tests-03.log", "build/state-transactions-01.log", "build/planning-originals-02.log", "build/planner-compile-01.log"],
        "planning_proof": {"registry": "parity/registry-02/report.json", "original_identities_passed": 57, "identity_scope": "4 original merge-readiness cases and all53 original PlannerUnit cases", "latest_differential": "parity/planning-06/report.json", "requests": 33773, "responses": 33773, "native_exit": 0, "open_differences": 0, "repair_history": "planning-04's29 path-root/empty differences retained; fresh expanded planning-05 and planning-06 pass; planning-originals-04 retains E0599 and planning-originals-05/06 pass after utf8 method repair", "affected_tests": "build/planning-originals-06.log", "explicit_binary_build": "build/planning-build-06.log"},
        "deep_value_repair": {"original_failure": "foundation-06: native stack abort0xC00000FD,182 consequential missing responses from one crash", "intermediate_failure": "foundation-07: crash resolved,4 decoder-limit differences", "observed_boundary": "json-limit-probe-01.json: CPython3.12.10 JSON accepts2997 nested containers and rejects2998 at this adapter; sys recursion setting remains1000", "latest": "foundation-09:13169 requests/responses, native exit0, zero differences", "native_lifecycle": "foundation-tests-03.log: parse/clone/equality/serialization/drop through2997 and rejection/error cleanup passed"},
        "transaction_proof": {"command": "cargo test --locked --manifest-path experiments/graph-engine-rust/Cargo.toml --test state_transactions", "exit": 0, "tests": 5, "scope": "actual SQLite transaction/hook/metadata/rollback/replay/state matrix on targeted fixtures; not replacement for original full GraphCase workload", "raw_log": "build/state-transactions-01.log"},
        "coverage": {"modules": 17, "definitions": 383, "cli_leaves": 39, "actual_mutations": 25,
                     "original_behavior_identities": 359, "original_hygiene": 1,
                     "original_behavior_identities_proven": 57},
        "source_count_erratum": "Frozen design says24 mutations; actual state.py259–285 has25. Preserve all25; immutable v2 unchanged.",
        "remaining_scope": [
            "Finish private config path/case-identity/lease/root/error edge contracts and task/result validators; all53 original planning equivalents pass and full policy-loader component differential passes.",
            "Finish helper register/accounting; full usage/checkpoint/report; durable initialization/open/replay/mutation/fault/cross-engine proof.",
            "Implement exact graphctl/helper-register CLIs and parser errors, every graph route/fanout/delegated review/recovery/semantic/timing path.",
            "Implement full evidence/check/source capture/generated-output/review binding/PR61 status scope/acceptance/completion paths.",
            "Map every original behavior identity and383 definitions to exact native assertions/fixtures/negative proof; current component counts are not workload parity.",
            "Finish full parity runner/registered verify facade/benchmark methodology and provisional complete-engine release checks.",
            "Independent simplification/reviews/testing/security/operations and final exclusive measurements remain root-coordinated later work."
        ],
        "known_remaining_compatibility_proof": ["Foundation nesting lifecycle is covered through observed2997-container acceptance and2998 rejection; complete private-API ingress profiles remain pending.", "Complete malformed/private API matrices, full path/glob edge semantics, reparse/locking/initialization/fault proofs.", "Unix source branches will be implemented but remain runtime-unverified on this Windows host."],
        "delivery_repairs_used": 0, "design_revisions_used_reported_by_root": 1,
        "authorization_limits": "No nested agents, supervisor graphctl/ledger/session-log/roster access, commits/pushes/PRs/merges/profile writes/publication/cleanup.",
    }
    output = root / "implementation-checkpoint.json"
    output.write_text(json.dumps(report, indent=2)+"\n", encoding="utf-8")
    print(json.dumps({"checkpoint": str(output), "status": report["status"], "source_files": len(report["bindings"]["source_files_sha256"]), "reads": args.reads, "discoveries": args.discoveries}))


if __name__ == "__main__":
    main()
