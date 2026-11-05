use super::{
    artifacts::{persist_artifact, ArtifactResolver},
    error::{EngineError, EngineResult, ManifestJsonTraceSite, TaskValidationMutation},
    graph_payload::artifact_reference,
    graph_writes::GraphWrite,
    persistent_store::PersistentSession,
    policy,
    semantic_state::{mutation_task_validator, SemanticContext},
    snapshots::{self, Snapshot, SnapshotFailure},
    state::mutate_persistent,
};
use crate::{
    compat::{self, integer, PyString, Value},
    domain::{
        contracts::{self, ContractError},
        control_manifests::validate_control_manifest,
        errors::{attribute_error, string_item, ValidationFailure},
        evidence,
    },
};
use num_bigint::BigInt;

pub(super) fn verify_control_evidence(
    session: &dyn PersistentSession,
    run_id: &str,
    manifest: &Value,
    context: &SemanticContext<'_>,
) -> EngineResult<Vec<Value>> {
    let resolver = ArtifactResolver {
        files: context.files,
        snapshots: context.snapshots,
        repository: context.repository,
        skill: context.skill,
        installation: context.installation,
        policy: context.policy,
        runtime: context.runtime,
        ledger: Some(session),
    };
    let mut verified_items = Vec::new();
    for item in manifest["evidence"]
        .as_list()
        .expect("validated evidence list")
    {
        if item.as_object().is_none() {
            return Err(ContractError::new("evidence", "INVALID_OBJECT").into());
        }
        contracts::require_keys(
            item,
            &["kind", "ref", "sha256"],
            &["kind", "ref", "sha256"],
            "evidence",
        )?;
        let verified = resolver.resolve_reference(&item["ref"], &item["sha256"], &item["kind"])?;
        persist_artifact(session, run_id, &verified)?;
        verified_items.push(artifact_reference(&verified));
    }
    Ok(verified_items)
}

fn manifest_snapshot(
    context: &SemanticContext<'_>,
    run_id: &str,
    path: &Value,
    maximum: &BigInt,
) -> EngineResult<Snapshot> {
    let repository = string_item(context.policy, "repository_id")?;
    let repository = repository
        .as_string()
        .ok_or_else(|| attribute_error(repository, "encode"))?;
    let repository = String::from_utf8(compat::encode_utf8(repository)?)
        .expect("the UTF-8 encoder returns valid UTF-8");
    let inbox = context.store.inbox_root(&repository, run_id);
    let path = policy::path_from_value(context.files, path)?;
    context
        .store
        .verify_inbox_manifest(&repository, run_id, &path)?;
    let policy_maximum = integer::coerce(string_item(
        string_item(context.policy, "limits")?,
        "manifest_bytes",
    )?)
    .map_err(ValidationFailure::from)?;
    let maximum = maximum.min(&policy_maximum);
    snapshots::safe_json_snapshot_integer(context.snapshots, &path, &[inbox], maximum).map_err(
        |error| match error {
            SnapshotFailure::Contract(error) => error.into(),
            SnapshotFailure::Io(error) => error.into(),
            SnapshotFailure::Json(error) => {
                EngineError::manifest_json(error, ManifestJsonTraceSite::Block)
            }
        },
    )
}

pub fn abort_run(
    session: &dyn PersistentSession,
    run_id: &str,
    arguments: &Value,
    context: &SemanticContext<'_>,
) -> EngineResult<Value> {
    let request = Value::object([
        ("command", "abort".into()),
        (
            "reason_code",
            contracts::opaque(string_item(arguments, "reason_code")?, "reason_code")?.into(),
        ),
        (
            "authority_ref",
            Value::String(contracts::validate_ref(
                string_item(arguments, "authority_ref")?,
                "authority_ref",
                false,
            )?),
        ),
    ]);
    let operation = contracts::opaque(string_item(arguments, "op_id")?, "op_id")?;
    stop_run(
        session,
        run_id,
        &operation,
        &request,
        StopKind::Abort,
        context,
    )
}

pub fn block_run(
    session: &dyn PersistentSession,
    run_id: &str,
    arguments: &Value,
    context: &SemanticContext<'_>,
) -> EngineResult<Value> {
    let snapshot = manifest_snapshot(
        context,
        run_id,
        string_item(arguments, "evidence_manifest")?,
        &BigInt::from(128 * 1024),
    )?;
    let request = Value::object([
        ("command", "block".into()),
        (
            "reason_code",
            contracts::opaque(string_item(arguments, "reason_code")?, "reason_code")?.into(),
        ),
        ("evidence_digest", snapshot.digest.clone().into()),
    ]);
    let operation = contracts::opaque(string_item(arguments, "op_id")?, "op_id")?;
    stop_run(
        session,
        run_id,
        &operation,
        &request,
        StopKind::Block(snapshot),
        context,
    )
}

enum StopKind {
    Abort,
    Block(Snapshot),
}

fn stop_run(
    session: &dyn PersistentSession,
    run_id: &str,
    operation: &str,
    request: &Value,
    kind: StopKind,
    context: &SemanticContext<'_>,
) -> EngineResult<Value> {
    let mut semantic = *context;
    semantic.evidence_enable = false;
    mutate_persistent(
        session,
        run_id,
        operation,
        request,
        |session, current, _| {
            let current = &current.fields;
            let blocked = matches!(&kind, StopKind::Block(_));
            let status = string_item(current, "status")?;
            if !status.is_text("initialized")
                && !status.is_text("active")
                && (blocked || !status.is_text("blocked"))
            {
                return Err(EngineError::state("INVALID_RUN_TRANSITION"));
            }
            let run = string_item(current, "run_id")?;
            if let StopKind::Block(snapshot) = &kind {
                let mut manifest = validate_control_manifest(
                    snapshot
                        .parsed
                        .as_ref()
                        .expect("safe JSON snapshot has parsed value"),
                    "block",
                    run_id,
                    None,
                    &request["reason_code"]
                        .text()
                        .expect("opaque reason is ASCII"),
                )?;
                let verified_items = verify_control_evidence(session, run_id, &manifest, context)?;
                manifest.set("evidence", Value::List(verified_items));
                let artifact = evidence::canonical_ledger_artifact(
                    &PyString::from(format!("block-{operation}")),
                    &"evidence_manifest".into(),
                    &manifest,
                )?;
                persist_artifact(session, run_id, &artifact)?;
            }
            let (target, code) = if blocked {
                ("blocked", "GRAPH_BLOCKED")
            } else {
                ("aborted", "ABORTED")
            };
            session.write(
                GraphWrite::StopRun,
                &[
                    target.into(),
                    request["reason_code"].clone(),
                    context.store.platform.utc_now()?.into(),
                    run.clone(),
                ],
            )?;
            Ok(Value::object([
                ("code", code.into()),
                ("status", target.into()),
            ]))
        },
        mutation_task_validator(&semantic, TaskValidationMutation::RunControl),
        context.store.fault,
    )
}
