use graph_engine::{
    application::{error::EngineError, graph_transitions},
    compat::Value,
    infrastructure::sqlite::SqliteSession,
};

fn fixture() -> SqliteSession {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE nodes(branch_id TEXT,status TEXT,retry_count INTEGER,max_retries INTEGER,envelope_json TEXT); CREATE TABLE review_delegation_members(request_slot_id TEXT,child_branch_id TEXT); CREATE TABLE review_delegation_dependencies(request_slot_id TEXT,before_branch_id TEXT,after_branch_id TEXT); CREATE TABLE fanouts(fanout_id TEXT,run_id TEXT,status TEXT,member_branch_ids_json TEXT); CREATE TABLE fanout_dependencies(fanout_id TEXT,before_branch_id TEXT,after_branch_id TEXT);").unwrap();
    SqliteSession::from_connection(connection)
}

#[test]
fn review_successors_wait_for_exhausted_predecessor_and_preserve_order() {
    let session = fixture();
    for (id, status, retries) in [("A", "failed", 0), ("B", "pending", 0), ("C", "pending", 0)] {
        session
            .execute_values(
                "INSERT INTO nodes VALUES(?,?,?,1,?)",
                &[
                    id.into(),
                    status.into(),
                    i64::from(retries).into(),
                    format!("{{\"status\":\"{status}\"}}").into(),
                ],
            )
            .unwrap();
    }
    for id in ["B", "C", "missing"] {
        session
            .execute_values(
                "INSERT INTO review_delegation_members VALUES('SLOT',?)",
                &[id.into()],
            )
            .unwrap();
    }
    session
        .execute_values(
            "INSERT INTO review_delegation_dependencies VALUES('SLOT','A','B')",
            &[],
        )
        .unwrap();
    session
        .execute_values(
            "INSERT INTO review_delegation_dependencies VALUES('SLOT','B','C')",
            &[],
        )
        .unwrap();
    assert!(
        graph_transitions::promote_review_successors(&session, &"SLOT".into())
            .unwrap()
            .is_empty()
    );
    session
        .execute_values("UPDATE nodes SET retry_count=1 WHERE branch_id='A'", &[])
        .unwrap();
    assert!(Value::List(
        graph_transitions::promote_review_successors(&session, &"SLOT".into()).unwrap()
    )
    .py_eq(&Value::List(vec!["B".into()])));
    let rows = session
        .query_values(
            "SELECT status,envelope_json FROM nodes WHERE branch_id='B'",
            &[],
        )
        .unwrap();
    assert!(rows[0]["status"].is_text("ready"));
    assert!(rows[0]["envelope_json"].is_text("{\"status\": \"ready\"}"));
}

#[test]
fn fanout_promotions_sort_final_ids_and_do_not_promote_ready_nodes_again() {
    let session = fixture();
    for id in ["Z", "A"] {
        session
            .execute_values(
                "INSERT INTO nodes VALUES(?,'pending',0,1,'{}')",
                &[id.into()],
            )
            .unwrap();
    }
    session
        .execute_values(
            "INSERT INTO fanouts VALUES('F1','RUN','assessed','[\"Z\",\"A\"]')",
            &[],
        )
        .unwrap();
    session
        .execute_values(
            "INSERT INTO fanouts VALUES('F2','RUN','assessed','[\"Z\"]')",
            &[],
        )
        .unwrap();
    assert!(Value::List(
        graph_transitions::promote_fanout_successors(&session, &"RUN".into()).unwrap()
    )
    .py_eq(&Value::List(vec!["A".into(), "Z".into()])));
    assert!(
        graph_transitions::promote_fanout_successors(&session, &"RUN".into())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn settled_state_short_circuits_retry_errors_and_assignment_lookup_stops_at_first_match() {
    let mut branch = Value::object([
        ("status", "succeeded".into()),
        ("retry_count", Value::Null),
        ("max_retries", "invalid".into()),
    ]);
    assert!(graph_transitions::branch_settled(&branch).unwrap());
    branch.set("status", "failed".into());
    assert!(
        matches!(graph_transitions::branch_settled(&branch).unwrap_err(),EngineError::Python{kind:"TypeError",message}if message.equals("'>=' not supported between instances of 'NoneType' and 'str'"))
    );
    let assignment = Value::object([("assignment_id", "REVIEW".into())]);
    let plan = Value::object([(
        "conditional_review_assignments",
        Value::List(vec![assignment.clone(), Value::Null]),
    )]);
    assert!(
        graph_transitions::review_assignment(&plan, &"REVIEW".into())
            .unwrap()
            .py_eq(&assignment)
    );
}
