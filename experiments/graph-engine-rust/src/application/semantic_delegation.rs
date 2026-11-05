use super::{
    error::{EngineError, EngineResult},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
};
use crate::{
    compat::{self, Value},
    domain::{
        contract_values::{get, in_strings},
        errors::{self, string_item},
        fanout_contract::validate_fanout_assessment,
        ids, planner, python_operations,
        review_collection::freeze_terminal_member,
        review_contracts::validate_preliminary,
        review_findings::consolidate_findings,
        review_identity::{delegated_identity, request_slot_id},
        review_request::{validate_fanout_request, FanoutRequestContext},
    },
};

fn invalid(code: &str) -> EngineError {
    EngineError::state(code)
}

fn keyed(items: &Value) -> EngineResult<Vec<(Value, Value)>> {
    let mut result: Vec<(Value, Value)> = Vec::new();
    for value in errors::iterable_values(items)? {
        let key = string_item(&value, "assignment_id")?.clone();
        errors::require_hashable(&key)?;
        if let Some((_, previous)) = result
            .iter_mut()
            .find(|(candidate, _)| candidate.container_eq(&key))
        {
            *previous = value;
        } else {
            result.push((key, value));
        }
    }
    Ok(result)
}

fn assignment<'a>(
    assignments: &'a [(Value, Value)],
    key: &Value,
) -> EngineResult<Option<&'a Value>> {
    errors::require_hashable(key)?;
    Ok(assignments
        .iter()
        .find(|(candidate, _)| candidate.container_eq(key))
        .map(|(_, value)| value))
}

struct Context<'a> {
    ledger: &'a dyn GraphLedgerReadPort,
    run: &'a Value,
    plan_row: &'a Value,
    assignments: &'a [(Value, Value)],
    limits: &'a Value,
    case_sensitive: bool,
}

fn validate_rounds(requests: &[Value], limits: &Value) -> EngineResult<()> {
    let mut groups: Vec<((Value, Value), Vec<&Value>)> = Vec::new();
    for request in requests {
        let parent = string_item(request, "parent_branch_id")?;
        let generation = string_item(request, "generation")?;
        errors::require_hashable(parent)?;
        errors::require_hashable(generation)?;
        if let Some((_, group)) = groups
            .iter_mut()
            .find(|((a, b), _)| a.container_eq(parent) && b.container_eq(generation))
        {
            group.push(request);
        } else {
            groups.push(((parent.clone(), generation.clone()), vec![request]));
        }
    }
    for (_, group) in groups {
        let rounds = group
            .iter()
            .map(|request| string_item(request, "round_number").cloned())
            .collect::<Result<Vec<_>, _>>()?;
        let expected = Value::list((1..=group.len()).map(Value::from));
        if !Value::List(rounds).py_eq(&expected)
            || python_operations::greater(
                &Value::from(group.len()),
                string_item(limits, "max_request_rounds")?,
            )?
        {
            return Err(invalid("DELEGATION_ROUND_LIMIT"));
        }
        if group[..group.len() - 1]
            .iter()
            .any(|request| !request["status"].is_text("sealed"))
        {
            return Err(invalid("DELEGATION_ROUND_NOT_SETTLED"));
        }
    }
    Ok(())
}

fn validate_parent(context: &Context<'_>, request: &Value) -> EngineResult<(Value, Value, Value)> {
    let run_id = string_item(context.run, "run_id")?;
    let expected = request_slot_id([
        run_id,
        string_item(context.run, "policy_digest")?,
        string_item(context.plan_row, "plan_digest")?,
        string_item(request, "parent_branch_id")?,
        string_item(request, "parent_attempt_id")?,
        string_item(request, "parent_claim_digest")?,
        string_item(request, "generation")?,
        string_item(request, "round_number")?,
    ])?;
    if !string_item(request, "request_slot_id")?.is_text(&expected)
        || !in_strings(
            string_item(request, "status")?,
            &["awaiting_assessment", "running", "sealed"],
        )?
    {
        return Err(invalid("DELEGATION_STATE_INVALID"));
    }
    let parent = context.ledger.first(
        GraphRead::Node,
        &[string_item(request, "parent_branch_id")?.clone()],
    )?;
    let attempt = context.ledger.first(
        GraphRead::ReviewParentAttempt,
        &[
            string_item(request, "parent_branch_id")?.clone(),
            string_item(request, "parent_attempt_id")?.clone(),
        ],
    )?;
    let Some(parent) = parent else {
        return Err(invalid("DELEGATION_PARENT_STATE_INVALID"));
    };
    if !string_item(&parent, "role")?.is_text("code_reviewer")
        || !string_item(&parent, "depth")?.py_eq(&0_i64.into())
        || in_strings(
            string_item(&parent, "node_key")?,
            &["code_reviewer_naming", "code_reviewer_bug_hunter"],
        )?
        || attempt.is_none()
        || !string_item(attempt.as_ref().unwrap(), "claim_digest")?
            .py_eq(string_item(request, "parent_claim_digest")?)
        || !string_item(attempt.as_ref().unwrap(), "outcome")?.is_text("delegated")
    {
        return Err(invalid("DELEGATION_PARENT_STATE_INVALID"));
    }
    let preliminary = context.ledger.first(
        GraphRead::Artifact,
        &[string_item(request, "preliminary_ref")?.clone()],
    )?;
    let artifact = context.ledger.first(
        GraphRead::Artifact,
        &[string_item(request, "request_ref")?.clone()],
    )?;
    if preliminary.is_none()
        || !string_item(preliminary.as_ref().unwrap(), "kind")?.is_text("delivery_review")
        || artifact.is_none()
        || !matches!(string_item(request,"request_digest")?,Value::String(text) if text.len()==64)
        || !matches!(string_item(request,"preliminary_digest")?,Value::String(text) if text.len()==64)
        || !string_item(artifact.as_ref().unwrap(), "kind")?.is_text("evidence_manifest")
    {
        return Err(invalid("DELEGATION_ARTIFACT_INVALID"));
    }
    let contents = (|| -> EngineResult<(Value, Value)> {
        let envelope = compat::loads_value(string_item(&parent, "envelope_json")?)?;
        let preliminary = validate_preliminary(
            &compat::loads_value(string_item(preliminary.as_ref().unwrap(), "content_json")?)?,
            run_id,
            string_item(request, "parent_branch_id")?,
            string_item(request, "parent_attempt_id")?,
            string_item(request, "generation")?,
            Some(string_item(&envelope, "inputs")?),
            true,
        )?;
        let assignments = Value::list(context.assignments.iter().map(|(_, value)| value.clone()));
        let contents = validate_fanout_request(
            &compat::loads_value(string_item(artifact.as_ref().unwrap(), "content_json")?)?,
            &FanoutRequestContext {
                run_id,
                parent_branch_id: string_item(request, "parent_branch_id")?,
                parent_attempt_id: string_item(request, "parent_attempt_id")?,
                round_number: string_item(request, "round_number")?,
                plan_assignments: &assignments,
                preliminary: &preliminary,
                limits: context.limits,
                depth: &0_i64.into(),
                historical: true,
            },
        )?;
        Ok((preliminary, contents))
    })();
    match contents {
        Err(EngineError::Contract(_)) => Err(invalid("DELEGATION_ARTIFACT_INVALID")),
        Err(EngineError::Python {
            kind: "TypeError" | "ValueError",
            ..
        }) => Err(invalid("DELEGATION_ARTIFACT_INVALID")),
        Err(EngineError::Json(error))
            if matches!(error.kind, "TypeError" | "ValueError" | "JSONDecodeError") =>
        {
            Err(invalid("DELEGATION_ARTIFACT_INVALID"))
        }
        other => {
            let (preliminary, contents) = other?;
            Ok((parent, preliminary, contents))
        }
    }
}

fn validate_authority(context: &Context<'_>, request: &Value) -> EngineResult<()> {
    let mut operation_request = Value::object([("command", "record.review-fanout".into())]);
    for (key, source) in [
        ("branch_id", "parent_branch_id"),
        ("attempt_id", "parent_attempt_id"),
        ("claim_digest", "parent_claim_digest"),
        ("preliminary_digest", "preliminary_digest"),
        ("request_digest", "request_digest"),
        ("authority_ref", "authority_ref"),
    ] {
        operation_request.set(key, string_item(request, source)?.clone());
    }
    let digest = ids::sha256_bytes(&ids::canonical_bytes(&operation_request)?);
    let mut matching = 0;
    context.ledger.visit(
        GraphRead::DelegationOperations,
        &[string_item(context.run, "run_id")?.clone(), digest.into()],
        &mut |operation| {
            let response = compat::loads_value(string_item(&operation, "response_json")?)?;
            if get(&response, "request_slot_id")?.py_eq(string_item(request, "request_slot_id")?) {
                matching += 1;
            }
            Ok(())
        },
    )?;
    if matching != 1 {
        return Err(invalid("DELEGATION_AUTHORITY_INVALID"));
    }
    Ok(())
}

fn validate_members(
    context: &Context<'_>,
    request: &Value,
    contents: &Value,
) -> EngineResult<(Vec<Value>, Value)> {
    let members = context.ledger.all(
        GraphRead::ReviewMembersForValidation,
        &[string_item(request, "request_slot_id")?.clone()],
    )?;
    let mut tuples = Vec::new();
    for member in &members {
        tuples.push(Value::list([
            string_item(member, "child_branch_id")?.clone(),
            string_item(member, "assignment_id")?.clone(),
            string_item(member, "ordinal")?.clone(),
        ]));
    }
    let mut expected = Vec::new();
    for member in errors::iterable_values(string_item(contents, "members")?)? {
        expected.push(Value::list([
            delegated_identity(
                string_item(request, "request_slot_id")?,
                string_item(&member, "assignment_id")?,
                string_item(&member, "ordinal")?,
                &"branch".into(),
            )?
            .into(),
            string_item(&member, "assignment_id")?.clone(),
            string_item(&member, "ordinal")?.clone(),
        ]));
    }
    let tuples = Value::List(tuples);
    if !tuples.py_eq(&Value::List(expected))
        || !tuples.py_eq(&compat::loads_value(string_item(
            request,
            "child_member_tuples_json",
        )?)?)
        || members.is_empty()
        || python_operations::greater(
            &Value::from(members.len()),
            string_item(context.limits, "max_children_per_request")?,
        )?
    {
        return Err(invalid("DELEGATION_MEMBER_INVALID"));
    }
    let mut cost = Value::from(0_i64);
    for member in &members {
        let assignment = assignment(context.assignments, string_item(member, "assignment_id")?)?;
        let child = context.ledger.first(
            GraphRead::Node,
            &[string_item(member, "child_branch_id")?.clone()],
        )?;
        if assignment.is_none() || child.is_none() {
            return Err(invalid("DELEGATION_MEMBER_INVALID"));
        }
        let child = child.unwrap();
        if !string_item(&child, "request_slot_id")?.py_eq(string_item(request, "request_slot_id")?)
            || !string_item(&child, "assignment_id")?.py_eq(string_item(member, "assignment_id")?)
            || !string_item(&child, "ordinal")?.py_eq(string_item(member, "ordinal")?)
            || !string_item(&child, "parent_branch_id")?
                .py_eq(string_item(request, "parent_branch_id")?)
            || !string_item(&child, "depth")?.py_eq(&1_i64.into())
        {
            return Err(invalid("DELEGATION_MEMBER_INVALID"));
        }
        cost = python_operations::add(&cost, string_item(assignment.unwrap(), "dispatch_weight")?)?;
    }
    if !cost.py_eq(string_item(request, "dispatch_cost")?) {
        return Err(invalid("DELEGATION_COST_INVALID"));
    }
    Ok((members, cost))
}

fn validate_assessment(
    context: &Context<'_>,
    request: &Value,
    fanout: &Value,
    members: &[Value],
) -> EngineResult<()> {
    let artifact = context.ledger.first(
        GraphRead::Artifact,
        &[string_item(fanout, "assessment_ref")?.clone()],
    )?;
    let Some(artifact) = artifact else {
        return Err(invalid("DELEGATION_ASSESSMENT_INVALID"));
    };
    if !string_item(&artifact, "kind")?.is_text("evidence_manifest")
        || !string_item(&artifact, "sha256")?.py_eq(string_item(fanout, "assessment_digest")?)
        || !string_item(&artifact, "content_json")?.truthy()
    {
        return Err(invalid("DELEGATION_ASSESSMENT_INVALID"));
    }
    let content = compat::loads_value(string_item(&artifact, "content_json")?)?;
    let keys = errors::iterable_set(&content)?;
    let expected = [
        "schema_version",
        "kind",
        "run_id",
        "fanout_id",
        "members",
        "dependencies",
        "evidence",
        "authority_ref",
        "actor",
        "host_identity",
        "assessed_at",
    ];
    if keys.len() != expected.len()
        || expected
            .iter()
            .any(|key| !errors::set_contains(&keys, &Value::from(*key)))
    {
        return Err(invalid("DELEGATION_ASSESSMENT_INVALID"));
    }
    if !get(&content, "authority_ref")?.py_eq(string_item(fanout, "authority_ref")?)
        || !get(&content, "actor")?.py_eq(string_item(fanout, "actor")?)
        || !get(&content, "host_identity")?.py_eq(string_item(fanout, "host_identity")?)
        || !string_item(fanout, "host_identity")?.py_eq(string_item(context.run, "host_identity")?)
        || !get(&content, "assessed_at")?.py_eq(string_item(fanout, "assessed_at")?)
    {
        return Err(invalid("DELEGATION_ASSESSMENT_INVALID"));
    }
    let empty = Value::List(Vec::new());
    get(&content, "members")?;
    let mut actual = errors::iterable_values(content.get("members").unwrap_or(&empty))?
        .iter()
        .map(|member| Ok(get(member, "branch_id")?.clone()))
        .collect::<EngineResult<Vec<_>>>()?;
    python_operations::sort_comparable_values(&mut actual)?;
    let mut expected = members
        .iter()
        .map(|member| string_item(member, "child_branch_id").cloned())
        .collect::<Result<Vec<_>, _>>()?;
    python_operations::sort_comparable_values(&mut expected)?;
    if !Value::List(actual).py_eq(&Value::List(expected.clone())) {
        return Err(invalid("DELEGATION_ASSESSMENT_INVALID"));
    }
    let mut base = Value::object([]);
    for key in [
        "schema_version",
        "kind",
        "run_id",
        "fanout_id",
        "members",
        "dependencies",
        "evidence",
    ] {
        base.set(key, string_item(&content, key)?.clone());
    }
    let normalized = (|| -> EngineResult<(Value, Vec<Value>)> {
        let mut normalized = validate_fanout_assessment(
            &base,
            string_item(context.run, "run_id")?,
            string_item(request, "request_slot_id")?,
            &members
                .iter()
                .map(|member| string_item(member, "child_branch_id").cloned())
                .collect::<Result<Vec<_>, _>>()?,
        )?;
        let dependencies = planner::validate_fanout_ordering(
            normalized["members"].as_list().unwrap(),
            normalized["dependencies"].as_list().unwrap(),
            context.case_sensitive,
        )
        .map_err(|_| invalid("DELEGATION_ASSESSMENT_INVALID"))?;
        normalized.set("dependencies", Value::List(dependencies.clone()));
        Ok((normalized, dependencies))
    })();
    let (normalized, dependencies) = match normalized {
        Err(EngineError::Contract(_)) => return Err(invalid("DELEGATION_ASSESSMENT_INVALID")),
        Err(EngineError::Python {
            kind: "ValueError", ..
        }) => return Err(invalid("DELEGATION_ASSESSMENT_INVALID")),
        other => other?,
    };
    if !base.py_eq(&normalized) {
        return Err(invalid("DELEGATION_ASSESSMENT_INVALID"));
    }
    for evidence in errors::iterable_values(string_item(&normalized, "evidence")?)? {
        let artifact = context.ledger.first(
            GraphRead::ArtifactKindDigest,
            &[string_item(&evidence, "ref")?.clone()],
        )?;
        if artifact.is_none()
            || !string_item(artifact.as_ref().unwrap(), "kind")?
                .py_eq(string_item(&evidence, "kind")?)
            || !string_item(artifact.as_ref().unwrap(), "sha256")?
                .py_eq(string_item(&evidence, "sha256")?)
        {
            return Err(invalid("DELEGATION_ASSESSMENT_INVALID"));
        }
    }
    for member in errors::iterable_values(string_item(&base, "members")?)? {
        for key in [
            "writable_paths",
            "mutable_state_refs",
            "exclusive_device_refs",
        ] {
            if string_item(string_item(&member, "resources")?, key)?.truthy() {
                return Err(invalid("DELEGATION_READ_ONLY_REQUIRED"));
            }
        }
    }
    let stored = context.ledger.all(
        GraphRead::ReviewDependencies,
        &[string_item(request, "request_slot_id")?.clone()],
    )?;
    if !Value::List(dependencies).py_eq(&Value::List(stored)) {
        return Err(invalid("DELEGATION_ASSESSMENT_INVALID"));
    }
    for member in members {
        let child = context
            .ledger
            .first(
                GraphRead::Node,
                &[string_item(member, "child_branch_id")?.clone()],
            )?
            .unwrap_or(Value::Null);
        let predecessors = context.ledger.all(
            GraphRead::ReviewPredecessors,
            &[
                string_item(request, "request_slot_id")?.clone(),
                string_item(member, "child_branch_id")?.clone(),
            ],
        )?;
        let mut eligible = true;
        for predecessor in predecessors {
            let status = string_item(&predecessor, "status")?;
            if !(in_strings(status, &["succeeded", "skipped"])?
                || (in_strings(status, &["failed", "timed_out"])?
                    && python_operations::greater_equal(
                        string_item(&predecessor, "retry_count")?,
                        string_item(&predecessor, "max_retries")?,
                    )?))
            {
                eligible = false;
                break;
            }
        }
        if (eligible && string_item(&child, "status")?.is_text("pending"))
            || (!eligible && !string_item(&child, "status")?.is_text("pending"))
        {
            return Err(invalid("DELEGATION_DEPENDENCY_STATE_INVALID"));
        }
    }
    Ok(())
}

fn validate_collection(
    context: &Context<'_>,
    request: &Value,
    parent: &Value,
    preliminary: &Value,
    collection: &Value,
    members: &[Value],
) -> EngineResult<()> {
    if !string_item(collection, "status")?.is_text("sealed")
        || !string_item(collection, "collection_digest")?.truthy()
        || !string_item(collection, "content_json")?.truthy()
    {
        return Err(invalid("DELEGATION_COLLECTION_INVALID"));
    }
    let content = compat::loads_value(string_item(collection, "content_json")?)?;
    if !get(&content, "request_slot_id")?.py_eq(string_item(request, "request_slot_id")?)
        || !string_item(collection, "collection_digest")?
            .is_text(&ids::sha256_bytes(&ids::canonical_bytes(&content)?))
    {
        return Err(invalid("DELEGATION_COLLECTION_INVALID"));
    }
    let mut sources = Vec::new();
    if string_item(preliminary, "findings")?.truthy() {
        let envelope = compat::loads_value(string_item(parent, "envelope_json")?)?;
        sources.push(Value::object([
            (
                "request_slot_id",
                string_item(request, "request_slot_id")?.clone(),
            ),
            ("branch_id", string_item(parent, "branch_id")?.clone()),
            ("role", "code_reviewer".into()),
            ("model", string_item(&envelope, "model")?.clone()),
            ("assignment_id", "primary-reviewer".into()),
            ("ordinal", 0_i64.into()),
            ("review_lens", "preliminary".into()),
            ("findings", string_item(preliminary, "findings")?.clone()),
        ]));
    }
    let mut frozen = Vec::new();
    for member in members {
        let mut child = context
            .ledger
            .first(
                GraphRead::Node,
                &[string_item(member, "child_branch_id")?.clone()],
            )?
            .unwrap_or(Value::Null);
        let envelope = compat::loads_value(string_item(&child, "envelope_json")?)?;
        let raw = string_item(&child, "result_json")?;
        let result = if raw.truthy() {
            compat::loads_value(raw)?
        } else {
            Value::object([])
        };
        let findings = if string_item(&child, "status")?.is_text("succeeded") {
            get(&result, "findings")?;
            result
                .get("findings")
                .cloned()
                .unwrap_or_else(|| Value::List(Vec::new()))
        } else {
            Value::List(Vec::new())
        };
        sources.push(Value::object([
            (
                "request_slot_id",
                string_item(request, "request_slot_id")?.clone(),
            ),
            ("branch_id", string_item(&child, "branch_id")?.clone()),
            ("role", string_item(&child, "role")?.clone()),
            ("model", string_item(&envelope, "model")?.clone()),
            (
                "assignment_id",
                string_item(member, "assignment_id")?.clone(),
            ),
            ("ordinal", string_item(member, "ordinal")?.clone()),
            (
                "review_lens",
                string_item(string_item(&envelope, "review_assignment")?, "review_lens")?.clone(),
            ),
            ("findings", findings),
        ]));
        let branch = compat::repr::value_str(string_item(&child, "branch_id")?)?;
        let digest = compat::repr::value_str(string_item(&child, "result_digest")?)?;
        let mut reference = compat::PyString::from("ledger:");
        reference.0.extend(branch.0);
        reference.0.extend(compat::PyString::from("#sha256=").0);
        reference.0.extend(digest.0);
        let artifact = context.ledger.first(
            GraphRead::DelegatedResultArtifact,
            &[Value::String(reference)],
        )?;
        let attempts = context.ledger.all(
            GraphRead::DelegatedMemberAttempts,
            &[string_item(&child, "branch_id")?.clone()],
        )?;
        let artifact = artifact.ok_or_else(|| invalid("DELEGATION_COLLECTION_INVALID"))?;
        child.set(
            "assignment_id",
            string_item(member, "assignment_id")?.clone(),
        );
        child.set("ordinal", string_item(member, "ordinal")?.clone());
        frozen.push(freeze_terminal_member(
            &child,
            &envelope,
            &result,
            &artifact,
            &Value::List(attempts),
        )?);
    }
    let issues = consolidate_findings(&Value::List(sources))?;
    let frozen = frozen
        .iter()
        .map(|member| member.string_keyed_value())
        .collect::<Option<Vec<_>>>();
    let Some(frozen) = frozen else {
        return Err(invalid("DELEGATION_COLLECTION_INVALID"));
    };
    let expected = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "review_nested_collection".into()),
        ("run_id", string_item(context.run, "run_id")?.clone()),
        (
            "request_slot_id",
            string_item(request, "request_slot_id")?.clone(),
        ),
        (
            "parent_branch_id",
            string_item(request, "parent_branch_id")?.clone(),
        ),
        (
            "preliminary_findings",
            string_item(preliminary, "findings")?.clone(),
        ),
        ("members", Value::List(frozen)),
        ("issues", issues),
    ]);
    if !content.py_eq(&expected) {
        return Err(invalid("DELEGATION_COLLECTION_INVALID"));
    }
    Ok(())
}

pub fn validate_review_delegation(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
    case_sensitive: bool,
) -> EngineResult<()> {
    let plan_row = ledger
        .first(
            GraphRead::ReviewPlan,
            &[string_item(run, "run_id")?.clone()],
        )?
        .unwrap_or(Value::Null);
    let plan = compat::loads_value(string_item(&plan_row, "plan_json")?)?;
    let empty = Value::List(Vec::new());
    get(&plan, "conditional_review_assignments")?;
    let assignments = keyed(plan.get("conditional_review_assignments").unwrap_or(&empty))?;
    let limits = get(&plan, "reviewer_delegation_limits")?;
    let requests = ledger.all(
        GraphRead::ReviewRequestsForValidation,
        &[string_item(run, "run_id")?.clone()],
    )?;
    if !requests.is_empty() && (assignments.is_empty() || !limits.truthy()) {
        return Err(invalid("DELEGATION_DISABLED"));
    }
    let mut total_cost = Value::from(0_i64);
    let mut total_children = 0_usize;
    validate_rounds(&requests, limits)?;
    let context = Context {
        ledger,
        run,
        plan_row: &plan_row,
        assignments: &assignments,
        limits,
        case_sensitive,
    };
    for request in requests {
        let (parent, preliminary, contents) = validate_parent(&context, &request)?;
        validate_authority(&context, &request)?;
        let (members, cost) = validate_members(&context, &request, &contents)?;
        total_cost = python_operations::add(&total_cost, &cost)?;
        total_children += members.len();
        let slot = string_item(&request, "request_slot_id")?;
        let fanout = ledger.first(GraphRead::ReviewFanout, &[slot.clone()])?;
        let collection = ledger.first(GraphRead::ReviewCollection, &[slot.clone()])?;
        let (Some(fanout), Some(collection)) = (fanout, collection) else {
            return Err(invalid("DELEGATION_STATE_INVALID"));
        };
        if string_item(&request, "status")?.is_text("awaiting_assessment")
            && !string_item(&fanout, "status")?.is_text("awaiting")
        {
            return Err(invalid("DELEGATION_STATE_INVALID"));
        }
        if in_strings(string_item(&request, "status")?, &["running", "sealed"])?
            && !string_item(&fanout, "status")?.is_text("assessed")
        {
            return Err(invalid("DELEGATION_STATE_INVALID"));
        }
        if string_item(&fanout, "status")?.is_text("assessed") {
            validate_assessment(&context, &request, &fanout, &members)?;
        }
        if string_item(&request, "status")?.is_text("sealed") {
            validate_collection(
                &context,
                &request,
                &parent,
                &preliminary,
                &collection,
                &members,
            )?;
        } else if !string_item(&collection, "status")?.is_text("open") {
            return Err(invalid("DELEGATION_COLLECTION_INVALID"));
        }
    }
    if limits.truthy()
        && (python_operations::greater(
            &Value::from(total_children),
            string_item(limits, "max_children_per_run")?,
        )? || python_operations::greater(
            &total_cost,
            string_item(limits, "max_weighted_dispatch_cost")?,
        )?)
    {
        return Err(invalid("DELEGATION_LIMIT_INVALID"));
    }
    Ok(())
}
