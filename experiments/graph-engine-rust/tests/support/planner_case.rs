use super::hardening_case::{write_json, HardeningCase};
use graph_engine::{
    application::{
        artifacts::ArtifactResolver,
        error::{EngineError, EngineResult},
        graph_initialization::{prepare_execution_plan, PlanRequest},
        persistent_store::InitializeRequest,
    },
    compat::{self, SerializationProfile, Value},
    domain::{execution::reconstruct_execution_plan, ids, planner},
};

pub(super) fn task_v2(case: &HardeningCase, route: &str) -> Value {
    let mut task = case.fixture.graph.task("delivery", route, &[]);
    task.set("schema_version", 2_i64.into());
    task.set("risk_level", "low".into());
    task.set(
        "model_sizing",
        Value::object([
            ("scope_extent", "bounded".into()),
            ("uncertainty", "low".into()),
        ]),
    );
    task
}

pub(super) fn selection(model: &str, effort: &str) -> Value {
    Value::object([("model", model.into()), ("reasoning_effort", effort.into())])
}

pub(super) fn initialize(
    case: &HardeningCase,
    task: &Value,
    host: Option<&str>,
    size: Option<&str>,
    approve: bool,
) -> EngineResult<Value> {
    let path = case.fixture.graph.repository.join("docs/task.json");
    write_json(&path, task);
    let mut args = vec![
        "--ack-degraded-permissions",
        "--ack-degraded-durability",
        "init",
        "--run-id",
        "RUN-1",
        "--task-brief",
        path.to_str().unwrap(),
        "--op-id",
        "init-1",
    ];
    if let Some(size) = size {
        args.extend(["--size", size]);
    }
    if let Some(host) = host {
        args.extend(["--host", host]);
    }
    let result = case.run(&args)?;
    if approve {
        approve_plan(case, &result["execution_plan_digest"], "plan-approval-1")?;
    }
    Ok(result)
}

pub(super) fn approve_plan(case: &HardeningCase, digest: &Value, op: &str) -> EngineResult<Value> {
    case.run(&[
        "record",
        "plan-approval",
        "--run-id",
        "RUN-1",
        "--plan-digest",
        &digest.text().unwrap(),
        "--decision",
        "APPROVE",
        "--authority-ref",
        "authority:test",
        "--op-id",
        op,
    ])
}

pub(super) fn preview(case: &HardeningCase, task: &Value, host: Option<&str>) -> Value {
    let path = case.fixture.graph.repository.join("docs/task.json");
    write_json(&path, task);
    let mut args = vec![
        "plan",
        "--run-id",
        "RUN-1",
        "--task-brief",
        path.to_str().unwrap(),
    ];
    if let Some(host) = host {
        args.extend(["--host", host]);
    }
    case.run(&args).unwrap()["execution_plan"].clone()
}

pub(super) fn resume(case: &HardeningCase) -> EngineResult<Value> {
    case.run(&[
        "--ack-degraded-permissions",
        "--ack-degraded-durability",
        "resume",
        "--run-id",
        "RUN-1",
    ])
}

pub(super) fn no_database(case: &HardeningCase) {
    assert!(!case
        .fixture
        .graph
        .store
        .db_path("albanian-live-translate", "RUN-1")
        .exists());
}

pub(super) fn contract_failure(result: EngineResult<Value>, expected: Option<&str>) {
    let error = match result {
        Err(error) => error,
        Ok(result) => panic!("expected a contract error, got {:?}", result["code"].text()),
    };
    assert!(
        matches!(&error, EngineError::Contract(error)
        if expected.is_none_or(|code| error.code == code)),
        "{error:?}"
    );
}

pub(super) enum HistoricalPlan {
    LegacyCatalog,
    LegacyProjection,
}

pub(super) fn initialize_historical(
    case: &HardeningCase,
    task: &Value,
    host: &str,
    size: Option<&str>,
    vintage: HistoricalPlan,
) -> Value {
    let path = case.fixture.graph.repository.join("docs/task.json");
    write_json(&path, task);
    case.fixture
        .with_context(|context| {
            let mut prepared = prepare_execution_plan(
                context,
                &PlanRequest {
                    run_id: &"RUN-1".into(),
                    task_brief: &Value::String(context.files.spelling(&path)),
                    size,
                    host,
                },
            )?;
            let plan = match vintage {
                HistoricalPlan::LegacyCatalog => reconstruct_execution_plan(
                    "RUN-1",
                    &prepared.task,
                    &Value::object([("host", host.into())]),
                    size,
                )
                .unwrap(),
                HistoricalPlan::LegacyProjection => {
                    let mut delegation = prepared.task["reviewer_delegation"].clone();
                    let mut assignments = delegation["assignments"].as_list().unwrap().to_vec();
                    for assignment in &mut assignments {
                        let role = assignment["role"].text().unwrap();
                        let scope = assignment["scope_refs"].as_list().unwrap();
                        let legacy = planner::effective_role_capabilities(
                            context.policy,
                            &prepared.full_task,
                            &role,
                            None,
                            context.files.windows(),
                        )?
                        .into_iter()
                        .filter(|capability| {
                            ["filesystem_read", "external_read"]
                                .iter()
                                .any(|effect| capability["effect"].is_text(effect))
                                && scope
                                    .iter()
                                    .any(|reference| reference.py_eq(&capability["target_ref"]))
                        });
                        assignment.set("effect_capabilities", Value::list(legacy));
                    }
                    delegation.set("assignments", Value::List(assignments));
                    prepared.task.set("reviewer_delegation", delegation);
                    let mut plan = graph_engine::domain::execution::build_execution_plan(
                        "RUN-1",
                        &prepared.task,
                        size,
                        host,
                    )
                    .unwrap();
                    plan.remove("capability_projection_version");
                    plan.remove("plan_digest");
                    let digest = ids::sha256_bytes(&ids::canonical_bytes(&plan)?);
                    plan.set("plan_digest", digest.into());
                    plan
                }
            };
            let snapshot = &prepared.task_snapshot;
            let task_ref = Value::from(format!("repo:docs/task.json#sha256={}", snapshot.digest));
            let input = Value::object([
                ("ref", task_ref.clone()),
                ("kind", "task_brief".into()),
                ("sha256", snapshot.digest.as_str().into()),
                ("size_bytes", snapshot.size.into()),
            ]);
            let mut artifact = input.clone();
            artifact.set("source_type", "repo".into());
            artifact.set(
                "source_path",
                Value::String(context.files.spelling(&snapshot.path)),
            );
            artifact.set("content_json", Value::Null);
            artifact.set("device", Value::Integer(snapshot.identity.device.into()));
            artifact.set("inode", Value::Integer(snapshot.identity.inode.into()));
            let mut artifacts = vec![artifact];
            let resolver = ArtifactResolver {
                files: context.files,
                snapshots: context.snapshots,
                repository: context.repository,
                skill: context.skill,
                installation: context.installation,
                policy: context.policy,
                runtime: Some(&context.store.state_root),
                ledger: None,
            };
            let mut evidence_refs = Vec::new();
            let mut evidence_digests = Vec::new();
            for reference in prepared.full_task["evidence_paths"].as_list().unwrap() {
                let verified = resolver
                    .resolve_unhashed_reference(reference, &"acceptance_evidence".into())?;
                evidence_refs.push(verified.reference.clone());
                evidence_digests.push(Value::tuple([
                    verified.reference.clone(),
                    verified.sha256.clone(),
                ]));
                artifacts.push(Value::object([
                    ("ref", verified.reference),
                    ("kind", verified.kind),
                    ("sha256", verified.sha256),
                    ("size_bytes", verified.size_bytes),
                    ("source_type", verified.source_type),
                    ("source_path", verified.source_path),
                    ("content_json", verified.content_json),
                    ("device", verified.device),
                    ("inode", verified.inode),
                ]));
            }
            graph_engine::domain::python_operations::sort_comparable_values(&mut evidence_refs)?;
            graph_engine::domain::python_operations::sort_comparable_values(&mut evidence_digests)?;
            prepared
                .task
                .set("evidence_paths", Value::List(evidence_refs));
            let spec = planner::bootstrap(context.policy)?;
            let branch = planner::envelope(
                "RUN-1",
                context.current_policy_digest,
                context.policy,
                &prepared.task,
                &spec,
                "ready",
                &[input],
                0,
                Some(&plan),
                context.files.windows(),
            )?;
            let mut bootstrap = branch.clone();
            bootstrap.set("stage", spec.stage.into());
            bootstrap.set(
                "envelope_json",
                Value::String(compat::dumps_text(&branch, SerializationProfile::CLI)?),
            );
            let request = Value::object([
                ("command", "init".into()),
                ("run_id", "RUN-1".into()),
                ("op_id", "init-1".into()),
                ("policy_digest", context.current_policy_digest.into()),
                ("task_digest", snapshot.digest.as_str().into()),
                ("evidence_digests", Value::List(evidence_digests)),
                ("ack_permissions", true.into()),
                ("ack_durability", true.into()),
                ("execution_size", plan["size"].clone()),
                ("execution_plan_digest", plan["plan_digest"].clone()),
            ]);
            let digest = ids::sha256_bytes(&ids::canonical_bytes(&request)?);
            context.store.initialize(&InitializeRequest {
                repository: context.repository,
                policy: context.policy,
                policy_digest: context.current_policy_digest,
                task: &prepared.task,
                task_path: &snapshot.path,
                task_digest: &snapshot.digest,
                run_id: "RUN-1",
                operation_id: "init-1",
                request_digest: &digest,
                acknowledge_permissions: true,
                acknowledge_durability: true,
                bootstrap: &bootstrap,
                execution_plan: &plan,
                task_ref: &task_ref,
                initial_artifacts: &artifacts,
            })
        })
        .unwrap()
}
