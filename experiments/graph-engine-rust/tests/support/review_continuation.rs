use super::*;
use evidence_schema::assert_branch_envelope_schema;
use std::{collections::BTreeSet, fs, process::Command};

const BINDING_FIELDS: [&str; 4] = [
    "review_request_slots",
    "child_members",
    "terminal_non_successes",
    "finding_sources",
];

fn find_branch<'a>(branches: &'a Value, id: &Value) -> &'a Value {
    branches
        .as_list()
        .unwrap()
        .iter()
        .find(|branch| branch["branch_id"].py_eq(id))
        .unwrap()
}

fn visible_parent(case: &HardeningCase, command: &str, parent: &Value) -> Value {
    let visible = case.run(&[command, "--all", "--run-id", "RUN-1"]).unwrap();
    find_branch(&visible["branches"], &parent["branch_id"]).clone()
}

fn claim_parent(case: &mut HardeningCase, parent: &Value) -> Value {
    loop {
        let branch = case.claim();
        if branch["branch_id"].py_eq(&parent["branch_id"]) {
            return branch;
        }
        case.success_with(&branch, Some("APPROVE"), vec![]).unwrap();
    }
}

fn claim_child(case: &mut HardeningCase) -> Value {
    loop {
        let branch = case.claim();
        if branch["depth"].py_eq(&1_i64.into()) {
            return branch;
        }
        case.success_with(&branch, Some("APPROVE"), vec![]).unwrap();
    }
}

fn assert_dispatch_redacted(
    envelope: &Value,
    expected_capabilities: Option<&Value>,
    expected_assignment: Option<&Value>,
) {
    fn visit(value: &Value) {
        if let Some(entries) = value.as_object() {
            for (key, value) in entries {
                let key = Value::String(key.clone()).text().unwrap();
                assert!(
                    ![
                        "authority_ref",
                        "actor",
                        "host_identity",
                        "operation_id",
                        "budget",
                        "evidence_manifest_ref",
                        "result_artifact",
                        "attempts",
                        "authority",
                        "capabilities",
                    ]
                    .contains(&key.as_str()),
                    "{key}"
                );
                if key == "effect" {
                    assert!(value.is_text("filesystem_read") || value.is_text("external_read"));
                }
                visit(value);
            }
        } else if let Some(items) = value.as_list() {
            for item in items {
                visit(item);
            }
        } else if let Some(text) = value.as_string() {
            assert!(!text.starts_with("ledger:"));
        }
    }
    visit(envelope);
    for key in [
        "artifact_ref",
        "decision",
        "failure_code",
        "started_at",
        "finished_at",
    ] {
        assert!(envelope.get(key).unwrap().is_null());
    }
    assert!(envelope["evidence"].py_eq(&Value::List(vec![])));
    assert!(envelope["retry_count"].py_eq(&0_i64.into()));
    assert!(envelope["max_retries"].py_eq(&0_i64.into()));
    assert!(envelope["stopping_condition"].py_eq(&Value::object([
        ("kind", "valid_result_returned".into()),
        ("max_branch_attempts", 1_i64.into()),
    ])));
    assert_branch_envelope_schema(envelope);
    let context = &envelope["inputs"].as_list().unwrap()[0];
    assert!(context["ref"]
        .text()
        .unwrap()
        .starts_with("thread:review-context-"));
    assert!(context["content"]["kind"].is_text("review_continuation_context"));
    assert!(context["content"]["task_scope"]
        .get("capabilities")
        .is_none());
    assert!(context["content"]["preliminary"].truthy());
    assert!(context["content"]["nested_collections"].truthy());
    if let Some(expected) = expected_capabilities {
        assert!(envelope["effect_capabilities"].py_eq(expected));
    }
    if let Some(expected) = expected_assignment {
        assert!(envelope["code_review_assignment"].py_eq(expected));
    }
    let mut keys = BTreeSet::from(
        [
            "schema_version",
            "run_id",
            "branch_id",
            "node_instance_id",
            "node_key",
            "role",
            "model",
            "reasoning_effort",
            "mandatory",
            "generation",
            "status",
            "effect_capabilities",
            "output_contract",
            "stopping_condition",
            "inputs",
            "review_continuation",
            "attempt_id",
            "claim_digest",
            "lease_expires_at",
            "artifact_ref",
            "evidence",
            "decision",
            "retry_count",
            "max_retries",
            "failure_code",
            "started_at",
            "finished_at",
        ]
        .map(str::to_owned),
    );
    for key in ["claim_token", "code_review_assignment"] {
        if envelope.get(key).is_some() {
            keys.insert(key.to_owned());
        }
    }
    let actual = envelope
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, _)| Value::String(key.clone()).text().unwrap())
        .collect();
    assert_eq!(keys, actual);
}

fn write_request(
    case: &mut HardeningCase,
    parent: &Value,
    preliminary: &Value,
    request: &Value,
    operation: &str,
) -> EngineResult<Value> {
    let preliminary = case.inbox(preliminary);
    let request = case.inbox(request);
    let token = parent["claim_token"].text().unwrap();
    run(
        case,
        &request_args(
            parent,
            &preliminary,
            &request,
            &token,
            "authority:test",
            operation,
        ),
    )
}

fn assess_request(case: &mut HardeningCase, request: &Value, label: &str, operation: &str) {
    let manifest = assessment(case, request, label);
    let path = case.inbox(&manifest);
    run(case, &assessment_args(request, &path, operation)).unwrap();
}

fn finish_child(
    case: &mut HardeningCase,
    child: &Value,
    status: &str,
    findings: Vec<Value>,
    operation: &str,
) -> Value {
    let args = child_result_args(
        case,
        child,
        child_manifest(child, status, findings),
        operation,
    );
    run(case, &args).unwrap()
}

fn delegate_round(
    case: &mut HardeningCase,
    parent: &Value,
    round: i64,
    finding_id: &str,
) -> (Value, Value, Value) {
    let (preliminary, mut request) = manifests(parent, &[1]);
    request.set("round", round.into());
    let recorded = write_request(
        case,
        parent,
        &preliminary,
        &request,
        &format!("round-{round}-request"),
    )
    .unwrap();
    let assessment = format!("round-assessment-{round}");
    assess_request(
        case,
        &recorded,
        &assessment,
        &format!("round-{round}-assessment"),
    );
    let child = claim_child(case);
    let mut finding = child_finding();
    finding.set("finding_id", finding_id.into());
    finish_child(
        case,
        &child,
        "succeeded",
        vec![finding],
        &format!("round-{round}-child-result"),
    );
    (recorded, preliminary, request)
}

fn assert_continuation_preserves_focus(primary_key: &str) {
    let (mut case, parent) = to_delivery_review(primary_key, 15);
    let expected = &parent["code_review_assignment"];
    delegate_round(&mut case, &parent, 1, "REV-910");
    for command in ["ready", "next"] {
        assert_dispatch_redacted(
            &visible_parent(&case, command, &parent),
            None,
            Some(expected),
        );
    }
    let claimed = case.claim();
    assert!(claimed["branch_id"].py_eq(&parent["branch_id"]));
    assert_dispatch_redacted(&claimed, None, Some(expected));
}

#[test]
fn test_clean_code_continuations_preserve_assigned_focus() {
    assert_continuation_preserves_focus("code_reviewer");
}

#[test]
fn test_architecture_continuations_preserve_assigned_focus() {
    assert_continuation_preserves_focus("code_reviewer_architecture");
}

fn filesystem_read(path: &str) -> Value {
    Value::object([
        ("effect", "filesystem_read".into()),
        ("action", "read".into()),
        ("target_ref", path.into()),
    ])
}

fn reopen_store(case: &mut HardeningCase) {
    let store = &case.fixture.graph.store;
    case.fixture.graph.store = StateStore::at_root(
        store.state_root.clone(),
        store.platform,
        store.database,
        store.fault,
    )
    .unwrap();
}

#[test]
fn test_resumed_reviewer_dispatch_uses_explicit_read_only_allowlist() {
    let (mut case, parent) = to_delivery_review_with(
        "code_reviewer",
        15,
        |_| {},
        |task, policy| {
            let capabilities = policy["role_capabilities"]
                .as_object()
                .unwrap()
                .iter()
                .flat_map(|(_, capabilities)| capabilities.as_list().unwrap())
                .map(|item| {
                    (
                        item["effect"].text().unwrap(),
                        item["action"].text().unwrap(),
                        item["target_ref"].text().unwrap(),
                    )
                })
                .collect::<BTreeSet<_>>();
            for effect in [
                "filesystem_read",
                "filesystem_write",
                "command",
                "external_read",
            ] {
                assert!(capabilities.iter().any(|(actual, _, _)| actual == effect));
            }
            let capabilities =
                Value::list(capabilities.into_iter().map(|(effect, action, target)| {
                    Value::object([
                        ("effect", effect.into()),
                        ("action", action.into()),
                        ("target_ref", target.into()),
                    ])
                }));
            let mut authority = task["authority"].clone();
            authority.set("capabilities", capabilities);
            task.set("authority", authority);
        },
    );
    delegate_round(&mut case, &parent, 1, "REV-701");
    let expected = Value::list([filesystem_read("repo:docs/"), filesystem_read("repo:src/")]);
    for command in ["ready", "next"] {
        let dispatch = visible_parent(&case, command, &parent);
        assert_dispatch_redacted(&dispatch, Some(&expected), None);
        let context = &dispatch["inputs"].as_list().unwrap()[0]["content"];
        let assignment = &context["assignments"].as_list().unwrap()[0];
        assert!(
            assignment["effect_capabilities"].py_eq(&Value::list([filesystem_read("repo:docs/")]))
        );
    }
    reopen_store(&mut case);
    case.run(&[
        "resume",
        "--run-id",
        "RUN-1",
        "--ack-degraded-permissions",
        "--ack-degraded-durability",
    ])
    .unwrap();
    assert_dispatch_redacted(
        &visible_parent(&case, "next", &parent),
        Some(&expected),
        None,
    );
    let claimed = claim_parent(&mut case, &parent);
    assert_dispatch_redacted(&claimed, Some(&expected), None);
    for field in ["attempt_id", "claim_digest", "lease_expires_at"] {
        assert!(!claimed[field].is_null());
    }
    assert!(claimed.get("claim_token").is_some());
}

fn edit_delegation(value: &mut Value, rounds: i64, instances: i64, weight: Option<i64>) {
    let mut config = value["reviewer_delegation"].clone();
    let mut limits = config["limits"].clone();
    limits.set("max_request_rounds", rounds.into());
    config.set("limits", limits);
    let mut assignment = config["assignments"].as_list().unwrap()[0].clone();
    assignment.set("max_instances", instances.into());
    if let Some(weight) = weight {
        assignment.set("model", "gpt-6.1-sol".into());
        assignment.set("reasoning_effort", "max".into());
        assignment.set("dispatch_weight", weight.into());
    }
    config.set("assignments", Value::list([assignment]));
    value.set("reviewer_delegation", config);
}

fn parent_result(
    case: &mut HardeningCase,
    parent: &Value,
    decision: &str,
    disposition: &str,
    finding: &str,
    artifact: &str,
) -> Value {
    let mut result = Value::object([
        ("schema_version", 1_i64.into()),
        ("run_id", "RUN-1".into()),
        ("branch_id", parent["branch_id"].clone()),
        ("status", "succeeded".into()),
        ("output_kind", "delivery_review".into()),
        (
            "artifact_ref",
            case.repo_artifact("delivery_review", artifact),
        ),
        ("evidence", Value::List(vec![])),
        ("decision", decision.into()),
        (
            "findings",
            Value::list([Value::object([
                ("finding_id", finding.into()),
                ("disposition", disposition.into()),
            ])]),
        ),
    ]);
    for field in BINDING_FIELDS {
        result.set(field, parent["review_continuation"][field].clone());
    }
    result
}

fn collection_inputs<'a>(branch: &'a Value, kind: &str) -> Vec<&'a Value> {
    branch["inputs"]
        .as_list()
        .unwrap()
        .iter()
        .filter_map(|item| {
            (item["kind"].is_text("collection") && item["content"]["kind"].is_text(kind))
                .then_some(&item["content"])
        })
        .collect()
}

fn approve_remaining(case: &mut HardeningCase) {
    let ready = case.run(&["ready", "--run-id", "RUN-1"]).unwrap();
    for _ in ready["branches"].as_list().unwrap() {
        let branch = case.claim();
        case.success_with(&branch, Some("APPROVE"), vec![]).unwrap();
    }
}

#[test]
fn test_two_rounds_bind_cumulative_raw_collections_and_round_three_is_rejected() {
    let (mut case, parent) = to_delivery_review_with(
        "code_reviewer",
        15,
        |_| {},
        |task, _| {
            edit_delegation(task, 2, 2, None);
        },
    );
    let (first, _, _) = delegate_round(&mut case, &parent, 1, "REV-301");
    let resumed = claim_parent(&mut case, &parent);
    let (second, mut preliminary, mut request) = delegate_round(&mut case, &resumed, 2, "REV-301");
    let resumed = claim_parent(&mut case, &parent);
    reopen_store(&mut case);
    assert!(case.status().unwrap()["status"].is_text("active"));
    let slot = second["request_slot_id"].text().unwrap();
    case.fixture
        .with_session(|session, _| {
            let connection = session.connection();
            connection
                .execute_batch("DROP TRIGGER review_request_update_guard")
                .unwrap();
            connection
                .execute(
                    "UPDATE review_delegation_requests SET round_number=3 WHERE request_slot_id=?",
                    [&slot],
                )
                .unwrap();
            Ok(())
        })
        .unwrap();
    state_error(
        case.run(&["resume", "--run-id", "RUN-1"]),
        "DELEGATION_ROUND_LIMIT",
    );
    case.fixture
        .with_session(|session, _| {
            session
                .connection()
                .execute(
                    "UPDATE review_delegation_requests SET round_number=2 WHERE request_slot_id=?",
                    [&slot],
                )
                .unwrap();
            Ok(())
        })
        .unwrap();
    assert!(case.status().unwrap()["status"].is_text("active"));
    let continuation = &resumed["review_continuation"];
    let slots = continuation["review_request_slots"]
        .as_list()
        .unwrap()
        .iter()
        .map(|item| item.as_list().unwrap()[0].clone());
    assert!(Value::list(slots).py_eq(&Value::list([
        first["request_slot_id"].clone(),
        second["request_slot_id"].clone(),
    ])));
    assert_eq!(continuation["child_members"].as_list().unwrap().len(), 2);
    request.set("parent_attempt_id", resumed["attempt_id"].clone());
    request.set("round", 3_i64.into());
    preliminary.set("parent_attempt_id", resumed["attempt_id"].clone());
    state_error(
        write_request(
            &mut case,
            &resumed,
            &preliminary,
            &request,
            "round-3-request",
        ),
        "DELEGATION_ROUND_LIMIT",
    );
    let result = parent_result(
        &mut case,
        &resumed,
        "REVISE",
        "repair",
        "REV-301",
        "two-round-final",
    );
    case.record_as(&resumed, result, "two-round-final").unwrap();
    approve_remaining(&mut case);
    case.advance("delivery_collection");
    let supervisor = case.claim();
    let mut slots = collection_inputs(&supervisor, "review_nested_collection")
        .into_iter()
        .map(|item| item["request_slot_id"].text().unwrap())
        .collect::<Vec<_>>();
    slots.sort();
    let mut expected = [first["request_slot_id"].text().unwrap(), slot];
    expected.sort();
    assert_eq!(slots, expected);
}

fn skip_child(case: &mut HardeningCase, id: &Value, op: &str) -> Vec<String> {
    let branch = id.text().unwrap();
    let evidence = case.repo_artifact("skip", &format!("skip-evidence-{}", case.counter()));
    let path = case.inbox(&Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "skip".into()),
        ("run_id", "RUN-1".into()),
        ("reason_code", "NOT_NEEDED".into()),
        ("evidence", Value::list([evidence])),
        ("branch_id", id.clone()),
    ]));
    [
        "record",
        "skip",
        "--run-id",
        "RUN-1",
        "--branch-id",
        &branch,
        "--reason-code",
        "NOT_NEEDED",
        "--evidence-manifest",
        path.to_str().unwrap(),
        "--op-id",
        op,
    ]
    .map(str::to_owned)
    .to_vec()
}

#[test]
fn test_exact_request_cost_boundary_and_collected_skips() {
    let (mut case, parent) = to_delivery_review_with(
        "code_reviewer",
        15,
        |policy| edit_delegation(policy, 2, 3, Some(5)),
        |task, _| edit_delegation(task, 2, 3, None),
    );
    let (mut preliminary, mut request) = manifests(&parent, &[1, 2, 3]);
    let recorded = write_request(
        &mut case,
        &parent,
        &preliminary,
        &request,
        "boundary-request",
    )
    .unwrap();
    let children = recorded["child_branch_ids"].as_list().unwrap();
    assert!(recorded["dispatch_cost"].py_eq(&15_i64.into()));
    assert_eq!(children.len(), 3);
    let status = case.status().unwrap();
    assert!(children
        .iter()
        .all(|id| find_branch(&status["branches"], id)["status"].is_text("pending")));
    assess_request(
        &mut case,
        &recorded,
        "boundary-assessment",
        "boundary-assessment",
    );
    for (index, child) in children.iter().enumerate() {
        let args = skip_child(&mut case, child, &format!("boundary-skip-{index}"));
        run(&case, &args).unwrap();
    }
    let resumed = claim_parent(&mut case, &parent);
    let statuses = resumed["review_continuation"]["terminal_non_successes"]
        .as_list()
        .unwrap()
        .iter()
        .map(|item| item["status"].clone());
    assert!(Value::list(statuses).py_eq(&Value::list([
        "skipped".into(),
        "skipped".into(),
        "skipped".into()
    ])));
    preliminary.set("parent_attempt_id", resumed["attempt_id"].clone());
    request.set("parent_attempt_id", resumed["attempt_id"].clone());
    request.set("round", 2_i64.into());
    request.set(
        "members",
        Value::list([request["members"].as_list().unwrap()[0].clone()]),
    );
    let before = case.status().unwrap()["state_revision"].clone();
    state_error(
        write_request(
            &mut case,
            &resumed,
            &preliminary,
            &request,
            "boundary-over-cost",
        ),
        "DELEGATION_COST_LIMIT",
    );
    assert!(case.status().unwrap()["state_revision"].py_eq(&before));
}

#[test]
fn test_supervisor_records_dispatches_and_settles_approved_request() {
    let (mut case, parent) = to_delivery_review("code_reviewer", 15);
    let (preliminary, request) = manifests(&parent, &[1]);
    let preliminary_path = case.inbox(&preliminary);
    let request_path = case.inbox(&request);
    let token = parent["claim_token"].text().unwrap();
    let args = request_args(
        &parent,
        &preliminary_path,
        &request_path,
        &token,
        "authority:test",
        "review-fanout-1",
    );
    let recorded = run(&case, &args).unwrap();
    let slot = &recorded["request_slot_id"];
    assert!(recorded["dispatch_cost"].py_eq(&3_i64.into()));
    let status = case.status().unwrap();
    let waiting = find_branch(&status["branches"], &parent["branch_id"]);
    assert!(waiting["status"].is_text("waiting_for_review_children"));
    for field in ["attempt_id", "claim_digest", "lease_expires_at"] {
        assert!(waiting[field].is_null());
    }
    let revision = &recorded["state_revision"];
    let replay = run(&case, &args).unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(replay["state_revision"].py_eq(revision));
    let mut preliminary_changed = preliminary.clone();
    let mut finding = child_finding();
    finding.set("finding_id", "REV-099".into());
    finding.set("defect_id", "PRELIM".into());
    finding.set("summary", "preliminary".into());
    finding.set("fix_variant", "inspect".into());
    preliminary_changed.set("findings", Value::list([finding]));
    let mut request_changed = request.clone();
    let mut member = request["members"].as_list().unwrap()[0].clone();
    member.set("ordinal", 2_i64.into());
    request_changed.set("members", Value::list([member]));
    let changed_preliminary = case.inbox(&preliminary_changed);
    let changed_request = case.inbox(&request_changed);
    for (index, (prelim, request, authority)) in [
        (&preliminary_path, &request_path, "authority:test"),
        (&changed_preliminary, &request_path, "authority:test"),
        (&preliminary_path, &changed_request, "authority:test"),
        (&preliminary_path, &request_path, "authority:other"),
    ]
    .into_iter()
    .enumerate()
    {
        let args = request_args(
            &parent,
            prelim,
            request,
            &token,
            authority,
            &format!("review-fanout-fresh-{index}"),
        );
        state_error(run(&case, &args), "DELEGATION_SLOT_CONFLICT");
    }
    assert!(case.status().unwrap()["state_revision"].py_eq(revision));
    assess_request(
        &mut case,
        &recorded,
        "review-assessment",
        "review-assessment-1",
    );
    let mut child = case.claim();
    if !child["depth"].py_eq(&1_i64.into()) {
        case.success_with(&child, Some("APPROVE"), vec![]).unwrap();
        child = case.claim();
    }
    assert!(child["parent_branch_id"].py_eq(&parent["branch_id"]));
    assert!(child["depth"].py_eq(&1_i64.into()));
    assert!(child["effect_capabilities"].py_eq(&Value::list([filesystem_read("repo:docs/")])));
    assert!(child["inputs"].py_eq(&Value::List(vec![])));
    let assignment = String::from_utf8(
        compat::dumps(
            &child["review_assignment"],
            compat::SerializationProfile::CLI,
        )
        .unwrap(),
    )
    .unwrap();
    for forbidden in [
        "repo:docs/artifacts",
        "ledger:",
        "authority_ref",
        "actor",
        "host_identity",
        "budget",
        "operation",
    ] {
        assert!(!assignment.contains(forbidden));
    }
    let finding = child_finding();
    let modified = |key: &str, value: Value| {
        let mut changed = finding.clone();
        changed.set(key, value);
        vec![changed]
    };
    for (index, invalid) in [
        modified("acceptance_id", "AC-999".into()),
        modified("evidence_ids", Value::list(["E-OTHER".into()])),
        modified("location", "src/Cache.cs".into()),
        vec![finding.clone(), finding.clone()],
        modified("finding_id", "invalid".into()),
    ]
    .into_iter()
    .enumerate()
    {
        let before = case.status().unwrap()["state_revision"].clone();
        let args = child_result_args(
            &mut case,
            &child,
            child_manifest(&child, "succeeded", invalid),
            &format!("bad-result-{index}"),
        );
        assert!(matches!(run(&case, &args), Err(EngineError::Contract(_))));
        assert!(case.status().unwrap()["state_revision"].py_eq(&before));
    }
    finish_child(
        &mut case,
        &child,
        "succeeded",
        vec![finding],
        "valid-child-result",
    );
    assert_dispatch_redacted(&visible_parent(&case, "next", &parent), None, None);
    let resumed = case.claim();
    assert!(resumed["branch_id"].py_eq(&parent["branch_id"]));
    assert_dispatch_redacted(&resumed, None, None);
    let binding = &resumed["review_continuation"];
    let serialized =
        String::from_utf8(compat::dumps(binding, compat::SerializationProfile::CLI).unwrap())
            .unwrap();
    for forbidden in [
        "operation_id",
        "authority_ref",
        "host_identity",
        "actor",
        "ledger_path",
        "claim_token",
    ] {
        assert!(!serialized.contains(forbidden));
    }
    let context = &resumed["inputs"].as_list().unwrap()[0]["content"];
    let collection = context["nested_collections"]
        .as_list()
        .unwrap()
        .last()
        .unwrap();
    let mut sources = Vec::new();
    for issue in collection["issues"].as_list().unwrap() {
        for origin in issue["provenance"].as_list().unwrap() {
            sources.push(Value::list([
                origin["request_slot_id"].clone(),
                issue["issue_key"].clone(),
                origin["branch_id"].clone(),
                origin["finding_id"].clone(),
                origin["assignment_id"].clone(),
                origin["ordinal"].clone(),
            ]));
        }
    }
    graph_engine::domain::python_operations::sort_comparable_values(&mut sources).unwrap();
    let mut result = parent_result(
        &mut case,
        &resumed,
        "REVISE",
        "repair",
        "REV-101",
        "parent-final",
    );
    result.set("terminal_non_successes", Value::List(vec![]));
    result.set("finding_sources", Value::List(sources));
    case.record_as(&resumed, result, "parent-final").unwrap();
    let status = case.status().unwrap();
    let sealed = &status["reviewer_delegations"].as_list().unwrap()[0];
    assert!(sealed["status"].is_text("sealed"));
    assert!(sealed["dispatch_cost"].py_eq(&3_i64.into()));
    if status["branches"]
        .as_list()
        .unwrap()
        .iter()
        .any(|item| item["stage"].is_text("delivery") && item["status"].is_text("ready"))
    {
        let branch = case.claim();
        case.success_with(&branch, Some("APPROVE"), vec![]).unwrap();
    }
    case.advance("delivery_collection");
    let supervisor = case.claim();
    let outer = collection_inputs(&supervisor, "collection")[0];
    let parent_member = find_branch(&outer["members"], &parent["branch_id"]);
    assert!(parent_member["result"]["review_request_slots"].py_eq(&binding["review_request_slots"]));
    let slots = collection_inputs(&supervisor, "review_nested_collection")
        .into_iter()
        .map(|item| item["request_slot_id"].clone());
    assert!(Value::list(slots).py_eq(&Value::list([slot.clone()])));
}

fn start_mixed_round(
    case: &mut HardeningCase,
    parent: &Value,
    round: i64,
    ordinals: &[i64],
) -> Value {
    let (preliminary, mut request) = manifests(parent, ordinals);
    request.set("round", round.into());
    let recorded = write_request(
        case,
        parent,
        &preliminary,
        &request,
        &format!("mixed-request-{round}"),
    )
    .unwrap();
    let label = format!("mixed-assessment-{round}");
    assess_request(case, &recorded, &label, &label);
    recorded
}

fn timeout_child(case: &mut HardeningCase, child: &Value, op: &str) {
    let args = terminal_control_args(case, child, "timeout", op);
    run(case, &args).unwrap();
}

#[test]
fn test_mixed_terminal_raw_results_survive_restart_and_reach_supervisor() {
    let (mut case, parent) = to_delivery_review_with(
        "code_reviewer",
        15,
        |policy| edit_delegation(policy, 2, 3, None),
        |task, _| edit_delegation(task, 2, 3, None),
    );
    start_mixed_round(&mut case, &parent, 1, &[1, 2, 3]);
    let succeeded = claim_child(&mut case);
    let mut finding = child_finding();
    finding.set("finding_id", "REV-501".into());
    finding.set("summary", "race".into());
    finding.set("fix_variant", "serialize".into());
    finish_child(
        &mut case,
        &succeeded,
        "succeeded",
        vec![finding],
        "mixed-success",
    );
    let failed = claim_child(&mut case);
    finish_child(&mut case, &failed, "failed", vec![], "mixed-failed-1");
    let timed_out = claim_child(&mut case);
    timeout_child(&mut case, &timed_out, "mixed-timeout-1");
    for (branch, label) in [(&failed, "failure"), (&timed_out, "timeout")] {
        case.run(&[
            "record",
            "retry",
            "--run-id",
            "RUN-1",
            "--branch-id",
            &branch["branch_id"].text().unwrap(),
            "--reason-code",
            "RETRY_REVIEW",
            "--op-id",
            &format!("mixed-retry-{label}"),
        ])
        .unwrap();
    }
    for _ in 0..2 {
        let retried = claim_child(&mut case);
        if retried["branch_id"].py_eq(&failed["branch_id"]) {
            finish_child(&mut case, &retried, "failed", vec![], "mixed-failed-2");
        } else {
            timeout_child(&mut case, &retried, "mixed-timeout-2");
        }
    }
    let resumed = claim_parent(&mut case, &parent);
    let second = start_mixed_round(&mut case, &resumed, 2, &[1]);
    let skipped_id = &second["child_branch_ids"].as_list().unwrap()[0];
    let args = skip_child(&mut case, skipped_id, "mixed-skip");
    run(&case, &args).unwrap();
    assert_dispatch_redacted(&visible_parent(&case, "next", &parent), None, None);
    reopen_store(&mut case);
    let resumed = claim_parent(&mut case, &parent);
    assert_dispatch_redacted(&resumed, None, None);
    let result = parent_result(
        &mut case,
        &resumed,
        "REVISE",
        "repair",
        "REV-501",
        "mixed-parent-final",
    );
    case.record_as(&resumed, result, "mixed-parent-final")
        .unwrap();
    let status = case.status().unwrap();
    for _ in status["branches"]
        .as_list()
        .unwrap()
        .iter()
        .filter(|branch| branch["stage"].is_text("delivery") && branch["status"].is_text("ready"))
    {
        let branch = case.claim();
        case.success_with(&branch, Some("APPROVE"), vec![]).unwrap();
    }
    case.advance("delivery_collection");
    let supervisor = case.claim();
    let nested = collection_inputs(&supervisor, "review_nested_collection");
    assert_eq!(nested.len(), 2);
    let members = nested
        .iter()
        .flat_map(|collection| collection["members"].as_list().unwrap());
    let by_status = members
        .clone()
        .map(|item| (item["status"].text().unwrap(), item))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        by_status
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["succeeded", "failed", "timed_out", "skipped"])
    );
    assert!(by_status["succeeded"]["terminal"]["result"]["status"].is_text("succeeded"));
    assert!(by_status["failed"]["terminal"]["result"]["failure_code"].is_text("REVIEW_FAILED"));
    assert!(by_status["timed_out"]["terminal"]["result"]["kind"].is_text("timeout"));
    assert!(by_status["skipped"]["terminal"]["result"]["kind"].is_text("skip"));
    for status in ["failed", "timed_out"] {
        assert_eq!(
            by_status[status]["terminal"]["attempts"]
                .as_list()
                .unwrap()
                .len(),
            2
        );
    }
    assert!(by_status["skipped"]["terminal"]["attempts"].py_eq(&Value::List(vec![])));
    for member in members {
        let terminal = &member["terminal"];
        assert!(terminal["result_artifact"]["ref"]
            .text()
            .unwrap()
            .starts_with("ledger:"));
        assert!(terminal["result_artifact"]["sha256"].py_eq(&terminal["result_digest"]));
    }
}

fn supervisor_manifest(disposition: &str, outcome: &str) -> (HardeningCase, Value, Value) {
    let (mut case, parent) = to_delivery_review("code_reviewer", 15);
    delegate_round(&mut case, &parent, 1, "REV-801");
    let resumed = claim_parent(&mut case, &parent);
    let result = parent_result(
        &mut case,
        &resumed,
        "BLOCK",
        "block",
        "REV-801",
        "supervisor-parent-final",
    );
    case.record_as(&resumed, result, "supervisor-parent-final")
        .unwrap();
    approve_remaining(&mut case);
    case.advance("delivery_collection");
    let supervisor = case.claim();
    assert!(supervisor["node_key"].is_text("supervisor_delivery_consolidation"));
    let outer = collection_inputs(&supervisor, "collection")[0];
    let nested = collection_inputs(&supervisor, "review_nested_collection")[0];
    let issue = &nested["issues"].as_list().unwrap()[0];
    let mut sources = issue["provenance"]
        .as_list()
        .unwrap()
        .iter()
        .map(|source| {
            Value::list([
                nested["request_slot_id"].clone(),
                source["branch_id"].clone(),
                source["finding_id"].clone(),
            ])
        })
        .collect::<Vec<_>>();
    graph_engine::domain::python_operations::sort_comparable_values(&mut sources).unwrap();
    let manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "delivery_consolidation".into()),
        ("run_id", "RUN-1".into()),
        ("join_id", outer["join_id"].clone()),
        ("generation", 0_i64.into()),
        (
            "source_branch_ids",
            Value::list(
                outer["members"]
                    .as_list()
                    .unwrap()
                    .iter()
                    .map(|item| item["branch_id"].clone()),
            ),
        ),
        (
            "finding_dispositions",
            Value::list([Value::object([
                ("finding_id", "REV-801".into()),
                ("disposition", "block".into()),
            ])]),
        ),
        (
            "delegated_finding_dispositions",
            Value::list([Value::object([
                ("issue_key", issue["issue_key"].clone()),
                ("source_identities", Value::List(sources)),
                ("disposition", disposition.into()),
            ])]),
        ),
        ("outcome", outcome.into()),
    ]);
    (case, supervisor, manifest)
}

fn supervisor_outcome(disposition: &str, outcome: &str) -> (HardeningCase, Value) {
    let (mut case, supervisor, manifest) = supervisor_manifest(disposition, outcome);
    case.record_as(&supervisor, manifest, "delegated-consolidation")
        .unwrap();
    let advanced = case.advance("delivery_consolidation");
    assert!(advanced["outcome"].is_text(outcome));
    (case, advanced)
}

#[test]
fn test_supervisor_delegated_accept_controls_outcome() {
    let (case, _) = supervisor_outcome("accept", "ACCEPT");
    assert!(case.status().unwrap()["joins"]
        .as_list()
        .unwrap()
        .iter()
        .any(|item| item["join_key"].is_text("closure")));
}

#[test]
fn test_supervisor_delegated_repair_controls_outcome() {
    let (_, advanced) = supervisor_outcome("repair", "REPAIR");
    assert!(advanced["successor_branch_ids"].truthy());
}

#[test]
fn test_supervisor_delegated_redesign_controls_outcome() {
    let (_, advanced) = supervisor_outcome("redesign", "REDESIGN");
    assert!(advanced["successor_branch_ids"].truthy());
}

#[test]
fn test_supervisor_delegated_block_controls_outcome() {
    let (case, _) = supervisor_outcome("block", "BLOCK");
    assert!(case.status().unwrap()["status"].is_text("blocked"));
}

#[test]
fn test_supervisor_delegated_outcome_contradiction_is_rejected() {
    let (mut case, supervisor, manifest) = supervisor_manifest("accept", "BLOCK");
    case.record_as(&supervisor, manifest, "delegated-consolidation")
        .unwrap();
    let join = case.open_join("delivery_consolidation");
    contract_error(
        case.run(&[
            "join",
            "advance",
            "--run-id",
            "RUN-1",
            "--join-id",
            &join["join_id"].text().unwrap(),
            "--op-id",
            "contradictory-delivery",
        ]),
        "OUTCOME_PRECEDENCE_MISMATCH",
    );
}

#[test]
fn terminal_collection_and_parent_continuation_commit_atomically_in_format7() {
    let (mut case, parent) = format7_parent();
    let recorded = record_request(&mut case, &parent, &[1], "one-child");
    assess_request(&mut case, &recorded, "one-child", "assess-one-child");
    let child = claim_child(&mut case);
    let args = child_result_args(
        &mut case,
        &child,
        child_manifest(&child, "succeeded", vec![child_finding()]),
        "settle-only-child",
    );
    assert_rollback(&case, &args);
    assert!(
        read_child(&case, &parent["branch_id"])["status"].is_text("waiting_for_review_children")
    );
    let settled = run(&case, &args).unwrap();
    assert!(settled["branch_status"].is_text("succeeded"));
    let after_settle = ledger_snapshot(&case);
    let replay = run(&case, &args).unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(replay["state_revision"].py_eq(&settled["state_revision"]));
    assert_eq!(ledger_snapshot(&case), after_settle);
    let covered = case.fixture.graph.repository.join("docs/covered.txt");
    let original = fs::read(&covered).unwrap();
    fs::write(&covered, b"changed while the child was settling\n").unwrap();
    state_error(case.claim_raw(), "REVIEW_SOURCE_CHANGED");
    assert_eq!(ledger_snapshot(&case), after_settle);
    fs::write(&covered, original).unwrap();
    let resumed = claim_parent(&mut case, &parent);
    assert!(!resumed["attempt_id"].py_eq(&parent["attempt_id"]));
    let result = parent_result(
        &mut case,
        &resumed,
        "REVISE",
        "repair",
        "REV-101",
        "fresh-parent",
    );
    let before = ledger_snapshot(&case);
    let old = child_result_args(&mut case, &parent, result.clone(), "old-parent-result");
    state_error(run(&case, &old), "ATTEMPT_FENCE_MISMATCH");
    assert_eq!(ledger_snapshot(&case), before);
    assert_result_diagnostics(&mut case, &resumed, &result);
    let fresh = child_result_args(&mut case, &resumed, result, "fresh-parent-result");
    assert!(run(&case, &fresh).unwrap()["branch_status"].is_text("succeeded"));
}

fn assert_result_diagnostics(case: &mut HardeningCase, parent: &Value, result: &Value) {
    let before = ledger_snapshot(case);
    let mut malformed = result.clone();
    for field in BINDING_FIELDS {
        malformed.set(field, Value::Null);
    }
    malformed.remove("finding_sources");
    let op = "fresh-parent-result";
    let missing = child_result_args(case, parent, malformed.clone(), op);
    assert_continuation_contract(case, &missing, "result", "DELEGATION_BINDING_INCOMPLETE");

    let graph = &case.fixture.graph;
    let external = Command::new(env!("CARGO_BIN_EXE_graphctl"))
        .arg("--repo")
        .arg(&graph.repository)
        .arg("--state-root")
        .arg(&graph.runtime_root)
        .args(&missing)
        .current_dir(&graph.repository)
        .env("PYTHONIOENCODING", "utf-8:strict")
        .output()
        .unwrap();
    assert_eq!(external.status.code(), Some(4), "{external:?}");
    assert!(external.stderr.is_empty(), "{external:?}");
    let mut expected = br#"{"code":"DELEGATION_BINDING_INCOMPLETE","field":"result","ok":false,"run_id":null,"schema_version":1,"state_revision":null}"#.to_vec();
    expected.extend_from_slice(if cfg!(windows) { b"\r\n" } else { b"\n" });
    assert_eq!(external.stdout, expected);
    assert_eq!(ledger_snapshot(case), before);

    malformed.set("finding_sources", Value::Null);
    for (field, code) in [
        ("review_request_slots", "DELEGATION_COLLECTION_MISMATCH"),
        ("child_members", "DELEGATION_MEMBER_MISMATCH"),
        ("terminal_non_successes", "DELEGATION_TERMINAL_MISMATCH"),
        ("finding_sources", "DELEGATION_FINDING_SOURCE_MISMATCH"),
    ] {
        let args = child_result_args(case, parent, malformed.clone(), op);
        assert_continuation_contract(case, &args, &format!("result.{field}"), code);
        assert_eq!(ledger_snapshot(case), before);
        malformed.set(field, result[field].clone());
    }
    malformed.set("findings", Value::list([]));
    let args = child_result_args(case, parent, malformed.clone(), op);
    assert_continuation_contract(
        case,
        &args,
        "result.findings",
        "DELEGATION_DISPOSITION_MISSING",
    );

    malformed.set(
        "findings",
        Value::list([Value::object([("finding_id", Value::list([]))])]),
    );
    let args = child_result_args(case, parent, malformed, op);
    let error = run(case, &args).unwrap_err();
    assert!(
        matches!(error, EngineError::Python { kind: "TypeError", ref message }
            if message.equals("unhashable type: 'list'")),
        "{error:?}"
    );

    let mut args = child_result_args(case, parent, result.clone(), op);
    let manifest_index = args
        .iter()
        .position(|arg| arg == "--result-manifest")
        .unwrap()
        + 1;
    for (payload, expected_kind, expected_message) in [
        (
            Value::Null,
            "TypeError",
            "'NoneType' object is not iterable",
        ),
        (
            Value::list([]),
            "AttributeError",
            "'list' object has no attribute 'get'",
        ),
    ] {
        args[manifest_index] = case.inbox(&payload).to_str().unwrap().to_owned();
        let error = run(case, &args).unwrap_err();
        assert!(
            matches!(error, EngineError::Python { kind, ref message }
                if kind == expected_kind && message.equals(expected_message)),
            "{error:?}"
        );
    }
    assert_eq!(ledger_snapshot(case), before);
}

fn assert_continuation_contract(case: &HardeningCase, args: &[String], field: &str, code: &str) {
    let error = run(case, args).unwrap_err();
    assert!(
        matches!(error, EngineError::Contract(ref error)
            if error.field.equals(field) && error.code == code),
        "{error:?}"
    );
}

fn format7_parent() -> (HardeningCase, Value) {
    let mut case = HardeningCase::stock();
    case.edit_policy(|policy| policy.set("reviewer_delegation", delegation_config(true, 15)));
    let mut task = case.fixture.graph.task("delivery", "fast_path", &[]);
    task.set("reviewer_delegation", delegation_config(false, 15));
    case.fixture.initialize(&task, true).unwrap();
    case.impact("fast_path");
    let writer = case.claim();
    case.success_with(&writer, Some("IMPLEMENTED"), vec![])
        .unwrap();
    let graph = &case.fixture.graph;
    let template = graph.root.join("git-template-empty");
    let config = graph.root.join("git-config-empty");
    fs::write(
        graph.repository.join("docs/covered.txt"),
        b"covered fixture input\n",
    )
    .unwrap();
    fs::create_dir(&template).unwrap();
    fs::write(&config, b"").unwrap();
    for args in [
        &["init"][..],
        &["add", "docs/engineering-graph.md", "docs/covered.txt"],
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "fixture",
        ],
    ] {
        let output = Command::new("git")
            .args(args)
            .current_dir(&graph.repository)
            .env("GIT_CONFIG_GLOBAL", &config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TEMPLATE_DIR", &template)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    let info = graph.repository.join(".git/info");
    fs::create_dir_all(&info).unwrap();
    fs::write(info.join("exclude"), b"/docs/artifacts/\n").unwrap();
    enable_child_format7(&mut case, "docs/covered.txt");
    case.advance("implementation");
    loop {
        let reviewer = case.claim();
        if reviewer["node_key"].is_text("code_reviewer") {
            return (case, reviewer);
        }
        case.success_with(&reviewer, Some("APPROVE"), vec![])
            .unwrap();
    }
}

#[test]
fn test_usage_counts_parent_child_and_resumed_attempt_once() {
    let (mut case, parent) = to_delivery_review("code_reviewer", 15);
    let (delegated, _, _) = delegate_round(&mut case, &parent, 1, "REV-123");
    let resumed = claim_parent(&mut case, &parent);
    let child_id = &delegated["child_branch_ids"].as_list().unwrap()[0];
    let child = case
        .fixture
        .with_session(|session, _| {
            let attempt = session.all(
                GraphRead::ReviewSettlementAttempts,
                std::slice::from_ref(child_id),
            )?;
            Ok(Value::object([
                ("branch_id", child_id.clone()),
                ("attempt_id", attempt[0]["attempt_id"].clone()),
            ]))
        })
        .unwrap();
    let mut usage = Value::Null;
    for (index, branch) in [&parent, &child, &resumed].into_iter().enumerate() {
        let identifier = format!("usage-session-{index}");
        let log = case.fixture.graph.root.join(format!("{identifier}.jsonl"));
        let counters = Value::object([
            ("input_tokens", 10_i64.into()),
            ("output_tokens", 1_i64.into()),
            ("total_tokens", 11_i64.into()),
        ]);
        let mut bytes = Vec::new();
        for (kind, payload) in [
            (
                "session_meta",
                Value::object([("id", identifier.as_str().into())]),
            ),
            (
                "turn_context",
                Value::object([("model", "gpt-6-astra".into()), ("effort", "medium".into())]),
            ),
            (
                "event_msg",
                Value::object([
                    ("type", "token_count".into()),
                    (
                        "info",
                        Value::object([
                            ("total_token_usage", counters.clone()),
                            ("last_token_usage", counters),
                        ]),
                    ),
                ]),
            ),
        ] {
            bytes.extend(
                compat::dumps(
                    &Value::object([("type", kind.into()), ("payload", payload)]),
                    compat::SerializationProfile::STORED,
                )
                .unwrap(),
            );
            bytes.extend(if cfg!(windows) {
                b"\r\n".as_slice()
            } else {
                b"\n".as_slice()
            });
        }
        fs::write(&log, bytes).unwrap();
        let recorded = case
            .run(&[
                "record",
                "usage",
                "--run-id",
                "RUN-1",
                "--action",
                "bind",
                "--session-log",
                log.to_str().unwrap(),
                "--branch-id",
                &branch["branch_id"].text().unwrap(),
                "--attempt-id",
                &branch["attempt_id"].text().unwrap(),
                "--start-offset",
                "0",
                "--source-id",
                &ids::sha256_bytes(identifier.as_bytes()),
                "--prefix-sha256",
                &ids::sha256_bytes(b""),
                "--op-id",
                &format!("usage-bind-{index}"),
            ])
            .unwrap();
        usage = case
            .run(&[
                "record",
                "usage",
                "--run-id",
                "RUN-1",
                "--action",
                "close",
                "--session-log",
                log.to_str().unwrap(),
                "--binding-id",
                &recorded["binding_id"].text().unwrap(),
                "--op-id",
                &format!("usage-close-{index}"),
            ])
            .unwrap()["usage"]
            .clone();
    }
    let total = |scope: &Value, expected: i64| {
        assert!(scope["observed_totals"]["total_tokens"].py_eq(&expected.into()));
    };
    total(&usage, 33);
    total(&usage["agents"][&parent["branch_id"].text().unwrap()], 22);
    total(&usage["agents"][&child_id.text().unwrap()], 11);
    assert_eq!(usage["attempts"].as_object().unwrap().len(), 3);
    total(&usage["phases"]["review_testing"], 33);
}
