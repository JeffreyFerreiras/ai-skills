use super::sqlite::SqliteSession;
use crate::{
    application::{
        error::EngineResult,
        graph_writes::{GraphLedgerWritePort, GraphWrite},
    },
    compat::Value,
};

impl GraphLedgerWritePort for SqliteSession {
    fn write(&self, action: GraphWrite, parameters: &[Value]) -> EngineResult<()> {
        let statement = match action {
            GraphWrite::ReadyNode => {
                "UPDATE nodes SET status='ready',envelope_json=? WHERE branch_id=?"
            }
            GraphWrite::PlanApproval => "UPDATE execution_plans SET status=?,authority_ref=?,approved_at=?,approved_by=?,approval_digest=? WHERE run_id=?",
            GraphWrite::PlanApprovalRunState => "UPDATE runs SET status=?,blocked_reason=?,finished_at=CASE WHEN ?='blocked' THEN COALESCE(finished_at,?) ELSE finished_at END WHERE run_id=?",
            GraphWrite::StopRun => "UPDATE runs SET status=?,blocked_reason=?,finished_at=COALESCE(finished_at,?) WHERE run_id=?",
            GraphWrite::CompleteRun => "UPDATE runs SET status='complete',finished_at=COALESCE(finished_at,?) WHERE run_id=?",
            GraphWrite::AcceptanceEvidence => "INSERT INTO acceptance_evidence VALUES(?,?,?,?)",
            GraphWrite::ReplaceAcceptanceEvidence => "INSERT OR REPLACE INTO acceptance_evidence VALUES(?,?,?,?)",
            GraphWrite::ClaimAttempt => "INSERT INTO branch_attempts(run_id,branch_id,attempt_number,attempt_id,claim_digest,started_at) VALUES(?,?,?,?,?,?)",
            GraphWrite::ClaimNode => "UPDATE nodes SET status='running',started_at=COALESCE(started_at,?),envelope_json=? WHERE branch_id=?",
            GraphWrite::BranchResult => "UPDATE nodes SET status=?,envelope_json=?,result_json=?,result_digest=?,failure_code=?,finished_at=? WHERE branch_id=?",
            GraphWrite::BranchControlResult => "UPDATE nodes SET status=?,reason_code=?,envelope_json=?,result_json=?,result_digest=?,finished_at=? WHERE branch_id=?",
            GraphWrite::RetryNode => "UPDATE nodes SET status='ready',envelope_json=?,retry_count=retry_count+1,result_json=NULL,result_digest=NULL,failure_code=NULL,reason_code=NULL,finished_at=NULL WHERE branch_id=?",
            GraphWrite::RecoverRun => "UPDATE runs SET status='active',blocked_reason=NULL,finished_at=NULL WHERE run_id=?",
            GraphWrite::FinishAttempt => "UPDATE branch_attempts SET finished_at=?,outcome=? WHERE branch_id=? AND attempt_number=?",
            GraphWrite::SelectRoute => "UPDATE runs SET status='active',selected_route=?,selected_tags_json=? WHERE run_id=?",
            GraphWrite::AwaitingFanout => "INSERT INTO fanouts(fanout_id,run_id,stage,generation,status,member_branch_ids_json) VALUES(?,?,?,?,'awaiting',?)",
            GraphWrite::FanoutAssessment => "UPDATE fanouts SET status='assessed',assessment_ref=?,assessment_digest=?,authority_ref=?,actor=?,host_identity=?,assessed_at=? WHERE fanout_id=?",
            GraphWrite::FanoutDependency => "INSERT INTO fanout_dependencies VALUES(?,?,?,?)",
            GraphWrite::SealJoin => "UPDATE joins SET status='sealed',result_json=? WHERE join_id=?",
            GraphWrite::DesignGeneration => "UPDATE runs SET design_generation=? WHERE run_id=?",
            GraphWrite::ImplementationGeneration => "UPDATE runs SET implementation_generation=? WHERE run_id=?",
            GraphWrite::UpdateNodeEnvelope => "UPDATE nodes SET envelope_json=? WHERE branch_id=?",
            GraphWrite::ConsumeBudget => "INSERT INTO budget_consumptions VALUES(?,?,?,?)",
            GraphWrite::IncrementBudget => "UPDATE budgets SET used=used+? WHERE run_id=? AND budget_id=?",
            GraphWrite::EnableEvidence => "UPDATE runs SET state_schema_version=7 WHERE run_id=?",
            GraphWrite::Approval => "INSERT INTO approvals VALUES(?,?,?,?,?,?)",
            GraphWrite::ApprovalAttestation => "INSERT INTO approval_attestations VALUES(?,?,?,?,?,?)",
            GraphWrite::CheckEvidence => "INSERT INTO check_evidence VALUES(?,?,?,?,?)",
            GraphWrite::SelectCheckEvidence => "INSERT OR REPLACE INTO check_evidence VALUES(?,?,?,?,?)",
            GraphWrite::ReviewRequest => "INSERT INTO review_delegation_requests VALUES(?,?,?,?,?,?,?,?,?,?,?,?,'awaiting_assessment',?,?,?,NULL)",
            GraphWrite::ReviewMember => "INSERT INTO review_delegation_members VALUES(?,?,?,?)",
            GraphWrite::AwaitingReviewFanout => "INSERT INTO review_delegation_fanouts(request_slot_id,status) VALUES(?,'awaiting')",
            GraphWrite::OpenReviewCollection => "INSERT INTO review_delegation_collections(request_slot_id,status) VALUES(?,'open')",
            GraphWrite::WaitForReviewChildren => "UPDATE nodes SET status='waiting_for_review_children',envelope_json=?,finished_at=? WHERE branch_id=?",
            GraphWrite::ReviewFanoutAssessment => "UPDATE review_delegation_fanouts SET status='assessed',assessment_ref=?,assessment_digest=?,authority_ref=?,actor=?,host_identity=?,assessed_at=? WHERE request_slot_id=?",
            GraphWrite::ReviewDependency => "INSERT INTO review_delegation_dependencies VALUES(?,?,?,?)",
            GraphWrite::StartReviewRequest => "UPDATE review_delegation_requests SET status='running' WHERE request_slot_id=?",
            GraphWrite::SealReviewCollection => "UPDATE review_delegation_collections SET status='sealed',collection_ref=?,collection_digest=?,content_json=?,sealed_at=? WHERE request_slot_id=?",
            GraphWrite::SealReviewRequest => "UPDATE review_delegation_requests SET status='sealed',sealed_at=? WHERE request_slot_id=?",
            GraphWrite::ResumeReviewParent => "UPDATE nodes SET status='ready',envelope_json=?,finished_at=NULL WHERE branch_id=?",
        };
        self.execute_values(statement, parameters)?;
        Ok(())
    }
}
