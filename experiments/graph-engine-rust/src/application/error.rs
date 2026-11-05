use crate::{
    compat::{JsonError, PyString},
    domain::{
        constraints::ConstraintValidationSite, contracts::ContractError, errors::ValidationFailure,
        state::StateError,
    },
};
use std::io;

#[derive(Clone, Copy, Debug)]
pub enum BranchResultTraceSite {
    ManifestAttempt,
    MapperStatus,
    MapperRoute,
    MapperEvidenceReferences,
    ContinuationFields,
    FindingsIterable,
    FindingId,
}

#[derive(Clone, Copy, Debug)]
pub enum ConstraintCommand {
    Preview,
    Initialize,
}

#[derive(Clone, Copy, Debug)]
pub enum ManifestJsonTraceSite {
    BranchResult,
    TerminalEvidence,
    Retry,
    FanoutAssessment,
    ReviewPreliminary,
    ReviewRequest,
    ReviewAssessment,
    EvidenceEnable,
    JoinAdvance,
    Recover,
    Block,
}

#[derive(Clone, Copy, Debug)]
pub enum PlanTraceCommand {
    Preview,
    Initialize,
}

#[derive(Clone, Copy, Debug)]
pub enum PlanSnapshot {
    Task,
    Allowance,
}

#[derive(Clone, Copy, Debug)]
pub enum PolicySnapshotSource {
    Legacy,
    Seed,
    Runtime,
}

#[derive(Clone, Copy, Debug)]
pub enum TaskValidationMutation {
    RunControl,
    BranchResult,
    NextClaim,
    RecordControl,
    Complete,
    JoinAdvance,
    CheckAbandon,
    Usage,
    FanoutAssessment,
    ReviewFanoutAssessment,
    EvidenceEnable,
    LegacyCheckRun,
    Recover,
    ReviewFanout,
    Format7CheckStart,
    Format7CheckFinish,
}

#[derive(Clone, Copy, Debug)]
pub enum MutationValidationPhase {
    BeforeAction,
    AfterAction,
}

#[derive(Clone, Copy, Debug)]
pub enum PersistedTaskValidation {
    ReadOnly,
    Format7CheckDirect,
    Mutation(TaskValidationMutation, MutationValidationPhase),
}

#[derive(Clone, Copy, Debug)]
pub enum SnapshotJsonTraceSite {
    Manifest(ManifestJsonTraceSite),
    Plan(PlanTraceCommand, PlanSnapshot),
    Policy(PolicySnapshotSource),
    ConstraintBundle,
    ConstraintContext,
    ActivatedBundle,
    ConstraintSupport(ConstraintCommand),
    StagedConsolidation,
    PersistedTask(PersistedTaskValidation),
}

#[derive(Clone, Copy, Debug)]
pub enum GraphctlTraceSite {
    BranchResult(BranchResultTraceSite),
    ConstraintSupportPayload(ConstraintCommand),
    ConstraintValidation(ConstraintCommand, ConstraintValidationSite),
    SnapshotJson(SnapshotJsonTraceSite),
}

#[derive(Debug)]
pub enum EngineError {
    Contract(ContractError),
    State(StateError),
    Helper(crate::domain::helper_register::HelperRegisterError),
    Json(JsonError),
    Io(io::Error),
    Python {
        kind: &'static str,
        message: PyString,
    },
    TracedGraphctl {
        site: GraphctlTraceSite,
        kind: &'static str,
        message: PyString,
    },
    TracedGraphctlJson {
        site: SnapshotJsonTraceSite,
        error: JsonError,
    },
    Storage {
        kind: &'static str,
        message: String,
        code: Option<i32>,
    },
    Injected(String),
    ProcessDeath(String),
}
impl From<ContractError> for EngineError {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}
impl From<StateError> for EngineError {
    fn from(error: StateError) -> Self {
        Self::State(error)
    }
}
impl From<JsonError> for EngineError {
    fn from(error: JsonError) -> Self {
        Self::Json(error)
    }
}
impl From<io::Error> for EngineError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<ValidationFailure> for EngineError {
    fn from(error: ValidationFailure) -> Self {
        match error {
            ValidationFailure::Contract(e) => Self::Contract(e),
            ValidationFailure::Python { kind, message } => Self::Python { kind, message },
        }
    }
}
impl EngineError {
    pub fn state(code: &str) -> Self {
        StateError::new(code).into()
    }
    pub fn python(kind: &'static str, message: impl Into<PyString>) -> Self {
        Self::Python {
            kind,
            message: message.into(),
        }
    }

    pub(super) fn with_branch_result_trace(self, site: BranchResultTraceSite) -> Self {
        self.with_graphctl_trace(GraphctlTraceSite::BranchResult(site))
    }

    pub(super) fn with_graphctl_trace(self, site: GraphctlTraceSite) -> Self {
        match self {
            Self::Python { kind, message } if matches!(kind, "AttributeError" | "TypeError") => {
                Self::TracedGraphctl {
                    site,
                    kind,
                    message,
                }
            }
            error => error,
        }
    }

    pub(super) fn manifest_json(error: JsonError, site: ManifestJsonTraceSite) -> Self {
        Self::Json(error).with_snapshot_trace(SnapshotJsonTraceSite::Manifest(site))
    }

    pub(super) fn with_snapshot_trace(self, site: SnapshotJsonTraceSite) -> Self {
        match self {
            Self::Json(error) if matches!(error.kind, "ValueError" | "RecursionError") => {
                Self::TracedGraphctlJson { site, error }
            }
            error => error,
        }
    }

    pub(super) fn without_graphctl_trace(self) -> Self {
        match self {
            Self::TracedGraphctl { kind, message, .. } => Self::Python { kind, message },
            Self::TracedGraphctlJson { error, .. } => Self::Json(error),
            error => error,
        }
    }
}
pub type EngineResult<T> = Result<T, EngineError>;
impl From<crate::domain::helper_register::HelperFailure> for EngineError {
    fn from(error: crate::domain::helper_register::HelperFailure) -> Self {
        match error {
            crate::domain::helper_register::HelperFailure::Validation(error) => error.into(),
            crate::domain::helper_register::HelperFailure::Register(error) => Self::Helper(error),
            crate::domain::helper_register::HelperFailure::Serialization(error) => error.into(),
        }
    }
}
impl From<crate::domain::helper_register::HelperRegisterError> for EngineError {
    fn from(error: crate::domain::helper_register::HelperRegisterError) -> Self {
        Self::Helper(error)
    }
}
impl From<crate::compat::integer::PythonIntegerError> for EngineError {
    fn from(error: crate::compat::integer::PythonIntegerError) -> Self {
        Self::Python {
            kind: error.kind,
            message: error.message,
        }
    }
}
impl From<crate::domain::usage_contracts::UsageFailure> for EngineError {
    fn from(error: crate::domain::usage_contracts::UsageFailure) -> Self {
        match error {
            crate::domain::usage_contracts::UsageFailure::State(code) => Self::state(code),
            crate::domain::usage_contracts::UsageFailure::Validation(error) => error.into(),
        }
    }
}
