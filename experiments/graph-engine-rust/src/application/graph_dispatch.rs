use super::{
    error::{EngineError, EngineResult},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
};
use crate::{
    compat::{self, Value},
    domain::{errors, ids, python_dictionary::PythonDictionary},
};

fn item<'a>(value: &'a Value, key: &str) -> EngineResult<&'a Value> {
    Ok(errors::string_item(value, key)?)
}

fn optional<'a>(value: &'a Value, key: &str) -> EngineResult<&'a Value> {
    if value.as_object().is_none() {
        return Err(errors::attribute_error(value, "get").into());
    }
    Ok(value.get(key).unwrap_or(&compat::value::NULL))
}

fn copy_dictionary(value: &Value) -> EngineResult<Value> {
    PythonDictionary::coerce(value)?
        .string_keyed_value()
        .ok_or_else(|| {
            EngineError::python(
                "NotImplementedError",
                "general-key dispatch mapping remains unimplemented",
            )
        })
}

fn subset(value: &Value, keys: &[&str]) -> EngineResult<Value> {
    keys.iter()
        .map(|key| Ok(((*key).into(), item(value, key)?.clone())))
        .collect::<EngineResult<Vec<_>>>()
        .map(Value::Object)
}

fn read_only_capabilities(value: &Value) -> EngineResult<Value> {
    let mut result = Vec::new();
    for capability in errors::iterable_values(value)? {
        let effect = item(&capability, "effect")?;
        if effect.is_text("filesystem_read") || effect.is_text("external_read") {
            result.push(copy_dictionary(&capability)?);
        }
    }
    Ok(Value::List(result))
}

fn required_row(
    ledger: &dyn GraphLedgerReadPort,
    query: GraphRead,
    parameters: &[Value],
) -> EngineResult<Value> {
    ledger
        .first(query, parameters)?
        .ok_or_else(|| EngineError::python("TypeError", "'NoneType' object is not subscriptable"))
}

fn artifact(ledger: &dyn GraphLedgerReadPort, reference: &Value) -> EngineResult<Value> {
    let row = required_row(
        ledger,
        GraphRead::ArtifactContent,
        std::slice::from_ref(reference),
    )?;
    Ok(compat::loads_value(item(&row, "content_json")?)?)
}

pub fn effective_code_review_assignment(
    ledger: &dyn GraphLedgerReadPort,
    envelope: &Value,
    assignment: &Value,
) -> EngineResult<Value> {
    let mut projected = copy_dictionary(assignment)?;
    if !optional(&projected, "required_skill")?.is_text("review-agent") {
        return Ok(projected);
    }
    let empty = Value::List(vec![]);
    if envelope.as_object().is_none() {
        return Err(errors::attribute_error(envelope, "get").into());
    }
    let inputs = envelope.get("inputs").unwrap_or(&empty);
    for input in errors::iterable_values(inputs)? {
        if !optional(&input, "kind")?.is_text("evidence_manifest") {
            continue;
        }
        let Some(row) = ledger.first(
            GraphRead::ImmutableArtifactContent,
            &[item(&input, "ref")?.clone()],
        )?
        else {
            continue;
        };
        if !item(&row, "content_json")?.truthy() {
            continue;
        }
        let recovery = match compat::loads_value(item(&row, "content_json")?) {
            Err(error) if error.kind == "JSONDecodeError" => continue,
            result => result?,
        };
        if optional(&recovery, "kind")?.is_text("recovery_authorization")
            && optional(&recovery, "branch_id")?.py_eq(optional(envelope, "branch_id")?)
            && optional(&recovery, "required_skill")?.is_text("review-agent")
            && optional(&recovery, "replacement_skill")?.is_text("bug-hunter-review")
            && optional(&recovery, "decision")?.is_text("APPROVE")
            && optional(&recovery, "equivalence_basis")?
                .is_text("user-authorized-equivalent-review")
        {
            projected.set(
                "required_skill",
                item(&recovery, "replacement_skill")?.clone(),
            );
            projected.set(
                "skill_override",
                Value::object([
                    ("from", item(&recovery, "required_skill")?.clone()),
                    ("to", item(&recovery, "replacement_skill")?.clone()),
                    ("authorization_ref", item(&input, "ref")?.clone()),
                ]),
            );
            break;
        }
    }
    Ok(projected)
}

pub fn review_dispatch_projection(
    ledger: &dyn GraphLedgerReadPort,
    envelope: &Value,
    claim_token: Option<&Value>,
) -> EngineResult<Value> {
    let continuation = optional(envelope, "review_continuation")?;
    if !continuation.truthy() {
        let mut projected = copy_dictionary(envelope)?;
        if let Some(assignment) = projected.get("code_review_assignment") {
            let assignment = effective_code_review_assignment(ledger, envelope, assignment)?;
            projected.set("code_review_assignment", assignment);
        }
        if let Some(token) = claim_token {
            projected.set("claim_token", token.clone());
        }
        return Ok(projected);
    }
    let run_id = item(envelope, "run_id")?;
    let run = required_row(
        ledger,
        GraphRead::DispatchTask,
        std::slice::from_ref(run_id),
    )?;
    let plan = required_row(
        ledger,
        GraphRead::ExecutionPlanBody,
        std::slice::from_ref(run_id),
    )?;
    let plan = compat::loads_value(item(&plan, "plan_json")?)?;
    let empty = Value::List(vec![]);
    let assignments = plan.get("conditional_review_assignments").unwrap_or(&empty);
    if plan.as_object().is_none() {
        return Err(errors::attribute_error(&plan, "get").into());
    }
    let mut by_id = PythonDictionary::default();
    for assignment in errors::iterable_values(assignments)? {
        by_id.insert(
            item(&assignment, "assignment_id")?.clone(),
            assignment.into(),
        )?;
    }
    let mut preliminary_items = Vec::new();
    let mut nested_items = Vec::new();
    let mut projected_assignments = Vec::new();
    let rows = ledger.all(
        GraphRead::DispatchCollections,
        &[
            item(envelope, "branch_id")?.clone(),
            item(envelope, "generation")?.clone(),
        ],
    )?;
    for row in rows {
        let preliminary = artifact(ledger, item(&row, "preliminary_ref")?)?;
        let request = artifact(ledger, item(&row, "request_ref")?)?;
        let request_slot = item(&row, "request_slot_id")?.clone();
        let preliminary_findings = item(&preliminary, "findings")?.clone();
        let evidence = errors::iterable_values(item(&preliminary, "evidence")?)?
            .iter()
            .map(|value| subset(value, &["evidence_id", "kind", "sha256"]))
            .collect::<EngineResult<Vec<_>>>()?;
        preliminary_items.push(Value::object([
            ("request_slot_id", request_slot),
            ("findings", preliminary_findings),
            ("evidence", Value::List(evidence)),
        ]));
        let collection = compat::loads_value(item(&row, "content_json")?)?;
        let request_slot = item(&row, "request_slot_id")?.clone();
        let collection_digest = item(&row, "collection_digest")?.clone();
        let preliminary_findings = item(&collection, "preliminary_findings")?.clone();
        let mut members = Vec::new();
        for member in errors::iterable_values(item(&collection, "members")?)? {
            let mut projected = subset(
                &member,
                &[
                    "branch_id",
                    "assignment_id",
                    "ordinal",
                    "role",
                    "status",
                    "findings",
                ],
            )?;
            for field in ["reason_code", "failure_code", "result_digest"] {
                projected.set(field, item(item(&member, "terminal")?, field)?.clone());
            }
            members.push(projected);
        }
        nested_items.push(Value::object([
            ("request_slot_id", request_slot),
            ("collection_digest", collection_digest),
            ("preliminary_findings", preliminary_findings),
            ("members", Value::List(members)),
            ("issues", item(&collection, "issues")?.clone()),
        ]));
        for member in errors::iterable_values(item(&request, "members")?)? {
            let id = item(&member, "assignment_id")?;
            let Some(assignment) = by_id.get_key_mut(id)? else {
                return Err(EngineError::python(
                    "KeyError",
                    compat::repr::value_repr(id)?,
                ));
            };
            let crate::domain::python_dictionary::DictionaryValue::Value(assignment) = assignment
            else {
                unreachable!("assignment lookup stores Value entries")
            };
            let mut projected = Value::object([
                ("request_slot_id", item(&row, "request_slot_id")?.clone()),
                ("assignment_id", item(&member, "assignment_id")?.clone()),
                ("ordinal", item(&member, "ordinal")?.clone()),
                ("reason_code", item(&member, "reason_code")?.clone()),
                ("acceptance_ids", item(&member, "acceptance_ids")?.clone()),
                ("evidence_ids", item(&member, "evidence_ids")?.clone()),
                ("scope_refs", item(assignment, "scope_refs")?.clone()),
                ("review_lens", item(assignment, "review_lens")?.clone()),
            ]);
            projected.set(
                "effect_capabilities",
                read_only_capabilities(item(assignment, "effect_capabilities")?)?,
            );
            projected_assignments.push(projected);
        }
    }
    let task = compat::loads_value(item(&run, "task_json")?)?;
    let context = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "review_continuation_context".into()),
        (
            "task_scope",
            subset(
                &task,
                &[
                    "task_id",
                    "acceptance_ids",
                    "request_mode",
                    "minimum_route",
                    "risk_level",
                ],
            )?,
        ),
        ("assignments", Value::List(projected_assignments)),
        ("preliminary", Value::List(preliminary_items)),
        ("nested_collections", Value::List(nested_items)),
        ("binding", continuation.clone()),
    ]);
    let bytes = ids::canonical_bytes(&context)?;
    let context_digest = ids::sha256_bytes(&bytes);
    let mut projected = subset(
        envelope,
        &[
            "schema_version",
            "run_id",
            "branch_id",
            "node_instance_id",
            "node_key",
            "role",
            "model",
            "reasoning_effort",
            "mandatory",
            "generation",
            "status",
        ],
    )?;
    projected.set(
        "effect_capabilities",
        read_only_capabilities(item(envelope, "effect_capabilities")?)?,
    );
    projected.set(
        "output_contract",
        copy_dictionary(item(envelope, "output_contract")?)?,
    );
    projected.set(
        "stopping_condition",
        Value::object([
            ("kind", "valid_result_returned".into()),
            ("max_branch_attempts", 1_i64.into()),
        ]),
    );
    let context_size = ids::canonical_bytes(&context)?.len();
    projected.set(
        "inputs",
        Value::List(vec![Value::object([
            ("kind", "review_context".into()),
            (
                "ref",
                format!("thread:review-context-{}", &context_digest[..24]).into(),
            ),
            ("sha256", context_digest.into()),
            ("size_bytes", (context_size as i64).into()),
            ("content", context),
        ])]),
    );
    projected.set("review_continuation", continuation.clone());
    projected.set("artifact_ref", Value::Null);
    projected.set("evidence", Value::List(vec![]));
    projected.set("decision", Value::Null);
    projected.set("retry_count", 0_i64.into());
    projected.set("max_retries", 0_i64.into());
    for field in ["attempt_id", "claim_digest", "lease_expires_at"] {
        projected.set(field, optional(envelope, field)?.clone());
    }
    for field in ["failure_code", "started_at", "finished_at"] {
        projected.set(field, Value::Null);
    }
    if let Some(assignment) = envelope.get("code_review_assignment") {
        projected.set(
            "code_review_assignment",
            effective_code_review_assignment(ledger, envelope, assignment)?,
        );
    }
    if let Some(token) = claim_token {
        projected.set("claim_token", token.clone());
    }
    Ok(projected)
}

pub fn ready_envelopes(ledger: &dyn GraphLedgerReadPort, run: &Value) -> EngineResult<Vec<Value>> {
    ledger
        .all(GraphRead::ReadyEnvelopes, &[item(run, "run_id")?.clone()])?
        .iter()
        .map(|row| {
            review_dispatch_projection(
                ledger,
                &compat::loads_value(item(row, "envelope_json")?)?,
                None,
            )
        })
        .collect()
}
