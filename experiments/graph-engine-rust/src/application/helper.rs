use super::{
    error::EngineResult, helper_ledger::HelperReservationLedger,
    helper_repository::HelperRegisterRepository,
};
use crate::compat::Value;
use std::path::Path;

pub struct HelperRegister<'a> {
    pub repository: HelperRegisterRepository<'a>,
}

impl HelperRegister<'_> {
    pub fn initialize(
        &self,
        state_root: &Path,
        repository: &Path,
        run_id: &Value,
        plan: &Path,
        allowance: &Path,
        observation: &Path,
    ) -> EngineResult<Value> {
        self.repository
            .initialize(state_root, repository, run_id, plan, allowance, observation)
    }

    fn ledger(&self) -> HelperReservationLedger<'_> {
        HelperReservationLedger {
            repository: &self.repository,
            files: self.repository.files,
        }
    }

    pub fn preflight(
        &self,
        register: &Path,
        context: &Value,
        request: &Value,
    ) -> EngineResult<Value> {
        self.ledger().preflight(register, context, request)
    }

    pub fn reserve(
        &self,
        register: &Path,
        context: &Value,
        request: &Value,
    ) -> EngineResult<Value> {
        self.ledger().reserve(register, context, request)
    }

    pub fn settle(
        &self,
        register: &Path,
        context: &Value,
        settlement: &Value,
    ) -> EngineResult<Value> {
        self.ledger().settle(register, context, settlement)
    }

    pub fn status(&self, register: &Path, context: &Value) -> EngineResult<Value> {
        self.ledger().status(register, context)
    }
}
