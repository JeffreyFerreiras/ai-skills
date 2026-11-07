use super::{
    errors::{require_hashable, string_item, ValidationFailure, ValidationResult},
    usage::{self, Interval, Segment},
    usage_contracts::Binding,
};
use crate::compat::{self, PyString, Value};
use num_bigint::BigInt;
use num_traits::{FromPrimitive, Zero};
use std::{cmp::Ordering, collections::BTreeSet};

fn json_error(error: compat::JsonError) -> ValidationFailure {
    ValidationFailure::python(error.kind, error.message)
}
fn text(value: &Value) -> ValidationResult<PyString> {
    compat::repr::value_str(value).map_err(json_error)
}
fn missing_key(value: &Value) -> ValidationFailure {
    match compat::repr::value_repr(value) {
        Ok(message) => ValidationFailure::python("KeyError", message),
        Err(error) => json_error(error),
    }
}
fn phase(row: &Value) -> ValidationResult<&'static str> {
    let stage = string_item(row, "stage")?;
    require_hashable(stage)?;
    stage
        .text()
        .as_deref()
        .and_then(usage::stage_phase)
        .ok_or_else(|| missing_key(stage))
}
fn fields(target: &mut Value, value: Value) {
    let Value::Object(target) = target else {
        unreachable!("report object")
    };
    if let Value::Object(items) = &value {
        target.extend(items.iter().cloned());
    }
}
fn contains(values: &[Value], value: &Value) -> ValidationResult<bool> {
    require_hashable(value)?;
    Ok(values.iter().any(|item| item.container_eq(value)))
}
fn add_scope(
    scopes: &mut Vec<(String, Value)>,
    phase: &str,
    generation: &Value,
) -> ValidationResult<()> {
    require_hashable(generation)?;
    if !scopes
        .iter()
        .any(|(previous, number)| previous == phase && number.container_eq(generation))
    {
        scopes.push((phase.into(), generation.clone()));
    }
    Ok(())
}
fn greater(right: &Value, left: &Value) -> ValidationResult<bool> {
    fn integer(value: &Value) -> Option<BigInt> {
        match value {
            Value::Integer(value) => Some(value.clone()),
            Value::Bool(value) => Some(u8::from(*value).into()),
            _ => None,
        }
    }
    fn integer_float(integer: &BigInt, float: f64) -> Option<Ordering> {
        if float.is_nan() {
            return None;
        }
        if float == f64::INFINITY {
            return Some(Ordering::Less);
        }
        if float == f64::NEG_INFINITY {
            return Some(Ordering::Greater);
        }
        let truncated = BigInt::from_f64(float).expect("finite float truncates to an integer");
        Some(match integer.cmp(&truncated) {
            Ordering::Equal if float.fract() > 0.0 => Ordering::Less,
            Ordering::Equal if float.fract() < 0.0 => Ordering::Greater,
            ordering => ordering,
        })
    }
    if let (Some(right), Some(left)) = (integer(right), integer(left)) {
        return Ok(right > left);
    }
    match (right, left) {
        (Value::String(right), Value::String(left)) => Ok(right > left),
        (Value::Bytes(right), Value::Bytes(left)) => Ok(right > left),
        (Value::Float(right), Value::Float(left)) => Ok(right > left),
        (Value::ConstantNaN, Value::ConstantNaN | Value::Float(_))
        | (Value::Float(_), Value::ConstantNaN) => Ok(false),
        (Value::Float(right), left) if integer(left).is_some() => {
            Ok(integer_float(&integer(left).unwrap(), *right) == Some(Ordering::Less))
        }
        (right, Value::Float(left)) if integer(right).is_some() => {
            Ok(integer_float(&integer(right).unwrap(), *left) == Some(Ordering::Greater))
        }
        (Value::ConstantNaN, left) if integer(left).is_some() => Ok(false),
        (right, Value::ConstantNaN) if integer(right).is_some() => Ok(false),
        _ => Err(ValidationFailure::python(
            "TypeError",
            format!(
                "'>' not supported between instances of '{}' and '{}'",
                right.python_type(),
                left.python_type()
            ),
        )),
    }
}

fn summary(bindings: &[&Binding], segments: &[&Segment], missing: bool) -> ValidationResult<Value> {
    let intervals = bindings
        .iter()
        .map(|binding| &binding.interval)
        .collect::<Vec<&Interval>>();
    Ok(usage::summarize(&intervals, segments, missing)?.value())
}

pub fn report(bindings: &[Binding], attempts: &[Value], run: &Value) -> ValidationResult<Value> {
    let all_bindings = bindings.iter().collect::<Vec<_>>();
    let covered = bindings
        .iter()
        .map(|binding| binding.raw["attempt_id"].clone())
        .collect::<Vec<_>>();
    let mut missing = Vec::new();
    for row in attempts {
        let attempt = string_item(row, "attempt_id")?;
        if !contains(&covered, attempt)? {
            missing.push(attempt.clone());
        }
    }
    let mut primary_scopes = Vec::new();
    for binding in bindings {
        if binding.raw["branch_id"].is_null() {
            add_scope(
                &mut primary_scopes,
                &binding.interval.phase,
                &binding.raw["generation"],
            )?;
        }
    }
    let mut executed_scopes = vec![("scoping".into(), Value::Integer(BigInt::zero()))];
    for row in attempts {
        add_scope(
            &mut executed_scopes,
            phase(row)?,
            string_item(row, "generation")?,
        )?;
    }
    let status = string_item(run, "status")?;
    if status.is_text("complete") {
        let design = string_item(run, "design_generation")?;
        let implementation = string_item(run, "implementation_generation")?;
        let generation = if greater(implementation, design)? {
            implementation
        } else {
            design
        };
        add_scope(&mut executed_scopes, "closure", generation)?;
    }
    let executed_phases = executed_scopes
        .iter()
        .map(|(phase, _)| phase.clone())
        .collect::<BTreeSet<_>>();
    let missing_scopes = executed_scopes
        .iter()
        .filter(|(phase, generation)| {
            !primary_scopes
                .iter()
                .any(|(previous, number)| previous == phase && number.container_eq(generation))
        })
        .cloned()
        .collect::<Vec<_>>();
    let missing_primary = missing_scopes
        .iter()
        .map(|(phase, _)| phase.clone())
        .collect::<BTreeSet<_>>();
    let running = status.is_text("initialized") || status.is_text("active");
    let segments = bindings
        .iter()
        .flat_map(|binding| &binding.interval.segments)
        .collect::<Vec<_>>();
    let association_coverage = if bindings.is_empty() {
        "unavailable"
    } else if !missing.is_empty() || !missing_primary.is_empty() {
        "partial"
    } else {
        "complete"
    };
    let mut result = Value::object([("usage_schema_version", 1_i64.into())]);
    fields(
        &mut result,
        summary(
            &all_bindings,
            &segments,
            !missing.is_empty() || !missing_primary.is_empty() || running,
        )?,
    );
    for (key, value) in [
        ("running", running.into()),
        ("association_coverage", association_coverage.into()),
        (
            "missing_primary_phases",
            Value::list(
                missing_primary
                    .iter()
                    .map(|phase| Value::from(phase.as_str())),
            ),
        ),
        ("missing_executed_attempts", Value::list(missing.clone())),
        (
            "open_bindings",
            Value::list(
                bindings
                    .iter()
                    .filter(|binding| !binding.interval.closed)
                    .map(|binding| binding.raw["binding_id"].clone()),
            ),
        ),
        (
            "bindings",
            Value::list(bindings.iter().map(|binding| {
                let value = binding.value();
                Value::Object(
                    [
                        "binding_id",
                        "branch_id",
                        "attempt_id",
                        "role",
                        "phase",
                        "generation",
                        "closed",
                    ]
                    .iter()
                    .map(|key| ((*key).into(), value[key].clone()))
                    .collect(),
                )
            })),
        ),
    ] {
        result.set(key, value);
    }
    let mut phases = Value::Object(Vec::new());
    for phase_name in usage::PHASES {
        let selected = bindings
            .iter()
            .filter(|binding| binding.interval.phase == phase_name)
            .collect::<Vec<_>>();
        let selected_segments = selected
            .iter()
            .flat_map(|binding| &binding.interval.segments)
            .filter(|segment| segment.phase == phase_name)
            .collect::<Vec<_>>();
        let mut phase_missing = false;
        for row in attempts {
            if contains(&missing, string_item(row, "attempt_id")?)? && phase(row)? == phase_name {
                phase_missing = true;
                break;
            }
        }
        let mut value = summary(
            &selected,
            &selected_segments,
            phase_missing || missing_primary.contains(phase_name),
        )?;
        value.set(
            "executed",
            (executed_phases.contains(phase_name) || !selected.is_empty()).into(),
        );
        phases.set(phase_name, value);
    }
    result.set("phases", phases);
    for (dimension, field) in [
        ("roles", "role"),
        ("agents", "branch_id"),
        ("attempts", "attempt_id"),
        ("generations", "generation"),
    ] {
        let mut values = BTreeSet::new();
        for binding in bindings {
            if !binding.raw[field].is_null() {
                values.insert(text(&binding.raw[field])?);
            }
        }
        if field == "role" && !missing_primary.is_empty() {
            values.insert("supervisor".into());
        }
        if field == "generation" {
            for (_, generation) in &missing_scopes {
                values.insert(text(generation)?);
            }
        }
        let mut groups = Vec::new();
        for value in values {
            let mut selected = Vec::new();
            for binding in bindings {
                if text(&binding.raw[field])? == value {
                    selected.push(binding);
                }
            }
            let mut group_missing = Vec::new();
            for row in attempts {
                let attempt = string_item(row, "attempt_id")?;
                if contains(&missing, attempt)? && text(string_item(row, field)?)? == value {
                    group_missing.push(attempt.clone());
                }
            }
            let mut group_primary = BTreeSet::new();
            for (phase, generation) in &missing_scopes {
                if (field == "role" && value.equals("supervisor"))
                    || (field == "generation" && text(generation)? == value)
                {
                    group_primary.insert(phase.clone());
                }
            }
            let selected_segments = selected
                .iter()
                .flat_map(|binding| &binding.interval.segments)
                .collect::<Vec<_>>();
            let mut group = summary(
                &selected,
                &selected_segments,
                !group_missing.is_empty() || !group_primary.is_empty(),
            )?;
            group.set("missing_executed_attempts", Value::list(group_missing));
            group.set(
                "missing_primary_phases",
                Value::list(
                    group_primary
                        .iter()
                        .map(|phase| Value::from(phase.as_str())),
                ),
            );
            groups.push((value, group));
        }
        result.set(dimension, Value::Object(groups));
    }
    for (dimension, model_dimension) in [("models", true), ("efforts", false)] {
        let values = segments
            .iter()
            .map(|segment| {
                if model_dimension {
                    segment.context.model.clone()
                } else {
                    segment.context.effort.clone()
                }
            })
            .collect::<BTreeSet<_>>();
        let mut groups = Value::Object(Vec::new());
        for value in values {
            let matches = |segment: &&Segment| {
                if model_dimension {
                    segment.context.model == value
                } else {
                    segment.context.effort == value
                }
            };
            let selected = bindings
                .iter()
                .filter(|binding| {
                    binding
                        .interval
                        .segments
                        .iter()
                        .any(|segment| matches(&segment))
                })
                .collect::<Vec<_>>();
            let selected_segments = segments.iter().copied().filter(matches).collect::<Vec<_>>();
            let mut group = summary(&selected, &selected_segments, false)?;
            group.set("attribution_scope", "observed_intervals_only".into());
            group.set("association_coverage", association_coverage.into());
            groups.set(&value, group);
        }
        result.set(dimension, groups);
    }
    let pairs = segments
        .iter()
        .map(|segment| {
            (
                segment.context.model.clone(),
                segment.context.effort.clone(),
            )
        })
        .collect::<BTreeSet<_>>();
    let mut groups = Value::Object(Vec::new());
    for (model, effort) in pairs {
        let matches =
            |segment: &&Segment| segment.context.model == model && segment.context.effort == effort;
        let selected = bindings
            .iter()
            .filter(|binding| {
                binding
                    .interval
                    .segments
                    .iter()
                    .any(|segment| matches(&segment))
            })
            .collect::<Vec<_>>();
        let selected_segments = segments.iter().copied().filter(matches).collect::<Vec<_>>();
        let mut group = summary(&selected, &selected_segments, false)?;
        group.set("attribution_scope", "observed_intervals_only".into());
        group.set("association_coverage", association_coverage.into());
        groups.set(&format!("{model}/{effort}"), group);
    }
    result.set("model_efforts", groups);
    let unattributed = segments
        .iter()
        .copied()
        .filter(|segment| segment.phase == "unattributed")
        .collect::<Vec<_>>();
    result.set(
        "unattributed",
        summary(&all_bindings, &unattributed, false)?,
    );
    Ok(result)
}
