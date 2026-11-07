use super::sqlite::{self, SqliteSession};
use crate::{
    application::{
        error::{EngineError, EngineResult},
        persistent_store::{FinalState, PersistentSession, StateDatabase},
    },
    compat::Value,
};
use std::path::Path;

pub struct NativeStateDatabase;
impl StateDatabase for NativeStateDatabase {
    fn create(&self, path: &Path) -> EngineResult<Box<dyn PersistentSession>> {
        Ok(Box::new(SqliteSession::create(path)?))
    }
    fn connect(&self, path: &Path) -> EngineResult<Box<dyn PersistentSession>> {
        Ok(Box::new(SqliteSession::connect_existing(path)?))
    }
    fn version(&self) -> &str {
        sqlite::sqlite_version()
    }
    fn locking_probe(&self, path: &Path) -> EngineResult<()> {
        let result = (|| {
            let first = rusqlite::Connection::open(path)?;
            let second = rusqlite::Connection::open(path)?;
            first.busy_timeout(std::time::Duration::ZERO)?;
            second.busy_timeout(std::time::Duration::ZERO)?;
            first.execute_batch("BEGIN IMMEDIATE")?;
            let Err(error) = second.execute_batch("BEGIN IMMEDIATE") else {
                return Ok(false);
            };
            if !matches!(sqlite::sqlite_failure(error), EngineError::Storage { kind: "OperationalError", message, .. } if message.to_lowercase().contains("locked"))
            {
                return Ok(false);
            }
            first.execute_batch("ROLLBACK")?;
            second.execute_batch("BEGIN IMMEDIATE; ROLLBACK")?;
            Ok::<_, rusqlite::Error>(true)
        })();
        if matches!(result, Ok(true)) {
            Ok(())
        } else {
            Err(EngineError::state("SQLITE_LOCK_PROBE_FAILED"))
        }
    }
}
fn scalar(session: &SqliteSession, sql: &str) -> EngineResult<Value> {
    session
        .query_values(sql, &[])?
        .first()
        .and_then(Value::as_object)
        .and_then(|fields| fields.first())
        .map(|(_, value)| value.clone())
        .ok_or_else(|| EngineError::python("TypeError", "'NoneType' object is not subscriptable"))
}
impl PersistentSession for SqliteSession {
    fn first_run(&self) -> EngineResult<Option<Value>> {
        Ok(self
            .query_values("SELECT * FROM runs LIMIT 1", &[])?
            .into_iter()
            .next())
    }
    fn incomplete_rows(&self) -> EngineResult<(Vec<Value>, Vec<Value>)> {
        Ok((
            self.query_values("SELECT run_id,authoritative FROM runs", &[])?,
            self.query_values(
                "SELECT run_id,operation_id,request_digest FROM operations",
                &[],
            )?,
        ))
    }
    fn integrity(&self) -> EngineResult<Value> {
        scalar(self, "PRAGMA integrity_check")
    }
    fn journal(&self, enable_wal: bool) -> EngineResult<Value> {
        scalar(
            self,
            if enable_wal {
                "PRAGMA journal_mode=WAL"
            } else {
                "PRAGMA journal_mode"
            },
        )
    }
    fn set_full_synchronous(&self) -> EngineResult<()> {
        self.connection()
            .execute_batch("PRAGMA synchronous=FULL")
            .map_err(sqlite::sqlite_failure)
    }
    fn synchronous(&self) -> EngineResult<Value> {
        scalar(self, "PRAGMA synchronous")
    }
    fn begin_initial(&self) -> EngineResult<()> {
        self.connection()
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(sqlite::sqlite_failure)?;
        Ok(())
    }
    fn insert_initial_run(&self, values: &[Value]) -> EngineResult<()> {
        self.execute_values("INSERT INTO runs(run_id,repository_id,repository_digest,repository_path,repository_device,repository_inode,status,state_revision,engine_version,state_schema_version,policy_digest,policy_json,task_digest,task_ref,task_path,task_json,request_mode,minimum_route,durability,durability_detail,permission_verification,degraded_permissions_ack,degraded_durability_ack,journal_mode,synchronous,sqlite_version,local_filesystem,host_identity,database_device,database_inode,authoritative,started_at,finished_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)", values)?;
        Ok(())
    }
    fn insert_initial_plan(&self, values: &[Value]) -> EngineResult<()> {
        self.execute_values("INSERT INTO execution_plans(run_id,size,plan_json,plan_digest,status) VALUES(?,?,?,?,?)", values)?;
        Ok(())
    }
    fn insert_artifact(&self, values: &[Value]) -> EngineResult<()> {
        self.execute_values(
            "INSERT INTO artifacts VALUES(?,?,?,?,?,?,?,?,?,?,?)",
            values,
        )?;
        Ok(())
    }
    fn insert_budget(&self, values: &[Value]) -> EngineResult<()> {
        self.execute_values("INSERT INTO budgets VALUES(?,?,?,0)", values)?;
        Ok(())
    }
    fn finalize_initial(&self, state: &FinalState<'_>) -> EngineResult<()> {
        self.connection()
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(sqlite::sqlite_failure)?;
        self.execute_values("UPDATE runs SET journal_mode=?,synchronous='FULL',durability=?,durability_detail=?,local_filesystem=?,database_device=?,database_inode=?,authoritative=1 WHERE run_id=?", &[state.journal.into(), state.durability.into(), state.detail.into(), state.locality.into(), Value::Integer(state.device.into()), Value::Integer(state.inode.into()), state.run_id.into()])?;
        Ok(())
    }
    fn update_initial_response(
        &self,
        run_id: &str,
        operation_id: &str,
        response_json: &[u8],
    ) -> EngineResult<()> {
        let response = std::str::from_utf8(response_json).unwrap();
        self.execute_values(
            "UPDATE operations SET response_json=? WHERE run_id=? AND operation_id=?",
            &[response.into(), run_id.into(), operation_id.into()],
        )?;
        Ok(())
    }
    fn insert_node(&self, values: &[Value]) -> EngineResult<()> {
        self.execute_values("INSERT INTO nodes(branch_id,run_id,node_instance_id,node_key,role,stage,generation,mandatory,specialist_tag,status,retry_count,max_retries,envelope_json,parent_branch_id,request_slot_id,assignment_id,ordinal,depth) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)", values)?;
        Ok(())
    }
    fn insert_join_row(&self, values: &[Value]) -> EngineResult<()> {
        self.execute_values("INSERT INTO joins(join_id,run_id,join_key,kind,stage,generation,status) VALUES(?,?,?,?,?,?,?)", values)?;
        Ok(())
    }
    fn insert_join_member(&self, values: &[Value]) -> EngineResult<()> {
        self.execute_values("INSERT INTO join_members VALUES(?,?,?)", values)?;
        Ok(())
    }
}
