use super::{
    artifacts::{persist_artifact, ArtifactResolver},
    check_eligibility::CheckEligibilityPort,
    claim_runtime::ClaimServices,
    error::{EngineError, EngineResult, SnapshotJsonTraceSite},
    evidence_lifecycle::{current_review_binding, evidence_activation, verify_review_source},
    evidence_verification::EvidenceVerification,
    graph_initialization::{maximum, reference_roots},
    graph_ledger::GraphRead,
    graph_payload::{branch_result_input, persist_payload, sorted_inputs},
    graph_recovery::repair_packet,
    graph_writes::GraphWrite,
    persistent_store::PersistentSession,
    policy,
    run_context::RunContext,
    semantic_state::SemanticContext,
    snapshots::{safe_json_snapshot_integer, SnapshotFailure},
    source_capture::{bounded_git, MAX_ENUMERATION_BYTES, SOURCE_TIMEOUT},
};
use crate::{
    compat::{self, unicode, PyString, SerializationProfile, Value},
    domain::{
        contracts::{require_keys, ContractError},
        errors, ids, planner, python_operations,
        result_contract::is_actionable_finding,
        source_paths,
    },
};
use num_bigint::BigInt;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(super) fn verify_delivery_result_source(
    session: &dyn PersistentSession,
    run: &Value,
    envelope: &Value,
    artifact: &Value,
    context: &SemanticContext<'_>,
    claims: &ClaimServices<'_>,
) -> EngineResult<()> {
    let registering = Value::Tuple(vec![envelope["node_key"].clone(), artifact.clone()]);
    let (binding, _) = verified_review_source(session, run, context, claims, Some(&registering))?;
    if !envelope["inputs"]
        .as_list()
        .expect("validated envelope inputs")
        .iter()
        .any(|input| input["ref"].py_eq(&binding["ref"]))
    {
        return Err(EngineError::state("REVIEW_BINDING_MISMATCH"));
    }
    Ok(())
}

pub(super) fn verified_review_source(
    session: &dyn PersistentSession,
    run: &Value,
    context: &SemanticContext<'_>,
    claims: &ClaimServices<'_>,
    registering: Option<&Value>,
) -> EngineResult<(Value, Value)> {
    let files = claims
        .source_files
        .create(context.repository, context.policy, context.files);
    let verification = EvidenceVerification {
        session,
        run,
        context: *context,
        clock: claims.runtime,
        source_files: &*files,
        process: claims.process,
    };
    verify_review_source(
        session,
        claims.runtime,
        &verification,
        run,
        registering,
        true,
    )
}

pub(super) fn staged_consolidation(
    session: &dyn PersistentSession,
    run: &Value,
    raw_manifest: &Value,
    resolver: &ArtifactResolver<'_>,
    context: &SemanticContext<'_>,
) -> EngineResult<Option<Value>> {
    let Some((_, binding)) = current_review_binding(session, run)? else {
        return Ok(None);
    };
    let mut staged_reference = None;
    for output in errors::iterable_values(&binding["coverage"]["outputs"])? {
        if !output["purpose"].is_text("consolidation") {
            continue;
        }
        let path = policy::path_from_value(context.files, &output["path"])?;
        if !context.files.exists(&context.repository.join(path), true)? {
            continue;
        }
        let reference = python_operations::add(&"repo:".into(), &output["path"])?;
        let staged =
            resolver.resolve_unhashed_reference(&reference, &"delivery_consolidation".into())?;
        let roots = reference_roots(
            context.files,
            context.repository,
            &context.store.state_root,
            context.policy,
        )?;
        let path = policy::path_from_value(context.files, &staged.source_path)?;
        let snapshot = safe_json_snapshot_integer(
            context.snapshots,
            &path,
            &roots,
            maximum(&context.policy["limits"]["manifest_bytes"])?,
        )
        .map_err(|failure| match failure {
            SnapshotFailure::Contract(error) => EngineError::from(error),
            SnapshotFailure::Io(error) => EngineError::from(error),
            SnapshotFailure::Json(error) => EngineError::from(error)
                .with_snapshot_trace(SnapshotJsonTraceSite::StagedConsolidation),
        })?;
        if !snapshot
            .parsed
            .expect("safe JSON snapshot is parsed")
            .py_eq(raw_manifest)
        {
            return Err(EngineError::state("GENERATED_OUTPUT_RESULT_MISMATCH"));
        }
        let run_id = run["run_id"].text().expect("validated run ID");
        persist_artifact(session, &run_id, &staged)?;
        staged_reference = Some(Value::object([
            ("kind", staged.kind),
            ("ref", staged.reference),
            ("sha256", staged.sha256),
        ]));
    }
    Ok(staged_reference)
}

fn repository_reference(reference: &Value) -> Option<PyString> {
    let reference = reference.as_string()?;
    reference.starts_with("repo:").then(|| {
        let marker = PyString::from("#sha256=");
        let end = reference
            .0
            .windows(marker.len())
            .position(|window| window == marker.0)
            .unwrap_or(reference.len());
        PyString(reference.0[5..end].to_vec())
    })
}

pub(super) fn attach_delivery_repair_packet(
    session: &dyn PersistentSession,
    run: &Value,
    join_id: &Value,
    collection: &Value,
    members: &[Value],
    consolidation: &Value,
    writer_id: &Value,
    revision: &BigInt,
    context: &SemanticContext<'_>,
) -> EngineResult<()> {
    let writer = session
        .first(GraphRead::Node, std::slice::from_ref(writer_id))?
        .ok_or_else(|| EngineError::state("BRANCH_NOT_FOUND"))?;
    let mut packet = repair_packet(session, run, &writer, None, context.files.windows())?;
    let mut origins = Vec::with_capacity(members.len());
    let mut reviewer_keys = BTreeSet::new();
    for member in members {
        let envelope = compat::loads_value(&member["envelope_json"])?;
        let result = compat::loads_value(&member["result_json"])?;
        let mut findings = Vec::new();
        let mut observations = Vec::new();
        for finding in result
            .get("findings")
            .and_then(Value::as_list)
            .unwrap_or_default()
        {
            if is_actionable_finding(finding)? {
                findings.push(finding.clone());
            } else {
                observations.push(finding.clone());
            }
        }
        origins.push(Value::object([
            ("branch_id", member["branch_id"].clone()),
            ("attempt_id", envelope["attempt_id"].clone()),
            ("result_ref", branch_result_input(session, member, &result)?),
            ("artifact_ref", envelope["artifact_ref"].clone()),
            ("findings", Value::List(findings)),
            ("observations", Value::List(observations)),
            ("evidence", envelope["evidence"].clone()),
        ]));
        reviewer_keys.insert(
            member["node_key"]
                .as_string()
                .expect("validated node key")
                .clone(),
        );
    }
    let mut return_gates = vec!["senior_engineer".into()];
    return_gates.extend(reviewer_keys.into_iter().map(Value::String));
    return_gates.push(consolidation["node_key"].clone());
    let result = compat::loads_value(&consolidation["result_json"])?;
    packet.set("origin_kind", "delivery_review".into());
    packet.set("origin_gate", consolidation["node_key"].clone());
    packet.set("origin_collection_id", collection["join_id"].clone());
    packet.set(
        "origin_consolidation_ref",
        branch_result_input(session, consolidation, &result)?,
    );
    packet.set("origin_reports", Value::List(origins));
    packet.set("return_gates", Value::List(return_gates));
    packet.set("next_permitted_action", "implement_repair".into());
    packet.set("state_revision", Value::Integer(revision.clone()));
    let identifier = format!(
        "delivery-repair-{}",
        join_id.text().expect("validated join ID")
    );
    let artifact = persist_payload(
        session,
        run,
        context.policy,
        &identifier.into(),
        &"failure".into(),
        &packet,
    )?;
    let mut envelope = compat::loads_value(&writer["envelope_json"])?;
    let mut inputs = errors::iterable_values(&envelope["inputs"])?;
    inputs.push(artifact.as_input());
    envelope.set("inputs", sorted_inputs(inputs)?);
    let json = String::from_utf8(compat::dumps(
        &envelope,
        SerializationProfile::SORTED_STORED,
    )?)
    .expect("stored JSON is UTF-8");
    session.write(
        GraphWrite::UpdateNodeEnvelope,
        &[json.into(), writer_id.clone()],
    )
}

fn prepare_outputs(
    loaded: &RunContext,
    verification: &EvidenceVerification<'_>,
    repository: &Path,
    specs: &[planner::NodeSpec],
    inputs: &[Value],
    activation: &Value,
    plan: &Value,
) -> EngineResult<Value> {
    let fields = ["schema_version", "kind", "outputs"];
    require_keys(plan, &fields, &fields, "generated_outputs")?;
    let outputs = plan["outputs"]
        .as_list()
        .filter(|outputs| outputs.len() <= 32)
        .filter(|_| {
            plan["schema_version"].py_eq(&1_i64.into())
                && plan["kind"].is_text("generated_output_plan")
        })
        .ok_or_else(|| ContractError::new("generated_outputs", "SCHEMA_MISMATCH"))?;
    if outputs.is_empty() {
        return Ok(Value::List(vec![]));
    }
    let policy = &loaded.policy;
    if policy["schema_version"].py_eq(&3_i64.into()) {
        return Err(ContractError::new("generated_outputs", "RUNTIME_ARTIFACTS_REQUIRED").into());
    }

    let mut producers = BTreeMap::new();
    for spec in specs {
        let template = planner::node_template(policy, &spec.key)?;
        producers.insert(
            spec.key.as_str(),
            template["output_contract"]["artifact_kind"].clone(),
        );
    }
    producers.insert(
        "supervisor_delivery_consolidation",
        "delivery_consolidation".into(),
    );
    let writes = errors::iterable_values(&loaded.task["authority"]["capabilities"])?
        .into_iter()
        .filter(|capability| {
            capability["effect"].is_text("filesystem_write") && capability["action"].is_text("edit")
        })
        .filter_map(|capability| {
            let target = capability["target_ref"].as_string()?;
            target
                .starts_with("repo:")
                .then(|| PyString(target.0[5..].to_vec()))
        })
        .collect::<Vec<_>>();
    let mut protected = Vec::new();
    if let Some(roots) = policy.get("implementation_roots") {
        protected.extend(errors::iterable_values(roots)?);
    }
    for (_, paths) in activation["checks"]
        .as_object()
        .expect("validated activation checks")
    {
        protected.extend(errors::iterable_values(paths)?);
    }
    protected.extend(
        inputs
            .iter()
            .filter_map(|input| repository_reference(&input["ref"]))
            .map(Value::String),
    );

    let files = verification.source_files;
    let windows = files.windows();
    let within = |root: &PyString, path: &PyString| source_paths::contains(root, path, windows);
    let roots = errors::iterable_values(&policy["artifact_roots"]["repo"])?;
    let fields = ["path", "purpose", "producer_node_key", "artifact_kind"];
    let mut seen = BTreeSet::new();
    let mut prepared = Vec::with_capacity(outputs.len());
    for output in outputs {
        require_keys(output, &fields, &fields, "generated_output")?;
        let target = python_operations::add(&"repo:".into(), &output["path"])?;
        let path = files.normalize_target(&PyString(
            target.as_string().expect("string target").0[5..].to_vec(),
        ))?;
        let key = unicode::normcase(&path, windows);
        let producer = &output["producer_node_key"];
        errors::require_hashable(producer)?;
        let expected = producer.text().and_then(|key| producers.get(key.as_str()));
        let report = "review_report".into();
        let purpose = if producer.is_text("supervisor_delivery_consolidation") {
            &output["purpose"]
        } else {
            &report
        };
        let mismatch = || ContractError::new("generated_output", "PRODUCER_MISMATCH");
        let expected = expected.ok_or_else(mismatch)?;
        if seen.contains(&key) || path.0.last() == Some(&47) || !output["purpose"].py_eq(purpose) {
            return Err(mismatch().into());
        }
        errors::require_hashable(purpose)?;
        let acceptance = "acceptance_evidence".into();
        let kind = if purpose.is_text("acceptance_wrapper") {
            &acceptance
        } else {
            expected
        };
        if !["review_report", "consolidation", "acceptance_wrapper"]
            .iter()
            .any(|allowed| purpose.is_text(allowed))
            || !output["artifact_kind"].py_eq(kind)
        {
            return Err(mismatch().into());
        }
        let path_value = Value::String(path.clone());
        let path_on_disk = policy::path_from_value(verification.context.files, &path_value)?;
        let extension = path_on_disk
            .extension()
            .filter(|value| !value.is_empty())
            .map(|extension| {
                let mut suffix = PyString::from(".");
                suffix
                    .0
                    .extend(verification.context.files.spelling(Path::new(extension)).0);
                unicode::lower(&suffix)
            })
            .unwrap_or_default();
        let kind = kind.text().expect("validated artifact kind");
        let extensions =
            errors::iterable_values(&policy["artifact_kinds"][kind.as_str()]["extensions"])?;
        let extension = Value::String(extension);
        if !writes.iter().any(|root| within(root, &path))
            || !roots
                .iter()
                .any(|root| within(root.as_string().expect("validated artifact root"), &path))
            || protected.iter().any(|root| {
                let root = root.as_string().expect("validated protected root");
                within(root, &path) || within(&path, root)
            })
            || !extensions.iter().any(|suffix| suffix.py_eq(&extension))
            || files.metadata(&path)?.is_some()
        {
            return Err(EngineError::state("SOURCE_UNVERIFIABLE"));
        }
        let mut literal = PyString::from(":(literal)");
        literal.0.extend_from_slice(&path.0);
        let mut budget = MAX_ENUMERATION_BYTES;
        let tracked = bounded_git(
            verification.process,
            files,
            repository,
            &["ls-files".into(), "-z".into(), "--".into(), literal],
            files.elapsed() + SOURCE_TIMEOUT,
            &mut budget,
        )?;
        if !tracked.is_empty() {
            return Err(EngineError::state("SOURCE_UNVERIFIABLE"));
        }
        seen.insert(key);
        let mut output = output.clone();
        output.set("path", path_value);
        prepared.push((path, output));
    }
    prepared.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(Value::List(
        prepared.into_iter().map(|(_, output)| output).collect(),
    ))
}

pub(super) fn bind_review_source(
    session: &dyn PersistentSession,
    run: &Value,
    join: &Value,
    source: &Value,
    loaded: &RunContext,
    context: &SemanticContext<'_>,
    claims: &ClaimServices<'_>,
    specs: &[planner::NodeSpec],
    inputs: &[Value],
    plan: Option<&Value>,
) -> EngineResult<Value> {
    let (activation_row, activation) = evidence_activation(session, run)?;
    let repository = policy::path_from_value(context.files, &run["repository_path"])?;
    let files = claims
        .source_files
        .create(&repository, &loaded.policy, context.files);
    let verification = EvidenceVerification {
        session,
        run,
        context: *context,
        clock: claims.runtime,
        source_files: &*files,
        process: claims.process,
    };
    let outputs = match plan {
        Some(plan) => prepare_outputs(
            loaded,
            &verification,
            &repository,
            specs,
            inputs,
            &activation,
            plan,
        )?,
        None => Value::List(vec![]),
    };
    let coverage = Value::object([
        ("roots", activation["roots"].clone()),
        ("outputs", outputs),
        ("activation_ref", activation_row["ref"].clone()),
    ]);
    let captured = verification.capture_source(&coverage["roots"], &coverage["outputs"])?;
    let envelope = compat::loads_value(&source["envelope_json"])?;
    let digest = |value: &Value| -> EngineResult<Value> {
        Ok(ids::sha256_bytes(&ids::canonical_bytes(value)?).into())
    };
    let binding = Value::object([
        ("kind", "review_source_binding".into()),
        ("schema_version", 1_i64.into()),
        ("run_id", run["run_id"].clone()),
        ("dependency_join_id", join["join_id"].clone()),
        ("source_generation", join["generation"].clone()),
        (
            "source",
            Value::object([
                ("branch_id", source["branch_id"].clone()),
                ("attempt_id", envelope["attempt_id"].clone()),
                ("claim_digest", envelope["claim_digest"].clone()),
                ("result_digest", source["result_digest"].clone()),
            ]),
        ),
        ("handoff_ref", envelope["artifact_ref"].clone()),
        ("task_digest", run["task_digest"].clone()),
        ("policy_digest", run["policy_digest"].clone()),
        ("plan_digest", activation["plan_digest"].clone()),
        ("coverage_digest", digest(&coverage)?),
        ("outputs_digest", digest(&coverage["outputs"])?),
        ("source_digest", captured["sha256"].clone()),
        ("coverage", coverage),
    ]);
    let mut identifier = PyString::from("review-source-");
    identifier
        .0
        .extend_from_slice(&join["join_id"].as_string().expect("validated join ID").0);
    let artifact = persist_payload(
        session,
        run,
        &loaded.policy,
        &identifier,
        &"evidence_manifest".into(),
        &binding,
    )?;
    let coverage = &binding["coverage"];
    let current = verification.capture_source(&coverage["roots"], &coverage["outputs"])?;
    if !current["sha256"].py_eq(&captured["sha256"]) {
        return Err(EngineError::state("REVIEW_SOURCE_CHANGED"));
    }
    Ok(artifact.as_input())
}
