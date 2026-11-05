use super::{
    artifacts::{persist_artifact, ArtifactResolver},
    error::{EngineError, EngineResult, TaskValidationMutation},
    graph_ledger::GraphRead,
    graph_writes::GraphWrite,
    persistent_store::PersistentSession,
    semantic_state::{mutation_task_validator, SemanticContext},
    state::mutate_persistent,
};
use crate::{
    compat::Value,
    domain::{contracts, errors, ids},
};

pub fn record(
    session: &dyn PersistentSession,
    run_id: &str,
    arguments: &Value,
    policy: &Value,
    task: &Value,
    context: &SemanticContext<'_>,
) -> EngineResult<Value> {
    let item = |key| errors::string_item(arguments, key);
    let operation = contracts::opaque(item("op_id")?, "op_id")?;
    let mut request = Value::object([("command", "record.approval".into())]);
    if let Some(branch) = arguments.get("branch_id") {
        request.set("branch_id", branch.clone());
    }
    request.set(
        "approval_id",
        contracts::opaque(item("approval_id")?, "approval_id")?.into(),
    );
    request.set(
        "scope_ref",
        Value::String(contracts::validate_ref(
            item("scope_ref")?,
            "scope_ref",
            true,
        )?),
    );
    request.set("decision", item("decision")?.clone());
    request.set(
        "authority_ref",
        Value::String(contracts::validate_ref(
            item("authority_ref")?,
            "authority_ref",
            false,
        )?),
    );
    request.set(
        "artifact_sha256",
        contracts::digest(item("artifact_sha256")?, "artifact_sha256")?.into(),
    );
    let actor = if item("actor")?.is_null() {
        context.provenance.actor()?
    } else {
        contracts::bounded_string(item("actor")?, "actor", 256)?.clone()
    };
    request.set("actor", Value::String(actor));
    if !request["actor"].py_eq(&Value::String(context.provenance.actor()?)) {
        return Err(EngineError::state("APPROVAL_ACTOR_MISMATCH"));
    }
    mutate_persistent(
        session,
        run_id,
        &operation,
        &request,
        |session, current, _| {
            let required =
                errors::iterable_set(errors::string_item(task, "required_human_decisions")?)?;
            if !errors::set_contains(&required, &request["approval_id"]) {
                return Err(EngineError::state("UNEXPECTED_APPROVAL"));
            }
            let resolver = ArtifactResolver {
                files: context.files,
                snapshots: context.snapshots,
                repository: context.repository,
                skill: context.skill,
                installation: context.installation,
                policy,
                runtime: context.runtime,
                ledger: Some(session),
            };
            let verified = resolver.resolve_reference(
                &request["scope_ref"],
                &request["artifact_sha256"],
                &"acceptance_evidence".into(),
            )?;
            persist_artifact(session, run_id, &verified)?;
            let run = errors::string_item(&current.fields, "run_id")?;
            let existing = session.first(
                GraphRead::ExistingApproval,
                &[run.clone(), request["approval_id"].clone()],
            )?;
            let keys = ["scope_ref", "decision", "authority_ref", "artifact_sha256"];
            if let Some(existing) = existing {
                for key in keys {
                    if !errors::string_item(&existing, key)?.py_eq(&request[key]) {
                        return Err(EngineError::state("APPROVAL_CONFLICT"));
                    }
                }
            } else {
                let approved_at = Value::from(context.store.platform.utc_now()?);
                let host = Value::from(context.store.platform.host_identity()?);
                let digest = ids::sha256_bytes(&ids::canonical_bytes(&Value::object([
                    ("run_id", run.clone()),
                    ("approval_id", request["approval_id"].clone()),
                    ("scope_ref", request["scope_ref"].clone()),
                    ("decision", request["decision"].clone()),
                    ("authority_ref", request["authority_ref"].clone()),
                    ("artifact_sha256", request["artifact_sha256"].clone()),
                    ("actor", request["actor"].clone()),
                    ("host_identity", host),
                    ("approved_at", approved_at.clone()),
                ]))?);
                let mut values = vec![run.clone(), request["approval_id"].clone()];
                values.extend(keys.map(|key| request[key].clone()));
                session.write(GraphWrite::Approval, &values)?;
                session.write(
                    GraphWrite::ApprovalAttestation,
                    &[
                        run.clone(),
                        request["approval_id"].clone(),
                        request["actor"].clone(),
                        context.store.platform.host_identity()?.into(),
                        approved_at,
                        digest.into(),
                    ],
                )?;
            }
            Ok(Value::object([("code", "APPROVAL_RECORDED".into())]))
        },
        mutation_task_validator(context, TaskValidationMutation::RecordControl),
        context.store.fault,
    )
}
