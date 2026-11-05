#[path = "support/store.rs"]
mod store_fixture;

use graph_engine::{
    application::{
        error::EngineError, graph_ledger::GraphRead, graph_writes::GraphWrite,
        persistent_store::StateStore, state, state_history,
    },
    compat::{self, SerializationProfile, Value},
    infrastructure::{state_database::NativeStateDatabase, state_platform::NativeStatePlatform},
};
use std::cell::RefCell;

#[test]
fn persistent_session_ports_share_transaction_and_after_validation_sees_all_writes() {
    let fixture = store_fixture::Fixture::new("session-owned-ports");
    let platform = NativeStatePlatform;
    let database = NativeStateDatabase;
    let no_fault = |_: &str| Ok(());
    let store =
        StateStore::at_root(fixture.root.join("state"), &platform, &database, &no_fault).unwrap();
    store.initialize(&fixture.request()).unwrap();
    store
        .with_run("fixture-repository", "RUN-1", |session| {
            let branch = &fixture.node["branch_id"];
            let observed = RefCell::new(vec![]);
            let fault = |point: &str| {
                assert_eq!(point, "before_commit");
                let node = session.first(GraphRead::Node, &[branch.clone()])?.unwrap();
                let envelope = compat::loads_value(&node["envelope_json"])?;
                assert_eq!(envelope["component_marker"].as_i64(), Some(1));
                assert_eq!(
                    session
                        .all(GraphRead::OperationResponses, &["RUN-1".into()])?
                        .len(),
                    1
                );
                observed.borrow_mut().push("hook".to_string());
                Ok(())
            };
            let response = state::mutate(
                session,
                "RUN-1",
                "PORTS",
                &Value::object([("command", "abort".into())]),
                |_, _, _| {
                    let node = session.first(GraphRead::Node, &[branch.clone()])?.unwrap();
                    let mut envelope = compat::loads_value(&node["envelope_json"])?;
                    envelope.set("component_marker", 1i64.into());
                    let bytes = compat::dumps(&envelope, SerializationProfile::SORTED_STORED)?;
                    session.write(
                        GraphWrite::ReadyNode,
                        &[String::from_utf8(bytes).unwrap().into(), branch.clone()],
                    )?;
                    observed.borrow_mut().push("action".to_string());
                    Ok(Value::object([("code", "PORT_COMPONENT".into())]))
                },
                |_, run| {
                    state_history::validate_operation_ledger(session, &run.fields)?;
                    let count = session
                        .all(GraphRead::OperationResponses, &["RUN-1".into()])?
                        .len();
                    observed
                        .borrow_mut()
                        .push(format!("validate:{}:{count}", run.revision()?));
                    Ok(())
                },
                &fault,
            )?;
            assert_eq!(response["state_revision"].as_i64(), Some(2));
            assert_eq!(
                *observed.borrow(),
                ["validate:1:1", "action", "hook", "validate:2:2"]
            );
            let fault = |_: &str| Err(EngineError::Injected("rollback component".into()));
            let failure = state::mutate(
                session,
                "RUN-1",
                "ROLLBACK",
                &Value::object([("command", "abort".into())]),
                |_, _, _| {
                    session.write(
                        GraphWrite::ReadyNode,
                        &["{\"component_marker\":2}".into(), branch.clone()],
                    )?;
                    Ok(Value::object([("code", "ROLLBACK_COMPONENT".into())]))
                },
                |_, run| state_history::validate_operation_ledger(session, &run.fields),
                &fault,
            )
            .unwrap_err();
            assert!(
                matches!(failure,EngineError::Injected(message)if message=="rollback component")
            );
            let node = session.first(GraphRead::Node, &[branch.clone()])?.unwrap();
            assert_eq!(
                compat::loads_value(&node["envelope_json"])?["component_marker"].as_i64(),
                Some(1)
            );
            assert_eq!(
                session
                    .all(GraphRead::OperationResponses, &["RUN-1".into()])?
                    .len(),
                2
            );
            Ok(())
        })
        .unwrap();
}
