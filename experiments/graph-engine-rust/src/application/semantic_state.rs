use super::{
    artifacts::ArtifactResolver,
    check_receipts::{self, CheckProvenancePort},
    error::{
        EngineError, EngineResult, MutationValidationPhase, PersistedTaskValidation,
        SnapshotJsonTraceSite, TaskValidationMutation,
    },
    evidence_integrity,
    execution_validation::validate_execution_plan,
    graph_ledger::GraphRead,
    persistent_store::{PersistentSession, StateStore},
    policy::{self, PolicyFilePort},
    semantic_delegation, semantic_fanouts, semantic_joins, semantic_nodes, semantic_route,
    semantic_topology,
    snapshots::{self, IntegerFileSnapshotPort, SnapshotFailure},
    state::RunRecord,
    state_history,
};
use crate::{
    compat::{self, PyString, Value},
    domain::{
        check_receipts::configured_check,
        contract_values::{contains, get, in_strings},
        errors::{self, string_item},
        ids, policy_validation, python_operations, task_contract,
    },
};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
pub struct SemanticContext<'a> {
    pub store: &'a StateStore<'a>,
    pub files: &'a dyn PolicyFilePort,
    pub snapshots: &'a dyn IntegerFileSnapshotPort,
    pub provenance: &'a dyn CheckProvenancePort,
    pub repository: &'a Path,
    pub skill: &'a Path,
    pub installation: &'a Path,
    pub runtime: Option<&'a Path>,
    pub policy: &'a Value,
    pub current_policy_digest: &'a str,
    pub case_sensitive: bool,
    pub evidence_enable: bool,
}

fn invalid(code: &str) -> EngineError {
    EngineError::state(code)
}

fn snapshot_error(error: SnapshotFailure) -> EngineError {
    match error {
        SnapshotFailure::Contract(error) => error.into(),
        SnapshotFailure::Io(error) => error.into(),
        SnapshotFailure::Json(error) => error.into(),
    }
}

fn lifecycle_shape_error(error: &EngineError) -> bool {
    let kinds = [
        "KeyError",
        "TypeError",
        "IndexError",
        "ValueError",
        "JSONDecodeError",
        "UnicodeDecodeError",
        "UnicodeEncodeError",
        "UnicodeTranslateError",
    ];
    match error {
        EngineError::Contract(_) => true,
        EngineError::Python { kind, .. } => kinds.contains(kind),
        EngineError::Json(error) => kinds.contains(&error.kind),
        _ => false,
    }
}

fn database_error(error: &EngineError) -> bool {
    matches!(
        error,
        EngineError::Storage {
            kind: "DatabaseError"
                | "DataError"
                | "OperationalError"
                | "IntegrityError"
                | "InternalError"
                | "ProgrammingError"
                | "NotSupportedError",
            ..
        }
    )
}

fn joined_root(
    context: &SemanticContext<'_>,
    root: &Path,
    component: &Value,
) -> EngineResult<PathBuf> {
    let text = component.as_string().ok_or_else(|| {
        EngineError::python(
            "TypeError",
            format!(
                "unsupported operand type(s) for /: '{}' and '{}'",
                if context.files.windows() {
                    "WindowsPath"
                } else {
                    "PosixPath"
                },
                component.python_type()
            ),
        )
    })?;
    Ok(root.join(context.files.from_string(text)?))
}

fn task_document(
    session: &dyn PersistentSession,
    run: &Value,
    context: &SemanticContext<'_>,
    trace: Option<PersistedTaskValidation>,
) -> EngineResult<(Value, PathBuf)> {
    let path = string_item(run, "task_path")?;
    let path = policy::path_from_value(context.files, path)?;
    let selected_root = match context.runtime {
        Some(root) => root.to_path_buf(),
        None => policy::resolve_state_root(context.files, None, context.installation)?,
    };
    let modern = string_item(context.policy, "schema_version")?.py_eq(&3_i64.into());
    let artifact_roots = string_item(context.policy, "artifact_roots")?;
    let roots = string_item(artifact_roots, if modern { "runtime" } else { "repo" })?;
    let mut permitted = Vec::new();
    for root in errors::iterable_values(roots)? {
        permitted.push(joined_root(
            context,
            if modern {
                &selected_root
            } else {
                context.repository
            },
            &root,
        )?);
    }
    let maximum = string_item(
        string_item(string_item(context.policy, "artifact_kinds")?, "task_brief")?,
        "max_bytes",
    )?;
    let maximum = maximum.strict_integer().ok_or_else(|| {
        EngineError::python(
            "TypeError",
            format!(
                "'{}' object cannot be interpreted as an integer",
                maximum.python_type()
            ),
        )
    })?;
    let snapshot =
        snapshots::safe_json_snapshot_integer(context.snapshots, &path, &permitted, maximum)
            .map_err(|failure| {
                let error = snapshot_error(failure);
                match trace {
                    Some(site) => {
                        error.with_snapshot_trace(SnapshotJsonTraceSite::PersistedTask(site))
                    }
                    None => error,
                }
            })?;
    if !string_item(run, "task_digest")?.is_text(&snapshot.digest) {
        return Err(invalid("INPUT_DIGEST_MISMATCH"));
    }
    let task = compat::loads_value(string_item(run, "task_json")?)?;
    let plan = validate_execution_plan(session, run, &task)?;
    get(&plan, "capability_projection_version")?;
    let projection = plan
        .get("capability_projection_version")
        .filter(|value| !value.is_null());
    let digest = string_item(run, "policy_digest")?
        .as_string()
        .expect("policy digest matched the current string");
    let full = task_contract::validate_task_brief(
        snapshot.parsed.as_ref().unwrap(),
        digest,
        context.policy,
        projection,
        context.files.windows(),
    )?;
    policy::validate_task_capability_paths(
        context.files,
        context.repository,
        &full,
        context.policy,
        Some(&selected_root),
        projection,
    )?;
    let mut expected = task_contract::authoritative_task_subset(&full)?
        .string_keyed_value()
        .expect("validated task dictionary has string keys");
    let empty = Value::List(Vec::new());
    get(&task, "evidence_paths")?;
    let evidence = task.get("evidence_paths").unwrap_or(&empty);
    let mut paths = Vec::new();
    let marker = PyString::from("#sha256=");
    for reference in errors::iterable_values(evidence)? {
        let text = reference
            .as_string()
            .ok_or_else(|| errors::attribute_error(&reference, "split"))?;
        let end = text
            .0
            .windows(marker.len())
            .position(|part| part == marker.0)
            .unwrap_or(text.len());
        paths.push(Value::String(PyString(text.0[..end].to_vec())));
    }
    let mut expected_paths = errors::iterable_values(string_item(&full, "evidence_paths")?)?;
    python_operations::sort_comparable_values(&mut expected_paths)?;
    if !Value::List(paths).py_eq(&Value::List(expected_paths)) {
        return Err(invalid("TASK_METADATA_INVALID"));
    }
    expected.set("evidence_paths", evidence.clone());
    if !task.py_eq(&expected) {
        return Err(invalid("TASK_METADATA_INVALID"));
    }
    Ok((task, selected_root))
}

fn validate_evidence(
    session: &dyn PersistentSession,
    run: &Value,
    task: &Value,
    context: &SemanticContext<'_>,
) -> EngineResult<()> {
    let run_id = string_item(run, "run_id")?;
    for (query, id_column, valid_ids, kind, check) in [
        (
            GraphRead::AcceptanceForValidation,
            "criterion_id",
            errors::iterable_set(string_item(task, "acceptance_ids")?)?,
            "acceptance_evidence",
            false,
        ),
        (
            GraphRead::ChecksForValidation,
            "check_id",
            errors::iterable_set(string_item(task, "required_check_ids")?)?,
            "check_evidence",
            true,
        ),
    ] {
        session.visit(query, &[run_id.clone()], &mut |row| {
            let artifact = session.first(
                GraphRead::Artifact,
                &[string_item(&row, "artifact_ref")?.clone()],
            )?;
            let identifier = string_item(&row, id_column)?;
            errors::require_hashable(identifier)?;
            if !errors::set_contains(&valid_ids, identifier) || artifact.is_none() {
                return Err(invalid("EVIDENCE_STATE_INVALID"));
            }
            let artifact = artifact.unwrap();
            if !string_item(&artifact, "kind")?.is_text(kind)
                || !string_item(&artifact, "sha256")?.py_eq(string_item(&row, "artifact_sha256")?)
            {
                return Err(invalid("EVIDENCE_STATE_INVALID"));
            }
            if check {
                if !string_item(&artifact, "source_type")?.is_text("ledger")
                    || !string_item(&artifact, "content_json")?.truthy()
                {
                    return Err(invalid("CHECK_EVIDENCE_PROVENANCE_INVALID"));
                }
                let verified = (|| -> EngineResult<Value> {
                    let receipt = compat::loads_value(string_item(&artifact, "content_json")?)?;
                    let configured = configured_check(context.policy, identifier)?;
                    if string_item(run, "state_schema_version")?.py_eq(&7_i64.into())
                        || context.evidence_enable
                    {
                        check_receipts::validate_receipt_integrity(
                            &receipt,
                            run_id,
                            identifier,
                            configured,
                            context.provenance,
                        )?;
                    } else {
                        check_receipts::validate_check_receipt(
                            &receipt,
                            run_id,
                            identifier,
                            configured,
                            context.provenance,
                            true,
                        )?;
                    }
                    Ok(receipt)
                })();
                let receipt = match verified {
                    Err(EngineError::Contract(_) | EngineError::State(_)) => {
                        return Err(invalid("CHECK_EVIDENCE_PROVENANCE_INVALID"))
                    }
                    Err(EngineError::Json(error)) if error.kind == "JSONDecodeError" => {
                        return Err(invalid("CHECK_EVIDENCE_PROVENANCE_INVALID"))
                    }
                    other => other?,
                };
                if !string_item(&row, "outcome")?.py_eq(string_item(&receipt, "outcome")?) {
                    return Err(invalid("CHECK_EVIDENCE_PROVENANCE_INVALID"));
                }
            }
            Ok(())
        })?;
    }
    session.visit(
        GraphRead::ApprovalsForValidation,
        &[run_id.clone()],
        &mut |row| {
            let artifact = session.first(
                GraphRead::Artifact,
                &[string_item(&row, "scope_ref")?.clone()],
            )?;
            let decisions = errors::iterable_set(string_item(task, "required_human_decisions")?)?;
            let identifier = string_item(&row, "approval_id")?;
            errors::require_hashable(identifier)?;
            if !errors::set_contains(&decisions, identifier)
                || !in_strings(string_item(&row, "decision")?, &["APPROVE", "REJECT"])?
                || artifact.is_none()
            {
                return Err(invalid("APPROVAL_STATE_INVALID"));
            }
            let artifact = artifact.unwrap();
            if !string_item(&artifact, "kind")?.is_text("acceptance_evidence")
                || !string_item(&artifact, "sha256")?.py_eq(string_item(&row, "artifact_sha256")?)
            {
                return Err(invalid("APPROVAL_STATE_INVALID"));
            }
            let attestation = session
                .first(
                    GraphRead::ApprovalAttestation,
                    &[run_id.clone(), identifier.clone()],
                )?
                .ok_or_else(|| invalid("APPROVAL_ATTESTATION_INVALID"))?;
            let mut attested = Value::object([("run_id", run_id.clone())]);
            for key in [
                "approval_id",
                "scope_ref",
                "decision",
                "authority_ref",
                "artifact_sha256",
            ] {
                attested.set(key, string_item(&row, key)?.clone());
            }
            for key in ["actor", "host_identity", "approved_at"] {
                attested.set(key, string_item(&attestation, key)?.clone());
            }
            let expected = ids::sha256_bytes(&ids::canonical_bytes(&attested)?);
            if !string_item(&attestation, "host_identity")?
                .is_text(&context.store.platform.host_identity()?)
                || !string_item(&attestation, "actor")?.truthy()
                || !string_item(&attestation, "approval_digest")?.is_text(&expected)
            {
                return Err(invalid("APPROVAL_ATTESTATION_INVALID"));
            }
            Ok(())
        },
    )
}

fn verify_inner(
    session: &dyn PersistentSession,
    run: &Value,
    context: &SemanticContext<'_>,
    trace: Option<PersistedTaskValidation>,
) -> EngineResult<()> {
    let schema = string_item(run, "state_schema_version")?;
    errors::require_hashable(schema)?;
    if !schema.py_eq(&Value::from(i64::from(crate::STATE_SCHEMA_VERSION)))
        && !schema.py_eq(&7_i64.into())
    {
        return Err(invalid("UNSUPPORTED_STATE_SCHEMA"));
    }
    if schema.py_eq(&7_i64.into()) {
        let lifecycle = (|| -> EngineResult<()> {
            let task = compat::loads_value(string_item(run, "task_json")?)?;
            evidence_integrity::validate_lifecycle_integrity(
                session,
                run,
                context.policy,
                &task,
                context.provenance,
                context.files.windows(),
            )
        })();
        if let Err(error) = lifecycle {
            if lifecycle_shape_error(&error) {
                return Err(invalid("EVIDENCE_LIFECYCLE_INVALID"));
            }
            return Err(error);
        }
    }
    let compatible = match policy_validation::engine_version_compatible(
        string_item(run, "engine_version")?,
        context.policy,
    ) {
        Ok(value) => value,
        Err(errors::ValidationFailure::Contract(_)) => false,
        Err(error) => return Err(error.into()),
    };
    if !compatible {
        return Err(invalid("ENGINE_VERSION_INCOMPATIBLE"));
    }
    if !in_strings(
        string_item(run, "status")?,
        &["initialized", "active", "blocked", "complete", "aborted"],
    )? || !string_item(run, "authoritative")?.py_eq(&1_i64.into())
    {
        return Err(invalid("RUN_STATE_INVALID"));
    }
    if !string_item(run, "policy_digest")?.is_text(context.current_policy_digest) {
        return Err(invalid("CONFIG_CHANGED"));
    }
    if !string_item(run, "host_identity")?.is_text(&context.store.platform.host_identity()?) {
        return Err(invalid("HOST_IDENTITY_MISMATCH"));
    }
    let (repository, display) = context.store.repository_identity(context.repository)?;
    if !string_item(run, "repository_device")?.py_eq(&Value::Integer(repository.device.into()))
        || !string_item(run, "repository_inode")?.py_eq(&Value::Integer(repository.inode.into()))
        || !string_item(run, "repository_path")?.py_eq(&Value::String(display))
    {
        return Err(invalid("REPOSITORY_IDENTITY_MISMATCH"));
    }
    let (task, selected_root) = task_document(session, run, context, trace)?;
    state_history::validate_budget_state(session, run, &task, context.policy)?;
    state_history::validate_operation_ledger(session, run)?;
    semantic_nodes::validate_nodes(session, run, &task, context.policy, context.files.windows())?;
    semantic_joins::validate_joins(session, run, context.policy)?;
    semantic_topology::validate_reconstructed_graph(session, run)?;
    semantic_route::validate_route_and_topology(session, run, &task, context.policy)?;
    semantic_fanouts::validate_fanouts(session, run, context.case_sensitive)?;
    semantic_delegation::validate_review_delegation(session, run, context.case_sensitive)?;
    state_history::validate_attempts(session, run)?;
    let policy = match compat::loads_value(string_item(run, "policy_json")?) {
        Err(error) if error.kind == "JSONDecodeError" => {
            return Err(invalid("POLICY_STATE_INVALID"))
        }
        other => other?,
    };
    if !policy.py_eq(context.policy) {
        return Err(invalid("POLICY_STATE_INVALID"));
    }
    if !matches!(string_item(run,"local_filesystem")?,Value::String(text) if !text.is_empty())
        || python_operations::less(string_item(run, "design_generation")?, &0_i64.into())?
        || python_operations::less(
            string_item(run, "implementation_generation")?,
            &0_i64.into(),
        )?
    {
        return Err(invalid("RUN_STATE_INVALID"));
    }
    let route = string_item(run, "selected_route")?;
    if !route.is_null() {
        let raw = string_item(run, "selected_tags_json")?;
        let empty = Value::from("[]");
        let tags = compat::loads_value(if raw.truthy() { raw } else { &empty })?;
        if !contains(string_item(context.policy, "routes")?, route)? {
            return Err(invalid("RUN_STATE_INVALID"));
        }
        let mut sorted = errors::iterable_set(&tags)?;
        python_operations::sort_comparable_values(&mut sorted)?;
        if !tags.py_eq(&Value::List(sorted)) {
            return Err(invalid("RUN_STATE_INVALID"));
        }
        for tag in errors::iterable_values(&tags)? {
            if !contains(string_item(context.policy, "impact_tags")?, &tag)? {
                return Err(invalid("RUN_STATE_INVALID"));
            }
        }
    }
    validate_evidence(session, run, &task, context)?;
    let run_id = string_item(run, "run_id")?;
    let artifacts = session.all(GraphRead::ArtifactsForValidation, &[run_id.clone()])?;
    if artifacts.is_empty()
        || session
            .first(
                GraphRead::ArtifactExists,
                &[string_item(run, "task_ref")?.clone()],
            )?
            .is_none()
    {
        return Err(invalid("ARTIFACT_REGISTRY_INVALID"));
    }
    let resolver = ArtifactResolver {
        files: context.files,
        snapshots: context.snapshots,
        repository: context.repository,
        skill: context.skill,
        installation: context.installation,
        policy: context.policy,
        runtime: Some(&selected_root),
        ledger: Some(session),
    };
    for artifact in artifacts {
        resolver.reverify_artifact(&artifact)?;
    }
    Ok(())
}

pub fn verify_semantic_state(
    session: &dyn PersistentSession,
    run: &Value,
    context: &SemanticContext<'_>,
) -> EngineResult<()> {
    verify_semantic_state_with_trace(session, run, context, None)
}

pub(super) fn verify_read_only(
    session: &dyn PersistentSession,
    run: &Value,
    context: &SemanticContext<'_>,
) -> EngineResult<()> {
    verify_semantic_state_with_trace(
        session,
        run,
        context,
        Some(PersistedTaskValidation::ReadOnly),
    )
}

pub(super) fn mutation_task_validator<'a>(
    context: &'a SemanticContext<'a>,
    command: TaskValidationMutation,
) -> impl FnMut(&dyn PersistentSession, &RunRecord) -> EngineResult<()> + 'a {
    // A mutation validates once before its action and again before commit.
    let mut phase = MutationValidationPhase::BeforeAction;
    move |session, run| {
        let site = PersistedTaskValidation::Mutation(command, phase);
        phase = MutationValidationPhase::AfterAction;
        verify_semantic_state_with_trace(session, &run.fields, context, Some(site))
    }
}

pub(super) fn verify_semantic_state_with_trace(
    session: &dyn PersistentSession,
    run: &Value,
    context: &SemanticContext<'_>,
    trace: Option<PersistedTaskValidation>,
) -> EngineResult<()> {
    match verify_inner(session, run, context, trace) {
        Err(error) if database_error(&error) => Err(invalid("DATABASE_STATE_INVALID")),
        other => other,
    }
}

pub fn verify_resume(
    session: &dyn PersistentSession,
    run: &Value,
    context: &SemanticContext<'_>,
) -> EngineResult<()> {
    let mut selected = *context;
    selected.evidence_enable = false;
    verify_semantic_state(session, run, &selected)
}
