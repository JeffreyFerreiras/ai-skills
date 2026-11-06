use super::{
    error::{EngineError, EngineResult},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    join_validation::{canonical_collection_members, join_members},
};
use crate::{
    compat::{self, Value},
    domain::{
        contract_values::{contains, get, in_strings},
        errors::{iterable_values, set_contains, set_values, string_item},
        execution, ids,
    },
};

fn invalid(code: &str) -> EngineError {
    EngineError::state(code)
}

fn expected_node_keys(join: &Value, run: &Value, policy: &Value) -> EngineResult<Vec<Value>> {
    let key = string_item(join, "join_key")?;
    let mut keys: Vec<Value> = match key.text().as_deref() {
        Some("design_inputs") => vec!["tech_lead".into()],
        Some("implementation") => vec!["senior_engineer".into()],
        Some("research_collection") => vec![
            "design_research_architecture".into(),
            "design_research_validation".into(),
        ],
        Some("design_collection") => vec!["architect".into()],
        Some("delivery_collection") => vec!["code_reviewer".into(), "test_engineer".into()],
        Some("design_consolidation") => vec!["supervisor_design_consolidation".into()],
        Some("delivery_consolidation") => vec!["supervisor_delivery_consolidation".into()],
        Some("closure") => vec![if string_item(run, "selected_route")?.is_text("advisory") {
            "advisory_reviewer".into()
        } else if string_item(run, "selected_route")?.is_text("design_only") {
            "supervisor_design_consolidation".into()
        } else {
            "supervisor_delivery_consolidation".into()
        }],
        _ => Vec::new(),
    };
    if in_strings(key, &["design_collection", "delivery_collection"])? {
        let raw = string_item(run, "selected_tags_json")?;
        let fallback = Value::from("[]");
        let tags = compat::loads_value(if raw.truthy() { raw } else { &fallback })?;
        let specialists = string_item(policy, "specialists")?;
        for tag in iterable_values(&tags)? {
            if contains(specialists, &tag)? {
                let specialist = specialists
                    .as_object()
                    .and_then(|items| items.iter().find(|(name, _)| tag.as_string() == Some(name)))
                    .map(|(_, value)| value)
                    .expect("an existing object key was selected");
                keys.push(string_item(specialist, "node_key")?.clone());
            }
        }
    }
    Ok(set_values(&keys)?)
}

fn same_set(left: &[Value], right: &[Value]) -> bool {
    left.len() == right.len() && left.iter().all(|value| set_contains(right, value))
}

pub fn validate_joins(
    connection: &dyn GraphLedgerReadPort,
    run: &Value,
    policy: &Value,
) -> EngineResult<()> {
    let run_id = string_item(run, "run_id")?;
    let plan_row = connection
        .first(GraphRead::ExecutionPlanBody, &[run_id.clone()])?
        .unwrap_or(Value::Null);
    let plan = compat::loads_value(string_item(&plan_row, "plan_json")?)?;
    connection.visit(
        GraphRead::JoinsForValidation,
        &[run_id.clone()],
        &mut |join| {
            let key = string_item(&join, "join_key")?;
            let binding = match key.text().as_deref() {
                Some("design_inputs") => Some(("dependency", "design")),
                Some("implementation") => Some(("dependency", "delivery")),
                Some("research_collection") => Some(("collection", "research")),
                Some("design_collection") => Some(("collection", "design")),
                Some("delivery_collection") => Some(("collection", "delivery")),
                Some("design_consolidation") => Some(("consolidation", "design")),
                Some("delivery_consolidation") => Some(("consolidation", "delivery")),
                Some("closure") => Some(("closure", "closure")),
                _ => None,
            };
            if !binding.is_some_and(|(kind, stage)| {
                join["kind"].is_text(kind) && join["stage"].is_text(stage)
            }) || !in_strings(string_item(&join, "status")?, &["open", "sealed"])?
            {
                return Err(invalid("JOIN_STATE_INVALID"));
            }
            let expected_id = ids::stable_id_values(
                run_id,
                string_item(run, "policy_digest")?,
                "join",
                &key.text().expect("validated join binding"),
                string_item(&join, "generation")?,
                None,
            )?;
            let join_id = string_item(&join, "join_id")?;
            if !join_id.is_text(&expected_id) {
                return Err(invalid("STABLE_ID_INVALID"));
            }
            let members = join_members(connection, join_id)?;
            if members.is_empty()
                || members
                    .iter()
                    .any(|member| !member["join_mandatory"].truthy())
            {
                return Err(invalid("JOIN_MEMBERSHIP_INVALID"));
            }
            let actual = set_values(
                &members
                    .iter()
                    .map(|member| string_item(member, "node_key").cloned())
                    .collect::<Result<Vec<_>, _>>()?,
            )?;
            let mut expected = expected_node_keys(&join, run, policy)?;
            if key.is_text("delivery_collection")
                && get(&plan, "code_review_panel_version")?.py_eq(&1_i64.into())
            {
                expected.extend(execution::PANEL.iter().map(|(key, _, _)| Value::from(*key)));
                expected = set_values(&expected)?;
            }
            let redesign = if key.is_text("delivery_collection")
                && actual.len() == 1
                && actual[0].is_text("senior_engineer")
                && members.len() == 1
            {
                let raw = string_item(&members[0], "result_json")?;
                let fallback = Value::from("{}");
                let result = match compat::loads_value(if raw.truthy() { raw } else { &fallback }) {
                    Ok(result) => result,
                    Err(error) if error.kind == "JSONDecodeError" => Value::object([]),
                    Err(error) => return Err(error.into()),
                };
                members[0]["status"].is_text("succeeded")
                    && get(&result, "decision")?.is_text("REDESIGN_REQUIRED")
            } else {
                false
            };
            if (!same_set(&actual, &expected) && !redesign)
                || members
                    .iter()
                    .any(|member| !member["generation"].py_eq(&join["generation"]))
            {
                return Err(invalid("JOIN_MEMBERSHIP_INVALID"));
            }
            if join["status"].is_text("sealed") {
                let frozen = match compat::loads_value(string_item(&join, "result_json")?) {
                    Ok(value) => value,
                    Err(error) if matches!(error.kind, "TypeError" | "JSONDecodeError") => {
                        return Err(invalid("JOIN_RESULT_INVALID"))
                    }
                    Err(error) => return Err(error.into()),
                };
                if join["kind"].is_text("collection") {
                    let expected = canonical_collection_members(connection, &members)?;
                    if !frozen.py_eq(&expected) {
                        return Err(invalid("COLLECTION_ARTIFACT_INVALID"));
                    }
                    let document = Value::object([
                        ("schema_version", 1_i64.into()),
                        ("kind", "collection".into()),
                        ("join_id", join_id.clone()),
                        ("members", expected),
                    ]);
                    let reference = format!(
                        "ledger:{expected_id}#sha256={}",
                        ids::sha256_bytes(&ids::canonical_bytes(&document)?)
                    );
                    if connection
                        .first(GraphRead::Artifact, &[reference.into()])?
                        .is_none()
                    {
                        return Err(invalid("COLLECTION_ARTIFACT_INVALID"));
                    }
                }
            }
            Ok(())
        },
    )
}
