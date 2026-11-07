use super::{
    error::{EngineError, EngineResult},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
};
use crate::{
    compat::{self, Value},
    domain::errors,
};

fn item<'a>(value: &'a Value, key: &str) -> EngineResult<&'a Value> {
    Ok(errors::string_item(value, key)?)
}
fn key_equal(left: &Value, right: &Value) -> bool {
    left.py_eq(right) || matches!((left, right), (Value::ConstantNaN, Value::ConstantNaN))
}

pub fn lifecycle_artifacts(
    ledger: &dyn GraphLedgerReadPort,
    run_id: &Value,
    kind: &Value,
) -> EngineResult<Vec<(Value, Value)>> {
    let mut artifacts = Vec::new();
    ledger.visit(
        GraphRead::LedgerArtifacts,
        std::slice::from_ref(run_id),
        &mut |row| {
            let value = compat::loads_value(item(&row, "content_json")?)?;
            if value.as_object().is_some()
                && value
                    .get("kind")
                    .unwrap_or(&compat::value::NULL)
                    .py_eq(kind)
            {
                artifacts.push((row, value));
            }
            Ok(())
        },
    )?;
    Ok(artifacts)
}

pub fn evidence_activation(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
) -> EngineResult<(Value, Value)> {
    let mut matches =
        lifecycle_artifacts(ledger, item(run, "run_id")?, &"evidence_activation".into())?;
    if matches.len() != 1 {
        return Err(EngineError::state("EVIDENCE_ACTIVATION_INVALID"));
    }
    let (row, value) = matches.pop().unwrap();
    let plan = ledger.first(
        GraphRead::ActivationPlanDigest,
        std::slice::from_ref(item(run, "run_id")?),
    )?;
    let get = |key| value.get(key).unwrap_or(&compat::value::NULL);
    if !get("schema_version").py_eq(&1i64.into())
        || !get("original_format").py_eq(&6i64.into())
        || !get("task_digest").py_eq(item(run, "task_digest")?)
        || !get("policy_digest").py_eq(item(run, "policy_digest")?)
    {
        return Err(EngineError::state("EVIDENCE_ACTIVATION_INVALID"));
    }
    let plan = plan.ok_or_else(|| {
        EngineError::python("TypeError", "'NoneType' object is not subscriptable")
    })?;
    if !get("plan_digest").py_eq(item(&plan, "plan_digest")?)
        || !item(&row, "kind")?.is_text("evidence_manifest")
    {
        return Err(EngineError::state("EVIDENCE_ACTIVATION_INVALID"));
    }
    Ok((row, value))
}

#[derive(Default)]
pub struct ReservationStates {
    values: Vec<(Value, Value)>,
}
impl ReservationStates {
    pub fn get(&self, key: &Value) -> EngineResult<Option<&Value>> {
        errors::require_hashable(key)?;
        Ok(self
            .values
            .iter()
            .find(|(candidate, _)| key_equal(candidate, key))
            .map(|(_, value)| value))
    }
    fn get_mut(&mut self, key: &Value) -> EngineResult<Option<&mut Value>> {
        errors::require_hashable(key)?;
        Ok(self
            .values
            .iter_mut()
            .find(|(candidate, _)| key_equal(candidate, key))
            .map(|(_, value)| value))
    }
    fn insert(&mut self, key: Value, value: Value) -> EngineResult<()> {
        if let Some(previous) = self.get_mut(&key)? {
            *previous = value;
        } else {
            self.values.push((key, value));
        }
        Ok(())
    }
    pub fn values(&self) -> Vec<Value> {
        self.values.iter().map(|(_, value)| value.clone()).collect()
    }
}

pub fn reservation_states(
    ledger: &dyn GraphLedgerReadPort,
    run_id: &Value,
) -> EngineResult<ReservationStates> {
    let mut reservations = ReservationStates::default();
    for (row, mut value) in lifecycle_artifacts(ledger, run_id, &"check_reservation".into())? {
        let identifier = item(&value, "reservation_id")?.clone();
        value.set("artifact_ref", item(&row, "ref")?.clone());
        value.set("state", "open".into());
        reservations.insert(identifier, value)?;
    }
    ledger.visit(
        GraphRead::ReservationEvents,
        std::slice::from_ref(run_id),
        &mut |event| {
            let event_type = item(&event, "event_type")?;
            errors::require_hashable(event_type)?;
            if !event_type.is_text("check.finish") && !event_type.is_text("check.abandon") {
                return Ok(());
            }
            let request = compat::loads_value(item(&event, "detail_json")?)?;
            if request.as_object().is_none() {
                return Err(errors::attribute_error(&request, "get").into());
            }
            let identifier = request
                .get("reservation_id")
                .unwrap_or(&compat::value::NULL);
            let Some(reservation) = reservations.get_mut(identifier)? else {
                return Err(EngineError::state("CHECK_RESERVATION_INVALID"));
            };
            if !item(reservation, "state")?.is_text("open") {
                return Err(EngineError::state("CHECK_RESERVATION_INVALID"));
            }
            reservation.set(
                "state",
                if event_type.is_text("check.finish") {
                    "finished"
                } else {
                    "abandoned"
                }
                .into(),
            );
            Ok(())
        },
    )?;
    Ok(reservations)
}

pub fn current_review_binding(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
) -> EngineResult<Option<(Value, Value)>> {
    let mut matches = Vec::new();
    for (row, value) in lifecycle_artifacts(
        ledger,
        item(run, "run_id")?,
        &"review_source_binding".into(),
    )? {
        if value
            .get("source_generation")
            .unwrap_or(&compat::value::NULL)
            .py_eq(item(run, "implementation_generation")?)
        {
            matches.push((row, value));
        }
    }
    if matches.len() > 1 {
        return Err(EngineError::state("REVIEW_BINDING_MISMATCH"));
    }
    Ok(matches.into_iter().next())
}

pub fn verify_review_source(
    ledger: &dyn GraphLedgerReadPort,
    clock: &dyn super::check_fences::CheckClockPort,
    source: &dyn super::check_eligibility::CheckEligibilityPort,
    run: &Value,
    registering: Option<&Value>,
    check_freshness: bool,
) -> EngineResult<(Value, Value)> {
    let Some((row, value)) = current_review_binding(ledger, run)? else {
        return Err(EngineError::state("REVIEW_BINDING_MISMATCH"));
    };
    ledger.visit(
        GraphRead::DeliveryReviewEnvelopes,
        &[
            item(run, "run_id")?.clone(),
            item(run, "implementation_generation")?.clone(),
        ],
        &mut |member| {
            let reference = item(&row, "ref")?;
            let envelope = compat::loads_value(item(&member, "envelope_json")?)?;
            let mut inputs = Vec::new();
            for input in errors::iterable_values(item(&envelope, "inputs")?)? {
                let reference = item(&input, "ref")?.clone();
                errors::require_hashable(&reference)?;
                inputs.push(reference);
            }
            errors::require_hashable(reference)?;
            if !inputs
                .iter()
                .any(|candidate| key_equal(candidate, reference))
            {
                return Err(EngineError::state("REVIEW_BINDING_MISMATCH"));
            }
            Ok(())
        },
    )?;
    let fence = item(&value, "source")?;
    let verified = super::check_fences::verify_check_fence(ledger, clock, run, fence, false)?;
    if !item(&verified.branch, "result_digest")?.py_eq(item(fence, "result_digest")?) {
        return Err(EngineError::state("REVIEW_BINDING_MISMATCH"));
    }
    if !check_freshness {
        return Ok((row, value));
    }
    let coverage = item(&value, "coverage")?;
    let outputs = item(coverage, "outputs")?;
    source.verify_generated_outputs(outputs, registering)?;
    let snapshot = source.capture_source(item(coverage, "roots")?, outputs)?;
    if !item(&snapshot, "sha256")?.py_eq(item(&value, "source_digest")?) {
        return Err(EngineError::state("REVIEW_SOURCE_CHANGED"));
    }
    Ok((row, value))
}
