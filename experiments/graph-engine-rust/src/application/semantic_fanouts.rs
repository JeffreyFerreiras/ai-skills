use super::{
    error::{EngineError, EngineResult},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    join_validation::join_members,
};
use crate::{
    compat::{self, Value},
    domain::{
        contract_values::{get, in_strings},
        errors::{self, string_item},
        fanout_contract::validate_fanout_assessment,
        ids, planner, python_operations,
    },
};

fn invalid(code: &str) -> EngineError {
    EngineError::state(code)
}

fn pair(value: &Value) -> EngineResult<(Value, Value)> {
    let stage = string_item(value, "stage")?;
    let generation = string_item(value, "generation")?;
    errors::require_hashable(stage)?;
    errors::require_hashable(generation)?;
    Ok((stage.clone(), generation.clone()))
}

fn matching(left: &(Value, Value), right: &(Value, Value)) -> bool {
    left.0.container_eq(&right.0) && left.1.container_eq(&right.1)
}

fn normalized_assessment(
    content: &Value,
    run_id: &Value,
    fanout_id: &Value,
    members: &[Value],
    case_sensitive: bool,
) -> EngineResult<Vec<Value>> {
    let mut submitted = Value::object([]);
    for key in [
        "schema_version",
        "kind",
        "run_id",
        "fanout_id",
        "members",
        "dependencies",
        "evidence",
    ] {
        submitted.set(key, string_item(content, key)?.clone());
    }
    let normalized = validate_fanout_assessment(&submitted, run_id, fanout_id, members)?;
    planner::validate_fanout_ordering(
        normalized["members"].as_list().unwrap(),
        normalized["dependencies"].as_list().unwrap(),
        case_sensitive,
    )
    .map_err(|_| invalid("FANOUT_ASSESSMENT_INVALID"))
}

pub fn validate_fanouts(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
    case_sensitive: bool,
) -> EngineResult<()> {
    let run_id = string_item(run, "run_id")?;
    let mut collections: Vec<((Value, Value), Value)> = Vec::new();
    ledger.visit(GraphRead::CollectionJoins, &[run_id.clone()], &mut |join| {
        let members = join_members(ledger, string_item(&join, "join_id")?)?;
        let mut identifiers = members
            .iter()
            .map(|member| string_item(member, "branch_id").cloned())
            .collect::<Result<Vec<_>, _>>()?;
        if identifiers.len() > 1 {
            python_operations::sort_comparable_values(&mut identifiers)?;
            let key = pair(&join)?;
            let value = Value::List(identifiers);
            if let Some((_, previous)) = collections
                .iter_mut()
                .find(|(candidate, _)| matching(candidate, &key))
            {
                *previous = value;
            } else {
                collections.push((key, value));
            }
        }
        Ok(())
    })?;
    let fanouts = ledger.all(GraphRead::FanoutsForValidation, &[run_id.clone()])?;
    let mut keys = Vec::new();
    for row in &fanouts {
        let key = pair(row)?;
        if !keys.iter().any(|candidate| matching(candidate, &key)) {
            keys.push(key);
        }
    }
    if keys.len() != collections.len()
        || keys.iter().any(|key| {
            !collections
                .iter()
                .any(|(candidate, _)| matching(candidate, key))
        })
    {
        return Err(invalid("FANOUT_STATE_INVALID"));
    }
    for fanout in fanouts {
        let expected = ids::stable_id_with_key_value(
            run_id,
            string_item(run, "policy_digest")?,
            "fanout",
            string_item(&fanout, "stage")?,
            string_item(&fanout, "generation")?,
            &Value::Null,
        )?;
        let member_ids = match compat::loads_value(string_item(&fanout, "member_branch_ids_json")?)
        {
            Err(error) if matches!(error.kind, "TypeError" | "JSONDecodeError") => {
                return Err(invalid("FANOUT_MEMBER_INVALID"))
            }
            other => other?,
        };
        if !string_item(&fanout, "fanout_id")?.is_text(&expected) {
            return Err(invalid("FANOUT_MEMBER_INVALID"));
        }
        let mut sorted = errors::iterable_set(&member_ids)?;
        python_operations::sort_comparable_values(&mut sorted)?;
        if !member_ids.py_eq(&Value::List(sorted)) {
            return Err(invalid("FANOUT_MEMBER_INVALID"));
        }
        let key = pair(&fanout)?;
        let collection = collections
            .iter()
            .find(|(candidate, _)| matching(candidate, &key))
            .map(|(_, value)| value)
            .unwrap_or(&compat::value::NULL);
        if !member_ids.py_eq(collection) {
            return Err(invalid("FANOUT_MEMBER_INVALID"));
        }
        let identifier = string_item(&fanout, "fanout_id")?;
        let dependencies = ledger.all(GraphRead::FanoutDependencies, &[identifier.clone()])?;
        if string_item(&fanout, "status")?.is_text("awaiting") {
            if !dependencies.is_empty() {
                return Err(invalid("FANOUT_ASSESSMENT_REQUIRED"));
            }
            for member in member_ids.as_list().unwrap() {
                let status = ledger
                    .first(GraphRead::NodeStatus, &[member.clone()])?
                    .unwrap_or(Value::Null);
                if !string_item(&status, "status")?.is_text("pending") {
                    return Err(invalid("FANOUT_ASSESSMENT_REQUIRED"));
                }
            }
            continue;
        }
        let artifact = ledger.first(
            GraphRead::Artifact,
            &[string_item(&fanout, "assessment_ref")?.clone()],
        )?;
        let Some(artifact) = artifact else {
            return Err(invalid("FANOUT_ASSESSMENT_INVALID"));
        };
        if !string_item(&artifact, "kind")?.is_text("evidence_manifest")
            || !string_item(&artifact, "sha256")?.py_eq(string_item(&fanout, "assessment_digest")?)
            || !string_item(&artifact, "content_json")?.truthy()
        {
            return Err(invalid("FANOUT_ASSESSMENT_INVALID"));
        }
        let result = (|| -> EngineResult<(Value, Vec<Value>)> {
            let content = compat::loads_value(string_item(&artifact, "content_json")?)?;
            let normalized = normalized_assessment(
                &content,
                run_id,
                identifier,
                member_ids.as_list().unwrap(),
                case_sensitive,
            )?;
            Ok((content, normalized))
        })();
        let (content, normalized) = match result {
            Err(EngineError::Contract(_)) => return Err(invalid("FANOUT_ASSESSMENT_INVALID")),
            Err(EngineError::Python {
                kind: "KeyError" | "ValueError",
                ..
            }) => return Err(invalid("FANOUT_ASSESSMENT_INVALID")),
            Err(EngineError::Json(error))
                if matches!(error.kind, "ValueError" | "JSONDecodeError") =>
            {
                return Err(invalid("FANOUT_ASSESSMENT_INVALID"))
            }
            other => other?,
        };
        for key in ["authority_ref", "actor", "host_identity", "assessed_at"] {
            if !get(&content, key)?.py_eq(string_item(&fanout, key)?) {
                return Err(invalid("FANOUT_ASSESSMENT_INVALID"));
            }
        }
        if !string_item(&fanout, "host_identity")?.py_eq(string_item(run, "host_identity")?) {
            return Err(invalid("FANOUT_ASSESSMENT_INVALID"));
        }
        if !Value::List(dependencies).py_eq(&Value::List(normalized)) {
            return Err(invalid("FANOUT_DEPENDENCY_STATE_INVALID"));
        }
        for member in member_ids.as_list().unwrap() {
            let branch = ledger
                .first(GraphRead::Node, &[member.clone()])?
                .ok_or_else(|| invalid("FANOUT_MEMBER_INVALID"))?;
            let envelope = compat::loads_value(string_item(&branch, "envelope_json")?)?;
            let mut matching = Vec::new();
            for input in errors::iterable_values(string_item(&envelope, "inputs")?)? {
                if string_item(&input, "ref")?.py_eq(string_item(&artifact, "ref")?) {
                    matching.push(input);
                }
            }
            if matching.len() != 1
                || !string_item(&matching[0], "sha256")?.py_eq(string_item(&artifact, "sha256")?)
                || !string_item(&matching[0], "kind")?.is_text("evidence_manifest")
            {
                return Err(invalid("FANOUT_ASSESSMENT_INVALID"));
            }
            let predecessors = ledger.all(
                GraphRead::FanoutPredecessors,
                &[identifier.clone(), member.clone()],
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
            if (eligible && string_item(&branch, "status")?.is_text("pending"))
                || (!eligible && !string_item(&branch, "status")?.is_text("pending"))
            {
                return Err(invalid("FANOUT_DEPENDENCY_STATE_INVALID"));
            }
        }
    }
    Ok(())
}
