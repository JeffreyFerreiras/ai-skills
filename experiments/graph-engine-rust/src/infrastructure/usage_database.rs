use super::sqlite::SqliteSession;
use crate::{
    application::{
        error::{EngineError, EngineResult},
        usage_store::{BindingAssociation, UsageAssociation, UsageObservation, UsageSession},
    },
    compat::Value,
};

fn field(row: &Value, key: &str) -> EngineResult<Value> {
    row.get(key)
        .cloned()
        .ok_or_else(|| EngineError::python("IndexError", "No item with that key"))
}

fn association(row: &Value) -> EngineResult<UsageAssociation> {
    Ok(UsageAssociation {
        attempt_id: field(row, "attempt_id")?,
        branch_id: field(row, "branch_id")?,
        role: field(row, "role")?,
        stage: field(row, "stage")?,
        generation: field(row, "generation")?,
    })
}

impl UsageSession for SqliteSession {
    fn visit_usage_observations(
        &self,
        run_id: &Value,
        visit: &mut dyn FnMut(UsageObservation) -> EngineResult<()>,
    ) -> EngineResult<()> {
        let mut statement = self.connection().prepare(
            "SELECT detail_json FROM events WHERE run_id=? AND event_type='usage.observation' ORDER BY event_id",
        ).map_err(super::sqlite::sqlite_failure)?;
        let mut rows = statement
            .query(rusqlite::params_from_iter(
                super::sqlite::parameters(std::slice::from_ref(run_id), 1)?.iter(),
            ))
            .map_err(super::sqlite::sqlite_failure)?;
        while let Some(row) = rows.next().map_err(super::sqlite::sqlite_failure)? {
            let row = super::sqlite::row_value(row)?;
            visit(UsageObservation {
                detail: field(&row, "detail_json")?,
            })?;
        }
        Ok(())
    }

    fn usage_binding_association(
        &self,
        run_id: &Value,
        branch_id: &Value,
        attempt_id: &Value,
    ) -> EngineResult<Option<BindingAssociation>> {
        self.query_values(
            "SELECT n.role,n.stage,n.generation FROM nodes n JOIN branch_attempts a ON a.branch_id=n.branch_id WHERE n.run_id=? AND n.branch_id=? AND a.attempt_id=?",
            &[run_id.clone(), branch_id.clone(), attempt_id.clone()],
        )?
        .first()
        .map(|row| Ok(BindingAssociation {
            role: field(row, "role")?,
            stage: field(row, "stage")?,
            generation: field(row, "generation")?,
        }))
        .transpose()
    }

    fn usage_record_association(
        &self,
        run_id: &Value,
        branch_id: &str,
        attempt_id: &str,
    ) -> EngineResult<Option<UsageAssociation>> {
        self.query_values(
            "SELECT n.role,n.stage,n.generation,n.branch_id,a.attempt_id FROM nodes n JOIN branch_attempts a ON a.branch_id=n.branch_id WHERE n.run_id=? AND n.branch_id=? AND a.attempt_id=?",
            &[run_id.clone(), branch_id.into(), attempt_id.into()],
        )?
        .first()
        .map(association)
        .transpose()
    }

    fn usage_attempts(&self, run_id: &Value) -> EngineResult<Vec<UsageAssociation>> {
        self.query_values(
            "SELECT a.attempt_id,a.branch_id,n.role,n.stage,n.generation FROM branch_attempts a JOIN nodes n ON n.branch_id=a.branch_id WHERE a.run_id=?",
            &[run_id.clone()],
        )?
        .iter()
        .map(association)
        .collect()
    }
}
