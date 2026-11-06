use super::{
    error::{EngineError, EngineResult},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    review_binding::validate_review_result_binding,
    timing::TERMINAL,
};
use crate::{
    compat::{self, datetime, repr, Value},
    domain::{
        contract_values::{contains, get, in_strings},
        contracts,
        errors::{
            iterable_set, iterable_values, require_hashable, set_contains, string_item,
            ValidationFailure,
        },
        execution, ids, planner, python_operations,
        result_contract::validate_result_manifest,
        review_identity::delegated_identity,
        task_contract::validate_impact_map,
    },
};

pub fn predates_phase_dispositions(run: &Value) -> EngineResult<bool> {
    Ok(
        crate::domain::policy_validation::version_tuple(string_item(run, "engine_version")?)?
            < crate::domain::policy_validation::version_tuple(&Value::from(
                crate::PHASE_DISPOSITIONS_ENGINE_VERSION,
            ))?,
    )
}

const IMMUTABLE_KEYS: &[&str] = &[
    "schema_version",
    "run_id",
    "branch_id",
    "node_instance_id",
    "node_key",
    "role",
    "model",
    "reasoning_effort",
    "dispatch_runtime",
    "mandatory",
    "generation",
    "inputs",
    "effect_capabilities",
    "output_contract",
    "stopping_condition",
    "retry_count",
    "max_retries",
    "research_assignment",
    "code_review_assignment",
    "simplification_review_version",
    "attempt_id",
    "claim_digest",
    "lease_expires_at",
];

fn invalid(code: &str) -> EngineError {
    EngineError::state(code)
}

fn contract_result(result: Result<Value, ValidationFailure>) -> EngineResult<()> {
    match result {
        Ok(_) => Ok(()),
        Err(ValidationFailure::Contract(_)) => Err(invalid("TERMINAL_RESULT_INVALID")),
        Err(error) => Err(error.into()),
    }
}

fn required_text(value: &Value) -> EngineResult<String> {
    value.text().ok_or_else(|| invalid("NODE_BINDING_INVALID"))
}

fn ordinary_envelope(
    run: &Value,
    node: &Value,
    stored: &Value,
    policy: &Value,
    task: &Value,
    plan: &Value,
    windows: bool,
) -> EngineResult<Value> {
    get(stored, "inputs")?;
    let empty = Value::List(Vec::new());
    let inputs = iterable_values(stored.get("inputs").unwrap_or(&empty))?;
    let spec = planner::NodeSpec {
        key: required_text(string_item(node, "node_key")?)?,
        role: required_text(string_item(node, "role")?)?,
        stage: required_text(string_item(node, "stage")?)?,
        generation: 0,
        mandatory: true,
        specialist_tag: node["specialist_tag"].text(),
    };
    let mut expected = planner::envelope(
        &run["run_id"].text().unwrap_or_default(),
        &run["policy_digest"].text().unwrap_or_default(),
        policy,
        task,
        &spec,
        &required_text(string_item(node, "status")?)?,
        &inputs,
        0,
        Some(plan),
        windows,
    )?;
    // SQLite can preserve numeric spellings that the typed planning API does not use.
    for key in [
        "run_id",
        "generation",
        "retry_count",
        "branch_id",
        "node_instance_id",
    ] {
        let source = if key == "run_id" { run } else { node };
        expected.set(key, string_item(source, key)?.clone());
    }
    let mut sorted = Vec::new();
    for input in inputs {
        let key = Value::list([
            string_item(&input, "kind")?.clone(),
            string_item(&input, "ref")?.clone(),
        ]);
        sorted.push((key, input));
    }
    python_operations::sort_by_less(&mut sorted, |left, right| {
        python_operations::less(&left.0, &right.0)
    })?;
    expected.set(
        "inputs",
        Value::List(sorted.into_iter().map(|(_, input)| input).collect()),
    );
    for key in ["attempt_id", "claim_digest", "lease_expires_at"] {
        expected.set(key, get(stored, key)?.clone());
    }
    if stored.get("review_continuation").is_some() {
        expected.set(
            "review_continuation",
            string_item(stored, "review_continuation")?.clone(),
        );
    }
    Ok(expected)
}

fn validate_delegated_envelope(
    connection: &dyn GraphLedgerReadPort,
    node: &Value,
    stored: &Value,
    assignment: &Value,
    template: &Value,
) -> EngineResult<()> {
    let keys = iterable_set(stored)?;
    if [
        "parent_branch_id",
        "request_slot_id",
        "assignment_id",
        "ordinal",
        "depth",
        "review_assignment",
    ]
    .iter()
    .any(|key| !set_contains(&keys, &Value::from(*key)))
    {
        return Err(invalid("ENVELOPE_INVALID"));
    }
    let slot = string_item(node, "request_slot_id")?;
    let request = connection.first(GraphRead::ReviewRequest, &[slot.clone()])?;
    let member = connection.first(
        GraphRead::ReviewMember,
        &[
            slot.clone(),
            string_item(node, "assignment_id")?.clone(),
            string_item(node, "ordinal")?.clone(),
        ],
    )?;
    let (Some(request), Some(member)) = (request, member) else {
        return Err(invalid("NODE_BINDING_INVALID"));
    };
    if !string_item(&member, "child_branch_id")?.py_eq(string_item(node, "branch_id")?) {
        return Err(invalid("NODE_BINDING_INVALID"));
    }
    let request_artifact = connection.first(
        GraphRead::Artifact,
        &[string_item(&request, "request_ref")?.clone()],
    )?;
    let preliminary_artifact = connection.first(
        GraphRead::Artifact,
        &[string_item(&request, "preliminary_ref")?.clone()],
    )?;
    let (Some(request_artifact), Some(preliminary_artifact)) =
        (request_artifact, preliminary_artifact)
    else {
        return Err(invalid("DELEGATION_REQUEST_INVALID"));
    };
    let frozen = compat::loads_value(string_item(&request_artifact, "content_json")?)?;
    let preliminary = compat::loads_value(string_item(&preliminary_artifact, "content_json")?)?;
    let empty = Value::List(Vec::new());
    get(&frozen, "members")?;
    let mut selected = None;
    for member in iterable_values(frozen.get("members").unwrap_or(&empty))? {
        if get(&member, "assignment_id")?.py_eq(string_item(node, "assignment_id")?)
            && get(&member, "ordinal")?.py_eq(string_item(node, "ordinal")?)
        {
            selected = Some(member);
            break;
        }
    }
    let selected = selected.ok_or_else(|| invalid("DELEGATION_REQUEST_INVALID"))?;
    get(&preliminary, "evidence")?;
    let mut evidence: Vec<(Value, Value)> = Vec::new();
    for item in iterable_values(preliminary.get("evidence").unwrap_or(&empty))? {
        let key = string_item(&item, "evidence_id")?.clone();
        require_hashable(&key)?;
        if let Some((_, previous)) = evidence
            .iter_mut()
            .find(|(other, _)| other.container_eq(&key))
        {
            *previous = item;
        } else {
            evidence.push((key, item));
        }
    }
    let mut selected_evidence = Vec::new();
    for key in iterable_values(string_item(&selected, "evidence_ids")?)? {
        require_hashable(&key)?;
        let Some((_, item)) = evidence.iter().find(|(other, _)| other.container_eq(&key)) else {
            return Err(invalid("DELEGATION_REQUEST_INVALID"));
        };
        selected_evidence.push(item);
    }
    let evidence = selected_evidence
        .into_iter()
        .map(|item| {
            Ok(Value::object([
                ("evidence_id", string_item(item, "evidence_id")?.clone()),
                ("kind", string_item(item, "kind")?.clone()),
                ("sha256", string_item(item, "sha256")?.clone()),
            ]))
        })
        .collect::<EngineResult<Vec<_>>>()?;
    let expected_assignment = Value::object([
        (
            "review_lens",
            string_item(assignment, "review_lens")?.clone(),
        ),
        (
            "prompt_template",
            string_item(assignment, "prompt_template")?.clone(),
        ),
        (
            "reason_code",
            string_item(&selected, "reason_code")?.clone(),
        ),
        (
            "acceptance_ids",
            Value::List(iterable_values(string_item(&selected, "acceptance_ids")?)?),
        ),
        (
            "evidence_ids",
            Value::List(iterable_values(string_item(&selected, "evidence_ids")?)?),
        ),
        ("evidence", Value::List(evidence)),
        (
            "scope_refs",
            Value::List(iterable_values(string_item(assignment, "scope_refs")?)?),
        ),
    ]);
    for key in [
        "parent_branch_id",
        "request_slot_id",
        "assignment_id",
        "ordinal",
    ] {
        if !string_item(stored, key)?.py_eq(string_item(node, key)?) {
            return Err(invalid("ENVELOPE_INVALID"));
        }
    }
    let mut output = string_item(template, "output_contract")?.clone();
    output.set("artifact_required", false.into());
    if !string_item(stored, "depth")?.py_eq(&1_i64.into())
        || !string_item(stored, "model")?.py_eq(string_item(assignment, "model")?)
        || !string_item(stored, "reasoning_effort")?
            .py_eq(string_item(assignment, "reasoning_effort")?)
        || !string_item(stored, "effect_capabilities")?
            .py_eq(string_item(assignment, "effect_capabilities")?)
        || !string_item(stored, "output_contract")?.py_eq(&output)
        || !string_item(stored, "review_assignment")?.py_eq(&expected_assignment)
        || !string_item(stored, "inputs")?.py_eq(&empty)
    {
        return Err(invalid("ENVELOPE_INVALID"));
    }
    Ok(())
}

fn validate_attempt_envelope(node: &Value, stored: &Value) -> EngineResult<()> {
    let running = node["status"].is_text("running");
    if running || in_strings(&node["status"], &["succeeded", "failed", "timed_out"])? {
        let attempt = get(stored, "attempt_id")?;
        let claim = get(stored, "claim_digest")?;
        if !attempt.as_string().is_some_and(|value| !value.is_empty())
            || !claim.as_string().is_some_and(|value| value.len() == 64)
            || (running && !get(stored, "lease_expires_at")?.truthy())
        {
            return Err(invalid("ATTEMPT_STATE_INVALID"));
        }
        if contracts::opaque(string_item(stored, "attempt_id")?, "attempt_id").is_err()
            || contracts::digest(string_item(stored, "claim_digest")?, "claim_digest").is_err()
        {
            return Err(invalid("ATTEMPT_STATE_INVALID"));
        }
        if running && datetime::fromisoformat(string_item(stored, "lease_expires_at")?).is_err() {
            return Err(invalid("ATTEMPT_STATE_INVALID"));
        }
    } else {
        for key in ["attempt_id", "claim_digest", "lease_expires_at"] {
            if !get(stored, key)?.is_null() {
                return Err(invalid("ATTEMPT_STATE_INVALID"));
            }
        }
    }
    Ok(())
}

fn validate_inputs(connection: &dyn GraphLedgerReadPort, stored: &Value) -> EngineResult<()> {
    let required = ["kind", "ref", "sha256", "size_bytes"];
    for input in iterable_values(string_item(stored, "inputs")?)? {
        let keys = iterable_set(&input)?;
        if keys.iter().any(|key| {
            !["kind", "ref", "sha256", "size_bytes", "content"]
                .iter()
                .any(|allowed| key.is_text(allowed))
        }) || required
            .iter()
            .any(|key| !set_contains(&keys, &Value::from(*key)))
        {
            return Err(invalid("INPUT_ARTIFACT_INVALID"));
        }
        let artifact = connection
            .first(GraphRead::Artifact, &[string_item(&input, "ref")?.clone()])?
            .ok_or_else(|| invalid("INPUT_ARTIFACT_INVALID"))?;
        for key in ["kind", "sha256", "size_bytes"] {
            if !string_item(&artifact, key)?.container_eq(string_item(&input, key)?) {
                return Err(invalid("INPUT_ARTIFACT_INVALID"));
            }
        }
        if set_contains(&keys, &Value::from("content")) {
            if !string_item(&artifact, "source_type")?.is_text("ledger")
                || string_item(&artifact, "content_json")?.is_null()
            {
                return Err(invalid("INPUT_ARTIFACT_INVALID"));
            }
            let content = match compat::loads_value(string_item(&artifact, "content_json")?) {
                Ok(content) => content,
                Err(error) if error.kind == "JSONDecodeError" => {
                    return Err(invalid("INPUT_ARTIFACT_INVALID"))
                }
                Err(error) => return Err(error.into()),
            };
            if !string_item(&input, "content")?.py_eq(&content) {
                return Err(invalid("INPUT_ARTIFACT_INVALID"));
            }
        }
    }
    Ok(())
}

fn validate_terminal(
    connection: &dyn GraphLedgerReadPort,
    run: &Value,
    node: &Value,
    stored: &Value,
    template: &Value,
    task: &Value,
    policy: &Value,
) -> EngineResult<()> {
    if !string_item(node, "result_json")?.truthy() || !string_item(node, "result_digest")?.truthy()
    {
        return Err(invalid("TERMINAL_RESULT_MISSING"));
    }
    let result = match compat::loads_value(string_item(node, "result_json")?) {
        Ok(value) => value,
        Err(error) if error.kind == "JSONDecodeError" => {
            return Err(invalid("TERMINAL_RESULT_INVALID"))
        }
        Err(error) => return Err(error.into()),
    };
    if !node["result_digest"].is_text(&ids::sha256_bytes(&ids::canonical_bytes(&result)?)) {
        return Err(invalid("TERMINAL_RESULT_INVALID"));
    }
    let reference = python_operations::add(
        &Value::from("ledger:"),
        &Value::String(repr::value_str(string_item(node, "branch_id")?)?),
    )?;
    let reference = python_operations::add(&reference, &Value::from("#sha256="))?;
    let reference = python_operations::add(
        &reference,
        &Value::String(repr::value_str(string_item(node, "result_digest")?)?),
    )?;
    let artifact = connection
        .first(GraphRead::ImmutableArtifact, &[reference])?
        .ok_or_else(|| invalid("TERMINAL_RESULT_INVALID"))?;
    let wrapper_kind = match node["status"].text().as_deref() {
        Some("succeeded") => "branch_result",
        Some("failed") => "failure",
        Some("timed_out") => "timeout",
        Some("skipped") => "skip",
        _ => unreachable!("validated terminal status"),
    };
    if !string_item(&artifact, "kind")?.is_text(wrapper_kind) {
        return Err(invalid("TERMINAL_RESULT_INVALID"));
    }
    let mut branch = node.clone();
    branch.set("run_id", string_item(run, "run_id")?.clone());
    branch.set(
        "output_contract",
        string_item(stored, "output_contract")?.clone(),
    );
    if node["node_key"].is_text("impact_mapper") && node["status"].is_text("succeeded") {
        contract_result(validate_impact_map(&result, task, policy))?;
    } else if in_strings(&node["status"], &["succeeded", "failed"])? {
        if node["node_key"]
            .as_string()
            .is_some_and(|key| key.starts_with("supervisor_"))
        {
            let allowed = [
                "schema_version",
                "kind",
                "run_id",
                "join_id",
                "generation",
                "source_branch_ids",
                "finding_dispositions",
                "delegated_finding_dispositions",
                "outcome",
                "attempt_id",
                "claim_digest",
            ];
            let mut filtered = Value::object([]);
            for key in allowed {
                if contains(&result, &Value::from(key))? {
                    filtered.set(key, string_item(&result, key)?.clone());
                }
            }
            contract_result(validate_result_manifest(&filtered, &branch, false))?;
        } else {
            let historical = match predates_phase_dispositions(run) {
                Err(EngineError::Contract(_)) => return Err(invalid("TERMINAL_RESULT_INVALID")),
                result => result?,
            };
            contract_result(validate_result_manifest(&result, &branch, historical))?;
            if node["status"].is_text("succeeded") {
                validate_review_result_binding(connection, stored, &result)?;
            }
        }
    } else {
        let kind = if node["status"].is_text("timed_out") {
            "timeout"
        } else {
            "skip"
        };
        if !get(&result, "kind")?.is_text(kind) {
            return Err(invalid("TERMINAL_RESULT_INVALID"));
        }
    }
    if in_strings(
        &node["node_key"],
        &["design_research_architecture", "design_research_validation"],
    )? {
        if !get(&result, "decision")?.is_null() || get(&result, "findings")?.truthy() {
            return Err(invalid("TERMINAL_RESULT_INVALID"));
        }
        if node["status"].is_text("succeeded") && !get(&result, "evidence")?.truthy() {
            return Err(invalid("TERMINAL_RESULT_INVALID"));
        }
        let empty = Value::List(Vec::new());
        for evidence in iterable_values(result.get("evidence").unwrap_or(&empty))? {
            let artifact = connection
                .first(
                    GraphRead::ArtifactKindDigest,
                    &[get(&evidence, "ref")?.clone()],
                )?
                .ok_or_else(|| invalid("TERMINAL_RESULT_INVALID"))?;
            if !string_item(&artifact, "kind")?.container_eq(get(&evidence, "kind")?)
                || !string_item(&artifact, "sha256")?.container_eq(get(&evidence, "sha256")?)
            {
                return Err(invalid("TERMINAL_RESULT_INVALID"));
            }
        }
    }
    let redesign = node["node_key"].is_text("senior_engineer")
        && node["status"].is_text("succeeded")
        && get(&result, "decision")?.is_text("REDESIGN_REQUIRED");
    let reference = get(stored, "artifact_ref")?;
    if redesign {
        if !reference.is_null() {
            return Err(invalid("TERMINAL_RESULT_INVALID"));
        }
    } else {
        if reference.as_object().is_none() {
            return Err(invalid("TERMINAL_RESULT_INVALID"));
        }
        let artifact = connection
            .first(GraphRead::Artifact, &[get(reference, "ref")?.clone()])?
            .ok_or_else(|| invalid("TERMINAL_RESULT_INVALID"))?;
        if !string_item(&artifact, "kind")?.container_eq(get(reference, "kind")?)
            || !string_item(&artifact, "sha256")?.container_eq(get(reference, "sha256")?)
        {
            return Err(invalid("TERMINAL_RESULT_INVALID"));
        }
        let expected_kind = if node["status"].is_text("succeeded") {
            string_item(string_item(template, "output_contract")?, "artifact_kind")?.clone()
        } else {
            wrapper_kind.into()
        };
        if !string_item(&artifact, "kind")?.py_eq(&expected_kind) {
            return Err(invalid("TERMINAL_RESULT_INVALID"));
        }
    }
    if !get(&result, "attempt_id")?.py_eq(get(stored, "attempt_id")?)
        || !get(&result, "claim_digest")?.py_eq(get(stored, "claim_digest")?)
    {
        return Err(invalid("ATTEMPT_STATE_INVALID"));
    }
    Ok(())
}

pub fn validate_nodes(
    connection: &dyn GraphLedgerReadPort,
    run: &Value,
    task: &Value,
    policy: &Value,
    windows: bool,
) -> EngineResult<()> {
    let row = connection
        .first(
            GraphRead::ExecutionPlanBody,
            &[string_item(run, "run_id")?.clone()],
        )?
        .unwrap_or(Value::Null);
    let plan = compat::loads_value(string_item(&row, "plan_json")?)?;
    let specialists = string_item(policy, "specialists")?;
    let specialist_items = specialists.as_object().ok_or_else(|| {
        EngineError::python(
            "AttributeError",
            format!(
                "'{}' object has no attribute 'items'",
                specialists.python_type()
            ),
        )
    })?;
    let mut by_node: Vec<(Value, Value)> = Vec::new();
    for (tag, specialist) in specialist_items {
        let key = string_item(specialist, "node_key")?;
        require_hashable(key)?;
        if let Some((_, previous)) = by_node
            .iter_mut()
            .find(|(other, _)| other.container_eq(key))
        {
            *previous = Value::String(tag.clone());
        } else {
            by_node.push((key.clone(), Value::String(tag.clone())));
        }
    }
    connection.visit(
        GraphRead::NodesForValidation,
        &[string_item(run, "run_id")?.clone()],
        &mut |node| {
            let delegated = compat::integer::coerce(string_item(&node, "depth")?)? == 1.into();
            let node_key = string_item(&node, "node_key")?;
            let role = string_item(&node, "role")?;
            let template;
            let assignment;
            let expected_branch;
            let expected_node;
            if delegated {
                if !node_key.is_text("delegated_review")
                    || !string_item(&node, "stage")?.is_text("review_delegation")
                    || !in_strings(role, &["code_reviewer", "security_reviewer"])?
                    || !string_item(&node, "request_slot_id")?.truthy()
                    || !string_item(&node, "assignment_id")?.truthy()
                    || !matches!(
                        string_item(&node, "ordinal")?,
                        Value::Integer(_) | Value::Bool(_)
                    )
                    || python_operations::less(string_item(&node, "ordinal")?, &1_i64.into())?
                    || !string_item(&node, "parent_branch_id")?.truthy()
                    || !string_item(&node, "specialist_tag")?.is_null()
                {
                    return Err(invalid("NODE_BINDING_INVALID"));
                }
                let empty = Value::List(Vec::new());
                get(&plan, "conditional_review_assignments")?;
                let mut found = None;
                for candidate in
                    iterable_values(plan.get("conditional_review_assignments").unwrap_or(&empty))?
                {
                    if string_item(&candidate, "assignment_id")?
                        .py_eq(string_item(&node, "assignment_id")?)
                    {
                        found = Some(candidate);
                        break;
                    }
                }
                assignment = found.ok_or_else(|| invalid("NODE_BINDING_INVALID"))?;
                if !string_item(&assignment, "role")?.py_eq(role) {
                    return Err(invalid("NODE_BINDING_INVALID"));
                }
                expected_branch = delegated_identity(
                    string_item(&node, "request_slot_id")?,
                    string_item(&node, "assignment_id")?,
                    string_item(&node, "ordinal")?,
                    &Value::from("branch"),
                )?;
                expected_node = delegated_identity(
                    string_item(&node, "request_slot_id")?,
                    string_item(&node, "assignment_id")?,
                    string_item(&node, "ordinal")?,
                    &Value::from("node"),
                )?;
                let key = if role.is_text("code_reviewer") {
                    "code_reviewer"
                } else {
                    "security_reviewer"
                };
                template = string_item(string_item(policy, "node_templates")?, key)?.clone();
            } else {
                if compat::integer::coerce(string_item(&node, "depth")?)? != 0.into()
                    || [
                        "parent_branch_id",
                        "request_slot_id",
                        "assignment_id",
                        "ordinal",
                    ]
                    .iter()
                    .any(|key| !node[*key].is_null())
                {
                    return Err(invalid("NODE_BINDING_INVALID"));
                }
                require_hashable(node_key)?;
                let panel = execution::PANEL
                    .iter()
                    .any(|(key, _, _)| node_key.is_text(key));
                if panel
                    && !node_key.is_text("code_reviewer")
                    && !get(&plan, "code_review_panel_version")?.py_eq(&1_i64.into())
                {
                    return Err(invalid("NODE_BINDING_INVALID"));
                }
                let key = if panel {
                    "code_reviewer".to_owned()
                } else {
                    node_key.text().unwrap_or_default()
                };
                let templates = string_item(policy, "node_templates")?;
                template = get(templates, &key)?.clone();
                if template.is_null()
                    || !role.py_eq(string_item(&template, "role")?)
                    || !contains(
                        string_item(&template, "stages")?,
                        string_item(&node, "stage")?,
                    )?
                    || !string_item(&node, "mandatory")?.truthy()
                {
                    return Err(invalid("NODE_BINDING_INVALID"));
                }
                let mut name = required_text(node_key)?;
                name.push('@');
                name.push_str(&required_text(string_item(&node, "stage")?)?);
                let tag = string_item(&node, "specialist_tag")?;
                expected_branch = ids::stable_id_with_tag_value(
                    string_item(run, "run_id")?,
                    string_item(run, "policy_digest")?,
                    "branch",
                    &name,
                    string_item(&node, "generation")?,
                    tag,
                )?;
                expected_node = ids::stable_id_with_tag_value(
                    string_item(run, "run_id")?,
                    string_item(run, "policy_digest")?,
                    "node",
                    &name,
                    string_item(&node, "generation")?,
                    tag,
                )?;
                let no_tag = Value::Null;
                let expected_tag = by_node
                    .iter()
                    .find(|(key, _)| key.container_eq(node_key))
                    .map(|(_, tag)| tag)
                    .unwrap_or(&no_tag);
                if !string_item(&node, "specialist_tag")?.py_eq(expected_tag) {
                    return Err(invalid("NODE_BINDING_INVALID"));
                }
                assignment = Value::Null;
            }
            if !string_item(&node, "branch_id")?.is_text(&expected_branch)
                || !string_item(&node, "node_instance_id")?.is_text(&expected_node)
            {
                return Err(invalid("STABLE_ID_INVALID"));
            }
            if !in_strings(
                string_item(&node, "status")?,
                &[
                    "pending",
                    "ready",
                    "running",
                    "waiting_for_review_children",
                    "succeeded",
                    "failed",
                    "timed_out",
                    "skipped",
                ],
            )? || !python_operations::less_equal(
                &0_i64.into(),
                string_item(&node, "retry_count")?,
            )? || !python_operations::less_equal(
                string_item(&node, "retry_count")?,
                string_item(&node, "max_retries")?,
            )? || !string_item(&node, "max_retries")?
                .py_eq(string_item(&template, "max_retries")?)
            {
                return Err(invalid("BRANCH_STATE_INVALID"));
            }
            let stored = match compat::loads_value(string_item(&node, "envelope_json")?) {
                Ok(value) => value,
                Err(error) if error.kind == "JSONDecodeError" => {
                    return Err(invalid("ENVELOPE_INVALID"))
                }
                Err(error) => return Err(error.into()),
            };
            let expected = if delegated {
                validate_delegated_envelope(connection, &node, &stored, &assignment, &template)?;
                stored.clone()
            } else {
                ordinary_envelope(run, &node, &stored, policy, task, &plan, windows)?
            };
            let actual_keys = iterable_set(&stored)?;
            let expected_keys = iterable_set(&expected)?;
            if actual_keys.len() != expected_keys.len()
                || actual_keys
                    .iter()
                    .any(|key| !set_contains(&expected_keys, key))
            {
                return Err(invalid("ENVELOPE_INVALID"));
            }
            for key in IMMUTABLE_KEYS {
                if !get(&stored, key)?.py_eq(get(&expected, key)?) {
                    return Err(invalid("ENVELOPE_INVALID"));
                }
            }
            if !string_item(&stored, "status")?.py_eq(string_item(&node, "status")?)
                || !string_item(&stored, "failure_code")?.py_eq(string_item(&node, "failure_code")?)
            {
                return Err(invalid("ENVELOPE_STATE_INVALID"));
            }
            validate_attempt_envelope(&node, &stored)?;
            validate_inputs(connection, &stored)?;
            if in_strings(string_item(&node, "status")?, TERMINAL)? {
                validate_terminal(connection, run, &node, &stored, &template, task, policy)?;
            }
            Ok(())
        },
    )
}
