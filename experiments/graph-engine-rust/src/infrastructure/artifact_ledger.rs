use super::sqlite::SqliteSession;
use crate::{
    application::{artifacts::ArtifactLedger, error::EngineResult},
    compat::Value,
};

impl ArtifactLedger for SqliteSession {
    fn immutable_artifact(&self, reference: &Value, digest: &Value) -> EngineResult<Option<Value>> {
        Ok(self
            .query_values(
                "SELECT * FROM artifacts WHERE ref=? AND sha256=? AND immutable=1",
                &[reference.clone(), digest.clone()],
            )?
            .into_iter()
            .next())
    }
    fn artifact_by_reference(&self, reference: &Value) -> EngineResult<Option<Value>> {
        Ok(self
            .query_values("SELECT * FROM artifacts WHERE ref=?", &[reference.clone()])?
            .into_iter()
            .next())
    }
    fn insert_artifact_values(&self, values: &[Value]) -> EngineResult<()> {
        self.execute_values(
            "INSERT INTO artifacts VALUES(?,?,?,?,?,?,?,?,?,?,?)",
            values,
        )?;
        Ok(())
    }
}
