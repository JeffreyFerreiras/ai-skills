"""SQLite persistence, atomic initialization, and idempotent mutations."""

import errno
import getpass
import json
import os
import platform
import sqlite3
import stat
import tempfile
from contextlib import contextmanager, ExitStack
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Callable, Dict, Iterable, Iterator, List, Mapping, Optional, Sequence, Tuple

from . import ENGINE_VERSION, STATE_SCHEMA_VERSION
from .contracts import ContractError, ensure_safe_components
from .ids import canonical_bytes, repository_digest, sha256_bytes
from . import workflow_authority as workflow


UNSUPPORTED_POSIX_FILESYSTEMS = {
    "9p", "afs", "ceph", "cifs", "davfs", "gcsfuse", "glusterfs",
    "nfs", "nfs4", "smb3", "sshfs",
}

SemanticValidator = Callable[[sqlite3.Connection, sqlite3.Row], None]


SCHEMA = """
CREATE TABLE runs (
  run_id TEXT PRIMARY KEY, repository_id TEXT NOT NULL, repository_digest TEXT NOT NULL,
  repository_path TEXT NOT NULL, repository_device INTEGER NOT NULL, repository_inode INTEGER NOT NULL,
  status TEXT NOT NULL, state_revision INTEGER NOT NULL, engine_version TEXT NOT NULL,
  state_schema_version INTEGER NOT NULL, policy_digest TEXT NOT NULL, policy_json TEXT NOT NULL,
  task_digest TEXT NOT NULL, task_ref TEXT NOT NULL, task_path TEXT NOT NULL, task_json TEXT NOT NULL,
  request_mode TEXT NOT NULL, minimum_route TEXT NOT NULL, selected_route TEXT,
  selected_tags_json TEXT, design_generation INTEGER NOT NULL DEFAULT 0,
  implementation_generation INTEGER NOT NULL DEFAULT 0, durability TEXT NOT NULL,
  durability_detail TEXT NOT NULL, permission_verification TEXT NOT NULL,
  degraded_permissions_ack INTEGER NOT NULL, degraded_durability_ack INTEGER NOT NULL,
  journal_mode TEXT NOT NULL, synchronous TEXT NOT NULL, sqlite_version TEXT NOT NULL,
  local_filesystem TEXT NOT NULL, host_identity TEXT NOT NULL, database_device INTEGER,
  database_inode INTEGER, blocked_reason TEXT, authoritative INTEGER NOT NULL
  ,started_at TEXT NOT NULL, finished_at TEXT
);
CREATE TABLE execution_plans (
  run_id TEXT PRIMARY KEY REFERENCES runs(run_id), size TEXT NOT NULL,
  plan_json TEXT NOT NULL, plan_digest TEXT NOT NULL, status TEXT NOT NULL,
  authority_ref TEXT, approved_at TEXT, approved_by TEXT, approval_digest TEXT
);
CREATE TABLE nodes (
  branch_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(run_id), node_instance_id TEXT NOT NULL UNIQUE,
  node_key TEXT NOT NULL, role TEXT NOT NULL, stage TEXT NOT NULL, generation INTEGER NOT NULL,
  mandatory INTEGER NOT NULL, specialist_tag TEXT, status TEXT NOT NULL, retry_count INTEGER NOT NULL,
  max_retries INTEGER NOT NULL, envelope_json TEXT NOT NULL, result_json TEXT, result_digest TEXT,
  failure_code TEXT, reason_code TEXT, started_at TEXT, finished_at TEXT,
  parent_branch_id TEXT REFERENCES nodes(branch_id), request_slot_id TEXT,
  assignment_id TEXT, ordinal INTEGER, depth INTEGER NOT NULL DEFAULT 0,
  UNIQUE(run_id,node_key,stage,generation,specialist_tag)
);
CREATE TABLE joins (
  join_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(run_id), join_key TEXT NOT NULL,
  kind TEXT NOT NULL, stage TEXT NOT NULL, generation INTEGER NOT NULL, status TEXT NOT NULL,
  degraded INTEGER NOT NULL DEFAULT 0, result_json TEXT,
  UNIQUE(run_id,join_key,generation)
);
CREATE TABLE join_members (
  join_id TEXT NOT NULL REFERENCES joins(join_id), branch_id TEXT NOT NULL REFERENCES nodes(branch_id),
  mandatory INTEGER NOT NULL, PRIMARY KEY(join_id,branch_id)
);
CREATE TABLE budgets (
  run_id TEXT NOT NULL REFERENCES runs(run_id), budget_id TEXT NOT NULL, limit_value INTEGER NOT NULL,
  used INTEGER NOT NULL, PRIMARY KEY(run_id,budget_id)
);
CREATE TABLE budget_consumptions (
  run_id TEXT NOT NULL REFERENCES runs(run_id), budget_id TEXT NOT NULL,
  source_branch_id TEXT NOT NULL REFERENCES nodes(branch_id), amount INTEGER NOT NULL,
  PRIMARY KEY(run_id,budget_id,source_branch_id)
);
CREATE TABLE operations (
  run_id TEXT NOT NULL REFERENCES runs(run_id), operation_id TEXT NOT NULL, request_digest TEXT NOT NULL,
  response_json TEXT NOT NULL, resulting_revision INTEGER NOT NULL,
  PRIMARY KEY(run_id,operation_id)
);
CREATE TABLE approvals (
  run_id TEXT NOT NULL REFERENCES runs(run_id), approval_id TEXT NOT NULL, scope_ref TEXT NOT NULL,
  decision TEXT NOT NULL, authority_ref TEXT NOT NULL, artifact_sha256 TEXT NOT NULL,
  PRIMARY KEY(run_id,approval_id)
);
CREATE TABLE approval_attestations (
  run_id TEXT NOT NULL REFERENCES runs(run_id), approval_id TEXT NOT NULL,
  actor TEXT NOT NULL, host_identity TEXT NOT NULL, approved_at TEXT NOT NULL,
  approval_digest TEXT NOT NULL, PRIMARY KEY(run_id,approval_id),
  FOREIGN KEY(run_id,approval_id) REFERENCES approvals(run_id,approval_id)
);
CREATE TABLE acceptance_evidence (
  run_id TEXT NOT NULL REFERENCES runs(run_id), criterion_id TEXT NOT NULL, artifact_ref TEXT NOT NULL,
  artifact_sha256 TEXT NOT NULL, PRIMARY KEY(run_id,criterion_id)
);
CREATE TABLE check_evidence (
  run_id TEXT NOT NULL REFERENCES runs(run_id), check_id TEXT NOT NULL, outcome TEXT NOT NULL,
  artifact_ref TEXT NOT NULL, artifact_sha256 TEXT NOT NULL, PRIMARY KEY(run_id,check_id)
);
CREATE TABLE events (
  event_id INTEGER PRIMARY KEY AUTOINCREMENT, run_id TEXT NOT NULL REFERENCES runs(run_id),
  revision INTEGER NOT NULL, event_type TEXT NOT NULL, source_id TEXT, detail_json TEXT NOT NULL
);
CREATE TABLE artifacts (
  ref TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(run_id), kind TEXT NOT NULL,
  sha256 TEXT NOT NULL, size_bytes INTEGER NOT NULL, source_type TEXT NOT NULL,
  source_path TEXT, content_json TEXT, device INTEGER, inode INTEGER, immutable INTEGER NOT NULL,
  UNIQUE(run_id,kind,sha256,ref)
);
CREATE TABLE fanouts (
  fanout_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
  stage TEXT NOT NULL, generation INTEGER NOT NULL CHECK(generation>=0),
  status TEXT NOT NULL CHECK(status IN ('awaiting','assessed')),
  member_branch_ids_json TEXT NOT NULL,
  assessment_ref TEXT REFERENCES artifacts(ref) ON DELETE RESTRICT,
  assessment_digest TEXT, authority_ref TEXT, actor TEXT, host_identity TEXT, assessed_at TEXT,
  UNIQUE(run_id,stage,generation),
  CHECK(
    (status='awaiting' AND assessment_ref IS NULL AND assessment_digest IS NULL AND authority_ref IS NULL
      AND actor IS NULL AND host_identity IS NULL AND assessed_at IS NULL)
    OR
    (status='assessed' AND assessment_ref IS NOT NULL AND assessment_digest IS NOT NULL
      AND authority_ref IS NOT NULL AND actor IS NOT NULL AND host_identity IS NOT NULL AND assessed_at IS NOT NULL)
  )
);
CREATE TABLE fanout_dependencies (
  fanout_id TEXT NOT NULL REFERENCES fanouts(fanout_id) ON DELETE RESTRICT,
  before_branch_id TEXT NOT NULL REFERENCES nodes(branch_id) ON DELETE RESTRICT,
  after_branch_id TEXT NOT NULL REFERENCES nodes(branch_id) ON DELETE RESTRICT,
  reason TEXT NOT NULL,
  PRIMARY KEY(fanout_id,before_branch_id,after_branch_id),
  CHECK(before_branch_id<>after_branch_id)
);
CREATE TABLE branch_attempts (
  run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
  branch_id TEXT NOT NULL REFERENCES nodes(branch_id) ON DELETE RESTRICT,
  attempt_number INTEGER NOT NULL CHECK(attempt_number>0), attempt_id TEXT NOT NULL,
  claim_digest TEXT NOT NULL CHECK(length(claim_digest)=64), started_at TEXT NOT NULL,
  finished_at TEXT, outcome TEXT,
  PRIMARY KEY(branch_id,attempt_number), UNIQUE(run_id,attempt_id),
  CHECK((finished_at IS NULL AND outcome IS NULL) OR (finished_at IS NOT NULL AND outcome IS NOT NULL))
);
CREATE TABLE review_delegation_requests (
  request_slot_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
  parent_branch_id TEXT NOT NULL REFERENCES nodes(branch_id) ON DELETE RESTRICT,
  parent_attempt_id TEXT NOT NULL, parent_claim_digest TEXT NOT NULL,
  generation INTEGER NOT NULL, round_number INTEGER NOT NULL,
  request_digest TEXT NOT NULL, preliminary_ref TEXT NOT NULL REFERENCES artifacts(ref),
  preliminary_digest TEXT NOT NULL, request_ref TEXT NOT NULL REFERENCES artifacts(ref),
  authority_ref TEXT NOT NULL, status TEXT NOT NULL,
  child_member_tuples_json TEXT NOT NULL, dispatch_cost INTEGER NOT NULL,
  created_at TEXT NOT NULL, sealed_at TEXT,
  UNIQUE(parent_branch_id,generation,round_number)
);
CREATE TABLE review_delegation_members (
  request_slot_id TEXT NOT NULL REFERENCES review_delegation_requests(request_slot_id) ON DELETE RESTRICT,
  assignment_id TEXT NOT NULL, ordinal INTEGER NOT NULL,
  child_branch_id TEXT NOT NULL UNIQUE REFERENCES nodes(branch_id) ON DELETE RESTRICT,
  PRIMARY KEY(request_slot_id,assignment_id,ordinal)
);
CREATE TABLE review_delegation_fanouts (
  request_slot_id TEXT PRIMARY KEY REFERENCES review_delegation_requests(request_slot_id) ON DELETE RESTRICT,
  status TEXT NOT NULL, assessment_ref TEXT REFERENCES artifacts(ref), assessment_digest TEXT,
  authority_ref TEXT, actor TEXT, host_identity TEXT, assessed_at TEXT
);
CREATE TABLE review_delegation_dependencies (
  request_slot_id TEXT NOT NULL REFERENCES review_delegation_requests(request_slot_id) ON DELETE RESTRICT,
  before_branch_id TEXT NOT NULL REFERENCES nodes(branch_id) ON DELETE RESTRICT,
  after_branch_id TEXT NOT NULL REFERENCES nodes(branch_id) ON DELETE RESTRICT,
  reason TEXT NOT NULL, PRIMARY KEY(request_slot_id,before_branch_id,after_branch_id)
);
CREATE TABLE review_delegation_collections (
  request_slot_id TEXT PRIMARY KEY REFERENCES review_delegation_requests(request_slot_id) ON DELETE RESTRICT,
  status TEXT NOT NULL, collection_ref TEXT REFERENCES artifacts(ref), collection_digest TEXT,
  content_json TEXT, sealed_at TEXT
);
CREATE TRIGGER fanout_transition_guard BEFORE UPDATE ON fanouts
BEGIN
  SELECT CASE
    WHEN OLD.status<>'awaiting' OR NEW.status<>'assessed'
      OR OLD.fanout_id<>NEW.fanout_id OR OLD.run_id<>NEW.run_id OR OLD.stage<>NEW.stage
      OR OLD.generation<>NEW.generation OR OLD.member_branch_ids_json<>NEW.member_branch_ids_json
    THEN RAISE(ABORT,'FANOUT_IMMUTABLE')
  END;
END;
CREATE TRIGGER fanout_insert_guard BEFORE INSERT ON fanouts WHEN NEW.status<>'awaiting'
BEGIN SELECT RAISE(ABORT,'FANOUT_IMMUTABLE'); END;
CREATE TRIGGER fanout_delete_guard BEFORE DELETE ON fanouts
BEGIN SELECT RAISE(ABORT,'FANOUT_IMMUTABLE'); END;
CREATE TRIGGER fanout_dependency_update_guard BEFORE UPDATE ON fanout_dependencies
BEGIN SELECT RAISE(ABORT,'FANOUT_DEPENDENCY_IMMUTABLE'); END;
CREATE TRIGGER fanout_dependency_insert_guard BEFORE INSERT ON fanout_dependencies
WHEN (SELECT status FROM fanouts WHERE fanout_id=NEW.fanout_id)<>'awaiting'
BEGIN SELECT RAISE(ABORT,'FANOUT_DEPENDENCY_IMMUTABLE'); END;
CREATE TRIGGER fanout_dependency_delete_guard BEFORE DELETE ON fanout_dependencies
BEGIN SELECT RAISE(ABORT,'FANOUT_DEPENDENCY_IMMUTABLE'); END;
CREATE TRIGGER branch_attempt_identity_guard BEFORE UPDATE ON branch_attempts
BEGIN
  SELECT CASE
    WHEN OLD.run_id<>NEW.run_id OR OLD.branch_id<>NEW.branch_id
      OR OLD.attempt_number<>NEW.attempt_number OR OLD.attempt_id<>NEW.attempt_id
      OR OLD.claim_digest<>NEW.claim_digest OR OLD.started_at<>NEW.started_at
      OR OLD.finished_at IS NOT NULL OR (NEW.finished_at IS NULL)<>(NEW.outcome IS NULL)
    THEN RAISE(ABORT,'ATTEMPT_IMMUTABLE')
  END;
END;
CREATE TRIGGER branch_attempt_delete_guard BEFORE DELETE ON branch_attempts
BEGIN SELECT RAISE(ABORT,'ATTEMPT_IMMUTABLE'); END;
CREATE TRIGGER review_request_update_guard BEFORE UPDATE ON review_delegation_requests
BEGIN
  SELECT CASE WHEN
    NOT ((OLD.status='awaiting_assessment' AND NEW.status='running' AND NEW.sealed_at IS NULL)
      OR (OLD.status='running' AND NEW.status='sealed' AND NEW.sealed_at IS NOT NULL))
    OR OLD.request_slot_id<>NEW.request_slot_id OR OLD.run_id<>NEW.run_id
    OR OLD.parent_branch_id<>NEW.parent_branch_id OR OLD.parent_attempt_id<>NEW.parent_attempt_id
    OR OLD.parent_claim_digest<>NEW.parent_claim_digest OR OLD.generation<>NEW.generation
    OR OLD.round_number<>NEW.round_number OR OLD.request_digest<>NEW.request_digest
    OR OLD.preliminary_ref<>NEW.preliminary_ref OR OLD.preliminary_digest<>NEW.preliminary_digest
    OR OLD.request_ref<>NEW.request_ref OR OLD.authority_ref<>NEW.authority_ref
    OR OLD.child_member_tuples_json<>NEW.child_member_tuples_json
    OR OLD.dispatch_cost<>NEW.dispatch_cost OR OLD.created_at<>NEW.created_at
    THEN RAISE(ABORT,'REVIEW_REQUEST_IMMUTABLE') END;
END;
CREATE TRIGGER review_request_delete_guard BEFORE DELETE ON review_delegation_requests
BEGIN SELECT RAISE(ABORT,'REVIEW_REQUEST_IMMUTABLE'); END;
CREATE TRIGGER review_member_update_guard BEFORE UPDATE ON review_delegation_members
BEGIN SELECT RAISE(ABORT,'REVIEW_MEMBER_IMMUTABLE'); END;
CREATE TRIGGER review_member_delete_guard BEFORE DELETE ON review_delegation_members
BEGIN SELECT RAISE(ABORT,'REVIEW_MEMBER_IMMUTABLE'); END;
CREATE TRIGGER review_dependency_update_guard BEFORE UPDATE ON review_delegation_dependencies
BEGIN SELECT RAISE(ABORT,'REVIEW_DEPENDENCY_IMMUTABLE'); END;
CREATE TRIGGER review_dependency_delete_guard BEFORE DELETE ON review_delegation_dependencies
BEGIN SELECT RAISE(ABORT,'REVIEW_DEPENDENCY_IMMUTABLE'); END;
CREATE TRIGGER review_collection_update_guard BEFORE UPDATE ON review_delegation_collections
BEGIN
  SELECT CASE WHEN OLD.status<>'open' OR NEW.status<>'sealed'
    OR OLD.request_slot_id<>NEW.request_slot_id OR NEW.collection_ref IS NULL
    OR NEW.collection_digest IS NULL OR NEW.content_json IS NULL OR NEW.sealed_at IS NULL
    THEN RAISE(ABORT,'REVIEW_COLLECTION_IMMUTABLE') END;
END;
CREATE TRIGGER review_collection_delete_guard BEFORE DELETE ON review_delegation_collections
BEGIN SELECT RAISE(ABORT,'REVIEW_COLLECTION_IMMUTABLE'); END;
CREATE TRIGGER review_fanout_update_guard BEFORE UPDATE ON review_delegation_fanouts
BEGIN
  SELECT CASE WHEN OLD.status<>'awaiting' OR NEW.status<>'assessed'
    OR OLD.request_slot_id<>NEW.request_slot_id OR NEW.assessment_ref IS NULL
    OR NEW.assessment_digest IS NULL OR NEW.authority_ref IS NULL OR NEW.actor IS NULL
    OR NEW.host_identity IS NULL OR NEW.assessed_at IS NULL
    THEN RAISE(ABORT,'REVIEW_FANOUT_IMMUTABLE') END;
END;
CREATE TRIGGER review_fanout_delete_guard BEFORE DELETE ON review_delegation_fanouts
BEGIN SELECT RAISE(ABORT,'REVIEW_FANOUT_IMMUTABLE'); END;
"""

MUTATION_RUN_STATES = {
    "workflow-override.approve": {"initialized", "active", "blocked", "complete", "aborted"},
    "workflow-override.continue": {"initialized", "active", "blocked", "complete", "aborted"},
    "workflow-override.revoke": {"initialized", "active", "blocked", "complete", "aborted"},
    "workflow-override.end": {"initialized", "active", "blocked", "complete", "aborted"},
    "workflow-override.launch": {"initialized", "active", "blocked", "complete", "aborted"},
    "workflow-override.settle": {"initialized", "active", "blocked", "complete", "aborted"},
    "check.launch": {"active"},
    "design-risk.prepare": {"blocked"},
    "design-risk.accept": {"blocked"},
    "evidence.enable": {"active"},
    "check.start": {"active"},
    "check.finish": {"active"},
    "check.abandon": {"active"},
    "record.usage": {"initialized", "active", "blocked", "complete", "aborted"},
    "next.claim": {"initialized", "active"},
    "record.branch-result": {"initialized", "active"},
    "record.timeout": {"initialized", "active"},
    "record.skip": {"active"},
    "record.retry": {"initialized", "active"},
    "record.approval": {"initialized", "active"},
    "record.plan-approval": {"initialized"},
    "record.heartbeat": {"initialized", "active"},
    "record.budget-use": {"initialized", "active"},
    "record.acceptance-evidence": {"active"},
    "record.check-evidence": {"active"},
    "record.fanout-assessment": {"active"},
    "record.review-fanout": {"active"},
    "record.review-fanout-assessment": {"active"},
    "check.run": {"active"},
    "join.advance": {"active"},
    "complete": {"active"},
    "block": {"initialized", "active"},
    "abort": {"initialized", "active", "blocked"},
    "recover": {"blocked"},
}


class StateError(RuntimeError):
    def __init__(self, code: str):
        self.code = code
        super().__init__(code)


def utc_now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def workflow_bindings(run: Mapping[str, Any]) -> Dict[str, Any]:
    task = json.loads(run["task_json"])
    return {key: run[key] for key in ("repository_id", "repository_digest", "task_digest", "run_id", "policy_digest", "host_identity")} | {
        "task_id": task["task_id"], "effects_digest": workflow.hashed(task["authority"])}


def assess_workflow_merge_readiness(connection, run_id, *, task_digest, source_digest,
                                    semantic_validator, capture_source, **snapshot):
    """Assess an exact canonical closure; a caller status/context is never evidence."""
    from .merge_readiness import assess_merge_readiness
    if "graph_status" in snapshot:
        raise StateError("WORKFLOW_COMPLETION_UNVERIFIED")
    run = connection.execute("SELECT * FROM runs WHERE run_id=?", (run_id,)).fetchone()
    if run is None or run["task_digest"] != task_digest:
        raise StateError("WORKFLOW_COMPLETION_BINDING_MISMATCH")
    semantic_validator(connection, run)
    context = workflow_context(connection, run)
    endings = [item for item in context.history if item["kind"] == workflow.END
               and item.get("effective_status") == "complete_with_waivers"]
    if len(endings) != 1 or not context.objective_complete or context.inflight:
        raise StateError("WORKFLOW_COMPLETION_UNVERIFIED")
    ending = endings[0]
    closures = [item for item in context.history if item["kind"] == workflow.CONTINUATION
                and item["operation_id"] == ending["operation_id"]
                and item["authority_ref"] == ending["authority_ref"]
                and item["execution"] == {"kind": "closure", "task_digest": task_digest}
                and item["state"] == "settled" and item["outcome"] == "WAIVED"]
    if len(closures) != 1:
        raise StateError("WORKFLOW_COMPLETION_UNVERIFIED")
    current = capture_source(connection, run)
    workflow.validate_source(current)
    if current["sha256"] != source_digest or current != closures[0]["source"]:
        raise StateError("WORKFLOW_COMPLETION_SOURCE_CHANGED")
    result = assess_merge_readiness(graph_status="complete", **snapshot)
    result.update(raw_graph_status=run["status"], effective_graph_status="complete_with_waivers",
                  workflow_end_ref=ending["ref"], workflow_authority_ref=ending["authority_ref"],
                  assessed_run_id=run_id, assessed_task_digest=task_digest, assessed_source_digest=source_digest)
    return result


def workflow_settlement_joins(connection, run, execution, identifiers, facts):
    """Verify the existing planner's one ordinary result join against actual rows."""
    from .planner import NodeSpec, collection_join, join_id, next_join_for_success
    if (execution.get("kind") not in {"branch", "existing_branch"}
            or not isinstance(identifiers, list) or len(identifiers) != 1):
        raise StateError("WORKFLOW_SETTLEMENT_JOIN_INVALID")
    fence = execution["fence"]
    node = connection.execute("SELECT * FROM nodes WHERE run_id=? AND branch_id=?",
        (run["run_id"], fence["branch_id"])).fetchone()
    if (node is None or node["status"] != "succeeded" or node["depth"] != 0
            or node["result_digest"] != facts.get("result_digest")
            or facts.get("result_ref") != "ledger:" + node["branch_id"] + "#sha256=" + node["result_digest"]):
        raise StateError("WORKFLOW_SETTLEMENT_JOIN_INVALID")
    result = json.loads(node["result_json"])
    artifact = connection.execute("SELECT * FROM artifacts WHERE run_id=? AND ref=? AND immutable=1",
        (run["run_id"], facts["result_ref"])).fetchone()
    if (artifact is None or artifact["kind"] != "branch_result" or artifact["content_json"] != node["result_json"]
            or artifact["sha256"] != node["result_digest"] or workflow.hashed(result) != node["result_digest"]
            or result.get("attempt_id") != fence["attempt_id"] or result.get("claim_digest") != fence["claim_digest"]):
        raise StateError("WORKFLOW_SETTLEMENT_JOIN_INVALID")
    policy = json.loads(run["policy_json"])
    spec = next_join_for_success(policy, run["selected_route"], json.loads(run["selected_tags_json"] or "[]"), node)
    if node["node_key"] == "senior_engineer" and result.get("decision") == "REDESIGN_REQUIRED":
        spec = collection_join("delivery", node["generation"], [NodeSpec(node["node_key"], node["role"],
            node["stage"], node["generation"], bool(node["mandatory"]), node["specialist_tag"])])
    if spec is None or identifiers != [join_id(run["run_id"], run["policy_digest"], spec)]:
        raise StateError("WORKFLOW_SETTLEMENT_JOIN_INVALID")
    join = connection.execute("SELECT * FROM joins WHERE run_id=? AND join_id=?",
        (run["run_id"], identifiers[0])).fetchone()
    members = connection.execute("SELECT branch_id,mandatory FROM join_members WHERE join_id=? ORDER BY branch_id",
        (identifiers[0],)).fetchall()
    if (join is None or (join["join_key"], join["kind"], join["stage"], join["generation"])
            != (spec.key, spec.kind, spec.stage, spec.generation)
            or [tuple(row) for row in members] != [(node["branch_id"], 1)]):
        raise StateError("WORKFLOW_SETTLEMENT_JOIN_INVALID")
    return [{"join_id": identifiers[0], "join_key": spec.key, "kind": spec.kind, "stage": spec.stage,
             "generation": spec.generation, "member_branch_ids": [node["branch_id"]],
             "source_result_ref": facts["result_ref"], "source_result_digest": facts["result_digest"]}]


def workflow_context(connection: sqlite3.Connection, run: Mapping[str, Any], authority_ref=None):
    records = []
    provenance_inputs = {}
    for artifact in connection.execute("SELECT ref,sha256,content_json FROM artifacts WHERE run_id=? AND source_type='ledger'", (run["run_id"],)):
        value = json.loads(artifact["content_json"])
        if isinstance(value, dict) and value.get("kind") in workflow.KINDS:
            records.append({"ref": artifact["ref"], "sha256": artifact["sha256"], "value": value})
        elif isinstance(value, dict) and value.get("kind") == "workflow_provenance_input":
            if (set(value) != {"kind", "schema_version", "operation_id", "sha256", "raw_text"}
                    or value["schema_version"] != 1 or not isinstance(value["raw_text"], str)
                    or value["operation_id"] in provenance_inputs
                    or sha256_bytes(value["raw_text"].encode("utf-8")) != value["sha256"]):
                raise StateError("WORKFLOW_HUMAN_PROVENANCE_INVALID")
            provenance_inputs[value["operation_id"]] = value
    operations = {}
    for row in connection.execute("SELECT * FROM operations WHERE run_id=?", (run["run_id"],)):
        events = connection.execute("SELECT event_type,detail_json FROM events WHERE run_id=? AND source_id=? AND revision=?",
            (run["run_id"], row["operation_id"], row["resulting_revision"])).fetchall()
        if len(events) != 1:
            raise StateError("OPERATION_LEDGER_INVALID")
        operations[row["operation_id"]] = {"revision": row["resulting_revision"], "request_digest": row["request_digest"],
            "command": events[0]["event_type"], "request": json.loads(events[0]["detail_json"]),
            "response": json.loads(row["response_json"])}
    for record in records:
        authority = record["value"]
        if authority["kind"] != workflow.AUTHORITY:
            continue
        witness = provenance_inputs.get(authority.get("operation_id"))
        operation = operations.get(authority.get("operation_id"))
        if (witness is None or operation is None
                or witness["sha256"] != operation["request"].get("provenance_digest")
                or json.loads(witness["raw_text"]) != authority.get("provenance")):
            raise StateError("WORKFLOW_HUMAN_PROVENANCE_INVALID")
    try:
        return workflow.reconstruct_workflow_history(records, operations, workflow_bindings(run), utc_now(), authority_ref,
            objective_complete=run["status"] == "complete")
    except workflow.WorkflowAuthorityError as error:
        raise StateError(str(error)) from error


def utc_after(seconds: int) -> str:
    return (datetime.now(timezone.utc) + timedelta(seconds=seconds)).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def current_actor() -> str:
    return getpass.getuser() or "unknown-user"


RUNTIME_ROOT_ENV = "SOFTWARE_ENGINEERING_GRAPH_HOME"
STATE_ROOT_ENV = "SOFTWARE_ENGINEERING_GRAPH_STATE_HOME"
XDG_STATE_ROOT_ENV = "XDG_STATE_HOME"
STATE_ROOT_DIRECTORY = "software-engineering-graph"


def _absolute_state_root(value: Any) -> Path:
    if not isinstance(value, (str, os.PathLike)) or not os.fspath(value):
        raise StateError("STATE_ROOT_INVALID")
    candidate = Path(value)
    if not candidate.is_absolute():
        raise StateError("STATE_ROOT_MUST_BE_ABSOLUTE")
    return candidate.absolute()


def resolve_state_root(
    explicit: Optional[Path] = None, environ: Optional[Mapping[str, str]] = None,
    skill_root: Optional[Path] = None,
) -> Path:
    """Resolve one home for policy, ledger, lessons, and run artifacts."""
    environment = os.environ if environ is None else environ
    if explicit is not None:
        return _absolute_state_root(explicit)
    if RUNTIME_ROOT_ENV in environment:
        return _absolute_state_root(environment[RUNTIME_ROOT_ENV])
    if STATE_ROOT_ENV in environment:
        return _absolute_state_root(environment[STATE_ROOT_ENV])
    if XDG_STATE_ROOT_ENV in environment:
        return _absolute_state_root(environment[XDG_STATE_ROOT_ENV]) / STATE_ROOT_DIRECTORY
    installed = (skill_root or Path(__file__).resolve().parents[1]).resolve()
    profile = installed.parent.parent if installed.parent.name == "skills" else installed.parent
    return profile.with_name(profile.name + ".local") / STATE_ROOT_DIRECTORY


def installed_codex_home() -> Path:
    """Return the legacy installed profile root for explicit compatibility callers."""
    resolved = Path(__file__).resolve()
    for parent in resolved.parents:
        if parent.name == ".codex":
            return parent
    raise StateError("CODEX_PROFILE_ROOT_NOT_FOUND")


def current_host_identity() -> str:
    value = "\0".join((platform.system(), platform.node(), platform.machine(), getpass.getuser()))
    return sha256_bytes(b"graph-host-v1\0" + value.encode("utf-8"))


def _process_alive(process_id: int) -> bool:
    if process_id <= 0:
        return False
    if process_id == os.getpid():
        return True
    if os.name == "nt":
        import ctypes
        process = ctypes.windll.kernel32.OpenProcess(0x1000, False, process_id)
        if not process:
            return False
        ctypes.windll.kernel32.CloseHandle(process)
        return True
    try:
        os.kill(process_id, 0)
    except OSError as error:
        return error.errno == errno.EPERM
    return True


def repository_identity(repo: Path) -> Tuple[int, int, str]:
    resolved = repo.resolve(strict=True)
    ensure_safe_components(resolved)
    info = resolved.stat()
    if not stat.S_ISDIR(info.st_mode):
        raise ContractError("repo", "NOT_DIRECTORY")
    local_filesystem_identity(resolved)
    return int(info.st_dev), int(info.st_ino), str(resolved)


def _linux_filesystem_type(path: Path) -> Optional[str]:
    mountinfo = Path("/proc/self/mountinfo")
    if not mountinfo.is_file():
        return None
    resolved = str(path.resolve(strict=True))
    best_mount = ""
    best_type: Optional[str] = None
    try:
        lines = mountinfo.read_text(encoding="utf-8").splitlines()
    except OSError:
        return None
    for line in lines:
        before, separator, after = line.partition(" - ")
        if not separator:
            continue
        fields = before.split()
        trailing = after.split()
        if len(fields) < 5 or not trailing:
            continue
        mount_point = fields[4].replace("\\040", " ").replace("\\134", "\\")
        try:
            Path(resolved).relative_to(Path(mount_point))
        except ValueError:
            continue
        if len(mount_point) >= len(best_mount):
            best_mount = mount_point
            best_type = trailing[0]
    return best_type


def local_filesystem_identity(path: Path) -> str:
    resolved = path.resolve(strict=True)
    if os.name == "nt":
        text = str(resolved)
        if text.startswith("\\\\"):
            raise StateError("NON_LOCAL_FILESYSTEM")
        import ctypes
        drive_type = ctypes.windll.kernel32.GetDriveTypeW(resolved.anchor)
        if drive_type != 3:  # DRIVE_FIXED
            raise StateError("NON_LOCAL_FILESYSTEM")
        return "windows_fixed_drive:" + resolved.anchor.rstrip("\\").upper()
    fs_type = _linux_filesystem_type(resolved)
    if fs_type is not None and (fs_type in UNSUPPORTED_POSIX_FILESYSTEMS or fs_type.startswith("fuse")):
        raise StateError("NON_LOCAL_FILESYSTEM")
    return "posix_local:" + (fs_type or "lock_probe") + ":" + str(resolved.stat().st_dev)


class StateStore:
    def __init__(
        self,
        codex_home: Optional[Path] = None,
        fault_hook: Optional[Callable[[str], None]] = None,
        *,
        state_root: Optional[Path] = None,
    ):
        if codex_home is not None and state_root is not None:
            raise StateError("STATE_ROOT_CONFLICT")
        self.state_root = resolve_state_root(state_root if state_root is not None else codex_home)
        self.codex_home = self.state_root
        self.fault_hook = fault_hook or (lambda _point: None)

    def run_root(self, repository_id: str, run_id: str) -> Path:
        return self.codex_home / "graph-runs" / repository_digest(repository_id) / run_id

    def inbox_root(self, repository_id: str, run_id: str) -> Path:
        return self.codex_home / "graph-inbox" / repository_digest(repository_id) / run_id

    def db_path(self, repository_id: str, run_id: str) -> Path:
        return self.run_root(repository_id, run_id) / "state.sqlite3"

    def _mkdir_private(self, path: Path) -> None:
        ensure_safe_components(self.codex_home)
        current = self.codex_home
        current.mkdir(parents=True, exist_ok=True)
        for part in path.relative_to(self.codex_home).parts:
            current = current / part
            if current.exists():
                ensure_safe_components(current, self.codex_home)
            else:
                current.mkdir(mode=0o700)
            if os.name != "nt":
                os.chmod(current, 0o700)
                info = current.stat()
                if info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o700:
                    raise StateError("PERMISSION_VERIFICATION_FAILED")

    def _verify_private_directory(self, path: Path) -> os.stat_result:
        ensure_safe_components(path, self.codex_home)
        try:
            info = path.lstat()
        except OSError:
            raise StateError("STATE_DIRECTORY_INVALID")
        if not stat.S_ISDIR(info.st_mode) or path.is_symlink():
            raise StateError("STATE_DIRECTORY_INVALID")
        if os.name != "nt" and (info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o700):
            raise StateError("PERMISSION_VERIFICATION_FAILED")
        return info

    def verify_state_paths(
        self, repository_id: str, run_id: str, expected_locality: Optional[str] = None
    ) -> str:
        ensure_safe_components(self.codex_home)
        locality = local_filesystem_identity(self.codex_home)
        if expected_locality is not None and locality != expected_locality:
            raise StateError("FILESYSTEM_IDENTITY_CHANGED")
        run_info = self._verify_private_directory(self.run_root(repository_id, run_id))
        inbox_path = self.inbox_root(repository_id, run_id)
        inbox_info = self._verify_private_directory(inbox_path)
        if run_info.st_dev != inbox_info.st_dev:
            raise StateError("STATE_DIRECTORY_INVALID")
        for entry in inbox_path.iterdir():
            ensure_safe_components(entry, inbox_path)
            info = entry.lstat()
            if not stat.S_ISREG(info.st_mode) or entry.is_symlink():
                raise StateError("INBOX_ENTRY_INVALID")
            if os.name != "nt" and (info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) & 0o077):
                raise StateError("PERMISSION_VERIFICATION_FAILED")
        return locality

    def verify_inbox_manifest(self, repository_id: str, run_id: str, path: Path) -> None:
        self.verify_state_paths(repository_id, run_id)
        inbox = self.inbox_root(repository_id, run_id).absolute()
        candidate = path.absolute()
        if candidate.parent != inbox:
            raise ContractError("path", "OUTSIDE_FIXED_INBOX")
        ensure_safe_components(candidate, inbox)
        try:
            info = candidate.lstat()
        except OSError:
            raise ContractError("path", "NOT_REGULAR_FILE")
        if not stat.S_ISREG(info.st_mode) or candidate.is_symlink():
            raise ContractError("path", "NOT_REGULAR_FILE")
        if os.name != "nt" and (info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) & 0o077):
            raise StateError("PERMISSION_VERIFICATION_FAILED")

    def _write_incomplete_marker(
        self, marker: Path, payload: Mapping[str, Any], acknowledged: bool
    ) -> str:
        data = canonical_bytes(payload)
        descriptor = os.open(
            str(marker), os.O_WRONLY | os.O_CREAT | os.O_TRUNC | getattr(os, "O_BINARY", 0), 0o600
        )
        try:
            offset = 0
            while offset < len(data):
                offset += os.write(descriptor, data[offset:])
            os.fsync(descriptor)
        finally:
            os.close(descriptor)
        if os.name != "nt":
            os.chmod(marker, 0o600)
        return self._directory_sync(marker.parent, acknowledged)

    def _incomplete_database_request_digest(self, path: Path, run_id: str) -> str:
        try:
            with self.connect(path) as connection:
                if connection.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
                    raise StateError("INCOMPLETE_INIT_CORRUPT")
                runs = connection.execute(
                    "SELECT run_id,authoritative FROM runs"
                ).fetchall()
                operations = connection.execute(
                    "SELECT run_id,operation_id,request_digest FROM operations"
                ).fetchall()
                if (len(runs) != 1 or runs[0]["run_id"] != run_id or runs[0]["authoritative"] != 0
                        or len(operations) != 1 or operations[0]["run_id"] != run_id
                        or len(operations[0]["request_digest"]) != 64):
                    raise StateError("INCOMPLETE_INIT_CORRUPT")
                return operations[0]["request_digest"]
        except sqlite3.DatabaseError:
            raise StateError("INCOMPLETE_INIT_CORRUPT")

    def _probe_sqlite_locking(self, path: Path) -> None:
        self.fault_hook("locking_probe")
        first = sqlite3.connect(str(path), timeout=0, isolation_level=None)
        second = sqlite3.connect(str(path), timeout=0, isolation_level=None)
        try:
            first.execute("BEGIN IMMEDIATE")
            try:
                second.execute("BEGIN IMMEDIATE")
            except sqlite3.OperationalError as error:
                if "locked" not in str(error).lower():
                    raise StateError("SQLITE_LOCK_PROBE_FAILED")
            else:
                second.rollback()
                raise StateError("SQLITE_LOCK_PROBE_FAILED")
            first.rollback()
            second.execute("BEGIN IMMEDIATE")
            second.rollback()
        except sqlite3.DatabaseError:
            raise StateError("SQLITE_LOCK_PROBE_FAILED")
        finally:
            first.close()
            second.close()

    def _directory_sync(self, directory: Path, acknowledged: bool) -> str:
        self.fault_hook("directory_sync")
        try:
            descriptor = os.open(str(directory), os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
        except OSError as error:
            if os.name == "nt" and error.errno in {errno.EACCES, errno.EINVAL, errno.ENOTSUP, errno.EPERM}:
                if not acknowledged:
                    raise StateError("DEGRADED_DURABILITY_ACK_REQUIRED")
                return "directory_fsync_unavailable_windows"
            raise StateError("DIRECTORY_SYNC_FAILED")
        try:
            os.fsync(descriptor)
        except OSError as error:
            if error.errno in {errno.EINVAL, errno.ENOTSUP}:
                if not acknowledged:
                    raise StateError("DEGRADED_DURABILITY_ACK_REQUIRED")
                return "directory_fsync_unsupported"
            raise StateError("DIRECTORY_SYNC_FAILED")
        finally:
            os.close(descriptor)
        return "verified"

    def initialize(
        self,
        repo: Path,
        policy: Mapping[str, Any],
        policy_digest: str,
        task: Mapping[str, Any],
        task_path: Path,
        task_digest: str,
        run_id: str,
        op_id: str,
        request_digest: str,
        ack_permissions: bool,
        ack_durability: bool,
        bootstrap_row: Mapping[str, Any],
        execution_plan: Mapping[str, Any],
        task_ref: str,
        initial_artifacts: Sequence[Mapping[str, Any]],
    ) -> Dict[str, Any]:
        run_root = self.run_root(policy["repository_id"], run_id)
        inbox = self.inbox_root(policy["repository_id"], run_id)
        self._mkdir_private(run_root)
        self._mkdir_private(inbox)
        filesystem_identity = self.verify_state_paths(policy["repository_id"], run_id)
        permission_state = "verified"
        if os.name == "nt":
            permission_state = "DEGRADED_PERMISSION_VERIFICATION"
            if not ack_permissions:
                raise StateError("DEGRADED_PERMISSION_ACK_REQUIRED")
        final_path = run_root / "state.sqlite3"
        incomplete_markers = sorted(run_root.glob("*.incomplete.sqlite3.marker"))
        marker_digests: List[str] = []
        marker_corrupt = False
        for marker in incomplete_markers:
            try:
                incomplete = json.loads(marker.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                marker_corrupt = True
                continue
            marker_digest = incomplete.get("request_digest")
            if not isinstance(marker_digest, str) or len(marker_digest) != 64:
                marker_corrupt = True
                continue
            marker_digests.append(marker_digest)
        if final_path.exists():
            try:
                with self.open_run(policy["repository_id"], run_id) as connection:
                    operation = connection.execute(
                        "SELECT request_digest,response_json,resulting_revision FROM operations WHERE run_id=? AND operation_id=?",
                        (run_id, op_id),
                    ).fetchone()
                    if operation and operation["request_digest"] == request_digest:
                        result = json.loads(operation["response_json"])
                        result["code"] = "REPLAYED"
                        return result
                    if operation:
                        raise StateError("OPERATION_CONFLICT")
                raise StateError("RUN_ALREADY_EXISTS")
            except StateError as error:
                if error.code != "NON_AUTHORITATIVE_STATE":
                    raise
                database_request_digest = self._incomplete_database_request_digest(final_path, run_id)
                if database_request_digest != request_digest:
                    raise StateError("INCOMPLETE_INIT_CONFLICT")
                if marker_digests and any(item != database_request_digest for item in marker_digests):
                    raise StateError("INCOMPLETE_INIT_CORRUPT")
                failed_path = run_root / (".state.failed." + next(tempfile._get_candidate_names()) + ".sqlite3")
                os.replace(str(final_path), str(failed_path))
        elif marker_corrupt:
            raise StateError("INCOMPLETE_INIT_CORRUPT")
        elif marker_digests and any(item != request_digest for item in marker_digests):
            raise StateError("INCOMPLETE_INIT_CONFLICT")
        device, inode, repo_path = repository_identity(repo)
        lock_path = run_root / ".init.lock"
        lock_record = {"schema_version": 1, "host_identity": current_host_identity(), "process_id": os.getpid(), "request_digest": request_digest}
        for attempt in range(2):
            try:
                lock_fd = os.open(str(lock_path), os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
                os.write(lock_fd, canonical_bytes(lock_record))
                os.fsync(lock_fd)
                os.close(lock_fd)
                break
            except FileExistsError:
                if attempt:
                    raise StateError("INITIALIZATION_CONFLICT")
                try:
                    existing_lock = json.loads(lock_path.read_text(encoding="utf-8"))
                except (OSError, json.JSONDecodeError):
                    raise StateError("INITIALIZATION_CONFLICT")
                if (existing_lock.get("host_identity") != current_host_identity() or
                        _process_alive(existing_lock.get("process_id", -1))):
                    raise StateError("INITIALIZATION_CONFLICT")
                try:
                    lock_path.unlink()
                except OSError:
                    raise StateError("INITIALIZATION_CONFLICT")
        temporary = run_root / (".state." + next(tempfile._get_candidate_names()) + ".incomplete.sqlite3")
        incomplete_marker = temporary.with_suffix(temporary.suffix + ".marker")
        connection: Optional[sqlite3.Connection] = None
        try:
            marker_sync = self._write_incomplete_marker(incomplete_marker, {
                "schema_version": 1, "request_digest": request_digest,
                "operation_id": op_id, "failure_code": "INITIALIZING",
            }, ack_durability)
            connection = sqlite3.connect(str(temporary), isolation_level=None)
            connection.execute("PRAGMA journal_mode=DELETE")
            connection.execute("PRAGMA synchronous=FULL")
            connection.execute("PRAGMA foreign_keys=ON")
            connection.executescript(SCHEMA)
            connection.execute("BEGIN IMMEDIATE")
            task_json = json.dumps(task, sort_keys=True, separators=(",", ":"))
            policy_json = json.dumps(policy, sort_keys=True, separators=(",", ":"))
            row = (
                run_id, policy["repository_id"], repository_digest(policy["repository_id"]), repo_path,
                device, inode, "initialized", 1, ENGINE_VERSION, STATE_SCHEMA_VERSION, policy_digest,
                policy_json, task_digest, task_ref, str(task_path), task_json, task["request_mode"],
                task["minimum_route"], "degraded" if ack_durability and os.name == "nt" else "verified",
                "pending_final_verification", permission_state, int(ack_permissions), int(ack_durability),
                "delete", "FULL", sqlite3.sqlite_version, "pending", current_host_identity(), None, None, 0,
            )
            connection.execute(
                """INSERT INTO runs(
                run_id,repository_id,repository_digest,repository_path,repository_device,repository_inode,
                status,state_revision,engine_version,state_schema_version,policy_digest,policy_json,task_digest,
                task_ref,task_path,task_json,request_mode,minimum_route,durability,durability_detail,
                permission_verification,degraded_permissions_ack,degraded_durability_ack,journal_mode,
                synchronous,sqlite_version,local_filesystem,host_identity,database_device,database_inode,
                authoritative,started_at,finished_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)""",
                row + (utc_now(), None),
            )
            connection.execute(
                "INSERT INTO execution_plans(run_id,size,plan_json,plan_digest,status) VALUES(?,?,?,?,?)",
                (
                    run_id, execution_plan["size"],
                    json.dumps(execution_plan, sort_keys=True, separators=(",", ":")),
                    execution_plan["plan_digest"], "pending",
                ),
            )
            for artifact in initial_artifacts:
                connection.execute(
                    "INSERT INTO artifacts VALUES(?,?,?,?,?,?,?,?,?,?,?)",
                    (
                        artifact["ref"], run_id, artifact["kind"], artifact["sha256"], artifact["size_bytes"],
                        artifact["source_type"], artifact.get("source_path"), artifact.get("content_json"),
                        artifact.get("device"), artifact.get("inode"), 1,
                    ),
                )
            self._insert_node(connection, run_id, bootstrap_row)
            for budget_id, limit_value in (
                ("design_revisions", policy["limits"]["design_revisions"]),
                ("delivery_repairs", policy["limits"]["delivery_repairs"]),
                ("file_reads", task["inspection_budget"]["file_reads"]),
                ("discovery_commands", task["inspection_budget"]["discovery_commands"]),
            ):
                connection.execute("INSERT INTO budgets VALUES(?,?,?,0)", (run_id, budget_id, limit_value))
            response = {
                "schema_version": 1, "ok": True, "code": "INITIALIZED", "run_id": run_id,
                "state_revision": 1, "status": "initialized", "branch": json.loads(bootstrap_row["envelope_json"]),
                "execution_plan": dict(execution_plan), "execution_plan_digest": execution_plan["plan_digest"],
                "execution_plan_status": "pending", "approval_required": True,
                "inbox": str(inbox), "permission_verification": permission_state,
            }
            connection.execute(
                "INSERT INTO operations VALUES(?,?,?,?,?)",
                (run_id, op_id, request_digest, json.dumps(response, sort_keys=True), 1),
            )
            connection.execute(
                "INSERT INTO events(run_id,revision,event_type,source_id,detail_json) VALUES(?,?,?,?,?)",
                (run_id, 1, "init", op_id, "{}"),
            )
            connection.commit()
            connection.close()
            connection = None
            self._probe_sqlite_locking(temporary)
            if os.name != "nt":
                os.chmod(temporary, 0o600)
                file_info = temporary.stat()
                if file_info.st_uid != os.geteuid() or stat.S_IMODE(file_info.st_mode) != 0o600:
                    raise StateError("PERMISSION_VERIFICATION_FAILED")
            self.fault_hook("database_file_flush")
            fd = os.open(str(temporary), os.O_RDWR | getattr(os, "O_BINARY", 0))
            try:
                os.fsync(fd)
            finally:
                os.close(fd)
            first_sync = self._directory_sync(run_root, ack_durability)
            self.fault_hook("replace")
            os.replace(str(temporary), str(final_path))
            self.fault_hook("post_replace")
            second_sync = self._directory_sync(run_root, ack_durability)
            self.fault_hook("reopen")
            with self.connect(final_path) as verified:
                self.fault_hook("identity_verification")
                if not final_path.is_file():
                    raise StateError("DATABASE_IDENTITY_FAILED")
                database_info = final_path.stat()
                self.fault_hook("integrity_check")
                if verified.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
                    raise StateError("INTEGRITY_CHECK_FAILED")
                journal = verified.execute("PRAGMA journal_mode=WAL").fetchone()[0].lower()
                if journal not in {"wal", "delete"}:
                    raise StateError("UNSUPPORTED_JOURNAL_MODE")
                verified.execute("PRAGMA synchronous=FULL")
                sync = verified.execute("PRAGMA synchronous").fetchone()[0]
                if sync != 2:
                    raise StateError("SYNCHRONOUS_NOT_FULL")
                degraded_detail = next(
                    (item for item in (marker_sync, first_sync, second_sync) if item != "verified"),
                    "verified",
                )
                durability = "degraded" if degraded_detail != "verified" else "verified"
                verified.execute("BEGIN IMMEDIATE")
                verified.execute(
                    "UPDATE runs SET journal_mode=?,synchronous='FULL',durability=?,durability_detail=?,local_filesystem=?,database_device=?,database_inode=?,authoritative=1 WHERE run_id=?",
                    (journal, durability, degraded_detail, filesystem_identity, int(database_info.st_dev), int(database_info.st_ino), run_id),
                )
                response["durability"] = durability
                response["durability_detail"] = degraded_detail
                verified.execute(
                    "UPDATE operations SET response_json=? WHERE run_id=? AND operation_id=?",
                    (json.dumps(response, sort_keys=True), run_id, op_id),
                )
                verified.commit()
            for marker in incomplete_markers:
                try:
                    marker.unlink()
                except OSError:
                    pass
            try:
                incomplete_marker.unlink()
            except OSError:
                pass
            return response
        except Exception as error:
            if connection is not None:
                connection.close()
            try:
                self._write_incomplete_marker(incomplete_marker, {
                    "schema_version": 1,
                    "request_digest": request_digest,
                    "operation_id": op_id,
                    "failure_code": getattr(error, "code", "INIT_FAILURE"),
                }, ack_durability)
            except (OSError, StateError):
                pass
            if final_path.exists():
                failed_path = run_root / (".state.failed." + next(tempfile._get_candidate_names()) + ".sqlite3")
                try:
                    os.replace(str(final_path), str(failed_path))
                except OSError:
                    pass
            raise
        finally:
            try:
                lock_path.unlink()
            except OSError:
                pass

    @contextmanager
    def connect(self, path: Path) -> Iterator[sqlite3.Connection]:
        connection = sqlite3.connect(path.resolve().as_uri() + "?mode=rw", uri=True, timeout=5, isolation_level=None)
        connection.row_factory = sqlite3.Row
        connection.execute("PRAGMA foreign_keys=ON")
        connection.execute("PRAGMA synchronous=FULL")
        try:
            yield connection
        finally:
            connection.close()

    def open_run(self, repository_id: str, run_id: str) -> Iterator[sqlite3.Connection]:
        path = self.db_path(repository_id, run_id)
        locality = self.verify_state_paths(repository_id, run_id)
        ensure_safe_components(path, self.codex_home)
        if not path.is_file() or path.is_symlink():
            raise StateError("RUN_NOT_FOUND")
        info = path.stat()
        if not stat.S_ISREG(info.st_mode) or info.st_size > 64 * 1024 * 1024:
            raise StateError("DATABASE_FILE_INVALID")
        if os.name != "nt" and (info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o600):
            raise StateError("PERMISSION_VERIFICATION_FAILED")
        return self._verified_connection(path, info, locality)

    @contextmanager
    def _verified_connection(
        self, path: Path, info: os.stat_result, locality: str
    ) -> Iterator[sqlite3.Connection]:
        try:
            with self.connect(path) as connection:
                run = connection.execute("SELECT * FROM runs LIMIT 1").fetchone()
                if run is None or run["authoritative"] != 1:
                    raise StateError("NON_AUTHORITATIVE_STATE")
                if (run["database_device"], run["database_inode"]) != (int(info.st_dev), int(info.st_ino)):
                    raise StateError("DATABASE_IDENTITY_FAILED")
                if run["local_filesystem"] != locality:
                    raise StateError("FILESYSTEM_IDENTITY_CHANGED")
                if connection.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
                    raise StateError("INTEGRITY_CHECK_FAILED")
                journal = connection.execute("PRAGMA journal_mode").fetchone()[0].lower()
                synchronous = connection.execute("PRAGMA synchronous").fetchone()[0]
                if journal not in {"wal", "delete"} or journal != run["journal_mode"] or synchronous != 2 or run["synchronous"] != "FULL":
                    raise StateError("DURABILITY_STATE_INVALID")
                if run["durability"] not in {"verified", "degraded"} or (run["durability"] == "degraded" and not run["degraded_durability_ack"]):
                    raise StateError("DURABILITY_STATE_INVALID")
                if run["permission_verification"] == "DEGRADED_PERMISSION_VERIFICATION" and not run["degraded_permissions_ack"]:
                    raise StateError("PERMISSION_VERIFICATION_FAILED")
                yield connection
        except sqlite3.DatabaseError:
            raise StateError("DATABASE_STATE_INVALID")

    def mutate(
        self,
        connection: sqlite3.Connection,
        run_id: str,
        op_id: str,
        request: Mapping[str, Any],
        action: Callable[[sqlite3.Connection, sqlite3.Row, int], Dict[str, Any]],
        *,
        semantic_validator: SemanticValidator,
        input_verifier: Optional[Callable[[], None]] = None,
    ) -> Dict[str, Any]:
        dispatch = workflow.current_context()
        selected_ref = getattr(dispatch, "dispatch_ref", None)
        expected_revision = getattr(dispatch, "expected_revision", None)
        if selected_ref:
            request = {**request, "workflow_authority_ref": selected_ref, "expected_revision": expected_revision}
        request_digest = sha256_bytes(canonical_bytes(request))
        contexts = ExitStack()
        try:
            connection.execute("BEGIN IMMEDIATE")
        except sqlite3.OperationalError:
            raise StateError("TRANSACTION_CONFLICT")
        try:
            if input_verifier is not None:
                input_verifier()
            previous = connection.execute(
                "SELECT * FROM operations WHERE run_id=? AND operation_id=?", (run_id, op_id)
            ).fetchone()
            if previous:
                if previous["request_digest"] != request_digest:
                    raise StateError("OPERATION_CONFLICT")
                if selected_ref or request["command"].startswith("workflow-override."):
                    replay_run = connection.execute("SELECT * FROM runs WHERE run_id=?", (run_id,)).fetchone()
                    replay_context = workflow_context(connection, replay_run, selected_ref)
                    with workflow.using_context(replay_context):
                        semantic_validator(connection, replay_run)
                if request["command"] in {"design-risk.prepare", "design-risk.accept"}:
                    replay_run = connection.execute("SELECT * FROM runs WHERE run_id=?", (run_id,)).fetchone()
                    semantic_validator(connection, replay_run)
                result = json.loads(previous["response_json"])
                result["code"] = "REPLAYED"
                if selected_ref or request["command"].startswith("workflow-override."):
                    result["workflow_authority"] = replay_context.report()
                if request["command"] == "design-risk.prepare":
                    result["stale"] = replay_run["state_revision"] != previous["resulting_revision"]
                connection.rollback()
                return {key: value for key, value in result.items() if not key.startswith("_private_")}
            run = connection.execute("SELECT * FROM runs WHERE run_id=?", (run_id,)).fetchone()
            if run is None:
                raise StateError("RUN_NOT_FOUND")
            command = request["command"]
            context = workflow_context(connection, run, selected_ref)
            contexts.enter_context(workflow.using_context(context))
            context.capture_source = getattr(dispatch, "capture_source", None)
            context.normalized_request = request
            if selected_ref:
                if type(expected_revision) is not int or run["state_revision"] != expected_revision:
                    raise StateError("WORKFLOW_REVISION_STALE")
                continuation_id = request.get("workflow_reservation_id")
                phase = "launched" if command in {"workflow-override.launch", "check.launch"} else "settled"
                if continuation_id is None and command in {"record.branch-result", "record.timeout"}:
                    continuation_id = next((key for key, item in context.inflight.items()
                        if item["execution"].get("fence") == {"branch_id": request.get("branch_id"),
                            "attempt_id": request.get("attempt_id"), "claim_digest": request.get("claim_digest")}), None)
                if continuation_id is None:
                    phase = "reserved"
                elif command == "workflow-override.settle" and request.get("late_facts"):
                    phase = "facts"
                try:
                    context.begin(op_id, command, continuation_id=continuation_id, phase=phase)
                except workflow.WorkflowAuthorityError as error:
                    raise StateError(str(error)) from error
            terminal_before = {key: run[key] for key in ("status", "blocked_reason", "finished_at")} if run["status"] in {"blocked", "complete", "aborted"} else None
            budgets_before = {row["budget_id"]: row["used"] for row in connection.execute("SELECT * FROM budgets WHERE run_id=?", (run_id,))}
            protected_nodes = {row["branch_id"]: dict(row) for row in connection.execute(
                "SELECT * FROM nodes WHERE run_id=? AND status IN ('succeeded','failed','timed_out','skipped')", (run_id,))} if selected_ref else {}
            protected_joins = {row["join_id"]: dict(row) for row in connection.execute(
                "SELECT * FROM joins WHERE run_id=? AND status='sealed'", (run_id,))} if selected_ref else {}
            joins_before = {row[0] for row in connection.execute("SELECT join_id FROM joins WHERE run_id=?", (run_id,))} if selected_ref else set()
            status = run["status"]
            allowed_states = MUTATION_RUN_STATES.get(command)
            if allowed_states is None:
                raise StateError("UNKNOWN_MUTATION")
            if not workflow.gate(status in allowed_states, "RUN_ADMISSIBILITY", run_id, status, command):
                if command == "design-risk.accept":
                    semantic_validator(connection, run)
                    for prior in connection.execute("SELECT response_json FROM operations WHERE run_id=?", (run_id,)):
                        response = json.loads(prior[0])
                        if response.get("code") == "DESIGN_RISK_ACCEPTED" and response.get("design_join_id") == request["design_join_id"]:
                            raise StateError("DESIGN_RISK_ALREADY_ACCEPTED")
                if status in {"complete", "aborted"}:
                    raise StateError("TERMINAL_RUN")
                if status == "blocked":
                    raise StateError("GRAPH_BLOCKED")
                raise StateError("INVALID_RUN_TRANSITION")
            semantic_validator(connection, run)
            source_before = None
            if selected_ref:
                if context.capture_source is None:
                    raise StateError("SOURCE_UNVERIFIABLE")
                source_before = context.capture_source(connection, run)
                workflow.validate_source(source_before)
            revision = int(run["state_revision"]) + 1
            result = action(connection, run, revision)
            if selected_ref and context.continuation_id and context.pending_record is None:
                prior = context.actions[context.continuation_id]
                fence = prior["execution"].get("fence")
                node = connection.execute("SELECT * FROM nodes WHERE run_id=? AND branch_id=?",
                    (run_id, fence["branch_id"])).fetchone() if fence else None
                if node is not None and node["status"] in {"succeeded", "failed", "timed_out"}:
                    context.pending_record = {"state": "settled", "outcome": node["status"],
                        "facts": {"result_ref": "ledger:" + node["branch_id"] + "#sha256=" + node["result_digest"],
                                  "result_digest": node["result_digest"], "attempt_id": fence["attempt_id"],
                                  "claim_digest": fence["claim_digest"]}}
            if selected_ref and context.continuation_id and result.get("created_join_ids"):
                if (command != "record.branch-result" or context.phase != "settled" or context.pending_record is None
                        or any(identifier in joins_before for identifier in result["created_join_ids"])):
                    raise StateError("WORKFLOW_SETTLEMENT_JOIN_INVALID")
                prior = context.actions[context.continuation_id]
                facts = context.pending_record.get("facts", {})
                proofs = workflow_settlement_joins(connection, run, prior["execution"], result["created_join_ids"], facts)
                context.pending_record["facts"] = {**facts, "settlement_joins": proofs}
            result.update({"schema_version": 1, "ok": True, "run_id": run_id, "state_revision": revision})
            self.fault_hook("before_commit")
            connection.execute("UPDATE runs SET state_revision=? WHERE run_id=?", (revision, run_id))
            connection.execute(
                "INSERT INTO operations VALUES(?,?,?,?,?)",
                (run_id, op_id, request_digest, json.dumps(result, sort_keys=True), revision),
            )
            connection.execute(
                "INSERT INTO events(run_id,revision,event_type,source_id,detail_json) VALUES(?,?,?,?,?)",
                (run_id, revision, request["command"], op_id, json.dumps(request, sort_keys=True)),
            )
            updated_run = connection.execute("SELECT * FROM runs WHERE run_id=?", (run_id,)).fetchone()
            if selected_ref and terminal_before and any(updated_run[key] != value for key, value in terminal_before.items()):
                raise StateError("WORKFLOW_RAW_TERMINAL_CHANGED")
            for identifier, original in protected_nodes.items():
                current_node = connection.execute("SELECT * FROM nodes WHERE branch_id=?", (identifier,)).fetchone()
                if current_node is None or dict(current_node) != original:
                    raise StateError("WORKFLOW_RAW_HISTORY_CHANGED")
            for identifier, original in protected_joins.items():
                current_join = connection.execute("SELECT * FROM joins WHERE join_id=?", (identifier,)).fetchone()
                if current_join is None or dict(current_join) != original:
                    raise StateError("WORKFLOW_RAW_HISTORY_CHANGED")
            semantic_validator(connection, updated_run)
            if selected_ref and command == "next.claim" and context.decisions:
                claimed = result.get("branch", {})
                node = connection.execute("SELECT * FROM nodes WHERE run_id=? AND branch_id=?",
                    (run_id, claimed.get("branch_id"))).fetchone()
                if node is None:
                    raise StateError("ATTEMPT_FENCE_MISMATCH")
                env = json.loads(node["envelope_json"])
                context.pending_record = {"state": "launched", "outcome": None,
                    "execution": {"kind": "existing_branch", "target": workflow.envelope_identity(env),
                        "envelope": env, "inputs_digest": workflow.hashed(env["inputs"]),
                        "fence": {"branch_id": node["branch_id"], "attempt_id": env["attempt_id"],
                                  "claim_digest": env["claim_digest"]}},
                    "extension_use": {"branch_attempts": 1, node["role"]: 1}}
            if selected_ref and (context.decisions or context.continuation_id or context.pending_record):
                from .evidence import canonical_ledger_artifact, enforce_artifact_size, persist_artifact
                pending = context.pending_record or {}
                prior_action = context.actions.get(context.continuation_id) if context.continuation_id else None
                execution = prior_action["execution"] if prior_action else pending.get("execution", {
                    "kind": "bookkeeping", "command": command, "task_digest": run["task_digest"]})
                deltas = {row["budget_id"]: row["used"] - budgets_before[row["budget_id"]]
                    for row in connection.execute("SELECT * FROM budgets WHERE run_id=?", (run_id,))}
                if any(amount < 0 for amount in deltas.values()):
                    raise StateError("WORKFLOW_CONSUMPTION_REFUND")
                value = {"kind": workflow.CONTINUATION, "schema_version": 1,
                    "bindings": workflow_bindings(run), "authority_ref": selected_ref,
                    "operation_id": op_id, "request_digest": request_digest, "revision": revision,
                    "created_at": utc_now(), "phase": context.phase,
                    "reservation_id": context.continuation_id or op_id,
                    "ordinal": prior_action["ordinal"] if prior_action else context.used + 1,
                    "state": pending.get("state", "settled"), "action": command,
                    "decisions": context.decisions, "source": source_before,
                    "execution": execution, "execution_digest": workflow.hashed(execution),
                    "predecessor_ref": prior_action["latest_ref"] if prior_action else None,
                    "created_branch_ids": result.get("successor_branch_ids", []),
                    "created_join_ids": result.get("created_join_ids", []),
                    "consumption_deltas": deltas, "extension_use": pending.get("extension_use", {}),
                    "outcome": pending.get("outcome"), "facts": pending.get("facts", {})}
                artifact = canonical_ledger_artifact("workflow-continuation-" + op_id, "evidence_manifest", value)
                enforce_artifact_size(artifact.kind, artifact.size_bytes, json.loads(run["policy_json"]))
                persist_artifact(connection, run_id, artifact)
                result["workflow_continuation_ref"] = artifact.ref
                result["workflow_reservation_id"] = value["reservation_id"]
                if command == "next.claim":
                    result["workflow_dispatch"] = {"authority_ref": selected_ref, "reservation_id": value["reservation_id"]}
                result["workflow_authority"] = workflow_context(connection, updated_run, selected_ref).report()
                connection.execute("UPDATE operations SET response_json=? WHERE run_id=? AND operation_id=?", (json.dumps(result, sort_keys=True), run_id, op_id))
                with workflow.using_context(workflow_context(connection, updated_run, selected_ref)):
                    semantic_validator(connection, updated_run)
            connection.commit()
            return {key: value for key, value in result.items() if not key.startswith("_private_")}
        except Exception:
            connection.rollback()
            raise
        finally:
            contexts.close()

    @staticmethod
    def _insert_node(connection: sqlite3.Connection, run_id: str, row: Mapping[str, Any]) -> None:
        connection.execute(
            """INSERT INTO nodes(branch_id,run_id,node_instance_id,node_key,role,stage,generation,
            mandatory,specialist_tag,status,retry_count,max_retries,envelope_json,parent_branch_id,
            request_slot_id,assignment_id,ordinal,depth)
            VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)""",
            (
                row["branch_id"], run_id, row["node_instance_id"], row["node_key"], row["role"],
                row["stage"], row["generation"], int(row["mandatory"]), row.get("specialist_tag"),
                row["status"], row["retry_count"], row["max_retries"], row["envelope_json"],
                row.get("parent_branch_id"), row.get("request_slot_id"), row.get("assignment_id"),
                row.get("ordinal"), row.get("depth", 0),
            ),
        )

    @staticmethod
    def insert_join(connection: sqlite3.Connection, run_id: str, row: Mapping[str, Any], members: Sequence[Mapping[str, Any]]) -> None:
        connection.execute(
            "INSERT INTO joins(join_id,run_id,join_key,kind,stage,generation,status) VALUES(?,?,?,?,?,?,?)",
            (row["join_id"], run_id, row["join_key"], row["kind"], row["stage"], row["generation"], "open"),
        )
        for member in members:
            connection.execute(
                "INSERT INTO join_members VALUES(?,?,?)",
                (row["join_id"], member["branch_id"], int(member["mandatory"])),
            )

    @staticmethod
    def row_dict(row: sqlite3.Row) -> Dict[str, Any]:
        return {key: row[key] for key in row.keys()}
