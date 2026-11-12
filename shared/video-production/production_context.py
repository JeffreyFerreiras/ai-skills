#!/usr/bin/env python3
"""Validate declared production context and review provenance; never dispatch media work."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import math
import re
import stat
from pathlib import Path, PureWindowsPath


MAX_MANIFEST_BYTES = 256 * 1024
MAX_NODES = 256
MAX_DEPENDENCIES = 32
MAX_ARTIFACTS = 64
MAX_PACKET_BYTES = 64 * 1024
MAX_EVIDENCE = 128
FINAL_ROLES = {"final-consistency", "delivery", "final-encode"}
SCOPE = "Declared structure and artifact provenance; not semantic approval or host isolation"


class ContextError(ValueError):
    def __init__(self, code, locator, message):
        super().__init__(message)
        self.error = {"code": code, "locator": locator, "message": message}


def require(condition, code, locator, message):
    if not condition:
        raise ContextError(code, locator, message)


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"),
                      ensure_ascii=False, allow_nan=False).encode("utf-8")


def digest(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def text(value, locator):
    require(isinstance(value, str) and bool(value.strip()), "TYPE", locator, "Nonempty text required")
    return value


def integer(value, locator, minimum=0, maximum=None):
    require(type(value) is int and value >= minimum and (maximum is None or value <= maximum),
            "LIMIT", locator, f"Integer in range {minimum}..{maximum} required")
    return value


def objects(value, locator):
    require(isinstance(value, list), "TYPE", locator, "Object array required")
    for index, item in enumerate(value):
        require(isinstance(item, dict), "TYPE", f"{locator}[{index}]", "Object required")
    return value


def strings(value, locator, *, nonempty=True):
    require(isinstance(value, list) and (bool(value) or not nonempty), "TYPE", locator, "Array required")
    for item in value:
        text(item, locator)
    require(len(value) == len(set(value)), "DUPLICATE", locator, "Repeated identifier")
    return sorted(value)


def sha256(value, locator):
    require(isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None,
            "HASH", locator, "Lowercase SHA-256 required")
    return value


def schema(record):
    require(isinstance(record, dict) and type(record.get("schema_version")) is int
            and record["schema_version"] == 1, "SCHEMA", "schema_version", "Expected schema version 1")


def load_json(path, limit=MAX_MANIFEST_BYTES):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "DUPLICATE_KEY", key, "Duplicate JSON object key")
            result[key] = value
        return result

    with Path(path).open("rb") as stream:
        data = stream.read(limit + 1)
    require(len(data) <= limit, "MANIFEST_LIMIT", str(path), "JSON exceeds byte limit")
    return json.loads(data.decode("utf-8"), object_pairs_hook=unique,
                      parse_constant=lambda value: (_ for _ in ()).throw(
                          ContextError("TYPE", str(path), f"Nonfinite JSON value: {value}")))


def project_path(project, name):
    text(name, "path")
    relative = Path(name)
    windows = PureWindowsPath(name)
    require(not relative.is_absolute() and not windows.is_absolute() and not windows.drive
            and ".." not in windows.parts and ".." not in relative.parts
            and ".git" not in relative.parts and ".git" not in windows.parts,
            "PATH", name, "Expected a project-relative artifact without traversal")
    root = Path(project).resolve()
    path = root / relative
    for part in (path, *path.parents):
        if part == root:
            break
        try:
            info = part.lstat()
        except FileNotFoundError:
            continue
        require(not stat.S_ISLNK(info.st_mode) and not getattr(info, "st_file_attributes", 0) & 0x400,
                "LINK", name, "Linked/reparse artifacts are unsupported")
    require(path.resolve().is_relative_to(root), "PATH", name, "Artifact escapes project")
    return path


def artifact(project, reference):
    require(isinstance(reference, dict), "TYPE", "artifact", "Artifact reference required")
    require(set(reference) == {"path", "sha256"}, "TYPE", "artifact", "Only path and sha256 are accepted")
    expected = sha256(reference.get("sha256"), "artifact.sha256")
    path = project_path(project, reference.get("path"))
    require(path.is_file(), "MISSING_ARTIFACT", reference["path"], "Artifact is missing")
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(chunk)
    actual = result.hexdigest()
    require(actual == expected, "STALE_ARTIFACT", reference["path"], "Artifact bytes differ")
    return path


def references(project, values, maximum=MAX_EVIDENCE):
    require(isinstance(values, list) and len(values) <= maximum,
            "EVIDENCE_LIMIT", "evidence", "Evidence array exceeds limit")
    seen = set()
    for reference in values:
        artifact(project, reference)
        require(reference["path"] not in seen, "DUPLICATE_EVIDENCE", reference["path"], "Repeated evidence path")
        seen.add(reference["path"])
    return sorted(values, key=lambda item: item["path"])


def validate_context(record, project):
    """Check live declared bytes and return a detached, JSON-compatible validated context."""
    schema(record)
    require(set(record) == {"schema_version", "production_id", "stage_id", "generation", "nodes", "gates"},
            "TYPE", "context", "Only declared context fields are accepted")
    require(len(canonical(record)) <= MAX_MANIFEST_BYTES, "MANIFEST_LIMIT", "context", "Context exceeds byte limit")
    for key in ("production_id", "stage_id"):
        text(record.get(key), key)
    integer(record.get("generation"), "generation")
    nodes = record.get("nodes")
    require(isinstance(nodes, list) and 0 < len(nodes) <= MAX_NODES,
            "NODE_LIMIT", "nodes", "Context needs 1..256 nodes")
    by_id = {}
    for node in nodes:
        require(isinstance(node, dict), "TYPE", "node", "Node object required")
        require(set(node) == {"id", "kind", "scope_ids", "depends_on", "artifacts"},
                "TYPE", "node", "Only declared dependency fields are accepted")
        identifier = text(node.get("id"), "node.id")
        require(identifier not in by_id, "DUPLICATE", identifier, "Repeated node ID")
        text(node.get("kind"), identifier + ".kind")
        strings(node.get("scope_ids"), identifier + ".scope_ids")
        dependencies = strings(node.get("depends_on"), identifier + ".depends_on", nonempty=False)
        require(len(dependencies) <= MAX_DEPENDENCIES, "DEPENDENCY_LIMIT", identifier, "Too many dependencies")
        references(project, node.get("artifacts"), MAX_ARTIFACTS)
        by_id[identifier] = node
    fingerprints = {}
    visiting = set()

    def visit(identifier):
        require(identifier in by_id, "UNKNOWN_NODE", identifier, "Unknown dependency")
        require(identifier not in visiting, "CYCLE", identifier, "Dependency cycle")
        if identifier in fingerprints:
            return fingerprints[identifier]
        visiting.add(identifier)
        node = by_id[identifier]
        dependencies = {key: visit(key) for key in sorted(node["depends_on"])}
        fingerprints[identifier] = digest({"id": identifier, "kind": node["kind"],
            "scope_ids": sorted(node["scope_ids"]),
            "artifacts": sorted(node["artifacts"], key=lambda item: item["path"]),
            "depends_on": dependencies})
        visiting.remove(identifier)
        return fingerprints[identifier]

    for identifier in sorted(by_id):
        visit(identifier)
    gates = record.get("gates")
    require(isinstance(gates, list) and bool(gates), "TYPE", "gates", "Frozen gate definitions required")
    gate_ids = set()
    for gate in gates:
        require(isinstance(gate, dict) and set(gate) <= {"id", "prerequisite_nodes", "required", "check_ids",
                "consistency_coverage", "budget_checkpoint", "bounds"}, "TYPE", "gate", "Unknown gate fields")
        identifier = text(gate.get("id"), "gate.id")
        require(identifier not in gate_ids, "DUPLICATE", identifier, "Repeated gate ID")
        gate_ids.add(identifier)
        prerequisites = strings(gate.get("prerequisite_nodes"), identifier, nonempty=False)
        require(set(prerequisites) <= set(by_id), "UNKNOWN_NODE", identifier, "Unknown prerequisite")
        requirements = gate.get("required")
        require(isinstance(requirements, list) and bool(requirements), "TYPE", identifier, "Required roles missing")
        roles, reviewers = set(), set()
        scopes = {scope for node in nodes for scope in node["scope_ids"]}
        for requirement in requirements:
            require(isinstance(requirement, dict) and set(requirement) == {"role", "reviewer_id", "scope_ids", "criterion_ids"},
                    "TYPE", "required", "Only role assignment and coverage are accepted")
            role = text(requirement.get("role"), "role")
            require(":" not in role, "ROLE_NAMESPACE", "role", "Role IDs cannot contain the finding namespace separator ':'")
            reviewer = text(requirement.get("reviewer_id"), "reviewer_id")
            require(role not in roles and reviewer not in reviewers, "REVIEW_IDENTITY", identifier,
                    "Each role and reviewer must be distinct")
            roles.add(role)
            reviewers.add(reviewer)
            require(set(strings(requirement.get("scope_ids"), role)) <= scopes,
                    "SCOPE", role, "Unknown review scope")
            strings(requirement.get("criterion_ids"), role)
        strings(gate.get("check_ids"), identifier + ".check_ids")
        strings(gate.get("consistency_coverage"), identifier + ".consistency_coverage")
        bounds = gate.get("bounds", {})
        require(isinstance(bounds, dict) and set(bounds) <= {"packet_bytes", "evidence_refs", "readers", "writers"},
                "TYPE", "bounds", "Unknown inspection/resource bound")
        for key, default in (("packet_bytes", MAX_PACKET_BYTES), ("evidence_refs", MAX_EVIDENCE),
                             ("readers", 4), ("writers", 1)):
            integer(bounds.get(key, default), "bounds." + key, 1, default)
        artifact(project, gate.get("budget_checkpoint"))
    normalized_gates = sorted(gates, key=lambda gate: gate["id"])
    context_digest = digest({"production_id": record["production_id"], "stage_id": record["stage_id"],
                             "generation": record["generation"], "nodes": fingerprints, "gates": normalized_gates})
    return {"record": copy.deepcopy(record), "fingerprints": fingerprints,
            "contextDigest": context_digest, "project": str(Path(project).resolve())}


def refresh(context):
    current = validate_context(context["record"], context["project"])
    require(current["contextDigest"] == context["contextDigest"], "STALE_CONTEXT", "contextDigest", "Context changed")
    return current


def dependency_closure(context, scope_ids):
    nodes = {node["id"]: node for node in context["record"]["nodes"]}
    available = {scope for node in nodes.values() for scope in node["scope_ids"]}
    require(bool(scope_ids) and set(scope_ids) <= available, "SCOPE", "scope_ids", "Unknown or empty scope")
    found = set()

    def visit(identifier):
        if identifier not in found:
            found.add(identifier)
            for parent in nodes[identifier]["depends_on"]:
                visit(parent)

    for identifier, node in nodes.items():
        if set(node["scope_ids"]) & set(scope_ids):
            visit(identifier)
    return {identifier: context["fingerprints"][identifier] for identifier in sorted(found)}


def invalidated_nodes(previous, current):
    """Union edges retain invalidation even when a revision removes a dependency."""
    old, new = previous["fingerprints"], current["fingerprints"]
    stale = {key for key in old.keys() | new.keys() if old.get(key) != new.get(key)}
    descendants = {}
    for context in (previous, current):
        for node in context["record"]["nodes"]:
            for parent in node["depends_on"]:
                descendants.setdefault(parent, set()).add(node["id"])
    pending = list(stale)
    while pending:
        for child in descendants.get(pending.pop(), set()) - stale:
            stale.add(child)
            pending.append(child)
    return sorted(stale)


def gate_definition(context, identifier):
    matches = [gate for gate in context["record"]["gates"] if gate["id"] == identifier]
    require(len(matches) == 1, "GATE", identifier, "Unknown gate")
    return matches[0]


def requirement_for(definition, role):
    matches = [item for item in definition["required"] if item["role"] == role]
    require(len(matches) == 1, "ROLE", role, "Role not required by frozen gate")
    return matches[0]


def binding(context, gate):
    record = context["record"]
    return {"production_id": record["production_id"], "stage_id": record["stage_id"],
            "generation": record["generation"], "contextDigest": context["contextDigest"],
            "candidateDigest": sha256(gate.get("candidateDigest"), "candidateDigest"),
            "reservation_id": text(gate.get("reservation_id"), "reservation_id"), "gate_id": gate["id"]}


def bound(record, expected, locator):
    schema(record)
    require(all(record.get(key) == value for key, value in expected.items()),
            "CROSS_VERSION", locator, "Evidence binding differs from frozen candidate/context/reservation")


def check_budget(context, gate, budget, previous):
    """Compare supplied lineage; Supervisor alone selects the trusted latest checkpoint."""
    schema(budget)
    record = context["record"]
    for key in ("production_id", "stage_id"):
        require(budget.get(key) == record[key], "BUDGET_IDENTITY", key, "Budget identity differs")
    text(budget.get("lineage_id"), "lineage_id")
    sequence = integer(budget.get("sequence"), "sequence")
    spent = integer(budget.get("spent_rounds"), "spent_rounds", 1)
    policy_limit = integer(budget.get("policy_limit"), "policy_limit", 1, 3)
    allowance_limit = integer(budget.get("allowance_limit"), "allowance_limit", 1, 3)
    references(context["project"], budget.get("allowance_refs", []))
    require(spent <= min(policy_limit, allowance_limit), "BUDGET_EXHAUSTED", "spent_rounds", "Cumulative round cap exceeded")
    reservations = budget.get("reservations")
    require(isinstance(reservations, list) and len(reservations) == spent,
            "BUDGET_LINEAGE", "reservations", "Every spent round needs its reservation")
    indexed = {}
    for position, reservation in enumerate(reservations, 1):
        require(isinstance(reservation, dict), "TYPE", f"reservations[{position - 1}]", "Object required")
        identifier = text(reservation.get("id"), "reservation.id")
        require(identifier not in indexed and reservation.get("round") == position,
                "BUDGET_LINEAGE", identifier, "Unique ordered cumulative reservations required")
        sha256(reservation.get("candidateDigest"), "reservation.candidateDigest")
        sha256(reservation.get("contextDigest"), "reservation.contextDigest")
        indexed[identifier] = reservation
    active = indexed.get(gate["reservation_id"])
    require(active is not None and active == reservations[-1]
            and active["candidateDigest"] == gate["candidateDigest"]
            and active["contextDigest"] == context["contextDigest"],
            "BUDGET_BINDING", "reservation_id", "Current reservation differs")
    attempts = budget.get("attempts")
    objects(attempts, "attempts")
    require(all(item.get("status") in ("settled", "not-started") for item in attempts),
            "UNRESOLVED_ATTEMPT", "attempts", "Reconcile active, unknown or pending attempts first")
    attempts_by_id = {}
    for attempt in attempts:
        identifier = text(attempt.get("id"), "attempt.id")
        require(identifier not in attempts_by_id and attempt.get("reservation_id") in indexed,
                "BUDGET_LINEAGE", identifier, "Attempt needs unique ID and original reservation")
        attempts_by_id[identifier] = attempt
    require(previous is not None, "BUDGET_LINEAGE", "previous_budget", "Prior trusted checkpoint required")
    if previous is not None:
        schema(previous)
        for key in ("sequence", "spent_rounds", "policy_limit", "allowance_limit"):
            integer(previous.get(key), "previous_budget." + key)
        objects(previous.get("reservations"), "previous_budget.reservations")
        objects(previous.get("attempts", []), "previous_budget.attempts")
        require(all(budget.get(key) == previous.get(key) for key in ("production_id", "stage_id", "lineage_id"))
                and sequence > previous["sequence"] and spent >= previous["spent_rounds"]
                and reservations[:len(previous["reservations"])] == previous["reservations"]
                and policy_limit <= previous["policy_limit"] and allowance_limit <= previous["allowance_limit"]
                and budget.get("allowance_refs", []) == previous.get("allowance_refs", []),
                "BUDGET_LINEAGE", "previous_budget", "Checkpoint rollback, missing reservation or increased limit")
        for prior in previous.get("attempts", []):
            present = attempts_by_id.get(prior.get("id"))
            require(present is not None and present.get("reservation_id") == prior.get("reservation_id"),
                    "BUDGET_LINEAGE", "attempts", "Prior attempt must remain in its original reservation")
            if prior.get("status") not in ("settled", "not-started"):
                artifact(context["project"], present.get("reconciliation"))


def prerequisite_receipts(context, gate, definition):
    supplied = gate.get("prerequisites", [])
    require(isinstance(supplied, list), "PREREQUISITE", "prerequisites", "Prerequisite receipts required")
    indexed = {}
    for reference in supplied:
        path = artifact(context["project"], reference)
        receipt = load_json(path)
        schema(receipt)
        identifier = receipt.get("node_id")
        require(identifier not in indexed, "DUPLICATE_EVIDENCE", "prerequisites", "Repeated prerequisite receipt")
        require(receipt.get("production_id") == context["record"]["production_id"]
                and receipt.get("stage_id") == context["record"]["stage_id"]
                and receipt.get("generation") == context["record"]["generation"]
                and receipt.get("candidateDigest") == gate["candidateDigest"]
                and receipt.get("contextDigest") == context["contextDigest"]
                and receipt.get("fingerprint") == context["fingerprints"].get(identifier)
                and receipt.get("decision") == "APPROVE" and receipt.get("status") == "reviewed",
                "PREREQUISITE", str(identifier), "Current reviewed prerequisite required")
        indexed[identifier] = receipt
    require(set(indexed) == set(definition["prerequisite_nodes"]),
            "PREREQUISITE", "prerequisites", "Missing or extra prerequisite approval")


def encoded_audio_evidence(context, check):
    """Check linked encoded-audio records, without measuring or listening to media."""
    audio = check.get("encoded_audio")
    require(isinstance(audio, dict), "ENCODED_AUDIO", "encoded_audio", "Encoded AAC evidence required")
    encoded, wav = audio.get("encoded"), audio.get("approved_wav")
    artifact(context["project"], encoded)
    artifact(context["project"], wav)
    masters = [ref for node in context["record"]["nodes"] if node["kind"] == "encoded_master" for ref in node["artifacts"]]
    require(encoded in masters and encoded["path"] != wav["path"],
            "ENCODED_AUDIO", "encoded", "Exact declared encoded master, separate from approved WAV, required")
    settings = audio.get("settings")
    require(isinstance(settings, dict) and bool(settings), "ENCODED_AUDIO", "settings", "Encode settings required")
    records = {}
    for key in ("metadata", "diagnostics", "listening"):
        record = load_json(artifact(context["project"], audio.get(key)))
        schema(record)
        require(record.get("encoded_sha256") == encoded["sha256"], "STALE_ENCODED_QA", key, "Evidence belongs to another encode")
        records[key] = record
    require(records["metadata"].get("audio_codec") == "aac", "ENCODED_AUDIO", "metadata", "Encoded AAC metadata required")
    diagnostics = records["diagnostics"]
    require(diagnostics.get("input_wav_sha256") == wav["sha256"]
            and diagnostics.get("settingsDigest") == digest(settings)
            and all(type(diagnostics.get(key)) in (int, float) and math.isfinite(diagnostics[key])
                    for key in ("peak_dbfs", "true_peak_dbfs"))
            and type(diagnostics.get("clipped_samples")) is int and diagnostics["clipped_samples"] >= 0,
            "ENCODED_AUDIO", "diagnostics", "Bound decoded encoded-stream diagnostics required")
    listening = records["listening"]
    require(listening.get("coverage") == "full_encoded_audio" and listening.get("decision") == "APPROVE",
            "ENCODED_AUDIO", "listening", "Actual encoded-audio listening evidence required")
    text(listening.get("reviewer_id"), "listening.reviewer_id")


def build_role_packet(context, gate, role, phase="technical"):
    refresh(context)
    definition = gate_definition(context, gate["id"])
    requirement = requirement_for(definition, role)
    expected = binding(context, gate)
    prerequisite_receipts(context, gate, definition)
    require(phase in ("audience", "technical") and (phase != "audience" or role == "logical"),
            "PHASE", phase, "Only logical review has an audience phase")
    bounds = definition.get("bounds", {})
    evidence = references(context["project"], gate.get("validation_evidence", []), bounds.get("evidence_refs", MAX_EVIDENCE))
    checks = {}
    for reference in evidence:
        check = load_json(artifact(context["project"], reference))
        bound(check, expected, "check")
        identifier = check.get("check_id")
        require(identifier not in checks and check.get("status") == "passed",
                "CHECK", str(identifier), "Unique current passing validation required")
        if identifier == "encoded-audio":
            encoded_audio_evidence(context, check)
        checks[identifier] = check
    require(set(checks) == set(definition["check_ids"]), "CHECK", "validation_evidence", "All frozen checks required")
    packet = {"schema_version": 1, **expected, "role": role, "phase": phase,
              "reviewer_id": requirement["reviewer_id"]}
    if phase == "audience":
        # Audience fields are allowlisted; never include graph names, scopes, criteria or findings.
        audience = gate.get("audience")
        require(isinstance(audience, dict) and set(audience) == {"media", "intervals"},
                "PACKET_CONTAMINATION", "audience", "Only neutral media and intervals allowed")
        media = audience["media"]
        require(isinstance(media, list) and bool(media) and all(isinstance(ref, dict)
                and set(ref) == {"path", "sha256"} and isinstance(ref["path"], str)
                and re.fullmatch(r"audience/[0-9]+\.[a-z0-9]+", ref["path"]) for ref in media),
                "PACKET_CONTAMINATION", "audience.media", "Neutral audience/number.ext evidence required")
        media = references(context["project"], media, bounds.get("evidence_refs", MAX_EVIDENCE))
        intervals = audience["intervals"]
        require(isinstance(intervals, list) and bool(intervals), "TYPE", "intervals", "Viewing intervals required")
        for interval in intervals:
            require(isinstance(interval, list) and len(interval) == 2
                    and all(type(value) in (int, float) for value in interval)
                    and 0 <= interval[0] < interval[1], "TYPE", "intervals", "Increasing neutral intervals required")
        packet.update({"media": media, "intervals": intervals,
                       "questions": ["What situation is visible?", "What happens?", "What changes?", "What remains unclear?"]})
    else:
        if role == "logical":
            first_ref = gate.get("first_reading")
            first = load_json(artifact(context["project"], first_ref))
            bound(first, expected, "first_reading")
            audience_packet = load_json(artifact(context["project"], first.get("packet")), MAX_PACKET_BYTES)
            require(audience_packet == build_role_packet(context, gate, role, "audience"),
                    "BLIND_ORDER", "first_reading.packet", "Audience packet differs")
            require(first.get("reviewer_id") == requirement["reviewer_id"]
                    and first.get("input_scope") == "audience_evidence_only",
                    "BLIND_ORDER", "first_reading", "First reading reviewer/scope differs")
            text(first.get("observations"), "first_reading.observations")
            capture = load_json(artifact(context["project"], gate.get("host_capture")))
            bound(capture, expected, "host_capture")
            require(capture.get("packet") == first["packet"]
                    and capture.get("reviewer_id") == requirement["reviewer_id"]
                    and capture.get("available_inputs") == [first["packet"]]
                    and capture.get("first_reading_sha256") == first_ref["sha256"]
                    and type(capture.get("first_reading_sequence")) is int
                    and type(capture.get("intent_release_sequence")) is int
                    and 0 <= capture["first_reading_sequence"] < capture["intent_release_sequence"],
                    "ISOLATION_GAP", "host_capture", "Actual supplied packet and release-order evidence required")
            text(capture.get("fresh_session_id"), "fresh_session_id")
            packet["first_reading"] = first_ref
            packet["host_capture"] = gate["host_capture"]
        closure = dependency_closure(context, requirement["scope_ids"])
        selected = [node for node in context["record"]["nodes"] if node["id"] in closure]
        artifact_refs = [ref for node in selected for ref in node["artifacts"]]
        # Shared artifacts may appear on several dependency nodes, but supplied evidence is unique.
        combined = {ref["path"]: ref for ref in artifact_refs + evidence}
        require(len(combined) <= bounds.get("evidence_refs", MAX_EVIDENCE),
                "EVIDENCE_LIMIT", "packet", "Dependency closure exceeds evidence limit")
        packet.update({"scope_ids": sorted(requirement["scope_ids"]),
                       "criterion_ids": sorted(requirement["criterion_ids"]), "closure": closure,
                       "nodes": sorted(selected, key=lambda node: node["id"]),
                       "evidence": sorted(combined.values(), key=lambda ref: ref["path"]),
                       "prior_findings": gate.get("prior_findings", []), "bounds": bounds})
    require(len(canonical(packet)) <= bounds.get("packet_bytes", MAX_PACKET_BYTES),
            "PACKET_LIMIT", "packet", "Packet exceeds byte limit; bounded split required")
    return packet


def validate_carry_forward(previous, current, receipt):
    """Prove declared scoped sameness; never accept whole-final/delivery approval."""
    refresh(current)
    schema(receipt)
    require(receipt.get("status") == "carried_forward", "CARRY_PROOF", "status", "Explicit carry-forward status required")
    definition = gate_definition(current, receipt["gate_id"])
    old_definition = gate_definition(previous, receipt["gate_id"])
    role = receipt.get("role")
    require(role not in FINAL_ROLES, "CARRY_FINAL", "role", "Final acceptance requires fresh review")
    requirement = requirement_for(definition, role)
    old_requirement = requirement_for(old_definition, role)
    semantics = lambda gate: {key: value for key, value in gate.items() if key != "budget_checkpoint"}
    require(semantics(definition) == semantics(old_definition) and requirement == old_requirement,
            "CARRY_CONTRACT", "gate", "Frozen review contract changed")
    origin = load_json(artifact(current["project"], receipt.get("prior_receipt")))
    schema(origin)
    require(origin.get("status") == "reviewed" and origin.get("decision") == "APPROVE"
            and origin.get("role") == role and origin.get("reviewer_id") == requirement["reviewer_id"]
            and origin.get("production_id") == previous["record"]["production_id"]
            and origin.get("stage_id") == previous["record"]["stage_id"]
            and origin.get("generation") == previous["record"]["generation"]
            and origin.get("contextDigest") == previous["contextDigest"]
            and origin.get("gate_id") == receipt["gate_id"]
            and origin.get("candidateDigest") == receipt.get("old_candidateDigest")
            and origin.get("scope_ids") == sorted(requirement["scope_ids"])
            and origin.get("criterion_ids") == sorted(requirement["criterion_ids"])
            and origin.get("closure") == dependency_closure(previous, requirement["scope_ids"])
            and origin.get("findings") == [] and origin.get("unresolved_findings") == [],
            "CARRY_ORIGIN", "prior_receipt", "Valid old scoped approval with complete closure required")
    origin_packet = load_json(artifact(current["project"], origin.get("packet")), MAX_PACKET_BYTES)
    bound(origin_packet, {key: origin[key] for key in ("production_id", "stage_id", "generation", "contextDigest",
          "candidateDigest", "reservation_id", "gate_id")}, "origin_packet")
    require(origin_packet.get("role") == role and origin_packet.get("phase") == "technical"
            and origin_packet.get("reviewer_id") == origin["reviewer_id"] == old_requirement["reviewer_id"]
            and origin_packet.get("closure") == origin["closure"]
            and origin_packet.get("scope_ids") == origin["scope_ids"]
            and origin_packet.get("criterion_ids") == origin["criterion_ids"],
            "CARRY_ORIGIN", "origin_packet", "Origin packet coverage differs")
    expected = {"production_id": current["record"]["production_id"], "stage_id": current["record"]["stage_id"],
                "generation": current["record"]["generation"], "contextDigest": current["contextDigest"]}
    bound(receipt, expected, "carry_forward")
    sha256(receipt.get("candidateDigest"), "candidateDigest")
    require(receipt.get("old_contextDigest") == previous["contextDigest"]
            and receipt.get("scope_ids") == sorted(requirement["scope_ids"])
            and receipt.get("criterion_ids") == sorted(requirement["criterion_ids"])
            and receipt.get("reviewer_id") == origin["reviewer_id"]
            and receipt.get("old_closure") == origin["closure"]
            and receipt.get("new_closure") == dependency_closure(current, requirement["scope_ids"])
            and receipt["old_closure"] == receipt["new_closure"]
            and receipt.get("unresolved_findings") == [],
            "CARRY_PROOF", "closure", "Complete unchanged scope/dependency proof required")
    return True


def aggregate_reviews(context, gate, packets, results, budget, previous_budget=None):
    refresh(context)
    definition = gate_definition(context, gate["id"])
    expected = binding(context, gate)
    checkpoint = load_json(artifact(context["project"], definition["budget_checkpoint"]))
    require(checkpoint == previous_budget, "BUDGET_BINDING", "budget", "Frozen checkpoint must be the trusted prior state")
    check_budget(context, gate, budget, previous_budget)
    prerequisite_receipts(context, gate, definition)
    required = {item["role"]: item for item in definition["required"]}
    packet_by_role = {}
    for reference in references(context["project"], packets):
        packet = load_json(artifact(context["project"], reference), MAX_PACKET_BYTES)
        schema(packet)
        role = packet.get("role")
        require(role not in packet_by_role, "DUPLICATE_EVIDENCE", str(role), "Repeated role packet")
        require(role in required and packet == build_role_packet(context, gate, role),
                "PACKET_BINDING", str(role), "Packet differs from actual bounded technical packet")
        packet_by_role[role] = reference
    require(set(packet_by_role) == set(required), "MISSING_ROLE", "packets", "Every required role needs one packet")
    by_role, findings, dispositions = {}, [], {}
    for result in objects(results, "results"):
        bound(result, expected, "result")
        role = result.get("role")
        require(role in required and role not in by_role, "DUPLICATE_RESULT", str(role), "Unexpected or repeated role result")
        requirement = required[role]
        require(result.get("reviewer_id") == requirement["reviewer_id"]
                and result.get("packet") == packet_by_role[role]
                and result.get("scope_ids") == sorted(requirement["scope_ids"])
                and result.get("criterion_ids") == sorted(requirement["criterion_ids"])
                and result.get("closure") == dependency_closure(context, requirement["scope_ids"]),
                "REVIEW_BINDING", role, "Reviewer, packet or declared coverage differs")
        require(result.get("decision") in ("APPROVE", "REVISE", "BLOCK")
                and result.get("status") in ("reviewed", "carried_forward"), "DECISION", role, "Terminal result required")
        if result["status"] == "carried_forward":
            previous = gate.get("previous_context")
            require(isinstance(previous, dict), "CARRY_PROOF", role, "Prior validated context required")
            validate_carry_forward(previous, context, result)
        references(context["project"], result.get("evidence", []))
        entries = result.get("findings")
        local_ids = set()
        for finding in objects(entries, role + ".findings"):
            identifier = text(finding.get("id"), "finding.id")
            require(":" not in identifier and identifier not in local_ids,
                    "DUPLICATE_FINDING", identifier, "Unique local finding ID required")
            local_ids.add(identifier)
            require(finding.get("criterion_id") in requirement["criterion_ids"]
                    and finding.get("scope_id") in requirement["scope_ids"], "SCOPE", identifier, "Finding outside assigned coverage")
            for key in ("locator", "issue", "effect", "repair"):
                text(finding.get(key), "finding." + key)
            findings.append({**finding, "id": role + ":" + identifier, "role": role})
        current_dispositions = result.get("dispositions", {})
        require(isinstance(current_dispositions, dict), "TYPE", role, "Disposition map required")
        for identifier, disposition in current_dispositions.items():
            require(identifier.partition(":")[0] == role and bool(identifier.partition(":")[2])
                    and disposition in ("resolved", "still-open", "reopened", "blocked"),
                    "DISPOSITION", identifier, "Role-owned explicit disposition required")
            dispositions[identifier] = disposition
        by_role[role] = result
    require(set(by_role) == set(required), "MISSING_ROLE", "results", "Every required role needs one terminal result")
    prior = strings(gate.get("prior_findings", []), "prior_findings", nonempty=False)
    unresolved = sorted(set(finding["id"] for finding in findings)
                        | {identifier for identifier in prior if dispositions.get(identifier) != "resolved"})
    decisions = {result["decision"] for result in results}
    decision = "BLOCK" if "BLOCK" in decisions else ("REVISE" if "REVISE" in decisions or unresolved else "APPROVE")
    # Preserve repair findings while consistency is pending or negative. Only fresh
    # complete positive evidence can advance an otherwise approving round.
    final = gate.get("final_consistency")
    consistency = None
    if final is not None:
        consistency = load_json(artifact(context["project"], final))
        bound(consistency, expected, "final_consistency")
        require((consistency.get("status") == "reviewed" and consistency.get("decision") in ("APPROVE", "REVISE", "BLOCK"))
                or (consistency.get("status") == "pending" and consistency.get("decision") is None),
                "FINAL_CONSISTENCY", "final_consistency", "Reviewed decision or pending consistency required")
        references(context["project"], consistency.get("evidence", []))
    approved = (consistency is not None and consistency.get("status") == "reviewed"
                and consistency.get("decision") == "APPROVE" and consistency.get("fresh") is True
                and consistency.get("live_candidateDigest") == gate["candidateDigest"]
                and consistency.get("coverage") == definition.get("consistency_coverage")
                and bool(consistency.get("coverage")))
    require(decision != "APPROVE" or approved, "FINAL_CONSISTENCY", "final_consistency",
            "Fresh final cross-scope/live candidate checks required before approval")
    if consistency is not None and consistency.get("decision") == "BLOCK":
        decision = "BLOCK"
    if decision == "REVISE" and budget["spent_rounds"] >= min(budget["policy_limit"], budget["allowance_limit"]):
        decision = "BLOCK"
    findings.sort(key=lambda item: (item["criterion_id"], item["scope_id"], item["role"], item["id"], item["locator"]))
    return {"schema_version": 1, **expected, "decision": decision, "findings": findings,
            "dispositions": dict(sorted(dispositions.items())), "unresolved_findings": unresolved,
            "roles": sorted(by_role), "spent_rounds": budget["spent_rounds"], "scope": SCOPE}


def write_new(project, name, record):
    path = project_path(project, name)
    data = canonical(record) + b"\n"
    # One exclusive write: an interrupted write is an unknown result for Supervisor reconciliation.
    with path.open("xb") as stream:
        stream.write(data)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    for name in ("check", "packet", "aggregate"):
        command = subparsers.add_parser(name)
        command.add_argument("context", type=Path)
        command.add_argument("--project", type=Path, required=True)
        if name != "check":
            command.add_argument("--gate", required=True)
            command.add_argument("--evidence", type=Path, required=True, help="Explicit runtime evidence index")
            command.add_argument("--output", required=True, help="New project-relative output path")
        if name == "packet":
            command.add_argument("--role", required=True)
            command.add_argument("--phase", choices=("audience", "technical"), default="technical")
    args = parser.parse_args()
    try:
        context = validate_context(load_json(args.context), args.project)
        result = {"valid": True, "errors": [], "contextDigest": context["contextDigest"], "scope": SCOPE}
        if args.command != "check":
            index = load_json(args.evidence)
            schema(index)
            gate = index.get("gate")
            require(isinstance(gate, dict), "TYPE", "gate", "Runtime gate object required")
            require(gate.get("id") == args.gate, "GATE", args.gate, "Evidence index gate differs")
            if args.command == "packet":
                result = build_role_packet(context, gate, args.role, args.phase)
            else:
                # The index names exact immutable JSON artifacts, never a directory/glob.
                results = [load_json(artifact(args.project, ref)) for ref in references(args.project, index["results"])]
                budget = load_json(artifact(args.project, index["budget"]))
                previous = load_json(artifact(args.project, index["previous_budget"])) if index.get("previous_budget") else None
                result = aggregate_reviews(context, gate, index["packets"], results, budget, previous)
            write_new(args.project, args.output, result)
    except ContextError as error:
        result = {"valid": False, "errors": [error.error], "scope": SCOPE}
    except (OSError, ValueError, KeyError, TypeError, OverflowError, RecursionError) as error:
        result = {"valid": False, "errors": [{"code": "INPUT", "locator": "input", "message": str(error)}], "scope": SCOPE}
    print(json.dumps(result, sort_keys=True, ensure_ascii=False, allow_nan=False))
    return 1 if result.get("valid") is False or result.get("decision") in ("BLOCK", "REVISE") else 0


if __name__ == "__main__":
    raise SystemExit(main())
