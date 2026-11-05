use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        semantic_topology::validate_reconstructed_graph,
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
}
impl Fixture {
    fn new() -> Self {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(
            "CREATE TABLE nodes(run_id TEXT,branch_id TEXT,node_key TEXT,status TEXT,generation INTEGER,result_json TEXT,envelope_json TEXT);
             CREATE TABLE joins(run_id TEXT,join_id TEXT,join_key TEXT,kind TEXT,stage TEXT,status TEXT,generation INTEGER);
             CREATE TABLE fanouts(run_id TEXT,fanout_id TEXT);
             CREATE TABLE operations(run_id TEXT,response_json TEXT,resulting_revision INTEGER);
             CREATE TABLE execution_plans(run_id TEXT,plan_json TEXT);
             CREATE TABLE artifacts(run_id TEXT,kind TEXT,ref TEXT);
             CREATE TABLE join_members(join_id TEXT,branch_id TEXT,mandatory INTEGER);
             INSERT INTO execution_plans VALUES('R','{}');
             INSERT INTO nodes VALUES('R','mapper','impact_mapper','pending',0,NULL,'{}');"
        ).unwrap();
        let fixture = Self {
            connection,
            run: Value::object([
                ("run_id", "R".into()),
                ("policy_digest", "policy".into()),
                ("policy_json", "{}".into()),
                ("selected_tags_json", Value::Null),
                ("status", "active".into()),
            ]),
        };
        fixture.response(
            0,
            &Value::object([("branch", Value::object([("branch_id", "mapper".into())]))]),
        );
        fixture
    }
    fn response(&self, revision: i64, response: &Value) {
        self.connection
            .execute(
                "INSERT INTO operations VALUES('R',?,?)",
                params![json(response), revision],
            )
            .unwrap();
    }
    fn collection(&self, status: &str, inputs: Value) {
        self.response(
            1,
            &Value::object([
                ("code", "JOIN_ADVANCED".into()),
                ("join_id", "J".into()),
                ("created_join_ids", Value::list(["J".into()])),
                ("successor_branch_ids", Value::list(["S".into()])),
            ]),
        );
        self.connection
            .execute(
                "INSERT INTO joins VALUES('R','J','other_collection','collection','other',?,0)",
                [status],
            )
            .unwrap();
        self.connection
            .execute(
                "INSERT INTO nodes VALUES('R','S','supervisor','pending',0,NULL,?)",
                [json(&Value::object([("inputs", inputs)]))],
            )
            .unwrap();
        self.connection
            .execute(
                "INSERT INTO artifacts VALUES('R','collection','ledger:J#sha256=digest')",
                [],
            )
            .unwrap();
    }
    fn validate(self) -> EngineResult<()> {
        validate_reconstructed_graph(&SqliteSession::from_connection(self.connection), &self.run)
    }
}

fn expect_state(result: EngineResult<()>, code: &str) {
    match result {
        Err(EngineError::State(error)) => assert_eq!(error.code, code),
        other => panic!("expected {code}, got {other:?}"),
    }
}

#[test]
fn bootstrap_graph_is_reconstructed_from_native_operation_rows() {
    Fixture::new().validate().unwrap();
}

#[test]
fn unexplained_node_is_rejected_before_join_and_fanout_differences() {
    let fixture = Fixture::new();
    fixture.connection.execute_batch("INSERT INTO nodes VALUES('R','extra','unexpected','pending',0,NULL,'{}'); INSERT INTO joins VALUES('R','extra','other','collection','other','open',0); INSERT INTO fanouts VALUES('R','extra');").unwrap();
    expect_state(fixture.validate(), "TOPOLOGY_NODE_INVALID");
}

#[test]
fn unexplained_join_is_rejected_before_fanout_differences() {
    let fixture = Fixture::new();
    fixture.connection.execute_batch("INSERT INTO joins VALUES('R','extra','other','collection','other','open',0); INSERT INTO fanouts VALUES('R','extra');").unwrap();
    expect_state(fixture.validate(), "TOPOLOGY_JOIN_INVALID");
}

#[test]
fn unexplained_fanout_is_rejected() {
    let fixture = Fixture::new();
    fixture
        .connection
        .execute("INSERT INTO fanouts VALUES('R','extra')", [])
        .unwrap();
    expect_state(fixture.validate(), "TOPOLOGY_EDGE_INVALID");
}

#[test]
fn operation_topology_lists_must_contain_strings() {
    let fixture = Fixture::new();
    fixture.response(
        1,
        &Value::object([("child_branch_ids", Value::list([1_i64.into()]))]),
    );
    expect_state(fixture.validate(), "TOPOLOGY_HISTORY_INVALID");
}

#[test]
fn only_one_operation_can_advance_a_join() {
    let fixture = Fixture::new();
    let response = Value::object([("code", "JOIN_ADVANCED".into()), ("join_id", "J".into())]);
    fixture.response(1, &response);
    fixture.response(2, &response);
    expect_state(fixture.validate(), "TOPOLOGY_HISTORY_INVALID");
}

#[test]
fn sealed_nonclosure_join_requires_a_historical_advance() {
    let fixture = Fixture::new();
    fixture.response(
        1,
        &Value::object([("created_join_ids", Value::list(["J".into()]))]),
    );
    fixture
        .connection
        .execute(
            "INSERT INTO joins VALUES('R','J','other','collection','other','sealed',0)",
            [],
        )
        .unwrap();
    expect_state(fixture.validate(), "TOPOLOGY_HISTORY_INVALID");
}

#[test]
fn an_advance_cannot_leave_its_join_open() {
    let fixture = Fixture::new();
    fixture.collection("open", Value::list([]));
    expect_state(fixture.validate(), "TOPOLOGY_HISTORY_INVALID");
}

#[test]
fn collection_successor_must_bind_the_registered_collection_reference() {
    let fixture = Fixture::new();
    fixture.collection("sealed", Value::list([]));
    expect_state(fixture.validate(), "TOPOLOGY_EDGE_INVALID");
}

#[test]
fn collection_successor_can_bind_the_registered_collection_reference() {
    let fixture = Fixture::new();
    fixture.collection(
        "sealed",
        Value::list([Value::object([("ref", "ledger:J#sha256=digest".into())])]),
    );
    fixture.validate().unwrap();
}

#[test]
fn complete_graph_requires_one_sealed_closure() {
    let mut fixture = Fixture::new();
    fixture.run.set("status", "complete".into());
    expect_state(fixture.validate(), "TOPOLOGY_JOIN_INVALID");
}

#[test]
fn select_all_mapper_projection_retains_invalid_text_failure() {
    let fixture = Fixture::new();
    fixture.connection.execute_batch("ALTER TABLE nodes ADD COLUMN unrelated TEXT; UPDATE nodes SET unrelated=CAST(x'ff' AS TEXT);").unwrap();
    // The impact-mapper lookup selects every column, so it must retain this failure.
    assert!(matches!(
        fixture.validate(),
        Err(EngineError::Storage { .. }) | Err(EngineError::Python { .. })
    ));
}
