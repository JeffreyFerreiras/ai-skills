use super::{
    artifacts::ArtifactResolver,
    error::{EngineError, EngineResult},
    graph_initialization::{
        constraint_markers, constraint_snapshot, maximum, node_row, normalize_constraint_path,
        reference_roots, task_roots,
    },
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    graph_payload::persist_payload,
    graph_writes::GraphWrite,
    persistent_store::{insert_join, node_values, PersistentSession},
    policy,
    run_context::RunContext,
    semantic_state::SemanticContext,
    snapshots::{self, SnapshotFailure},
    source_capture::{source_roots, SourceFileFactory},
};
use crate::{
    compat::{self, PyString, SerializationProfile, Value},
    domain::{constraints, contracts, errors, planner, python_operations, source_paths},
};

pub(crate) fn task_input(ledger: &dyn GraphLedgerReadPort, run: &Value) -> EngineResult<Value> {
    let artifact = ledger
        .first(GraphRead::Artifact, std::slice::from_ref(&run["task_ref"]))?
        .ok_or_else(|| EngineError::state("INPUT_ARTIFACT_INVALID"))?;
    Ok(Value::object([
        ("kind", "task_brief".into()),
        ("ref", run["task_ref"].clone()),
        ("sha256", run["task_digest"].clone()),
        ("size_bytes", artifact["size_bytes"].clone()),
    ]))
}

fn assigned_scope(
    full_task: &Value,
    loaded: &RunContext,
    context: &SemanticContext<'_>,
    spec: &planner::NodeSpec,
    source_files: &dyn SourceFileFactory,
) -> EngineResult<Value> {
    if !["tech_lead", "senior_engineer"].contains(&spec.key.as_str()) {
        return Ok(Value::List(vec![]));
    }
    let files = source_files.create(context.repository, &loaded.policy, context.files);
    let roots = source_roots(&*files, &loaded.task["authority"]["capabilities"])?;
    let declared = errors::iterable_values(&full_task["scope"]["included"])?;
    if declared.is_empty() {
        return Ok(Value::List(vec![]));
    }
    let mut relative = Vec::new();
    for value in &declared {
        let text = value
            .as_string()
            .ok_or_else(|| errors::attribute_error(value, "startswith"))?;
        if !text.starts_with("repo:") {
            return Ok(Value::List(vec![]));
        }
        relative.push(&text.0[5..]);
    }
    if full_task["scope"]["excluded"].truthy() {
        return Ok(Value::List(vec![]));
    }
    let paths = relative
        .into_iter()
        .map(|path| {
            contracts::lexical_relative(&Value::String(PyString(path.to_vec())), "task_scope")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let roots = roots.as_list().expect("normalized roots are a list");
    let windows = context.files.windows();
    if paths.iter().any(|path| {
        !roots.iter().any(|root| {
            source_paths::contains(
                root.as_string().expect("normalized root is a string"),
                path,
                windows,
            )
        })
    }) {
        return Ok(Value::List(vec![]));
    }
    Ok(Value::list(paths.into_iter().map(Value::String)))
}

fn planning_constraint_input(
    session: &dyn PersistentSession,
    run: &Value,
    loaded: &RunContext,
    context: &SemanticContext<'_>,
    spec: &planner::NodeSpec,
    source_files: &dyn SourceFileFactory,
) -> EngineResult<Option<Value>> {
    let path = policy::path_from_value(context.files, &run["task_path"])?;
    let snapshot = snapshots::safe_json_snapshot_integer(
        context.snapshots,
        &path,
        &task_roots(context)?,
        maximum(&loaded.policy["artifact_kinds"]["task_brief"]["max_bytes"])?,
    )
    .map_err(|failure| match failure {
        SnapshotFailure::Contract(error) => EngineError::from(error),
        SnapshotFailure::Io(error) => EngineError::from(error),
        SnapshotFailure::Json(error) => EngineError::from(error),
    })?;
    let full_task = snapshot.parsed.expect("safe JSON snapshot is parsed");
    let markers = constraint_markers(&full_task)?;
    let Some(marker) = markers.first() else {
        return Ok(None);
    };
    let row = session
        .first(
            GraphRead::ConstraintBundleForRun,
            &[run["run_id"].clone(), marker.as_str().into()],
        )?
        .ok_or_else(|| {
            EngineError::python("TypeError", "'NoneType' object is not subscriptable")
        })?;
    let runtime = &context.store.state_root;
    let roots = reference_roots(context.files, context.repository, runtime, &loaded.policy)?;
    let resolver = ArtifactResolver {
        files: context.files,
        snapshots: context.snapshots,
        repository: context.repository,
        skill: context.skill,
        installation: context.installation,
        policy: &loaded.policy,
        runtime: Some(runtime),
        ledger: Some(session),
    };
    let bundle = constraint_snapshot(
        &resolver,
        &row["source_path"],
        &roots,
        maximum(&loaded.policy["limits"]["manifest_bytes"])?,
    )?;
    let scope = assigned_scope(&full_task, loaded, context, spec, source_files)?;
    let supported = session
        .all(
            GraphRead::ArtifactReferencesForRun,
            std::slice::from_ref(&run["run_id"]),
        )?
        .into_iter()
        .map(|row| row["ref"].clone())
        .collect::<Vec<_>>();
    let selection = constraints::select_constraints(
        &bundle,
        &Value::object([
            ("repository_id", loaded.policy["repository_id"].clone()),
            ("acceptance_ids", loaded.task["acceptance_ids"].clone()),
            ("role", spec.role.as_str().into()),
            ("scope_paths", scope.clone()),
        ]),
        constraints::Selection {
            activated: true,
            supported: &supported,
            authority_refs: std::slice::from_ref(&loaded.task["policy_approval"]["authority_ref"]),
            normalize: &|path| normalize_constraint_path(path, context.files.windows()),
        },
    )?;
    let diagnostics = Value::list(
        selection["excluded"]
            .as_list()
            .expect("selector exclusions are a list")
            .iter()
            .filter(|item| item["reason"].is_text("unresolved_scope"))
            .cloned(),
    );
    if !selection["active"].truthy() && !diagnostics.truthy() {
        return Ok(None);
    }
    let projection = Value::object([
        ("kind", "planning_constraint_projection".into()),
        ("schema_version", 1_i64.into()),
        ("task_digest", run["task_digest"].clone()),
        ("bundle_ref", row["ref"].clone()),
        ("role", spec.role.as_str().into()),
        ("scope_paths", scope),
        ("records", selection["active"].clone()),
        ("diagnostics", diagnostics),
    ]);
    let branch_id = planner::branch_id(
        &run["run_id"].text().expect("validated run ID"),
        &run["policy_digest"]
            .text()
            .expect("validated policy digest"),
        spec,
    )?;
    let artifact = persist_payload(
        session,
        run,
        &loaded.policy,
        &format!("constraints-{branch_id}").into(),
        &"evidence_manifest".into(),
        &projection,
    )?;
    Ok(Some(artifact.as_input()))
}

pub(crate) fn insert_spec(
    session: &dyn PersistentSession,
    run: &Value,
    loaded: &RunContext,
    context: &SemanticContext<'_>,
    spec: &planner::NodeSpec,
    inputs: &[Value],
    status: &str,
    source_files: &dyn SourceFileFactory,
) -> EngineResult<Value> {
    let plan = session
        .first(
            GraphRead::ExecutionPlanBody,
            std::slice::from_ref(&run["run_id"]),
        )?
        .ok_or_else(|| EngineError::state("EXECUTION_PLAN_STATE_INVALID"))?;
    let plan = compat::loads_value(&plan["plan_json"])?;
    let mut inputs = inputs.to_vec();
    if run["state_schema_version"].py_eq(&7_i64.into()) {
        inputs.extend(planning_constraint_input(
            session,
            run,
            loaded,
            context,
            spec,
            source_files,
        )?);
    }
    let run_id = run["run_id"].text().expect("validated run ID");
    let policy_digest = run["policy_digest"]
        .text()
        .expect("validated policy digest");
    let envelope = planner::envelope(
        &run_id,
        &policy_digest,
        &loaded.policy,
        &loaded.task,
        spec,
        status,
        &inputs,
        0,
        Some(&plan),
        context.files.windows(),
    )?;
    let row = node_row(spec, &envelope)?;
    session.insert_node(&node_values(&run_id, &row)?)?;
    Ok(row)
}

pub(crate) fn insert_join_spec(
    session: &dyn PersistentSession,
    run: &Value,
    spec: &planner::JoinSpec,
) -> EngineResult<Value> {
    let run_id = run["run_id"].text().expect("validated run ID");
    let digest = run["policy_digest"]
        .text()
        .expect("validated policy digest");
    let id = Value::from(planner::join_id(&run_id, &digest, spec)?);
    let row = Value::object([
        ("join_id", id.clone()),
        ("join_key", spec.key.as_str().into()),
        ("kind", spec.kind.as_str().into()),
        ("stage", spec.stage.as_str().into()),
        ("generation", spec.generation.into()),
    ]);
    let mut members = Vec::new();
    for member in &spec.members {
        members.push(Value::object([
            (
                "branch_id",
                planner::branch_id(&run_id, &digest, member)?.into(),
            ),
            ("mandatory", member.mandatory.into()),
        ]));
    }
    insert_join(session, &run_id, &row, &members)?;
    Ok(id)
}

#[derive(Default)]
pub(crate) struct SuccessorTopology {
    pub branches: Vec<Value>,
    pub joins: Vec<Value>,
    pub fanouts: Vec<Value>,
}

pub(crate) fn insert_collection_successors(
    session: &dyn PersistentSession,
    run: &Value,
    loaded: &RunContext,
    context: &SemanticContext<'_>,
    spec: &planner::JoinSpec,
    inputs: &[Value],
    source_files: &dyn SourceFileFactory,
) -> EngineResult<SuccessorTopology> {
    let mut topology = SuccessorTopology::default();
    let parallel = spec.members.len() > 1;
    let status = if parallel { "pending" } else { "ready" };
    for member in &spec.members {
        let row = insert_spec(
            session,
            run,
            loaded,
            context,
            member,
            inputs,
            status,
            source_files,
        )?;
        topology.branches.push(row["branch_id"].clone());
    }
    python_operations::sort_comparable_values(&mut topology.branches)?;
    topology.joins.push(insert_join_spec(session, run, spec)?);
    if parallel {
        let run_id = run["run_id"].text().expect("validated run ID");
        let digest = run["policy_digest"]
            .text()
            .expect("validated policy digest");
        let id = Value::from(planner::fanout_id(
            &run_id,
            &digest,
            &spec.stage,
            spec.generation,
        )?);
        let members = Value::List(topology.branches.clone());
        let encoded = String::from_utf8(compat::dumps(&members, SerializationProfile::CLI)?)
            .expect("stored JSON is UTF-8");
        session.write(
            GraphWrite::AwaitingFanout,
            &[
                id.clone(),
                run["run_id"].clone(),
                spec.stage.as_str().into(),
                spec.generation.into(),
                encoded.into(),
            ],
        )?;
        topology.fanouts.push(id);
    }
    Ok(topology)
}
