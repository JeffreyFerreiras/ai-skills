"""Bounded future-task constraint selection over verified, plain input records."""

from .contracts import ContractError, bounded_string, lexical_relative, opaque, require_keys, validate_ref


def _contains(root, path):
    # Serialized paths use forward slashes; platform case handling belongs to the caller.
    return root.rstrip("/") == path.rstrip("/") or (root.endswith("/") and path.startswith(root))


def validate_bundle(value):
    if not isinstance(value, dict):
        raise ContractError("planning_constraints", "INVALID_OBJECT")
    require_keys(value, {"kind", "schema_version", "records"}, {"kind", "schema_version", "records"}, "planning_constraints")
    if value["kind"] != "planning_constraints" or value["schema_version"] != 1 or not isinstance(value["records"], list) or len(value["records"]) > 32:
        raise ContractError("planning_constraints", "SCHEMA_MISMATCH")
    fields = {"id", "revision", "statement", "rationale", "state", "repository_id", "paths", "roles", "acceptance_ids",
              "finding_id", "confirmed_finding_ref", "accepted_fix_ref", "authority_ref", "invalidation_reason", "supersedes"}
    identities = set()
    for record in value["records"]:
        if not isinstance(record, dict):
            raise ContractError("constraint", "INVALID_OBJECT")
        require_keys(record, fields, fields, "constraint")
        opaque(record["id"], "constraint.id")
        revision = record["revision"]
        if isinstance(revision, bool) or not isinstance(revision, int) or not 1 <= revision <= 1000000:
            raise ContractError("constraint.revision", "INVALID_VALUE")
        identity = record["id"] + ":" + str(revision)
        if identity in identities:
            raise ContractError("constraint", "DUPLICATE_ID")
        identities.add(identity)
        for field in ("statement", "rationale"):
            bounded_string(record[field], "constraint." + field, 4096)
        opaque(record["repository_id"], "repository_id")
        opaque(record["finding_id"], "finding_id")
        if record["state"] not in {"candidate", "active", "invalidated", "superseded"}:
            raise ContractError("constraint.state", "INVALID_VALUE")
        if not isinstance(record["paths"], list) or not 1 <= len(record["paths"]) <= 16:
            raise ContractError("constraint.paths", "INVALID_LIST")
        for path in record["paths"]:
            if lexical_relative(path) != path or any(character in path for character in "*?[]{}"):
                raise ContractError("constraint.paths", "INVALID_PATH")
        for field in ("roles", "acceptance_ids"):
            if not isinstance(record[field], list) or not 1 <= len(record[field]) <= 32 or len(set(record[field])) != len(record[field]):
                raise ContractError("constraint." + field, "INVALID_LIST")
            for item in record[field]:
                opaque(item, "constraint." + field)
        for field in ("confirmed_finding_ref", "accepted_fix_ref"):
            if record[field] is not None:
                validate_ref(record[field], field, content_required=True)
        validate_ref(record["authority_ref"], "authority_ref")
        if record["invalidation_reason"] is not None:
            bounded_string(record["invalidation_reason"], "invalidation_reason", 1024)
        if record["state"] == "invalidated" and not record["invalidation_reason"]:
            raise ContractError("invalidation_reason", "MISSING_FIELD")
        if not isinstance(record["supersedes"], list) or len(record["supersedes"]) > 32:
            raise ContractError("supersedes", "INVALID_LIST")
        for predecessor in record["supersedes"]:
            bounded_string(predecessor, "supersedes", 160)
    graph = {record["id"] + ":" + str(record["revision"]): record["supersedes"] for record in value["records"]}
    visited = set()
    def visit(identity, ancestors):
        if identity in ancestors:
            raise ContractError("supersedes", "SUPERSESSION_CYCLE")
        if identity in visited:
            return
        for predecessor in graph.get(identity, []):
            visit(predecessor, ancestors | {identity})
        visited.add(identity)
    for identity in graph:
        visit(identity, set())
    return value


def select_constraints(bundle, context, *, activated=False, supported=(), authority_refs=(), normalize=lambda path: path):
    """Select applicability, never grant capabilities or infer semantic truth."""
    validate_bundle(bundle)
    require_keys(context, {"repository_id", "acceptance_ids", "role", "scope_paths"},
                 {"repository_id", "acceptance_ids", "role", "scope_paths"}, "constraint_context")
    opaque(context["repository_id"], "constraint_context.repository_id")
    opaque(context["role"], "constraint_context.role")
    for field in ("acceptance_ids", "scope_paths"):
        if not isinstance(context[field], list) or len(context[field]) > 128:
            raise ContractError("constraint_context." + field, "INVALID_LIST")
        for item in context[field]:
            if field == "acceptance_ids":
                opaque(item, "constraint_context.acceptance_ids")
            elif lexical_relative(item) != item:
                raise ContractError("constraint_context.scope_paths", "INVALID_PATH")
    superseded = {identity for record in bundle["records"] for identity in record["supersedes"]}
    selected, candidates, excluded = [], [], []
    for record in bundle["records"]:
        identity = record["id"] + ":" + str(record["revision"])
        reason = None
        if record["state"] in {"invalidated", "superseded"} or identity in superseded:
            reason = record["state"] if record["state"] != "active" else "superseded"
        elif record["repository_id"] != context["repository_id"] or context["role"] not in record["roles"]:
            reason = "not_applicable"
        elif not set(record["acceptance_ids"]).issubset(context["acceptance_ids"]):
            reason = "acceptance_mismatch"
        elif not context["scope_paths"]:
            reason = "unresolved_scope"
        elif not any(_contains(normalize(root), normalize(path)) or _contains(normalize(path), normalize(root))
                     for root in record["paths"] for path in context["scope_paths"]):
            reason = "path_mismatch"
        elif any(other["id"] == record["id"] and other["revision"] != record["revision"]
                 and other["id"] + ":" + str(other["revision"]) not in superseded
                 and other["state"] in {"active", "candidate"} for other in bundle["records"]):
            reason = "conflicting_revisions"
        elif record["confirmed_finding_ref"] not in supported or record["accepted_fix_ref"] not in supported:
            reason = "missing_support"
        elif activated and record["authority_ref"] not in authority_refs:
            reason = "authority_mismatch"
        if reason:
            excluded.append({"id": identity, "reason": reason})
        elif record["state"] == "candidate" or not activated:
            candidates.append(record)
        else:
            selected.append(record)
    return {"active": sorted(selected, key=lambda item: (item["id"], item["revision"])),
            "candidates": sorted(candidates, key=lambda item: (item["id"], item["revision"])),
            "excluded": sorted(excluded, key=lambda item: item["id"])}
