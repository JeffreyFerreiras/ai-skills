use super::{
    error::{EngineError, EngineResult},
    state::{self, FaultHook, GraphSession, RunRecord},
    usage_bindings::{self, AssociationArguments},
    usage_report,
    usage_source::{self, UsageSourcePort},
    usage_store::UsageSession,
};
use crate::{
    compat::{self, SerializationProfile, Value},
    domain::{
        contracts, ids,
        usage::{Interval, Limits, Source},
        usage_contracts::{self, Binding},
    },
};
use num_bigint::BigInt;
use num_traits::ToPrimitive;
use std::path::Path;

pub struct UsageArguments<'a> {
    pub operation_id: &'a Value,
    pub action: &'a Value,
    pub session_log: &'a Path,
    pub start_offset: &'a Value,
    pub source_id: &'a Value,
    pub prefix_sha256: &'a Value,
    pub binding_id: &'a Value,
    pub branch_id: &'a Value,
    pub attempt_id: &'a Value,
    pub phase: &'a Value,
    pub generation: &'a Value,
}

pub struct PreparedUsage {
    pub run_id: String,
    pub operation_id: String,
    pub request: Value,
    source: Source,
    supplied: Option<Value>,
    association: Option<Value>,
    binding_id: String,
    previous_digest: String,
}

pub(crate) struct UsagePrelude {
    operation_id: String,
    has_checkpoint: bool,
}

pub(crate) fn prelude(operation: &Value, historical: [&Value; 3]) -> EngineResult<UsagePrelude> {
    let operation_id = contracts::opaque(operation, "op_id")?;
    let supplied_count = historical.iter().filter(|value| !value.is_null()).count();
    if supplied_count != 0 && supplied_count != historical.len() {
        return Err(EngineError::state("USAGE_CHECKPOINT_INVALID"));
    }
    Ok(UsagePrelude {
        operation_id,
        has_checkpoint: supplied_count != 0,
    })
}

pub fn prepare(
    args: UsageArguments<'_>,
    session: &dyn UsageSession,
    run_id: &str,
    run: &RunRecord,
    files: &dyn UsageSourcePort,
) -> EngineResult<PreparedUsage> {
    let validated = prelude(
        args.operation_id,
        [args.start_offset, args.source_id, args.prefix_sha256],
    )?;
    prepare_after_prelude(args, session, run_id, run, files, validated)
}

pub(crate) fn prepare_after_prelude(
    args: UsageArguments<'_>,
    session: &dyn UsageSession,
    run_id: &str,
    run: &RunRecord,
    files: &dyn UsageSourcePort,
    validated: UsagePrelude,
) -> EngineResult<PreparedUsage> {
    let operation_id = validated.operation_id;
    let source = usage_source::read_source(files, args.session_log, Limits::default())?;
    let supplied = if validated.has_checkpoint {
        Some(usage_contracts::validate_checkpoint(
            &source,
            &Value::object([
                ("offset", args.start_offset.clone()),
                ("source_id", args.source_id.clone()),
                ("prefix_sha256", args.prefix_sha256.clone()),
            ]),
        )?)
    } else {
        None
    };
    let mut request = Value::object([
        ("command", "record.usage".into()),
        ("action", args.action.clone()),
        (
            "source_id",
            source
                .source_id
                .as_ref()
                .map_or(Value::Null, |source| source.as_str().into()),
        ),
    ]);
    let (association, binding_id) = if args.action.is_text("bind") {
        if !args.binding_id.is_null() {
            return Err(EngineError::state("USAGE_ARGUMENT_INVALID"));
        }
        let association = usage_bindings::association(
            session,
            &run.fields["run_id"],
            AssociationArguments {
                branch_id: args.branch_id,
                attempt_id: args.attempt_id,
                phase: args.phase,
                generation: args.generation,
            },
        )?;
        if let Value::Object(items) = &association {
            for (key, value) in items {
                request.set(&key.utf8().expect("fixed association key"), value.clone());
            }
        }
        request.set("checkpoint", supplied.clone().unwrap_or(Value::Null));
        let binding_id = ids::stable_id_values(
            &run.fields["run_id"],
            &run.fields["policy_digest"],
            "usage",
            &operation_id,
            &association["generation"],
            None,
        )?;
        (Some(association), binding_id)
    } else {
        if supplied.is_some()
            || [args.phase, args.generation, args.branch_id, args.attempt_id]
                .iter()
                .any(|value| !value.is_null())
        {
            return Err(EngineError::state("USAGE_ARGUMENT_INVALID"));
        }
        (None, contracts::opaque(args.binding_id, "binding_id")?)
    };
    request.set("binding_id", binding_id.as_str().into());
    let initial =
        usage_bindings::load_bindings(session, &run.fields["run_id"]).map_err(|error| {
            if usage_bindings::optional_corruption(&error) {
                EngineError::state("USAGE_STATE_INVALID")
            } else {
                error
            }
        })?;
    let previous_digest = ids::sha256_bytes(&ids::canonical_bytes(
        &usage_bindings::bindings_value(&initial),
    )?);
    Ok(PreparedUsage {
        run_id: run_id.into(),
        operation_id,
        request,
        source,
        supplied,
        association,
        binding_id,
        previous_digest,
    })
}

fn offset(value: &Value) -> usize {
    value
        .strict_integer()
        .and_then(ToPrimitive::to_usize)
        .expect("validated source offset fits memory")
}

impl PreparedUsage {
    fn new_binding(&self, current: &[Binding]) -> EngineResult<Binding> {
        let selected = match &self.supplied {
            Some(value) => value.clone(),
            None if self.source.source_id.is_some() => self.source.checkpoint(None)?,
            None => Value::object([
                ("offset", 0_i64.into()),
                ("prefix_sha256", ids::sha256_bytes(b"").into()),
            ]),
        };
        let start = offset(&selected["offset"]);
        let prior = self
            .source
            .snapshots
            .iter()
            .find(|snapshot| snapshot.offset == start);
        let contiguous = current.iter().any(|binding| {
            binding.interval.source_id == self.source.source_id
                && binding.interval.closed
                && binding.interval.offset == start
        });
        let association = self
            .association
            .as_ref()
            .expect("bind has a prepared association");
        let mut raw = Value::object([
            ("usage_schema_version", 1_i64.into()),
            ("binding_id", self.binding_id.as_str().into()),
            ("adapter", "codex_jsonl_v1".into()),
            (
                "source_id",
                self.source
                    .source_id
                    .as_ref()
                    .map_or(Value::Null, |value| value.as_str().into()),
            ),
        ]);
        if let Value::Object(items) = association {
            for (key, value) in items {
                raw.set(&key.utf8().expect("fixed association key"), value.clone());
            }
        }
        let interval = Interval {
            source_id: self.source.source_id.clone(),
            phase: association["phase"].text().expect("validated phase"),
            start_offset: start,
            offset: start,
            prefix_sha256: selected["prefix_sha256"].text().expect("validated digest"),
            previous: prior.map(|snapshot| snapshot.counters.clone()),
            observed: prior.is_some(),
            closed: false,
            broken: false,
            diagnostics: Vec::new(),
            metric_partial: Vec::new(),
            segments: Vec::new(),
            boundary_kind: if contiguous { "phase" } else { "baseline" }.into(),
        };
        raw.set("start_offset", Value::Integer(start.into()));
        let mut binding = Binding { raw, interval };
        let boundary = binding.interval.boundary_kind.clone();
        binding.interval.advance(&self.source, false, &boundary)?;
        Ok(binding)
    }

    pub fn apply(
        &self,
        session: &dyn UsageSession,
        current: &RunRecord,
        revision: &BigInt,
    ) -> EngineResult<Value> {
        let current_bindings = usage_bindings::load_bindings(session, &current.fields["run_id"])?;
        if ids::sha256_bytes(&ids::canonical_bytes(&usage_bindings::bindings_value(
            &current_bindings,
        ))?) != self.previous_digest
        {
            return Err(EngineError::state("USAGE_STALE_CHECKPOINT"));
        }
        let binding = if self.request["action"].is_text("bind") {
            self.new_binding(&current_bindings)?
        } else {
            let mut binding = current_bindings
                .iter()
                .find(|binding| binding.raw["binding_id"].is_text(&self.binding_id))
                .cloned()
                .ok_or_else(|| EngineError::state("USAGE_BINDING_NOT_FOUND"))?;
            if binding.interval.closed {
                return Err(EngineError::state("USAGE_BINDING_CLOSED"));
            }
            let boundary = binding.interval.boundary_kind.clone();
            binding.interval.advance(
                &self.source,
                self.request["action"].is_text("close"),
                &boundary,
            )?;
            binding
        };
        for other in &current_bindings {
            if binding.interval.source_id.is_none()
                || other.raw["binding_id"].is_text(&self.binding_id)
                || other.interval.source_id != binding.interval.source_id
            {
                continue;
            }
            let end = if binding.interval.closed {
                binding.interval.offset
            } else {
                i64::MAX as usize
            };
            let other_end = if other.interval.closed {
                other.interval.offset
            } else {
                i64::MAX as usize
            };
            if binding.interval.start_offset < other_end && other.interval.start_offset < end {
                return Err(EngineError::state("USAGE_INTERVAL_OVERLAP"));
            }
        }
        let value = binding.value();
        usage_contracts::validate_binding(&value)?;
        session.insert_usage_observation(
            &self.run_id,
            revision,
            &self.binding_id,
            &compat::dumps(&value, SerializationProfile::SORTED_STORED)?,
        )?;
        Ok(Value::object([
            (
                "code",
                if binding.interval.source_id.is_some() {
                    "USAGE_RECORDED"
                } else {
                    "USAGE_UNAVAILABLE"
                }
                .into(),
            ),
            ("binding_id", self.binding_id.as_str().into()),
            (
                "checkpoint",
                binding
                    .interval
                    .source_id
                    .as_ref()
                    .map_or(Value::Null, |source| {
                        Value::object([
                            ("checkpoint_schema_version", 1_i64.into()),
                            ("source_id", source.as_str().into()),
                            ("offset", Value::Integer(binding.interval.offset.into())),
                            (
                                "prefix_sha256",
                                binding.interval.prefix_sha256.as_str().into(),
                            ),
                        ])
                    }),
            ),
            ("usage", usage_report::report(session, &current.fields)?),
        ]))
    }

    pub fn execute<V>(
        &self,
        session: &dyn UsageSession,
        semantic: V,
        fault: &dyn FaultHook,
    ) -> EngineResult<Value>
    where
        V: FnMut(&dyn GraphSession, &RunRecord) -> EngineResult<()>,
    {
        state::mutate(
            session,
            &self.run_id,
            &self.operation_id,
            &self.request,
            |_, current, revision| self.apply(session, current, revision),
            semantic,
            fault,
        )
    }
}
