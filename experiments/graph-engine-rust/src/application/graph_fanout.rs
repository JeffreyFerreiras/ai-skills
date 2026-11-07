use super::{
    artifacts::{persist_artifact, ArtifactResolver},
    error::{EngineError, EngineResult, ManifestJsonTraceSite, TaskValidationMutation},
    graph_ledger::GraphRead,
    graph_payload::sorted_inputs,
    graph_transitions::promote_fanout_successors,
    graph_writes::GraphWrite,
    manifest_snapshot::inbox_snapshot,
    persistent_store::PersistentSession,
    semantic_state::{mutation_task_validator, SemanticContext},
    state::mutate_persistent,
};
use crate::{
    compat::{self, SerializationProfile, Value},
    domain::{contracts, errors, evidence, fanout_contract::validate_fanout_assessment, planner},
};

fn add_assessment_input(
    session: &dyn PersistentSession,
    member: &Value,
    assessment: &Value,
) -> EngineResult<()> {
    let node = session
        .first(GraphRead::Node, std::slice::from_ref(member))?
        .filter(|node| node["status"].is_text("pending"))
        .ok_or_else(|| EngineError::state("FANOUT_MEMBER_INVALID"))?;
    let mut envelope = compat::loads_value(&node["envelope_json"])?;
    let mut inputs = errors::iterable_values(errors::string_item(&envelope, "inputs")?)?;
    inputs.push(assessment.clone());
    envelope.set("inputs", sorted_inputs(inputs)?);
    let encoded = String::from_utf8(compat::dumps(
        &envelope,
        SerializationProfile::SORTED_STORED,
    )?)
    .expect("stored JSON is UTF-8");
    session.write(
        GraphWrite::UpdateNodeEnvelope,
        &[encoded.into(), member.clone()],
    )
}

pub(super) fn verify_evidence(
    session: &dyn PersistentSession,
    run_id: &str,
    context: &SemanticContext<'_>,
    policy: &Value,
    evidence: &[Value],
) -> EngineResult<Vec<Value>> {
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
    evidence
        .iter()
        .map(|item| {
            let verified =
                resolver.resolve_reference(&item["ref"], &item["sha256"], &item["kind"])?;
            persist_artifact(session, run_id, &verified)?;
            Ok(Value::object([
                ("kind", verified.kind),
                ("ref", verified.reference),
                ("sha256", verified.sha256),
            ]))
        })
        .collect()
}

pub fn record(
    session: &dyn PersistentSession,
    run_id: &str,
    arguments: &Value,
    policy: &Value,
    context: &SemanticContext<'_>,
) -> EngineResult<Value> {
    let snapshot = inbox_snapshot(
        context,
        policy,
        run_id,
        &arguments["assessment_manifest"],
        (256 * 1024).into(),
        ManifestJsonTraceSite::FanoutAssessment,
    )?;
    let authority = Value::String(contracts::validate_ref(
        &arguments["authority_ref"],
        "authority_ref",
        false,
    )?);
    let fanout_id = Value::from(contracts::opaque(&arguments["fanout_id"], "fanout_id")?);
    let request = Value::object([
        ("command", "record.fanout-assessment".into()),
        ("fanout_id", fanout_id.clone()),
        ("assessment_digest", snapshot.digest.into()),
        ("authority_ref", authority.clone()),
    ]);
    let operation = contracts::opaque(&arguments["op_id"], "op_id")?;
    mutate_persistent(
        session,
        run_id,
        &operation,
        &request,
        |session, current, _| {
            let run = &current.fields;
            let fanout = session
                .first(
                    GraphRead::FanoutForRun,
                    &[run["run_id"].clone(), fanout_id.clone()],
                )?
                .ok_or_else(|| EngineError::state("FANOUT_NOT_FOUND"))?;
            if !fanout["status"].is_text("awaiting") {
                return Err(EngineError::state("FANOUT_ASSESSMENT_EXISTS"));
            }
            let members = compat::loads_value(&fanout["member_branch_ids_json"])?;
            let members = errors::iterable_values(&members)?;
            let mut manifest = validate_fanout_assessment(
                snapshot
                    .parsed
                    .as_ref()
                    .expect("safe JSON snapshot is parsed"),
                &run["run_id"],
                &fanout_id,
                &members,
            )?;
            let dependencies = planner::validate_fanout_ordering(
                manifest["members"].as_list().expect("validated members"),
                manifest["dependencies"]
                    .as_list()
                    .expect("validated dependencies"),
                context.case_sensitive,
            )
            .map_err(|error| EngineError::state(&error))?;

            let verified_evidence = verify_evidence(
                session,
                run_id,
                context,
                policy,
                manifest["evidence"].as_list().expect("validated evidence"),
            )?;
            let assessed_at = Value::from(context.store.platform.utc_now()?);
            let actor = Value::String(context.provenance.actor()?);
            let host = Value::from(context.provenance.host_identity()?);
            manifest.set("dependencies", Value::List(dependencies.clone()));
            manifest.set("evidence", sorted_inputs(verified_evidence)?);
            manifest.set("authority_ref", authority.clone());
            manifest.set("actor", actor.clone());
            manifest.set("host_identity", host.clone());
            manifest.set("assessed_at", assessed_at.clone());
            let artifact = evidence::canonical_ledger_artifact(
                fanout_id.as_string().expect("validated fanout ID"),
                &"evidence_manifest".into(),
                &manifest,
            )?;
            persist_artifact(session, run_id, &artifact)?;
            for dependency in dependencies {
                session.write(
                    GraphWrite::FanoutDependency,
                    &[
                        fanout_id.clone(),
                        dependency["before_branch_id"].clone(),
                        dependency["after_branch_id"].clone(),
                        dependency["reason"].clone(),
                    ],
                )?;
            }
            let assessment = artifact.as_input();
            for member in members {
                add_assessment_input(session, &member, &assessment)?;
            }
            session.write(
                GraphWrite::FanoutAssessment,
                &[
                    artifact.reference.clone(),
                    artifact.sha256.clone(),
                    authority.clone(),
                    actor,
                    host,
                    assessed_at,
                    fanout_id.clone(),
                ],
            )?;
            let ready = promote_fanout_successors(session, &run["run_id"])?;
            Ok(Value::object([
                ("code", "FANOUT_ASSESSMENT_RECORDED".into()),
                ("fanout_id", fanout_id.clone()),
                ("assessment_ref", artifact.reference),
                ("assessment_digest", artifact.sha256),
                ("ready_branch_ids", Value::List(ready)),
            ]))
        },
        mutation_task_validator(context, TaskValidationMutation::FanoutAssessment),
        context.store.fault,
    )
}
