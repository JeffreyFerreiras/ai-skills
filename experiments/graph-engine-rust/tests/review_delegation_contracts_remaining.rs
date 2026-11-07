#[allow(dead_code)]
#[path = "support/evidence_schema.rs"]
mod evidence_schema;
#[path = "support/review_finding_contract_cases.rs"]
mod review_finding_contract_cases;

use evidence_schema::validate_schema;
use graph_engine::{
    application::error::EngineError,
    compat::{self, Value},
    domain::{
        hosts::{dispatch_weight_for, supported_dispatch_weights},
        review_contracts::validate_preliminary,
        review_request::{validate_fanout_request, FanoutRequestContext},
        reviewer_delegation::{validate_policy_config, validate_task_config},
    },
};

fn json(source: &str) -> Value {
    compat::loads(source.as_bytes()).unwrap()
}

fn list(values: &[&str]) -> Value {
    Value::list(values.iter().copied().map(Value::from))
}

fn changed(base: &Value, field: &str, value: Value) -> Value {
    let mut changed = base.clone();
    changed.set(field, value);
    changed
}

fn first_changed(base: &Value, key: &str, update: impl FnOnce(&mut Value)) -> Value {
    let mut list = base[key].as_list().unwrap().to_vec();
    update(&mut list[0]);
    changed(base, key, Value::List(list))
}

fn contract<T, E: Into<EngineError>>(result: Result<T, E>, expected: Option<&str>) {
    match result.map_err(Into::into) {
        Err(EngineError::Contract(error)) => {
            assert!(expected.is_none_or(|code| error.code == code), "{error}");
        }
        Err(error) => panic!("expected contract error {expected:?}, got {error:?}"),
        Ok(_) => panic!("expected contract error {expected:?}, got success"),
    }
}

fn repository_schema() -> Value {
    compat::loads(include_bytes!(
        "../../../skills/software-engineering-graph/references/repository-config.schema.json"
    ))
    .unwrap()
}

fn policy_config() -> Value {
    json(
        r#"{
      "limits": {
        "max_depth":1, "max_children_per_request":3, "max_children_per_run":6,
        "max_request_rounds":2, "max_weighted_dispatch_cost":15
      },
      "assignments":[{
        "assignment_id":"correctness", "role":"code_reviewer", "model":"gpt-6.1-sol",
        "reasoning_effort":"high", "review_lens":"Cross-file correctness and concurrency",
        "prompt_template":"Review only the approved evidence and scope.",
        "allowed_reason_codes":["CROSS_FILE"], "allowed_evidence_kinds":["finding","implementation_handoff"],
        "scope_refs":["repo:docs/"], "max_instances":2, "dispatch_weight":3
      }]
    }"#,
    )
}

fn task_config() -> Value {
    json(
        r#"{
      "limits": {
        "max_depth":1, "max_children_per_request":3, "max_children_per_run":6,
        "max_request_rounds":1, "max_weighted_dispatch_cost":15
      },
      "assignments":[{
        "assignment_id":"correctness", "allowed_reason_codes":["CROSS_FILE"],
        "allowed_acceptance_ids":["AC-001"], "allowed_evidence_kinds":["finding","implementation_handoff"],
        "scope_refs":["repo:docs/"], "max_instances":2
      }]
    }"#,
    )
}

fn assignment(config: &Value) -> &Value {
    &config["assignments"].as_list().unwrap()[0]
}

fn with_model(config: &Value, model: &str, effort: &str, weight: i64) -> Value {
    first_changed(config, "assignments", |assignment| {
        assignment.set("model", model.into());
        assignment.set("reasoning_effort", effort.into());
        assignment.set("dispatch_weight", weight.into());
    })
}

fn preliminary() -> Value {
    Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "review_preliminary".into()),
        ("run_id", "RUN-1".into()),
        ("parent_branch_id", "parent".into()),
        ("parent_attempt_id", "attempt".into()),
        ("generation", 0_i64.into()),
        ("findings", list(&[])),
        (
            "evidence",
            Value::list([Value::object([
                ("evidence_id", "E-1".into()),
                ("kind", "finding".into()),
                ("sha256", "a".repeat(64).into()),
            ])]),
        ),
    ])
}

fn approved(reference: &str, digest: &str) -> Value {
    Value::object([
        ("kind", "finding".into()),
        ("ref", format!("{reference}#sha256={digest}").into()),
        ("sha256", digest.into()),
    ])
}

fn check_preliminary(
    value: &Value,
    evidence: Value,
    resolved: bool,
) -> graph_engine::domain::errors::ValidationResult<Value> {
    validate_preliminary(
        value,
        &"RUN-1".into(),
        &"parent".into(),
        &"attempt".into(),
        &0_i64.into(),
        Some(&Value::list([evidence])),
        resolved,
    )
}

fn contracts() -> (Value, Value, Value) {
    let policy = validate_policy_config(&policy_config(), false)
        .unwrap()
        .unwrap();
    let task = validate_task_config(&task_config(), Some(&policy), &["AC-001".into()])
        .unwrap()
        .unwrap();
    let preliminary = check_preliminary(
        &preliminary(),
        approved("repo:docs/evidence.json", &"a".repeat(64)),
        false,
    )
    .unwrap();
    let request = json(
        r#"{
      "schema_version":1, "kind":"review_fanout_request", "run_id":"RUN-1",
      "parent_branch_id":"parent", "parent_attempt_id":"attempt", "round":1,
      "members":[{
        "assignment_id":"correctness", "ordinal":1, "reason_code":"CROSS_FILE",
        "acceptance_ids":["AC-001"], "evidence_ids":["E-1"]
      }]
    }"#,
    );
    (task, preliminary, request)
}

fn check_request(
    request: &Value,
    task: &Value,
    preliminary: &Value,
    depth: i64,
    historical: bool,
) -> graph_engine::domain::errors::ValidationResult<Value> {
    validate_fanout_request(
        request,
        &FanoutRequestContext {
            run_id: &"RUN-1".into(),
            parent_branch_id: &"parent".into(),
            parent_attempt_id: &"attempt".into(),
            round_number: &1_i64.into(),
            plan_assignments: &task["assignments"],
            preliminary,
            limits: &task["limits"],
            depth: &depth.into(),
            historical,
        },
    )
}

#[test]
fn test_retired_review_pairs_are_auditable_but_not_new_selections_or_requests() {
    let schema = repository_schema();
    let assignment_schema = &schema["$defs"]["reviewerDelegationAssignment"];
    for (model, effort, weight) in [
        ("gpt-5.6-sol", "high", 3),
        ("gpt-5.6-sol", "xhigh", 4),
        ("gpt-5.6-sol", "max", 5),
        ("gpt-5.6-luna", "max", 3),
    ] {
        let config = with_model(&policy_config(), model, effort, weight);
        assert!(validate_schema(assignment(&config), assignment_schema, &schema).is_err());
        contract(
            validate_policy_config(&config, false),
            Some("DELEGATION_ASSIGNMENT_UNSUPPORTED"),
        );
        assert_eq!(dispatch_weight_for(model, effort, false), None);
        assert!(!supported_dispatch_weights().contains_key(&(model.to_owned(), effort.to_owned())));
        let historical = validate_policy_config(&config, true).unwrap().unwrap();
        assert!(assignment(&historical)["model"].is_text(model));
        let (task, preliminary, request) = contracts();
        let task = with_model(&task, model, effort, weight);
        contract(
            check_request(&request, &task, &preliminary, 0, false),
            Some("MODEL_RETIRED"),
        );
        assert!(check_request(&request, &task, &preliminary, 0, true)
            .unwrap()
            .py_eq(&request));
    }
}

fn assert_model_weights(schema: &Value, model: &str, variants: &[(&str, i64)]) {
    let assignment_schema = &schema["$defs"]["reviewerDelegationAssignment"];
    for &(effort, weight) in variants {
        for candidate in [3, 4, 5] {
            let config = with_model(&policy_config(), model, effort, candidate);
            let shape = validate_schema(assignment(&config), assignment_schema, schema);
            let validated = validate_policy_config(&config, false);
            if candidate == weight {
                shape.unwrap();
                assert!(assignment(&validated.unwrap().unwrap())["model"].is_text(model));
            } else {
                assert!(shape.is_err());
                contract(validated, None);
            }
        }
    }
}

#[test]
fn test_astra_delegation_schema_and_runtime_agree_on_effort_weights() {
    assert_model_weights(
        &repository_schema(),
        "gpt-6-astra",
        &[("medium", 3), ("high", 3), ("xhigh", 4), ("max", 5)],
    );
}

#[test]
fn test_low_astra_and_medium_sol_delegation_remain_unsupported() {
    let schema = repository_schema();
    let assignment_schema = &schema["$defs"]["reviewerDelegationAssignment"];
    for (model, effort) in [
        ("gpt-6-astra", "low"),
        ("gpt-5.6-sol", "medium"),
        ("gpt-6.1-sol", "low"),
        ("gpt-6.1-sol", "medium"),
    ] {
        let config = with_model(&policy_config(), model, effort, 3);
        assert!(validate_schema(assignment(&config), assignment_schema, &schema).is_err());
        contract(validate_policy_config(&config, false), None);
    }
}

#[test]
fn test_current_delegation_models_schema_and_runtime_agree_on_effort_weights() {
    let schema = repository_schema();
    for model in ["gpt-6.1-sol", "claude-opus-5-5"] {
        assert_model_weights(&schema, model, &[("high", 3), ("xhigh", 4), ("max", 5)]);
    }
}

#[test]
fn test_request_contract_is_exhaustive_and_assignment_bounded() {
    let (task, preliminary, request) = contracts();
    let normalized = check_request(&request, &task, &preliminary, 0, false).unwrap();
    assert!(normalized["members"].as_list().unwrap()[0]["assignment_id"].is_text("correctness"));
    for (field, value) in [
        ("role", "code_reviewer"),
        ("model", "gpt-5.6-sol"),
        ("reasoning_effort", "high"),
        ("capability", "command"),
        ("scope", "repo:other/"),
        ("prompt", "override"),
        ("operation_id", "opaque"),
        ("path", "state.sqlite3"),
        ("authority_ref", "authority:test"),
    ] {
        let changed = first_changed(&request, "members", |member| {
            member.set(field, value.into());
        });
        contract(
            check_request(&changed, &task, &preliminary, 0, false),
            Some("UNKNOWN_FIELD"),
        );
    }
}

#[test]
fn test_limits_and_undeclared_values_fail_closed() {
    let (task, preliminary, request) = contracts();
    for (field, value) in [
        ("assignment_id", "undeclared".into()),
        ("reason_code", "OTHER".into()),
        ("acceptance_ids", list(&["AC-999"])),
        ("evidence_ids", list(&["E-999"])),
    ] {
        let changed = first_changed(&request, "members", |member| {
            member.set(field, value);
        });
        contract(check_request(&changed, &task, &preliminary, 0, false), None);
    }
    let member = &request["members"].as_list().unwrap()[0];
    let expanded = changed(&request, "members", Value::List(vec![member.clone(); 4]));
    contract(
        check_request(&expanded, &task, &preliminary, 0, false),
        None,
    );
    contract(
        check_request(&request, &task, &preliminary, 1, false),
        Some("DELEGATION_DEPTH_EXCEEDED"),
    );
    let unsupported = with_model(&policy_config(), "gpt-5.6-luna", "high", 2);
    contract(
        validate_policy_config(&unsupported, false),
        Some("DELEGATION_ASSIGNMENT_UNSUPPORTED"),
    );
}
