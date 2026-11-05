use super::error::EngineResult;
use crate::compat::Value;

#[derive(Clone, Copy, Debug)]
pub enum GraphWrite {
    ReadyNode,
    PlanApproval,
    PlanApprovalRunState,
    StopRun,
    CompleteRun,
    AcceptanceEvidence,
    ReplaceAcceptanceEvidence,
    ClaimAttempt,
    ClaimNode,
    BranchResult,
    BranchControlResult,
    RetryNode,
    RecoverRun,
    FinishAttempt,
    SelectRoute,
    AwaitingFanout,
    FanoutAssessment,
    FanoutDependency,
    SealJoin,
    DesignGeneration,
    ImplementationGeneration,
    UpdateNodeEnvelope,
    ConsumeBudget,
    IncrementBudget,
    EnableEvidence,
    Approval,
    ApprovalAttestation,
    CheckEvidence,
    SelectCheckEvidence,
    ReviewRequest,
    ReviewMember,
    AwaitingReviewFanout,
    OpenReviewCollection,
    WaitForReviewChildren,
    ReviewFanoutAssessment,
    ReviewDependency,
    StartReviewRequest,
    SealReviewCollection,
    SealReviewRequest,
    ResumeReviewParent,
}

pub trait GraphLedgerWritePort {
    fn write(&self, action: GraphWrite, parameters: &[Value]) -> EngineResult<()>;
}
