use super::{
    check_eligibility::CheckEligibilityPort,
    check_execution::CheckRuntimePort,
    check_fences::verify_check_fence,
    check_receipts::validate_receipt_integrity,
    claim_runtime::ClaimServices,
    error::{EngineError, EngineResult, PersistedTaskValidation, TaskValidationMutation},
    evidence_lifecycle::{current_review_binding, evidence_activation, reservation_states},
    evidence_verification::EvidenceVerification,
    graph_check::run_configured_check,
    graph_ledger::GraphRead,
    graph_payload::persist_payload,
    graph_record_control::claim_token_digest,
    graph_writes::GraphWrite,
    persistent_store::PersistentSession,
    run_context::RunContext,
    semantic_state::{mutation_task_validator, verify_semantic_state_with_trace, SemanticContext},
    source_capture::source_roots,
    state::mutate_persistent,
};
use crate::{
    compat::{self, Value},
    domain::{
        check_receipts::configured_check, contract_values::contains, contracts, errors, ids,
        python_operations, source_paths,
    },
};

struct Fences {
    executor: Value,
    source: Value,
    source_generation: Value,
    capabilities: Value,
}

struct Coverage {
    roots: Value,
    definition: Value,
    activation_ref: Value,
    plan_digest: Value,
    binding: Option<(Value, Value)>,
}

struct CheckRun<'a> {
    session: &'a dyn PersistentSession,
    run_id: &'a str,
    arguments: &'a Value,
    loaded: &'a RunContext,
    context: &'a SemanticContext<'a>,
    claims: &'a ClaimServices<'a>,
    runtime: &'a dyn CheckRuntimePort,
}

fn digest(value: &Value) -> EngineResult<String> {
    Ok(ids::sha256_bytes(&ids::canonical_bytes(value)?))
}

fn unbound(fence: &Value) -> Value {
    let mut result = fence.clone();
    result.set("result_digest", Value::Null);
    result
}

fn scope_contains(root: &Value, path: &Value, windows: bool) -> EngineResult<bool> {
    let root = root
        .as_string()
        .ok_or_else(|| errors::attribute_error(root, "rstrip"))?;
    let path = path
        .as_string()
        .ok_or_else(|| errors::attribute_error(path, "rstrip"))?;
    Ok(source_paths::contains(root, path, windows))
}

fn included_in(roots: &[Value], path: &Value, windows: bool) -> EngineResult<bool> {
    for root in roots {
        if scope_contains(root, path, windows)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn check_budget(
    session: &dyn PersistentSession,
    run_id: &Value,
    budget_id: &Value,
) -> EngineResult<()> {
    let budget = session
        .first(
            GraphRead::BudgetForUse,
            &[run_id.clone(), budget_id.clone()],
        )?
        .ok_or_else(|| {
            EngineError::python("TypeError", "'NoneType' object is not subscriptable")
        })?;
    if !python_operations::less(&budget["used"], &budget["limit_value"])? {
        return Err(EngineError::state("BUDGET_LIMIT"));
    }
    session.write(
        GraphWrite::IncrementBudget,
        &[1_i64.into(), run_id.clone(), budget_id.clone()],
    )
}

impl CheckRun<'_> {
    fn fences(&self, run: &Value) -> EngineResult<Fences> {
        let args = self.arguments;
        if !args["executor_claim_token"].truthy() {
            return Err(EngineError::state("ATTEMPT_FENCE_MISMATCH"));
        }
        let mut executor = Value::object([
            ("branch_id", args["executor_branch_id"].clone()),
            ("attempt_id", args["executor_attempt_id"].clone()),
            (
                "claim_digest",
                claim_token_digest(&args["executor_claim_token"])?.into(),
            ),
            ("result_digest", Value::Null),
        ]);
        let verified = verify_check_fence(self.session, self.claims.runtime, run, &executor, true)?;
        executor.set("result_digest", verified.branch["result_digest"].clone());
        let mut source = Value::object([
            ("branch_id", args["source_branch_id"].clone()),
            ("attempt_id", args["source_attempt_id"].clone()),
            ("claim_digest", args["source_claim_digest"].clone()),
            ("result_digest", Value::Null),
        ]);
        let source_branch =
            verify_check_fence(self.session, self.claims.runtime, run, &source, false)?.branch;
        source.set("result_digest", source_branch["result_digest"].clone());
        Ok(Fences {
            executor,
            source,
            source_generation: source_branch["generation"].clone(),
            capabilities: verified.envelope["effect_capabilities"].clone(),
        })
    }

    fn coverage(
        &self,
        run: &Value,
        check_id: &Value,
        capabilities: &Value,
    ) -> EngineResult<Coverage> {
        let (row, activation) = evidence_activation(self.session, run)?;
        let files = self.claims.source_files.create(
            self.context.repository,
            &self.loaded.policy,
            self.context.files,
        );
        let executor_roots = errors::iterable_values(&source_roots(&*files, capabilities)?)?;
        let activation_roots = errors::iterable_values(errors::string_item(&activation, "roots")?)?;
        let windows = self.context.files.windows();
        let mut roots = Vec::new();
        for (inner, outer) in [
            (&activation_roots, &executor_roots),
            (&executor_roots, &activation_roots),
        ] {
            for root in inner {
                if included_in(outer, root, windows)?
                    && !roots
                        .iter()
                        .any(|existing: &Value| existing.container_eq(root))
                {
                    roots.push(root.clone());
                }
            }
        }
        python_operations::sort_comparable_values(&mut roots)?;
        let id = check_id
            .text()
            .expect("validated configured check identifier");
        let paths = errors::string_item(errors::string_item(&activation, "checks")?, &id)?;
        for path in errors::iterable_values(paths)? {
            if !included_in(&roots, &path, windows)? {
                return Err(EngineError::state("SOURCE_UNVERIFIABLE"));
            }
        }
        let roots = Value::List(roots);
        let activation_ref = row["ref"].clone();
        let binding = current_review_binding(self.session, run)?;
        let definition = if let Some((_, value)) = &binding {
            let coverage = errors::string_item(value, "coverage")?;
            if !roots.py_eq(errors::string_item(coverage, "roots")?) {
                return Err(EngineError::state("CHECK_REVIEW_BINDING_MISMATCH"));
            }
            coverage.clone()
        } else {
            Value::object([
                ("roots", roots.clone()),
                ("outputs", Value::List(vec![])),
                ("activation_ref", activation_ref.clone()),
            ])
        };
        Ok(Coverage {
            roots,
            definition,
            activation_ref,
            plan_digest: activation["plan_digest"].clone(),
            binding,
        })
    }

    fn verify_source<T>(
        &self,
        run: &Value,
        action: impl FnOnce(&EvidenceVerification<'_>) -> EngineResult<T>,
    ) -> EngineResult<T> {
        let source_files = self.claims.source_files.create(
            self.context.repository,
            &self.loaded.policy,
            self.context.files,
        );
        action(&EvidenceVerification {
            session: self.session,
            run,
            context: *self.context,
            clock: self.claims.runtime,
            source_files: &*source_files,
            process: self.claims.process,
        })
    }

    fn capture_source(&self, run: &Value, coverage: &Coverage) -> EngineResult<Value> {
        self.verify_source(run, |verifier| {
            let capture =
                verifier.capture_source(&coverage.roots, &coverage.definition["outputs"])?;
            Ok(capture["sha256"].clone())
        })
    }

    fn check_predecessor(&self, run: &Value, check_id: &Value) -> EngineResult<()> {
        let selected = self.session.first(
            GraphRead::SelectedCheckArtifact,
            &[run["run_id"].clone(), check_id.clone()],
        )?;
        let predecessor = selected
            .as_ref()
            .map_or(&compat::value::NULL, |row| &row["artifact_ref"]);
        if !predecessor.py_eq(&self.arguments["replace_ref"]) {
            return Err(EngineError::state("CHECK_PREDECESSOR_CONFLICT"));
        }
        Ok(())
    }

    fn reserve(
        &self,
        run: &Value,
        request: &Value,
        operation: &str,
        fences: &Fences,
        coverage: &Coverage,
    ) -> EngineResult<Value> {
        self.fences(run)?;
        self.check_predecessor(run, &request["check_id"])?;
        let history = reservation_states(self.session, &run["run_id"])?.values();
        let mut repeat = false;
        for reservation in history {
            if !reservation["check_id"].py_eq(&request["check_id"]) {
                continue;
            }
            if reservation["state"].is_text("open") {
                return Err(EngineError::state("CHECK_EXECUTION_UNKNOWN"));
            }
            let executor = &reservation["executor"];
            repeat |= executor["branch_id"].py_eq(&fences.executor["branch_id"])
                && executor["attempt_id"].py_eq(&fences.executor["attempt_id"]);
        }
        self.verify_source(run, |verifier| {
            verifier.verify_generated_outputs(&coverage.definition["outputs"], None)
        })?;
        let before = self.capture_source(run, coverage)?;
        if let Some((_, binding)) = &coverage.binding {
            if !before.py_eq(&binding["source_digest"]) {
                return Err(EngineError::state("CHECK_REVIEW_BINDING_MISMATCH"));
            }
        }
        let budget_id = Value::from(if run["selected_route"].is_text("design_only") {
            "design_revisions"
        } else {
            "delivery_repairs"
        });
        if repeat {
            check_budget(self.session, &run["run_id"], &budget_id)?;
        }
        let identity_digest = digest(&Value::List(vec![run["run_id"].clone(), operation.into()]))?;
        let reservation_id = format!("reservation-{}", &identity_digest[..40]);
        let mut reservation = request.clone();
        for (key, value) in [
            ("kind", "check_reservation".into()),
            ("schema_version", 1_i64.into()),
            ("executor", fences.executor.clone()),
            ("source", fences.source.clone()),
            ("reservation_id", reservation_id.as_str().into()),
            ("before_source_digest", before),
            ("repeat_cost", i64::from(repeat).into()),
            ("budget_id", budget_id),
            ("request_digest", digest(request)?.into()),
            (
                "review_source_binding_ref",
                coverage
                    .binding
                    .as_ref()
                    .map_or(Value::Null, |(row, _)| row["ref"].clone()),
            ),
        ] {
            reservation.set(key, value);
        }
        let artifact = persist_payload(
            self.session,
            run,
            &self.loaded.policy,
            &reservation_id.as_str().into(),
            &"evidence_manifest".into(),
            &reservation,
        )?;
        Ok(Value::object([
            ("code", "CHECK_RESERVED".into()),
            ("reservation_id", reservation_id.into()),
            ("artifact_ref", artifact.reference),
        ]))
    }

    fn finish(
        &self,
        run: &Value,
        check_id: &Value,
        configured: &Value,
        receipt: &Value,
        artifact_id: &str,
    ) -> EngineResult<Value> {
        self.fences(run)?;
        let reservations = reservation_states(self.session, &run["run_id"])?;
        if !reservations
            .get(&receipt["reservation_id"])?
            .is_some_and(|state| state["state"].is_text("open"))
        {
            return Err(EngineError::state("CHECK_RESERVATION_CLOSED"));
        }
        self.check_predecessor(run, check_id)?;
        validate_receipt_integrity(receipt, &run["run_id"], check_id, configured, self.runtime)?;
        let artifact = persist_payload(
            self.session,
            run,
            &self.loaded.policy,
            &artifact_id.into(),
            &"check_evidence".into(),
            receipt,
        )?;
        self.session.write(
            GraphWrite::SelectCheckEvidence,
            &[
                run["run_id"].clone(),
                check_id.clone(),
                receipt["outcome"].clone(),
                artifact.reference.clone(),
                artifact.sha256,
            ],
        )?;
        Ok(Value::object([
            ("code", "CHECK_RECORDED".into()),
            ("artifact_ref", artifact.reference),
            ("outcome", receipt["outcome"].clone()),
            ("validity", receipt["validity"].clone()),
        ]))
    }

    fn execute(&self) -> EngineResult<Value> {
        let run = &self.loaded.run.fields;
        let check_id = &self.arguments["check_id"];
        let fences = self.fences(run)?;
        verify_semantic_state_with_trace(
            self.session,
            run,
            self.context,
            Some(PersistedTaskValidation::Format7CheckDirect),
        )?;
        let configured = configured_check(&self.loaded.policy, check_id)?;
        let required = errors::string_item(&self.loaded.task, "required_check_ids")?;
        let authority = Value::object([
            ("effect", "command".into()),
            ("action", "run".into()),
            ("target_ref", configured["command_id"].clone()),
        ]);
        if !contains(required, check_id)? || !contains(&fences.capabilities, &authority)? {
            return Err(EngineError::state("CHECK_AUTHORITY_REQUIRED"));
        }
        let coverage = self.coverage(run, check_id, &fences.capabilities)?;
        let request = Value::object([
            ("command", "check.start".into()),
            ("check_id", check_id.clone()),
            ("executor", unbound(&fences.executor)),
            ("source", unbound(&fences.source)),
            ("source_generation", fences.source_generation.clone()),
            ("predecessor_ref", self.arguments["replace_ref"].clone()),
            ("coverage", coverage.definition.clone()),
            ("activation_ref", coverage.activation_ref.clone()),
        ]);
        let operation = contracts::opaque(&self.arguments["op_id"], "op_id")?;
        let operation_digest = ids::sha256_bytes(operation.as_bytes());
        let short_digest = &operation_digest[..40];
        let start_id = format!("check-start-{short_digest}");
        let finish_id = format!("check-finish-{short_digest}");
        if let Some(prior) = self.session.operation(self.run_id, &finish_id)? {
            let start = self
                .session
                .operation(self.run_id, &start_id)?
                .ok_or_else(|| {
                    EngineError::python("TypeError", "'NoneType' object is not subscriptable")
                })?;
            if !start.request_digest.is_text(&digest(&request)?) {
                return Err(EngineError::state("OPERATION_CONFLICT"));
            }
            let mut response = compat::loads_value(&prior.response_json)?;
            response.set("code", "REPLAYED".into());
            return Ok(response);
        }
        let reserved = mutate_persistent(
            self.session,
            self.run_id,
            &start_id,
            &request,
            |_, current, _| self.reserve(&current.fields, &request, &operation, &fences, &coverage),
            mutation_task_validator(self.context, TaskValidationMutation::Format7CheckStart),
            self.context.store.fault,
        )?;
        if reserved["code"].is_text("REPLAYED") {
            return Err(EngineError::state("CHECK_EXECUTION_UNKNOWN"));
        }
        let states = reservation_states(self.session, &run["run_id"])?;
        let reservation = states
            .get(&reserved["reservation_id"])?
            .ok_or_else(|| EngineError::state("CHECK_RESERVATION_INVALID"))?;
        let mut receipt = run_configured_check(
            self.context.repository,
            &run["run_id"],
            check_id,
            configured,
            self.claims.process,
            self.runtime,
            false,
        )?;
        let before = &reservation["before_source_digest"];
        let (after, validity) = match self.capture_source(run, &coverage) {
            Ok(after) if after.py_eq(before) => (after, "valid"),
            Ok(after) => (after, "source_changed"),
            Err(EngineError::State(_)) => ("0".repeat(64).into(), "unverifiable"),
            Err(error) => return Err(error),
        };
        for (key, value) in [
            ("schema_version", 2_i64.into()),
            ("executor", fences.executor.clone()),
            ("source", fences.source),
            ("source_generation", fences.source_generation),
            ("repo_worktree_sha256", after.clone()),
            ("task_digest", run["task_digest"].clone()),
            ("policy_digest", run["policy_digest"].clone()),
            ("plan_digest", coverage.plan_digest),
            ("reservation_id", reservation["reservation_id"].clone()),
            ("predecessor_ref", self.arguments["replace_ref"].clone()),
            ("coverage_digest", digest(&coverage.definition)?.into()),
            ("coverage", coverage.definition),
            ("before_source_digest", before.clone()),
            ("after_source_digest", after),
            ("validity", validity.into()),
            ("activation_ref", coverage.activation_ref),
            (
                "review_source_binding_ref",
                reservation["review_source_binding_ref"].clone(),
            ),
        ] {
            receipt.set(key, value);
        }
        let finish_request = Value::object([
            ("command", "check.finish".into()),
            ("reservation_id", reservation["reservation_id"].clone()),
            ("executor", fences.executor),
            ("receipt_digest", digest(&receipt)?.into()),
        ]);
        let artifact_id = format!("receipt-{short_digest}");
        mutate_persistent(
            self.session,
            self.run_id,
            &finish_id,
            &finish_request,
            |_, current, _| {
                self.finish(
                    &current.fields,
                    check_id,
                    configured,
                    &receipt,
                    &artifact_id,
                )
            },
            mutation_task_validator(self.context, TaskValidationMutation::Format7CheckFinish),
            self.context.store.fault,
        )
    }
}

pub fn run(
    session: &dyn PersistentSession,
    run_id: &str,
    arguments: &Value,
    loaded: &RunContext,
    context: &SemanticContext<'_>,
    claims: &ClaimServices<'_>,
    runtime: &dyn CheckRuntimePort,
) -> EngineResult<Value> {
    CheckRun {
        session,
        run_id,
        arguments,
        loaded,
        context,
        claims,
        runtime,
    }
    .execute()
}

pub fn abandon(
    session: &dyn PersistentSession,
    run_id: &str,
    arguments: &Value,
    context: &SemanticContext<'_>,
) -> EngineResult<Value> {
    let reservation_id = Value::from(contracts::opaque(
        &arguments["reservation_id"],
        "reservation_id",
    )?);
    let request = Value::object([
        ("command", "check.abandon".into()),
        ("reservation_id", reservation_id.clone()),
        (
            "reason",
            Value::String(contracts::bounded_string(&arguments["reason"], "reason", 1024)?.clone()),
        ),
        ("actor", Value::String(context.provenance.actor()?)),
    ]);
    let operation = contracts::opaque(&arguments["op_id"], "op_id")?;
    mutate_persistent(
        session,
        run_id,
        &operation,
        &request,
        |session, current, _| {
            let run = &current.fields;
            if !run["state_schema_version"].py_eq(&7_i64.into()) {
                return Err(EngineError::state("EVIDENCE_ENABLE_REQUIRED"));
            }
            let states = reservation_states(session, &run["run_id"])?;
            if !states
                .get(&reservation_id)?
                .is_some_and(|state| state["state"].is_text("open"))
            {
                return Err(EngineError::state("CHECK_RESERVATION_CLOSED"));
            }
            Ok(Value::object([
                ("code", "CHECK_ABANDONED".into()),
                ("reservation_id", reservation_id.clone()),
            ]))
        },
        mutation_task_validator(context, TaskValidationMutation::CheckAbandon),
        context.store.fault,
    )
}
