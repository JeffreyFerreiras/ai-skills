"""Local, provider-neutral execution and verification of required checks."""

import subprocess
import os
import json
import stat
import threading
import time
from datetime import datetime
from pathlib import Path
from typing import Any, Dict, Mapping, Sequence

from .contracts import ContractError, bounded_string, digest, opaque, require_keys, safe_file_snapshot, lexical_relative, validate_ref
from .config import _repository_target
from .evidence import resolve_reference
from .ids import canonical_bytes, sha256_bytes
from .state import StateError, current_actor, current_host_identity, utc_now


def repository_worktree_digest(repo: Path) -> str:
    """Return a compact digest of the Git state used by a local check."""
    values: Dict[str, Any] = {"git": True}
    for name, arguments in (
        ("head", ["rev-parse", "HEAD"]),
        ("status", ["status", "--porcelain=v1"]),
        ("diff", ["diff", "--binary", "HEAD", "--"]),
    ):
        try:
            result = subprocess.run(
                ["git"] + arguments,
                cwd=str(repo),
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
                timeout=10,
            )
        except (OSError, subprocess.TimeoutExpired):
            values["git"] = False
            values[name] = None
            continue
        if result.returncode != 0:
            values["git"] = False
            values[name] = None
        elif name == "diff":
            values[name] = sha256_bytes(result.stdout)
        else:
            values[name] = result.stdout.decode("utf-8", errors="replace")
    return sha256_bytes(canonical_bytes(values))


def configured_check(policy: Mapping[str, Any], check_id: str) -> Mapping[str, Any]:
    check = policy.get("required_checks", {}).get(check_id)
    if not isinstance(check, dict):
        raise StateError("UNKNOWN_CHECK")
    if not isinstance(check.get("argv"), list) or not check["argv"]:
        raise StateError("CHECK_COMMAND_NOT_CONFIGURED")
    return check


def run_check(
    repo: Path,
    run_id: str,
    check_id: str,
    command_id: str,
    argv: Sequence[str],
    timeout_seconds: int,
    *,
    legacy_digest: bool = True,
) -> Dict[str, Any]:
    started_at = utc_now()
    try:
        completed = subprocess.run(
            list(argv),
            cwd=str(repo),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            timeout=timeout_seconds,
        )
        exit_code = int(completed.returncode)
        timed_out = False
        stdout = completed.stdout
        stderr = completed.stderr
    except subprocess.TimeoutExpired as error:
        exit_code = 124
        timed_out = True
        stdout = error.stdout or b""
        stderr = error.stderr or b""
    except OSError as error:
        exit_code = 127
        timed_out = False
        stdout = b""
        stderr = str(error).encode("utf-8", errors="replace")
    finished_at = utc_now()
    return {
        "schema_version": 1,
        "kind": "check_receipt",
        "run_id": opaque(run_id, "run_id"),
        "check_id": opaque(check_id, "check_id"),
        "command_id": opaque(command_id, "command_id"),
        "argv": [bounded_string(item, "argv", 1024) for item in argv],
        "outcome": "PASS" if exit_code == 0 else "FAIL",
        "exit_code": exit_code,
        "timed_out": timed_out,
        "stdout_sha256": sha256_bytes(stdout),
        "stderr_sha256": sha256_bytes(stderr),
        "repo_worktree_sha256": repository_worktree_digest(repo) if legacy_digest else "0" * 64,
        "started_at": started_at,
        "finished_at": finished_at,
        "producer_actor": current_actor(),
        "producer_host_identity": current_host_identity(),
    }


def validate_check_receipt(
    receipt: Any,
    run_id: str,
    check_id: str,
    expected: Mapping[str, Any],
    repo: Path,
    *,
    check_freshness: bool = True,
) -> Dict[str, Any]:
    if not isinstance(receipt, dict):
        raise ContractError("check_receipt", "INVALID_OBJECT")
    allowed = {
        "schema_version", "kind", "run_id", "check_id", "command_id", "argv", "outcome",
        "exit_code", "timed_out", "stdout_sha256", "stderr_sha256", "repo_worktree_sha256",
        "started_at", "finished_at", "producer_actor", "producer_host_identity",
    }
    require_keys(receipt, allowed, allowed, "check_receipt")
    if receipt["schema_version"] != 1 or receipt["kind"] != "check_receipt":
        raise ContractError("check_receipt", "SCHEMA_MISMATCH")
    if receipt["run_id"] != run_id or receipt["check_id"] != check_id:
        raise ContractError("check_receipt", "IDENTITY_MISMATCH")
    if receipt["command_id"] != expected["command_id"]:
        raise ContractError("command_id", "COMMAND_MISMATCH")
    if receipt.get("argv") != expected.get("argv"):
        raise ContractError("argv", "COMMAND_MISMATCH")
    if receipt["outcome"] not in {"PASS", "FAIL"} or not isinstance(receipt["exit_code"], int) or isinstance(receipt["exit_code"], bool):
        raise ContractError("check_receipt", "OUTCOME_INVALID")
    if receipt["outcome"] == "PASS" and receipt["exit_code"] != 0:
        raise ContractError("outcome", "OUTCOME_MISMATCH")
    if receipt["outcome"] == "FAIL" and receipt["exit_code"] == 0:
        raise ContractError("outcome", "OUTCOME_MISMATCH")
    if not isinstance(receipt["timed_out"], bool):
        raise ContractError("timed_out", "INVALID_TYPE")
    if receipt["timed_out"] != (receipt["exit_code"] == 124):
        raise ContractError("timed_out", "OUTCOME_MISMATCH")
    for field in ("stdout_sha256", "stderr_sha256", "repo_worktree_sha256"):
        digest(receipt[field], "check_receipt." + field)
    for field in ("started_at", "finished_at", "producer_actor", "producer_host_identity"):
        bounded_string(receipt[field], "check_receipt." + field, 256)
    try:
        started = datetime.fromisoformat(receipt["started_at"].replace("Z", "+00:00"))
        finished = datetime.fromisoformat(receipt["finished_at"].replace("Z", "+00:00"))
    except (AttributeError, ValueError):
        raise ContractError("check_receipt", "TIMESTAMP_INVALID")
    if finished < started:
        raise ContractError("check_receipt", "TIMESTAMP_INVALID")
    if receipt["producer_actor"] != current_actor() or receipt["producer_host_identity"] != current_host_identity():
        raise ContractError("check_receipt", "PRODUCER_MISMATCH")
    if check_freshness and receipt["repo_worktree_sha256"] != repository_worktree_digest(repo):
        raise ContractError("repo_worktree_sha256", "REPOSITORY_CHANGED")
    return dict(receipt)


def validate_receipt_integrity(receipt, run_id, check_id, expected, repo):
    """Keep historical provenance validation independent from present-source eligibility."""
    if not isinstance(receipt, dict) or receipt.get("schema_version") == 1:
        return validate_check_receipt(receipt, run_id, check_id, expected, repo, check_freshness=False)
    additions = {"executor", "source", "source_generation", "task_digest", "plan_digest", "policy_digest",
                 "reservation_id", "predecessor_ref", "coverage", "coverage_digest", "before_source_digest",
                 "after_source_digest", "validity", "activation_ref", "review_source_binding_ref"}
    base = {key: value for key, value in receipt.items() if key not in additions}
    if receipt.get("schema_version") != 2 or not additions.issubset(receipt):
        raise ContractError("check_receipt", "SCHEMA_MISMATCH")
    base["schema_version"] = 1
    validate_check_receipt(base, run_id, check_id, expected, repo, check_freshness=False)
    for field in ("executor", "source"):
        fence = receipt[field]
        require_keys(fence, {"branch_id", "attempt_id", "claim_digest", "result_digest"},
                     {"branch_id", "attempt_id", "claim_digest", "result_digest"}, field)
        opaque(fence["branch_id"], field)
        opaque(fence["attempt_id"], field)
        digest(fence["claim_digest"], field)
        if fence["result_digest"] is not None:
            digest(fence["result_digest"], field)
    for field in ("task_digest", "plan_digest", "policy_digest", "coverage_digest", "before_source_digest", "after_source_digest"):
        digest(receipt[field], field)
    opaque(receipt["reservation_id"], "reservation_id")
    if type(receipt["source_generation"]) is not int or receipt["source_generation"] < 0:
        raise ContractError("source_generation", "INVALID_VALUE")
    for field in ("activation_ref", "predecessor_ref", "review_source_binding_ref"):
        if receipt[field] is not None:
            validate_ref(receipt[field], field, content_required=True)
    require_keys(receipt["coverage"], {"roots", "outputs", "activation_ref"}, {"roots", "outputs", "activation_ref"}, "coverage")
    if receipt["coverage"]["activation_ref"] != receipt["activation_ref"]:
        raise ContractError("coverage", "ACTIVATION_MISMATCH")
    if receipt["coverage_digest"] != sha256_bytes(canonical_bytes(receipt["coverage"])):
        raise ContractError("coverage", "DIGEST_DISAGREEMENT")
    if receipt["validity"] not in {"valid", "source_changed", "unverifiable"}:
        raise ContractError("validity", "INVALID_VALUE")
    if receipt["validity"] == "valid" and receipt["before_source_digest"] != receipt["after_source_digest"]:
        raise ContractError("validity", "SOURCE_CHANGED")
    return dict(receipt)


SOURCE_MAX_PATHS = 10000
SOURCE_MAX_FILE_BYTES = 16 * 1024 * 1024
SOURCE_MAX_BYTES = 128 * 1024 * 1024
SOURCE_MAX_ENUMERATION_BYTES = 2 * 1024 * 1024
SOURCE_TIMEOUT_SECONDS = 30


def lifecycle_artifacts(connection, run_id, kind):
    values = []
    for row in connection.execute("SELECT * FROM artifacts WHERE run_id=? AND source_type='ledger'", (run_id,)):
        value = json.loads(row["content_json"])
        if isinstance(value, dict) and value.get("kind") == kind:
            values.append((dict(row), value))
    return values


def evidence_activation(connection, run):
    matches = lifecycle_artifacts(connection, run["run_id"], "evidence_activation")
    if len(matches) != 1:
        raise StateError("EVIDENCE_ACTIVATION_INVALID")
    row, value = matches[0]
    plan = connection.execute("SELECT plan_digest FROM execution_plans WHERE run_id=?", (run["run_id"],)).fetchone()
    if (value.get("schema_version") != 1 or value.get("original_format") != 6
            or value.get("task_digest") != run["task_digest"] or value.get("policy_digest") != run["policy_digest"]
            or value.get("plan_digest") != plan[0] or row["kind"] != "evidence_manifest"):
        raise StateError("EVIDENCE_ACTIVATION_INVALID")
    return row, value


def reservation_states(connection, run_id):
    reservations = {value["reservation_id"]: {**value, "artifact_ref": row["ref"], "state": "open"}
                    for row, value in lifecycle_artifacts(connection, run_id, "check_reservation")}
    for event in connection.execute("SELECT event_type,detail_json FROM events WHERE run_id=? ORDER BY revision", (run_id,)):
        if event["event_type"] not in {"check.finish", "check.abandon"}:
            continue
        request = json.loads(event["detail_json"])
        reservation = reservations.get(request.get("reservation_id"))
        if reservation is None or reservation["state"] != "open":
            raise StateError("CHECK_RESERVATION_INVALID")
        reservation["state"] = "finished" if event["event_type"] == "check.finish" else "abandoned"
    return reservations


def verify_check_fence(connection, run, fence, *, executor=False):
    branch = connection.execute("SELECT * FROM nodes WHERE run_id=? AND branch_id=?",
                                (run["run_id"], fence["branch_id"])).fetchone()
    if branch is None:
        raise StateError("ATTEMPT_FENCE_MISMATCH")
    env = json.loads(branch["envelope_json"])
    attempt = connection.execute("SELECT * FROM branch_attempts WHERE run_id=? AND branch_id=? AND attempt_id=?",
                                 (run["run_id"], fence["branch_id"], fence["attempt_id"])).fetchone()
    if (attempt is None or env.get("attempt_id") != fence["attempt_id"]
            or env.get("claim_digest") != fence["claim_digest"]
            or attempt["claim_digest"] != fence["claim_digest"]
            or branch["status"] not in {"running", "succeeded"}):
        raise StateError("ATTEMPT_FENCE_MISMATCH")
    if branch["status"] == "running" and env.get("lease_expires_at", "") <= utc_now():
        raise StateError("ATTEMPT_FENCE_MISMATCH")
    expected_generation = run["implementation_generation"] if branch["stage"] in {"implementation", "delivery"} else run["design_generation"]
    if branch["generation"] != expected_generation:
        raise StateError("ATTEMPT_FENCE_MISMATCH")
    if not executor:
        expected_source = {"full_delivery": "senior_engineer", "fast_path": "senior_engineer",
                           "design_only": "tech_lead", "advisory": "advisory_reviewer"}.get(run["selected_route"])
        if branch["node_key"] != expected_source:
            raise StateError("SOURCE_ATTEMPT_INVALID")
    return branch, env


def current_review_binding(connection, run):
    matches = [(row, value) for row, value in lifecycle_artifacts(connection, run["run_id"], "review_source_binding")
               if value.get("source_generation") == run["implementation_generation"]]
    if len(matches) > 1:
        raise StateError("REVIEW_BINDING_MISMATCH")
    return matches[0] if matches else None


def verify_review_source(connection, run, policy, registering=None, check_freshness=True):
    binding = current_review_binding(connection, run)
    if binding is None:
        raise StateError("REVIEW_BINDING_MISMATCH")
    row, value = binding
    for member in connection.execute("SELECT envelope_json FROM nodes WHERE run_id=? AND stage='delivery' AND generation=? AND depth=0",
                                     (run["run_id"], run["implementation_generation"])):
        if row["ref"] not in {item["ref"] for item in json.loads(member[0])["inputs"]}:
            raise StateError("REVIEW_BINDING_MISMATCH")
    source, _ = verify_check_fence(connection, run, value["source"])
    if source["result_digest"] != value["source"]["result_digest"]:
        raise StateError("REVIEW_BINDING_MISMATCH")
    if not check_freshness:
        return row, value
    verify_generated_outputs(connection, run, policy, value["coverage"]["outputs"], registering)
    snapshot = capture_evidence_source(connection, run, policy, value["coverage"]["roots"], value["coverage"]["outputs"])
    if snapshot["sha256"] != value["source_digest"]:
        raise StateError("REVIEW_SOURCE_CHANGED")
    return row, value


def validate_lifecycle_integrity(connection, run, policy, task):
    activation_row, activation = evidence_activation(connection, run)
    activations = connection.execute("SELECT * FROM events WHERE run_id=? AND event_type='evidence.enable'", (run["run_id"],)).fetchall()
    if len(activations) != 1:
        raise StateError("EVIDENCE_ACTIVATION_INVALID")
    activation_request = json.loads(activations[0]["detail_json"])
    if (activation_request.get("coverage_manifest_digest") != activation.get("coverage_manifest_digest")
            or activation_request.get("checks") != activation.get("checks") or activation_request.get("roots") != activation.get("roots")
            or activation.get("actor") != current_actor() or activation.get("host_identity") != current_host_identity()):
        raise StateError("EVIDENCE_ACTIVATION_INVALID")
    if set(activation["checks"]) != set(task["required_check_ids"]) or activation.get("complete") is not True:
        raise StateError("EVIDENCE_ACTIVATION_INVALID")
    reservations = reservation_states(connection, run["run_id"])
    slots = set()
    pending = set()
    consumed = {"delivery_repairs": 0, "design_revisions": 0}
    selected_receipts = {}
    finished_receipts = set()
    for event in connection.execute("SELECT e.*,o.request_digest,o.response_json FROM events e JOIN operations o ON o.run_id=e.run_id AND o.operation_id=e.source_id WHERE e.run_id=? ORDER BY e.revision", (run["run_id"],)):
        request = json.loads(event["detail_json"])
        response = json.loads(event["response_json"])
        if event["event_type"] in {"evidence.enable", "check.start", "check.finish", "check.abandon"}:
            if sha256_bytes(canonical_bytes(request)) != event["request_digest"]:
                raise StateError("CHECK_RESERVATION_INVALID")
        if event["event_type"] == "check.start":
            reservation = reservations.get(response.get("reservation_id"))
            if reservation is None or reservation["request_digest"] != event["request_digest"]:
                raise StateError("CHECK_RESERVATION_INVALID")
            for key in ("check_id", "source_generation", "predecessor_ref", "coverage", "activation_ref"):
                if reservation[key] != request[key]:
                    raise StateError("CHECK_RESERVATION_INVALID")
            for key in ("executor", "source"):
                if {**reservation[key], "result_digest": None} != request[key]:
                    raise StateError("CHECK_RESERVATION_INVALID")
                historical = connection.execute("SELECT claim_digest FROM branch_attempts WHERE run_id=? AND branch_id=? AND attempt_id=?",
                                                (run["run_id"], reservation[key]["branch_id"], reservation[key]["attempt_id"])).fetchone()
                if historical is None or historical[0] != reservation[key]["claim_digest"]:
                    raise StateError("CHECK_RESERVATION_INVALID")
            slot = (reservation["executor"]["branch_id"], reservation["executor"]["attempt_id"], reservation["check_id"])
            if reservation["repeat_cost"] != int(slot in slots) or reservation["check_id"] in pending:
                raise StateError("CHECK_RESERVATION_INVALID")
            slots.add(slot)
            pending.add(reservation["check_id"])
            consumed[reservation["budget_id"]] += reservation["repeat_cost"]
        elif event["event_type"] in {"check.finish", "check.abandon"}:
            reservation = reservations[request["reservation_id"]]
            pending.remove(reservation["check_id"])
            if event["event_type"] == "check.finish":
                receipt_row = connection.execute("SELECT * FROM artifacts WHERE ref=?", (response["artifact_ref"],)).fetchone()
                if receipt_row is None or receipt_row["sha256"] != request["receipt_digest"]:
                    raise StateError("CHECK_RESERVATION_INVALID")
                receipt = json.loads(receipt_row["content_json"])
                validate_receipt_integrity(receipt, run["run_id"], reservation["check_id"], configured_check(policy, reservation["check_id"]), Path(run["repository_path"]))
                for key in ("executor", "source", "source_generation", "coverage", "predecessor_ref", "before_source_digest", "review_source_binding_ref"):
                    if receipt[key] != reservation[key]:
                        raise StateError("CHECK_RESERVATION_INVALID")
                if (receipt["reservation_id"] != request["reservation_id"] or receipt["task_digest"] != run["task_digest"]
                        or receipt["policy_digest"] != run["policy_digest"] or receipt["plan_digest"] != activation["plan_digest"]
                        or receipt["activation_ref"] != activation_row["ref"]):
                    raise StateError("CHECK_RESERVATION_INVALID")
                selected_receipts[reservation["check_id"]] = response["artifact_ref"]
                finished_receipts.add(response["artifact_ref"])
        elif event["event_type"] == "join.advance":
            outcome = response.get("outcome")
            if outcome == "REPAIR":
                consumed["delivery_repairs"] += 1
            elif outcome in {"REVISE", "REDESIGN"}:
                consumed["design_revisions"] += 1
        elif event["event_type"] == "record.budget-use" and request["budget_id"] in consumed:
            consumed[request["budget_id"]] += request["amount"]
    for budget in connection.execute("SELECT * FROM budgets WHERE run_id=?", (run["run_id"],)):
        if budget["budget_id"] in consumed and budget["used"] != consumed[budget["budget_id"]]:
            raise StateError("BUDGET_STATE_INVALID")
    for check_id, reference in selected_receipts.items():
        selected = connection.execute("SELECT artifact_ref FROM check_evidence WHERE run_id=? AND check_id=?", (run["run_id"], check_id)).fetchone()
        if selected is None or selected[0] != reference:
            raise StateError("CHECK_PREDECESSOR_CONFLICT")
    for row, receipt in lifecycle_artifacts(connection, run["run_id"], "check_receipt"):
        validate_receipt_integrity(receipt, run["run_id"], receipt["check_id"], configured_check(policy, receipt["check_id"]), Path(run["repository_path"]))
        if receipt["schema_version"] == 2 and row["ref"] not in finished_receipts:
            raise StateError("CHECK_RESERVATION_INVALID")
    for row, binding in lifecycle_artifacts(connection, run["run_id"], "review_source_binding"):
        source = connection.execute("SELECT * FROM nodes WHERE run_id=? AND branch_id=?", (run["run_id"], binding["source"]["branch_id"])).fetchone()
        dependency = connection.execute("SELECT * FROM joins WHERE run_id=? AND join_id=? AND kind='dependency' AND stage='delivery' AND status='sealed'",
                                        (run["run_id"], binding["dependency_join_id"])).fetchone()
        if (source is None or source["result_digest"] != binding["source"]["result_digest"]
                or dependency is None or dependency["generation"] != binding["source_generation"]
                or not connection.execute("SELECT 1 FROM join_members WHERE join_id=? AND branch_id=?", (binding["dependency_join_id"], source["branch_id"])).fetchone()
                or json.loads(source["envelope_json"])["attempt_id"] != binding["source"]["attempt_id"]
                or json.loads(source["envelope_json"])["claim_digest"] != binding["source"]["claim_digest"]
                or binding["task_digest"] != run["task_digest"] or binding["policy_digest"] != run["policy_digest"]
                or binding["plan_digest"] != activation["plan_digest"] or binding["coverage"]["roots"] != activation["roots"]
                or source["generation"] != binding["source_generation"] or source["node_key"] != "senior_engineer"
                or binding["coverage_digest"] != sha256_bytes(canonical_bytes(binding["coverage"]))
                or binding["outputs_digest"] != sha256_bytes(canonical_bytes(binding["coverage"]["outputs"]))
                or binding["coverage"]["activation_ref"] != activation_row["ref"]):
            raise StateError("REVIEW_BINDING_MISMATCH")
        requests = [json.loads(event[0]) for event in connection.execute("SELECT detail_json FROM events WHERE run_id=? AND event_type='join.advance'", (run["run_id"],))]
        origin = next((request for request in requests if request.get("join_id") == binding["dependency_join_id"]), None)
        declared = sorted([{**item, "path": lexical_relative(item["path"])}
                           for item in origin.get("generated_outputs", [])], key=lambda item: item["path"]) if origin else None
        if declared != binding["coverage"]["outputs"]:
            raise StateError("REVIEW_BINDING_MISMATCH")
        for member in connection.execute("SELECT envelope_json FROM nodes WHERE run_id=? AND stage='delivery' AND generation=? AND depth=0", (run["run_id"], binding["source_generation"])):
            if row["ref"] not in {item["ref"] for item in json.loads(member[0])["inputs"]}:
                raise StateError("REVIEW_BINDING_MISMATCH")
    validate_repair_and_acceptance_history(connection, run, task, activation)


def validate_repair_and_acceptance_history(connection, run, task, activation):
    """Validate immutable packet provenance without making old source freshness a run gate."""
    for _, packet in lifecycle_artifacts(connection, run["run_id"], "repair_packet"):
        branch = connection.execute("SELECT * FROM nodes WHERE run_id=? AND branch_id=?",
                                    (run["run_id"], packet["branch_id"])).fetchone()
        if (branch is None or packet["task_digest"] != run["task_digest"]
                or packet["plan_digest"] != activation["plan_digest"] or packet["run_id"] != run["run_id"]
                or packet["generation"] != branch["generation"]
                or (packet.get("origin_kind") != "delivery_review" and packet["origin_gate"] != branch["node_key"])
                or not set(packet["failed_criterion_ids"]).issubset(task["acceptance_ids"])):
            raise StateError("REPAIR_PACKET_INVALID")
        if packet.get("origin_kind") == "delivery_review":
            collection = connection.execute("SELECT * FROM joins WHERE run_id=? AND join_id=? AND join_key='delivery_collection' AND status='sealed'",
                                            (run["run_id"], packet["origin_collection_id"])).fetchone()
            consolidation_ref = packet["origin_consolidation_ref"]
            consolidation = connection.execute("SELECT * FROM artifacts WHERE run_id=? AND ref=? AND sha256=?",
                                               (run["run_id"], consolidation_ref["ref"], consolidation_ref["sha256"])).fetchone()
            result = json.loads(consolidation["content_json"]) if consolidation else {}
            if (collection is None or collection["generation"] + 1 != branch["generation"]
                    or branch["node_key"] != "senior_engineer" or packet["origin_gate"] != "supervisor_delivery_consolidation"
                    or result.get("outcome") != "REPAIR" or result.get("generation") != collection["generation"]):
                raise StateError("REPAIR_PACKET_INVALID")
            members = {row[0] for row in connection.execute("SELECT branch_id FROM join_members WHERE join_id=?", (collection["join_id"],))}
            if {item["branch_id"] for item in packet["origin_reports"]} != members:
                raise StateError("REPAIR_PACKET_INVALID")
            for origin in packet["origin_reports"]:
                member = connection.execute("SELECT * FROM nodes WHERE branch_id=?", (origin["branch_id"],)).fetchone()
                env = json.loads(member["envelope_json"])
                if (origin["attempt_id"] != env["attempt_id"] or origin["result_ref"]["sha256"] != member["result_digest"]
                        or origin["artifact_ref"] != env["artifact_ref"] or origin["evidence"] != env["evidence"]
                        or origin["findings"] != json.loads(member["result_json"]).get("findings", [])):
                    raise StateError("REPAIR_PACKET_INVALID")
        allowed = json.loads(branch["envelope_json"])["effect_capabilities"]
        for capability in packet["authorized_scope"]:
            if not any(capability["effect"] == item["effect"] and capability["action"] == item["action"]
                       and source_path_contains(item["target_ref"], capability["target_ref"])
                       for item in allowed if item["effect"].startswith("filesystem")):
                raise StateError("REPAIR_PACKET_INVALID")
        if packet["failure_result_ref"]:
            result = connection.execute("SELECT content_json FROM artifacts WHERE run_id=? AND ref=?",
                                        (run["run_id"], packet["failure_result_ref"])).fetchone()
            failure = json.loads(result[0]) if result else {}
            if (failure.get("branch_id") != branch["branch_id"]
                    or not (failure.get("status") in {"failed", "timed_out"} or failure.get("kind") == "timeout")
                    or failure.get("attempt_id") != packet["attempt_id"]):
                raise StateError("REPAIR_PACKET_INVALID")
        if packet["attempt_id"] and not connection.execute(
                "SELECT 1 FROM branch_attempts WHERE run_id=? AND branch_id=? AND attempt_id=?",
                (run["run_id"], branch["branch_id"], packet["attempt_id"])).fetchone():
            raise StateError("REPAIR_PACKET_INVALID")
    wrappers = {row["ref"]: payload for row, payload in lifecycle_artifacts(connection, run["run_id"], "acceptance_binding")}
    for reference, wrapper in wrappers.items():
        if (wrapper["run_id"] != run["run_id"] or wrapper["task_digest"] != run["task_digest"]
                or wrapper["plan_digest"] != activation["plan_digest"] or wrapper["criterion_id"] not in task["acceptance_ids"]
                or set(wrapper["checks"]) != set(task["required_check_ids"])):
            raise StateError("ACCEPTANCE_BINDING_MISMATCH")
        evidence = wrapper["underlying_evidence"]
        if not connection.execute("SELECT 1 FROM artifacts WHERE run_id=? AND ref=? AND sha256=? AND kind='acceptance_evidence'",
                                  (run["run_id"], evidence["ref"], evidence["sha256"])).fetchone():
            raise StateError("ACCEPTANCE_BINDING_MISMATCH")
        for check_id, receipt_ref in wrapper["checks"].items():
            row = connection.execute("SELECT content_json FROM artifacts WHERE run_id=? AND ref=?", (run["run_id"], receipt_ref)).fetchone()
            receipt = json.loads(row[0]) if row else {}
            if (receipt.get("check_id") != check_id or receipt.get("outcome") != "PASS"
                    or receipt.get("source") != wrapper["source"] or receipt.get("after_source_digest") != wrapper["source_digest"]
                    or receipt.get("coverage_digest") != wrapper["coverage_digest"]
                    or receipt.get("review_source_binding_ref") != wrapper["review_source_binding_ref"]):
                raise StateError("ACCEPTANCE_BINDING_MISMATCH")
        predecessor = wrapper["predecessor_ref"]
        if predecessor in wrappers and wrappers[predecessor]["criterion_id"] != wrapper["criterion_id"]:
            raise StateError("ACCEPTANCE_PREDECESSOR_CONFLICT")


def receipt_eligibility(connection, run, policy, receipt, registering=None):
    if receipt.get("schema_version") != 2:
        return "legacy_unbound"
    try:
        verify_check_fence(connection, run, receipt["source"])
        verify_check_fence(connection, run, receipt["executor"], executor=True)
        if receipt["validity"] != "valid":
            return receipt["validity"]
        verify_generated_outputs(connection, run, policy, receipt["coverage"]["outputs"], registering)
        current = capture_evidence_source(connection, run, policy, receipt["coverage"]["roots"], receipt["coverage"]["outputs"])
        if current["sha256"] != receipt["after_source_digest"]:
            return "source_changed"
        binding = current_review_binding(connection, run)
        if binding and (receipt["review_source_binding_ref"] != binding[0]["ref"]
                        or receipt["coverage_digest"] != binding[1]["coverage_digest"]
                        or current["sha256"] != binding[1]["source_digest"]):
            return "CHECK_REVIEW_BINDING_MISMATCH"
        return "eligible" if receipt["outcome"] == "PASS" else "failed"
    except (StateError, ContractError) as error:
        return str(error)


def prepare_generated_outputs(repo, policy, task, value, producers, input_refs, relevant_inputs):
    if not isinstance(value, dict):
        raise ContractError("generated_outputs", "INVALID_OBJECT")
    require_keys(value, {"schema_version", "kind", "outputs"}, {"schema_version", "kind", "outputs"}, "generated_outputs")
    if value["schema_version"] != 1 or value["kind"] != "generated_output_plan" or not isinstance(value["outputs"], list) or len(value["outputs"]) > 32:
        raise ContractError("generated_outputs", "SCHEMA_MISMATCH")
    writes = [item["target_ref"][5:] for item in task["authority"]["capabilities"]
              if item["effect"] == "filesystem_write" and item["action"] == "edit" and item["target_ref"].startswith("repo:")]
    protected = list(policy.get("implementation_roots", [])) + list(relevant_inputs)
    protected += [item.split("#sha256=", 1)[0][5:] for item in input_refs if item.startswith("repo:")]
    result = []
    seen = set()
    for item in value["outputs"]:
        require_keys(item, {"path", "purpose", "producer_node_key", "artifact_kind"},
                     {"path", "purpose", "producer_node_key", "artifact_kind"}, "generated_output")
        path = _repository_target(repo, "repo:" + item["path"], policy["denied_patterns"])[5:]
        key = os.path.normcase(path)
        expected = producers.get(item["producer_node_key"])
        purpose = "review_report" if item["producer_node_key"] != "supervisor_delivery_consolidation" else item["purpose"]
        if (key in seen or path.endswith("/") or expected is None or item["purpose"] != purpose
                or item["purpose"] not in {"review_report", "consolidation", "acceptance_wrapper"}
                or item["artifact_kind"] != ("acceptance_evidence" if purpose == "acceptance_wrapper" else expected)):
            raise ContractError("generated_output", "PRODUCER_MISMATCH")
        if (not any(source_path_contains(root, path) for root in writes)
                or not any(source_path_contains(root, path) for root in policy["artifact_roots"]["repo"])
                or any(source_path_contains(root, path) or source_path_contains(path, root) for root in protected)
                or Path(path).suffix.lower() not in policy["artifact_kinds"][item["artifact_kind"]]["extensions"]):
            raise StateError("SOURCE_UNVERIFIABLE")
        if (repo / path).exists() or (repo / path).is_symlink():
            raise StateError("SOURCE_UNVERIFIABLE")
        tracked = _bounded_git(repo, ["ls-files", "-z", "--", ":(literal)" + path], time.monotonic() + SOURCE_TIMEOUT_SECONDS)
        if tracked:
            raise StateError("SOURCE_UNVERIFIABLE")
        seen.add(key)
        result.append({**item, "path": path})
    return sorted(result, key=lambda item: item["path"])


def verify_generated_outputs(connection, run, policy, outputs, registering=None):
    repo = Path(run["repository_path"])
    for output in outputs:
        path = output["path"]
        _repository_target(repo, "repo:" + path, policy["denied_patterns"])
        candidate = repo / path
        try:
            metadata = candidate.lstat()
        except FileNotFoundError:
            continue
        if not stat.S_ISREG(metadata.st_mode) or candidate.is_symlink() or metadata.st_size > SOURCE_MAX_FILE_BYTES:
            raise StateError("SOURCE_UNVERIFIABLE")
        reference = None
        if registering and registering[0] == output["producer_node_key"]:
            proposed = registering[1]
            if proposed and proposed["ref"].split("#sha256=", 1)[0] == "repo:" + path and proposed["kind"] == output["artifact_kind"]:
                reference = proposed
        if reference is None:
            for branch in connection.execute("SELECT result_json FROM nodes WHERE run_id=? AND node_key=? AND generation=? AND status='succeeded'",
                                             (run["run_id"], output["producer_node_key"], run["implementation_generation"])):
                proposed = json.loads(branch[0]).get("artifact_ref")
                if proposed and proposed["ref"].split("#sha256=", 1)[0] == "repo:" + path and proposed["kind"] == output["artifact_kind"]:
                    reference = proposed
        if reference is None and output["purpose"] == "acceptance_wrapper":
            for row in connection.execute("SELECT a.content_json FROM acceptance_evidence e JOIN artifacts a ON a.ref=e.artifact_ref WHERE e.run_id=?", (run["run_id"],)):
                wrapper = json.loads(row[0] or "{}")
                proposed = wrapper.get("underlying_evidence")
                if proposed and proposed["ref"].split("#sha256=", 1)[0] == "repo:" + path and proposed["kind"] == output["artifact_kind"]:
                    reference = proposed
        if reference is None:
            raise StateError("GENERATED_OUTPUT_UNREGISTERED")
        resolve_reference(reference["ref"], reference["sha256"], reference["kind"], repo, Path(__file__).resolve().parents[1], policy, connection)


def source_path_contains(root: str, path: str) -> bool:
    """Match normalized exact/subtree targets using the host's case rules."""
    root_key = os.path.normcase(root.rstrip("/")).replace("\\", "/")
    path_key = os.path.normcase(path.rstrip("/")).replace("\\", "/")
    return path_key == root_key or (root.endswith("/") and path_key.startswith(root_key + "/"))


def source_roots(repo: Path, policy: Mapping[str, Any], capabilities: Sequence[Mapping[str, Any]]) -> list:
    return sorted(set(_repository_target(repo, item["target_ref"], policy["denied_patterns"])[5:]
                      for item in capabilities if item["effect"] == "filesystem_read"
                      and item["action"] == "read" and item["target_ref"].startswith("repo:")))


def _bounded_git(repo: Path, arguments: Sequence[str], deadline: float, output_budget=None) -> bytes:
    """Drain bounded output while the process runs, terminating on either ceiling."""
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise StateError("SOURCE_UNVERIFIABLE")
    try:
        process = subprocess.Popen(["git"] + list(arguments), cwd=str(repo),
                                   stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    except OSError:
        raise StateError("SOURCE_UNVERIFIABLE")
    chunks = []
    overflow = threading.Event()
    maximum = output_budget[0] if output_budget is not None else SOURCE_MAX_ENUMERATION_BYTES

    def read_output():
        size = 0
        while True:
            chunk = process.stdout.read(65536)
            if not chunk:
                break
            size += len(chunk)
            if size > maximum:
                overflow.set()
                process.kill()
                break
            chunks.append(chunk)

    reader = threading.Thread(target=read_output, daemon=True)
    reader.start()
    try:
        process.wait(timeout=remaining)
        reader.join(max(0, deadline - time.monotonic()))
        if reader.is_alive() or overflow.is_set() or process.returncode != 0:
            raise StateError("SOURCE_UNVERIFIABLE")
        data = b"".join(chunks)
        if output_budget is not None:
            output_budget[0] -= len(data)
        return data
    except subprocess.TimeoutExpired:
        raise StateError("SOURCE_UNVERIFIABLE")
    finally:
        if process.poll() is None:
            process.kill()
        process.wait()
        reader.join()
        process.stdout.close()


def capture_evidence_source(connection, run, policy, roots, outputs=()):
    _, activation = evidence_activation(connection, run)
    known_inputs = sorted({path for paths in activation["checks"].values() for path in paths})
    return capture_source(Path(run["repository_path"]), policy, roots, outputs, known_inputs)


def capture_source(repo: Path, policy: Mapping[str, Any], roots: Sequence[str], outputs=(), known_inputs=()) -> Dict[str, Any]:
    """Capture only authorized source. Unverifiable coverage never yields a digest."""
    deadline = time.monotonic() + SOURCE_TIMEOUT_SECONDS
    output_budget = [SOURCE_MAX_ENUMERATION_BYTES]
    if not roots:
        raise StateError("SOURCE_UNVERIFIABLE")
    try:
        normalized = sorted(set(_repository_target(repo, "repo:" + root, policy["denied_patterns"])[5:]
                                for root in roots))
        selectors = [":(literal)" + root.rstrip("/") for root in normalized]

        def enumerate_paths():
            index = _bounded_git(repo, ["ls-files", "--stage", "-z", "--"] + selectors, deadline, output_budget)
            untracked = _bounded_git(repo, ["ls-files", "--others", "--exclude-standard", "-z", "--"] + selectors, deadline, output_budget)
            entries = {}
            for item in index.split(b"\0"):
                if not item:
                    continue
                metadata, path_bytes = item.split(b"\t", 1)
                if metadata.startswith(b"160000 "):
                    raise StateError("SOURCE_UNVERIFIABLE")
                path = path_bytes.decode("utf-8", errors="strict")
                entries.setdefault(path, []).append(metadata.decode("ascii"))
            for item in untracked.split(b"\0"):
                if item:
                    entries.setdefault(item.decode("utf-8", errors="strict"), [])
            for root in normalized:
                if not root.endswith("/"):
                    entries.setdefault(root, [])
            # Ignored content is readable only through an explicit exact read target.
            # Declaring an input does not grant that authority.
            if known_inputs:
                ignored = _bounded_git(repo, ["ls-files", "--others", "--ignored", "--exclude-standard", "-z", "--"] + selectors,
                                       deadline, output_budget)
                ignored_paths = [item.decode("utf-8", errors="strict") for item in ignored.split(b"\0") if item]
                if (any(not path.endswith("/") and path not in entries for path in known_inputs)
                        or any(path not in entries and any(source_path_contains(root, path) for root in known_inputs)
                               for path in ignored_paths)):
                    raise StateError("SOURCE_UNVERIFIABLE")
            if len(entries) > SOURCE_MAX_PATHS:
                raise StateError("SOURCE_UNVERIFIABLE")
            return entries

        head = _bounded_git(repo, ["rev-parse", "HEAD"], deadline, output_budget).decode("ascii").strip()
        entries = enumerate_paths()
        excluded = {item["path"] for item in outputs}
        records = []
        total = 0
        for relative, index in sorted(entries.items()):
            if time.monotonic() > deadline or not any(source_path_contains(root, relative) for root in normalized):
                raise StateError("SOURCE_UNVERIFIABLE")
            _repository_target(repo, "repo:" + relative, policy["denied_patterns"])
            path = repo / relative
            try:
                metadata = path.lstat()
            except FileNotFoundError:
                if relative not in excluded:
                    records.append({"path": relative, "index": index, "exists": False})
                continue
            if (not stat.S_ISREG(metadata.st_mode) or path.is_symlink()
                    or metadata.st_size > SOURCE_MAX_FILE_BYTES):
                raise StateError("SOURCE_UNVERIFIABLE")
            if relative in excluded:
                continue
            total += metadata.st_size
            if total > SOURCE_MAX_BYTES:
                raise StateError("SOURCE_UNVERIFIABLE")
            snapshot = safe_file_snapshot(path, [repo], SOURCE_MAX_FILE_BYTES)
            total += snapshot.size - metadata.st_size
            if total > SOURCE_MAX_BYTES:
                raise StateError("SOURCE_UNVERIFIABLE")
            records.append({"path": relative, "index": index, "exists": True,
                            "mode": stat.S_IMODE(metadata.st_mode), "sha256": snapshot.digest})
        if entries != enumerate_paths() or time.monotonic() > deadline:
            raise StateError("SOURCE_UNVERIFIABLE")
        return {"sha256": sha256_bytes(canonical_bytes({"head": head, "paths": records})),
                "roots": normalized, "path_count": len(records)}
    except (OSError, ValueError, UnicodeError, ContractError):
        raise StateError("SOURCE_UNVERIFIABLE")
