use super::{
    artifacts::ArtifactResolver,
    error::{
        ConstraintCommand, EngineError, EngineResult, GraphctlTraceSite, PlanSnapshot,
        PlanTraceCommand, SnapshotJsonTraceSite,
    },
    persistent_store::InitializeRequest,
    policy::{self, PolicyFilePort},
    semantic_state::SemanticContext,
    snapshots::{self, Snapshot, SnapshotFailure},
};
use crate::{
    compat::{self, repr, unicode, PyString, SerializationProfile, Value},
    domain::{
        check_receipts::configured_check,
        constraints,
        contract_values::get,
        contracts::{self, ContractError},
        errors::{self, string_item},
        evidence::VerifiedArtifact,
        execution, helper_register, ids, planner,
    },
};
use num_bigint::BigInt;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub struct PlanRequest<'a> {
    pub run_id: &'a Value,
    pub task_brief: &'a Value,
    pub size: Option<&'a str>,
    pub host: &'a str,
}

pub struct InitRequest<'a> {
    pub plan: PlanRequest<'a>,
    pub operation_id: &'a Value,
    pub acknowledge_permissions: bool,
    pub acknowledge_durability: bool,
}

pub struct PreparedPlan {
    pub task_snapshot: Snapshot,
    pub full_task: Value,
    pub task: Value,
    pub execution_plan: Value,
}

fn snapshot_error(error: SnapshotFailure) -> EngineError {
    match error {
        SnapshotFailure::Contract(error) => error.into(),
        SnapshotFailure::Io(error) => error.into(),
        SnapshotFailure::Json(error) => error.into(),
    }
}

fn joined(files: &dyn PolicyFilePort, root: &Path, value: &Value) -> EngineResult<PathBuf> {
    let text = value.as_string().ok_or_else(|| {
        EngineError::python(
            "TypeError",
            format!(
                "unsupported operand type(s) for /: '{}' and '{}'",
                if files.windows() {
                    "WindowsPath"
                } else {
                    "PosixPath"
                },
                value.python_type()
            ),
        )
    })?;
    Ok(root.join(files.from_string(text)?))
}

pub(crate) fn task_roots(context: &SemanticContext<'_>) -> EngineResult<Vec<PathBuf>> {
    task_roots_at(
        context.files,
        context.repository,
        &context.store.state_root,
        context.policy,
    )
}

fn task_roots_at(
    files: &dyn PolicyFilePort,
    repository: &Path,
    runtime_root: &Path,
    policy: &Value,
) -> EngineResult<Vec<PathBuf>> {
    let modern = policy["schema_version"].py_eq(&3_i64.into());
    let root = if modern { runtime_root } else { repository };
    let values = string_item(
        string_item(policy, "artifact_roots")?,
        if modern { "runtime" } else { "repo" },
    )?;
    errors::iterable_values(values)?
        .iter()
        .map(|value| files.absolute(&joined(files, root, value)?))
        .collect()
}

pub(crate) fn reference_roots(
    files: &dyn PolicyFilePort,
    repository: &Path,
    runtime_root: &Path,
    policy: &Value,
) -> EngineResult<Vec<PathBuf>> {
    let mut roots = task_roots_at(files, repository, runtime_root, policy)?;
    if policy["schema_version"].py_eq(&3_i64.into()) {
        for value in errors::iterable_values(string_item(policy, "implementation_roots")?)? {
            roots.push(joined(files, repository, &value)?);
        }
    }
    Ok(roots)
}

fn reference_path(context: &SemanticContext<'_>, reference: &Value) -> EngineResult<PathBuf> {
    let text = reference
        .as_string()
        .ok_or_else(|| errors::attribute_error(reference, "startswith"))?;
    let (root, prefix) = if text.starts_with("runtime:") {
        (context.store.state_root.as_path(), 8)
    } else if text.starts_with("repo:") {
        (context.repository, 5)
    } else {
        return Err(ContractError::new("artifact_ref", "AUTHORITATIVE_REF_REQUIRED").into());
    };
    let relative = contracts::lexical_relative(
        &Value::String(PyString(text.0[prefix..].to_vec())),
        "artifact_ref",
    )?;
    Ok(root.join(context.files.from_string(&relative)?))
}

pub(crate) fn maximum(value: &Value) -> EngineResult<&BigInt> {
    value.strict_integer().ok_or_else(|| {
        EngineError::python(
            "TypeError",
            format!(
                "'{}' object cannot be interpreted as an integer",
                value.python_type()
            ),
        )
    })
}

pub fn prepare_execution_plan(
    context: &SemanticContext<'_>,
    request: &PlanRequest<'_>,
) -> EngineResult<PreparedPlan> {
    prepare_execution_plan_traced(context, request, PlanTraceCommand::Preview)
        .map_err(EngineError::without_graphctl_trace)
}

pub(super) fn prepare_execution_plan_traced(
    context: &SemanticContext<'_>,
    request: &PlanRequest<'_>,
    command: PlanTraceCommand,
) -> EngineResult<PreparedPlan> {
    let run_id = contracts::opaque(request.run_id, "run_id")?;
    let path = policy::path_from_value(context.files, request.task_brief)?;
    let task_snapshot = snapshots::safe_json_snapshot_integer(
        context.snapshots,
        &path,
        &task_roots(context)?,
        maximum(&context.policy["artifact_kinds"]["task_brief"]["max_bytes"])?,
    )
    .map_err(|error| {
        snapshot_error(error)
            .with_snapshot_trace(SnapshotJsonTraceSite::Plan(command, PlanSnapshot::Task))
    })?;
    let projection = 1_i64.into();
    let full_task = crate::domain::task_contract::validate_task_brief(
        task_snapshot
            .parsed
            .as_ref()
            .expect("safe JSON snapshot has parsed value"),
        &context.current_policy_digest.into(),
        context.policy,
        Some(&projection),
        context.files.windows(),
    )?;
    policy::validate_task_capability_paths(
        context.files,
        context.repository,
        &full_task,
        context.policy,
        Some(&context.store.state_root),
        Some(&projection),
    )?;
    for check in errors::iterable_values(string_item(&full_task, "required_check_ids")?)? {
        configured_check(context.policy, &check)?;
    }
    if full_task["schema_version"].py_eq(&3_i64.into()) {
        let allowance = string_item(&full_task, "helper_allowance")?;
        let path = reference_path(context, &allowance["ref"])?;
        let snapshot = snapshots::safe_json_snapshot_integer(
            context.snapshots,
            &path,
            &reference_roots(
                context.files,
                context.repository,
                &context.store.state_root,
                context.policy,
            )?,
            &BigInt::from(256 * 1024),
        )
        .map_err(|error| {
            snapshot_error(error).with_snapshot_trace(SnapshotJsonTraceSite::Plan(
                command,
                PlanSnapshot::Allowance,
            ))
        })?;
        if !allowance["sha256"].is_text(&snapshot.digest) {
            return Err(
                ContractError::new("helper_allowance.sha256", "INPUT_DIGEST_MISMATCH").into(),
            );
        }
        let mut ceilings = Vec::new();
        for role in helper_register::ELIGIBLE_PARENTS {
            ceilings.push((
                PyString::from(*role),
                Value::List(planner::effective_role_capabilities(
                    context.policy,
                    &full_task,
                    role,
                    None,
                    context.files.windows(),
                )?),
            ));
        }
        let ceilings = Value::Object(ceilings);
        let absolute = |text: &PyString| {
            context
                .files
                .from_string(text)
                .is_ok_and(|path| path.is_absolute())
        };
        helper_register::validate_allowance(
            snapshot
                .parsed
                .as_ref()
                .expect("safe JSON snapshot has parsed value"),
            request.run_id,
            Some(&Value::from(request.host)),
            Some(&ceilings),
            false,
            &absolute,
        )?;
    }
    let task = crate::domain::task_contract::authoritative_task_subset(&full_task)?
        .string_keyed_value()
        .expect("validated task has string-keyed dictionaries");
    let execution_plan =
        execution::build_execution_plan(&run_id, &task, request.size, request.host).map_err(
            |message| {
                ContractError::new(
                    if message == "EXECUTION_SIZE_BELOW_SAFETY_FLOOR" {
                        "size"
                    } else {
                        "model_overrides"
                    },
                    &message,
                )
            },
        )?;
    Ok(PreparedPlan {
        task_snapshot,
        full_task,
        task,
        execution_plan,
    })
}

pub fn execution_preflight(
    context: &SemanticContext<'_>,
    prepared: &PreparedPlan,
) -> EngineResult<Value> {
    let task = &prepared.full_task;
    let plan = &prepared.execution_plan;
    let roles: BTreeSet<_> = plan["assignments"]
        .as_list()
        .expect("validated plan assignments")
        .iter()
        .map(|assignment| assignment["role"].text().expect("validated role"))
        .collect();
    let mut capabilities = Value::Object(Vec::new());
    for role in &roles {
        capabilities.set(
            role,
            Value::List(planner::effective_role_capabilities(
                context.policy,
                task,
                role,
                Some(
                    plan.get("capability_projection_version")
                        .unwrap_or(&Value::Null),
                ),
                context.files.windows(),
            )?),
        );
    }
    let mut checks = Vec::new();
    for check_id in errors::iterable_values(string_item(task, "required_check_ids")?)? {
        let check = configured_check(context.policy, &check_id)?;
        let capability = Value::object([
            ("effect", "command".into()),
            ("action", "run".into()),
            ("target_ref", check["command_id"].clone()),
        ]);
        checks.push(Value::object([
            ("check_id", check_id),
            ("command_id", check["command_id"].clone()),
            (
                "capable_roles",
                Value::list(
                    roles
                        .iter()
                        .filter(|role| {
                            capabilities[role.as_str()]
                                .as_list()
                                .unwrap()
                                .iter()
                                .any(|allowed| allowed.py_eq(&capability))
                        })
                        .map(|role| Value::from(role.as_str())),
                ),
            ),
        ]));
    }
    let mut proposed_task = Value::object([
        (
            "path",
            Value::String(context.files.spelling(&prepared.task_snapshot.path)),
        ),
        ("sha256", prepared.task_snapshot.digest.clone().into()),
    ]);
    for key in [
        "schema_version",
        "task_id",
        "objective",
        "user_outcome",
        "scope",
        "acceptance_criteria",
    ] {
        proposed_task.set(key, string_item(task, key)?.clone());
    }
    Ok(Value::object([
        ("schema_version", 1_i64.into()),
        ("task", proposed_task),
        (
            "policy_schema_version",
            context.policy["schema_version"].clone(),
        ),
        ("policy_digest", context.current_policy_digest.into()),
        ("plan_digest", plan["plan_digest"].clone()),
        (
            "requested_capabilities",
            task["authority"]["capabilities"].clone(),
        ),
        ("role_capabilities", capabilities),
        ("required_checks", Value::List(checks)),
        (
            "required_human_decisions",
            task["required_human_decisions"].clone(),
        ),
        ("inspection_budget", task["inspection_budget"].clone()),
        (
            "loop_limits",
            Value::object([
                (
                    "design_revisions",
                    context.policy["limits"]["design_revisions"].clone(),
                ),
                (
                    "delivery_repairs",
                    context.policy["limits"]["delivery_repairs"].clone(),
                ),
            ]),
        ),
    ]))
}

pub(crate) fn constraint_markers(full_task: &Value) -> EngineResult<Vec<String>> {
    let prefix = "planning-constraints:v1:";
    let mut markers = Vec::new();
    for value in errors::iterable_values(string_item(full_task, "constraints")?)? {
        let text = value
            .as_string()
            .ok_or_else(|| errors::attribute_error(&value, "startswith"))?;
        if text.starts_with(prefix) {
            markers.push(contracts::digest(
                &Value::String(PyString(text.0[prefix.len()..].to_vec())),
                "planning_constraints",
            )?);
        }
    }
    if markers.len() > 1 {
        return Err(ContractError::new("planning_constraints", "MULTIPLE_ACTIVE_BUNDLES").into());
    }
    Ok(markers)
}

fn constraint_validation_error(
    failure: constraints::LocatedConstraintFailure,
    command: ConstraintCommand,
) -> EngineError {
    let error = EngineError::from(failure.error);
    match failure.site {
        Some(site) => {
            error.with_graphctl_trace(GraphctlTraceSite::ConstraintValidation(command, site))
        }
        None => error,
    }
}

fn constraint_support(
    resolver: &ArtifactResolver<'_>,
    roots: &[PathBuf],
    maximum: &BigInt,
    bundle: &Value,
    command: ConstraintCommand,
) -> EngineResult<Vec<VerifiedArtifact>> {
    constraints::validate_bundle_with_site(bundle)
        .map_err(|failure| constraint_validation_error(failure, command))?;
    let mut artifacts: Vec<VerifiedArtifact> = Vec::new();
    for record in bundle["records"].as_list().expect("validated records list") {
        for (key, kind) in [
            ("confirmed_finding_ref", "finding"),
            ("accepted_fix_ref", "acceptance_evidence"),
        ] {
            let reference = string_item(record, key)?;
            if reference.is_null() {
                continue;
            }
            let text = reference
                .as_string()
                .expect("validated reference is a string");
            let marker = PyString::from("#sha256=");
            let index = text
                .0
                .windows(marker.len())
                .rposition(|part| part == marker.0)
                .expect("validated reference contains digest");
            let digest = Value::String(PyString(text.0[index + marker.len()..].to_vec()));
            let artifact = resolver.resolve_reference(reference, &digest, &kind.into())?;
            if !artifact.source_path.truthy() {
                return Err(ContractError::new("constraint_support", "FILE_REQUIRED").into());
            }
            let payload = constraint_snapshot(resolver, &artifact.source_path, roots, maximum)
                .map_err(|error| {
                    error.with_snapshot_trace(SnapshotJsonTraceSite::ConstraintSupport(command))
                })?;
            let finding_id = get(&payload, "finding_id").map_err(|error| {
                EngineError::from(error)
                    .with_graphctl_trace(GraphctlTraceSite::ConstraintSupportPayload(command))
            })?;
            if !finding_id.py_eq(&record["finding_id"])
                || key == "accepted_fix_ref" && !get(&payload, "status")?.is_text("accepted")
            {
                return Err(ContractError::new(
                    "constraint_support",
                    "FINDING_FIX_IDENTITY_MISMATCH",
                )
                .into());
            }
            if let Some(previous) = artifacts
                .iter_mut()
                .find(|previous| previous.reference.py_eq(reference))
            {
                *previous = artifact
            } else {
                artifacts.push(artifact)
            }
        }
    }
    Ok(artifacts)
}

pub(crate) fn constraint_snapshot(
    resolver: &ArtifactResolver<'_>,
    input: &Value,
    roots: &[PathBuf],
    maximum: &BigInt,
) -> EngineResult<Value> {
    let path = policy::path_from_value(resolver.files, input)?;
    let snapshot = snapshots::safe_json_snapshot_integer(resolver.snapshots, &path, roots, maximum)
        .map_err(snapshot_error)?;
    Ok(snapshot
        .parsed
        .expect("safe JSON snapshot has parsed value"))
}

pub(crate) fn normalize_constraint_path(path: &PyString, windows: bool) -> PyString {
    let mut normalized = unicode::normcase(path, windows);
    for point in &mut normalized.0 {
        if *point == u32::from('\\') {
            *point = u32::from('/');
        }
    }
    normalized
}

pub(crate) fn preview_constraints(
    resolver: &ArtifactResolver<'_>,
    runtime_root: &Path,
    bundle_path: &Value,
    context_path: &Value,
) -> EngineResult<Value> {
    let roots = reference_roots(
        resolver.files,
        resolver.repository,
        runtime_root,
        resolver.policy,
    )?;
    let maximum = maximum(&resolver.policy["limits"]["manifest_bytes"])?;
    let bundle = constraint_snapshot(resolver, bundle_path, &roots, maximum)
        .map_err(|error| error.with_snapshot_trace(SnapshotJsonTraceSite::ConstraintBundle))?;
    let context = constraint_snapshot(resolver, context_path, &roots, maximum)
        .map_err(|error| error.with_snapshot_trace(SnapshotJsonTraceSite::ConstraintContext))?;
    let support = constraint_support(
        resolver,
        &roots,
        maximum,
        &bundle,
        ConstraintCommand::Preview,
    )?;
    let references = support
        .into_iter()
        .map(|artifact| artifact.reference)
        .collect::<Vec<_>>();
    constraints::select_constraints_with_site(
        &bundle,
        &context,
        constraints::Selection {
            activated: false,
            supported: &references,
            authority_refs: &[],
            normalize: &|path| normalize_constraint_path(path, resolver.files.windows()),
        },
    )
    .map_err(|failure| constraint_validation_error(failure, ConstraintCommand::Preview))
}

fn relative_posix(files: &dyn PolicyFilePort, path: &Path, root: &Path) -> EngineResult<PyString> {
    let path = files.from_string(&files.spelling(path))?;
    let root = files.from_string(&files.spelling(root))?;
    let Some(ancestor) = path
        .ancestors()
        .find(|ancestor| files.same_path(ancestor, &root))
    else {
        let mut message = repr::string_repr(&files.spelling(&path));
        message
            .0
            .extend(PyString::from(" is not in the subpath of ").0);
        message
            .0
            .extend(repr::string_repr(&files.spelling(&root)).0);
        return Err(EngineError::python("ValueError", message));
    };
    let relative = path
        .strip_prefix(ancestor)
        .expect("matched ancestor is a path prefix");
    let mut text = files.spelling(relative);
    if text.is_empty() {
        text = ".".into()
    }
    if files.windows() {
        for point in &mut text.0 {
            if *point == 92 {
                *point = 47
            }
        }
    }
    Ok(text)
}

fn artifact_record(artifact: &VerifiedArtifact) -> Value {
    Value::object([
        ("ref", artifact.reference.clone()),
        ("kind", artifact.kind.clone()),
        ("sha256", artifact.sha256.clone()),
        ("size_bytes", artifact.size_bytes.clone()),
        ("source_type", artifact.source_type.clone()),
        ("source_path", artifact.source_path.clone()),
        ("content_json", artifact.content_json.clone()),
        ("device", artifact.device.clone()),
        ("inode", artifact.inode.clone()),
    ])
}

pub(crate) fn node_row(spec: &planner::NodeSpec, envelope: &Value) -> EngineResult<Value> {
    Ok(Value::object([
        ("branch_id", envelope["branch_id"].clone()),
        ("node_instance_id", envelope["node_instance_id"].clone()),
        ("node_key", spec.key.as_str().into()),
        ("role", spec.role.as_str().into()),
        ("stage", spec.stage.as_str().into()),
        ("generation", spec.generation.into()),
        ("mandatory", spec.mandatory.into()),
        (
            "specialist_tag",
            spec.specialist_tag
                .as_ref()
                .map_or(Value::Null, |tag| tag.as_str().into()),
        ),
        ("status", envelope["status"].clone()),
        ("retry_count", envelope["retry_count"].clone()),
        ("max_retries", envelope["max_retries"].clone()),
        (
            "envelope_json",
            String::from_utf8(compat::dumps(envelope, SerializationProfile::CLI)?)
                .expect("ASCII JSON is UTF-8")
                .into(),
        ),
    ]))
}

pub fn initialize_run(
    context: &SemanticContext<'_>,
    request: &InitRequest<'_>,
) -> EngineResult<Value> {
    let run_id = contracts::opaque(request.plan.run_id, "run_id")?;
    let operation = contracts::opaque(request.operation_id, "op_id")?;
    let PreparedPlan {
        task_snapshot,
        full_task,
        mut task,
        execution_plan,
    } = prepare_execution_plan_traced(context, &request.plan, PlanTraceCommand::Initialize)?;
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
    let mut verified = Vec::new();
    for reference in full_task["evidence_paths"]
        .as_list()
        .expect("validated evidence list")
    {
        verified
            .push(resolver.resolve_unhashed_reference(reference, &"acceptance_evidence".into())?);
    }
    let mut support = Vec::new();
    for marker in constraint_markers(&full_task)? {
        let bundle = verified
            .iter()
            .find(|artifact| artifact.sha256.is_text(&marker));
        let Some(bundle) = bundle.filter(|artifact| artifact.source_path.truthy()) else {
            return Err(
                ContractError::new("planning_constraints", "ACTIVATION_BUNDLE_MISSING").into(),
            );
        };
        let roots = reference_roots(
            context.files,
            context.repository,
            &context.store.state_root,
            context.policy,
        )?;
        let maximum = maximum(&context.policy["limits"]["manifest_bytes"])?;
        let bundle = constraint_snapshot(&resolver, &bundle.source_path, &roots, maximum)
            .map_err(|error| error.with_snapshot_trace(SnapshotJsonTraceSite::ActivatedBundle))?;
        support.extend(constraint_support(
            &resolver,
            &roots,
            maximum,
            &bundle,
            ConstraintCommand::Initialize,
        )?);
        for record in bundle["records"]
            .as_list()
            .expect("validated bundle records")
        {
            if record["state"].is_text("active")
                && !record["authority_ref"].py_eq(&full_task["policy_approval"]["authority_ref"])
            {
                return Err(
                    ContractError::new("planning_constraints", "AUTHORITY_MISMATCH").into(),
                );
            }
        }
    }
    let mut refs = verified
        .iter()
        .map(|artifact| artifact.reference.clone())
        .collect::<Vec<_>>();
    crate::domain::python_operations::sort_comparable_values(&mut refs)?;
    task.set("evidence_paths", Value::List(refs));
    let spec = planner::bootstrap(context.policy)?;
    let modern = context.policy["schema_version"].py_eq(&3_i64.into());
    let root = if modern {
        context.store.state_root.clone()
    } else {
        context.files.absolute(context.repository)?
    };
    let mut task_ref = PyString::from(if modern { "runtime:" } else { "repo:" });
    task_ref
        .0
        .extend(relative_posix(context.files, &task_snapshot.path, &root)?.0);
    task_ref
        .0
        .extend(PyString::from(format!("#sha256={}", task_snapshot.digest)).0);
    let task_ref = Value::String(task_ref);
    let task_artifact = VerifiedArtifact {
        reference: task_ref.clone(),
        kind: "task_brief".into(),
        sha256: task_snapshot.digest.clone().into(),
        size_bytes: Value::Integer(task_snapshot.size.into()),
        source_type: if modern {
            "runtime".into()
        } else {
            "repo".into()
        },
        source_path: Value::String(context.files.spelling(&task_snapshot.path)),
        content_json: Value::Null,
        device: Value::Integer(task_snapshot.identity.device.into()),
        inode: Value::Integer(task_snapshot.identity.inode.into()),
    };
    let input = Value::object([
        ("kind", "task_brief".into()),
        ("ref", task_ref.clone()),
        ("sha256", task_snapshot.digest.clone().into()),
        ("size_bytes", task_snapshot.size.into()),
    ]);
    let envelope = planner::envelope(
        &run_id,
        context.current_policy_digest,
        context.policy,
        &task,
        &spec,
        "ready",
        &[input],
        0,
        Some(&execution_plan),
        context.files.windows(),
    )?;
    let bootstrap = node_row(&spec, &envelope)?;
    let mut evidence_digests = verified
        .iter()
        .map(|artifact| Value::tuple([artifact.reference.clone(), artifact.sha256.clone()]))
        .collect::<Vec<_>>();
    crate::domain::python_operations::sort_comparable_values(&mut evidence_digests)?;
    let canonical_request = Value::object([
        ("command", "init".into()),
        ("run_id", run_id.clone().into()),
        ("op_id", operation.clone().into()),
        ("policy_digest", context.current_policy_digest.into()),
        ("task_digest", task_snapshot.digest.clone().into()),
        ("evidence_digests", Value::List(evidence_digests)),
        ("ack_permissions", request.acknowledge_permissions.into()),
        ("ack_durability", request.acknowledge_durability.into()),
        ("execution_size", execution_plan["size"].clone()),
        (
            "execution_plan_digest",
            execution_plan["plan_digest"].clone(),
        ),
    ]);
    let digest = ids::sha256_bytes(&ids::canonical_bytes(&canonical_request)?);
    let mut artifacts = vec![artifact_record(&task_artifact)];
    artifacts.extend(verified.iter().chain(&support).map(artifact_record));
    context.store.initialize(&InitializeRequest {
        repository: context.repository,
        policy: context.policy,
        policy_digest: context.current_policy_digest,
        task: &task,
        task_path: &task_snapshot.path,
        task_digest: &task_snapshot.digest,
        run_id: &run_id,
        operation_id: &operation,
        request_digest: &digest,
        acknowledge_permissions: request.acknowledge_permissions,
        acknowledge_durability: request.acknowledge_durability,
        bootstrap: &bootstrap,
        execution_plan: &execution_plan,
        task_ref: &task_ref,
        initial_artifacts: &artifacts,
    })
}
