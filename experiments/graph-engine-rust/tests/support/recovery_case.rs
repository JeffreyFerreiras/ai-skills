use super::hardening_case::HardeningCase;
use graph_engine::{
    application::{
        error::EngineResult,
        graph_ledger::{GraphLedgerReadPort, GraphRead},
    },
    compat::{self, Value},
    domain::ids,
};
use rusqlite::types::Value as SqlValue;
use std::path::Path;

pub(super) const BLOCK_REASON: &str = "MISSING_REQUIRED_REVIEW_SKILL";
pub(super) const RECOVERY_REASON: &str = "REVIEW_EQUIVALENT_SKILL_AUTHORIZED";

pub(super) fn block_evidence(case: &mut HardeningCase, reason: &str) -> Value {
    let evidence = case.repo_artifact("block", &format!("block-evidence-{}", case.counter()));
    Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "block".into()),
        ("run_id", "RUN-1".into()),
        ("reason_code", reason.into()),
        ("evidence", Value::list([evidence])),
    ])
}

pub(super) fn block(case: &HardeningCase, reason: &str, path: &Path, operation: &str) -> Value {
    case.run(&[
        "block",
        "--run-id",
        "RUN-1",
        "--reason-code",
        reason,
        "--evidence-manifest",
        path.to_str().unwrap(),
        "--op-id",
        operation,
    ])
    .unwrap()
}

pub(super) fn recovery_manifest(case: &HardeningCase, branch: &Value, evidence: Value) -> Value {
    case.fixture
        .with_session(|session, _| {
            let run = session.first(GraphRead::Run, &["RUN-1".into()])?.unwrap();
            let plan = session
                .first(GraphRead::ExecutionPlan, &["RUN-1".into()])?
                .unwrap();
            let node = session
                .first(GraphRead::Node, std::slice::from_ref(&branch["branch_id"]))?
                .unwrap();
            let env = compat::loads_value(&node["envelope_json"])?;
            let mut inputs: Vec<_> = env["inputs"]
                .as_list()
                .unwrap()
                .iter()
                .map(|input| input["ref"].as_string().unwrap().clone())
                .collect();
            inputs.sort();
            let id = branch["branch_id"].text().unwrap();
            let failure = node["result_digest"].text().unwrap();
            Ok(Value::object([
                ("schema_version", 1_i64.into()),
                ("kind", "recovery_authorization".into()),
                ("run_id", "RUN-1".into()),
                ("branch_id", branch["branch_id"].clone()),
                ("blocked_reason", BLOCK_REASON.into()),
                ("reason_code", RECOVERY_REASON.into()),
                ("authority_ref", plan["authority_ref"].clone()),
                ("task_digest", run["task_digest"].clone()),
                ("plan_digest", plan["plan_digest"].clone()),
                (
                    "failure_result_ref",
                    format!("ledger:{id}#sha256={failure}").into(),
                ),
                ("required_skill", "review-agent".into()),
                ("replacement_skill", "bug-hunter-review".into()),
                ("decision", "APPROVE".into()),
                (
                    "equivalence_basis",
                    "user-authorized-equivalent-review".into(),
                ),
                (
                    "prior_input_refs",
                    Value::list(inputs.into_iter().map(Value::String)),
                ),
                (
                    "effect_capabilities_digest",
                    ids::sha256_bytes(&ids::canonical_bytes(&env["effect_capabilities"])?).into(),
                ),
                ("failure_evidence", env["evidence"].clone()),
                ("evidence", Value::list([evidence])),
            ]))
        })
        .unwrap()
}

pub(super) fn recover_args(
    branch: &Value,
    reason: &Value,
    authority: &Value,
    path: &Path,
    operation: &str,
) -> Vec<String> {
    [
        "recover",
        "--run-id",
        "RUN-1",
        "--branch-id",
        &branch["branch_id"].text().unwrap(),
        "--reason-code",
        &reason.text().unwrap(),
        "--authority-ref",
        &authority.text().unwrap(),
        "--recovery-manifest",
        path.to_str().unwrap(),
        "--op-id",
        operation,
    ]
    .map(str::to_owned)
    .to_vec()
}

pub(super) fn run(case: &HardeningCase, args: &[String]) -> EngineResult<Value> {
    case.run(&args.iter().map(String::as_str).collect::<Vec<_>>())
}

pub(super) fn authorize(
    case: &mut HardeningCase,
    branch: &Value,
    manifest: &Value,
    expected: &Value,
    operation: &str,
) -> EngineResult<Value> {
    let path = case.inbox(manifest);
    run(
        case,
        &recover_args(
            branch,
            &expected["reason_code"],
            &expected["authority_ref"],
            &path,
            operation,
        ),
    )
}

pub(super) fn fail_review(case: &mut HardeningCase, branch: &Value, evidence: Value) {
    case.record_as(
        branch,
        Value::object([
            ("schema_version", 1_i64.into()),
            ("run_id", "RUN-1".into()),
            ("branch_id", branch["branch_id"].clone()),
            ("status", "failed".into()),
            (
                "output_kind",
                branch["output_contract"]["artifact_kind"].clone(),
            ),
            ("failure_code", "MISSING_REQUIRED_SKILL".into()),
            ("evidence", Value::list([evidence])),
        ]),
        "result",
    )
    .unwrap();
}

pub(super) fn state_error(result: EngineResult<Value>, code: &str) {
    let error = result.unwrap_err();
    assert!(
        matches!(&error, graph_engine::application::error::EngineError::State(error) if error.code == code),
        "{error:?}"
    );
}

pub(super) fn artifact_bytes(case: &HardeningCase, reference: &Value) -> String {
    case.fixture
        .with_session(|session, _| {
            let row = session
                .first(GraphRead::ArtifactContent, std::slice::from_ref(reference))?
                .unwrap();
            Ok(row["content_json"].text().unwrap())
        })
        .unwrap()
}

#[derive(Debug, PartialEq)]
pub(super) struct LedgerSnapshot {
    schema: Vec<(String, String, Option<String>)>,
    rows: Vec<Vec<Vec<SqlValue>>>,
}

pub(super) fn ledger_snapshot(case: &HardeningCase) -> LedgerSnapshot {
    case.fixture
        .with_session(|session, _| {
            let connection = session.connection();
            let mut query = connection
                .prepare("SELECT type,name,sql FROM sqlite_master ORDER BY type,name")
                .unwrap();
            let schema = query
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            let mut rows = Vec::new();
            for (_, table, _) in schema.iter().filter(|(kind, _, _)| kind == "table") {
                let table = table.replace('"', "\"\"");
                let mut query = connection
                    .prepare(&format!("SELECT * FROM \"{table}\""))
                    .unwrap();
                let columns = query.column_count();
                let contents = query
                    .query_map([], |row| {
                        (0..columns)
                            .map(|column| row.get(column))
                            .collect::<rusqlite::Result<Vec<SqlValue>>>()
                    })
                    .unwrap()
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .unwrap();
                rows.push(contents);
            }
            Ok(LedgerSnapshot { schema, rows })
        })
        .unwrap()
}
