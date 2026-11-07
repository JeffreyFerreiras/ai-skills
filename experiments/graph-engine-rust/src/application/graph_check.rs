use super::{
    artifacts::{persist_artifact, ArtifactResolver},
    check_execution::{run_check, CheckExecution, CheckRuntimePort},
    check_receipts::validate_check_receipt,
    error::{EngineError, EngineResult, TaskValidationMutation},
    graph_ledger::GraphRead,
    graph_payload::persist_payload,
    graph_writes::GraphWrite,
    persistent_store::PersistentSession,
    process::ProcessPort,
    run_context::RunContext,
    semantic_state::{mutation_task_validator, SemanticContext},
    state::mutate_persistent,
};
use crate::{
    compat::{self, Value},
    domain::{check_receipts::configured_check, contract_values::contains, contracts, errors},
};
use std::{path::Path, time::Duration};

fn require_check(task: &Value, check_id: &Value) -> EngineResult<()> {
    if !contains(errors::string_item(task, "required_check_ids")?, check_id)? {
        return Err(EngineError::state("UNKNOWN_CHECK"));
    }
    Ok(())
}

pub(crate) fn run_configured_check(
    repository: &Path,
    run_id: &Value,
    check_id: &Value,
    configured: &Value,
    process: &dyn ProcessPort,
    runtime: &dyn CheckRuntimePort,
    legacy_digest: bool,
) -> EngineResult<Value> {
    let argv = configured["argv"]
        .as_list()
        .expect("configured check has an argument list")
        .iter()
        .map(|value| value.as_string().expect("validated check argument").clone())
        .collect::<Vec<_>>();
    let timeout = configured.get("timeout_seconds").map_or(300, |value| {
        value.as_i64().expect("validated check timeout") as u64
    });
    run_check(
        &CheckExecution {
            repository,
            run_id,
            check_id,
            command_id: &configured["command_id"],
            arguments: &argv,
            timeout: Duration::from_secs(timeout),
            legacy_digest,
        },
        process,
        runtime,
    )
}

pub fn run_legacy(
    session: &dyn PersistentSession,
    run_id: &str,
    arguments: &Value,
    loaded: &RunContext,
    context: &SemanticContext<'_>,
    process: &dyn ProcessPort,
    runtime: &dyn CheckRuntimePort,
) -> EngineResult<Value> {
    let check_id = contracts::opaque(&arguments["check_id"], "check_id")?;
    let check = Value::from(check_id.as_str());
    require_check(&loaded.task, &check)?;
    let configured = configured_check(&loaded.policy, &check)?;
    let command = contracts::opaque(&configured["command_id"], "command_id")?;
    let request = Value::object([
        ("command", "check.run".into()),
        ("check_id", check.clone()),
        ("command_id", command.into()),
        ("argv", configured["argv"].clone()),
    ]);
    let operation = contracts::opaque(&arguments["op_id"], "op_id")?;
    mutate_persistent(
        session,
        run_id,
        &operation,
        &request,
        |session, current, _| {
            let run = &current.fields;
            let stored_run_id = errors::string_item(run, "run_id")?;
            if session
                .first(
                    GraphRead::CheckEvidenceForRun,
                    &[stored_run_id.clone(), check.clone()],
                )?
                .is_some()
            {
                return Err(EngineError::state("EVIDENCE_CONFLICT"));
            }
            let receipt = run_configured_check(
                context.repository,
                stored_run_id,
                &check,
                configured,
                process,
                runtime,
                true,
            )?;
            validate_check_receipt(&receipt, stored_run_id, &check, configured, runtime, true)?;
            let artifact = persist_payload(
                session,
                run,
                &loaded.policy,
                &format!("check-{check_id}-{operation}").into(),
                &"check_evidence".into(),
                &receipt,
            )?;
            session.write(
                GraphWrite::CheckEvidence,
                &[
                    stored_run_id.clone(),
                    check.clone(),
                    receipt["outcome"].clone(),
                    artifact.reference.clone(),
                    artifact.sha256.clone(),
                ],
            )?;
            Ok(Value::object([
                ("code", "CHECK_RECORDED".into()),
                ("check_id", check.clone()),
                ("outcome", receipt["outcome"].clone()),
                ("artifact_ref", artifact.reference),
                ("artifact_sha256", artifact.sha256),
            ]))
        },
        mutation_task_validator(context, TaskValidationMutation::LegacyCheckRun),
        context.store.fault,
    )
}

pub fn record_legacy_evidence(
    session: &dyn PersistentSession,
    run_id: &str,
    arguments: &Value,
    loaded: &RunContext,
    context: &SemanticContext<'_>,
) -> EngineResult<Value> {
    let operation = contracts::opaque(&arguments["op_id"], "op_id")?;
    let request = Value::object([
        ("command", "record.check-evidence".into()),
        (
            "artifact_ref",
            Value::String(contracts::validate_ref(
                &arguments["artifact_ref"],
                "artifact_ref",
                true,
            )?),
        ),
        (
            "artifact_sha256",
            contracts::digest(&arguments["artifact_sha256"], "artifact_sha256")?.into(),
        ),
        (
            "check_id",
            contracts::opaque(&arguments["check_id"], "check_id")?.into(),
        ),
        ("outcome", arguments["outcome"].clone()),
    ]);
    mutate_persistent(
        session,
        run_id,
        &operation,
        &request,
        |session, current, _| {
            let check = &request["check_id"];
            require_check(&loaded.task, check)?;
            let resolver = ArtifactResolver {
                files: context.files,
                snapshots: context.snapshots,
                repository: context.repository,
                skill: context.skill,
                installation: context.installation,
                policy: &loaded.policy,
                runtime: context.runtime,
                ledger: Some(session),
            };
            let artifact = resolver.resolve_reference(
                &request["artifact_ref"],
                &request["artifact_sha256"],
                &"check_evidence".into(),
            )?;
            if !artifact.source_type.is_text("ledger") || !artifact.content_json.truthy() {
                return Err(EngineError::state("CHECK_RECEIPT_REQUIRED"));
            }
            let receipt = match compat::loads_value(&artifact.content_json) {
                Ok(receipt) => receipt,
                Err(error) if error.kind == "JSONDecodeError" => {
                    return Err(EngineError::state("CHECK_RECEIPT_REQUIRED"));
                }
                Err(error) => return Err(error.into()),
            };
            let expected = configured_check(&loaded.policy, check)?;
            let stored_run_id = errors::string_item(&current.fields, "run_id")?;
            validate_check_receipt(
                &receipt,
                stored_run_id,
                check,
                expected,
                context.provenance,
                true,
            )?;
            persist_artifact(session, run_id, &artifact)?;
            let values = [
                request["outcome"].clone(),
                artifact.reference,
                artifact.sha256,
            ];
            let existing = session.first(
                GraphRead::CheckEvidenceForRun,
                &[stored_run_id.clone(), check.clone()],
            )?;
            if let Some(existing) = existing {
                for (key, expected) in ["outcome", "artifact_ref", "artifact_sha256"]
                    .iter()
                    .zip(values)
                {
                    if !errors::string_item(&existing, key)?.container_eq(&expected) {
                        return Err(EngineError::state("EVIDENCE_CONFLICT"));
                    }
                }
            } else {
                let [outcome, reference, digest] = values;
                session.write(
                    GraphWrite::CheckEvidence,
                    &[
                        stored_run_id.clone(),
                        check.clone(),
                        outcome,
                        reference,
                        digest,
                    ],
                )?;
            }
            Ok(Value::object([("code", "CHECK_EVIDENCE_RECORDED".into())]))
        },
        mutation_task_validator(context, TaskValidationMutation::RecordControl),
        context.store.fault,
    )
}
