use graph_engine::{
    application::{
        check_receipts::CheckProvenancePort,
        error::{EngineError, EngineResult},
        evidence_integrity::validate_lifecycle_integrity,
    },
    compat::{PyString, Value},
    domain::ids,
    infrastructure::sqlite::SqliteSession,
};
use rusqlite::{params, Connection};

struct Provenance;
impl CheckProvenancePort for Provenance {
    fn actor(&self) -> EngineResult<PyString> {
        Ok("actor".into())
    }
    fn host_identity(&self) -> EngineResult<String> {
        Ok("host".into())
    }
    fn repository_worktree_digest(&self) -> EngineResult<String> {
        panic!("immutable history must not capture the present source")
    }
}

fn json(value: &Value) -> String {
    String::from_utf8(ids::canonical_bytes(value).unwrap()).unwrap()
}

struct Fixture {
    connection: Connection,
    run: Value,
    policy: Value,
    task: Value,
    activation: Value,
}
impl Fixture {
    fn new() -> Self {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(
            "CREATE TABLE artifacts(ref TEXT,run_id TEXT,kind TEXT,source_type TEXT,content_json TEXT,sha256 TEXT);
             CREATE TABLE execution_plans(run_id TEXT,plan_digest TEXT);
             CREATE TABLE events(run_id TEXT,event_type TEXT,detail_json TEXT,revision INTEGER,source_id TEXT);
             CREATE TABLE operations(run_id TEXT,operation_id TEXT,request_digest TEXT,response_json TEXT);
             CREATE TABLE budgets(run_id TEXT,budget_id TEXT,used INTEGER);
             CREATE TABLE branch_attempts(run_id TEXT,branch_id TEXT,attempt_id TEXT,claim_digest TEXT);
             CREATE TABLE check_evidence(run_id TEXT,check_id TEXT,artifact_ref TEXT);
             CREATE TABLE nodes(run_id TEXT,branch_id TEXT,generation INTEGER,node_key TEXT,envelope_json TEXT,result_digest TEXT);
             CREATE TABLE joins(run_id TEXT,join_id TEXT,kind TEXT,stage TEXT,status TEXT,generation INTEGER,join_key TEXT);
             CREATE TABLE join_members(join_id TEXT,branch_id TEXT);
             INSERT INTO execution_plans VALUES('R','plan');
             INSERT INTO budgets VALUES('R','delivery_repairs',0),('R','design_revisions',0);"
        ).unwrap();
        let mut fixture = Self {
            connection,
            run: Value::object([
                ("run_id", "R".into()),
                ("task_digest", "task".into()),
                ("policy_digest", "policy".into()),
                ("repository_path", "repo".into()),
            ]),
            policy: Value::object([]),
            task: Value::object([
                ("required_check_ids", Value::list([])),
                ("acceptance_ids", Value::list(["AC01".into()])),
            ]),
            activation: Value::object([
                ("kind", "evidence_activation".into()),
                ("schema_version", 1_i64.into()),
                ("original_format", 6_i64.into()),
                ("task_digest", "task".into()),
                ("policy_digest", "policy".into()),
                ("plan_digest", "plan".into()),
                ("coverage_manifest_digest", "coverage".into()),
                ("checks", Value::list([])),
                ("roots", Value::list([])),
                ("actor", "actor".into()),
                ("host_identity", "host".into()),
                ("complete", true.into()),
            ]),
        };
        fixture.store_activation();
        let request = Value::object([
            ("coverage_manifest_digest", "coverage".into()),
            ("checks", Value::list([])),
            ("roots", Value::list([])),
        ]);
        fixture.event(1, "evidence.enable", &request, &Value::object([]));
        fixture
    }

    fn artifact(&self, reference: &str, kind: &str, payload: &Value) {
        self.connection
            .execute(
                "INSERT INTO artifacts VALUES(?,'R',?,'ledger',?,?)",
                params![
                    reference,
                    kind,
                    json(payload),
                    ids::sha256_bytes(&ids::canonical_bytes(payload).unwrap())
                ],
            )
            .unwrap();
    }

    fn store_activation(&mut self) {
        self.connection
            .execute("DELETE FROM artifacts WHERE ref='activation'", [])
            .unwrap();
        self.artifact("activation", "evidence_manifest", &self.activation);
    }

    fn event(&self, revision: i64, kind: &str, request: &Value, response: &Value) {
        let operation = format!("operation-{revision}");
        self.connection
            .execute(
                "INSERT INTO events VALUES('R',?,?,?,?)",
                params![kind, json(request), revision, operation],
            )
            .unwrap();
        self.connection
            .execute(
                "INSERT INTO operations VALUES('R',?,?,?)",
                params![
                    operation,
                    ids::sha256_bytes(&ids::canonical_bytes(request).unwrap()),
                    json(response)
                ],
            )
            .unwrap();
    }

    fn reservation(&self, repeat_cost: i64) {
        let fence = |branch: &str| {
            Value::object([
                ("branch_id", branch.into()),
                ("attempt_id", "attempt".into()),
                ("claim_digest", "claim".into()),
                ("result_digest", Value::Null),
            ])
        };
        let mut request = Value::object([
            ("check_id", "check".into()),
            ("source_generation", 0_i64.into()),
            ("predecessor_ref", Value::Null),
            ("coverage", Value::object([])),
            ("activation_ref", "activation".into()),
            ("executor", fence("executor")),
            ("source", fence("source")),
        ]);
        let digest = ids::sha256_bytes(&ids::canonical_bytes(&request).unwrap());
        let mut reservation = request.clone();
        reservation.set("kind", "check_reservation".into());
        reservation.set("reservation_id", "reservation".into());
        reservation.set("request_digest", digest.into());
        reservation.set("repeat_cost", repeat_cost.into());
        reservation.set("budget_id", "delivery_repairs".into());
        self.artifact("reservation-ref", "evidence_manifest", &reservation);
        self.event(
            2,
            "check.start",
            &request,
            &Value::object([("reservation_id", "reservation".into())]),
        );
        request = Value::object([("reservation_id", "reservation".into())]);
        self.event(3, "check.abandon", &request, &Value::object([]));
        self.connection.execute_batch("INSERT INTO branch_attempts VALUES('R','executor','attempt','claim'),('R','source','attempt','claim');").unwrap();
    }

    fn repair(&self, target: &str) {
        let envelope = Value::object([(
            "effect_capabilities",
            Value::list([Value::object([
                ("effect", "filesystem_write".into()),
                ("action", "edit".into()),
                ("target_ref", "repo:src/".into()),
            ])]),
        )]);
        self.connection
            .execute(
                "INSERT INTO nodes VALUES('R','writer',1,'senior_engineer',?,NULL)",
                [json(&envelope)],
            )
            .unwrap();
        let packet = Value::object([
            ("kind", "repair_packet".into()),
            ("branch_id", "writer".into()),
            ("task_digest", "task".into()),
            ("plan_digest", "plan".into()),
            ("run_id", "R".into()),
            ("generation", 1_i64.into()),
            ("origin_gate", "senior_engineer".into()),
            ("failed_criterion_ids", Value::list(["AC01".into()])),
            (
                "authorized_scope",
                Value::list([Value::object([
                    ("effect", "filesystem_write".into()),
                    ("action", "edit".into()),
                    ("target_ref", target.into()),
                ])]),
            ),
            ("failure_result_ref", Value::Null),
            ("attempt_id", Value::Null),
        ]);
        self.artifact("repair", "evidence_manifest", &packet);
    }

    fn acceptance(&self, reference: &str, criterion: &str, predecessor: Value) {
        self.connection.execute("INSERT INTO artifacts VALUES('underlying','R','acceptance_evidence','file',NULL,'digest')", []).unwrap();
        self.artifact(
            reference,
            "evidence_manifest",
            &Value::object([
                ("kind", "acceptance_binding".into()),
                ("run_id", "R".into()),
                ("task_digest", "task".into()),
                ("plan_digest", "plan".into()),
                ("criterion_id", criterion.into()),
                ("checks", Value::object([])),
                (
                    "underlying_evidence",
                    Value::object([("ref", "underlying".into()), ("sha256", "digest".into())]),
                ),
                ("predecessor_ref", predecessor),
            ]),
        );
    }

    fn validate(self) -> EngineResult<()> {
        validate_lifecycle_integrity(
            &SqliteSession::from_connection(self.connection),
            &self.run,
            &self.policy,
            &self.task,
            &Provenance,
            true,
        )
    }
}

fn expect_state(result: EngineResult<()>, code: &str) {
    match result {
        Err(EngineError::State(error)) => assert_eq!(error.code, code),
        other => panic!("expected {code}, got {other:?}"),
    }
}

#[test]
fn activation_event_and_runtime_provenance_match_in_sqlite() {
    Fixture::new().validate().unwrap();
}

#[test]
fn complete_requires_the_boolean_singleton() {
    let mut fixture = Fixture::new();
    fixture.activation.set("complete", 1_i64.into());
    fixture.store_activation();
    expect_state(fixture.validate(), "EVIDENCE_ACTIVATION_INVALID");
}

#[test]
fn duplicate_activation_events_are_rejected_before_event_replay() {
    let fixture = Fixture::new();
    fixture
        .connection
        .execute(
            "INSERT INTO events SELECT run_id,event_type,detail_json,9,'duplicate' FROM events",
            [],
        )
        .unwrap();
    expect_state(fixture.validate(), "EVIDENCE_ACTIVATION_INVALID");
}

#[test]
fn event_request_digest_is_checked_before_reservation_fields() {
    let fixture = Fixture::new();
    fixture.event(2, "check.start", &Value::object([]), &Value::object([]));
    fixture
        .connection
        .execute(
            "UPDATE operations SET request_digest='wrong' WHERE operation_id='operation-2'",
            [],
        )
        .unwrap();
    expect_state(fixture.validate(), "CHECK_RESERVATION_INVALID");
}

#[test]
fn start_and_abandon_replay_historical_fences_without_source_capture() {
    let fixture = Fixture::new();
    fixture.reservation(0);
    fixture.validate().unwrap();
}

#[test]
fn reservation_rejects_a_changed_historical_claim() {
    let fixture = Fixture::new();
    fixture.reservation(0);
    fixture
        .connection
        .execute(
            "UPDATE branch_attempts SET claim_digest='changed' WHERE branch_id='source'",
            [],
        )
        .unwrap();
    expect_state(fixture.validate(), "CHECK_RESERVATION_INVALID");
}

#[test]
fn first_executor_slot_cannot_charge_a_repeat() {
    let fixture = Fixture::new();
    fixture.reservation(1);
    expect_state(fixture.validate(), "CHECK_RESERVATION_INVALID");
}

#[test]
fn budget_history_precedes_repair_packet_validation() {
    let fixture = Fixture::new();
    fixture.artifact(
        "invalid-repair",
        "evidence_manifest",
        &Value::object([("kind", "repair_packet".into())]),
    );
    fixture
        .connection
        .execute(
            "UPDATE budgets SET used=1 WHERE budget_id='delivery_repairs'",
            [],
        )
        .unwrap();
    expect_state(fixture.validate(), "BUDGET_STATE_INVALID");
}

#[test]
fn repair_scope_uses_authorized_subtree_without_present_source_capture() {
    let fixture = Fixture::new();
    fixture.repair("repo:src/module.rs");
    fixture.validate().unwrap();
}

#[test]
fn repair_scope_cannot_escape_the_authorized_subtree() {
    let fixture = Fixture::new();
    fixture.repair("repo:src-other/module.rs");
    expect_state(fixture.validate(), "REPAIR_PACKET_INVALID");
}

#[test]
fn acceptance_binding_requires_registered_underlying_evidence() {
    let fixture = Fixture::new();
    fixture.acceptance("accepted", "AC01", Value::Null);
    fixture
        .connection
        .execute("DELETE FROM artifacts WHERE ref='underlying'", [])
        .unwrap();
    expect_state(fixture.validate(), "ACCEPTANCE_BINDING_MISMATCH");
}

#[test]
fn acceptance_binding_can_reference_immutable_underlying_evidence() {
    let fixture = Fixture::new();
    fixture.acceptance("accepted", "AC01", Value::Null);
    fixture.validate().unwrap();
}
