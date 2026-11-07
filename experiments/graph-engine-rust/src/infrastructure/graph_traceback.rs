use super::traceback_frames::{append, frame, underline};
use crate::{
    application::error::{
        BranchResultTraceSite, ConstraintCommand, GraphctlTraceSite, ManifestJsonTraceSite,
        MutationValidationPhase, PersistedTaskValidation, PlanSnapshot, PlanTraceCommand,
        PolicySnapshotSource, SnapshotJsonTraceSite, TaskValidationMutation,
    },
    compat::PyString,
    domain::constraints::ConstraintValidationSite,
};
use std::path::Path;

const CONTINUATION_FUNCTION: &str = "_validate_review_continuation_result";
const FINDINGS_SOURCE: &str = r#"final_finding_ids = {item.get("finding_id") for item in value.get("findings", []) if isinstance(item, dict)}"#;
const EVIDENCE_ENABLE_CALL: &str =
    "command_evidence_enable(args, connection, run, stored_policy, task, state, semantic_validator)";
const RECOVER_CALL: &str =
    "command_recover(args, connection, run, stored_policy, state, semantic_validator)";

/// Render the frozen CPython 3.12.10 frames for a traced public CLI failure.
pub fn graphctl_traceback(
    installation: &Path,
    site: GraphctlTraceSite,
    kind: &str,
    message: &PyString,
) -> PyString {
    let script = installation.join("scripts").join("graphctl.py");
    let engine = installation.join("graph_engine");
    let cli = engine.join("cli.py");
    let mut output = PyString::from("Traceback (most recent call last):\n");
    frame(
        &mut output,
        &script,
        15,
        "<module>",
        "raise SystemExit(main())",
        Some("main()"),
    );
    frame(
        &mut output,
        &cli,
        3326,
        "main",
        "result, exit_code = execute(argv)",
        Some("execute(argv)"),
    );
    match site {
        GraphctlTraceSite::BranchResult(site) => {
            branch_result_frames(&mut output, &engine, &cli, site)
        }
        GraphctlTraceSite::ConstraintSupportPayload(command) => {
            constraint_support_call(&mut output, &cli, command);
            frame(
                &mut output,
                &cli,
                1611,
                "_constraint_support",
                r#"if payload.get("finding_id") != record["finding_id"] or (key == "accepted_fix_ref" and payload.get("status") != "accepted"):"#,
                Some("payload.get"),
            );
        }
        GraphctlTraceSite::ConstraintValidation(command, site) => {
            constraint_validation_frames(&mut output, &engine, &cli, command, site)
        }
        GraphctlTraceSite::SnapshotJson(site) => {
            snapshot_json_frames(&mut output, &engine, &cli, site)
        }
    }
    append(&mut output, format!("{kind}: "));
    output.0.extend_from_slice(&message.0);
    append(&mut output, "\n");
    output
}

fn record_dispatch_frame(output: &mut PyString, cli: &Path, line: u16, source: &str) {
    frame(
        output,
        cli,
        3288,
        "execute",
        "result = command_record(",
        Some("command_record("),
    );
    frame(
        output,
        cli,
        line,
        "command_record",
        source,
        source.strip_prefix("return "),
    );
}

fn manifest_json_frames(output: &mut PyString, cli: &Path, site: ManifestJsonTraceSite) {
    use ManifestJsonTraceSite::*;

    let (line, function, source) = match site {
        BranchResult => {
            record_dispatch_frame(output, cli, 1575,
                "return _record_branch_result(args, connection, run, policy, task, store, semantic_validator)");
            (
                756,
                "_record_branch_result",
                r#"snapshot = _manifest_snapshot(store, policy, run["run_id"], args.result_manifest)"#,
            )
        }
        TerminalEvidence | Retry => {
            record_dispatch_frame(output, cli, 1588,
                "return _record_control(args, connection, run, policy, task, store, semantic_validator)");
            if matches!(site, Retry) {
                (
                    976,
                    "_record_control",
                    r#"snapshot = _manifest_snapshot(store, policy, run["run_id"], args.repair_manifest)"#,
                )
            } else {
                (
                    964,
                    "_record_control",
                    r#"snapshot = _manifest_snapshot(store, policy, run["run_id"], args.evidence_manifest, 128 * 1024)"#,
                )
            }
        }
        FanoutAssessment => {
            record_dispatch_frame(output, cli, 1577, "return command_fanout_assessment(");
            (
                1271,
                "command_fanout_assessment",
                r#"snapshot = _manifest_snapshot(store, policy, run["run_id"], args.assessment_manifest, 256 * 1024)"#,
            )
        }
        ReviewPreliminary | ReviewRequest => {
            record_dispatch_frame(output, cli, 1582,
                "return command_review_fanout(args, connection, run, policy, task, store, semantic_validator)");
            if matches!(site, ReviewPreliminary) {
                (
                    1354,
                    "command_review_fanout",
                    r#"preliminary_snapshot = _manifest_snapshot(store, policy, run["run_id"], args.preliminary_manifest)"#,
                )
            } else {
                (
                    1355,
                    "command_review_fanout",
                    r#"request_snapshot = _manifest_snapshot(store, policy, run["run_id"], args.request_manifest)"#,
                )
            }
        }
        ReviewAssessment => {
            record_dispatch_frame(
                output,
                cli,
                1584,
                "return command_review_fanout_assessment(",
            );
            (
                1507,
                "command_review_fanout_assessment",
                r#"snapshot = _manifest_snapshot(store, policy, run["run_id"], args.assessment_manifest)"#,
            )
        }
        EvidenceEnable => {
            let call = EVIDENCE_ENABLE_CALL;
            frame(
                output,
                cli,
                3286,
                "execute",
                &format!("result = {call}"),
                Some(call),
            );
            (
                1654,
                "command_evidence_enable",
                r#"snapshot = _manifest_snapshot(store, policy, run["run_id"], args.coverage_manifest)"#,
            )
        }
        JoinAdvance => {
            join_advance_dispatch_frame(output, cli);
            (
                2201,
                "command_join_advance",
                r#"output_snapshot = _manifest_snapshot(store, policy, run["run_id"], args.generated_output_plan)"#,
            )
        }
        Recover => {
            let call = RECOVER_CALL;
            frame(
                output,
                cli,
                3280,
                "execute",
                &format!("result = {call}"),
                Some(call),
            );
            (
                2598,
                "command_recover",
                r#"snapshot = _manifest_snapshot(store, policy, run["run_id"], args.recovery_manifest)"#,
            )
        }
        Block => {
            frame(
                output,
                cli,
                3313,
                "execute",
                "result = command_run_control(",
                Some("command_run_control("),
            );
            (
                3068,
                "command_run_control",
                r#"snapshot = _manifest_snapshot(store, policy, run["run_id"], args.evidence_manifest, 128 * 1024)"#,
            )
        }
    };
    frame(
        output,
        cli,
        line,
        function,
        source,
        source.split_once(" = ").map(|(_, call)| call),
    );
    let call = r#"safe_json_snapshot(manifest_path, [inbox], min(maximum, int(policy["limits"]["manifest_bytes"])))"#;
    frame(
        output,
        cli,
        496,
        "_manifest_snapshot",
        &format!("return {call}"),
        Some(call),
    );
}

fn initialization_dispatch_frame(output: &mut PyString, cli: &Path) {
    frame(
        output,
        cli,
        3258,
        "execute",
        "return command_init(args, repo, policy, policy_snapshot, state), 0",
        Some("command_init(args, repo, policy, policy_snapshot, state)"),
    );
}

fn plan_snapshot_frames(
    output: &mut PyString,
    cli: &Path,
    command: PlanTraceCommand,
    snapshot: PlanSnapshot,
) {
    match command {
        PlanTraceCommand::Preview => {
            let call = "_prepare_execution_plan(args, repo, policy, policy_snapshot, runtime_root)";
            frame(
                output,
                cli,
                3247,
                "execute",
                &format!("_snapshot, _full_task, _task, plan = {call}"),
                Some(call),
            );
        }
        PlanTraceCommand::Initialize => {
            initialization_dispatch_frame(output, cli);
            let call =
                "_prepare_execution_plan(args, repo, policy, policy_snapshot, store.state_root)";
            frame(
                output,
                cli,
                699,
                "command_init",
                &format!("task_snapshot, full_task, task, execution_plan = {call}"),
                Some(call),
            );
        }
    }
    let (line, source) = match snapshot {
        PlanSnapshot::Task => (665, "task_snapshot = safe_json_snapshot("),
        PlanSnapshot::Allowance => (672, "allowance_snapshot = safe_json_snapshot("),
    };
    frame(
        output,
        cli,
        line,
        "_prepare_execution_plan",
        source,
        Some("safe_json_snapshot("),
    );
}

fn policy_snapshot_frames(
    output: &mut PyString,
    engine: &Path,
    cli: &Path,
    source: PolicySnapshotSource,
) {
    frame(
        output,
        cli,
        3235,
        "execute",
        "policy, policy_snapshot = load_policy(repo, runtime_root=runtime_root)",
        Some("load_policy(repo, runtime_root=runtime_root)"),
    );
    let (line, assignment, call, suffix) = match source {
        PolicySnapshotSource::Legacy => (
            507,
            "snapshot",
            "safe_json_snapshot(legacy_path, [installed_root], 256 * 1024)",
            "",
        ),
        PolicySnapshotSource::Seed => (
            519,
            "seed",
            "safe_json_snapshot(default_path, [default_home], 256 * 1024)",
            ".data",
        ),
        PolicySnapshotSource::Runtime => (
            520,
            "snapshot",
            "safe_json_snapshot(_ensure_profile_policy(root, home, seed), [home], 256 * 1024)",
            "",
        ),
    };
    frame(
        output,
        &engine.join("config.py"),
        line,
        "load_policy",
        &format!("{assignment} = {call}{suffix}"),
        Some(call),
    );
}

fn join_advance_dispatch_frame(output: &mut PyString, cli: &Path) {
    frame(
        output,
        cli,
        3307,
        "execute",
        r#"result = command_join_validate(args, connection, run) if args.join_kind == "validate" else command_join_advance("#,
        Some("command_join_advance("),
    );
}

fn check_dispatch_frame(output: &mut PyString, cli: &Path) {
    frame(
        output,
        cli,
        3294,
        "execute",
        "result = handler(",
        Some("handler("),
    );
}

fn format7_check_dispatch_frames(output: &mut PyString, cli: &Path) {
    check_dispatch_frame(output, cli);
    return_call_frame(
        output,
        cli,
        1831,
        "command_check_run",
        "command_check_v2(args, connection, run, policy, task, store, semantic_validator)",
    );
}

fn persisted_task_mutation_frames(
    output: &mut PyString,
    engine: &Path,
    cli: &Path,
    command: TaskValidationMutation,
) {
    use TaskValidationMutation::*;

    const OPERATION_MUTATION: &str =
        r#"store.mutate(connection, run["run_id"], opaque(args.op_id, "op_id"), request, action,"#;

    let (line, function) = match command {
        RunControl | Complete => {
            frame(
                output,
                cli,
                3313,
                "execute",
                "result = command_run_control(",
                Some("command_run_control("),
            );
            if matches!(command, Complete) {
                let call = "command_complete(args, connection, run, policy, task, store, semantic_validator)";
                frame(
                    output,
                    cli,
                    3045,
                    "command_run_control",
                    &format!("return {call}"),
                    Some(call),
                );
                (3033, "command_complete")
            } else {
                (3100, "command_run_control")
            }
        }
        BranchResult => {
            branch_mutation_entry_frames(output, cli);
            return;
        }
        NextClaim => {
            frame(
                output,
                cli,
                3302,
                "execute",
                "result = command_next(",
                Some("command_next("),
            );
            (2168, "command_next")
        }
        RecordControl => {
            record_dispatch_frame(output, cli, 1588,
                "return _record_control(args, connection, run, policy, task, store, semantic_validator)");
            (1260, "_record_control")
        }
        JoinAdvance => {
            join_advance_dispatch_frame(output, cli);
            (2455, "command_join_advance")
        }
        CheckAbandon | LegacyCheckRun => {
            check_dispatch_frame(output, cli);
            if matches!(command, CheckAbandon) {
                return_call_frame(
                    output,
                    cli,
                    1821,
                    "command_check_abandon",
                    OPERATION_MUTATION,
                );
                return;
            }
            (1864, "command_check_run")
        }
        Usage => {
            record_dispatch_frame(
                output,
                cli,
                1573,
                "return usage.record(args, connection, run, store, semantic_validator)",
            );
            let call = r#"store.mutate(connection, run["run_id"], operation_id, request, action, semantic_validator=semantic_validator)"#;
            return_call_frame(output, &engine.join("usage.py"), 522, "record", call);
            return;
        }
        FanoutAssessment => {
            record_dispatch_frame(output, cli, 1577, "return command_fanout_assessment(");
            (1343, "command_fanout_assessment")
        }
        ReviewFanout => {
            record_dispatch_frame(output, cli, 1582,
                "return command_review_fanout(args, connection, run, policy, task, store, semantic_validator)");
            (1496, "command_review_fanout")
        }
        ReviewFanoutAssessment => {
            record_dispatch_frame(
                output,
                cli,
                1584,
                "return command_review_fanout_assessment(",
            );
            (1561, "command_review_fanout_assessment")
        }
        EvidenceEnable => {
            frame(
                output,
                cli,
                3286,
                "execute",
                &format!("result = {EVIDENCE_ENABLE_CALL}"),
                Some(EVIDENCE_ENABLE_CALL),
            );
            return_call_frame(
                output,
                cli,
                1693,
                "command_evidence_enable",
                OPERATION_MUTATION,
            );
            return;
        }
        Recover => {
            frame(
                output,
                cli,
                3280,
                "execute",
                &format!("result = {RECOVER_CALL}"),
                Some(RECOVER_CALL),
            );
            (2682, "command_recover")
        }
        Format7CheckStart | Format7CheckFinish => {
            format7_check_dispatch_frames(output, cli);
            if matches!(command, Format7CheckStart) {
                let call = r#"store.mutate(connection, run["run_id"], start_id, request, start, semantic_validator=semantic_validator)"#;
                frame(
                    output,
                    cli,
                    1773,
                    "command_check_v2",
                    &format!("reserved = {call}"),
                    Some(call),
                );
            } else {
                let call = r#"store.mutate(connection, run["run_id"], finish_id, finish_request, finish, semantic_validator=semantic_validator)"#;
                return_call_frame(output, cli, 1808, "command_check_v2", call);
            }
            return;
        }
    };
    frame(
        output,
        cli,
        line,
        function,
        "return store.mutate(",
        Some("store.mutate("),
    );
}

fn return_call_frame(output: &mut PyString, path: &Path, line: u16, function: &str, call: &str) {
    frame(
        output,
        path,
        line,
        function,
        &format!("return {call}"),
        Some(call),
    );
}

fn persisted_task_frames(
    output: &mut PyString,
    engine: &Path,
    cli: &Path,
    validation: PersistedTaskValidation,
) {
    match validation {
        PersistedTaskValidation::ReadOnly => {
            frame(output, cli, 3275, "execute", "verify_semantic_state(", None);
        }
        PersistedTaskValidation::Format7CheckDirect => {
            format7_check_dispatch_frames(output, cli);
            frame(
                output,
                cli,
                1714,
                "command_check_v2",
                "semantic_validator(connection, run)",
                None,
            );
        }
        PersistedTaskValidation::Mutation(command, phase) => {
            persisted_task_mutation_frames(output, engine, cli, command);
            let (line, source) = match phase {
                MutationValidationPhase::BeforeAction => {
                    (948, "semantic_validator(connection, run)")
                }
                MutationValidationPhase::AfterAction => {
                    (963, "semantic_validator(connection, updated_run)")
                }
            };
            frame(
                output,
                &engine.join("state.py"),
                line,
                "mutate",
                source,
                None,
            );
        }
    }
    if !matches!(validation, PersistedTaskValidation::ReadOnly) {
        frame(
            output,
            cli,
            3263,
            "<lambda>",
            "semantic_validator = lambda conn, current: verify_semantic_state(",
            Some("verify_semantic_state("),
        );
    }
    let validator = engine.join("validator.py");
    frame(
        output,
        &validator,
        1809,
        "verify_semantic_state",
        "_verify_semantic_state(",
        None,
    );
    let call = r#"safe_json_snapshot(task_path, roots, policy["artifact_kinds"]["task_brief"]["max_bytes"])"#;
    frame(
        output,
        &validator,
        1706,
        "_verify_semantic_state",
        &format!("snapshot = {call}"),
        Some(call),
    );
}

fn snapshot_json_frames(
    output: &mut PyString,
    engine: &Path,
    cli: &Path,
    site: SnapshotJsonTraceSite,
) {
    match site {
        SnapshotJsonTraceSite::Manifest(site) => manifest_json_frames(output, cli, site),
        SnapshotJsonTraceSite::Plan(command, snapshot) => {
            plan_snapshot_frames(output, cli, command, snapshot)
        }
        SnapshotJsonTraceSite::Policy(source) => {
            policy_snapshot_frames(output, engine, cli, source)
        }
        SnapshotJsonTraceSite::ConstraintBundle | SnapshotJsonTraceSite::ConstraintContext => {
            let (line, input) = match site {
                SnapshotJsonTraceSite::ConstraintBundle => (3250, "bundle"),
                _ => (3251, "context"),
            };
            let call = format!(
                r#"safe_json_snapshot(Path(args.{input}), _reference_roots(repo, policy, runtime_root), policy["limits"]["manifest_bytes"])"#
            );
            frame(
                output,
                cli,
                line,
                "execute",
                &format!("{input} = {call}.parsed"),
                Some(&call),
            );
        }
        SnapshotJsonTraceSite::ActivatedBundle => {
            initialization_dispatch_frame(output, cli);
            let call = r#"safe_json_snapshot(Path(bundle_artifact.source_path), _reference_roots(repo, policy, store.state_root), policy["limits"]["manifest_bytes"])"#;
            frame(
                output,
                cli,
                711,
                "command_init",
                &format!("bundle = {call}.parsed"),
                Some(call),
            );
        }
        SnapshotJsonTraceSite::ConstraintSupport(command) => {
            constraint_support_call(output, cli, command);
            let call = r#"safe_json_snapshot(Path(artifact.source_path), _reference_roots(repo, policy, runtime_root), policy["limits"]["manifest_bytes"])"#;
            frame(
                output,
                cli,
                1610,
                "_constraint_support",
                &format!("payload = {call}.parsed"),
                Some(call),
            );
        }
        SnapshotJsonTraceSite::StagedConsolidation => {
            branch_mutation_frames(output, engine, cli);
            let call = r#"safe_json_snapshot(Path(staged.source_path), _reference_roots(repo_path, policy, store.state_root), policy["limits"]["manifest_bytes"])"#;
            frame(
                output,
                cli,
                878,
                "action",
                &format!("staged_snapshot = {call}"),
                Some(call),
            );
        }
        SnapshotJsonTraceSite::PersistedTask(validation) => {
            persisted_task_frames(output, engine, cli, validation);
        }
    }
    let call = r#"json.loads(snapshot.data.decode("utf-8"))"#;
    frame(
        output,
        &engine.join("contracts.py"),
        231,
        "safe_json_snapshot",
        &format!("parsed = {call}"),
        Some(call),
    );
    frame(
        output,
        Path::new(r"C:\Python312\Lib\json\__init__.py"),
        346,
        "loads",
        "return _default_decoder.decode(s)",
        Some("_default_decoder.decode(s)"),
    );
    let decoder = Path::new(r"C:\Python312\Lib\json\decoder.py");
    frame(
        output,
        decoder,
        338,
        "decode",
        "obj, end = self.raw_decode(s, idx=_w(s, 0).end())",
        Some("self.raw_decode(s, idx=_w(s, 0).end())"),
    );
    frame(
        output,
        decoder,
        354,
        "raw_decode",
        "obj, end = self.scan_once(s, idx)",
        Some("self.scan_once(s, idx)"),
    );
}

fn branch_mutation_entry_frames(output: &mut PyString, cli: &Path) {
    record_dispatch_frame(output, cli, 1575,
        "return _record_branch_result(args, connection, run, policy, task, store, semantic_validator)");
    frame(
        output,
        cli,
        947,
        "_record_branch_result",
        "return store.mutate(",
        Some("store.mutate("),
    );
}

fn branch_mutation_frames(output: &mut PyString, engine: &Path, cli: &Path) {
    branch_mutation_entry_frames(output, cli);
    frame(
        output,
        &engine.join("state.py"),
        950,
        "mutate",
        "result = action(connection, run, revision)",
        Some("action(connection, run, revision)"),
    );
}

fn branch_result_frames(
    output: &mut PyString,
    engine: &Path,
    cli: &Path,
    site: BranchResultTraceSite,
) {
    branch_mutation_frames(output, engine, cli);
    match site {
        BranchResultTraceSite::ManifestAttempt => {
            frame(output, cli, 774, "action",
                "_check_attempt(current_branch, env, args.attempt_id, args.claim_token, snapshot.parsed)", None);
            frame(
                output,
                cli,
                609,
                "_check_attempt",
                r#"manifest.get("attempt_id") != attempt_id or manifest.get("claim_digest") != token_digest"#,
                Some("manifest.get"),
            );
        }
        BranchResultTraceSite::MapperStatus => frame(
            output,
            cli,
            784,
            "action",
            r#"if current_branch["node_key"] == "impact_mapper" and snapshot.parsed.get("status") != "failed":"#,
            Some("snapshot.parsed.get"),
        ),
        BranchResultTraceSite::MapperRoute => mapper_impact_frame(
            output,
            engine,
            cli,
            731,
            r#"if route not in ROUTES or route not in policy["routes"]:"#,
            "route not in ROUTES",
            0,
        ),
        BranchResultTraceSite::MapperEvidenceReferences => mapper_impact_frame(
            output,
            engine,
            cli,
            754,
            r#"for ref in value["evidence_refs"]:"#,
            r#"value["evidence_refs"]"#,
            "value".len(),
        ),
        BranchResultTraceSite::ContinuationFields => continuation_frame(
            output,
            cli,
            634,
            "if not required.issubset(value):",
            "required.issubset(value)",
        ),
        BranchResultTraceSite::FindingsIterable => continuation_frame(
            output,
            cli,
            658,
            FINDINGS_SOURCE,
            r#"value.get("findings", [])"#,
        ),
        BranchResultTraceSite::FindingId => continuation_frame(
            output,
            cli,
            658,
            FINDINGS_SOURCE,
            r#"item.get("finding_id")"#,
        ),
    }
}

fn mapper_impact_frame(
    output: &mut PyString,
    engine: &Path,
    cli: &Path,
    line: u16,
    source: &str,
    expression: &str,
    receiver: usize,
) {
    frame(
        output,
        cli,
        785,
        "action",
        "impact = validate_impact_map(snapshot.parsed, task, policy)",
        Some("validate_impact_map(snapshot.parsed, task, policy)"),
    );
    frame(
        output,
        &engine.join("contracts.py"),
        line,
        "validate_impact_map",
        source,
        None,
    );
    underline(output, source, expression, receiver);
}

fn continuation_frame(output: &mut PyString, cli: &Path, line: u16, source: &str, highlight: &str) {
    frame(
        output,
        cli,
        815,
        "action",
        "_validate_review_continuation_result(conn, current_branch, env, snapshot.parsed)",
        None,
    );
    frame(
        output,
        cli,
        line,
        CONTINUATION_FUNCTION,
        source,
        Some(highlight),
    );
}

fn constraint_support_call(output: &mut PyString, cli: &Path, command: ConstraintCommand) {
    match command {
        ConstraintCommand::Preview => frame(
            output,
            cli,
            3252,
            "execute",
            "support = _constraint_support(repo, policy, bundle, runtime_root)",
            Some("_constraint_support(repo, policy, bundle, runtime_root)"),
        ),
        ConstraintCommand::Initialize => {
            initialization_dispatch_frame(output, cli);
            frame(
                output,
                cli,
                712,
                "command_init",
                "support = _constraint_support(repo, policy, bundle, store.state_root)",
                Some("_constraint_support(repo, policy, bundle, store.state_root)"),
            );
        }
    }
}

fn constraint_bundle_call(output: &mut PyString, cli: &Path, command: ConstraintCommand) {
    constraint_support_call(output, cli, command);
    frame(
        output,
        cli,
        1600,
        "_constraint_support",
        "validate_bundle(bundle)",
        None,
    );
}

fn constraint_selection_call(output: &mut PyString, cli: &Path) {
    frame(
        output,
        cli,
        3253,
        "execute",
        "result = select_constraints(bundle, context, supported={item.ref for item in support},",
        Some("select_constraints(bundle, context, supported={item.ref for item in support},"),
    );
}

fn constraint_validation_frames(
    output: &mut PyString,
    engine: &Path,
    cli: &Path,
    command: ConstraintCommand,
    site: ConstraintValidationSite,
) {
    let constraints = engine.join("constraints.py");
    match site {
        ConstraintValidationSite::RecordState => {
            constraint_bundle_call(output, cli, command);
            frame(
                output,
                &constraints,
                36,
                "validate_bundle",
                r#"if record["state"] not in {"candidate", "active", "invalidated", "superseded"}:"#,
                Some(
                    r#"record["state"] not in {"candidate", "active", "invalidated", "superseded"}"#,
                ),
            );
        }
        ConstraintValidationSite::RecordAudience => {
            constraint_bundle_call(output, cli, command);
            frame(
                output, &constraints, 44, "validate_bundle",
                "if not isinstance(record[field], list) or not 1 <= len(record[field]) <= 32 or len(set(record[field])) != len(record[field]):",
                Some("set(record[field])"),
            );
        }
        ConstraintValidationSite::ContextKeys => {
            constraint_selection_call(output, cli);
            frame(
                output,
                &constraints,
                78,
                "select_constraints",
                r#"require_keys(context, {"repository_id", "acceptance_ids", "role", "scope_paths"},"#,
                None,
            );
            frame(
                output,
                &engine.join("contracts.py"),
                78,
                "require_keys",
                "missing = required_set - set(value)",
                Some("set(value)"),
            );
        }
        ConstraintValidationSite::ContextIndex => {
            constraint_selection_call(output, cli);
            let source = r#"opaque(context["repository_id"], "constraint_context.repository_id")"#;
            frame(output, &constraints, 80, "select_constraints", source, None);
            underline(
                output,
                source,
                r#"context["repository_id"]"#,
                "context".len(),
            );
        }
    }
}
