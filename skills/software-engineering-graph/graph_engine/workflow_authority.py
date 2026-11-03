"""Pure workflow authority policy and append-only lifecycle reconstruction.

Persistence and executable-envelope validation belong to the state/CLI adapters.
A workflow decision cannot supply evidence, an execution capability, or a human
identity. The local attestation limitation is part of every approval and report.
"""

from contextlib import contextmanager
from contextvars import ContextVar
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
from typing import Any, Callable, Dict, Mapping, Optional, Sequence, Tuple

from .ids import canonical_bytes, sha256_bytes, stable_id


AUTHORITY = "workflow_authority"
CONTINUATION = "workflow_continuation"
REVOCATION = "workflow_revocation"
END = "workflow_end"
KINDS = frozenset({AUTHORITY, CONTINUATION, REVOCATION, END})
OPEN_STATES = frozenset({"reserved", "launched", "unknown"})
SETTLED_STATES = frozenset({"settled", "succeeded", "failed", "cancelled", "external_denied"})
BLANKET_INTENT = (
    "I request blanket authority for any present or future graph-owned workflow "
    "restriction for this exact task and run, within the recorded authorized "
    "effects and finite stop contract. This does not grant external permissions "
    "or change actual evidence, decisions, outcomes, history or consumed use."
)
ASSURANCE = (
    "Local principal attestation and hashes do not authenticate human identity. "
    "A dishonest process using the same principal cannot be distinguished "
    "cryptographically from the trusted Supervisor."
)


class WorkflowAuthorityError(ValueError):
    """Invalid or unusable authority; this failure is never a workflow waiver."""


def hashed(value: Any) -> str:
    return sha256_bytes(canonical_bytes(value))


def envelope_identity(value: Mapping[str, Any]) -> Dict[str, Any]:
    """Keep executable identity distinct from inputs and append-only runtime facts."""
    runtime = {"status", "artifact_ref", "evidence", "decision", "retry_count", "attempt_id", "claim_digest",
               "lease_expires_at", "failure_code", "started_at", "finished_at", "inputs"}
    return {key: item for key, item in value.items() if key not in runtime}


def _fail(code="WORKFLOW_HISTORY_INVALID"):
    raise WorkflowAuthorityError(code)


def _text(value, maximum=4096):
    return isinstance(value, str) and bool(value.strip()) and len(value) <= maximum and not any(ord(char) < 32 for char in value)


def _digest(value):
    return isinstance(value, str) and len(value) == 64 and all(char in "0123456789abcdef" for char in value)


def timestamp(value: Any) -> datetime:
    if not isinstance(value, str):
        _fail("WORKFLOW_TIMESTAMP_INVALID")
    try:
        result = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise WorkflowAuthorityError("WORKFLOW_TIMESTAMP_INVALID") from error
    if result.tzinfo is None:
        _fail("WORKFLOW_TIMESTAMP_INVALID")
    return result.astimezone(timezone.utc)


def validate_contract(value: Any, now: str) -> Dict[str, Any]:
    keys = {"max_actions", "max_inflight", "expires_at"}
    if not isinstance(value, dict) or set(value) != keys:
        _fail("WORKFLOW_CONTRACT_INVALID")
    if (type(value["max_actions"]) is not int or value["max_actions"] < 1
            or type(value["max_inflight"]) is not int
            or not 1 <= value["max_inflight"] <= value["max_actions"]):
        _fail("WORKFLOW_CONTRACT_INVALID")
    if timestamp(value["expires_at"]) <= timestamp(now):
        _fail("WORKFLOW_AUTHORITY_EXPIRED")
    return dict(value)


def validate_source(value):
    """Check a historical capture's internal truth; never rebind it to new bytes."""
    if (not isinstance(value, dict) or set(value) != {"sha256", "roots", "path_count", "manifest"}
            or not _digest(value["sha256"]) or not isinstance(value["roots"], list)
            or not value["roots"] or value["roots"] != sorted(set(value["roots"]))
            or any(not _text(root) for root in value["roots"])):
        _fail("WORKFLOW_SOURCE_INVALID")
    manifest = value["manifest"]
    if (not isinstance(manifest, dict) or set(manifest) != {"head", "paths"}
            or not isinstance(manifest["head"], str) or len(manifest["head"]) not in {40, 64}
            or any(char not in "0123456789abcdef" for char in manifest["head"])
            or not isinstance(manifest["paths"], list) or type(value["path_count"]) is not int
            or value["path_count"] != len(manifest["paths"]) or hashed(manifest) != value["sha256"]):
        _fail("WORKFLOW_SOURCE_INVALID")
    paths = []
    for item in manifest["paths"]:
        if not isinstance(item, dict) or not isinstance(item.get("exists"), bool):
            _fail("WORKFLOW_SOURCE_INVALID")
        keys = {"path", "index", "exists"} | ({"mode", "sha256"} if item["exists"] else set())
        if (set(item) != keys or not _text(item["path"]) or "\\" in item["path"]
                or item["path"].startswith("/") or ".." in item["path"].split("/")
                or not isinstance(item["index"], list)
                or (item["exists"] and (type(item["mode"]) is not int or not _digest(item["sha256"])))):
            _fail("WORKFLOW_SOURCE_INVALID")
        paths.append(item["path"])
    if paths != sorted(set(paths)):
        _fail("WORKFLOW_SOURCE_INVALID")
    return value


def validate_provenance(value: Any, intent: Mapping[str, Any], now: Optional[str] = None) -> Dict[str, Any]:
    """Validate exact statements; the adapter separately attests trusted origin."""
    if (not isinstance(value, dict) or set(value) != {"origin_kind", "request", "approval"}
            or value["origin_kind"] != "trusted_human_message"):
        _fail("WORKFLOW_HUMAN_PROVENANCE_REQUIRED")
    intent_digest = hashed(intent)
    for name, statement in (("request", BLANKET_INTENT),
                            ("approval", "I approve workflow intent " + intent_digest)):
        message = value[name]
        fields = {"message_id", "provenance_ref", "statement", "content_sha256", "timestamp", "intent_digest"}
        if (not isinstance(message, dict) or set(message) != fields
                or any(not _text(message[key], 16384) for key in fields)
                or message["statement"] != statement or message["intent_digest"] != intent_digest
                or message["content_sha256"] != sha256_bytes(statement.encode("utf-8"))):
            _fail("WORKFLOW_HUMAN_PROVENANCE_REQUIRED")
        if now is not None and timestamp(message["timestamp"]) > timestamp(now):
            _fail("WORKFLOW_HUMAN_PROVENANCE_REQUIRED")
    if (value["request"]["message_id"] == value["approval"]["message_id"]
            or timestamp(value["approval"]["timestamp"]) < timestamp(value["request"]["timestamp"])):
        _fail("WORKFLOW_HUMAN_PROVENANCE_REQUIRED")
    return {**value, "request": dict(value["request"]), "approval": dict(value["approval"])}


@dataclass(frozen=True)
class WorkflowRequirement:
    code: str
    subject_ref: str
    raw_state: Any
    effective_action: str
    supporting_refs: Tuple[str, ...] = ()


@dataclass(frozen=True)
class Decision:
    requirement: WorkflowRequirement
    outcome: str
    authority_ref: Optional[str] = None
    continuation_ref: Optional[str] = None


@dataclass
class EffectiveContext:
    authority_ref: Optional[str] = None
    authority: Optional[Mapping[str, Any]] = None
    state: str = "absent"
    used: int = 0
    inflight: Dict[str, Mapping[str, Any]] = field(default_factory=dict)
    history: Tuple[Mapping[str, Any], ...] = ()
    historical_decisions: Tuple[Mapping[str, Any], ...] = ()
    actions: Dict[str, Mapping[str, Any]] = field(default_factory=dict)
    operation_id: Optional[str] = None
    action: Optional[str] = None
    decisions: list = field(default_factory=list)
    continuation_id: Optional[str] = None
    phase: Optional[str] = None
    validation_depth: int = 0
    dispatch_ref: Optional[str] = None
    expected_revision: Optional[int] = None
    capture_source: Optional[Callable] = None
    pending_record: Optional[Dict[str, Any]] = None
    objective_complete: bool = False

    @property
    def remaining(self):
        return max(0, self.authority["intent"]["contract"]["max_actions"] - self.used) if self.authority else 0

    @property
    def settling(self):
        return self.continuation_id if self.phase in {"settled", "facts"} else None

    def begin(self, operation_id, action, *, continuation_id=None, phase="reserved"):
        if not self.authority:
            _fail("WORKFLOW_AUTHORITY_REQUIRED")
        if continuation_id is None:
            if self.state != "active" or self.remaining == 0:
                _fail("WORKFLOW_AUTHORITY_" + ("EXHAUSTED" if self.remaining == 0 else self.state.upper()))
            if len(self.inflight) >= self.authority["intent"]["contract"]["max_inflight"]:
                _fail("WORKFLOW_INFLIGHT_LIMIT")
        else:
            prior = self.actions.get(continuation_id)
            if prior is None or prior["authority_ref"] != self.authority_ref:
                _fail("WORKFLOW_RESERVATION_NOT_FOUND")
            if phase == "launched":
                if self.state != "active" or prior["state"] != "reserved":
                    _fail("WORKFLOW_LAUNCH_STOPPED")
            elif phase in {"settled", "facts"}:
                if prior["state"] not in OPEN_STATES:
                    _fail("WORKFLOW_RESERVATION_CLOSED")
            else:
                _fail("WORKFLOW_PHASE_INVALID")
        self.operation_id, self.action = operation_id, action
        self.continuation_id, self.phase = continuation_id, phase

    def report(self):
        extension_use = {}
        for action in self.actions.values():
            if action["authority_ref"] != self.authority_ref:
                continue
            for key, amount in action.get("extension_use", {}).items():
                extension_use[key] = extension_use.get(key, 0) + amount
        return {"authority_ref": self.authority_ref, "state": self.state,
                "used_actions": self.used, "remaining_actions": self.remaining,
                "inflight_actions": sorted(self.inflight), "extension_use": extension_use,
                "objective_complete": self.objective_complete, "assurance": ASSURANCE,
                "waivers": list(self.historical_decisions),
                "actions": [{key: item.get(key) for key in
                    ("reservation_id", "authority_ref", "ordinal", "state", "execution", "outcome", "source", "latest_ref")}
                    for item in self.actions.values()]}


_context = ContextVar("graph_workflow_authority", default=None)


@contextmanager
def using_context(context):
    token = _context.set(context)
    try:
        yield context
    finally:
        _context.reset(token)


def current_context():
    return _context.get()


@contextmanager
def validation_scope():
    context = current_context()
    if context is not None:
        context.validation_depth += 1
    try:
        yield
    finally:
        if context is not None:
            context.validation_depth -= 1


def decide_workflow(requirement, context, *, satisfied):
    """One rule for all graph-owned requirements; codes never select eligibility."""
    if not isinstance(requirement, WorkflowRequirement) or not all(
            _text(value) for value in (requirement.code, requirement.subject_ref, requirement.effective_action)):
        _fail("WORKFLOW_REQUIREMENT_INVALID")
    if satisfied:
        return Decision(requirement, "SATISFIED")
    if context is None:
        return Decision(requirement, "STOPPED")
    encoded = asdict(requirement)
    for previous in context.historical_decisions if context.validation_depth else ():
        if hashed(previous["requirement"]) == hashed(encoded):
            return Decision(requirement, "WAIVED", previous["authority_ref"], previous["continuation_ref"])
    if context.operation_id and context.authority and (context.state == "active" or context.settling):
        decision = Decision(requirement, "WAIVED", context.authority_ref)
        if asdict(decision) not in context.decisions:
            context.decisions.append(asdict(decision))
        return decision
    return Decision(requirement, "STOPPED", context.authority_ref)


def gate(satisfied, code, subject_ref, raw_state, effective_action):
    return decide_workflow(WorkflowRequirement(code, subject_ref, raw_state, effective_action),
                           current_context(), satisfied=satisfied).outcome != "STOPPED"


def _validate_decisions(values, authority_ref, reference):
    if not isinstance(values, list):
        _fail()
    result = []
    for value in values:
        if (not isinstance(value, dict) or set(value) != {"requirement", "outcome", "authority_ref", "continuation_ref"}
                or value["outcome"] != "WAIVED" or value["authority_ref"] != authority_ref
                or value["continuation_ref"] is not None):
            _fail()
        requirement = value["requirement"]
        if (not isinstance(requirement, dict)
                or set(requirement) != {"code", "subject_ref", "raw_state", "effective_action", "supporting_refs"}
                or not all(_text(requirement[key]) for key in ("code", "subject_ref", "effective_action"))
                or not isinstance(requirement["supporting_refs"], list)
                or any(not _text(item) for item in requirement["supporting_refs"])):
            _fail()
        result.append({**value, "continuation_ref": reference})
    return result


def _validate_settlement_joins(value, operation, bindings):
    """Join bookkeeping must retain canonical result provenance, never launch work."""
    response, facts = operation["response"], value["facts"]
    if (not isinstance(facts, dict) or value["created_join_ids"] != response.get("created_join_ids", [])
            or value["created_branch_ids"] != response.get("successor_branch_ids", [])):
        _fail()
    joins = value["created_join_ids"]
    proofs = facts.get("settlement_joins", [])
    if not joins:
        if proofs:
            _fail()
        return
    execution = value["execution"]
    fence, target = execution.get("fence", {}), execution.get("target", {})
    if (value["phase"] != "settled" or value["action"] != "record.branch-result"
            or operation["command"] != "record.branch-result" or value["state"] != "succeeded"
            or value["outcome"] != "succeeded" or execution.get("kind") not in {"branch", "existing_branch"}
            or not isinstance(joins, list) or len(joins) != 1 or not isinstance(proofs, list) or len(proofs) != 1
            or response.get("branch_id") != fence.get("branch_id") or response.get("branch_status") != "succeeded"
            or any(operation["request"].get(key) != fence.get(key) for key in ("branch_id", "attempt_id", "claim_digest"))
            or facts.get("attempt_id") != fence.get("attempt_id") or facts.get("claim_digest") != fence.get("claim_digest")
            or not _digest(facts.get("result_digest"))
            or facts.get("result_ref") != "ledger:" + str(fence.get("branch_id")) + "#sha256=" + facts["result_digest"]):
        _fail("WORKFLOW_SETTLEMENT_JOIN_INVALID")
    proof = proofs[0]
    fields = {"join_id", "join_key", "kind", "stage", "generation", "member_branch_ids", "source_result_ref", "source_result_digest"}
    if (not isinstance(proof, dict) or set(proof) != fields or type(proof["generation"]) is not int
            or proof["generation"] != target.get("generation") or proof["member_branch_ids"] != [fence.get("branch_id")]
            or proof["source_result_ref"] != facts["result_ref"] or proof["source_result_digest"] != facts["result_digest"]
            or not all(_text(proof[key]) for key in ("join_key", "kind", "stage"))
            or joins != [proof["join_id"]] or proof["join_id"] != stable_id(bindings["run_id"], bindings["policy_digest"],
                "join", proof["join_key"], proof["generation"])):
        _fail("WORKFLOW_SETTLEMENT_JOIN_INVALID")


def reconstruct_workflow_history(records, operations, bindings, now, selected_ref=None, *, objective_complete=False):
    """Derive current state while validating every immutable historical transition."""
    authorities, actions, stopped = {}, {}, {}
    decisions, history = [], []
    prepared = []
    for record in records:
        value = record.get("value")
        if not isinstance(value, dict) or type(value.get("revision")) is not int:
            _fail()
        prepared.append(record)
    for record in sorted(prepared, key=lambda item: (item["value"]["revision"],
            0 if item["value"]["kind"] == AUTHORITY else 2 if item["value"]["kind"] in {REVOCATION, END} else 1,
            item["ref"])):
        value, reference = record["value"], record["ref"]
        kind = value.get("kind")
        operation = operations.get(value.get("operation_id"))
        base = {"kind", "schema_version", "bindings", "operation_id", "request_digest", "revision", "created_at"}
        if (not base.issubset(value) or kind not in KINDS or value["schema_version"] != 1
                or value["bindings"] != bindings or operation is None
                or operation["revision"] != value["revision"] or operation["request_digest"] != value["request_digest"]
                or hashed(operation["request"]) != value["request_digest"]
                or operation["request"].get("command") != operation["command"]
                or record["sha256"] != hashed(value)
                or reference.rpartition("#sha256=")[2] != record["sha256"]
                or timestamp(value["created_at"]) > timestamp(now)):
            _fail()
        if kind == AUTHORITY:
            extra = {"intent", "intent_digest", "activation_ref", "provenance", "origin_kind", "actor", "host_identity", "assurance"}
            if (set(value) != base | extra or operation["command"] != "workflow-override.approve"
                    or value["origin_kind"] != "trusted_human_message" or value["host_identity"] != bindings["host_identity"]
                    or not _text(value["actor"]) or value["assurance"] != ASSURANCE):
                _fail()
            intent = value["intent"]
            if (not isinstance(intent, dict) or intent.get("blanket_intent") != BLANKET_INTENT
                    or intent.get("bindings") != bindings or value["intent_digest"] != hashed(intent)
                    or operation["request"].get("intent_digest") != value["intent_digest"]
                    or operation["request"].get("attest_human_origin") is not True
                    or intent.get("source_evolution") != "same_authorized_task"
                    or intent.get("scope_effects_digest") != bindings["effects_digest"]
                    or intent.get("initial_revision") != value["revision"] - 1):
                _fail()
            validate_source(intent["initial_source"])
            validate_contract(intent["contract"], value["created_at"])
            validate_provenance(value["provenance"], intent, value["created_at"])
            if any(ref not in stopped for ref in authorities):
                _fail("WORKFLOW_DUPLICATE_AUTHORITY")
            authorities[reference] = value
        else:
            authority_ref = value.get("authority_ref")
            authority = authorities.get(authority_ref)
            if authority is None or timestamp(value["created_at"]) < timestamp(authority["created_at"]):
                _fail()
            if kind in {REVOCATION, END}:
                if (set(value) - (base | {"authority_ref", "reason", "effective_status"})
                        or not _text(value.get("reason")) or authority_ref in stopped
                        or operation["command"] not in {"workflow-override.revoke", "workflow-override.end", "workflow-override.continue", "workflow-override.settle", "check.finish"}):
                    _fail()
                stopped[authority_ref] = {"state": "revoked" if kind == REVOCATION else "ended", "reason": value["reason"]}
            else:
                keys = {"authority_ref", "phase", "reservation_id", "ordinal", "state", "action", "decisions",
                        "source", "execution", "execution_digest", "predecessor_ref", "consumption_deltas",
                        "extension_use", "created_branch_ids", "created_join_ids", "outcome", "facts"}
                if (set(value) != base | keys or not _text(value["reservation_id"]) or not _text(value["action"])
                        or operation["request"].get("workflow_authority_ref") != authority_ref
                        or operation["request"].get("expected_revision") != value["revision"] - 1
                        or not isinstance(value["execution"], dict) or value["execution_digest"] != hashed(value["execution"])
                        or not isinstance(value["consumption_deltas"], dict)
                        or any(type(amount) is not int or amount < 0 for amount in value["consumption_deltas"].values())
                        or not isinstance(value["extension_use"], dict)
                        or any(type(amount) is not int or amount < 0 for amount in value["extension_use"].values())):
                    _fail()
                validate_source(value["source"])
                decisions.extend(_validate_decisions(value["decisions"], authority_ref, reference))
                reservation = value["reservation_id"]
                earlier = [item for item in actions.values() if item["authority_ref"] == authority_ref]
                if value["phase"] == "reserved":
                    if (authority_ref in stopped or reservation in actions or value["predecessor_ref"] is not None
                            or value["ordinal"] != len(earlier) + 1
                            or len(earlier) >= authority["intent"]["contract"]["max_actions"]
                            or sum(item["state"] in OPEN_STATES for item in earlier) >= authority["intent"]["contract"]["max_inflight"]
                            or any(item["state"] == "unknown" for item in actions.values())
                            or timestamp(value["created_at"]) >= timestamp(authority["intent"]["contract"]["expires_at"])
                            or value["state"] not in OPEN_STATES | SETTLED_STATES):
                        _fail()
                    actions[reservation] = {**value, "latest_ref": reference}
                elif value["phase"] in {"launched", "settled", "facts"}:
                    prior = actions.get(reservation)
                    if (prior is None or prior["authority_ref"] != authority_ref
                            or value["ordinal"] != prior["ordinal"]
                            or value["predecessor_ref"] != prior["latest_ref"]
                            or value["execution"] != prior["execution"]
                            or value["created_branch_ids"] or value["extension_use"]):
                        _fail()
                    _validate_settlement_joins(value, operation, bindings)
                    if value["phase"] == "launched":
                        if (authority_ref in stopped or prior["state"] != "reserved" or value["state"] != "launched"
                                or timestamp(value["created_at"]) >= timestamp(authority["intent"]["contract"]["expires_at"])
                                or any(item["state"] == "unknown" for item in actions.values())):
                            _fail()
                    elif prior["state"] not in OPEN_STATES or value["state"] not in SETTLED_STATES | {"unknown"}:
                        _fail()
                    actions[reservation] = {**prior, "state": value["state"], "outcome": value["outcome"],
                        "facts": value["facts"], "latest_ref": reference, "settled_after_stop": authority_ref in stopped}
                else:
                    _fail()
        history.append({"ref": reference, **value})
    if selected_ref is None and authorities:
        selected_ref = next(reversed(authorities))
    if selected_ref is not None and selected_ref not in authorities:
        _fail("WORKFLOW_AUTHORITY_MISMATCH")
    authority = authorities.get(selected_ref)
    selected = [value for value in actions.values() if value["authority_ref"] == selected_ref]
    inflight = {value["reservation_id"]: value for value in selected if value["state"] in OPEN_STATES}
    state = "absent"
    complete = objective_complete or any(item.get("effective_status") == "complete_with_waivers" for item in history if item["kind"] == END)
    if authority:
        state = stopped.get(selected_ref, {}).get("state", "active")
        if state == "active" and complete:
            state = "ended"
        if state == "active" and timestamp(now) >= timestamp(authority["intent"]["contract"]["expires_at"]):
            state = "expired"
        if state == "active" and len(selected) >= authority["intent"]["contract"]["max_actions"] and not inflight:
            state = "exhausted"
        if state == "active" and any(item["state"] == "unknown" for item in actions.values()):
            state = "uncertain"
        if state == "active" and any(item["state"] == "external_denied" for item in selected):
            state = "external_denied"
    return EffectiveContext(authority_ref=selected_ref, authority=authority, state=state, used=len(selected),
        inflight=inflight, history=tuple(history), historical_decisions=tuple(decisions), actions=actions,
        objective_complete=complete)
