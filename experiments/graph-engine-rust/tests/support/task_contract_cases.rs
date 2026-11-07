use super::{
    contract, edit, grant, planner, policy, role_grant, set, strings, validate_task, EngineError,
    NativePolicyFiles, PolicyCase, Value,
};
use graph_engine::{
    application::{artifacts::ArtifactResolver, policy::PolicyFilePort},
    compat,
    domain::{
        contracts::validate_ref, policy as domain_policy, task_contract::validate_impact_map,
    },
    infrastructure::sqlite::SqliteSession,
};
use std::{collections::BTreeSet, fs};

#[allow(dead_code)]
#[path = "evidence_schema.rs"]
mod evidence_schema;
use evidence_schema::{validate_schema, SchemaMismatch};

fn task_schema() -> Value {
    compat::loads(include_bytes!(
        "../../../../skills/software-engineering-graph/references/task-brief.schema.json"
    ))
    .unwrap()
}

fn repository_schema() -> Value {
    compat::loads(include_bytes!(
        "../../../../skills/software-engineering-graph/references/repository-config.schema.json"
    ))
    .unwrap()
}

fn task_v2(case: &PolicyCase, route: &str) -> Value {
    let mut task = case.graph.fixture.graph.task("delivery", route, &[]);
    task.set("schema_version", 2_i64.into());
    task.set(
        "model_sizing",
        Value::object([
            ("scope_extent", "bounded".into()),
            ("uncertainty", "low".into()),
        ]),
    );
    task
}

fn delivery_task(case: &PolicyCase) -> Value {
    let mut task = task_v2(case, "delivery_only");
    task.set(
        "delivery_readiness",
        Value::object([
            ("requirements_source", "JIRA-123".into()),
            ("requirements_complete", true.into()),
            ("acceptance_criteria_complete", true.into()),
            ("implementation_decisions_resolved", true.into()),
            ("architecture_decisions_resolved", true.into()),
            ("unresolved_items", strings(&[])),
        ]),
    );
    task
}

fn validate(case: &PolicyCase, task: &Value) -> Result<Value, EngineError> {
    validate_task(task, &case.snapshot.digest, &case.policy, cfg!(windows))
}

fn model_selection(model: &str, effort: &str) -> Value {
    Value::object([("model", model.into()), ("reasoning_effort", effort.into())])
}

#[test]
fn test_model_selection_schema_matches_runtime_contract() {
    let case = PolicyCase::new();
    let schema = task_schema();
    let mut task = task_v2(&case, "full_delivery");
    task.set(
        "model_overrides",
        Value::object([
            ("senior_engineer", model_selection("gpt-5.6-sol", "medium")),
            (
                "simplification_assignment",
                model_selection("gpt-6.1-sol", "medium"),
            ),
        ]),
    );
    validate(&case, &task).unwrap();
    validate_schema(&task, &schema, &schema).unwrap();
    for effort in ["low", "medium", "high", "xhigh", "max", "ultra"] {
        let mut terra = task.clone();
        set(
            &mut terra,
            &["model_overrides", "senior_engineer"],
            model_selection("gpt-6.1-terra", effort),
        );
        validate(&case, &terra).unwrap();
        validate_schema(&terra, &schema, &schema).unwrap();
    }
    for overrides in [
        strings(&[]),
        Value::object([("unknown", Value::object([]))]),
        Value::object([(
            "senior_engineer",
            Value::object([("model", "gpt-5.6-sol".into())]),
        )]),
        Value::object([("tech_lead", model_selection("primary-thread", "medium"))]),
        Value::object([("tech_lead", model_selection("gpt-5.6-sol", "inherited"))]),
    ] {
        let mut invalid = task.clone();
        invalid.set("model_overrides", overrides);
        contract(validate(&case, &invalid), None);
        assert!(validate_schema(&invalid, &schema, &schema).is_err());
    }
    let mut legacy = case.task();
    legacy.set("model_overrides", task["model_overrides"].clone());
    contract(validate(&case, &legacy), None);
    assert!(validate_schema(&legacy, &schema, &schema).is_err());
}

#[test]
fn test_runtime_artifact_reference_stays_in_selected_repository_home() {
    let case = PolicyCase::new();
    let files = NativePolicyFiles::default();
    let graph = &case.graph.fixture.graph;
    let policy_path =
        policy::profile_policy_path(&files, case.repo(), &graph.runtime_root).unwrap();
    fs::remove_file(policy_path).unwrap();
    let readme = if cfg!(windows) {
        "Project evidence\r\n"
    } else {
        "Project evidence\n"
    };
    fs::write(case.repo().join("README.md"), readme).unwrap();
    let (selected, _) = case.load().unwrap();
    let repository = selected["repository_id"].text().unwrap();
    let relative = format!("artifacts/{repository}/RUN-1/evidence.json");
    let artifact = graph
        .runtime_root
        .join("artifacts")
        .join(&repository)
        .join("RUN-1")
        .join("evidence.json");
    fs::create_dir_all(artifact.parent().unwrap()).unwrap();
    fs::write(&artifact, r#"{"result":"verified"}"#).unwrap();
    let resolver = ArtifactResolver {
        files: &files,
        snapshots: &files,
        repository: case.repo(),
        skill: &graph.skill_root,
        installation: &graph.skill_root,
        policy: &selected,
        runtime: Some(&graph.runtime_root),
        ledger: None,
    };
    let kind = "acceptance_evidence".into();
    let verified = resolver
        .resolve_unhashed_reference(&format!("runtime:{relative}").into(), &kind)
        .unwrap();
    assert!(verified.source_type.is_text("runtime"));
    let readme = resolver
        .resolve_unhashed_reference(&"repo:README.md".into(), &kind)
        .unwrap();
    assert!(readme.source_type.is_text("repo"));
    let verified_again = resolver
        .resolve_reference(&verified.reference, &verified.sha256, &kind)
        .unwrap();
    assert!(verified_again
        .source_path
        .py_eq(&Value::String(files.spelling(&artifact))));
    contract(
        resolver.resolve_unhashed_reference(
            &"runtime:artifacts/another-repository/evidence.json".into(),
            &kind,
        ),
        Some("OUTSIDE_ALLOWED_ROOT"),
    );
    fs::write(&artifact, r#"{"result":"changed"}"#).unwrap();
    contract(
        resolver.resolve_reference(&verified.reference, &verified.sha256, &kind),
        Some("INPUT_DIGEST_MISMATCH"),
    );
}

#[test]
fn test_real_policy_and_task_validate() {
    let case = PolicyCase::new();
    let task = case
        .graph
        .fixture
        .graph
        .task("delivery", "full_delivery", &["security_privacy"]);
    assert!(validate(&case, &task).unwrap()["mandatory_impact_tags"]
        .py_eq(&strings(&["security_privacy"])));
}

#[test]
fn test_v2_task_requires_exact_structured_model_sizing() {
    let case = PolicyCase::new();
    let task = task_v2(&case, "full_delivery");
    let sizing = Value::object([
        ("scope_extent", "bounded".into()),
        ("uncertainty", "low".into()),
    ]);
    assert!(validate(&case, &task).unwrap()["model_sizing"].py_eq(&sizing));
    let mut invalid = task.clone();
    invalid.remove("model_sizing").unwrap();
    contract(validate(&case, &invalid), Some("MISSING_FIELD"));
    let mut invalid = task.clone();
    set(&mut invalid, &["model_sizing", "extra"], "no".into());
    contract(validate(&case, &invalid), Some("UNKNOWN_FIELD"));
    let mut invalid = task;
    set(
        &mut invalid,
        &["model_sizing", "scope_extent"],
        "global".into(),
    );
    contract(validate(&case, &invalid), Some("UNKNOWN_VALUE"));
    let mut legacy = case.task();
    legacy.set("model_sizing", sizing);
    contract(validate(&case, &legacy), Some("UNKNOWN_FIELD"));
}

#[test]
fn test_delivery_only_requires_complete_bounded_readiness() {
    let case = PolicyCase::new();
    let mut task = delivery_task(&case);
    task.set("mandatory_impact_tags", strings(&["production_behavior"]));
    task.set("risk_level", "medium".into());
    set(
        &mut task,
        &["model_sizing", "scope_extent"],
        "cross_file".into(),
    );
    set(&mut task, &["model_sizing", "uncertainty"], "medium".into());
    let validated = validate(&case, &task).unwrap();
    assert!(validated["minimum_route"].is_text("delivery_only"));
    assert!(validated["delivery_readiness"].py_eq(&task["delivery_readiness"]));
    let mut missing = delivery_task(&case);
    missing.remove("delivery_readiness").unwrap();
    contract(
        validate(&case, &missing),
        Some("DELIVERY_READINESS_REQUIRED"),
    );
    for (path, value, code) in [
        (
            &["delivery_readiness", "requirements_complete"][..],
            false.into(),
            "READINESS_ASSERTION_REQUIRED",
        ),
        (
            &["delivery_readiness", "unresolved_items"][..],
            strings(&["Choose storage"]),
            "UNRESOLVED_ITEMS_PRESENT",
        ),
        (
            &["required_human_decisions"][..],
            strings(&["choose-storage"]),
            "DELIVERY_ONLY_UNRESOLVED_DECISION",
        ),
        (
            &["risk_level"][..],
            "high".into(),
            "DELIVERY_ONLY_INELIGIBLE",
        ),
        (
            &["mandatory_impact_tags"][..],
            strings(&["security_privacy"]),
            "DELIVERY_ONLY_INELIGIBLE",
        ),
        (
            &["model_sizing", "scope_extent"][..],
            "broadly_cross_cutting".into(),
            "DELIVERY_ONLY_INELIGIBLE",
        ),
        (
            &["model_sizing", "uncertainty"][..],
            "high".into(),
            "DELIVERY_ONLY_INELIGIBLE",
        ),
    ] {
        let mut candidate = delivery_task(&case);
        set(&mut candidate, path, value);
        contract(validate(&case, &candidate), Some(code));
    }
    let legacy = case
        .graph
        .fixture
        .graph
        .task("delivery", "delivery_only", &[]);
    contract(
        validate(&case, &legacy),
        Some("DELIVERY_READINESS_REQUIRED"),
    );
}

#[test]
fn test_published_task_schema_accepts_exact_v1_and_v2_contracts() {
    let case = PolicyCase::new();
    let schema = task_schema();
    let mut v3 = task_v2(&case, "full_delivery");
    v3.set("schema_version", 3_i64.into());
    v3.set(
        "helper_allowance",
        Value::object([
            ("ref", "repo:docs/helper-allowance.json".into()),
            ("sha256", "a".repeat(64).into()),
        ]),
    );
    for task in [
        case.task(),
        task_v2(&case, "full_delivery"),
        v3,
        delivery_task(&case),
    ] {
        validate_schema(&task, &schema, &schema).unwrap();
    }
    assert!(validate_schema(&task_v2(&case, "delivery_only"), &schema, &schema).is_err());
    let mut legacy = case.task();
    legacy.set(
        "model_sizing",
        task_v2(&case, "full_delivery")["model_sizing"].clone(),
    );
    assert!(validate_schema(&legacy, &schema, &schema).is_err());
}

#[test]
fn test_mapper_cannot_downgrade_or_remove_tag() {
    let case = PolicyCase::new();
    let graph = &case.graph.fixture.graph;
    let tagged = validate(
        &case,
        &graph.task("delivery", "full_delivery", &["security_privacy"]),
    )
    .unwrap();
    let fast = validate(&case, &graph.task("delivery", "fast_path", &[])).unwrap();
    let delivery = validate(&case, &delivery_task(&case)).unwrap();
    for (task, route, tags, code) in [
        (
            &tagged,
            "fast_path",
            &["security_privacy"][..],
            Some("ROUTE_DOWNGRADE"),
        ),
        (
            &tagged,
            "full_delivery",
            &[][..],
            Some("MANDATORY_TAG_REMOVED"),
        ),
        (
            &fast,
            "fast_path",
            &["security_privacy"][..],
            Some("FAST_PATH_INVARIANT"),
        ),
        (&delivery, "fast_path", &[][..], Some("ROUTE_DOWNGRADE")),
        (
            &delivery,
            "delivery_only",
            &["production_behavior"][..],
            None,
        ),
        (
            &delivery,
            "delivery_only",
            &["security_privacy"][..],
            Some("DELIVERY_ONLY_INELIGIBLE"),
        ),
    ] {
        let impact = Value::object([
            ("schema_version", 1_i64.into()),
            ("task_id", "TASK-1".into()),
            ("route_label", route.into()),
            ("impact_tags", strings(tags)),
            ("evidence_refs", strings(&[])),
            ("attempt_id", "attempt".into()),
            ("claim_digest", "a".repeat(64).into()),
        ]);
        let result = validate_impact_map(&impact, task, &case.policy);
        if let Some(code) = code {
            contract(result.map_err(Into::into), Some(code));
        } else {
            result.unwrap();
        }
    }
}

#[test]
fn test_advisory_task_cannot_receive_write_authority() {
    let case = PolicyCase::new();
    let mut task = case.graph.fixture.graph.task("advisory", "advisory", &[]);
    set(
        &mut task,
        &["authority", "capabilities"],
        Value::list([grant("filesystem_write", "edit", "repo:docs/")]),
    );
    contract(validate(&case, &task), Some("ADVISORY_MUST_BE_READ_ONLY"));
}

#[test]
fn test_secret_fields_and_urls_are_rejected_without_echo() {
    let case = PolicyCase::new();
    let mut task = case.task();
    task.set("password", "do-not-echo".into());
    assert!(!contract(validate(&case, &task), None)
        .to_string()
        .contains("do-not-echo"));
    contract(
        validate_ref(&"https://example.invalid/token".into(), "ref", false).map_err(Into::into),
        Some("URL_FORBIDDEN"),
    );
}

#[test]
fn test_repository_added_checks_cannot_expand_command_capabilities() {
    let case = PolicyCase::new();
    let mut value = case.policy.clone();
    set(
        &mut value,
        &["required_checks", "repo-added"],
        Value::object([
            ("command_id", "repository-added-command".into()),
            ("mandatory", true.into()),
        ]),
    );
    role_grant(
        &mut value,
        "senior_engineer",
        grant("command", "run", "repository-added-command"),
    );
    case.write(&value);
    let error = contract(case.load(), None);
    assert!(
        ["ENGINE_COMMAND_SET_CHANGED", "ENGINE_AUTHORITY_EXCEEDED"].contains(&error.code.as_str())
    );
    let wildcard = grant("command", "run", "*");
    assert!(!domain_policy::role_ceiling("senior_engineer")
        .iter()
        .any(|value| value.py_eq(&wildcard)));
}

#[test]
fn test_oversized_manifest_is_rejected() {
    let mut case = PolicyCase::new();
    case.graph.fixture.initialize(&case.task(), true).unwrap();
    let branch = case.graph.claim();
    let store = &case.graph.fixture.graph.store;
    let path = store
        .inbox_root("albanian-live-translate", "RUN-1")
        .join("oversized.json");
    fs::write(&path, format!("{{\"padding\":\"{}\"}}", "x".repeat(270000))).unwrap();
    if !cfg!(windows) {
        store.platform.set_mode(&path, 0o600).unwrap();
    }
    contract(
        case.graph.run(&[
            "record",
            "branch-result",
            "--run-id",
            "RUN-1",
            "--branch-id",
            &branch["branch_id"].text().unwrap(),
            "--attempt-id",
            &branch["attempt_id"].text().unwrap(),
            "--claim-token",
            &branch["claim_token"].text().unwrap(),
            "--result-manifest",
            path.to_str().unwrap(),
            "--op-id",
            "oversized-1",
        ]),
        Some("FILE_TOO_LARGE"),
    );
}

fn keys(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, _)| key.utf8().unwrap())
        .collect()
}

fn text_set(value: &Value) -> BTreeSet<String> {
    value
        .as_list()
        .unwrap()
        .iter()
        .map(|value| value.text().unwrap())
        .collect()
}

#[test]
fn test_repository_policy_reaches_required_roles_and_checks() {
    let case = PolicyCase::new();
    let roles: BTreeSet<_> = case.policy["node_templates"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(_, item)| item["role"].text().unwrap())
        .collect();
    for role in [
        "impact_mapper",
        "tech_lead",
        "software_architect",
        "senior_engineer",
        "code_reviewer",
        "test_engineer",
        "supervisor",
    ] {
        assert!(roles.contains(role));
    }
    for key in ["design_research_architecture", "design_research_validation"] {
        let template = &case.policy["node_templates"][key];
        assert!(template["role"].is_text("impact_mapper"));
        assert!(template["stages"].py_eq(&strings(&["research"])));
        assert_eq!(template["max_retries"].as_i64(), Some(1));
        for flag in [
            "artifact_required",
            "evidence_required",
            "decision_forbidden",
            "findings_forbidden",
        ] {
            assert!(template["output_contract"][flag].py_eq(&true.into()));
        }
    }
    assert!(case.policy["required_checks"]["repo-check"]["command_id"].is_text("npm-run-check"));
    let tags = [
        "audio_realtime_translation",
        "ios_webkit_native",
        "release_operations",
        "security_privacy",
    ];
    assert_eq!(keys(&case.policy["specialists"]), text_set(&strings(&tags)));
    let schema = repository_schema();
    assert_eq!(text_set(&schema["required"]), keys(&case.policy));
    assert!(schema["additionalProperties"].py_eq(&false.into()));
    let maximum_reviewers = 2 + tags.len();
    let selected = tags.map(str::to_owned);
    let legacy_plan = Value::object([]);
    let native =
        planner::delivery_review_nodes(&case.policy, &selected, 0, Some(&legacy_plan)).unwrap();
    assert_eq!(native.len(), maximum_reviewers);
    assert_eq!(maximum_reviewers, 6);
    let maximum = domain_policy::artifact_maximum("collection").unwrap();
    assert_eq!(
        case.policy["artifact_kinds"]["collection"]["max_bytes"].as_i64(),
        Some(maximum as i64)
    );
    assert!(
        maximum
            >= maximum_reviewers * domain_policy::artifact_maximum("branch_result").unwrap()
                + 64 * 1024
    );
}

#[test]
fn test_repository_fixture_matches_published_schema() {
    let case = PolicyCase::new();
    let schema = repository_schema();
    let mut fixture = compat::loads(&case.graph.fixture.graph.policy_bytes).unwrap();
    validate_schema(&fixture, &schema, &schema).unwrap();
    let v2 = case.v2();
    validate_schema(&v2, &schema, &schema).unwrap();
    let generated =
        policy::default_profile_policy(&NativePolicyFiles::default(), case.repo()).unwrap();
    validate_schema(&generated, &schema, &schema).unwrap();
    for missing in ["argv", "timeout_seconds"] {
        let mut incomplete = v2.clone();
        edit(&mut incomplete, &["required_checks", "focused"], |check| {
            check.remove(missing).unwrap();
        });
        assert!(validate_schema(&incomplete, &schema, &schema).is_err());
    }
    let mut legacy = fixture.clone();
    edit(&mut legacy, &["required_checks", "repo-check"], |check| {
        check.remove("argv");
        check.remove("timeout_seconds");
    });
    validate_schema(&legacy, &schema, &schema).unwrap();
    fixture.set("implementation_roots", strings(&[]));
    assert!(validate_schema(&fixture, &schema, &schema).is_err());
}

#[test]
fn test_repository_fixture_rejects_schema_missing_research_flags() {
    let case = PolicyCase::new();
    let schema = repository_schema();
    let fixture = compat::loads(&case.graph.fixture.graph.policy_bytes).unwrap();
    for flag in [
        "evidence_required",
        "decision_forbidden",
        "findings_forbidden",
    ] {
        let mut mutated = schema.clone();
        edit(
            &mut mutated,
            &["$defs", "outputContract", "properties"],
            |properties| {
                properties.remove(flag).unwrap();
            },
        );
        assert_eq!(
            validate_schema(&fixture, &mutated, &mutated),
            Err(SchemaMismatch::UnknownField)
        );
    }
}

#[test]
fn test_skill_dispatch_requirements_are_explicit() {
    let content = include_str!("../../../../skills/software-engineering-graph/SKILL.md");
    let start = "<!-- dispatch-transparency:start -->";
    let end = "<!-- dispatch-transparency:end -->";
    assert_eq!(content.matches(start).count(), 1);
    assert_eq!(content.matches(end).count(), 1);
    let (_, block) = content.split_once(start).unwrap();
    let (block, _) = block.split_once(end).unwrap();
    for phrase in [
        "Immediately before every dispatch",
        "concrete agent or task name",
        "exact approved model",
        "exact approved reasoning effort",
        "bounded scope",
        "initial dispatch",
        "fan-out member",
        "retry",
        "replacement",
        "follow-up",
        "same-role continuation",
        "Refuse the dispatch",
        "unavailable, unverifiable, or mismatched",
    ] {
        assert!(block.trim().contains(phrase), "{phrase}");
    }
}

#[test]
fn test_task_is_minimized_and_external_artifacts_are_reverified() {
    let case = PolicyCase::new();
    let mut task = case.task();
    task.set(
        "scope",
        Value::object([
            ("included", strings(&["customer-private-project-name"])),
            ("excluded", strings(&["confidential-future-acquisition"])),
        ]),
    );
    case.graph.fixture.initialize(&task, true).unwrap();
    let path = case
        .graph
        .fixture
        .graph
        .store
        .db_path("albanian-live-translate", "RUN-1");
    let database = SqliteSession::connect_existing(&path).unwrap();
    let rows = database
        .query_values("SELECT task_json FROM runs", &[])
        .unwrap();
    let stored = compat::loads_value(&rows[0]["task_json"]).unwrap();
    drop(database);
    for key in ["objective", "user_outcome", "constraints", "scope_ids"] {
        assert!(stored.get(key).is_none(), "{key}");
    }
    let bytes = fs::read(path).unwrap();
    for value in [
        b"customer-private-project-name".as_slice(),
        b"confidential-future-acquisition",
    ] {
        assert!(!bytes.windows(value.len()).any(|window| window == value));
    }
    assert!(stored["acceptance_ids"].py_eq(&strings(&["AC-001"])));
    fs::write(case.repo().join("docs/engineering-graph.md"), "changed").unwrap();
    let resumed = case.graph.run(&[
        "resume",
        "--run-id",
        "RUN-1",
        "--ack-degraded-permissions",
        "--ack-degraded-durability",
    ]);
    match resumed {
        Err(EngineError::Contract(error)) => assert_eq!(error.code, "INPUT_DIGEST_MISMATCH"),
        Err(EngineError::State(error)) => assert_eq!(error.code, "INPUT_DIGEST_MISMATCH"),
        Err(error) => panic!("unexpected error: {error:?}"),
        Ok(_) => panic!("changed evidence was accepted"),
    }
}

#[test]
fn test_nonexistent_and_digest_mismatched_artifacts_are_rejected() {
    let mut case = PolicyCase::new();
    case.graph.fixture.initialize(&case.task(), true).unwrap();
    case.graph.impact("full_delivery");
    let missing = format!("ledger:missing#sha256={}", "a".repeat(64));
    let record = |case: &PolicyCase, reference: &str, sha: &str, operation: &str| {
        case.graph.run(&[
            "record",
            "acceptance-evidence",
            "--run-id",
            "RUN-1",
            "--criterion-id",
            "AC-001",
            "--artifact-ref",
            reference,
            "--artifact-sha256",
            sha,
            "--op-id",
            operation,
        ])
    };
    contract(
        record(&case, &missing, &"a".repeat(64), "missing-evidence"),
        Some("LEDGER_ARTIFACT_NOT_FOUND"),
    );
    let artifact = case
        .graph
        .repo_artifact("acceptance_evidence", "digest-mismatch");
    contract(
        record(
            &case,
            &artifact["ref"].text().unwrap(),
            &"b".repeat(64),
            "mismatch-evidence",
        ),
        Some("DIGEST_DISAGREEMENT"),
    );
}
