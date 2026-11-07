use super::{
    error::{EngineError, EngineResult},
    usage_store::UsageSession,
};
use crate::{
    compat::{self, Value},
    domain::{
        contracts,
        errors::require_hashable,
        usage,
        usage_contracts::{self, Binding},
    },
};

pub fn optional_corruption(error: &EngineError) -> bool {
    match error {
        EngineError::Contract(_) => true,
        EngineError::Json(error) => matches!(
            error.kind,
            "JSONDecodeError"
                | "ValueError"
                | "UnicodeError"
                | "UnicodeDecodeError"
                | "UnicodeEncodeError"
                | "UnicodeTranslateError"
                | "TypeError"
                | "RecursionError"
        ),
        EngineError::Python { kind, .. } => matches!(
            *kind,
            "ValueError"
                | "TypeError"
                | "KeyError"
                | "RecursionError"
                | "UnicodeError"
                | "UnicodeDecodeError"
                | "UnicodeEncodeError"
                | "UnicodeTranslateError"
        ),
        _ => false,
    }
}

pub fn load_bindings(session: &dyn UsageSession, run_id: &Value) -> EngineResult<Vec<Binding>> {
    let mut bindings: Vec<Binding> = Vec::new();
    session.visit_usage_observations(run_id, &mut |observation| {
        let value = compat::loads_value(&observation.detail)?;
        usage_contracts::validate_binding(&value)?;
        if matches!(value["branch_id"], Value::Null) {
            if !value["role"].is_text("supervisor") || !matches!(value["attempt_id"], Value::Null) {
                return Err(EngineError::python("ValueError", ""));
            }
        } else {
            let association = session
                .usage_binding_association(run_id, &value["branch_id"], &value["attempt_id"])?
                .ok_or_else(|| EngineError::python("ValueError", ""))?;
            if !association.role.py_eq(&value["role"])
                || !association.generation.py_eq(&value["generation"])
            {
                return Err(EngineError::python("ValueError", ""));
            }
            require_hashable(&association.stage)?;
            let phase = association
                .stage
                .text()
                .as_deref()
                .and_then(usage::stage_phase);
            if !phase.is_some_and(|phase| value["phase"].is_text(phase)) {
                return Err(EngineError::python("ValueError", ""));
            }
        }
        let key = value["binding_id"]
            .text()
            .expect("validated binding identifier");
        let binding = Binding::from_value(value)?;
        if let Some(index) = bindings
            .iter()
            .position(|item| item.raw["binding_id"].is_text(&key))
        {
            bindings[index] = binding;
        } else {
            bindings.push(binding);
        }
        Ok(())
    })?;
    Ok(bindings)
}

pub fn bindings_value(bindings: &[Binding]) -> Value {
    Value::Object(
        bindings
            .iter()
            .map(|binding| {
                (
                    binding.raw["binding_id"]
                        .as_string()
                        .expect("validated binding identifier")
                        .clone(),
                    binding.value(),
                )
            })
            .collect(),
    )
}

pub struct AssociationArguments<'a> {
    pub branch_id: &'a Value,
    pub attempt_id: &'a Value,
    pub phase: &'a Value,
    pub generation: &'a Value,
}

pub fn association(
    session: &dyn UsageSession,
    run_id: &Value,
    args: AssociationArguments<'_>,
) -> EngineResult<Value> {
    if !matches!(args.branch_id, Value::Null) || !matches!(args.attempt_id, Value::Null) {
        if !args.branch_id.truthy()
            || !args.attempt_id.truthy()
            || !matches!(args.phase, Value::Null)
            || !matches!(args.generation, Value::Null)
        {
            return Err(EngineError::state("USAGE_ASSOCIATION_INVALID"));
        }
        let branch = contracts::opaque(args.branch_id, "branch_id")?;
        let attempt = contracts::opaque(args.attempt_id, "attempt_id")?;
        let selected = session
            .usage_record_association(run_id, &branch, &attempt)?
            .ok_or_else(|| EngineError::state("USAGE_ASSOCIATION_INVALID"))?;
        require_hashable(&selected.stage)?;
        let phase = selected
            .stage
            .text()
            .as_deref()
            .and_then(usage::stage_phase)
            .ok_or_else(|| EngineError::state("USAGE_ASSOCIATION_INVALID"))?;
        return Ok(Value::object([
            ("branch_id", selected.branch_id),
            ("attempt_id", selected.attempt_id),
            ("role", selected.role),
            ("phase", phase.into()),
            ("generation", selected.generation),
        ]));
    }
    if !usage::PHASES.iter().any(|phase| args.phase.is_text(phase))
        || !usage_contracts::integer(args.generation)
    {
        return Err(EngineError::state("USAGE_ASSOCIATION_INVALID"));
    }
    Ok(Value::object([
        ("branch_id", Value::Null),
        ("attempt_id", Value::Null),
        ("role", "supervisor".into()),
        ("phase", args.phase.clone()),
        ("generation", args.generation.clone()),
    ]))
}
