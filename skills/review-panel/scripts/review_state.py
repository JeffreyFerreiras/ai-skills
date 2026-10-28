#!/usr/bin/env python3
"""Per-run review-panel state: blind first pass, targeted follow-up, stale-source guard.

Standard library only. The Supervisor owns and writes the record; reviewers never
write it. Prompts are cooperative: this helper makes blind/locked packets easy to
construct correctly, but it cannot confine a model or serialize writers. Run one
Supervisor per record file.

Record and log files live outside the installed skill and unrelated source, for
example a disposable run directory passed on the command line.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import re
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

STATE_VERSION = 1

DEFAULT_ROLES = (
    "code-quality",
    "architecture",
    "naming",
    "bug-hunting",
    "logic",
    "counterexamples",
    "validation-audit",
)

PHASE_INITIAL = "initial"
PHASE_RECONCILING = "reconciling"
PHASE_CLOSED = "closed"

DISPOSITIONS = ("proposed", "accepted", "rejected", "unresolved", "out-of-scope")
STANCES = ("agree", "withdraw", "dispute", "note")
FIX_STATES = ("open", "fixed", "verified", "failed")
ROLE_STATES = ("pending", "reported", "unknown")
REQUEST_STATES = ("pending", "replied", "unresolved")

_FOLLOWUP_PREFIX = "Q"
_FINDING_RE = re.compile(r"^F([1-9][0-9]*)$")
_FOLLOWUP_RE = re.compile(r"^Q([1-9][0-9]*)$")
_SHA256_RE = re.compile(r"^[0-9a-fA-F]{64}$")


class StateError(ValueError):
    """Raised for malformed records, phase violations, or identity mismatches."""


def _utcnow() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def _is_nonempty_str(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def sha256_file(path: str | Path) -> str:
    """Hash a declared input file. The caller names the file; bytes are hashed here."""
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _check_digest(name: str, digest: str) -> None:
    if not isinstance(digest, str) or not _SHA256_RE.match(digest):
        raise StateError(f"input {name!r} must be a 64-character hex sha256 digest.")


def init_state(
    run_id: str,
    workspace: str,
    base: str,
    head: str,
    review_revision: str = "",
    inputs: dict[str, str] | None = None,
    scope: list[str] | None = None,
    roles: list[str] | None = None,
    supersedes: str = "",
) -> dict[str, Any]:
    if not run_id or not workspace or not base or not head:
        raise StateError("run_id, workspace, base, and head are required.")
    role_list = list(roles) if roles else list(DEFAULT_ROLES)
    if len(set(role_list)) != len(role_list) or not role_list:
        raise StateError("roles must be a non-empty list of unique names.")
    for name, digest in (inputs or {}).items():
        _check_digest(name, digest)
    for entry in scope or []:
        if not isinstance(entry, str):
            raise StateError("scope entries must be strings.")
    state = {
        "version": STATE_VERSION,
        "run_id": run_id,
        "review_identity": {
            "workspace": workspace,
            "base": base,
            "head": head,
            "review_revision": review_revision,
            "scope": list(scope or []),
            "inputs": [
                {"name": name, "sha256": digest}
                for name, digest in sorted((inputs or {}).items())
            ],
        },
        "phase": PHASE_INITIAL,
        "roles": {
            role: {"status": "pending", "reported_at": None, "note": ""}
            for role in role_list
        },
        "reports": {},
        "findings": [],
        "next_finding": 1,
        "followups": [],
        "next_followup": 1,
        "source_stale": False,
        "stale_reason": "",
        "history": [],
    }
    detail = f"run {run_id} opened."
    if supersedes:
        detail += f" Supersedes {supersedes}."
    state["history"].append({"at": _utcnow(), "event": "init", "detail": detail})
    return state


def _require_history(events: Any, where: str) -> None:
    if not isinstance(events, list):
        raise StateError(f"{where} history must be a list.")
    for event in events:
        if (
            not isinstance(event, dict)
            or not _is_nonempty_str(event.get("at"))
            or not _is_nonempty_str(event.get("event"))
            or not isinstance(event.get("detail"), str)
        ):
            raise StateError(f"{where} history entries need at/event/detail strings.")


def _validate(state: dict[str, Any]) -> None:
    if not isinstance(state, dict):
        raise StateError("record must be a JSON object.")
    if state.get("version") != STATE_VERSION:
        raise StateError(f"unsupported record version: {state.get('version')!r}.")
    for key in (
        "run_id",
        "review_identity",
        "phase",
        "roles",
        "reports",
        "findings",
        "next_finding",
        "followups",
        "next_followup",
        "source_stale",
        "stale_reason",
        "history",
    ):
        if key not in state:
            raise StateError(f"record is missing required key: {key}.")
    if not _is_nonempty_str(state["run_id"]):
        raise StateError("run_id must be a non-empty string.")
    if state["phase"] not in (PHASE_INITIAL, PHASE_RECONCILING, PHASE_CLOSED):
        raise StateError(f"unknown phase: {state['phase']!r}.")
    identity = state["review_identity"]
    if not isinstance(identity, dict):
        raise StateError("review_identity must be an object.")
    for key in ("workspace", "base", "head"):
        if not _is_nonempty_str(identity.get(key)):
            raise StateError(f"review_identity is missing required key: {key}.")
    if not isinstance(identity.get("review_revision"), str):
        raise StateError("review_identity.review_revision must be a string.")
    if not isinstance(identity.get("scope"), list) or not all(
        isinstance(entry, str) for entry in identity["scope"]
    ):
        raise StateError("review_identity.scope must be a list of strings.")
    if not isinstance(identity.get("inputs"), list):
        raise StateError("review_identity.inputs must be a list.")
    seen_inputs: set[str] = set()
    for item in identity["inputs"]:
        if (
            not isinstance(item, dict)
            or not _is_nonempty_str(item.get("name"))
            or item["name"] in seen_inputs
        ):
            raise StateError("review_identity.inputs needs unique non-empty names.")
        seen_inputs.add(item["name"])
        _check_digest(item["name"], item.get("sha256", ""))
    roles = state["roles"]
    if not isinstance(roles, dict) or not roles:
        raise StateError("roles must be a non-empty object.")
    for role, entry in roles.items():
        if (
            not isinstance(entry, dict)
            or entry.get("status") not in ROLE_STATES
            or not isinstance(entry.get("note"), str)
            or not (entry.get("reported_at") is None or isinstance(entry.get("reported_at"), str))
        ):
            raise StateError(f"role {role!r} has a malformed outcome entry.")
        if entry["status"] == "unknown" and not entry["note"].strip():
            raise StateError(f"role {role!r} is unknown without a reason.")
    reports = state["reports"]
    if not isinstance(reports, dict):
        raise StateError("reports must be an object.")
    for role, report in reports.items():
        if role not in roles:
            raise StateError(f"report for unknown role: {role!r}.")
        if (
            not isinstance(report, dict)
            or not _is_nonempty_str(report.get("text"))
            or not _is_nonempty_str(report.get("received_at"))
        ):
            raise StateError(f"report for {role!r} needs text/received_at strings.")
    for role, entry in roles.items():
        reported = role in reports
        if (entry["status"] == "reported") != reported:
            raise StateError(f"role {role!r} status does not match its stored report.")
    findings = state["findings"]
    if not isinstance(findings, list):
        raise StateError("findings must be a list.")
    seen_ids: set[str] = set()
    numeric_suffixes: list[int] = []
    for finding in findings:
        if not isinstance(finding, dict):
            raise StateError("each finding must be an object.")
        fid = finding.get("id")
        if not _is_nonempty_str(fid) or fid in seen_ids:
            raise StateError(f"findings need unique non-empty string ids; got {fid!r}.")
        seen_ids.add(fid)
        match = _FINDING_RE.match(fid)
        if match:
            numeric_suffixes.append(int(match.group(1)))
        if finding.get("origin_role") not in roles:
            raise StateError(f"finding {fid!r} has an unknown origin role.")
        if not _is_nonempty_str(finding.get("title")):
            raise StateError(f"finding {fid!r} needs a non-empty title.")
        if not isinstance(finding.get("evidence"), dict):
            raise StateError(f"finding {fid!r} evidence must be an object.")
        if finding.get("disposition") not in DISPOSITIONS:
            raise StateError(f"finding {fid!r} has an unknown disposition.")
        if finding["disposition"] != "proposed" and not _is_nonempty_str(
            finding.get("reason")
        ):
            raise StateError(f"finding {fid!r} needs a reason for its disposition.")
        if not isinstance(finding.get("reason"), str):
            raise StateError(f"finding {fid!r} reason must be a string.")
        if not isinstance(finding.get("stale"), bool):
            raise StateError(f"finding {fid!r} stale flag must be a boolean.")
        if not isinstance(finding.get("replies"), list):
            raise StateError(f"finding {fid!r} replies must be a list.")
        for reply in finding["replies"]:
            if (
                not isinstance(reply, dict)
                or reply.get("role") not in roles
                or reply.get("stance") not in STANCES
                or not _is_nonempty_str(reply.get("text"))
                or not _is_nonempty_str(reply.get("at"))
            ):
                raise StateError(f"finding {fid!r} has a malformed reply.")
        fix = finding.get("fix")
        if (
            not isinstance(fix, dict)
            or fix.get("status") not in FIX_STATES
            or not isinstance(fix.get("detail"), str)
        ):
            raise StateError(f"finding {fid!r} fix must name a known status.")
        _require_history(finding.get("history"), f"finding {fid!r}")
    next_finding = state["next_finding"]
    if not isinstance(next_finding, int) or isinstance(next_finding, bool):
        raise StateError("next_finding must be an integer.")
    floor = max(numeric_suffixes, default=0)
    if next_finding < 1 or next_finding <= floor:
        raise StateError(
            "next_finding must be positive and greater than existing finding ids."
        )
    followups = state["followups"]
    if not isinstance(followups, list):
        raise StateError("followups must be a list.")
    seen_requests: set[str] = set()
    request_numbers: list[int] = []
    for request in followups:
        if not isinstance(request, dict):
            raise StateError("each follow-up request must be an object.")
        rid = request.get("id")
        if not _is_nonempty_str(rid) or rid in seen_requests:
            raise StateError(f"follow-up requests need unique ids; got {rid!r}.")
        seen_requests.add(rid)
        match = _FOLLOWUP_RE.match(rid)
        if match:
            request_numbers.append(int(match.group(1)))
        if request.get("role") not in roles:
            raise StateError(f"follow-up {rid!r} names an unknown role.")
        ids = request.get("finding_ids")
        if not isinstance(ids, list) or not ids or not all(
            isinstance(fid, str) and fid in seen_ids for fid in ids
        ):
            raise StateError(f"follow-up {rid!r} must list known finding ids.")
        if request.get("status") not in REQUEST_STATES:
            raise StateError(f"follow-up {rid!r} has an unknown status.")
        if not isinstance(request.get("source"), dict):
            raise StateError(f"follow-up {rid!r} must snapshot its source identity.")
        if not _is_nonempty_str(request.get("created_at")) or not isinstance(
            request.get("reason"), str
        ):
            raise StateError(f"follow-up {rid!r} needs created_at and reason.")
    next_followup = state["next_followup"]
    if not isinstance(next_followup, int) or isinstance(next_followup, bool):
        raise StateError("next_followup must be an integer.")
    if next_followup < 1 or next_followup <= max(request_numbers, default=0):
        raise StateError(
            "next_followup must be positive and greater than existing request ids."
        )
    if not isinstance(state["source_stale"], bool):
        raise StateError("source_stale must be a boolean.")
    if not isinstance(state["stale_reason"], str):
        raise StateError("stale_reason must be a string.")
    _require_history(state.get("history"), "run")


def load_state(path: str | Path) -> dict[str, Any]:
    """Load and validate a record. Never writes; malformed files raise StateError."""
    raw = Path(path).read_bytes()
    try:
        state = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise StateError(f"record at {path} is not valid JSON: {error}.") from error
    if isinstance(state, dict):
        # Forward-compatible defaults for records written before these keys
        # existed. Present-but-wrong types still fail validation below.
        state.setdefault("followups", [])
        state.setdefault("next_followup", 1)
        state.setdefault("source_stale", False)
        state.setdefault("stale_reason", "")
    _validate(state)
    return state


def save_state(path: str | Path, state: dict[str, Any]) -> None:
    """Validate, then atomically replace the record.

    Validation happens before any file write, so a bad state never touches the
    record. The write goes to a temp file in the same directory followed by
    os.replace, so a failed write leaves the previous valid record readable.
    Rename is crash safety against partial writes, not power-loss durability
    and not a guard against concurrent writers: run one Supervisor per file.
    """
    _validate(copy.deepcopy(state))
    target = Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp_name = tempfile.mkstemp(
        dir=str(target.parent), prefix=target.name + ".", suffix=".tmp"
    )
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            json.dump(state, handle, indent=2, sort_keys=True)
            handle.write("\n")
        os.replace(tmp_name, target)
    except BaseException:
        try:
            os.unlink(tmp_name)
        except OSError:
            pass
        raise


def _require_role(state: dict[str, Any], role: str) -> None:
    if role not in state["roles"]:
        raise StateError(f"unknown role: {role!r}.")


def _require_locked(state: dict[str, Any], action: str) -> None:
    if state["phase"] == PHASE_INITIAL:
        raise StateError(
            f"cannot {action} before the initial phase is locked; "
            "blind collection is still open."
        )
    if state["phase"] == PHASE_CLOSED:
        raise StateError(f"cannot {action}; the run is closed. Start a new run.")


def _require_fresh(state: dict[str, Any], action: str) -> None:
    if state.get("source_stale"):
        raise StateError(
            f"cannot {action} on a stale source ({state.get('stale_reason', '')}). "
            "Start a new verified run for live decisions."
        )


def _pending_requests_for(state: dict[str, Any], finding_id: str) -> list[dict[str, Any]]:
    return [
        request
        for request in state["followups"]
        if request["status"] == "pending" and finding_id in request["finding_ids"]
    ]


def add_initial_report(state: dict[str, Any], role: str, text: str) -> None:
    """Store one role's immutable first-pass report. Phase must be initial."""
    if state["phase"] != PHASE_INITIAL:
        raise StateError("initial reports are closed: the initial phase is locked.")
    _require_role(state, role)
    status = state["roles"][role]["status"]
    if status == "reported":
        raise StateError(f"original report for {role!r} is immutable; not overwritten.")
    if status == "unknown":
        raise StateError(
            f"{role!r} is recorded unknown; reopen it with reopen_role before "
            "accepting a late report, so the coverage gap stays visible."
        )
    if not text or not text.strip():
        raise StateError("initial report text must not be empty.")
    state["reports"][role] = {"text": text, "received_at": _utcnow()}
    state["roles"][role] = {
        "status": "reported",
        "reported_at": state["reports"][role]["received_at"],
        "note": "",
    }
    state["history"].append({"at": _utcnow(), "event": "report", "detail": role})


def mark_unknown(state: dict[str, Any], role: str, reason: str) -> None:
    """Record a role as terminally unknown (timeout, failure, no coverage)."""
    if state["phase"] != PHASE_INITIAL:
        raise StateError("role outcomes are locked with the initial phase.")
    _require_role(state, role)
    status = state["roles"][role]["status"]
    if status == "reported":
        raise StateError(f"{role!r} already reported; cannot mark unknown.")
    if status == "unknown":
        raise StateError(f"{role!r} is already unknown; reopen it first to change that.")
    if not reason or not reason.strip():
        raise StateError("a reason is required when marking a role unknown.")
    state["roles"][role] = {"status": "unknown", "reported_at": _utcnow(), "note": reason}
    state["history"].append(
        {"at": _utcnow(), "event": "unknown", "detail": f"{role}: {reason}"}
    )


def reopen_role(state: dict[str, Any], role: str, reason: str) -> None:
    """Reopen an unknown role before the lock so a late report can be accepted.

    Preserves the original unknown reason in history. The host must first
    settle the old execution (confirm the worker stopped, inspect workspace
    state, never run a duplicate while the first outcome is unknown) before
    redispatching; this helper keeps no process engine and settles nothing
    itself. Only a fresh run can revisit outcomes after the lock.
    """
    if state["phase"] != PHASE_INITIAL:
        raise StateError("reopen is only possible before the initial phase is locked.")
    _require_role(state, role)
    if state["roles"][role]["status"] != "unknown":
        raise StateError(f"{role!r} is not unknown; only unknown roles can reopen.")
    if not reason or not reason.strip():
        raise StateError("a reason is required to reopen an unknown role.")
    prior = state["roles"][role]["note"]
    state["roles"][role] = {"status": "pending", "reported_at": None, "note": ""}
    state["history"].append(
        {
            "at": _utcnow(),
            "event": "reopen",
            "detail": f"{role}: {reason} (was unknown: {prior})",
        }
    )


def lock_initial(state: dict[str, Any]) -> None:
    """Lock the blind phase. Every role must be reported or explicitly unknown."""
    if state["phase"] != PHASE_INITIAL:
        raise StateError("initial phase is already locked.")
    pending = [r for r, s in state["roles"].items() if s["status"] == "pending"]
    if pending:
        raise StateError(
            "cannot lock while roles are pending: "
            + ", ".join(sorted(pending))
            + ". Record reports or mark them unknown first."
        )
    state["phase"] = PHASE_RECONCILING
    state["history"].append({"at": _utcnow(), "event": "lock", "detail": "initial locked"})


def close_run(state: dict[str, Any]) -> None:
    """Close a reconciling run. Requires a fresh source and no pending follow-ups."""
    if state["phase"] != PHASE_RECONCILING:
        raise StateError("only a reconciling run can be closed.")
    _require_fresh(state, "close the run")
    pending = [r["id"] for r in state["followups"] if r["status"] == "pending"]
    if pending:
        raise StateError(
            "cannot close with pending follow-up requests: "
            + ", ".join(sorted(pending))
            + ". Record replies or mark them unresolved with a reason first."
        )
    state["phase"] = PHASE_CLOSED
    state["history"].append({"at": _utcnow(), "event": "close", "detail": "run closed"})


def add_finding(
    state: dict[str, Any],
    origin_role: str,
    title: str,
    evidence: dict[str, Any] | None = None,
) -> dict[str, Any]:
    if state["phase"] == PHASE_CLOSED:
        raise StateError("cannot add findings to a closed run.")
    _require_fresh(state, "add findings")
    _require_role(state, origin_role)
    if not title or not title.strip():
        raise StateError("finding title must not be empty.")
    if evidence is not None and not isinstance(evidence, dict):
        raise StateError("finding evidence must be an object.")
    finding_id = f"F{state.get('next_finding', 1)}"
    if any(finding["id"] == finding_id for finding in state["findings"]):
        raise StateError(
            f"finding id {finding_id!r} already exists; next_finding is rewound. "
            "Fix the counter instead of duplicating a stable id."
        )
    finding = {
        "id": finding_id,
        "origin_role": origin_role,
        "title": title,
        "evidence": evidence or {},
        "disposition": "proposed",
        "reason": "",
        "stale": False,
        "replies": [],
        "fix": {"status": "open", "detail": ""},
        "history": [{"at": _utcnow(), "event": "proposed", "detail": origin_role}],
    }
    state["findings"].append(finding)
    state["next_finding"] = state.get("next_finding", 1) + 1
    state["history"].append(
        {"at": _utcnow(), "event": "finding", "detail": f"{finding_id} from {origin_role}"}
    )
    return finding


def _find(state: dict[str, Any], finding_id: str) -> dict[str, Any]:
    for finding in state["findings"]:
        if finding["id"] == finding_id:
            return finding
    raise StateError(f"unknown finding: {finding_id!r}.")


def set_disposition(
    state: dict[str, Any], finding_id: str, disposition: str, reason: str
) -> None:
    _require_locked(state, "record dispositions")
    _require_fresh(state, "record dispositions")
    _set_disposition(state, finding_id, disposition, reason)


def _set_disposition(
    state: dict[str, Any], finding_id: str, disposition: str, reason: str
) -> None:
    if disposition not in DISPOSITIONS:
        raise StateError(f"unknown disposition: {disposition!r}.")
    if disposition != "proposed" and (not reason or not reason.strip()):
        raise StateError("a reason is required for accepted/rejected/unresolved/out-of-scope.")
    finding = _find(state, finding_id)
    if disposition != "unresolved" and _pending_requests_for(state, finding_id):
        raise StateError(
            f"finding {finding_id!r} has a pending follow-up request; record the "
            "reply or mark the request unresolved before deciding it."
        )
    finding["disposition"] = disposition
    finding["reason"] = reason or ""
    finding["history"].append(
        {"at": _utcnow(), "event": f"disposition:{disposition}", "detail": reason or ""}
    )
    state["history"].append(
        {"at": _utcnow(), "event": "disposition", "detail": f"{finding_id} -> {disposition}"}
    )


def request_followup(
    state: dict[str, Any], role: str, finding_ids: list[str], reason: str = ""
) -> dict[str, Any]:
    """Persist a targeted follow-up request before its packet goes out."""
    _require_locked(state, "request follow-up")
    _require_fresh(state, "request follow-up")
    _require_role(state, role)
    if not finding_ids:
        raise StateError("at least one finding id is required for follow-up.")
    for finding_id in finding_ids:
        _find(state, finding_id)
    request_id = f"{_FOLLOWUP_PREFIX}{state.get('next_followup', 1)}"
    if any(request["id"] == request_id for request in state["followups"]):
        raise StateError(
            f"follow-up id {request_id!r} already exists; next_followup is rewound."
        )
    identity = state["review_identity"]
    request = {
        "id": request_id,
        "role": role,
        "finding_ids": list(finding_ids),
        "source": {
            "head": identity.get("head"),
            "review_revision": identity.get("review_revision"),
            "inputs": copy.deepcopy(identity.get("inputs", [])),
        },
        "status": "pending",
        "created_at": _utcnow(),
        "reason": reason or "",
    }
    state["followups"].append(request)
    state["next_followup"] = state.get("next_followup", 1) + 1
    state["history"].append(
        {
            "at": _utcnow(),
            "event": "followup-requested",
            "detail": f"{request_id} for {role}: {', '.join(finding_ids)}",
        }
    )
    return request


def _get_request(state: dict[str, Any], request_id: str) -> dict[str, Any]:
    for request in state["followups"]:
        if request["id"] == request_id:
            return request
    raise StateError(f"unknown follow-up request: {request_id!r}.")


def resolve_request(
    state: dict[str, Any], request_id: str, outcome: str, reason: str
) -> None:
    """Mark a follow-up request unresolved with an explicit reason. Replies arrive
    via add_reply; only an unresolved gap with a reason can close a request
    without a reviewer response."""
    _require_locked(state, "resolve follow-up requests")
    if outcome != "unresolved":
        raise StateError("only an 'unresolved' outcome with a reason closes a request this way.")
    if not reason or not reason.strip():
        raise StateError("a reason is required to leave a follow-up unresolved.")
    request = _get_request(state, request_id)
    if request["status"] != "pending":
        raise StateError(f"follow-up {request_id!r} is already {request['status']}.")
    request["status"] = "unresolved"
    request["reason"] = reason
    state["history"].append(
        {"at": _utcnow(), "event": "followup-unresolved", "detail": f"{request_id}: {reason}"}
    )


def add_reply(
    state: dict[str, Any],
    finding_id: str,
    role: str,
    text: str,
    stance: str = "note",
    request_id: str | None = None,
) -> None:
    _require_locked(state, "record replies")
    _require_fresh(state, "record replies")
    _require_role(state, role)
    if stance not in STANCES:
        raise StateError(f"unknown stance: {stance!r}.")
    if not text or not text.strip():
        raise StateError("reply text must not be empty.")
    finding = _find(state, finding_id)
    request = None
    if request_id is not None:
        request = _get_request(state, request_id)
        if request["status"] not in ("pending", "replied"):
            raise StateError(f"follow-up {request_id!r} is already {request['status']}.")
        if finding_id not in request["finding_ids"]:
            raise StateError(
                f"finding {finding_id!r} is not part of follow-up {request_id!r}."
            )
    finding["replies"].append(
        {"at": _utcnow(), "role": role, "stance": stance, "text": text}
    )
    finding["history"].append(
        {"at": _utcnow(), "event": f"reply:{stance}", "detail": f"{role} on {finding_id}"}
    )
    if request is not None and request["status"] == "pending":
        request["status"] = "replied"
        state["history"].append(
            {"at": _utcnow(), "event": "followup-replied", "detail": request_id}
        )
    if stance == "dispute" and finding["disposition"] != "unresolved":
        # A dispute stays visible: it reopens the finding instead of leaving a
        # closed verdict standing beside an unanswered objection.
        _set_disposition(
            state,
            finding_id,
            "unresolved",
            f"dispute by {role}: {text}",
        )


def set_fix(state: dict[str, Any], finding_id: str, status: str, detail: str = "") -> None:
    _require_locked(state, "record fix verification")
    _require_fresh(state, "record fix verification")
    if status not in FIX_STATES:
        raise StateError(f"unknown fix status: {status!r}.")
    if status == "verified" and _pending_requests_for(state, finding_id):
        raise StateError(
            f"finding {finding_id!r} has a pending follow-up request; a reply or "
            "an explicit unresolved gap is required before verification."
        )
    finding = _find(state, finding_id)
    finding["fix"] = {"status": status, "detail": detail}
    finding["history"].append(
        {"at": _utcnow(), "event": f"fix:{status}", "detail": detail}
    )


def first_packet(state: dict[str, Any], role: str) -> dict[str, Any]:
    """Blind first-pass dispatch packet: identity and scope only, never other results.

    Built during blind collection only. A stale record cannot launch new
    reviews; old records stay inspectable through load and read, not dispatch.
    """
    if state["phase"] != PHASE_INITIAL:
        raise StateError(
            "first-pass packets are built during blind collection only; "
            "this run is already locked or closed."
        )
    _require_fresh(state, "emit first-pass packets")
    _require_role(state, role)
    identity = copy.deepcopy(state["review_identity"])
    return {
        "run_id": state["run_id"],
        "role": role,
        "review_identity": identity,
        "scope": list(identity.get("scope", [])),
        "instruction": "Report findings from the frozen target only. No other reviewer output is included.",
    }


def followup_packet(state: dict[str, Any], request_id: str) -> dict[str, Any]:
    """Targeted reconciliation packet for one persisted follow-up request.

    Requires the initial phase to be locked and a fresh source. Unknown or
    silent roles stay uncovered; this packet never converts them to agreement.
    """
    _require_locked(state, "emit follow-up packets")
    _require_fresh(state, "emit follow-up packets")
    request = _get_request(state, request_id)
    if request["status"] not in ("pending", "replied"):
        raise StateError(f"follow-up {request_id!r} is already {request['status']}.")
    selected = [copy.deepcopy(_find(state, fid)) for fid in request["finding_ids"]]
    return {
        "run_id": state["run_id"],
        "request_id": request_id,
        "role": request["role"],
        "review_identity": copy.deepcopy(state["review_identity"]),
        "source": copy.deepcopy(request["source"]),
        "findings": selected,
        "instruction": (
            "Reply with agree, withdraw, or dispute for each listed finding. "
            "Silence is not agreement; unresolved dissent is recorded."
        ),
    }


def check_identity(
    state: dict[str, Any],
    workspace: str | None = None,
    base: str | None = None,
    head: str | None = None,
    review_revision: str | None = None,
    inputs: dict[str, str] | None = None,
    scope: list[str] | None = None,
) -> list[str]:
    """Compare a candidate source against the frozen identity. Returns mismatches."""
    identity = state["review_identity"]
    mismatches: list[str] = []
    for key, candidate in (
        ("workspace", workspace),
        ("base", base),
        ("head", head),
        ("review_revision", review_revision),
    ):
        if candidate is not None and candidate != identity.get(key):
            mismatches.append(f"{key}: frozen {identity.get(key)!r} != current {candidate!r}")
    if inputs is not None:
        frozen = {item["name"]: item["sha256"] for item in identity.get("inputs", [])}
        for name in sorted(set(frozen) | set(inputs)):
            if frozen.get(name) != inputs.get(name):
                mismatches.append(f"input {name!r} hash changed.")
    if scope is not None:
        if sorted(scope) != sorted(identity.get("scope", [])):
            mismatches.append(
                f"scope: frozen {sorted(identity.get('scope', []))!r} != current {sorted(scope)!r}"
            )
    return mismatches


def mark_findings_stale(state: dict[str, Any], reason: str) -> list[str]:
    """Mark every finding stale after a source change and freeze live decisions.

    All dispositions, including rejected and out-of-scope, become stale: a
    verdict bound to the old source must not read as current. Verified or
    fixed states reset to open so no old fix displays as currently valid.
    Evidence, stable ids, and history are preserved.
    """
    if not reason or not reason.strip():
        raise StateError("a reason is required to mark findings stale.")
    stale: list[str] = []
    for finding in state["findings"]:
        if not finding["stale"]:
            finding["stale"] = True
            finding["history"].append({"at": _utcnow(), "event": "stale", "detail": reason})
            stale.append(finding["id"])
        if finding["fix"]["status"] in ("fixed", "verified"):
            finding["fix"] = {"status": "open", "detail": f"reset: {reason}"}
            finding["history"].append(
                {"at": _utcnow(), "event": "fix:reset-stale", "detail": reason}
            )
    state["source_stale"] = True
    state["stale_reason"] = reason
    state["history"].append(
        {"at": _utcnow(), "event": "stale", "detail": f"{reason} ({len(stale)} findings)"}
    )
    return stale


def _load_for_cli(path: Path) -> dict[str, Any]:
    try:
        return load_state(path)
    except FileNotFoundError as error:
        raise StateError(f"no record at {path}; run init first.") from error


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Review-panel per-run state helper.")
    parser.add_argument("--state", required=True, help="Path to the run JSON record.")
    sub = parser.add_subparsers(dest="command", required=True)

    init = sub.add_parser("init", help="Create a fresh run record.")
    init.add_argument("--run-id", required=True)
    init.add_argument("--workspace", required=True)
    init.add_argument("--base", required=True)
    init.add_argument("--head", required=True)
    init.add_argument("--review-revision", default="")
    init.add_argument("--scope", default="")
    init.add_argument("--input", action="append", default=[], metavar="NAME=SHA256")
    init.add_argument(
        "--input-file", action="append", default=[], metavar="NAME=PATH",
        help="Hash a file's bytes and store them as a declared input.",
    )
    init.add_argument("--supersedes", default="", help="Prior run id this run replaces.")

    report = sub.add_parser("add-report", help="Store one role's initial report.")
    report.add_argument("--role", required=True)
    report.add_argument("--text", required=True)

    unknown = sub.add_parser("mark-unknown", help="Mark a role terminally unknown.")
    unknown.add_argument("--role", required=True)
    unknown.add_argument("--reason", required=True)

    reopen = sub.add_parser("reopen-role", help="Reopen an unknown role before lock.")
    reopen.add_argument("--role", required=True)
    reopen.add_argument("--reason", required=True)

    sub.add_parser("lock", help="Lock the initial phase.")
    sub.add_parser("close", help="Close a reconciling run with no pending follow-ups.")

    finding = sub.add_parser("add-finding", help="Add a finding with a stable id.")
    finding.add_argument("--origin-role", required=True)
    finding.add_argument("--title", required=True)
    finding.add_argument("--evidence", default="{}")

    disp = sub.add_parser("set-disposition", help="Record a finding disposition.")
    disp.add_argument("--id", required=True)
    disp.add_argument("--disposition", required=True, choices=DISPOSITIONS)
    disp.add_argument("--reason", default="")

    freq = sub.add_parser("request-followup", help="Persist a follow-up request.")
    freq.add_argument("--role", required=True)
    freq.add_argument("--id", action="append", default=[], dest="ids")
    freq.add_argument("--reason", default="")

    follow = sub.add_parser("followup-packet", help="Print a persisted follow-up packet.")
    follow.add_argument("--request", required=True)

    res = sub.add_parser("resolve-request", help="Leave a follow-up unresolved with reason.")
    res.add_argument("--request", required=True)
    res.add_argument("--outcome", required=True, choices=("unresolved",))
    res.add_argument("--reason", default="")

    reply = sub.add_parser("add-reply", help="Append a reviewer reply.")
    reply.add_argument("--id", required=True)
    reply.add_argument("--role", required=True)
    reply.add_argument("--stance", default="note", choices=STANCES)
    reply.add_argument("--text", required=True)
    reply.add_argument("--request", default=None)

    fix = sub.add_parser("set-fix", help="Record fix/verification status.")
    fix.add_argument("--id", required=True)
    fix.add_argument("--status", required=True, choices=FIX_STATES)
    fix.add_argument("--detail", default="")

    first = sub.add_parser("first-packet", help="Print the blind first-pass packet.")
    first.add_argument("--role", required=True)

    check = sub.add_parser("check-source", help="Compare a candidate source identity.")
    check.add_argument("--workspace", default=None)
    check.add_argument("--base", default=None)
    check.add_argument("--head", default=None)
    check.add_argument("--review-revision", default=None)
    check.add_argument("--scope", default=None, help="Comma-separated allowed paths.")
    check.add_argument("--input", action="append", default=[], metavar="NAME=SHA256")
    check.add_argument("--input-file", action="append", default=[], metavar="NAME=PATH")

    stale = sub.add_parser("mark-stale", help="Mark all findings stale after a source change.")
    stale.add_argument("--reason", required=True)
    return parser


def _parse_inputs(pairs: list[str]) -> dict[str, str]:
    inputs: dict[str, str] = {}
    for pair in pairs:
        name, sep, digest = pair.partition("=")
        if not sep or not name or not digest:
            raise StateError(f"bad --input value {pair!r}; expected NAME=SHA256.")
        _check_digest(name, digest)
        inputs[name] = digest
    return inputs


def _parse_input_files(pairs: list[str]) -> dict[str, str]:
    hashed: dict[str, str] = {}
    for pair in pairs:
        name, sep, path = pair.partition("=")
        if not sep or not name or not path:
            raise StateError(f"bad --input-file value {pair!r}; expected NAME=PATH.")
        try:
            hashed[name] = sha256_file(path)
        except OSError as error:
            raise StateError(f"cannot hash input file {path!r}: {error}.") from error
    return hashed


def _merge_inputs(explicit: dict[str, str], hashed: dict[str, str]) -> dict[str, str]:
    clash = set(explicit) & set(hashed)
    if clash:
        raise StateError(
            f"input names given both ways: {', '.join(sorted(clash))}. "
            "Use one --input or --input-file entry per name."
        )
    return {**hashed, **explicit}


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    path = Path(args.state)
    try:
        if args.command == "init":
            if path.exists():
                raise StateError(f"record already exists at {path}; refusing to overwrite.")
            inputs = _merge_inputs(
                _parse_inputs(args.input), _parse_input_files(args.input_file)
            )
            state = init_state(
                args.run_id,
                args.workspace,
                args.base,
                args.head,
                args.review_revision,
                inputs,
                [s for s in args.scope.split(",") if s] if args.scope else [],
                supersedes=args.supersedes,
            )
            save_state(path, state)
            print(json.dumps({"ok": True, "run_id": args.run_id}))
            return 0
        state = _load_for_cli(path)
        if args.command == "add-report":
            add_initial_report(state, args.role, args.text)
        elif args.command == "mark-unknown":
            mark_unknown(state, args.role, args.reason)
        elif args.command == "reopen-role":
            reopen_role(state, args.role, args.reason)
        elif args.command == "lock":
            lock_initial(state)
        elif args.command == "close":
            close_run(state)
        elif args.command == "add-finding":
            try:
                evidence = json.loads(args.evidence)
            except json.JSONDecodeError as error:
                raise StateError(f"bad --evidence JSON: {error}.") from error
            finding = add_finding(state, args.origin_role, args.title, evidence)
            print(json.dumps({"ok": True, "id": finding["id"]}))
            save_state(path, state)
            return 0
        elif args.command == "set-disposition":
            set_disposition(state, args.id, args.disposition, args.reason)
        elif args.command == "request-followup":
            request = request_followup(state, args.role, args.ids, args.reason)
            print(json.dumps({"ok": True, "request": request["id"]}))
            save_state(path, state)
            return 0
        elif args.command == "followup-packet":
            print(json.dumps(followup_packet(state, args.request), indent=2))
            return 0
        elif args.command == "resolve-request":
            resolve_request(state, args.request, args.outcome, args.reason)
        elif args.command == "add-reply":
            add_reply(state, args.id, args.role, args.text, args.stance, args.request)
        elif args.command == "set-fix":
            set_fix(state, args.id, args.status, args.detail)
        elif args.command == "first-packet":
            print(json.dumps(first_packet(state, args.role), indent=2))
            return 0
        elif args.command == "check-source":
            supplied = {
                "workspace": args.workspace,
                "base": args.base,
                "head": args.head,
                "review_revision": args.review_revision,
                "scope": [s for s in args.scope.split(",") if s]
                if args.scope is not None
                else None,
                "inputs": _merge_inputs(
                    _parse_inputs(args.input), _parse_input_files(args.input_file)
                )
                if (args.input or args.input_file)
                else None,
            }
            checked = sorted(
                name for name, value in supplied.items() if value is not None
            )
            if not checked:
                raise StateError(
                    "check-source needs at least one identity value "
                    "(--workspace/--base/--head/--review-revision/--scope/--input/--input-file); "
                    "a check with no inputs proves nothing."
                )
            mismatches = check_identity(state, **supplied)
            print(
                json.dumps(
                    {"ok": not mismatches, "mismatches": mismatches, "checked": checked},
                    indent=2,
                )
            )
            return 0 if not mismatches else 1
        elif args.command == "mark-stale":
            stale = mark_findings_stale(state, args.reason)
            print(json.dumps({"ok": True, "stale": stale}))
            save_state(path, state)
            return 0
        save_state(path, state)
        print(json.dumps({"ok": True}))
        return 0
    except StateError as error:
        print(json.dumps({"ok": False, "error": str(error)}), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
