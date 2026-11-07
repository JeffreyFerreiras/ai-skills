use super::{
    artifacts::ArtifactResolver,
    check_execution::CheckRuntimePort,
    claim_runtime::ClaimServices,
    error::{EngineError, EngineResult, PlanTraceCommand},
    graph_arguments,
    graph_claim::claim_next,
    graph_initialization::{
        execution_preflight, initialize_run, prepare_execution_plan_traced, preview_constraints,
        InitRequest, PlanRequest,
    },
    graph_next::next_ready,
    graph_plan_approval::record_plan_approval,
    graph_response::json_result,
    graph_run_control::{abort_run, block_run},
    persistent_store::{PersistentSession, StateDatabase, StatePlatform, StateStore},
    policy::{self, PolicyFilePort},
    run_context,
    semantic_state::{verify_read_only, SemanticContext},
    snapshots::IntegerFileSnapshotPort,
    state::FaultHook,
    usage_source::{self, UsageSourcePort},
};
use crate::{
    compat::{
        argparse::{ArgumentValue, Namespace, ParserOutcome},
        PyString, Value,
    },
    domain::{contracts, contracts::ContractError},
};
use std::path::Path;

pub struct CliServices<'a> {
    pub files: &'a dyn PolicyFilePort,
    pub snapshots: &'a dyn IntegerFileSnapshotPort,
    pub provenance: &'a dyn CheckRuntimePort,
    pub skill: &'a Path,
    pub installation: &'a Path,
    pub platform: &'a dyn StatePlatform,
    pub database: &'a dyn StateDatabase,
    pub fault: &'a dyn FaultHook,
    pub claims: ClaimServices<'a>,
    pub usage_files: &'a dyn UsageSourcePort,
}

#[derive(Debug)]
pub enum ExecuteOutcome {
    Response {
        result: Value,
        exit_code: i32,
    },
    ParserExit {
        exit_code: i32,
        stdout: PyString,
        stderr: PyString,
    },
}

pub struct ParsedRequest(Value);

impl ParsedRequest {
    pub fn repository(&self) -> Option<&PyString> {
        if self.0["command"].is_text("usage") {
            return None;
        }
        self.0["repo"].as_string()
    }
}

fn namespace_value(namespace: Namespace) -> Value {
    let mut result = Value::object([]);
    for (name, value) in namespace.fields {
        result.set(
            &name,
            match value {
                ArgumentValue::None => Value::Null,
                ArgumentValue::Boolean(value) => value.into(),
                ArgumentValue::Integer(value) => Value::Integer(value),
                ArgumentValue::String(value) => Value::String(value),
            },
        );
    }
    result
}

fn response(result: Value) -> ExecuteOutcome {
    let exit_code = if result["code"].is_text("NOT_READY")
        || result["code"].is_text("FANOUT_ASSESSMENT_REQUIRED")
    {
        2
    } else if result["code"].is_text("GRAPH_BLOCKED")
        || result["code"].is_text("EXECUTION_PLAN_REJECTED")
    {
        3
    } else {
        0
    };
    ExecuteOutcome::Response { result, exit_code }
}

fn plan_request<'a>(arguments: &'a Value, host: &'a str, size: Option<&'a str>) -> PlanRequest<'a> {
    PlanRequest {
        run_id: &arguments["run_id"],
        task_brief: &arguments["task_brief"],
        size,
        host,
    }
}

fn is_mutation(arguments: &Value) -> bool {
    let command = &arguments["command"];
    [
        "record", "complete", "block", "abort", "check", "evidence", "recover",
    ]
    .iter()
    .any(|value| command.is_text(value))
        || (["next", "ready"].iter().any(|value| command.is_text(value))
            && arguments["claim"].truthy())
        || (command.is_text("join") && arguments["join_kind"].is_text("advance"))
}

fn dispatch_run(
    session: &dyn PersistentSession,
    run_id: &str,
    loaded: &run_context::RunContext,
    arguments: &Value,
    context: &SemanticContext<'_>,
    services: &CliServices<'_>,
) -> EngineResult<Value> {
    let run = &loaded.run.fields;
    let stored_policy = &loaded.policy;
    let claims = &services.claims;
    if arguments["command"].is_text("status") || arguments["command"].is_text("resume") {
        let mut result = super::graph_status::status(session, run, context, claims)?;
        if arguments["command"].is_text("resume") {
            result.set("code", "RESUMED".into());
            result.set(
                "acknowledgments",
                Value::object([
                    ("host_identity", run["host_identity"].clone()),
                    (
                        "degraded_permissions",
                        arguments["ack_degraded_permissions"].truthy().into(),
                    ),
                    (
                        "degraded_durability",
                        arguments["ack_degraded_durability"].truthy().into(),
                    ),
                    (
                        "recorded_degraded_permissions",
                        run["degraded_permissions_ack"].truthy().into(),
                    ),
                    (
                        "recorded_degraded_durability",
                        run["degraded_durability_ack"].truthy().into(),
                    ),
                ]),
            );
            let running = Value::list(
                result["branches"]
                    .as_list()
                    .expect("status lists branches")
                    .iter()
                    .filter(|branch| branch["status"].is_text("running"))
                    .map(|branch| branch["branch_id"].clone()),
            );
            result.set("unresolved_running", running);
            let continuation =
                super::graph_status::continuation(session, run, &loaded.task, &result)?;
            let action = super::graph_status::resume_next_action(
                session,
                run,
                &result,
                claims.runtime,
                context.files.windows(),
            )?;
            result.set("continuation", continuation);
            result.set("next_action", action);
        }
        Ok(result)
    } else if arguments["command"].is_text("consolidation") {
        super::graph_consolidation::draft(session, run, &arguments["join_id"])
    } else if arguments["command"].is_text("recovery") {
        let packets = super::graph_recovery::recovery_packets(
            session,
            run,
            &arguments["branch_id"],
            context.files.windows(),
        )?;
        let mut result = json_result(
            true,
            "RECOVERY",
            run["run_id"].clone(),
            run["state_revision"].clone(),
        );
        result.set("packets", Value::List(packets));
        result.set(
            "blocked_recovery",
            super::graph_status::blocked_recovery_guidance(session, run)?,
        );
        Ok(result)
    } else if arguments["command"].is_text("recover") {
        super::graph_recovery::recover(session, run_id, arguments, loaded, context, claims)
    } else if arguments["command"].is_text("join") && arguments["join_kind"].is_text("validate") {
        super::join_validation::validate_run_join(session, run, &arguments["join_id"])
    } else if arguments["command"].is_text("join") && arguments["join_kind"].is_text("advance") {
        super::graph_join_advance::advance(session, run_id, arguments, loaded, context, claims)
    } else if arguments["command"].is_text("check") {
        if arguments["check_kind"].is_text("abandon") {
            return super::graph_check_v2::abandon(session, run_id, arguments, context);
        }
        if run["state_schema_version"].py_eq(&7_i64.into()) {
            return super::graph_check_v2::run(
                session,
                run_id,
                arguments,
                loaded,
                context,
                claims,
                services.provenance,
            );
        }
        super::graph_check::run_legacy(
            session,
            run_id,
            arguments,
            loaded,
            context,
            claims.process,
            services.provenance,
        )
    } else if arguments["command"].is_text("record")
        && arguments["record_kind"].is_text("check-evidence")
    {
        super::graph_check::record_legacy_evidence(session, run_id, arguments, loaded, context)
    } else if arguments["command"].is_text("record")
        && arguments["record_kind"].is_text("acceptance-evidence")
    {
        super::graph_closure::record_acceptance(session, run_id, arguments, loaded, context, claims)
    } else if arguments["command"].is_text("complete") {
        super::graph_closure::complete(session, run_id, arguments, loaded, context, claims)
    } else if arguments["command"].is_text("evidence") {
        super::graph_evidence_enable::enable(
            session,
            run_id,
            arguments,
            stored_policy,
            &loaded.task,
            context,
            claims,
        )
    } else if arguments["command"].is_text("record") && arguments["record_kind"].is_text("approval")
    {
        super::graph_approval::record(
            session,
            run_id,
            arguments,
            stored_policy,
            &loaded.task,
            context,
        )
    } else if arguments["command"].is_text("record") && arguments["record_kind"].is_text("usage") {
        super::graph_usage::record(
            session,
            run_id,
            &loaded.run,
            arguments,
            context,
            services.usage_files,
        )
    } else if arguments["command"].is_text("record")
        && arguments["record_kind"].is_text("fanout-assessment")
    {
        super::graph_fanout::record(session, run_id, arguments, stored_policy, context)
    } else if arguments["command"].is_text("record")
        && arguments["record_kind"].is_text("review-fanout")
    {
        super::graph_review_delegation::record_request(
            session,
            run_id,
            arguments,
            stored_policy,
            context,
            claims,
        )
    } else if arguments["command"].is_text("record")
        && arguments["record_kind"].is_text("review-fanout-assessment")
    {
        super::graph_review_delegation::record_assessment(
            session,
            run_id,
            arguments,
            stored_policy,
            context,
        )
    } else if arguments["command"].is_text("record")
        && arguments["record_kind"].is_text("branch-result")
    {
        super::graph_branch_result::record(session, run_id, arguments, loaded, context, claims)
    } else if arguments["command"].is_text("record")
        && arguments["record_kind"].is_text("plan-approval")
    {
        record_plan_approval(session, run_id, arguments, context)
    } else if arguments["command"].is_text("record")
        && ["heartbeat", "budget-use", "timeout", "skip", "retry"]
            .iter()
            .any(|kind| arguments["record_kind"].is_text(kind))
    {
        super::graph_record_control::record_control(
            session, run_id, arguments, loaded, context, claims,
        )
    } else if arguments["command"].is_text("abort") {
        abort_run(session, run_id, arguments, context)
    } else if arguments["command"].is_text("block") {
        block_run(session, run_id, arguments, context)
    } else if arguments["command"].is_text("next") || arguments["command"].is_text("ready") {
        let ready = arguments["command"].is_text("ready");
        let claim = !ready && arguments["claim"].truthy();
        if claim && !arguments["op_id"].truthy() {
            return Err(ContractError::new("op_id", "MISSING_FIELD").into());
        }
        if claim {
            return claim_next(session, run_id, arguments, context, claims);
        }
        next_ready(session, run, ready || arguments["all"].truthy(), context)
    } else {
        Err(EngineError::python(
            "NotImplementedError",
            "native graph CLI handler remains unimplemented",
        ))
    }
}

pub fn execute(
    argv: &[PyString],
    width: usize,
    supplied_store: Option<&StateStore<'_>>,
    services: &CliServices<'_>,
) -> EngineResult<ExecuteOutcome> {
    match parse(argv, width) {
        Ok(request) => execute_parsed(request, supplied_store, services)
            .map_err(EngineError::without_graphctl_trace),
        Err(exit) => Ok(exit),
    }
}

pub fn parse(argv: &[PyString], width: usize) -> Result<ParsedRequest, ExecuteOutcome> {
    match graph_arguments::shared_parser().parse(argv, width) {
        ParserOutcome::Arguments(namespace) => Ok(ParsedRequest(namespace_value(namespace))),
        ParserOutcome::Exit {
            code,
            stdout,
            stderr,
        } => Err(ExecuteOutcome::ParserExit {
            exit_code: code,
            stdout,
            stderr,
        }),
    }
}

pub fn execute_parsed(
    ParsedRequest(arguments): ParsedRequest,
    supplied_store: Option<&StateStore<'_>>,
    services: &CliServices<'_>,
) -> EngineResult<ExecuteOutcome> {
    if arguments["command"].is_text("usage") {
        let path = policy::path_from_value(services.files, &arguments["session_log"])?;
        let result = usage_source::checkpoint(services.usage_files, &path)?;
        return Ok(ExecuteOutcome::Response {
            result,
            exit_code: 0,
        });
    }
    if arguments["repo"].is_null() {
        return Err(ContractError::new("repo", "MISSING_FIELD").into());
    }
    let repository = services.files.resolve(
        &policy::path_from_value(services.files, &arguments["repo"])?,
        true,
    )?;
    let requested_root = if arguments["state_root"].is_null() {
        None
    } else {
        Some(policy::path_from_value(
            services.files,
            &arguments["state_root"],
        )?)
    };
    if let Some(root) = &requested_root {
        if !root.is_absolute() {
            return Err(EngineError::state("STATE_ROOT_MUST_BE_ABSOLUTE"));
        }
        if let Some(store) = supplied_store {
            if !services
                .files
                .same_path(&store.state_root, &services.files.absolute(root)?)
            {
                return Err(EngineError::state("STATE_ROOT_CONFLICT"));
            }
        }
    }
    let runtime_root = match supplied_store {
        Some(store) => store.state_root.clone(),
        None => policy::resolve_state_root(
            services.files,
            requested_root.as_deref(),
            services.installation,
        )?,
    };
    let (policy, policy_snapshot) = policy::load_policy_traced(
        services.files,
        &repository,
        services.installation,
        Some(&runtime_root),
    )?;
    if arguments["command"].is_text("policy") {
        let mut result = json_result(true, "POLICY_LOADED", Value::Null, Value::Null);
        result.set(
            "policy_path",
            Value::String(services.files.spelling(&policy_snapshot.path)),
        );
        result.set("policy_digest", policy_snapshot.digest.into());
        result.set(
            "policy_source",
            if services.files.under(&policy_snapshot.path, &runtime_root) {
                "runtime_home"
            } else {
                "legacy_installed_skill"
            }
            .into(),
        );
        result.set(
            "state_root",
            Value::String(services.files.spelling(&runtime_root)),
        );
        result.set(
            "lessons_path",
            Value::String(
                services
                    .files
                    .spelling(&runtime_root.join("lessons-learned.md")),
            ),
        );
        result.set(
            "runtime_artifact_root",
            Value::String(
                services.files.spelling(
                    &runtime_root.join("artifacts").join(
                        policy["repository_id"]
                            .text()
                            .expect("validated repository ID"),
                    ),
                ),
            ),
        );
        result.set("repository_id", policy["repository_id"].clone());
        return Ok(response(result));
    }
    if arguments["command"].is_text("constraints") {
        let resolver = ArtifactResolver {
            files: services.files,
            snapshots: services.snapshots,
            repository: &repository,
            skill: services.skill,
            installation: services.installation,
            policy: &policy,
            runtime: Some(&runtime_root),
            ledger: None,
        };
        let selection = preview_constraints(
            &resolver,
            &runtime_root,
            &arguments["bundle"],
            &arguments["context"],
        )?;
        let mut result = json_result(true, "CONSTRAINTS_SELECTED", Value::Null, Value::Null);
        for key in ["active", "candidates", "excluded"] {
            result.set(key, selection[key].clone());
        }
        return Ok(response(result));
    }
    // Plan preparation only needs the resolved root; it must not construct or open state.
    let state_view = StateStore {
        state_root: runtime_root.clone(),
        platform: services.platform,
        database: services.database,
        fault: services.fault,
    };
    let mut context = SemanticContext {
        store: supplied_store.unwrap_or(&state_view),
        files: services.files,
        snapshots: services.snapshots,
        provenance: services.provenance,
        repository: &repository,
        skill: services.skill,
        installation: services.installation,
        runtime: Some(&runtime_root),
        policy: &policy,
        current_policy_digest: &policy_snapshot.digest,
        case_sensitive: !services.files.windows(),
        evidence_enable: false,
    };
    if arguments["command"].is_text("plan") {
        let host = arguments["host"]
            .text()
            .expect("the parser restricts host to ASCII choices");
        let size = arguments["size"].text();
        let plan = prepare_execution_plan_traced(
            &context,
            &plan_request(&arguments, &host, size.as_deref()),
            PlanTraceCommand::Preview,
        )?;
        let mut result = json_result(
            true,
            "EXECUTION_PLAN_PREVIEW",
            arguments["run_id"].clone(),
            Value::Null,
        );
        result.set("preflight", execution_preflight(&context, &plan)?);
        result.set("execution_plan", plan.execution_plan);
        return Ok(response(result));
    }
    let constructed_store;
    if supplied_store.is_none() {
        constructed_store = StateStore::at_root(
            runtime_root.clone(),
            services.platform,
            services.database,
            services.fault,
        )?;
        context.store = &constructed_store;
    }
    if arguments["command"].is_text("init") {
        let host = arguments["host"]
            .text()
            .expect("the parser restricts host to ASCII choices");
        let size = arguments["size"].text();
        return initialize_run(
            &context,
            &InitRequest {
                plan: plan_request(&arguments, &host, size.as_deref()),
                operation_id: &arguments["op_id"],
                acknowledge_permissions: arguments["ack_degraded_permissions"].truthy(),
                acknowledge_durability: arguments["ack_degraded_durability"].truthy(),
            },
        )
        .map(response);
    }
    let run_id = contracts::opaque(&arguments["run_id"], "run_id")?;
    let repository_id = policy["repository_id"]
        .text()
        .expect("validated repository ID");
    context.store.with_run(&repository_id, &run_id, |session| {
        let loaded = run_context::load(session, &arguments["run_id"])?;
        let skill = services.files.resolve(services.skill, true)?;
        let mut semantic = context;
        semantic.skill = &skill;
        semantic.evidence_enable = arguments["command"].is_text("evidence")
            && loaded.run.fields["state_schema_version"].py_eq(&6_i64.into());
        if !is_mutation(&arguments) {
            verify_read_only(session, &loaded.run.fields, &semantic)?;
        }
        if arguments["command"].is_text("resume") {
            let run = &loaded.run.fields;
            if run["permission_verification"].is_text("DEGRADED_PERMISSION_VERIFICATION")
                && !arguments["ack_degraded_permissions"].truthy()
            {
                return Err(EngineError::state("DEGRADED_PERMISSION_ACK_REQUIRED"));
            }
            if run["durability"].is_text("degraded")
                && !arguments["ack_degraded_durability"].truthy()
            {
                return Err(EngineError::state("DEGRADED_DURABILITY_ACK_REQUIRED"));
            }
        }
        dispatch_run(session, &run_id, &loaded, &arguments, &semantic, services).map(response)
    })
}
