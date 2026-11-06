#[allow(dead_code)]
#[path = "support/hardening_case.rs"]
mod hardening_case;
#[path = "support/hardening_evidence_cases.rs"]
mod hardening_evidence_cases;
#[path = "support/hardening_ledger_cases.rs"]
mod hardening_ledger_cases;
#[path = "support/hardening_source_git.rs"]
mod hardening_source_git;

use graph_engine::{
    application::error::{EngineError, EngineResult},
    compat::{self, SerializationProfile, Value},
    infrastructure::sqlite::SqliteSession,
};
use hardening_case::HardeningCase;

const RUN: &str = "RUN-1";
const REPOSITORY: &str = "albanian-live-translate";

fn state_failure<T>(result: EngineResult<T>, expected: &[&str]) {
    match result {
        Err(EngineError::State(error)) => {
            assert!(expected.contains(&error.code.as_str()), "{error}")
        }
        Err(error) => panic!("expected state error {expected:?}, got {error:?}"),
        Ok(_) => panic!("expected state error {expected:?}, got success"),
    }
}

fn database(case: &HardeningCase) -> SqliteSession {
    SqliteSession::connect_existing(&case.fixture.graph.store.db_path(REPOSITORY, RUN)).unwrap()
}

fn row(session: &SqliteSession, sql: &str, parameters: &[Value]) -> Value {
    session
        .query_values(sql, parameters)
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
}

fn stored_json(value: &Value) -> Value {
    String::from_utf8(compat::dumps(value, SerializationProfile::STORED).unwrap())
        .unwrap()
        .into()
}

fn initialize(case: &HardeningCase, tags: &[&str]) -> Value {
    case.fixture
        .initialize(
            &case.fixture.graph.task("delivery", "full_delivery", tags),
            true,
        )
        .unwrap()
}

fn lookup<'a>(rows: &'a Value, key: &str, expected: &Value) -> &'a Value {
    rows.as_list()
        .unwrap()
        .iter()
        .find(|row| row[key].py_eq(expected))
        .unwrap()
}
