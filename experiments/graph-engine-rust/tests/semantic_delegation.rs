use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        semantic_delegation::validate_review_delegation,
    },
    compat::Value,
    domain::{
        ids,
        review_identity::{delegated_identity, request_slot_id},
    },
    infrastructure::sqlite::SqliteSession,
};
use rusqlite::{params, Connection};
fn json(value: &Value) -> String {
    String::from_utf8(ids::canonical_bytes(value).unwrap()).unwrap()
}
struct Fixture {
    connection: Connection,
    run: Value,
    plan: Value,
    slot: String,
    child: String,
}
impl Fixture {
    fn new() -> Self {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(
            "CREATE TABLE execution_plans(run_id TEXT,plan_json TEXT,plan_digest TEXT);
             CREATE TABLE review_delegation_requests(run_id TEXT,request_slot_id TEXT,parent_branch_id TEXT,parent_attempt_id TEXT,parent_claim_digest TEXT,generation INTEGER,round_number INTEGER,status TEXT,preliminary_ref TEXT,request_ref TEXT,request_digest TEXT,preliminary_digest TEXT,authority_ref TEXT,child_member_tuples_json TEXT,dispatch_cost INTEGER);
             CREATE TABLE nodes(branch_id TEXT,role TEXT,depth INTEGER,node_key TEXT,envelope_json TEXT,request_slot_id TEXT,assignment_id TEXT,ordinal INTEGER,parent_branch_id TEXT,status TEXT,result_json TEXT,result_digest TEXT,reason_code TEXT,failure_code TEXT,retry_count INTEGER,max_retries INTEGER,started_at TEXT,finished_at TEXT);
             CREATE TABLE branch_attempts(branch_id TEXT,attempt_id TEXT,claim_digest TEXT,outcome TEXT,attempt_number INTEGER,started_at TEXT,finished_at TEXT);
             CREATE TABLE artifacts(ref TEXT,kind TEXT,content_json TEXT,sha256 TEXT);
             CREATE TABLE operations(run_id TEXT,request_digest TEXT,response_json TEXT);
             CREATE TABLE review_delegation_members(request_slot_id TEXT,assignment_id TEXT,ordinal INTEGER,child_branch_id TEXT);
             CREATE TABLE review_delegation_fanouts(request_slot_id TEXT,status TEXT,assessment_ref TEXT,assessment_digest TEXT,authority_ref TEXT,actor TEXT,host_identity TEXT,assessed_at TEXT);
             CREATE TABLE review_delegation_collections(request_slot_id TEXT,status TEXT,collection_digest TEXT,content_json TEXT);
             CREATE TABLE review_delegation_dependencies(request_slot_id TEXT,before_branch_id TEXT,after_branch_id TEXT,reason TEXT);"
        ).unwrap();
        let run = Value::object([
            ("run_id", "RUN".into()),
            ("policy_digest", "policy".into()),
            ("host_identity", "host".into()),
        ]);
        let slot = request_slot_id([
            &run["run_id"],
            &run["policy_digest"],
            &"plan".into(),
            &"PARENT".into(),
            &"ATTEMPT".into(),
            &"claim".into(),
            &0_i64.into(),
            &1_i64.into(),
        ])
        .unwrap();
        let child = delegated_identity(
            &slot.clone().into(),
            &"R1".into(),
            &1_i64.into(),
            &"branch".into(),
        )
        .unwrap();
        let assignment = Value::object([
            ("assignment_id", "R1".into()),
            ("model", "gpt-6.1".into()),
            ("max_instances", 1_i64.into()),
            ("dispatch_weight", 3_i64.into()),
            ("allowed_reason_codes", Value::list(["FOCUS".into()])),
            ("allowed_acceptance_ids", Value::list(["AC-001".into()])),
            ("allowed_evidence_kinds", Value::list(["source".into()])),
            ("scope_refs", Value::list(["repo:docs/".into()])),
        ]);
        let plan = Value::object([
            ("conditional_review_assignments", Value::list([assignment])),
            (
                "reviewer_delegation_limits",
                Value::object([
                    ("max_depth", 1_i64.into()),
                    ("max_children_per_request", 2_i64.into()),
                    ("max_request_rounds", 2_i64.into()),
                    ("max_children_per_run", 4_i64.into()),
                    ("max_weighted_dispatch_cost", 9_i64.into()),
                ]),
            ),
        ]);
        connection
            .execute(
                "INSERT INTO execution_plans VALUES('RUN',?,'plan')",
                [json(&plan)],
            )
            .unwrap();
        let evidence_ref = format!("repo:docs/file#sha256={}", "a".repeat(64));
        let envelope = Value::object([
            ("model", "gpt-6.1".into()),
            (
                "inputs",
                Value::list([Value::object([
                    ("kind", "source".into()),
                    ("ref", evidence_ref.clone().into()),
                    ("sha256", "a".repeat(64).into()),
                ])]),
            ),
        ]);
        connection.execute("INSERT INTO nodes(branch_id,role,depth,node_key,envelope_json) VALUES('PARENT','code_reviewer',0,'code_reviewer',?)",[json(&envelope)]).unwrap();
        connection.execute("INSERT INTO branch_attempts(branch_id,attempt_id,claim_digest,outcome) VALUES('PARENT','ATTEMPT','claim','delegated')",[]).unwrap();
        let preliminary = Value::object([
            ("schema_version", 1_i64.into()),
            ("kind", "review_preliminary".into()),
            ("run_id", "RUN".into()),
            ("parent_branch_id", "PARENT".into()),
            ("parent_attempt_id", "ATTEMPT".into()),
            ("generation", 0_i64.into()),
            ("findings", Value::list([])),
            (
                "evidence",
                Value::list([Value::object([
                    ("evidence_id", "E1".into()),
                    ("kind", "source".into()),
                    ("ref", evidence_ref.into()),
                    ("sha256", "a".repeat(64).into()),
                ])]),
            ),
        ]);
        let request = Value::object([
            ("schema_version", 1_i64.into()),
            ("kind", "review_fanout_request".into()),
            ("run_id", "RUN".into()),
            ("parent_branch_id", "PARENT".into()),
            ("parent_attempt_id", "ATTEMPT".into()),
            ("round", 1_i64.into()),
            (
                "members",
                Value::list([Value::object([
                    ("assignment_id", "R1".into()),
                    ("ordinal", 1_i64.into()),
                    ("reason_code", "FOCUS".into()),
                    ("acceptance_ids", Value::list(["AC-001".into()])),
                    ("evidence_ids", Value::list(["E1".into()])),
                ])]),
            ),
        ]);
        connection
            .execute(
                "INSERT INTO artifacts VALUES('preliminary','delivery_review',?,'preliminary-sha')",
                [json(&preliminary)],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO artifacts VALUES('request','evidence_manifest',?,'request-sha')",
                [json(&request)],
            )
            .unwrap();
        let tuples = Value::list([Value::list([
            child.clone().into(),
            "R1".into(),
            1_i64.into(),
        ])]);
        connection.execute("INSERT INTO review_delegation_requests VALUES('RUN',?,'PARENT','ATTEMPT','claim',0,1,'awaiting_assessment','preliminary','request',?,?,'authority',?,3)",params![slot,"d".repeat(64),"e".repeat(64),json(&tuples)]).unwrap();
        connection
            .execute(
                "INSERT INTO review_delegation_members VALUES(?,'R1',1,?)",
                params![slot, child],
            )
            .unwrap();
        connection.execute("INSERT INTO nodes(branch_id,role,depth,node_key,envelope_json,request_slot_id,assignment_id,ordinal,parent_branch_id,status) VALUES(?,'code_reviewer',1,'code_reviewer','{}',?,'R1',1,'PARENT','pending')",params![child,slot]).unwrap();
        connection.execute("INSERT INTO review_delegation_fanouts(request_slot_id,status) VALUES(?,'awaiting')",[&slot]).unwrap();
        connection.execute("INSERT INTO review_delegation_collections(request_slot_id,status) VALUES(?,'open')",[&slot]).unwrap();
        let operation = Value::object([
            ("command", "record.review-fanout".into()),
            ("branch_id", "PARENT".into()),
            ("attempt_id", "ATTEMPT".into()),
            ("claim_digest", "claim".into()),
            ("preliminary_digest", "e".repeat(64).into()),
            ("request_digest", "d".repeat(64).into()),
            ("authority_ref", "authority".into()),
        ]);
        connection
            .execute(
                "INSERT INTO operations VALUES('RUN',?,?)",
                params![
                    ids::sha256_bytes(&ids::canonical_bytes(&operation).unwrap()),
                    json(&Value::object([("request_slot_id", slot.clone().into())]))
                ],
            )
            .unwrap();
        Self {
            connection,
            run,
            plan,
            slot,
            child,
        }
    }
    fn set_limit(&mut self, key: &str, value: Value) {
        let mut limits = self.plan["reviewer_delegation_limits"].clone();
        limits.set(key, value);
        self.plan.set("reviewer_delegation_limits", limits);
        self.store_plan();
    }
    fn store_plan(&self) {
        self.connection
            .execute("UPDATE execution_plans SET plan_json=?", [json(&self.plan)])
            .unwrap();
    }
    fn assessment(&self) {
        let evidence_ref = format!("repo:docs/file#sha256={}", "a".repeat(64));
        let content = Value::object([
            ("schema_version", 1_i64.into()),
            ("kind", "fanout_assessment".into()),
            ("run_id", "RUN".into()),
            ("fanout_id", self.slot.clone().into()),
            (
                "members",
                Value::list([Value::object([
                    ("branch_id", self.child.clone().into()),
                    (
                        "resources",
                        Value::object([
                            ("writable_paths", Value::list([])),
                            ("mutable_state_refs", Value::list([])),
                            ("exclusive_device_refs", Value::list([])),
                            ("services", Value::list([])),
                        ]),
                    ),
                ])]),
            ),
            ("dependencies", Value::list([])),
            (
                "evidence",
                Value::list([Value::object([
                    ("kind", "source".into()),
                    ("ref", evidence_ref.clone().into()),
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
                "INSERT INTO artifacts VALUES(?,'source','{}',?)",
                params![evidence_ref, "a".repeat(64)],
            )
            .unwrap();
        self.connection
            .execute(
                "INSERT INTO artifacts VALUES('assessment','evidence_manifest',?,'assessment-sha')",
                [json(&content)],
            )
            .unwrap();
        self.connection.execute("UPDATE review_delegation_fanouts SET status='assessed',assessment_ref='assessment',assessment_digest='assessment-sha',authority_ref='authority',actor='actor',host_identity='host',assessed_at='time'",[]).unwrap();
        self.connection
            .execute("UPDATE review_delegation_requests SET status='running'", [])
            .unwrap();
        self.connection
            .execute(
                "UPDATE nodes SET status='ready' WHERE branch_id=?",
                [&self.child],
            )
            .unwrap();
    }
    fn seal(&self) -> Value {
        self.assessment();
        let envelope = Value::object([
            ("model", "gpt-6.1".into()),
            (
                "review_assignment",
                Value::object([("review_lens", "bugs".into())]),
            ),
            ("artifact_ref", Value::Null),
            ("evidence", Value::list([])),
            ("decision", "ACCEPT".into()),
        ]);
        let result = Value::object([("findings", Value::list([]))]);
        let digest = "b".repeat(64);
        let reference = format!("ledger:{}#sha256={digest}", self.child);
        self.connection.execute("UPDATE nodes SET status='succeeded',envelope_json=?,result_json=?,result_digest=?,retry_count=0,max_retries=1,started_at='start',finished_at='finish' WHERE branch_id=?",params![json(&envelope),json(&result),digest,self.child]).unwrap();
        self.connection
            .execute(
                "INSERT INTO artifacts VALUES(?,'delivery_review',?,?)",
                params![reference, json(&result), digest],
            )
            .unwrap();
        self.connection.execute("INSERT INTO branch_attempts VALUES(?,'CHILD-ATTEMPT','child-claim','succeeded',1,'start','finish')",[&self.child]).unwrap();
        let terminal = Value::object([
            ("result", result),
            (
                "result_artifact",
                Value::object([
                    ("kind", "delivery_review".into()),
                    ("ref", reference.into()),
                    ("sha256", digest.into()),
                ]),
            ),
            ("artifact_ref", Value::Null),
            ("evidence", Value::list([])),
            ("decision", "ACCEPT".into()),
            ("reason_code", Value::Null),
            ("failure_code", Value::Null),
            ("result_digest", "b".repeat(64).into()),
            ("retry_count", 0_i64.into()),
            ("max_retries", 1_i64.into()),
            ("started_at", "start".into()),
            ("finished_at", "finish".into()),
            (
                "attempts",
                Value::list([Value::object([
                    ("attempt_number", 1_i64.into()),
                    ("attempt_id", "CHILD-ATTEMPT".into()),
                    ("claim_digest", "child-claim".into()),
                    ("started_at", "start".into()),
                    ("finished_at", "finish".into()),
                    ("outcome", "succeeded".into()),
                ])]),
            ),
        ]);
        let content = Value::object([
            ("schema_version", 1_i64.into()),
            ("kind", "review_nested_collection".into()),
            ("run_id", "RUN".into()),
            ("request_slot_id", self.slot.clone().into()),
            ("parent_branch_id", "PARENT".into()),
            ("preliminary_findings", Value::list([])),
            (
                "members",
                Value::list([Value::object([
                    ("branch_id", self.child.clone().into()),
                    ("assignment_id", "R1".into()),
                    ("ordinal", 1_i64.into()),
                    ("role", "code_reviewer".into()),
                    ("status", "succeeded".into()),
                    ("findings", Value::list([])),
                    ("terminal", terminal),
                ])]),
            ),
            ("issues", Value::list([])),
        ]);
        self.connection
            .execute("UPDATE review_delegation_requests SET status='sealed'", [])
            .unwrap();
        self.store_collection(&content);
        content
    }
    fn store_collection(&self, content: &Value) {
        self.connection.execute("UPDATE review_delegation_collections SET status='sealed',collection_digest=?,content_json=?",params![ids::sha256_bytes(&ids::canonical_bytes(content).unwrap()),json(content)]).unwrap();
    }
    fn validate(self) -> EngineResult<()> {
        validate_review_delegation(
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
fn immutable_awaiting_request_binds_parent_authority_members_and_cost() {
    Fixture::new().validate().unwrap();
}
#[test]
fn empty_request_history_does_not_require_delegation_assignments() {
    let mut f = Fixture::new();
    f.connection
        .execute("DELETE FROM review_delegation_requests", [])
        .unwrap();
    f.plan = Value::object([]);
    f.store_plan();
    f.validate().unwrap();
}
#[test]
fn requests_require_enabled_assignments() {
    let mut f = Fixture::new();
    f.plan
        .set("conditional_review_assignments", Value::list([]));
    f.store_plan();
    expect(f.validate(), "DELEGATION_DISABLED");
}
#[test]
fn request_rounds_are_contiguous_before_slot_validation() {
    let f = Fixture::new();
    f.connection
        .execute("UPDATE review_delegation_requests SET round_number=2", [])
        .unwrap();
    expect(f.validate(), "DELEGATION_ROUND_LIMIT");
}
#[test]
fn earlier_rounds_must_be_sealed() {
    let f = Fixture::new();
    f.connection.execute("INSERT INTO review_delegation_requests SELECT run_id,'second',parent_branch_id,parent_attempt_id,parent_claim_digest,generation,2,status,preliminary_ref,request_ref,request_digest,preliminary_digest,authority_ref,child_member_tuples_json,dispatch_cost FROM review_delegation_requests",[]).unwrap();
    expect(f.validate(), "DELEGATION_ROUND_NOT_SETTLED");
}
#[test]
fn request_slot_is_reconstructed() {
    let f = Fixture::new();
    f.connection
        .execute(
            "UPDATE review_delegation_requests SET request_slot_id='changed'",
            [],
        )
        .unwrap();
    expect(f.validate(), "DELEGATION_STATE_INVALID");
}
#[test]
fn parent_attempt_must_have_ended_in_delegation() {
    let f = Fixture::new();
    f.connection
        .execute("UPDATE branch_attempts SET outcome='succeeded'", [])
        .unwrap();
    expect(f.validate(), "DELEGATION_PARENT_STATE_INVALID");
}
#[test]
fn preliminary_json_failure_maps_to_the_artifact_error() {
    let f = Fixture::new();
    f.connection
        .execute(
            "UPDATE artifacts SET content_json='{' WHERE ref='preliminary'",
            [],
        )
        .unwrap();
    expect(f.validate(), "DELEGATION_ARTIFACT_INVALID");
}
#[test]
fn exactly_one_matching_operation_establishes_authority() {
    let f = Fixture::new();
    f.connection.execute("DELETE FROM operations", []).unwrap();
    expect(f.validate(), "DELEGATION_AUTHORITY_INVALID");
}
#[test]
fn member_tuple_mismatch_precedes_malformed_stored_tuple_json() {
    let f = Fixture::new();
    f.connection
        .execute(
            "UPDATE review_delegation_members SET child_branch_id='changed'",
            [],
        )
        .unwrap();
    f.connection
        .execute(
            "UPDATE review_delegation_requests SET child_member_tuples_json='{'",
            [],
        )
        .unwrap();
    expect(f.validate(), "DELEGATION_MEMBER_INVALID");
}
#[test]
fn dispatch_cost_is_recomputed_from_assignments() {
    let f = Fixture::new();
    f.connection
        .execute("UPDATE review_delegation_requests SET dispatch_cost=4", [])
        .unwrap();
    expect(f.validate(), "DELEGATION_COST_INVALID");
}
#[test]
fn fanout_and_collection_rows_are_required() {
    let f = Fixture::new();
    f.connection
        .execute("DELETE FROM review_delegation_fanouts", [])
        .unwrap();
    expect(f.validate(), "DELEGATION_STATE_INVALID");
}
#[test]
fn assessed_request_binds_immutable_evidence_and_dependencies() {
    let f = Fixture::new();
    f.assessment();
    f.validate().unwrap();
}
#[test]
fn assessed_request_requires_registered_evidence_digest() {
    let f = Fixture::new();
    f.assessment();
    f.connection
        .execute(
            "UPDATE artifacts SET sha256='wrong' WHERE kind='source'",
            [],
        )
        .unwrap();
    expect(f.validate(), "DELEGATION_ASSESSMENT_INVALID");
}
#[test]
fn assessed_dependency_requires_eligible_dispatch_state() {
    let f = Fixture::new();
    f.assessment();
    f.connection
        .execute("UPDATE nodes SET status='pending' WHERE depth=1", [])
        .unwrap();
    expect(f.validate(), "DELEGATION_DEPENDENCY_STATE_INVALID");
}
#[test]
fn sealed_collection_reconstructs_complete_terminal_members() {
    let f = Fixture::new();
    f.seal();
    f.validate().unwrap();
}
#[test]
fn sealed_collection_rejects_a_rehashed_but_incomplete_member() {
    let f = Fixture::new();
    let mut content = f.seal();
    content.set("members", Value::list([]));
    f.store_collection(&content);
    expect(f.validate(), "DELEGATION_COLLECTION_INVALID");
}
#[test]
fn per_run_cost_limit_is_checked_after_request_validation() {
    let mut f = Fixture::new();
    f.set_limit("max_weighted_dispatch_cost", 2_i64.into());
    expect(f.validate(), "DELEGATION_LIMIT_INVALID");
}
#[test]
fn per_run_child_limit_is_checked_after_request_validation() {
    let mut f = Fixture::new();
    f.set_limit("max_children_per_run", 0_i64.into());
    expect(f.validate(), "DELEGATION_LIMIT_INVALID");
}
#[test]
fn awaiting_collection_must_remain_open() {
    let f = Fixture::new();
    f.connection
        .execute(
            "UPDATE review_delegation_collections SET status='sealed'",
            [],
        )
        .unwrap();
    expect(f.validate(), "DELEGATION_COLLECTION_INVALID");
}
