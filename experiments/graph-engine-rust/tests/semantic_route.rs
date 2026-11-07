#[path = "../src/suite/task_fixture.rs"]
mod task_fixture;
use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        semantic_route::validate_route_and_topology,
    },
    compat::Value,
    domain::ids,
    infrastructure::sqlite::SqliteSession,
};
use rusqlite::Connection;

fn json(value: &Value) -> String {
    String::from_utf8(ids::canonical_bytes(value).unwrap()).unwrap()
}
struct Fixture {
    connection: Connection,
    run: Value,
    task: Value,
    policy: Value,
}
impl Fixture {
    fn new() -> Self {
        let source = task_fixture::TaskFixture::new();
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(
            "CREATE TABLE nodes(run_id TEXT,branch_id TEXT,node_key TEXT,stage TEXT,generation INTEGER,status TEXT,specialist_tag TEXT,depth INTEGER,result_json TEXT,envelope_json TEXT);
             CREATE TABLE joins(run_id TEXT,join_id TEXT,join_key TEXT,status TEXT,generation INTEGER);
             CREATE TABLE join_members(join_id TEXT,branch_id TEXT,mandatory INTEGER);
             CREATE TABLE execution_plans(run_id TEXT,plan_json TEXT);
             INSERT INTO nodes VALUES('R','mapper','impact_mapper','bootstrap',0,'ready',NULL,0,NULL,'{}');"
        ).unwrap();
        Self {
            connection,
            task: source.default_task(),
            policy: source.policy,
            run: Value::object([
                ("run_id", "R".into()),
                ("selected_route", Value::Null),
                ("selected_tags_json", Value::Null),
                ("design_generation", 0_i64.into()),
                ("implementation_generation", 0_i64.into()),
            ]),
        }
    }
    fn advisory(&mut self) {
        self.task.set("request_mode", "advisory".into());
        self.task.set("mandatory_impact_tags", Value::list([]));
        let classification = Value::object([
            ("schema_version", 1_i64.into()),
            ("task_id", self.task["task_id"].clone()),
            ("route_label", "advisory".into()),
            ("impact_tags", Value::list([])),
            ("evidence_refs", Value::list([])),
            ("attempt_id", "attempt-1".into()),
            ("claim_digest", "a".repeat(64).into()),
        ]);
        self.connection
            .execute(
                "UPDATE nodes SET status='succeeded',result_json=?",
                [json(&classification)],
            )
            .unwrap();
        self.connection.execute("INSERT INTO nodes VALUES('R','advisor','advisory_reviewer','advisory',0,'ready',NULL,0,NULL,'{}')",[]).unwrap();
        self.run.set("selected_route", "advisory".into());
        self.run.set("selected_tags_json", "[]".into());
    }
    fn validate(self) -> EngineResult<()> {
        validate_route_and_topology(
            &SqliteSession::from_connection(self.connection),
            &self.run,
            &self.task,
            &self.policy,
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
fn unclassified_mapper_keeps_the_bootstrap_graph() {
    Fixture::new().validate().unwrap();
}
#[test]
fn classification_is_required_before_persisting_a_route() {
    let mut f = Fixture::new();
    f.run.set("selected_route", "advisory".into());
    expect(f.validate(), "ROUTE_EVIDENCE_MISMATCH");
}
#[test]
fn bootstrap_cannot_have_join_rows() {
    let f = Fixture::new();
    f.connection
        .execute("INSERT INTO joins VALUES('R','J','other','open',0)", [])
        .unwrap();
    expect(f.validate(), "TOPOLOGY_STATE_INVALID");
}
#[test]
fn bootstrap_generations_are_zero() {
    let mut f = Fixture::new();
    f.run.set("implementation_generation", 1_i64.into());
    expect(f.validate(), "GENERATION_STATE_INVALID");
}
#[test]
fn advisory_route_matches_the_validated_impact_map() {
    let mut f = Fixture::new();
    f.advisory();
    f.validate().unwrap();
}
#[test]
fn stored_tags_match_the_normalized_classification() {
    let mut f = Fixture::new();
    f.advisory();
    f.run
        .set("selected_tags_json", "[\"security_privacy\"]".into());
    expect(f.validate(), "ROUTE_EVIDENCE_MISMATCH");
}
#[test]
fn advisory_route_rejects_an_implementation_node() {
    let mut f = Fixture::new();
    f.advisory();
    f.connection.execute("INSERT INTO nodes VALUES('R','writer','senior_engineer','implementation',0,'ready',NULL,0,NULL,'{}')",[]).unwrap();
    expect(f.validate(), "TOPOLOGY_STATE_INVALID");
}
#[test]
fn route_generation_counters_are_reconstructed() {
    let mut f = Fixture::new();
    f.advisory();
    f.run.set("design_generation", 1_i64.into());
    expect(f.validate(), "GENERATION_STATE_INVALID");
}
#[test]
fn delegated_depth_one_bindings_do_not_expand_the_parent_route() {
    let mut f = Fixture::new();
    f.advisory();
    f.connection.execute("INSERT INTO nodes VALUES('R','child','code_reviewer','delivery',0,'ready',NULL,1,NULL,'{}')",[]).unwrap();
    f.validate().unwrap();
}
#[test]
fn exactly_one_impact_mapper_is_required() {
    let f = Fixture::new();
    f.connection.execute("INSERT INTO nodes SELECT run_id,'second',node_key,stage,generation,status,specialist_tag,depth,result_json,envelope_json FROM nodes",[]).unwrap();
    expect(f.validate(), "TOPOLOGY_STATE_INVALID");
}
