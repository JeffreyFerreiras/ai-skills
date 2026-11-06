use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        semantic_fanouts::validate_fanouts,
    },
    compat::Value,
    domain::ids,
    infrastructure::sqlite::SqliteSession,
};
use rusqlite::{params, Connection};

fn json(value: &Value) -> String {
    String::from_utf8(ids::canonical_bytes(value).unwrap()).unwrap()
}

struct Fixture {
    connection: Connection,
    run: Value,
    identifier: String,
}
impl Fixture {
    fn new() -> Self {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(
            "CREATE TABLE joins(run_id TEXT,join_id TEXT,kind TEXT,stage TEXT,generation INTEGER);
             CREATE TABLE nodes(branch_id TEXT,status TEXT,envelope_json TEXT,retry_count INTEGER,max_retries INTEGER);
             CREATE TABLE join_members(join_id TEXT,branch_id TEXT,mandatory INTEGER);
             CREATE TABLE fanouts(run_id TEXT,fanout_id TEXT,stage TEXT,generation INTEGER,member_branch_ids_json TEXT,status TEXT,assessment_ref TEXT,assessment_digest TEXT,authority_ref TEXT,actor TEXT,host_identity TEXT,assessed_at TEXT);
             CREATE TABLE fanout_dependencies(fanout_id TEXT,before_branch_id TEXT,after_branch_id TEXT,reason TEXT);
             CREATE TABLE artifacts(ref TEXT,kind TEXT,sha256 TEXT,content_json TEXT);
             INSERT INTO joins VALUES('R','J','collection','delivery',0);
             INSERT INTO nodes VALUES('branch-A','pending','{}',0,1),('branch-B','pending','{}',0,1);
             INSERT INTO join_members VALUES('J','branch-A',1),('J','branch-B',1);"
        ).unwrap();
        let identifier = ids::stable_id("R", "policy", "fanout", "delivery", 0, None).unwrap();
        connection.execute("INSERT INTO fanouts VALUES('R',?,'delivery',0,?,'awaiting',NULL,NULL,NULL,NULL,NULL,NULL)",params![identifier,json(&Value::list(["branch-A".into(),"branch-B".into()]))]).unwrap();
        Self {
            connection,
            identifier,
            run: Value::object([
                ("run_id", "R".into()),
                ("policy_digest", "policy".into()),
                ("host_identity", "host".into()),
            ]),
        }
    }
    fn assessment(&self) {
        let members = Value::list(["branch-A", "branch-B"].map(|branch| {
            Value::object([
                ("branch_id", branch.into()),
                (
                    "resources",
                    Value::object([
                        ("writable_paths", Value::list([])),
                        ("mutable_state_refs", Value::list([])),
                        ("exclusive_device_refs", Value::list([])),
                        ("services", Value::list([])),
                    ]),
                ),
            ])
        }));
        let content = Value::object([
            ("schema_version", 1_i64.into()),
            ("kind", "fanout_assessment".into()),
            ("run_id", "R".into()),
            ("fanout_id", self.identifier.clone().into()),
            ("members", members),
            ("dependencies", Value::list([])),
            (
                "evidence",
                Value::list([Value::object([
                    ("kind", "evidence_manifest".into()),
                    (
                        "ref",
                        format!("repo:docs/fanout.md#sha256={}", "a".repeat(64)).into(),
                    ),
                    ("sha256", "a".repeat(64).into()),
                ])]),
            ),
            ("authority_ref", "authority".into()),
            ("actor", "actor".into()),
            ("host_identity", "host".into()),
            ("assessed_at", "time".into()),
        ]);
        self.connection
            .execute(
                "INSERT INTO artifacts VALUES('assessment','evidence_manifest','digest',?)",
                [json(&content)],
            )
            .unwrap();
        self.connection.execute("UPDATE fanouts SET status='assessed',assessment_ref='assessment',assessment_digest='digest',authority_ref='authority',actor='actor',host_identity='host',assessed_at='time'",[]).unwrap();
        let envelope = Value::object([(
            "inputs",
            Value::list([Value::object([
                ("ref", "assessment".into()),
                ("sha256", "digest".into()),
                ("kind", "evidence_manifest".into()),
            ])]),
        )]);
        self.connection
            .execute(
                "UPDATE nodes SET status='ready',envelope_json=?",
                [json(&envelope)],
            )
            .unwrap();
    }
    fn validate(self) -> EngineResult<()> {
        validate_fanouts(
            &SqliteSession::from_connection(self.connection),
            &self.run,
            true,
        )
    }
}
fn expect(result: EngineResult<()>, code: &str) {
    match result {
        Err(EngineError::State(error)) => assert_eq!(error.code, code),
        other => panic!("expected {code}, got {other:?}"),
    }
}

#[test]
fn awaiting_fanout_has_exact_collection_members_and_pending_branches() {
    Fixture::new().validate().unwrap();
}
#[test]
fn each_parallel_collection_requires_a_fanout() {
    let f = Fixture::new();
    f.connection.execute("DELETE FROM fanouts", []).unwrap();
    expect(f.validate(), "FANOUT_STATE_INVALID");
}
#[test]
fn member_order_is_canonical() {
    let f = Fixture::new();
    f.connection
        .execute(
            "UPDATE fanouts SET member_branch_ids_json=?",
            [json(&Value::list(["branch-B".into(), "branch-A".into()]))],
        )
        .unwrap();
    expect(f.validate(), "FANOUT_MEMBER_INVALID");
}
#[test]
fn awaiting_fanout_cannot_dispatch_a_member() {
    let f = Fixture::new();
    f.connection
        .execute(
            "UPDATE nodes SET status='ready' WHERE branch_id='branch-A'",
            [],
        )
        .unwrap();
    expect(f.validate(), "FANOUT_ASSESSMENT_REQUIRED");
}
#[test]
fn awaiting_fanout_cannot_have_dependency_rows() {
    let f = Fixture::new();
    f.connection
        .execute(
            "INSERT INTO fanout_dependencies VALUES(?,'branch-A','branch-B','order')",
            [&f.identifier],
        )
        .unwrap();
    expect(f.validate(), "FANOUT_ASSESSMENT_REQUIRED");
}
#[test]
fn assessed_fanout_binds_provenance_inputs_and_dispatch_eligibility() {
    let f = Fixture::new();
    f.assessment();
    f.validate().unwrap();
}
#[test]
fn assessment_provenance_must_match_the_run_host() {
    let f = Fixture::new();
    f.assessment();
    let mut f = f;
    f.run.set("host_identity", "other-host".into());
    expect(f.validate(), "FANOUT_ASSESSMENT_INVALID");
}
#[test]
fn dependency_rows_must_equal_the_normalized_assessment() {
    let f = Fixture::new();
    f.assessment();
    f.connection
        .execute(
            "INSERT INTO fanout_dependencies VALUES(?,'branch-A','branch-B','order')",
            [&f.identifier],
        )
        .unwrap();
    expect(f.validate(), "FANOUT_DEPENDENCY_STATE_INVALID");
}
#[test]
fn assessed_member_requires_the_immutable_assessment_input() {
    let f = Fixture::new();
    f.assessment();
    f.connection
        .execute(
            "UPDATE nodes SET envelope_json='{\"inputs\":[]}' WHERE branch_id='branch-B'",
            [],
        )
        .unwrap();
    expect(f.validate(), "FANOUT_ASSESSMENT_INVALID");
}
#[test]
fn eligible_member_cannot_remain_pending() {
    let f = Fixture::new();
    f.assessment();
    f.connection
        .execute(
            "UPDATE nodes SET status='pending' WHERE branch_id='branch-B'",
            [],
        )
        .unwrap();
    expect(f.validate(), "FANOUT_DEPENDENCY_STATE_INVALID");
}
