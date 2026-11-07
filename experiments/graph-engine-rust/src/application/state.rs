use super::error::{EngineError, EngineResult};
use crate::{
    compat::{self, SerializationProfile, Value},
    domain::{ids, state},
};
use num_bigint::BigInt;

#[derive(Clone, Debug)]
pub struct RunRecord {
    pub fields: Value,
}
impl RunRecord {
    pub fn status(&self) -> Option<String> {
        self.fields["status"].text()
    }
    pub fn revision(&self) -> EngineResult<BigInt> {
        let value = self
            .fields
            .get("state_revision")
            .ok_or_else(|| EngineError::python("IndexError", "No item with that key"))?;
        crate::compat::integer::coerce(value).map_err(Into::into)
    }
}
#[derive(Clone, Debug)]
pub struct OperationRecord {
    pub request_digest: Value,
    pub response_json: Value,
    pub resulting_revision: Value,
}
#[derive(Clone, Debug)]
pub enum RunPatch {
    BlockedReason(Option<String>),
    Status(String),
}

pub trait GraphSession {
    fn begin_immediate(&self) -> EngineResult<()>;
    fn commit(&self) -> EngineResult<()>;
    fn rollback(&self) -> EngineResult<()>;
    fn operation(&self, run: &str, operation: &str) -> EngineResult<Option<OperationRecord>>;
    fn run(&self, run: &str) -> EngineResult<Option<RunRecord>>;
    fn update_revision(&self, run: &str, revision: &BigInt) -> EngineResult<()>;
    fn insert_operation(
        &self,
        run: &str,
        operation: &str,
        request_digest: &str,
        response_json: &[u8],
        revision: &BigInt,
    ) -> EngineResult<()>;
    fn insert_event(
        &self,
        run: &str,
        revision: &BigInt,
        command: &str,
        operation: &str,
        detail_json: &[u8],
    ) -> EngineResult<()>;
    fn patch_run(&self, run: &str, patch: &RunPatch) -> EngineResult<()>;
    fn event_exists(&self, run: &str, operation: &str) -> EngineResult<bool>;
}
pub trait FaultHook {
    fn observe(&self, point: &str) -> EngineResult<()>;
}
impl<F> FaultHook for F
where
    F: Fn(&str) -> EngineResult<()>,
{
    fn observe(&self, point: &str) -> EngineResult<()> {
        self(point)
    }
}

pub fn mutate<A, V>(
    session: &dyn GraphSession,
    run_id: &str,
    operation_id: &str,
    request: &Value,
    action: A,
    mut semantic: V,
    fault: &dyn FaultHook,
) -> EngineResult<Value>
where
    A: FnOnce(&dyn GraphSession, &RunRecord, &BigInt) -> EngineResult<Value>,
    V: FnMut(&dyn GraphSession, &RunRecord) -> EngineResult<()>,
{
    let request_digest = ids::sha256_bytes(&ids::canonical_bytes(request)?);
    session.begin_immediate()?;
    let result = (|| {
        if let Some(previous) = session.operation(run_id, operation_id)? {
            session.rollback()?;
            if !previous.request_digest.is_text(&request_digest) {
                return Err(EngineError::state("OPERATION_CONFLICT"));
            }
            let mut response = compat::loads_value(&previous.response_json)?;
            if response.as_object().is_none() {
                let message = if matches!(response, Value::List(_)) {
                    "list indices must be integers or slices, not str".to_string()
                } else {
                    format!(
                        "'{}' object does not support item assignment",
                        response.python_type()
                    )
                };
                return Err(EngineError::python("TypeError", message));
            }
            response.set("code", "REPLAYED".into());
            return Ok(response);
        }
        let run = session
            .run(run_id)?
            .ok_or_else(|| EngineError::state("RUN_NOT_FOUND"))?;
        let command_value = request
            .get("command")
            .ok_or_else(|| EngineError::python("KeyError", "'command'"))?;
        let command = if let Some(command) = command_value.text() {
            command
        } else if command_value.hashable() {
            return Err(EngineError::state("UNKNOWN_MUTATION"));
        } else {
            return Err(EngineError::python(
                "TypeError",
                format!("unhashable type: '{}'", command_value.python_type()),
            ));
        };
        state::validate_mutation_state(&command, &run.status().unwrap_or_default())?;
        semantic(session, &run)?;
        let revision = run.revision()? + BigInt::from(1);
        let mut response = action(session, &run, &revision)?;
        if response.as_object().is_none() {
            return Err(EngineError::python(
                "AttributeError",
                format!(
                    "'{}' object has no attribute 'update'",
                    response.python_type()
                ),
            ));
        }
        response.set("schema_version", 1_i64.into());
        response.set("ok", true.into());
        response.set("run_id", run_id.into());
        response.set("state_revision", Value::Integer(revision.clone()));
        fault.observe("before_commit")?;
        session.update_revision(run_id, &revision)?;
        session.insert_operation(
            run_id,
            operation_id,
            &request_digest,
            &compat::dumps(&response, SerializationProfile::SORTED_STORED)?,
            &revision,
        )?;
        session.insert_event(
            run_id,
            &revision,
            &command,
            operation_id,
            &compat::dumps(request, SerializationProfile::SORTED_STORED)?,
        )?;
        let updated = session
            .run(run_id)?
            .ok_or_else(|| EngineError::state("RUN_NOT_FOUND"))?;
        semantic(session, &updated)?;
        session.commit()?;
        Ok(response)
    })();
    if result.is_err() {
        let _ = session.rollback();
    }
    result
}

pub fn mutate_persistent<A, V>(
    session: &dyn super::persistent_store::PersistentSession,
    run_id: &str,
    operation_id: &str,
    request: &Value,
    action: A,
    mut semantic: V,
    fault: &dyn FaultHook,
) -> EngineResult<Value>
where
    A: FnOnce(
        &dyn super::persistent_store::PersistentSession,
        &RunRecord,
        &BigInt,
    ) -> EngineResult<Value>,
    V: FnMut(&dyn super::persistent_store::PersistentSession, &RunRecord) -> EngineResult<()>,
{
    mutate(
        session,
        run_id,
        operation_id,
        request,
        |_, run, revision| action(session, run, revision),
        |_, run| semantic(session, run),
        fault,
    )
}
