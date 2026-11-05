#[path = "support/hardening_case.rs"]
mod hardening_case;

use graph_engine::{
    application::{
        error::EngineError,
        graph_ledger::{GraphLedgerReadPort, GraphRead},
    },
    compat::{self, Value},
    domain::ids,
};
use hardening_case::{write_json, HardeningCase};

fn exercise_constraint_claim(assigned_scope: bool) {
    let mut case = HardeningCase::new();
    let (finding, _) = case.evidence(
        "confirmed",
        &Value::object([
            ("finding_id", "ARCH-001".into()),
            ("status", "confirmed".into()),
        ]),
    );
    let (accepted, _) = case.evidence(
        "accepted",
        &Value::object([
            ("finding_id", "ARCH-001".into()),
            ("status", "accepted".into()),
        ]),
    );
    let base = Value::object([
        ("id", "constraint-1".into()),
        ("revision", 1_i64.into()),
        ("statement", "Verify covered inputs".into()),
        ("rationale", "Confirmed repair".into()),
        ("state", "active".into()),
        ("repository_id", "albanian-live-translate".into()),
        ("paths", Value::List(vec!["docs/".into()])),
        (
            "roles",
            Value::List(vec!["tech_lead".into(), "senior_engineer".into()]),
        ),
        ("acceptance_ids", Value::List(vec!["AC-001".into()])),
        ("finding_id", "ARCH-001".into()),
        ("confirmed_finding_ref", finding.into()),
        ("accepted_fix_ref", accepted.into()),
        ("authority_ref", "authority:test".into()),
        ("invalidation_reason", Value::Null),
        ("supersedes", Value::List(vec![])),
    ]);
    let mut unrelated = base.clone();
    unrelated.set("id", "unrelated".into());
    unrelated.set("paths", Value::List(vec!["src/".into()]));
    let mut invalidated = base.clone();
    invalidated.set("id", "invalidated".into());
    invalidated.set("state", "invalidated".into());
    invalidated.set("invalidation_reason", "No longer applies".into());
    let mut bundle = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "planning_constraints".into()),
        ("records", Value::List(vec![base, unrelated, invalidated])),
    ]);
    let bundle_path = case
        .fixture
        .graph
        .repository
        .join("docs/artifacts/constraints.json");
    let bytes = write_json(&bundle_path, &bundle);
    let route = if assigned_scope {
        "full_delivery"
    } else {
        "fast_path"
    };
    let mut task = case.fixture.graph.task("delivery", route, &[]);
    if assigned_scope {
        task.set(
            "scope",
            Value::object([
                ("included", Value::List(vec!["repo:docs/".into()])),
                ("excluded", Value::List(vec![])),
            ]),
        );
    }
    let mut evidence = task["evidence_paths"].as_list().unwrap().to_vec();
    evidence.push("repo:docs/artifacts/constraints.json".into());
    task.set("evidence_paths", Value::List(evidence));
    let mut constraints = task["constraints"].as_list().unwrap().to_vec();
    constraints.push(format!("planning-constraints:v1:{}", ids::sha256_bytes(&bytes)).into());
    task.set("constraints", Value::List(constraints));
    case.fixture.initialize(&task, true).unwrap();
    assert!(
        matches!(case.claim_raw(), Err(EngineError::State(error)) if error.code == "EVIDENCE_ENABLE_REQUIRED")
    );
    let coverage = compat::loads(br#"{"schema_version":1,"kind":"check_coverage","checks":[{"check_id":"repo-check","relevant_inputs":["docs/engineering-graph.md"],"complete":true}]}"#).unwrap();
    let path = case.inbox(&coverage);
    case.run(&[
        "evidence",
        "enable",
        "--run-id",
        "RUN-1",
        "--contract-version",
        "2",
        "--coverage-manifest",
        path.to_str().unwrap(),
        "--op-id",
        "enable",
    ])
    .unwrap();
    case.impact(route);
    let lead = case.claim();
    assert!(lead["node_key"].is_text(if assigned_scope {
        "tech_lead"
    } else {
        "senior_engineer"
    }));
    let inputs = lead["inputs"].as_list().unwrap();
    let projections = case
        .fixture
        .with_session(|session, _| {
            let mut projections = Vec::new();
            for input in inputs {
                let row = session
                    .first(GraphRead::ArtifactContent, &[input["ref"].clone()])?
                    .unwrap();
                let content = if row["content_json"].truthy() {
                    compat::loads_value(&row["content_json"])?
                } else {
                    Value::object([])
                };
                if content["kind"].is_text("planning_constraint_projection") {
                    projections.push(content);
                }
            }
            Ok(projections)
        })
        .unwrap();
    let projection = &projections[0];
    let records = projection["records"]
        .as_list()
        .unwrap()
        .iter()
        .map(|record| record["id"].text().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        records,
        if assigned_scope {
            vec!["constraint-1"]
        } else {
            vec![]
        }
    );
    if !assigned_scope {
        assert!(projection["scope_paths"].as_list().unwrap().is_empty());
        assert!(projection["diagnostics"]
            .as_list()
            .unwrap()
            .iter()
            .any(|item| item["reason"].is_text("unresolved_scope")));
    }
    assert!(!inputs.iter().any(|input| input["ref"]
        .as_string()
        .unwrap()
        .starts_with("repo:docs/artifacts/constraints.json")));
    bundle.set("records", Value::List(vec![]));
    write_json(&bundle_path, &bundle);
    assert!(matches!(
        case.status(),
        Err(EngineError::Contract(_) | EngineError::State(_))
    ));
    case.fixture.tear_down();
}

#[test]
fn test_planning_constraints_project_only_applicable_active_records() {
    exercise_constraint_claim(true);
}

#[test]
fn test_worker_constraints_exclude_broad_read_access_without_assigned_scope() {
    exercise_constraint_claim(false);
}
