use super::{
    artifacts::persist_artifact,
    claim_runtime::ClaimServices,
    error::{
        EngineError, EngineResult, ManifestJsonTraceSite, MutationValidationPhase,
        PersistedTaskValidation, TaskValidationMutation,
    },
    graph_ledger::GraphRead,
    graph_writes::GraphWrite,
    manifest_snapshot::inbox_snapshot,
    persistent_store::PersistentSession,
    semantic_state::{verify_semantic_state_with_trace, SemanticContext},
    source_capture::source_roots,
    state::mutate_persistent,
};
use crate::{
    compat::{PyString, Value},
    domain::{
        contracts::{self, ContractError},
        errors, evidence, source_paths,
    },
};
use std::collections::BTreeSet;

fn item<'a>(value: &'a Value, key: &str) -> EngineResult<&'a Value> {
    Ok(errors::string_item(value, key)?)
}
fn fail(code: &str) -> EngineError {
    ContractError::new("coverage", code).into()
}

pub fn enable(
    session: &dyn PersistentSession,
    run_id: &str,
    arguments: &Value,
    stored_policy: &Value,
    task: &Value,
    context: &SemanticContext<'_>,
    services: &ClaimServices<'_>,
) -> EngineResult<Value> {
    let snapshot = inbox_snapshot(
        context,
        stored_policy,
        run_id,
        item(arguments, "coverage_manifest")?,
        (256 * 1024).into(),
        ManifestJsonTraceSite::EvidenceEnable,
    )?;
    let value = snapshot.parsed.as_ref().expect("JSON snapshot is parsed");
    contracts::require_keys(
        value,
        &["schema_version", "kind", "checks"],
        &["schema_version", "kind", "checks"],
        "coverage",
    )?;
    if !item(arguments, "contract_version")?.py_eq(&2_i64.into())
        || !item(value, "schema_version")?.py_eq(&1_i64.into())
        || !item(value, "kind")?.is_text("check_coverage")
    {
        return Err(fail("SCHEMA_MISMATCH"));
    }
    let checks = item(value, "checks")?
        .as_list()
        .ok_or_else(|| fail("SCHEMA_MISMATCH"))?;
    if !(1..=32).contains(&checks.len()) {
        return Err(fail("SCHEMA_MISMATCH"));
    }
    let source_files =
        services
            .source_files
            .create(context.repository, stored_policy, context.files);
    let roots = source_roots(
        &*source_files,
        item(item(task, "authority")?, "capabilities")?,
    )?;
    let root_values = errors::iterable_values(&roots)?;
    let required = errors::iterable_values(item(task, "required_check_ids")?)?;
    let mut selected: Vec<(PyString, Value)> = Vec::new();
    for check in checks {
        contracts::require_keys(
            check,
            &["check_id", "relevant_inputs", "complete"],
            &["check_id", "relevant_inputs", "complete"],
            "coverage.check",
        )?;
        let check_id = item(check, "check_id")?;
        if !required.iter().any(|value| value.py_eq(check_id))
            || selected
                .iter()
                .any(|(key, _)| Value::String(key.clone()).py_eq(check_id))
            || !matches!(item(check, "complete")?, Value::Bool(true))
        {
            return Err(fail("COVERAGE_ATTESTATION_REQUIRED"));
        }
        let inputs = item(check, "relevant_inputs")?
            .as_list()
            .ok_or_else(|| fail("INVALID_LIST"))?;
        if !(1..=16).contains(&inputs.len()) {
            return Err(fail("INVALID_LIST"));
        }
        let mut normalized = BTreeSet::new();
        for input in inputs {
            normalized.insert(contracts::lexical_relative(input, "relevant_inputs")?);
        }
        for input in &normalized {
            if !root_values.iter().any(|root| {
                source_paths::contains(
                    root.as_string().expect("normalized root"),
                    input,
                    context.store.platform.windows(),
                )
            }) {
                return Err(EngineError::state("SOURCE_UNVERIFIABLE"));
            }
        }
        let key = check_id
            .as_string()
            .ok_or_else(|| fail("COVERAGE_ATTESTATION_REQUIRED"))?
            .clone();
        selected.push((
            key,
            Value::List(normalized.into_iter().map(Value::String).collect()),
        ));
    }
    let required = errors::set_values(&required)?;
    if selected.len() != required.len()
        || required.iter().any(|required| {
            !selected
                .iter()
                .any(|(key, _)| Value::String(key.clone()).py_eq(required))
        })
    {
        return Err(fail("CHECK_SET_MISMATCH"));
    }
    let checks = Value::Object(selected);
    let request = Value::object([
        ("command", "evidence.enable".into()),
        ("contract_version", 2_i64.into()),
        ("coverage_manifest_digest", snapshot.digest.clone().into()),
        ("checks", checks.clone()),
        ("roots", roots.clone()),
    ]);
    let operation = contracts::opaque(item(arguments, "op_id")?, "op_id")?;
    let mut validation_phase = MutationValidationPhase::BeforeAction;
    mutate_persistent(
        session,
        run_id,
        &operation,
        &request,
        |session, current, _| {
            let current = &current.fields;
            if !item(current, "state_schema_version")?.py_eq(&6_i64.into()) {
                return Err(EngineError::state("EVIDENCE_ALREADY_ENABLED"));
            }
            let plan = session
                .first(
                    GraphRead::ExecutionPlan,
                    std::slice::from_ref(item(current, "run_id")?),
                )?
                .ok_or_else(|| {
                    EngineError::python("TypeError", "'NoneType' object is not subscriptable")
                })?;
            if !item(&plan, "status")?.is_text("approved") {
                return Err(EngineError::state("EXECUTION_PLAN_NOT_APPROVED"));
            }
            let activation = Value::object([
                ("kind", "evidence_activation".into()),
                ("schema_version", 1_i64.into()),
                ("original_format", 6_i64.into()),
                ("run_id", item(current, "run_id")?.clone()),
                ("task_digest", item(current, "task_digest")?.clone()),
                ("plan_digest", item(&plan, "plan_digest")?.clone()),
                ("policy_digest", item(current, "policy_digest")?.clone()),
                ("actor", Value::String(context.provenance.actor()?)),
                (
                    "host_identity",
                    context.store.platform.host_identity()?.into(),
                ),
                ("checks", checks.clone()),
                ("coverage_manifest_digest", snapshot.digest.clone().into()),
                ("complete", true.into()),
                ("roots", roots.clone()),
            ]);
            let artifact = evidence::canonical_ledger_artifact(
                &"evidence-activation".into(),
                &"evidence_manifest".into(),
                &activation,
            )?;
            evidence::enforce_artifact_size(&artifact.kind, &artifact.size_bytes, stored_policy)?;
            persist_artifact(session, run_id, &artifact)?;
            session.write(
                GraphWrite::EnableEvidence,
                std::slice::from_ref(item(current, "run_id")?),
            )?;
            Ok(Value::object([
                ("code", "EVIDENCE_ENABLED".into()),
                ("artifact_ref", artifact.reference),
                ("state_schema_version", 7_i64.into()),
            ]))
        },
        |session, current| {
            let mut semantic = *context;
            semantic.evidence_enable =
                item(&current.fields, "state_schema_version")?.py_eq(&6_i64.into());
            let site = PersistedTaskValidation::Mutation(
                TaskValidationMutation::EvidenceEnable,
                validation_phase,
            );
            validation_phase = MutationValidationPhase::AfterAction;
            verify_semantic_state_with_trace(session, &current.fields, &semantic, Some(site))
        },
        context.store.fault,
    )
}
