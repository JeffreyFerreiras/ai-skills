"""Verified authoritative artifact resolution and persistence."""

import fnmatch
import json
import os
import sqlite3
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Any, Dict, Mapping, Optional, Sequence

from .config import ENGINE_ARTIFACT_MAX
from .contracts import ContractError, digest, lexical_relative, safe_file_snapshot, validate_ref
from .ids import canonical_bytes, sha256_bytes


@dataclass(frozen=True)
class VerifiedArtifact:
    ref: str
    kind: str
    sha256: str
    size_bytes: int
    source_type: str
    source_path: Optional[str]
    content_json: Optional[str]
    device: Optional[int]
    inode: Optional[int]

    def as_input(self) -> Dict[str, Any]:
        return {
            "kind": self.kind,
            "ref": self.ref,
            "sha256": self.sha256,
            "size_bytes": self.size_bytes,
        }


def _within_roots(relative: str, roots: Sequence[str]) -> bool:
    path = PurePosixPath(relative)
    for configured in roots:
        root = PurePosixPath(configured.rstrip("/"))
        try:
            path.relative_to(root)
            return True
        except ValueError:
            continue
    return False


def policy_path_allowed(relative: str, policy: Mapping[str, Any]) -> None:
    lexical_relative(relative, "artifact_ref")
    normalized = relative.replace("\\", "/")
    for pattern in policy["denied_patterns"]:
        if fnmatch.fnmatchcase(normalized.lower(), pattern.lower()) or PurePosixPath(normalized.lower()).match(pattern.lower()):
            raise ContractError("artifact_ref", "SENSITIVE_PATH")


def _kind_policy(kind: str, policy: Mapping[str, Any]) -> Mapping[str, Any]:
    config = policy["artifact_kinds"].get(kind)
    if not isinstance(config, dict):
        raise ContractError("artifact_kind", "UNKNOWN_ARTIFACT_KIND")
    return config


def artifact_size_limit(kind: str, policy: Mapping[str, Any]) -> int:
    kind_policy = _kind_policy(kind, policy)
    engine_maximum = ENGINE_ARTIFACT_MAX.get(kind)
    if engine_maximum is None:
        raise ContractError("artifact_kind", "UNKNOWN_ARTIFACT_KIND")
    return min(
        int(kind_policy["max_bytes"]), int(policy["limits"]["artifact_bytes"]),
        engine_maximum, 4 * 1024 * 1024,
    )


def enforce_artifact_size(kind: str, size_bytes: Any, policy: Mapping[str, Any]) -> None:
    if isinstance(size_bytes, bool) or not isinstance(size_bytes, int) or size_bytes < 0:
        raise ContractError("artifact_ref", "INVALID_SIZE")
    if size_bytes > artifact_size_limit(kind, policy):
        raise ContractError("artifact_ref", "FILE_TOO_LARGE")


def resolve_reference(
    ref: str,
    separate_digest: str,
    kind: str,
    repo: Path,
    skill_root: Path,
    policy: Mapping[str, Any],
    connection: Optional[sqlite3.Connection] = None,
    runtime_root: Optional[Path] = None,
) -> VerifiedArtifact:
    validate_ref(ref, "artifact_ref", content_required=True)
    expected = digest(separate_digest, "artifact_sha256")
    prefix, body = ref.split(":", 1)
    identity, marker, embedded = body.partition("#sha256=")
    if not marker or embedded != expected:
        raise ContractError("artifact_ref", "DIGEST_DISAGREEMENT")
    kind_policy = _kind_policy(kind, policy)
    maximum = artifact_size_limit(kind, policy)
    if prefix == "ledger":
        if connection is None:
            raise ContractError("artifact_ref", "LEDGER_CONTEXT_REQUIRED")
        row = connection.execute(
            "SELECT * FROM artifacts WHERE ref=? AND sha256=? AND immutable=1", (ref, expected)
        ).fetchone()
        if row is None or row["kind"] != kind:
            raise ContractError("artifact_ref", "LEDGER_ARTIFACT_NOT_FOUND")
        enforce_artifact_size(row["kind"], row["size_bytes"], policy)
        return VerifiedArtifact(
            row["ref"], row["kind"], row["sha256"], row["size_bytes"], row["source_type"],
            row["source_path"], row["content_json"], row["device"], row["inode"],
        )
    if prefix == "repo":
        relative = lexical_relative(identity, "artifact_ref")
        policy_path_allowed(relative, policy)
        permitted = (policy["implementation_roots"] if policy["schema_version"] == 3
                     else policy["artifact_roots"]["repo"])
        if not _within_roots(relative, permitted):
            raise ContractError("artifact_ref", "OUTSIDE_ALLOWED_ROOT")
        path = repo / relative
        roots = [repo / root for root in permitted]
    elif prefix == "runtime" and policy["schema_version"] == 3:
        relative = lexical_relative(identity, "artifact_ref")
        policy_path_allowed(relative, policy)
        if not _within_roots(relative, policy["artifact_roots"]["runtime"]):
            raise ContractError("artifact_ref", "OUTSIDE_ALLOWED_ROOT")
        if runtime_root is None:
            from .state import resolve_state_root
            runtime_root = resolve_state_root()
        path = runtime_root / relative
        roots = [runtime_root / root for root in policy["artifact_roots"]["runtime"]]
    elif prefix == "profile" and identity.startswith("software-engineering-graph/"):
        relative = lexical_relative(identity[len("software-engineering-graph/"):], "artifact_ref")
        policy_path_allowed(relative, policy)
        if not _within_roots(relative, policy["artifact_roots"]["profile"]):
            raise ContractError("artifact_ref", "OUTSIDE_ALLOWED_ROOT")
        path = skill_root / relative
        roots = [skill_root / root for root in policy["artifact_roots"]["profile"]]
    else:
        raise ContractError("artifact_ref", "AUTHORITATIVE_REF_REQUIRED")
    suffix = path.suffix.lower()
    if suffix not in set(kind_policy["extensions"]):
        raise ContractError("artifact_ref", "ARTIFACT_TYPE_MISMATCH")
    snapshot = safe_file_snapshot(path, roots, maximum)
    if snapshot.digest != expected:
        raise ContractError("artifact_ref", "INPUT_DIGEST_MISMATCH")
    return VerifiedArtifact(
        ref, kind, expected, snapshot.size, prefix, str(snapshot.path), None,
        snapshot.identity[0], snapshot.identity[1],
    )


def resolve_unhashed_reference(
    ref: str,
    kind: str,
    repo: Path,
    skill_root: Path,
    policy: Mapping[str, Any],
    runtime_root: Optional[Path] = None,
) -> VerifiedArtifact:
    validate_ref(ref, "artifact_ref", content_required=False)
    prefix, body = ref.split(":", 1)
    if "#" in body:
        raise ContractError("artifact_ref", "UNEXPECTED_DIGEST")
    kind_policy = _kind_policy(kind, policy)
    maximum = artifact_size_limit(kind, policy)
    if prefix == "repo":
        relative = lexical_relative(body, "artifact_ref")
        policy_path_allowed(relative, policy)
        permitted = (policy["implementation_roots"] if policy["schema_version"] == 3
                     else policy["artifact_roots"]["repo"])
        if not _within_roots(relative, permitted):
            raise ContractError("artifact_ref", "OUTSIDE_ALLOWED_ROOT")
        path = repo / relative
        roots = [repo / root for root in permitted]
        normalized_prefix = "repo:"
    elif prefix == "runtime" and policy["schema_version"] == 3:
        relative = lexical_relative(body, "artifact_ref")
        policy_path_allowed(relative, policy)
        if not _within_roots(relative, policy["artifact_roots"]["runtime"]):
            raise ContractError("artifact_ref", "OUTSIDE_ALLOWED_ROOT")
        if runtime_root is None:
            from .state import resolve_state_root
            runtime_root = resolve_state_root()
        path = runtime_root / relative
        roots = [runtime_root / root for root in policy["artifact_roots"]["runtime"]]
        normalized_prefix = "runtime:"
    elif prefix == "profile" and body.startswith("software-engineering-graph/"):
        relative = lexical_relative(body[len("software-engineering-graph/"):], "artifact_ref")
        policy_path_allowed(relative, policy)
        if not _within_roots(relative, policy["artifact_roots"]["profile"]):
            raise ContractError("artifact_ref", "OUTSIDE_ALLOWED_ROOT")
        path = skill_root / relative
        roots = [skill_root / root for root in policy["artifact_roots"]["profile"]]
        normalized_prefix = "profile:software-engineering-graph/"
    else:
        raise ContractError("artifact_ref", "AUTHORITATIVE_REF_REQUIRED")
    if path.suffix.lower() not in set(kind_policy["extensions"]):
        raise ContractError("artifact_ref", "ARTIFACT_TYPE_MISMATCH")
    snapshot = safe_file_snapshot(path, roots, maximum)
    normalized_ref = normalized_prefix + relative + "#sha256=" + snapshot.digest
    return VerifiedArtifact(
        normalized_ref, kind, snapshot.digest, snapshot.size, prefix, str(snapshot.path), None,
        snapshot.identity[0], snapshot.identity[1],
    )


def canonical_ledger_artifact(identifier: str, kind: str, content: Mapping[str, Any]) -> VerifiedArtifact:
    serialized = canonical_bytes(content)
    artifact_digest = sha256_bytes(serialized)
    return VerifiedArtifact(
        f"ledger:{identifier}#sha256={artifact_digest}", kind, artifact_digest,
        len(serialized), "ledger", None, serialized.decode("utf-8"), None, None,
    )


def persist_artifact(connection: sqlite3.Connection, run_id: str, artifact: VerifiedArtifact) -> None:
    existing = connection.execute("SELECT * FROM artifacts WHERE ref=?", (artifact.ref,)).fetchone()
    values = (
        artifact.ref, run_id, artifact.kind, artifact.sha256, artifact.size_bytes,
        artifact.source_type, artifact.source_path, artifact.content_json,
        artifact.device, artifact.inode, 1,
    )
    if existing is None:
        connection.execute("INSERT INTO artifacts VALUES(?,?,?,?,?,?,?,?,?,?,?)", values)
        return
    comparison = (
        existing["run_id"], existing["kind"], existing["sha256"], existing["size_bytes"],
        existing["source_type"], existing["source_path"], existing["content_json"],
        existing["device"], existing["inode"], existing["immutable"],
    )
    if comparison != values[1:]:
        raise ContractError("artifact_ref", "ARTIFACT_CONFLICT")


def reverify_artifact(
    connection: sqlite3.Connection,
    artifact: Mapping[str, Any],
    repo: Path,
    skill_root: Path,
    policy: Mapping[str, Any],
    runtime_root: Optional[Path] = None,
) -> None:
    enforce_artifact_size(artifact["kind"], artifact["size_bytes"], policy)
    if artifact["source_type"] == "ledger":
        if artifact["content_json"] is None:
            raise ContractError("artifact_ref", "LEDGER_CONTENT_MISSING")
        try:
            value = json.loads(artifact["content_json"])
        except json.JSONDecodeError:
            raise ContractError("artifact_ref", "LEDGER_CONTENT_INVALID")
        canonical = canonical_bytes(value)
        if sha256_bytes(canonical) != artifact["sha256"] or len(canonical) != artifact["size_bytes"]:
            raise ContractError("artifact_ref", "LEDGER_DIGEST_MISMATCH")
        return
    verified = resolve_reference(
        artifact["ref"], artifact["sha256"], artifact["kind"], repo, skill_root, policy, connection,
        runtime_root=runtime_root,
    )
    if (verified.device, verified.inode) != (artifact["device"], artifact["inode"]):
        raise ContractError("artifact_ref", "ARTIFACT_IDENTITY_MISMATCH")


def workflow_source_binding(connection, run, branch, *, source_capture=None):
    """Verify a distinct named continuation binding; never invent a review binding.

    Omitting source_capture checks historical integrity only. A caller granting
    positive current-source eligibility must supply the actual capture adapter.
    """
    from . import workflow_authority as workflow
    from .state import StateError, workflow_bindings, workflow_context
    env = json.loads(branch["envelope_json"])
    matches = []
    for item in env["inputs"]:
        if item["kind"] != "evidence_manifest":
            continue
        row = connection.execute("SELECT * FROM artifacts WHERE run_id=? AND ref=? AND immutable=1",
                                 (run["run_id"], item["ref"])).fetchone()
        if row is None or row["source_type"] != "ledger":
            continue
        value = json.loads(row["content_json"])
        if isinstance(value, dict) and value.get("kind") == "workflow_source_binding":
            matches.append((row, value))
    if not matches:
        return None
    if len(matches) != 1:
        raise StateError("WORKFLOW_SOURCE_BINDING_INVALID")
    row, value = matches[0]
    fields = {"kind", "schema_version", "bindings", "reservation_id", "authority_ref", "branch_id",
              "envelope_digest", "inputs_digest", "fence", "source", "source_digest", "coverage"}
    inputs = [item for item in env["inputs"] if item["ref"] != row["ref"]]
    if (set(value) != fields or value["schema_version"] != 1
            or workflow.hashed(value) != row["sha256"]
            or row["ref"].rpartition("#sha256=")[2] != row["sha256"]
            or value["bindings"] != workflow_bindings(run) or value["branch_id"] != branch["branch_id"]
            or value["envelope_digest"] != workflow.hashed(workflow.envelope_identity(env))
            or value["inputs_digest"] != workflow.hashed(inputs)
            or value["source_digest"] != value["source"].get("sha256")):
        raise StateError("WORKFLOW_SOURCE_BINDING_INVALID")
    workflow.validate_source(value["source"])
    coverage = value["coverage"]
    if (not isinstance(coverage, dict) or set(coverage) != {"roots", "outputs", "activation_ref"}
            or coverage["roots"] != value["source"]["roots"] or not isinstance(coverage["outputs"], list)):
        raise StateError("WORKFLOW_SOURCE_BINDING_INVALID")
    validate_ref(coverage["activation_ref"], "activation_ref", content_required=True)
    fence = value["fence"]
    if (not isinstance(fence, dict) or set(fence) != {"branch_id", "attempt_id", "claim_digest"}
            or fence["branch_id"] != branch["branch_id"]):
        raise StateError("ATTEMPT_FENCE_MISMATCH")
    from .contracts import opaque
    opaque(fence["attempt_id"], "attempt_id")
    digest(fence["claim_digest"], "claim_digest")
    reads = [cap["target_ref"][5:] for cap in env["effect_capabilities"]
             if cap["effect"] == "filesystem_read" and cap["action"] == "read" and cap["target_ref"].startswith("repo:")]
    def contains(root, candidate):
        left = os.path.normcase(root).replace("\\", "/")
        right = os.path.normcase(candidate).replace("\\", "/")
        return right == left.rstrip("/") or (left.endswith("/") and right.startswith(left))
    if any(not any(contains(root, selected) for root in reads) for selected in value["source"]["roots"]):
        raise StateError("WORKFLOW_SOURCE_AUTHORITY_EXCEEDED")
    context = workflow.current_context() or workflow_context(connection, run)
    authority = next((item for item in context.history
                      if item["kind"] == workflow.AUTHORITY and item["ref"] == value["authority_ref"]), None)
    if authority is None or authority["activation_ref"] != coverage["activation_ref"]:
        raise StateError("WORKFLOW_SOURCE_BINDING_INVALID")
    reserved = context.actions.get(value["reservation_id"])
    pending = context.pending_record if context.operation_id == value["reservation_id"] else None
    execution = reserved["execution"] if reserved else pending.get("execution") if pending else None
    if (execution is None or execution.get("kind") != "branch"
            or execution.get("target") != workflow.envelope_identity(env)
            or execution.get("source_binding_ref") != row["ref"] or execution.get("fence") != fence
            or execution.get("inputs_digest") != value["inputs_digest"]
            or (reserved is not None and reserved["authority_ref"] != value["authority_ref"])
            or (reserved is None and context.authority_ref != value["authority_ref"])):
        raise StateError("WORKFLOW_SOURCE_BINDING_INVALID")
    phase_state = reserved["state"] if reserved else pending["state"]
    phase_facts = reserved.get("facts", {}) if reserved else {}
    if context.continuation_id == value["reservation_id"] and context.pending_record:
        phase_state = context.pending_record["state"]
        phase_facts = context.pending_record.get("facts", {})
    attempts = connection.execute("SELECT * FROM branch_attempts WHERE run_id=? AND branch_id=?",
                                  (run["run_id"], branch["branch_id"])).fetchall()
    never_launched = phase_state in {"reserved", "cancelled"} and not attempts
    if never_launched:
        if branch["status"] != "ready" or any(env.get(key) is not None for key in ("attempt_id", "claim_digest", "lease_expires_at")):
            raise StateError("WORKFLOW_RESERVED_TARGET")
    else:
        if (len(attempts) != 1 or attempts[0]["attempt_id"] != fence["attempt_id"]
                or attempts[0]["claim_digest"] != fence["claim_digest"]
                or env.get("attempt_id") != fence["attempt_id"] or env.get("claim_digest") != fence["claim_digest"]):
            raise StateError("ATTEMPT_FENCE_MISMATCH")
        late = phase_facts.get("late_settlement") is True
        if late:
            if phase_facts.get("raw_node_digest") != workflow.hashed(dict(branch)):
                raise StateError("WORKFLOW_LATE_FACTS_MISMATCH")
        elif phase_state in {"settled", "succeeded", "failed"}:
            if branch["status"] not in {"succeeded", "failed", "timed_out"} or attempts[0]["finished_at"] is None:
                raise StateError("WORKFLOW_SETTLEMENT_MISMATCH")
        elif branch["status"] != "running" or attempts[0]["finished_at"] is not None:
            raise StateError("WORKFLOW_RESERVED_TARGET")
    if source_capture is not None:
        captured = source_capture(coverage)
        workflow.validate_source(captured)
        if captured["sha256"] != value["source_digest"]:
            raise StateError("WORKFLOW_SOURCE_CHANGED")
    return row, value, execution
