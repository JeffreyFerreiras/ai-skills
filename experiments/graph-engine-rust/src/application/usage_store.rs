use super::{error::EngineResult, state::GraphSession};
use crate::compat::Value;
use num_bigint::BigInt;

#[derive(Clone, Debug)]
pub struct UsageObservation {
    pub detail: Value,
}

#[derive(Clone, Debug)]
pub struct BindingAssociation {
    pub role: Value,
    pub stage: Value,
    pub generation: Value,
}

#[derive(Clone, Debug)]
pub struct UsageAssociation {
    pub attempt_id: Value,
    pub branch_id: Value,
    pub role: Value,
    pub stage: Value,
    pub generation: Value,
}

impl UsageAssociation {
    pub fn value(&self) -> Value {
        Value::object([
            ("attempt_id", self.attempt_id.clone()),
            ("branch_id", self.branch_id.clone()),
            ("role", self.role.clone()),
            ("stage", self.stage.clone()),
            ("generation", self.generation.clone()),
        ])
    }
}

pub trait UsageSession: GraphSession {
    fn visit_usage_observations(
        &self,
        run_id: &Value,
        visit: &mut dyn FnMut(UsageObservation) -> EngineResult<()>,
    ) -> EngineResult<()>;
    fn usage_binding_association(
        &self,
        run_id: &Value,
        branch_id: &Value,
        attempt_id: &Value,
    ) -> EngineResult<Option<BindingAssociation>>;
    fn usage_record_association(
        &self,
        run_id: &Value,
        branch_id: &str,
        attempt_id: &str,
    ) -> EngineResult<Option<UsageAssociation>>;
    fn usage_attempts(&self, run_id: &Value) -> EngineResult<Vec<UsageAssociation>>;
    fn insert_usage_observation(
        &self,
        run_id: &str,
        revision: &BigInt,
        binding_id: &str,
        detail_json: &[u8],
    ) -> EngineResult<()> {
        self.insert_event(
            run_id,
            revision,
            "usage.observation",
            binding_id,
            detail_json,
        )
    }
}
