"""Version-one human design-risk contracts, independent of storage and delivery."""

from .contracts import ContractError, bounded_string, digest, require_keys, validate_ref
from .ids import canonical_bytes, sha256_bytes


ASSURANCE = "local_attestation_only; same-user coherent forgery is not authenticated"
PREPARATION = "design_risk_preparation"
RECEIPT = "human_design_risk_acceptance"
ATTESTATION = "design_risk_attestation"
RESOLUTION = "effective_design_resolution"
KINDS = {PREPARATION, RECEIPT, ATTESTATION, RESOLUTION}
SCOPE_KEYS = {
    "run_id", "repository_identity", "task_ref", "task_digest", "plan_digest",
    "plan_approval_digest", "policy_digest", "prepared_revision", "design_generation",
    "implementation_generation", "design_result_ref", "design_artifact_ref",
    "original_review_source_binding_ref", "source_provenance", "review_collection_ref",
    "consolidation_join_id", "consolidation_result_ref", "review_results", "accepted_findings",
    "blocked_event", "blocked_reason", "source_digest", "head_oid",
    "continuation_envelope_digest", "budget_and_attempts_digest",
}


def hashed(value):
    return sha256_bytes(canonical_bytes(value))


def exact_keys(value, keys, field="design_risk"):
    if not isinstance(value, dict):
        raise ContractError(field, "DESIGN_RISK_BINDING_MISMATCH")
    require_keys(value, keys, keys, field)


def scope_digest(scope):
    unsigned = dict(scope)
    unsigned.pop("preparation_ref", None)
    return hashed(unsigned)


def validate_receipt(value):
    exact_keys(value, {"schema_version", "kind", "decision", "reason", "scope", "scope_digest", "human_provenance"})
    if (type(value["schema_version"]) is not int or value["schema_version"] != 1
            or value["kind"] != RECEIPT or value["decision"] != "ACCEPT_EXISTING_DESIGN_RISK"):
        raise ContractError("receipt", "DESIGN_RISK_AUTHORITY_MISMATCH")
    bounded_string(value["reason"], "reason", 2048)
    scope = value["scope"]
    exact_keys(scope, SCOPE_KEYS | {"preparation_ref"}, "scope")
    for key in ("prepared_revision", "design_generation", "implementation_generation"):
        if type(scope[key]) is not int or scope[key] < 0:
            raise ContractError("scope", "DESIGN_RISK_BINDING_MISMATCH")
    validate_ref(scope["preparation_ref"], "preparation_ref", content_required=True)
    digest(value["scope_digest"], "scope_digest")
    if scope_digest(scope) != value["scope_digest"]:
        raise ContractError("scope", "DESIGN_RISK_BINDING_MISMATCH")
    provenance = value["human_provenance"]
    exact_keys(provenance, {"authority_ref", "actor", "human_evidence_ref", "human_evidence_sha256",
                            "accepted_scope_digest", "asserted_role"}, "human_provenance")
    if provenance["asserted_role"] != "human" or provenance["accepted_scope_digest"] != value["scope_digest"]:
        raise ContractError("human_provenance", "DESIGN_RISK_AUTHORITY_MISMATCH")
    validate_ref(provenance["human_evidence_ref"], "human_evidence_ref", content_required=True)
    digest(provenance["human_evidence_sha256"], "human_evidence_sha256")
    return value


def validate_human_evidence(value, receipt, authority, actor):
    """Validate an explicit sanitized statement; this is not identity authentication."""
    exact_keys(value, {"schema_version", "kind", "origin", "authority_ref", "actor", "run_id",
                       "preparation_ref", "scope_digest", "accepted_findings", "source_provenance",
                       "decision", "statement", "provenance_ref"}, "human_evidence")
    scope = receipt["scope"]
    provenance = receipt["human_provenance"]
    if (type(value["schema_version"]) is not int or value["schema_version"] != 1
            or value["kind"] != "human_design_risk_authorization" or value["origin"] != "direct_human"
            or value["authority_ref"] != authority or value["actor"] != actor
            or provenance["authority_ref"] != authority or provenance["actor"] != actor
            or value["decision"] != receipt["decision"]
            or value["scope_digest"] != receipt["scope_digest"]
            or any(value[key] != scope[key] for key in
                   ("run_id", "preparation_ref", "accepted_findings", "source_provenance"))):
        raise ContractError("human_evidence", "DESIGN_RISK_AUTHORITY_MISMATCH")
    bounded_string(value["statement"], "statement", 4096)
    bounded_string(value["provenance_ref"], "provenance_ref", 1024)
    # The exact digest and assurance mode must be visible in the human's statement.
    if receipt["scope_digest"] not in value["statement"] or scope["source_provenance"] not in value["statement"]:
        raise ContractError("human_evidence", "DESIGN_RISK_AUTHORITY_MISMATCH")


def review_projection(members):
    results, accepted = [], []
    seen = {}
    for member in sorted(members, key=lambda item: item["branch_id"]):
        result = member["result"]
        if (member["status"] != "succeeded" or member["failure_code"] is not None
                or result.get("failure_code") is not None or result.get("decision") not in {"APPROVE", "REVISE"}):
            raise ContractError("reviews", "DESIGN_RISK_ENFORCEMENT_BOUNDARY")
        findings = result.get("findings", [])
        results.append({"branch_id": member["branch_id"], "attempt_id": result["attempt_id"],
                        "result_ref": member["result_ref"], "decision": result["decision"],
                        "findings_digest": hashed(findings)})
        for finding in findings:
            exact_keys(finding, set(finding) & {"finding_id", "disposition", "scope"}, "finding")
            if (not finding.get("finding_id") or finding.get("scope", "in_scope") not in {"in_scope", "out_of_scope"}
                    or finding.get("disposition") not in {"approve", "revise"}):
                raise ContractError("finding", "DESIGN_RISK_ENFORCEMENT_BOUNDARY")
            identifier = finding["finding_id"]
            if identifier in seen and seen[identifier] != finding:
                raise ContractError("finding", "DESIGN_RISK_INELIGIBLE")
            seen[identifier] = finding
            if finding["disposition"] == "revise":
                if finding.get("scope", "in_scope") != "in_scope" or result["decision"] != "REVISE":
                    raise ContractError("finding", "DESIGN_RISK_INELIGIBLE")
                accepted.append({"branch_id": member["branch_id"], "finding_id": identifier,
                                 "finding_digest": hashed(finding)})
    if not accepted:
        raise ContractError("reviews", "DESIGN_RISK_INELIGIBLE")
    return results, accepted
