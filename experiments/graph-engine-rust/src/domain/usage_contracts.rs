use super::{
    contracts::{self, ContractError},
    errors::{require_hashable, string_item, ValidationFailure, ValidationResult},
    ids,
    usage::{self, Context, Counters, Interval, Segment, Source},
};
use crate::compat::Value;
use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};

#[derive(Debug)]
pub enum UsageFailure {
    State(&'static str),
    Validation(ValidationFailure),
}
impl From<ValidationFailure> for UsageFailure {
    fn from(value: ValidationFailure) -> Self {
        Self::Validation(value)
    }
}
impl From<ContractError> for UsageFailure {
    fn from(value: ContractError) -> Self {
        Self::Validation(value.into())
    }
}
pub type UsageResult<T> = Result<T, UsageFailure>;

pub fn integer(value: &Value) -> bool {
    matches!(value, Value::Integer(number) if number >= &BigInt::zero() && number <= &BigInt::from(i64::MAX))
}

fn invalid() -> ValidationFailure {
    ValidationFailure::python("ValueError", "")
}

fn require(value: &Value, keys: &[&str], field: &str) -> ValidationResult<()> {
    let keys = Value::list(keys.iter().map(|key| Value::from(*key)));
    contracts::require_python_keys(value, &keys, &keys, field)
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Integer(value) => !value.is_zero(),
        Value::Float(value) => *value != 0.0,
        Value::ConstantNaN => true,
        Value::String(value) => !value.is_empty(),
        Value::Bytes(value) => !value.is_empty(),
        Value::List(value) | Value::Tuple(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

fn member(value: &Value, allowed: &[&str]) -> bool {
    allowed.iter().any(|item| value.is_text(item))
}

fn set_member(value: &Value, allowed: &[&str]) -> ValidationResult<bool> {
    require_hashable(value)?;
    Ok(member(value, allowed))
}

impl Source {
    pub fn checkpoint(&self, offset: Option<&Value>) -> UsageResult<Value> {
        let source_id = self
            .source_id
            .as_ref()
            .ok_or(UsageFailure::State("USAGE_CHECKPOINT_UNAVAILABLE"))?;
        let selected = match offset {
            None | Some(Value::Null) => {
                Value::Integer(self.snapshots.last().map_or(0, |item| item.offset).into())
            }
            Some(value) => value.clone(),
        };
        let stop = match &selected {
            Value::Integer(value) => value.clone(),
            Value::Bool(value) => BigInt::from(u8::from(*value)),
            _ => {
                return Err(ValidationFailure::python(
                    "TypeError",
                    "slice indices must be integers or None or have an __index__ method",
                )
                .into())
            }
        };
        let length = BigInt::from(self.data.len());
        let stop = if stop < BigInt::zero() {
            (stop + &length).max(BigInt::zero())
        } else {
            stop.min(length)
        };
        let stop = stop.to_usize().expect("clamped source slice fits memory");
        Ok(Value::object([
            ("checkpoint_schema_version", 1_i64.into()),
            ("source_id", source_id.as_str().into()),
            ("offset", selected),
            (
                "prefix_sha256",
                ids::sha256_bytes(&self.data[..stop]).into(),
            ),
        ]))
    }

    pub fn usable_checkpoint(&self) -> UsageResult<Value> {
        if self
            .diagnostics
            .iter()
            .any(|(_, code)| code != "missing_usage")
        {
            return Err(UsageFailure::State("USAGE_CHECKPOINT_UNAVAILABLE"));
        }
        self.checkpoint(None)
    }
}

pub fn validate_checkpoint(source: &Source, value: &Value) -> UsageResult<Value> {
    require(
        value,
        &["offset", "source_id", "prefix_sha256"],
        "usage_checkpoint",
    )?;
    let offset = string_item(value, "offset")?;
    if !integer(offset)
        || matches!(offset, Value::Integer(number) if number > &BigInt::from(source.data.len()))
    {
        return Err(UsageFailure::State("USAGE_CHECKPOINT_INVALID"));
    }
    let source_id = contracts::digest(string_item(value, "source_id")?, "source_id")?;
    let prefix = contracts::digest(string_item(value, "prefix_sha256")?, "prefix_sha256")?;
    let offset = match offset {
        Value::Integer(value) => value
            .to_usize()
            .expect("validated source offset fits memory"),
        _ => unreachable!("integer validation excludes other types"),
    };
    if source.source_id.as_ref() != Some(&source_id)
        || ids::sha256_bytes(&source.data[..offset]) != prefix
        || (offset != 0 && !source.snapshots.iter().any(|item| item.offset == offset))
    {
        return Err(UsageFailure::State("USAGE_CHECKPOINT_MISMATCH"));
    }
    Ok(value.clone())
}

pub fn validate_binding(value: &Value) -> ValidationResult<()> {
    if value.as_object().is_none() {
        return Err(invalid());
    }
    let keys = [
        "usage_schema_version",
        "binding_id",
        "source_id",
        "adapter",
        "branch_id",
        "attempt_id",
        "role",
        "phase",
        "generation",
        "start_offset",
        "offset",
        "prefix_sha256",
        "previous",
        "observed",
        "closed",
        "broken",
        "diagnostics",
        "metric_partial",
        "segments",
        "boundary_kind",
    ];
    require(value, &keys, "usage")?;
    if !value["usage_schema_version"].py_eq(&1_i64.into())
        || !value["adapter"].is_text("codex_jsonl_v1")
    {
        return Err(invalid());
    }
    if !member(&value["phase"], &usage::PHASES)
        || !set_member(
            &value["boundary_kind"],
            &["baseline", "phase", "continuation"],
        )?
    {
        return Err(invalid());
    }
    let binding_id = value["binding_id"].as_string().ok_or_else(invalid)?;
    if binding_id.len() != 27
        || !binding_id.starts_with("g2-")
        || !binding_id.0[3..]
            .iter()
            .all(|point| matches!(*point, 48..=57 | 97..=102))
    {
        return Err(invalid());
    }
    for key in ["binding_id", "role"] {
        contracts::opaque(&value[key], "usage")?;
    }
    for key in ["branch_id", "attempt_id"] {
        if !matches!(value[key], Value::Null) {
            contracts::opaque(&value[key], "usage")?;
        }
    }
    contracts::digest(&value["prefix_sha256"], "usage")?;
    if !matches!(value["source_id"], Value::Null) {
        contracts::digest(&value["source_id"], "usage")?;
    } else if truthy(&value["observed"])
        || !matches!(value["previous"], Value::Null)
        || truthy(&value["segments"])
        || !value["offset"].py_eq(&0_i64.into())
        || !value["start_offset"].py_eq(&0_i64.into())
        || !truthy(&value["diagnostics"])
    {
        return Err(invalid());
    }
    if ["generation", "offset", "start_offset"]
        .iter()
        .any(|key| !integer(&value[key]))
    {
        return Err(invalid());
    }
    if let (Value::Integer(offset), Value::Integer(start)) =
        (&value["offset"], &value["start_offset"])
    {
        if offset < start {
            return Err(invalid());
        }
    }
    if ["observed", "closed", "broken"]
        .iter()
        .any(|key| !matches!(value[key], Value::Bool(_)))
    {
        return Err(invalid());
    }
    if !matches!(value["previous"], Value::Null)
        && !Counters::from_value(&value["previous"], false)
            .is_some_and(|counters| counters.value().container_eq(&value["previous"]))
    {
        return Err(invalid());
    }
    for (key, allowed) in [
        ("diagnostics", usage::DIAGNOSTICS.as_slice()),
        ("metric_partial", usage::OPTIONAL.as_slice()),
    ] {
        let Value::List(items) = &value[key] else {
            return Err(invalid());
        };
        if items.len() > allowed.len() {
            return Err(invalid());
        }
        for item in items {
            if !set_member(item, allowed)? {
                return Err(invalid());
            }
        }
    }
    let Value::List(segments) = &value["segments"] else {
        return Err(invalid());
    };
    for segment in segments {
        require(segment, &["phase", "model", "effort", "totals"], "usage")?;
        let phase = string_item(segment, "phase")?;
        let model = string_item(segment, "model")?;
        let effort = string_item(segment, "effort")?;
        let totals = string_item(segment, "totals")?;
        if !(member(phase, &usage::PHASES) || phase.is_text("unattributed")) {
            return Err(invalid());
        }
        require_hashable(model)?;
        if !model
            .text()
            .is_some_and(|model| usage::known_models().contains(&model))
            || !set_member(effort, &usage::EFFORTS)?
            || !Counters::from_value(totals, true)
                .as_ref()
                .map_or(Value::Null, Counters::value)
                .container_eq(totals)
        {
            return Err(invalid());
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Binding {
    pub raw: Value,
    pub interval: Interval,
}

fn strings(value: &Value) -> Vec<String> {
    match value {
        Value::List(items) => items
            .iter()
            .map(|item| item.text().expect("validated string"))
            .collect(),
        _ => unreachable!("validated list"),
    }
}

fn number(value: &Value) -> usize {
    match value {
        Value::Integer(value) => value
            .to_usize()
            .expect("validated offset fits the native address space"),
        _ => unreachable!("validated integer"),
    }
}

impl Binding {
    pub fn from_value(raw: Value) -> ValidationResult<Self> {
        validate_binding(&raw)?;
        let Value::List(segments) = &raw["segments"] else {
            unreachable!()
        };
        let interval = Interval {
            source_id: raw["source_id"].text(),
            phase: raw["phase"].text().expect("validated phase"),
            start_offset: number(&raw["start_offset"]),
            offset: number(&raw["offset"]),
            prefix_sha256: raw["prefix_sha256"].text().expect("validated digest"),
            previous: Counters::from_value(&raw["previous"], false),
            observed: matches!(raw["observed"], Value::Bool(true)),
            closed: matches!(raw["closed"], Value::Bool(true)),
            broken: matches!(raw["broken"], Value::Bool(true)),
            diagnostics: strings(&raw["diagnostics"]),
            metric_partial: strings(&raw["metric_partial"]),
            boundary_kind: raw["boundary_kind"].text().expect("validated boundary"),
            segments: segments
                .iter()
                .map(|segment| Segment {
                    phase: segment["phase"].text().expect("validated phase"),
                    context: Context {
                        model: segment["model"].text().expect("validated model"),
                        effort: segment["effort"].text().expect("validated effort"),
                    },
                    totals: Counters::from_value(&segment["totals"], true),
                })
                .collect(),
        };
        Ok(Self { raw, interval })
    }

    pub fn value(&self) -> Value {
        let mut result = self.raw.clone();
        result.set("offset", Value::Integer(self.interval.offset.into()));
        result.set("prefix_sha256", self.interval.prefix_sha256.as_str().into());
        result.set(
            "previous",
            self.interval
                .previous
                .as_ref()
                .map_or(Value::Null, Counters::value),
        );
        result.set("observed", self.interval.observed.into());
        result.set("closed", self.interval.closed.into());
        result.set("broken", self.interval.broken.into());
        result.set(
            "diagnostics",
            Value::list(
                self.interval
                    .diagnostics
                    .iter()
                    .map(|item| Value::from(item.as_str())),
            ),
        );
        result.set(
            "metric_partial",
            Value::list(
                self.interval
                    .metric_partial
                    .iter()
                    .map(|item| Value::from(item.as_str())),
            ),
        );
        result.set(
            "segments",
            Value::list(self.interval.segments.iter().map(|segment| {
                Value::object([
                    ("phase", segment.phase.as_str().into()),
                    ("model", segment.context.model.as_str().into()),
                    ("effort", segment.context.effort.as_str().into()),
                    (
                        "totals",
                        segment.totals.as_ref().map_or(Value::Null, Counters::value),
                    ),
                ])
            })),
        );
        result.set("boundary_kind", self.interval.boundary_kind.as_str().into());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_segment_totals_pass_validation_and_fail_only_when_consumed() {
        let raw = Value::object([
            ("usage_schema_version", true.into()),
            ("binding_id", format!("g2-{}", "a".repeat(24)).into()),
            ("source_id", "a".repeat(64).into()),
            ("adapter", "codex_jsonl_v1".into()),
            ("branch_id", Value::Null),
            ("attempt_id", Value::Null),
            ("role", "supervisor".into()),
            ("phase", "scoping".into()),
            ("generation", 0_i64.into()),
            ("start_offset", 0_i64.into()),
            ("offset", 0_i64.into()),
            ("prefix_sha256", ids::sha256_bytes(b"").into()),
            ("previous", Value::Null),
            ("observed", false.into()),
            ("closed", false.into()),
            ("broken", false.into()),
            ("diagnostics", Value::List(Vec::new())),
            ("metric_partial", Value::List(Vec::new())),
            (
                "segments",
                Value::list([Value::object([
                    ("phase", "scoping".into()),
                    ("model", "gpt-6-astra".into()),
                    ("effort", "medium".into()),
                    ("totals", Value::Null),
                ])]),
            ),
            ("boundary_kind", "baseline".into()),
        ]);
        validate_binding(&raw).unwrap();
        let binding = Binding::from_value(raw).unwrap();
        assert!(matches!(
            binding.value()["usage_schema_version"],
            Value::Bool(true)
        ));
        assert!(binding.interval.segments[0].totals.is_none());
        let error = usage::summarize(
            &[&binding.interval],
            &[&binding.interval.segments[0]],
            false,
        )
        .unwrap_err();
        assert!(
            matches!(error, ValidationFailure::Python { kind: "TypeError", message } if message.equals("'NoneType' object is not subscriptable"))
        );
    }
}
