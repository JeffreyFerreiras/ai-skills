use super::{
    check_eligibility::{self, CheckEligibilityPort},
    error::{EngineError, EngineResult},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    source_capture::StatusSourceCache,
};
use crate::{
    compat::{self, Value},
    domain::{check_receipts, errors},
};

pub trait EvidenceStatusPort: GraphLedgerReadPort + CheckEligibilityPort {
    fn reservation_states(&self, run: &Value) -> EngineResult<Vec<Value>>;
}

fn item<'a>(value: &'a Value, key: &str) -> EngineResult<&'a Value> {
    Ok(errors::string_item(value, key)?)
}
fn optional(value: &Value, key: &str) -> EngineResult<Value> {
    if value.as_object().is_none() {
        return Err(errors::attribute_error(value, "get").into());
    }
    Ok(value.get(key).unwrap_or(&Value::Null).clone())
}

pub fn evidence_status(port: &dyn EvidenceStatusPort, run: &Value) -> EngineResult<Value> {
    if !item(run, "state_schema_version")?.py_eq(&7i64.into()) {
        return Ok(Value::Null);
    }
    let policy = compat::loads_value(item(run, "policy_json")?)?;
    let reservations = port.reservation_states(run)?;
    let mut cache = StatusSourceCache::default();
    let mut capture = |roots: &Value, outputs: &Value| {
        cache.capture_values(roots, outputs, || port.capture_source(roots, outputs))
    };
    let run_id = item(run, "run_id")?;
    let mut checks = Vec::new();
    port.visit(
        GraphRead::CheckEvidenceOrdered,
        std::slice::from_ref(run_id),
        &mut |selected| {
            let artifact = port
                .first(
                    GraphRead::ArtifactContent,
                    std::slice::from_ref(item(&selected, "artifact_ref")?),
                )?
                .ok_or_else(|| {
                    EngineError::python("TypeError", "'NoneType' object is not subscriptable")
                })?;
            let receipt = compat::loads_value(item(&artifact, "content_json")?)?;
            let mut options = Vec::new();
            port.visit(
                GraphRead::EligibleExecutorNodes,
                std::slice::from_ref(run_id),
                &mut |branch| {
                    let envelope = compat::loads_value(item(&branch, "envelope_json")?)?;
                    let configured =
                        check_receipts::configured_check(&policy, item(&selected, "check_id")?)?;
                    let command = item(configured, "command_id")?;
                    let needed = Value::object([
                        ("effect", "command".into()),
                        ("action", "run".into()),
                        ("target_ref", command.clone()),
                    ]);
                    if !errors::iterable_values(item(&envelope, "effect_capabilities")?)?
                        .iter()
                        .any(|value| value.py_eq(&needed))
                    {
                        return Ok(());
                    }
                    let fence = Value::object([
                        ("branch_id", item(&branch, "branch_id")?.clone()),
                        ("attempt_id", item(&envelope, "attempt_id")?.clone()),
                        ("claim_digest", item(&envelope, "claim_digest")?.clone()),
                    ]);
                    match port.verify_fence(&fence, true) {
                        Err(EngineError::State(_)) => return Ok(()),
                        Err(error) => return Err(error),
                        Ok(()) => (),
                    }
                    let mut repeated = false;
                    for reservation in &reservations {
                        let executor = item(reservation, "executor")?;
                        if item(executor, "branch_id")?.py_eq(item(&branch, "branch_id")?)
                            && item(executor, "attempt_id")?.py_eq(item(&envelope, "attempt_id")?)
                            && item(reservation, "check_id")?.py_eq(item(&selected, "check_id")?)
                        {
                            repeated = true;
                            break;
                        }
                    }
                    options.push(Value::object([
                        ("branch_id", item(&branch, "branch_id")?.clone()),
                        ("attempt_id", item(&envelope, "attempt_id")?.clone()),
                        ("repeat_cost", i64::from(repeated).into()),
                    ]));
                    Ok(())
                },
            )?;
            checks.push(Value::object([
                ("check_id", item(&selected, "check_id")?.clone()),
                ("artifact_ref", item(&selected, "artifact_ref")?.clone()),
                (
                    "eligibility",
                    check_eligibility::receipt_eligibility(
                        &receipt,
                        port,
                        None,
                        Some(&mut capture),
                    )?,
                ),
                ("source", optional(&receipt, "source")?),
                ("executor", optional(&receipt, "executor")?),
                ("predecessor_ref", optional(&receipt, "predecessor_ref")?),
                ("coverage", optional(&receipt, "coverage")?),
                (
                    "review_source_binding_ref",
                    optional(&receipt, "review_source_binding_ref")?,
                ),
                ("executor_options", Value::List(options)),
            ]));
            Ok(())
        },
    )?;
    let reservations = reservations
        .iter()
        .map(|reservation| {
            [
                "reservation_id",
                "check_id",
                "state",
                "repeat_cost",
                "budget_id",
            ]
            .iter()
            .map(|key| Ok(((*key).into(), item(reservation, key)?.clone())))
            .collect::<EngineResult<Vec<_>>>()
            .map(Value::Object)
        })
        .collect::<EngineResult<Vec<_>>>()?;
    Ok(Value::object([
        ("checks", Value::List(checks)),
        ("reservations", Value::List(reservations)),
    ]))
}
