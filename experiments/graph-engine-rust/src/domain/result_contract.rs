use super::contract_values::{
    contains, fail, get, in_strings, list, object, unique_strings, validate_attempt,
};
use super::contracts::{bounded_string, digest, opaque, require_keys, validate_ref};
use super::errors::{
    iterable_set, require_hashable, set_contains, string_item, ValidationFailure, ValidationResult,
};
use crate::compat::{PyString, Value};
use std::collections::BTreeSet;

pub const ALL_FINDING_DISPOSITIONS: &[&str] =
    &["approve", "revise", "block", "accept", "repair", "redesign"];
pub const DESIGN_FINDING_DISPOSITIONS: &[&str] = &["approve", "revise", "block"];
pub const DELIVERY_FINDING_DISPOSITIONS: &[&str] = &["accept", "repair", "redesign", "block"];

pub fn is_actionable_finding(finding: &Value) -> ValidationResult<bool> {
    Ok(!get(finding, "scope")?.is_text("out_of_scope"))
}

pub fn phase_finding_dispositions(stage: &Value, historical: bool) -> &'static [&'static str] {
    if historical {
        ALL_FINDING_DISPOSITIONS
    } else if stage.is_text("design") {
        DESIGN_FINDING_DISPOSITIONS
    } else if stage.is_text("delivery") {
        DELIVERY_FINDING_DISPOSITIONS
    } else {
        ALL_FINDING_DISPOSITIONS
    }
}

pub(super) fn finding_id(value: &Value) -> ValidationResult<PyString> {
    let Some(text) = value.as_string() else {
        let message = if matches!(value, Value::Bytes(_)) {
            "cannot use a string pattern on a bytes-like object".to_string()
        } else {
            format!(
                "expected string or bytes-like object, got '{}'",
                value.python_type()
            )
        };
        return Err(ValidationFailure::python("TypeError", message));
    };
    let dash = text.0.iter().position(|&point| point == 45);
    let valid = dash.is_some_and(|dash| {
        dash > 0
            && matches!(text.0[0], 65..=90)
            && text.0[..dash]
                .iter()
                .all(|point| matches!(point, 65..=90|48..=57))
            && text.len() >= dash + 4
            && text.0[dash + 1..]
                .iter()
                .all(|point| matches!(point, 48..=57))
    });
    if !valid {
        return Err(fail("finding_id", "INVALID_ID"));
    }
    Ok(text.clone())
}

fn validate_consolidation(value: &Value, branch: &Value) -> ValidationResult<Value> {
    object(value, "consolidation", "INVALID_OBJECT")?;
    const REQUIRED: &[&str] = &[
        "schema_version",
        "kind",
        "run_id",
        "join_id",
        "generation",
        "source_branch_ids",
        "finding_dispositions",
        "outcome",
        "attempt_id",
        "claim_digest",
    ];
    let mut allowed = REQUIRED.to_vec();
    allowed.push("delegated_finding_dispositions");
    require_keys(value, REQUIRED, &allowed, "consolidation")?;
    validate_attempt(value, "consolidation")?;
    if !value["schema_version"].py_eq(&1i64.into())
        || !value["run_id"].py_eq(string_item(branch, "run_id")?)
    {
        return Err(fail("consolidation", "RUN_OR_SCHEMA_MISMATCH"));
    }
    let design = string_item(branch, "node_key")?.is_text("supervisor_design_consolidation");
    let kind = if design {
        "design_consolidation"
    } else {
        "delivery_consolidation"
    };
    if !value["kind"].is_text(kind)
        || !value["generation"].py_eq(string_item(branch, "generation")?)
    {
        return Err(fail("consolidation", "KIND_OR_GENERATION_MISMATCH"));
    }
    opaque(&value["join_id"], "join_id")?;
    unique_strings(&value["source_branch_ids"], "source_branch_ids", None)?;
    let mut seen = BTreeSet::new();
    for item in list(
        &value["finding_dispositions"],
        "finding_dispositions",
        "INVALID_LIST",
    )? {
        object(item, "finding_dispositions", "INVALID_OBJECT")?;
        require_keys(
            item,
            &["finding_id", "disposition"],
            &["finding_id", "disposition"],
            "finding_dispositions",
        )?;
        let id = finding_id(&item["finding_id"])?;
        if !in_strings(&item["disposition"], ALL_FINDING_DISPOSITIONS)? {
            return Err(fail("disposition", "UNKNOWN_VALUE"));
        }
        if !seen.insert(id) {
            return Err(fail("finding_id", "DUPLICATE_ID"));
        }
    }
    let empty = Value::List(vec![]);
    let delegated = value
        .get("delegated_finding_dispositions")
        .unwrap_or(&empty);
    let mut delegated_seen = BTreeSet::new();
    for item in list(delegated, "delegated_finding_dispositions", "INVALID_LIST")? {
        object(item, "delegated_finding_dispositions", "INVALID_OBJECT")?;
        const FIELDS: &[&str] = &["issue_key", "source_identities", "disposition"];
        require_keys(item, FIELDS, FIELDS, "delegated_finding_dispositions")?;
        let issue = bounded_string(&item["issue_key"], "issue_key", 4096)?.clone();
        if delegated_seen.contains(&issue)
            || !in_strings(
                &item["disposition"],
                &["accept", "repair", "redesign", "block"],
            )?
        {
            return Err(fail(
                "delegated_finding_dispositions",
                "DUPLICATE_OR_INVALID_DISPOSITION",
            ));
        }
        delegated_seen.insert(issue);
        let identities = list(
            &item["source_identities"],
            "source_identities",
            "INVALID_LIST",
        )?;
        if identities.is_empty() {
            return Err(fail("source_identities", "INVALID_LIST"));
        }
        let mut normalized = Vec::new();
        for identity in identities {
            let parts = list(identity, "source_identities", "INVALID_LIST")?;
            if parts.len() != 3 {
                return Err(fail("source_identities", "INVALID_LIST"));
            }
            normalized.push(
                parts
                    .iter()
                    .map(|part| opaque(part, "source_identity"))
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        if normalized.iter().collect::<BTreeSet<_>>().len() != normalized.len()
            || !normalized.windows(2).all(|pair| pair[0] <= pair[1])
        {
            return Err(fail("source_identities", "DUPLICATE_OR_NON_CANONICAL"));
        }
    }
    let outcomes: &[&str] = if design {
        &["APPROVE", "REVISE", "BLOCK"]
    } else {
        &["ACCEPT", "REPAIR", "REDESIGN", "BLOCK"]
    };
    if !in_strings(&value["outcome"], outcomes)? {
        return Err(fail("outcome", "UNKNOWN_VALUE"));
    }
    let mut normalized = value.clone();
    normalized.set("branch_id", string_item(branch, "branch_id")?.clone());
    normalized.set("status", "succeeded".into());
    normalized.set("output_kind", kind.into());
    normalized.set("evidence", Value::List(vec![]));
    normalized.set("decision", value["outcome"].clone());
    Ok(normalized)
}

pub fn validate_result_manifest(
    value: &Value,
    branch: &Value,
    historical: bool,
) -> ValidationResult<Value> {
    let node_key = string_item(branch, "node_key")?;
    if node_key.is_text("impact_mapper")
        && value.as_object().is_some()
        && value.get("status").is_none()
    {
        return Ok(value.clone());
    }
    if in_strings(
        node_key,
        &[
            "supervisor_design_consolidation",
            "supervisor_delivery_consolidation",
        ],
    )? {
        return validate_consolidation(value, branch);
    }
    object(value, "result", "INVALID_OBJECT")?;
    const REQUIRED: &[&str] = &[
        "schema_version",
        "run_id",
        "branch_id",
        "status",
        "output_kind",
        "evidence",
        "attempt_id",
        "claim_digest",
    ];
    const ALLOWED: &[&str] = &[
        "schema_version",
        "run_id",
        "branch_id",
        "status",
        "output_kind",
        "artifact_ref",
        "evidence",
        "decision",
        "findings",
        "failure_code",
        "kind",
        "join_id",
        "generation",
        "source_branch_ids",
        "finding_dispositions",
        "delegated_finding_dispositions",
        "outcome",
        "attempt_id",
        "claim_digest",
        "review_request_slots",
        "child_members",
        "terminal_non_successes",
        "finding_sources",
    ];
    require_keys(value, REQUIRED, ALLOWED, "result")?;
    validate_attempt(value, "result")?;
    if !value["schema_version"].py_eq(&1i64.into())
        || !value["run_id"].py_eq(string_item(branch, "run_id")?)
        || !value["branch_id"].py_eq(string_item(branch, "branch_id")?)
    {
        return Err(fail("result", "BRANCH_MISMATCH"));
    }
    if !in_strings(&value["status"], &["succeeded", "failed"])? {
        return Err(fail("status", "INVALID_RESULT_STATUS"));
    }
    let output = string_item(branch, "output_contract")?;
    if !value["output_kind"].py_eq(string_item(output, "artifact_kind")?) {
        return Err(fail("output_kind", "OUTPUT_CONTRACT_MISMATCH"));
    }
    let research = in_strings(
        node_key,
        &["design_research_architecture", "design_research_validation"],
    )?;
    if research {
        for field in [
            "artifact_required",
            "evidence_required",
            "decision_forbidden",
            "findings_forbidden",
        ] {
            if !get(output, field)?.is_true() {
                return Err(fail("output_contract", "RESEARCH_CONTRACT_INVALID"));
            }
        }
    }
    let evidence = list(&value["evidence"], "evidence", "INVALID_LIST")?;
    let mut evidence_keys = BTreeSet::new();
    for item in evidence {
        object(item, "evidence", "INVALID_OBJECT")?;
        require_keys(
            item,
            &["kind", "ref", "sha256"],
            &["kind", "ref", "sha256"],
            "evidence",
        )?;
        let kind = bounded_string(&item["kind"], "evidence.kind", 64)?.clone();
        let reference = validate_ref(&item["ref"], "evidence.ref", true)?;
        let hash = digest(&item["sha256"], "evidence.sha256")?;
        evidence_keys.insert((kind, reference, hash));
    }
    if evidence_keys.len() != evidence.len() {
        return Err(fail("evidence", "DUPLICATE_VALUE"));
    }
    if research && value["status"].is_text("succeeded") && evidence.is_empty() {
        return Err(fail("evidence", "EVIDENCE_REQUIRED"));
    }
    let depth_one = get(branch, "depth")?.py_eq(&1i64.into());
    if value["status"].is_text("failed") {
        opaque(&value["failure_code"], "failure_code")?;
        if evidence.is_empty() && !depth_one {
            return Err(fail("evidence", "EVIDENCE_REQUIRED"));
        }
    } else {
        let artifact = &value["artifact_ref"];
        if depth_one && !artifact.is_null() {
            return Err(fail("artifact_ref", "DELEGATION_ARTIFACT_FORBIDDEN"));
        }
        let redesign =
            node_key.is_text("senior_engineer") && value["decision"].is_text("REDESIGN_REQUIRED");
        if redesign && !artifact.is_null() {
            return Err(fail("artifact_ref", "REDESIGN_ARTIFACT_FORBIDDEN"));
        }
        let empty = Value::List(vec![]);
        let required_decisions = iterable_set(
            output
                .get("artifact_required_for_decisions")
                .unwrap_or(&empty),
        )?;
        let default_required = Value::Bool(true);
        let required = output.get("artifact_required").unwrap_or(&default_required);
        let artifact_required = if required.truthy() {
            true
        } else {
            require_hashable(&value["decision"])?;
            set_contains(&required_decisions, &value["decision"])
        };
        if artifact_required && !redesign {
            object(artifact, "artifact_ref", "ARTIFACT_REQUIRED")?;
            require_keys(
                artifact,
                &["kind", "ref", "sha256"],
                &["kind", "ref", "sha256"],
                "artifact_ref",
            )?;
            if !artifact["kind"].py_eq(&value["output_kind"]) {
                return Err(fail("artifact_ref.kind", "OUTPUT_CONTRACT_MISMATCH"));
            }
            validate_ref(&artifact["ref"], "artifact_ref.ref", true)?;
            digest(&artifact["sha256"], "artifact_ref.sha256")?;
        }
        let decisions = output.get("decision_values").unwrap_or(&empty);
        if decisions.truthy() && !contains(decisions, &value["decision"])? {
            return Err(fail("decision", "DECISION_CONTRACT_MISMATCH"));
        }
        let evidence_decisions = iterable_set(
            output
                .get("evidence_required_for_decisions")
                .unwrap_or(&empty),
        )?;
        require_hashable(&value["decision"])?;
        if set_contains(&evidence_decisions, &value["decision"]) && evidence.is_empty() {
            return Err(fail("evidence", "REDESIGN_EVIDENCE_REQUIRED"));
        }
    }
    let empty = Value::List(vec![]);
    let findings = value.get("findings").unwrap_or(&empty);
    if research && !value["decision"].is_null() {
        return Err(fail("decision", "DECISION_FORBIDDEN"));
    }
    if research && findings.truthy() {
        return Err(fail("findings", "FINDINGS_FORBIDDEN"));
    }
    let findings = list(findings, "findings", "INVALID_LIST")?;
    if depth_one {
        return Ok(value.clone());
    }
    let stage = get(branch, "stage")?;
    let allowed_dispositions = phase_finding_dispositions(stage, historical);
    let mut ids = BTreeSet::new();
    let mut actionable = BTreeSet::new();
    for finding in findings {
        object(finding, "findings", "INVALID_OBJECT")?;
        require_keys(
            finding,
            &["finding_id", "disposition"],
            &["finding_id", "disposition", "scope"],
            "findings",
        )?;
        let id = finding_id(&finding["finding_id"])?;
        let default_scope = Value::from("in_scope");
        let scope = finding.get("scope").unwrap_or(&default_scope);
        if !in_strings(scope, &["in_scope", "out_of_scope"])? {
            return Err(fail("scope", "UNKNOWN_VALUE"));
        }
        let disposition = &finding["disposition"];
        if !in_strings(disposition, ALL_FINDING_DISPOSITIONS)? {
            return Err(fail("disposition", "UNKNOWN_VALUE"));
        }
        if scope.is_text("out_of_scope") {
            if !disposition.is_text("approve") {
                return Err(fail("findings", "OUT_OF_SCOPE_ACTIONABLE"));
            }
        } else if !in_strings(disposition, allowed_dispositions)? {
            return Err(fail("disposition", "PHASE_DISPOSITION_MISMATCH"));
        }
        if !ids.insert(id) {
            return Err(fail("finding_id", "DUPLICATE_ID"));
        }
        if is_actionable_finding(finding)? {
            actionable.insert(disposition.text().unwrap());
        }
    }
    let decision = &value["decision"];
    if stage.is_text("design") && !in_strings(node_key, &["tech_lead"])? {
        decision_findings(decision, &actionable, "approve", &["revise"])?;
    } else if stage.is_text("delivery") {
        decision_findings(decision, &actionable, "accept", &["repair", "redesign"])?;
    } else if node_key.is_text("senior_engineer") {
        if decision.is_text("IMPLEMENTED") && actionable.iter().any(|item| item != "accept") {
            return Err(fail("decision", "DECISION_FINDING_MISMATCH"));
        }
        if decision.is_text("REDESIGN_REQUIRED")
            && actionable != BTreeSet::from(["redesign".to_string()])
        {
            return Err(fail("decision", "REDESIGN_FINDING_REQUIRED"));
        }
    } else if !findings.is_empty() {
        return Err(fail("findings", "FINDINGS_NOT_ALLOWED"));
    }
    Ok(value.clone())
}

fn decision_findings(
    decision: &Value,
    actionable: &BTreeSet<String>,
    approved: &str,
    revisions: &[&str],
) -> ValidationResult<()> {
    if decision.is_text("APPROVE") && actionable.iter().any(|item| item != approved) {
        return Err(fail("decision", "DECISION_FINDING_MISMATCH"));
    }
    if decision.is_text("REVISE") && !revisions.iter().any(|item| actionable.contains(*item)) {
        return Err(fail("decision", "REVISION_FINDING_REQUIRED"));
    }
    if decision.is_text("BLOCK") && !actionable.contains("block") {
        return Err(fail("decision", "BLOCK_FINDING_REQUIRED"));
    }
    Ok(())
}
