use super::{
    artifacts::ArtifactResolver,
    check_eligibility::CheckEligibilityPort,
    check_fences::{self, CheckClockPort},
    error::{EngineError, EngineResult},
    evidence_lifecycle,
    evidence_status::EvidenceStatusPort,
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    persistent_store::PersistentSession,
    policy,
    process::ProcessPort,
    semantic_state::SemanticContext,
    source_capture::{self, SourceFilePort, MAX_FILE_BYTES},
};
use crate::{
    compat::{self, PyString, Value},
    domain::{errors, python_operations},
};

pub struct EvidenceVerification<'a> {
    pub session: &'a dyn PersistentSession,
    pub run: &'a Value,
    pub context: SemanticContext<'a>,
    pub clock: &'a dyn CheckClockPort,
    pub source_files: &'a dyn SourceFilePort,
    pub process: &'a dyn ProcessPort,
}

fn item<'a>(value: &'a Value, key: &str) -> EngineResult<&'a Value> {
    Ok(errors::string_item(value, key)?)
}

fn optional<'a>(value: &'a Value, key: &str) -> EngineResult<&'a Value> {
    if value.as_object().is_none() {
        return Err(errors::attribute_error(value, "get").into());
    }
    Ok(value.get(key).unwrap_or(&compat::value::NULL))
}

fn positional(value: &Value, index: usize) -> EngineResult<Value> {
    let absent = |kind| EngineError::python("IndexError", format!("{kind} index out of range"));
    match value {
        Value::List(values) | Value::Tuple(values) => values
            .get(index)
            .cloned()
            .ok_or_else(|| absent(value.python_type())),
        Value::String(text) => text
            .0
            .get(index)
            .map(|point| Value::String(PyString(vec![*point])))
            .ok_or_else(|| absent("string")),
        Value::Bytes(bytes) => bytes
            .get(index)
            .map(|byte| i64::from(*byte).into())
            .ok_or_else(|| EngineError::python("IndexError", "index out of range")),
        Value::Object(_) => Err(EngineError::python("KeyError", index.to_string())),
        _ => Err(EngineError::python(
            "TypeError",
            format!("'{}' object is not subscriptable", value.python_type()),
        )),
    }
}

fn proposed_matches(proposed: &Value, target: &Value, output: &Value) -> EngineResult<bool> {
    if !proposed.truthy() {
        return Ok(false);
    }
    let reference = item(proposed, "ref")?;
    let reference = reference
        .as_string()
        .ok_or_else(|| errors::attribute_error(reference, "split"))?;
    let marker = PyString::from("#sha256=");
    let end = reference
        .0
        .windows(marker.len())
        .position(|window| window == marker.0)
        .unwrap_or(reference.len());
    let prefix = Value::String(PyString(reference.0[..end].to_vec()));
    Ok(prefix.py_eq(target) && item(proposed, "kind")?.py_eq(item(output, "artifact_kind")?))
}

impl CheckEligibilityPort for EvidenceVerification<'_> {
    fn verify_fence(&self, fence: &Value, executor: bool) -> EngineResult<()> {
        check_fences::verify_check_fence(self.session, self.clock, self.run, fence, executor)
            .map(|_| ())
    }

    fn verify_generated_outputs(
        &self,
        outputs: &Value,
        registering: Option<&Value>,
    ) -> EngineResult<()> {
        let repository =
            policy::path_from_value(self.context.files, item(self.run, "repository_path")?)?;
        for output in errors::iterable_values(outputs)? {
            let path = item(&output, "path")?;
            let target = python_operations::add(&"repo:".into(), path)?;
            let Value::String(target_text) = &target else {
                unreachable!("string concatenation returns a string")
            };
            let relative = PyString(target_text.0[5..].to_vec());
            self.source_files.normalize_target(&relative)?;
            let Some(metadata) = self.source_files.metadata(&relative)? else {
                continue;
            };
            if !metadata.regular || metadata.symlink || metadata.size > MAX_FILE_BYTES {
                return Err(EngineError::state("SOURCE_UNVERIFIABLE"));
            }
            let mut reference = None;
            if let Some(registering) = registering.filter(|value| value.truthy()) {
                if positional(registering, 0)?.py_eq(item(&output, "producer_node_key")?) {
                    let proposed = positional(registering, 1)?;
                    if proposed_matches(&proposed, &target, &output)? {
                        reference = Some(proposed);
                    }
                }
            }
            if reference.is_none() {
                for branch in self.session.all(
                    GraphRead::GeneratedProducerResults,
                    &[
                        item(self.run, "run_id")?.clone(),
                        item(&output, "producer_node_key")?.clone(),
                        item(self.run, "implementation_generation")?.clone(),
                    ],
                )? {
                    let result = compat::loads_value(item(&branch, "result_json")?)?;
                    let proposed = optional(&result, "artifact_ref")?;
                    if proposed_matches(proposed, &target, &output)? {
                        reference = Some(proposed.clone());
                    }
                }
            }
            if reference.is_none() && item(&output, "purpose")?.is_text("acceptance_wrapper") {
                for row in self.session.all(
                    GraphRead::GeneratedAcceptanceWrappers,
                    &[item(self.run, "run_id")?.clone()],
                )? {
                    let content = item(&row, "content_json")?;
                    let empty = Value::from("{}");
                    let wrapper =
                        compat::loads_value(if content.truthy() { content } else { &empty })?;
                    let proposed = optional(&wrapper, "underlying_evidence")?;
                    if proposed_matches(proposed, &target, &output)? {
                        reference = Some(proposed.clone());
                    }
                }
            }
            let reference =
                reference.ok_or_else(|| EngineError::state("GENERATED_OUTPUT_UNREGISTERED"))?;
            ArtifactResolver {
                files: self.context.files,
                snapshots: self.context.snapshots,
                repository: &repository,
                skill: self.context.skill,
                installation: self.context.installation,
                policy: self.context.policy,
                runtime: None,
                ledger: Some(self.session),
            }
            .resolve_reference(
                item(&reference, "ref")?,
                item(&reference, "sha256")?,
                item(&reference, "kind")?,
            )?;
        }
        Ok(())
    }

    fn capture_source(&self, roots: &Value, outputs: &Value) -> EngineResult<Value> {
        let (_, activation) = evidence_lifecycle::evidence_activation(self.session, self.run)?;
        let checks = item(&activation, "checks")?;
        let checks = checks
            .as_object()
            .ok_or_else(|| errors::attribute_error(checks, "values"))?;
        let mut known: Vec<Value> = Vec::new();
        for (_, paths) in checks {
            for path in errors::iterable_values(paths)? {
                errors::require_hashable(&path)?;
                if !known
                    .iter()
                    .any(|existing| errors::container_equal(existing, &path))
                {
                    known.push(path);
                }
            }
        }
        python_operations::sort_comparable_values(&mut known)?;
        let repository =
            policy::path_from_value(self.context.files, item(self.run, "repository_path")?)?;
        source_capture::capture_source_values(
            &repository,
            roots,
            outputs,
            &Value::List(known),
            self.source_files,
            self.process,
        )
    }

    fn current_review_binding(&self) -> EngineResult<Option<(Value, Value)>> {
        evidence_lifecycle::current_review_binding(self.session, self.run)
    }
}

impl GraphLedgerReadPort for EvidenceVerification<'_> {
    fn first(&self, query: GraphRead, parameters: &[Value]) -> EngineResult<Option<Value>> {
        self.session.first(query, parameters)
    }
    fn all(&self, query: GraphRead, parameters: &[Value]) -> EngineResult<Vec<Value>> {
        self.session.all(query, parameters)
    }
    fn visit(
        &self,
        query: GraphRead,
        parameters: &[Value],
        visit: &mut dyn FnMut(Value) -> EngineResult<()>,
    ) -> EngineResult<()> {
        self.session.visit(query, parameters, visit)
    }
}

impl EvidenceStatusPort for EvidenceVerification<'_> {
    fn reservation_states(&self, run: &Value) -> EngineResult<Vec<Value>> {
        Ok(evidence_lifecycle::reservation_states(self.session, item(run, "run_id")?)?.values())
    }
}
