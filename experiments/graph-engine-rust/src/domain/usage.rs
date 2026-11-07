use super::errors::{ValidationFailure, ValidationResult};
use crate::compat::Value;
use num_bigint::BigInt;
use num_traits::Zero;
use std::{collections::BTreeSet, sync::OnceLock};

pub const PHASES: [&str; 5] = [
    "scoping",
    "research_design",
    "implementation",
    "review_testing",
    "closure",
];
pub const REQUIRED: [&str; 3] = ["input_tokens", "output_tokens", "total_tokens"];
pub const OPTIONAL: [&str; 3] = [
    "cached_input_tokens",
    "reasoning_output_tokens",
    "cache_write_tokens",
];
pub const MAX_BYTES: usize = 64 * 1024 * 1024;
pub const CHUNK_BYTES: usize = 1024 * 1024;
pub const MAX_RECORDS: usize = 100_000;
pub const EFFORTS: [&str; 9] = [
    "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra", "unknown",
];
pub const DIAGNOSTICS: [&str; 18] = [
    "source_unavailable",
    "source_unsafe",
    "source_changed",
    "source_limit",
    "record_limit",
    "malformed_record",
    "unsupported_usage",
    "missing_identity",
    "missing_usage",
    "identity_changed",
    "prefix_changed",
    "counter_reset",
    "prefix_gap",
    "ambiguous_increment",
    "unknown_context",
    "optional_discontinuity",
    "open_interval",
    "invalid_usage_state",
];

pub fn known_models() -> &'static BTreeSet<String> {
    static MODELS: OnceLock<BTreeSet<String>> = OnceLock::new();
    MODELS.get_or_init(|| {
        let mut models = BTreeSet::from(["unknown".into()]);
        for host in super::hosts::HOSTS {
            for row in super::hosts::catalog_for(host).expect("built-in host has a legacy catalog")
            {
                if row.model != "primary-thread" {
                    models.insert(row.model.into());
                }
            }
            for revision in super::hosts::supported_revisions(host) {
                for model in super::hosts::model_options(host, Some(*revision))
                    .expect("built-in host has model options for each supported catalog")
                    .keys()
                {
                    models.insert((*model).into());
                }
            }
        }
        models
    })
}

fn add_sorted(values: &mut Vec<String>, value: &str) {
    values.push(value.into());
    values.sort();
    values.dedup();
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Counters {
    pub required: [BigInt; 3],
    pub optional: [Option<BigInt>; 3],
}

impl Counters {
    pub fn from_value(value: &Value, aggregate: bool) -> Option<Self> {
        value.as_object()?;
        let valid = |value: &Value| match value {
            Value::Integer(number)
                if number >= &BigInt::zero()
                    && (aggregate || number <= &BigInt::from(i64::MAX)) =>
            {
                Some(number.clone())
            }
            _ => None,
        };
        let required = [
            valid(value.get(REQUIRED[0])?)?,
            valid(value.get(REQUIRED[1])?)?,
            valid(value.get(REQUIRED[2])?)?,
        ];
        if required[2] != &required[0] + &required[1] {
            return None;
        }
        let optional = std::array::from_fn(|index| {
            let number = valid(value.get(OPTIONAL[index])?)?;
            (index == 2 || number <= required[index]).then_some(number)
        });
        Some(Self { required, optional })
    }

    pub fn from_source(value: &Value) -> Option<Self> {
        if let Some(preferred) = value.get("cache_write_input_tokens") {
            let mut normalized = value.clone();
            normalized.set("cache_write_tokens", preferred.clone());
            Self::from_value(&normalized, false)
        } else {
            Self::from_value(value, false)
        }
    }

    pub fn value(&self) -> Value {
        Value::Object(
            REQUIRED
                .iter()
                .zip(&self.required)
                .map(|(name, number)| ((*name).into(), Value::Integer(number.clone())))
                .chain(OPTIONAL.iter().zip(&self.optional).map(|(name, number)| {
                    (
                        (*name).into(),
                        number
                            .as_ref()
                            .map_or(Value::Null, |number| Value::Integer(number.clone())),
                    )
                }))
                .collect(),
        )
    }

    pub fn zero() -> Self {
        Self {
            required: std::array::from_fn(|_| BigInt::zero()),
            optional: std::array::from_fn(|_| Some(BigInt::zero())),
        }
    }

    pub fn sum_into(&mut self, values: &Self) {
        for (target, value) in self.required.iter_mut().zip(&values.required) {
            *target += value;
        }
        for (target, value) in self.optional.iter_mut().zip(&values.optional) {
            if let (Some(target), Some(value)) = (target.as_mut(), value.as_ref()) {
                *target += value;
            } else {
                *target = None;
            }
        }
    }

    fn difference(&self, previous: &Self, partial: &mut Vec<String>) -> Self {
        let required =
            std::array::from_fn(|index| &self.required[index] - &previous.required[index]);
        let mut optional =
            std::array::from_fn(
                |index| match (&self.optional[index], &previous.optional[index]) {
                    (Some(now), Some(before)) if now >= before => Some(now - before),
                    _ => None,
                },
            );
        for index in 0..2 {
            if optional[index]
                .as_ref()
                .is_some_and(|value| value > &required[index])
            {
                optional[index] = None;
            }
        }
        for (name, value) in OPTIONAL.iter().zip(&optional) {
            if value.is_none() {
                add_sorted(partial, name);
            }
        }
        Self { required, optional }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Context {
    pub model: String,
    pub effort: String,
}

impl Context {
    pub fn from_value(value: &Value, known_models: &BTreeSet<String>) -> ValidationResult<Self> {
        if value.as_object().is_none() {
            return Err(super::errors::attribute_error(value, "get"));
        }
        let model = value
            .get("model")
            .and_then(Value::text)
            .filter(|model| known_models.contains(model))
            .unwrap_or_else(|| "unknown".into());
        let effort = value
            .get("effort")
            .or_else(|| value.get("reasoning_effort"))
            .and_then(Value::text)
            .filter(|effort| EFFORTS.contains(&effort.as_str()))
            .unwrap_or_else(|| "unknown".into());
        Ok(Self { model, effort })
    }

    pub fn value(&self) -> Value {
        Value::object([
            ("model", self.model.as_str().into()),
            ("effort", self.effort.as_str().into()),
        ])
    }

    pub fn unknown() -> Self {
        Self {
            model: "unknown".into(),
            effort: "unknown".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageSnapshot {
    pub offset: usize,
    pub counters: Counters,
    pub last: Option<Counters>,
    pub context: Context,
    pub ambiguous: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub data: Vec<u8>,
    pub source_id: Option<String>,
    pub snapshots: Vec<UsageSnapshot>,
    pub diagnostics: Vec<(usize, String)>,
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_bytes: usize,
    pub chunk_bytes: usize,
    pub max_records: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: MAX_BYTES,
            chunk_bytes: CHUNK_BYTES,
            max_records: MAX_RECORDS,
        }
    }
}

pub fn bounded_json(line: &[u8]) -> Result<Value, crate::compat::json::JsonError> {
    let (mut depth, mut quoted, mut escaped) = (0_i64, false, false);
    for byte in line {
        if quoted {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                quoted = false;
            }
        } else if *byte == b'"' {
            quoted = true;
        } else if *byte == b'[' || *byte == b'{' {
            depth += 1;
            if depth > 24 {
                return Err(crate::compat::json::JsonError {
                    kind: "ValueError",
                    message: String::new(),
                    position: 0,
                });
            }
        } else if *byte == b']' || *byte == b'}' {
            depth -= 1;
        }
    }
    crate::compat::loads_value(&Value::Bytes(line.to_vec()))
}

pub fn parse_source(
    data: Vec<u8>,
    failures: &[String],
    known_models: &BTreeSet<String>,
    limits: Limits,
) -> Source {
    let mut result = Source {
        data,
        source_id: None,
        snapshots: Vec::new(),
        diagnostics: failures.iter().map(|code| (0, code.clone())).collect(),
    };
    if !failures.is_empty() {
        return result;
    }
    let mut context = Context::unknown();
    let mut contexts = BTreeSet::new();
    let mut last_total = None;
    let (mut position, mut index) = (0, 0);
    while position < result.data.len() {
        let start = position;
        let end = result.data[start..]
            .iter()
            .position(|byte| *byte == b'\r' || *byte == b'\n')
            .map_or(result.data.len(), |offset| start + offset + 1);
        position = end;
        if result.data.get(end.wrapping_sub(1)) == Some(&b'\r')
            && result.data.get(end) == Some(&b'\n')
        {
            position += 1;
        }
        let line = &result.data[start..position];
        if !line.ends_with(b"\n") {
            break;
        }
        if index >= limits.max_records {
            result.diagnostics.push((position, "record_limit".into()));
            break;
        }
        index += 1;
        if line.len() > limits.chunk_bytes {
            result.diagnostics.push((position, "record_limit".into()));
            continue;
        }
        let Ok(record) = bounded_json(line) else {
            result
                .diagnostics
                .push((position, "malformed_record".into()));
            continue;
        };
        let Some(payload) = record
            .get("payload")
            .filter(|value| value.as_object().is_some())
        else {
            result
                .diagnostics
                .push((position, "malformed_record".into()));
            continue;
        };
        let kind = record.get("type");
        if kind.is_some_and(|kind| kind.is_text("session_meta")) {
            let identifier = payload.get("id").and_then(Value::as_string);
            let Some(identifier) = identifier.filter(|identifier| {
                (1..=128).contains(&identifier.len())
                    && identifier
                        .0
                        .iter()
                        .all(|point| matches!(*point, 48..=57 | 65..=90 | 97..=122 | 45 | 95))
            }) else {
                result
                    .diagnostics
                    .push((position, "missing_identity".into()));
                continue;
            };
            let hashed = crate::domain::ids::sha256_bytes(
                &identifier
                    .0
                    .iter()
                    .map(|point| *point as u8)
                    .collect::<Vec<_>>(),
            );
            if result
                .source_id
                .as_ref()
                .is_some_and(|current| current != &hashed)
            {
                result
                    .diagnostics
                    .push((position, "identity_changed".into()));
                result.source_id = None;
                return result;
            }
            result.source_id = Some(hashed);
        } else if kind.is_some_and(|kind| kind.is_text("turn_context")) {
            context = Context::from_value(payload, known_models)
                .expect("payload was checked as a mapping");
            contexts.insert(context.clone());
        } else if kind.is_some_and(|kind| kind.is_text("event_msg"))
            && payload
                .get("type")
                .is_some_and(|kind| kind.is_text("token_count"))
        {
            let info = payload.get("info");
            let counters = info
                .and_then(|info| info.get("total_token_usage"))
                .and_then(Counters::from_source);
            let Some(counters) = counters else {
                result
                    .diagnostics
                    .push((position, "unsupported_usage".into()));
                continue;
            };
            let ambiguous = contexts.len() > 1;
            result.snapshots.push(UsageSnapshot {
                offset: position,
                counters: counters.clone(),
                last: info
                    .and_then(|info| info.get("last_token_usage"))
                    .and_then(Counters::from_source),
                context: if ambiguous {
                    Context::unknown()
                } else {
                    context.clone()
                },
                ambiguous,
            });
            if last_total.as_ref() != Some(&counters) {
                contexts.clear();
                last_total = Some(counters);
            }
        }
    }
    if result.source_id.is_none() {
        result.diagnostics.push((0, "missing_identity".into()));
    }
    if result.snapshots.is_empty() {
        result.diagnostics.push((0, "missing_usage".into()));
    }
    result
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub phase: String,
    pub context: Context,
    pub totals: Option<Counters>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interval {
    pub source_id: Option<String>,
    pub phase: String,
    pub start_offset: usize,
    pub offset: usize,
    pub prefix_sha256: String,
    pub previous: Option<Counters>,
    pub observed: bool,
    pub closed: bool,
    pub broken: bool,
    pub diagnostics: Vec<String>,
    pub metric_partial: Vec<String>,
    pub segments: Vec<Segment>,
    pub boundary_kind: String,
}

impl Interval {
    fn add_diagnostic(&mut self, code: &str) {
        add_sorted(&mut self.diagnostics, code);
    }

    fn add_segment(
        &mut self,
        delta: Counters,
        snapshot: &UsageSnapshot,
        ambiguous: bool,
    ) -> ValidationResult<()> {
        let context = if ambiguous {
            Context::unknown()
        } else {
            snapshot.context.clone()
        };
        let phase = if ambiguous {
            self.add_diagnostic("ambiguous_increment");
            "unattributed".into()
        } else {
            self.phase.clone()
        };
        if context.model == "unknown" || context.effort == "unknown" {
            self.add_diagnostic("unknown_context");
        }
        for (name, value) in OPTIONAL.iter().zip(&delta.optional) {
            if value.is_none() {
                add_sorted(&mut self.metric_partial, name);
            }
        }
        if let Some(previous) = self.segments.last_mut() {
            if previous.phase == phase && previous.context == context {
                previous
                    .totals
                    .as_mut()
                    .ok_or_else(null_totals)?
                    .sum_into(&delta);
                return Ok(());
            }
        }
        self.segments.push(Segment {
            phase,
            context,
            totals: Some(delta),
        });
        Ok(())
    }

    pub fn advance(
        &mut self,
        source: &Source,
        closing: bool,
        boundary_kind: &str,
    ) -> ValidationResult<()> {
        let start = self.offset;
        if source.source_id.is_none() {
            for (_, code) in &source.diagnostics {
                self.add_diagnostic(code);
            }
        } else if source.source_id != self.source_id {
            self.add_diagnostic("identity_changed");
        } else if source.data.len() < start
            || crate::domain::ids::sha256_bytes(&source.data[..start]) != self.prefix_sha256
        {
            self.add_diagnostic("prefix_changed");
        } else if !self.broken {
            for (offset, code) in &source.diagnostics {
                if *offset == 0 || *offset > start {
                    self.add_diagnostic(code);
                }
            }
            let mut first = self.previous.is_none();
            let mut boundary = boundary_kind.to_owned();
            for snapshot in &source.snapshots {
                if snapshot.offset <= start {
                    continue;
                }
                self.observed = true;
                let mut delta = match &self.previous {
                    None => {
                        if snapshot
                            .last
                            .as_ref()
                            .is_some_and(|last| last.required == snapshot.counters.required)
                        {
                            Some(snapshot.counters.clone())
                        } else {
                            self.add_diagnostic("prefix_gap");
                            None
                        }
                    }
                    Some(previous)
                        if snapshot
                            .counters
                            .required
                            .iter()
                            .zip(&previous.required)
                            .any(|(now, before)| now < before) =>
                    {
                        self.add_diagnostic("counter_reset");
                        None
                    }
                    Some(previous) if previous.required == snapshot.counters.required => continue,
                    Some(previous) => Some(
                        snapshot
                            .counters
                            .difference(previous, &mut self.metric_partial),
                    ),
                };
                if let Some(values) = delta.as_ref() {
                    let mut ambiguous = snapshot.ambiguous;
                    let bridge = (first || boundary != "continuation") && self.previous.is_some();
                    if bridge
                        && snapshot
                            .last
                            .as_ref()
                            .is_none_or(|last| values.required != last.required)
                    {
                        if boundary == "baseline" {
                            delta = None;
                            self.add_diagnostic("prefix_gap");
                        } else {
                            ambiguous = true;
                        }
                    }
                    if let Some(delta) = delta {
                        self.add_segment(delta, snapshot, ambiguous)?;
                    }
                }
                self.previous = Some(snapshot.counters.clone());
                first = false;
                boundary = "continuation".into();
            }
            if let Some(last) = source.snapshots.last() {
                if last.offset >= start {
                    self.offset = last.offset;
                    self.prefix_sha256 =
                        crate::domain::ids::sha256_bytes(&source.data[..last.offset]);
                }
            }
            self.boundary_kind = boundary;
        }
        if ["identity_changed", "prefix_changed", "source_changed"]
            .iter()
            .any(|code| self.diagnostics.iter().any(|item| item == code))
        {
            self.broken = true;
        }
        if closing {
            self.closed = true;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub coverage: &'static str,
    pub observed_totals: Option<Counters>,
    pub complete_totals: Option<Counters>,
    pub diagnostics: BTreeSet<String>,
    pub metric_partial: BTreeSet<String>,
}

impl Summary {
    pub fn value(&self) -> Value {
        Value::object([
            ("coverage", self.coverage.into()),
            (
                "observed_totals",
                self.observed_totals
                    .as_ref()
                    .map_or(Value::Null, Counters::value),
            ),
            (
                "complete_totals",
                self.complete_totals
                    .as_ref()
                    .map_or(Value::Null, Counters::value),
            ),
            (
                "diagnostics",
                Value::list(
                    self.diagnostics
                        .iter()
                        .map(|item| Value::from(item.as_str())),
                ),
            ),
            (
                "metric_partial",
                Value::list(
                    self.metric_partial
                        .iter()
                        .map(|item| Value::from(item.as_str())),
                ),
            ),
        ])
    }
}

fn null_totals() -> ValidationFailure {
    ValidationFailure::python("TypeError", "'NoneType' object is not subscriptable")
}

pub fn summarize(
    intervals: &[&Interval],
    segments: &[&Segment],
    missing: bool,
) -> ValidationResult<Summary> {
    let observed = intervals.iter().any(|item| item.observed);
    let mut totals = Counters::zero();
    for segment in segments {
        totals.sum_into(segment.totals.as_ref().ok_or_else(null_totals)?);
    }
    let mut diagnostics = intervals
        .iter()
        .flat_map(|item| item.diagnostics.iter().cloned())
        .collect::<BTreeSet<_>>();
    let open = intervals.iter().any(|item| !item.closed);
    let complete = observed && !missing && diagnostics.is_empty() && !open;
    if open {
        diagnostics.insert("open_interval".into());
    }
    Ok(Summary {
        coverage: if complete {
            "complete"
        } else if observed {
            "partial"
        } else {
            "unavailable"
        },
        observed_totals: observed.then(|| totals.clone()),
        complete_totals: complete.then_some(totals),
        diagnostics,
        metric_partial: intervals
            .iter()
            .flat_map(|item| item.metric_partial.iter().cloned())
            .collect(),
    })
}

pub fn stage_phase(stage: &str) -> Option<&'static str> {
    Some(match stage {
        "bootstrap" => "scoping",
        "research" | "design" => "research_design",
        "implementation" => "implementation",
        "delivery" | "advisory" | "review_delegation" => "review_testing",
        "closure" => "closure",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counters(input: i64, output: i64) -> Counters {
        Counters {
            required: [input.into(), output.into(), (input + output).into()],
            optional: [Some(0.into()), Some(0.into()), Some(0.into())],
        }
    }

    fn interval(previous: Counters) -> Interval {
        Interval {
            source_id: Some("a".repeat(64)),
            phase: "scoping".into(),
            start_offset: 0,
            offset: 0,
            prefix_sha256: crate::domain::ids::sha256_bytes(b""),
            previous: Some(previous),
            observed: true,
            closed: false,
            broken: false,
            diagnostics: Vec::new(),
            metric_partial: Vec::new(),
            segments: Vec::new(),
            boundary_kind: "baseline".into(),
        }
    }

    fn source(samples: &[(i64, i64, i64, i64)]) -> Source {
        Source {
            data: vec![b'\n'; samples.len()],
            source_id: Some("a".repeat(64)),
            snapshots: samples
                .iter()
                .enumerate()
                .map(|(index, sample)| UsageSnapshot {
                    offset: index + 1,
                    counters: counters(sample.0, sample.1),
                    last: Some(counters(sample.2, sample.3)),
                    context: Context {
                        model: "gpt-6-astra".into(),
                        effort: "medium".into(),
                    },
                    ambiguous: false,
                })
                .collect(),
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn invalid_required_and_preferred_optional_values_stay_unavailable() {
        let mut value = counters(100, 20).value();
        value.set("input_tokens", true.into());
        assert!(Counters::from_value(&value, false).is_none());
        value = counters(100, 20).value();
        value.set("cache_write_tokens", 99_i64.into());
        value.set("cache_write_input_tokens", "invalid".into());
        assert_eq!(Counters::from_source(&value).unwrap().optional[2], None);
        let large = BigInt::from(i64::MAX) + BigInt::from(10);
        value = Value::object([
            ("input_tokens", Value::Integer(large.clone())),
            ("output_tokens", 0_i64.into()),
            ("total_tokens", Value::Integer(large.clone())),
        ]);
        assert!(Counters::from_value(&value, false).is_none());
        assert_eq!(
            Counters::from_value(&value, true).unwrap().required[2],
            large
        );
    }

    #[test]
    fn baseline_bridge_is_excluded_but_phase_bridge_is_unattributed() {
        let sampled = source(&[(30, 3, 30, 3)]);
        let mut baseline = interval(counters(10, 1));
        baseline.advance(&sampled, true, "baseline").unwrap();
        assert!(baseline.segments.is_empty());
        assert_eq!(baseline.diagnostics, ["prefix_gap"]);
        let mut phase = interval(counters(10, 1));
        phase.advance(&sampled, true, "phase").unwrap();
        assert_eq!(phase.segments[0].phase, "unattributed");
        assert_eq!(
            phase.segments[0].totals.as_ref().unwrap().required[2],
            BigInt::from(22)
        );
        assert_eq!(phase.segments[0].context, Context::unknown());
    }

    #[test]
    fn resets_preserve_prior_totals_and_repeated_samples_do_not_recount() {
        let sampled = source(&[(20, 2, 10, 1), (20, 2, 10, 1), (5, 1, 5, 1), (15, 2, 10, 1)]);
        let mut binding = interval(counters(10, 1));
        binding.advance(&sampled, true, "baseline").unwrap();
        assert_eq!(binding.segments.len(), 1);
        assert_eq!(
            binding.segments[0].totals.as_ref().unwrap().required[2],
            BigInt::from(22)
        );
        assert_eq!(binding.diagnostics, ["counter_reset"]);
        let summary = summarize(&[&binding], &[&binding.segments[0]], false).unwrap();
        assert_eq!(summary.coverage, "partial");
        assert!(summary.complete_totals.is_none());
        let prior = binding.segments.clone();
        let mut changed = sampled.clone();
        changed.data[0] = b'x';
        binding.advance(&changed, true, "continuation").unwrap();
        assert!(binding.broken);
        assert_eq!(binding.segments, prior);
        binding.advance(&sampled, true, "continuation").unwrap();
        assert_eq!(binding.segments, prior);
    }

    #[test]
    fn parser_uses_complete_lines_bounded_metadata_and_historical_context() {
        let mut bytes = Vec::new();
        let mut append = |value: Value| {
            bytes.extend(
                crate::compat::dumps(&value, crate::compat::SerializationProfile::STORED).unwrap(),
            );
            bytes.extend(b"\r\n");
        };
        append(Value::object([
            ("type", "session_meta".into()),
            (
                "payload",
                Value::object([("id", "synthetic-session".into())]),
            ),
        ]));
        append(Value::object([
            ("type", "turn_context".into()),
            (
                "payload",
                Value::object([
                    ("model", "gpt-5.6-terra".into()),
                    ("reasoning_effort", "ultra".into()),
                ]),
            ),
        ]));
        append(Value::object([
            ("type", "event_msg".into()),
            (
                "payload",
                Value::object([
                    ("type", "token_count".into()),
                    (
                        "info",
                        Value::object([
                            ("total_token_usage", counters(10, 1).value()),
                            ("last_token_usage", counters(10, 1).value()),
                        ]),
                    ),
                ]),
            ),
        ]));
        let complete = bytes.clone();
        bytes.pop();
        let unfinished = parse_source(bytes, &[], known_models(), Limits::default());
        assert!(unfinished.snapshots.is_empty());
        let parsed = parse_source(complete, &[], known_models(), Limits::default());
        assert_eq!(parsed.snapshots.len(), 1);
        assert_eq!(parsed.snapshots[0].context.model, "gpt-5.6-terra");
        assert_eq!(parsed.snapshots[0].context.effort, "ultra");
        assert_eq!(
            parsed.source_id,
            Some(crate::domain::ids::sha256_bytes(b"synthetic-session"))
        );
        assert!(parsed.diagnostics.is_empty());
        assert!(bounded_json(&[vec![b'['; 25], vec![b']'; 25]].concat()).is_err());
    }
}
