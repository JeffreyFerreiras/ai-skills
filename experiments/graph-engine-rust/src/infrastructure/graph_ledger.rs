use super::sqlite::{self, SqliteSession};
use crate::{
    application::{
        error::EngineResult,
        graph_ledger::{GraphLedgerReadPort, GraphRead},
    },
    compat::Value,
};

fn statement(query: GraphRead) -> &'static str {
    match query {
        GraphRead::Run => "SELECT * FROM runs WHERE run_id=?",
        GraphRead::ExecutionPlan => "SELECT * FROM execution_plans WHERE run_id=?",
        GraphRead::ExecutionPlanBody => "SELECT plan_json FROM execution_plans WHERE run_id=?",
        GraphRead::ExecutionPlanSummary => "SELECT size,plan_digest,status FROM execution_plans WHERE run_id=?",
        GraphRead::ExecutionPlanStatus => "SELECT status FROM execution_plans WHERE run_id=?",
        GraphRead::Artifact => "SELECT * FROM artifacts WHERE ref=?",
        GraphRead::LatestSucceededContextNode => "SELECT * FROM nodes WHERE run_id=? AND node_key=? AND status='succeeded' ORDER BY generation DESC LIMIT 1",
        GraphRead::SucceededNodeForGeneration => "SELECT * FROM nodes WHERE run_id=? AND node_key=? AND generation=? AND status='succeeded'",
        GraphRead::NodesForStatus => "SELECT * FROM nodes WHERE run_id=? ORDER BY branch_id",
        GraphRead::JoinsForStatus => "SELECT join_id,join_key,kind,generation,status,degraded FROM joins WHERE run_id=? ORDER BY join_id",
        GraphRead::BudgetsForStatus => "SELECT budget_id,limit_value,used FROM budgets WHERE run_id=? ORDER BY budget_id",
        GraphRead::ApprovalsForStatus => "SELECT a.approval_id,a.scope_ref,a.decision,a.authority_ref,a.artifact_sha256,aa.actor,aa.approved_at FROM approvals a JOIN approval_attestations aa ON aa.run_id=a.run_id AND aa.approval_id=a.approval_id WHERE a.run_id=? ORDER BY a.approval_id",
        GraphRead::AcceptanceForStatus => "SELECT criterion_id,artifact_ref,artifact_sha256 FROM acceptance_evidence WHERE run_id=? ORDER BY criterion_id",
        GraphRead::AcceptanceForCriterion => "SELECT * FROM acceptance_evidence WHERE run_id=? AND criterion_id=?",
        GraphRead::OpenClosure => "SELECT * FROM joins WHERE run_id=? AND join_key='closure' AND status='open' ORDER BY generation DESC LIMIT 1",
        GraphRead::ChecksForStatus => "SELECT check_id,outcome,artifact_ref,artifact_sha256 FROM check_evidence WHERE run_id=? ORDER BY check_id",
        GraphRead::CheckEvidenceForRun => "SELECT * FROM check_evidence WHERE run_id=? AND check_id=?",
        GraphRead::FanoutsForStatus => "SELECT fanout_id,stage,generation,status,member_branch_ids_json,assessment_ref,assessment_digest,authority_ref,actor,host_identity,assessed_at FROM fanouts WHERE run_id=? ORDER BY fanout_id",
        GraphRead::FanoutForRun => "SELECT * FROM fanouts WHERE run_id=? AND fanout_id=?",
        GraphRead::ReviewRequestsForStatus => "SELECT request_slot_id,parent_branch_id,generation,round_number,request_digest,\n        preliminary_ref,preliminary_digest,status,child_member_tuples_json,dispatch_cost,created_at,sealed_at\n        FROM review_delegation_requests WHERE run_id=? ORDER BY request_slot_id",
        GraphRead::CheckEvidenceOrdered => "SELECT * FROM check_evidence WHERE run_id=? ORDER BY check_id",
        GraphRead::EligibleExecutorNodes => "SELECT * FROM nodes WHERE run_id=? AND status IN ('running','succeeded') ORDER BY branch_id",
        GraphRead::AwaitingFanout => "SELECT fanout_id FROM fanouts WHERE run_id=? AND status='awaiting' ORDER BY fanout_id LIMIT 1",
        GraphRead::AwaitingFanoutScope => "SELECT fanout_id,stage,generation FROM fanouts WHERE run_id=? AND status='awaiting' ORDER BY fanout_id LIMIT 1",
        GraphRead::AwaitingReview => "SELECT f.request_slot_id FROM review_delegation_fanouts f\n            JOIN review_delegation_requests r USING(request_slot_id)\n            WHERE r.run_id=? AND f.status='awaiting' ORDER BY f.request_slot_id LIMIT 1",
        GraphRead::AwaitingReviewScope => "SELECT f.request_slot_id,r.parent_branch_id,r.round_number\n        FROM review_delegation_fanouts f JOIN review_delegation_requests r USING(request_slot_id)\n        WHERE r.run_id=? AND f.status='awaiting' ORDER BY f.request_slot_id LIMIT 1",
        GraphRead::FirstReadyNode => "SELECT * FROM nodes WHERE run_id=? AND status='ready' ORDER BY branch_id LIMIT 1",
        GraphRead::FirstReadyBranch => "SELECT branch_id FROM nodes WHERE run_id=? AND status='ready' ORDER BY branch_id LIMIT 1",
        GraphRead::OpenJoins => "SELECT * FROM joins WHERE run_id=? AND status='open' ORDER BY join_id",
        GraphRead::RetryableReviewChild => "SELECT branch_id FROM nodes WHERE run_id=? AND depth=1\n        AND status IN ('failed','timed_out') AND retry_count<max_retries ORDER BY branch_id LIMIT 1",
        GraphRead::FirstFailedRoot => "SELECT * FROM nodes WHERE run_id=? AND depth=0 AND status IN ('failed','timed_out') ORDER BY branch_id LIMIT 1",
        GraphRead::RunningBranches => "SELECT branch_id,envelope_json FROM nodes WHERE run_id=? AND status='running' ORDER BY branch_id",
        GraphRead::RootBranches => "SELECT * FROM nodes WHERE run_id=? AND depth=0 ORDER BY branch_id",
        GraphRead::BranchSealedForControl => "SELECT 1 FROM join_members jm JOIN joins j ON j.join_id=jm.join_id WHERE jm.branch_id=? AND j.status='sealed'",
        GraphRead::BudgetForUse => "SELECT * FROM budgets WHERE run_id=? AND budget_id=?",
        GraphRead::BranchForBudgetUse => "SELECT 1 FROM nodes WHERE branch_id=?",
        GraphRead::BudgetUseConflict => "SELECT 1 FROM budget_consumptions WHERE run_id=? AND budget_id=? AND source_branch_id=?",
        GraphRead::ExistingApproval => "SELECT * FROM approvals WHERE run_id=? AND approval_id=?",
        GraphRead::ReviewSettlementMembers => "SELECT m.assignment_id,m.ordinal,n.* FROM review_delegation_members m JOIN nodes n ON n.branch_id=m.child_branch_id WHERE m.request_slot_id=? ORDER BY m.assignment_id,m.ordinal",
        GraphRead::ReviewSettlementArtifact => "SELECT kind,ref,sha256 FROM artifacts WHERE ref=?",
        GraphRead::ReviewSettlementAttempts => "SELECT attempt_number,attempt_id,claim_digest,started_at,finished_at,outcome FROM branch_attempts WHERE branch_id=? ORDER BY attempt_number",
        GraphRead::Node => "SELECT * FROM nodes WHERE branch_id=?",
        GraphRead::OperationResponses => "SELECT response_json FROM operations WHERE run_id=? ORDER BY resulting_revision",
        GraphRead::ArtifactContent => "SELECT content_json FROM artifacts WHERE ref=?",
        GraphRead::FenceNode => "SELECT * FROM nodes WHERE run_id=? AND branch_id=?",
        GraphRead::FenceAttempt => "SELECT * FROM branch_attempts WHERE run_id=? AND branch_id=? AND attempt_id=?",
        GraphRead::LedgerArtifacts => "SELECT * FROM artifacts WHERE run_id=? AND source_type='ledger'",
        GraphRead::ActivationPlanDigest => "SELECT plan_digest FROM execution_plans WHERE run_id=?",
        GraphRead::ReservationEvents => "SELECT event_type,detail_json FROM events WHERE run_id=? ORDER BY revision",
        GraphRead::DeliveryReviewEnvelopes => "SELECT envelope_json FROM nodes WHERE run_id=? AND stage='delivery' AND generation=? AND depth=0",
        GraphRead::TimingAttemptIntervals => "SELECT started_at,finished_at FROM branch_attempts WHERE branch_id=? ORDER BY attempt_number",
        GraphRead::TimingAttemptStarts => "SELECT started_at FROM branch_attempts WHERE branch_id=?",
        GraphRead::TimingAttempts => "SELECT * FROM branch_attempts WHERE branch_id=? ORDER BY attempt_number",
        GraphRead::Join => "SELECT * FROM joins WHERE join_id=?",
        GraphRead::JoinForRun => "SELECT * FROM joins WHERE join_id=? AND run_id=?",
        GraphRead::JoinForKeyGeneration => "SELECT * FROM joins WHERE run_id=? AND join_key=? AND generation=?",
        GraphRead::JoinMembers => "SELECT n.*,jm.mandatory AS join_mandatory FROM join_members jm\n        JOIN nodes n ON n.branch_id=jm.branch_id WHERE jm.join_id=? ORDER BY n.branch_id",
        GraphRead::FanoutDependencyEdges => "SELECT d.before_branch_id,d.after_branch_id FROM fanout_dependencies d\n        JOIN fanouts f ON f.fanout_id=d.fanout_id WHERE f.run_id=?",
        GraphRead::NodesForValidation => "SELECT * FROM nodes WHERE run_id=?",
        GraphRead::JoinsForValidation => "SELECT * FROM joins WHERE run_id=?",
        GraphRead::SealedReviewCollections => "SELECT r.request_slot_id,c.collection_digest,c.content_json FROM review_delegation_requests r\n        JOIN review_delegation_collections c USING(request_slot_id)\n        WHERE r.parent_branch_id=? AND r.generation=? AND r.status='sealed' ORDER BY r.round_number",
        GraphRead::DeliveryNestedCollections => "SELECT c.collection_ref,c.collection_digest,c.content_json,a.kind,a.size_bytes FROM review_delegation_requests r\n        JOIN review_delegation_collections c USING(request_slot_id) LEFT JOIN artifacts a ON a.ref=c.collection_ref\n        WHERE r.parent_branch_id=? AND r.generation=COALESCE(?,r.generation) AND r.status='sealed' ORDER BY r.round_number",
        GraphRead::ReviewRequest => "SELECT * FROM review_delegation_requests WHERE request_slot_id=?",
        GraphRead::ReviewMember => "SELECT * FROM review_delegation_members WHERE request_slot_id=? AND assignment_id=? AND ordinal=?",
        GraphRead::ImmutableArtifact => "SELECT * FROM artifacts WHERE ref=? AND immutable=1",
        GraphRead::AttemptsForValidation => "SELECT * FROM branch_attempts WHERE run_id=? AND branch_id=? ORDER BY attempt_number",
        GraphRead::BudgetsForValidation => "SELECT * FROM budgets WHERE run_id=?",
        GraphRead::BudgetConsumed => "SELECT COALESCE(SUM(amount),0) FROM budget_consumptions WHERE run_id=? AND budget_id=?",
        GraphRead::InvalidBudgetConsumption => "SELECT 1 FROM budget_consumptions bc LEFT JOIN nodes n ON n.branch_id=bc.source_branch_id\n        WHERE bc.run_id=? AND (bc.amount<=0 OR n.run_id IS NULL OR n.run_id<>bc.run_id) LIMIT 1",
        GraphRead::OperationsForValidation => "SELECT * FROM operations WHERE run_id=? ORDER BY resulting_revision",
        GraphRead::JoinMemberCount => "SELECT COUNT(*) FROM join_members WHERE join_id=?",
        GraphRead::ReviewChildBranches => "SELECT child_branch_id FROM review_delegation_members WHERE request_slot_id=? ORDER BY child_branch_id",
        GraphRead::ReviewPredecessors => "SELECT n.* FROM review_delegation_dependencies d JOIN nodes n ON n.branch_id=d.before_branch_id\n            WHERE d.request_slot_id=? AND d.after_branch_id=?",
        GraphRead::AssessedFanouts => "SELECT * FROM fanouts WHERE run_id=? AND status='assessed' ORDER BY fanout_id",
        GraphRead::FanoutPredecessors => "SELECT n.* FROM fanout_dependencies d JOIN nodes n ON n.branch_id=d.before_branch_id\n                WHERE d.fanout_id=? AND d.after_branch_id=?",
        GraphRead::EvidenceActivationEvents => "SELECT * FROM events WHERE run_id=? AND event_type='evidence.enable'",
        GraphRead::EvidenceOperationEvents => "SELECT e.*,o.request_digest,o.response_json FROM events e JOIN operations o ON o.run_id=e.run_id AND o.operation_id=e.source_id WHERE e.run_id=? ORDER BY e.revision",
        GraphRead::AttemptClaimDigest => "SELECT claim_digest FROM branch_attempts WHERE run_id=? AND branch_id=? AND attempt_id=?",
        GraphRead::SelectedCheckArtifact => "SELECT artifact_ref FROM check_evidence WHERE run_id=? AND check_id=?",
        GraphRead::SealedDeliveryDependency => "SELECT * FROM joins WHERE run_id=? AND join_id=? AND kind='dependency' AND stage='delivery' AND status='sealed'",
        GraphRead::JoinHasMember => "SELECT 1 FROM join_members WHERE join_id=? AND branch_id=?",
        GraphRead::JoinAdvanceRequests => "SELECT detail_json FROM events WHERE run_id=? AND event_type='join.advance'",
        GraphRead::RepairOriginCollection => "SELECT * FROM joins WHERE run_id=? AND join_id=? AND join_key='delivery_collection' AND status='sealed'",
        GraphRead::ArtifactInRunDigest => "SELECT * FROM artifacts WHERE run_id=? AND ref=? AND sha256=?",
        GraphRead::ConstraintBundleForRun => "SELECT * FROM artifacts WHERE run_id=? AND sha256=? AND kind='acceptance_evidence'",
        GraphRead::ArtifactReferencesForRun => "SELECT ref FROM artifacts WHERE run_id=?",
        GraphRead::JoinMemberBranches => "SELECT branch_id FROM join_members WHERE join_id=?",
        GraphRead::ArtifactInRunContent => "SELECT content_json FROM artifacts WHERE run_id=? AND ref=?",
        GraphRead::AttemptExists => "SELECT 1 FROM branch_attempts WHERE run_id=? AND branch_id=? AND attempt_id=?",
        GraphRead::UnderlyingAcceptanceArtifact => "SELECT 1 FROM artifacts WHERE run_id=? AND ref=? AND sha256=? AND kind='acceptance_evidence'",
        GraphRead::TopologyNodeIds => "SELECT branch_id FROM nodes WHERE run_id=?",
        GraphRead::TopologyJoinIds => "SELECT join_id FROM joins WHERE run_id=?",
        GraphRead::TopologyFanoutIds => "SELECT fanout_id FROM fanouts WHERE run_id=?",
        GraphRead::ResearchJoinGenerations => "SELECT join_id,generation FROM joins WHERE run_id=? AND join_key='research_collection'",
        GraphRead::ImpactMapper => "SELECT * FROM nodes WHERE run_id=? AND node_key='impact_mapper'",
        GraphRead::JoinStatus => "SELECT status FROM joins WHERE join_id=?",
        GraphRead::SucceededNodes => "SELECT * FROM nodes WHERE run_id=? AND status='succeeded'",
        GraphRead::SealedJoins => "SELECT * FROM joins WHERE run_id=? AND status='sealed'",
        GraphRead::CollectionArtifactRefs => "SELECT ref FROM artifacts WHERE run_id=? AND kind='collection' AND ref LIKE ?",
        GraphRead::NodeEnvelope => "SELECT envelope_json FROM nodes WHERE branch_id=?",
        GraphRead::SealedClosureJoins => "SELECT * FROM joins WHERE run_id=? AND join_key='closure' AND status='sealed'",
        GraphRead::AnyJoin => "SELECT 1 FROM joins WHERE run_id=? LIMIT 1",
        GraphRead::SealedConsolidationJoins => "SELECT * FROM joins WHERE run_id=? AND join_key=? AND status='sealed' ORDER BY generation",
        GraphRead::ResearchJoinStatus => "SELECT status FROM joins WHERE run_id=? AND join_key='research_collection' AND generation=?",
        GraphRead::CollectionJoins => "SELECT * FROM joins WHERE run_id=? AND kind='collection'",
        GraphRead::FanoutsForValidation => "SELECT * FROM fanouts WHERE run_id=?",
        GraphRead::FanoutDependencies => "SELECT before_branch_id,after_branch_id,reason FROM fanout_dependencies WHERE fanout_id=? ORDER BY before_branch_id,after_branch_id,reason",
        GraphRead::NodeStatus => "SELECT status FROM nodes WHERE branch_id=?",
        GraphRead::ReviewPlan => "SELECT plan_json,plan_digest FROM execution_plans WHERE run_id=?",
        GraphRead::ReviewRequestsForValidation => "SELECT * FROM review_delegation_requests WHERE run_id=? ORDER BY parent_branch_id,generation,round_number",
        GraphRead::ReviewParentAttempt => "SELECT * FROM branch_attempts WHERE branch_id=? AND attempt_id=?",
        GraphRead::ReviewRequestRounds => "SELECT status FROM review_delegation_requests WHERE parent_branch_id=? AND generation=? ORDER BY round_number",
        GraphRead::ReviewDispatchUsage => "SELECT (SELECT COUNT(*) FROM review_delegation_members m JOIN review_delegation_requests r USING(request_slot_id) WHERE r.run_id=?1) AS children, COALESCE(SUM(dispatch_cost),0) AS cost FROM review_delegation_requests WHERE run_id=?1",
        GraphRead::DelegationOperations => "SELECT * FROM operations WHERE run_id=? AND request_digest=?",
        GraphRead::ReviewMembersForValidation => "SELECT * FROM review_delegation_members WHERE request_slot_id=? ORDER BY assignment_id,ordinal",
        GraphRead::ReviewFanout => "SELECT * FROM review_delegation_fanouts WHERE request_slot_id=?",
        GraphRead::ReviewCollection => "SELECT * FROM review_delegation_collections WHERE request_slot_id=?",
        GraphRead::ReviewDependencies => "SELECT before_branch_id,after_branch_id,reason FROM review_delegation_dependencies WHERE request_slot_id=? ORDER BY before_branch_id,after_branch_id",
        GraphRead::ArtifactKindDigest => "SELECT kind,sha256 FROM artifacts WHERE ref=?",
        GraphRead::DelegatedResultArtifact => "SELECT kind,ref,sha256 FROM artifacts WHERE ref=?",
        GraphRead::DelegatedMemberAttempts => "SELECT attempt_number,attempt_id,claim_digest,started_at,finished_at,outcome FROM branch_attempts WHERE branch_id=? ORDER BY attempt_number",
        GraphRead::AcceptanceForValidation => "SELECT * FROM acceptance_evidence WHERE run_id=?",
        GraphRead::ChecksForValidation => "SELECT * FROM check_evidence WHERE run_id=?",
        GraphRead::ApprovalsForValidation => "SELECT * FROM approvals WHERE run_id=?",
        GraphRead::ApprovalAttestation => "SELECT * FROM approval_attestations WHERE run_id=? AND approval_id=?",
        GraphRead::ArtifactsForValidation => "SELECT * FROM artifacts WHERE run_id=?",
        GraphRead::ArtifactExists => "SELECT 1 FROM artifacts WHERE ref=?",
        GraphRead::DispatchTask => "SELECT task_json FROM runs WHERE run_id=?",
        GraphRead::DispatchCollections => "SELECT r.request_slot_id,r.preliminary_ref,r.request_ref,c.collection_digest,c.content_json\n        FROM review_delegation_requests r JOIN review_delegation_collections c USING(request_slot_id)\n        WHERE r.parent_branch_id=? AND r.generation=? AND r.status='sealed' ORDER BY r.round_number",
        GraphRead::ReadyEnvelopes => "SELECT envelope_json FROM nodes WHERE run_id=? AND status='ready' ORDER BY branch_id",
        GraphRead::ImmutableArtifactContent => "SELECT content_json FROM artifacts WHERE ref=? AND immutable=1",
        GraphRead::GeneratedProducerResults => "SELECT result_json FROM nodes WHERE run_id=? AND node_key=? AND generation=? AND status='succeeded'",
        GraphRead::GeneratedAcceptanceWrappers => "SELECT a.content_json FROM acceptance_evidence e JOIN artifacts a ON a.ref=e.artifact_ref WHERE e.run_id=?",
        GraphRead::SealedBranchMembership => "SELECT 1 FROM joins j JOIN join_members m USING(join_id) WHERE m.branch_id=? AND j.status='sealed'",
        GraphRead::ClaimFanoutDependency => "SELECT 1 FROM fanout_dependencies d JOIN fanouts f ON f.fanout_id=d.fanout_id\n            JOIN nodes predecessor ON predecessor.branch_id=d.before_branch_id\n            WHERE f.run_id=? AND d.after_branch_id=? AND\n              NOT (predecessor.status IN ('succeeded','skipped') OR\n                (predecessor.status IN ('failed','timed_out') AND predecessor.retry_count>=predecessor.max_retries))\n            LIMIT 1",
        GraphRead::ClaimReviewDependency => "SELECT 1 FROM review_delegation_dependencies d\n            JOIN nodes predecessor ON predecessor.branch_id=d.before_branch_id\n            WHERE d.after_branch_id=? AND\n              NOT (predecessor.status IN ('succeeded','skipped') OR\n                (predecessor.status IN ('failed','timed_out') AND predecessor.retry_count>=predecessor.max_retries))\n            LIMIT 1",
        GraphRead::NextAttemptNumber => "SELECT COALESCE(MAX(attempt_number),0)+1 AS attempt_number FROM branch_attempts WHERE branch_id=?",
    }
}

impl GraphLedgerReadPort for SqliteSession {
    fn first(&self, query: GraphRead, parameters: &[Value]) -> EngineResult<Option<Value>> {
        let mut statement = self
            .connection()
            .prepare(statement(query))
            .map_err(sqlite::sqlite_failure)?;
        let parameters = sqlite::parameters(parameters, statement.parameter_count())?;
        let mut rows = statement
            .query(rusqlite::params_from_iter(parameters.iter()))
            .map_err(sqlite::sqlite_failure)?;
        rows.next()
            .map_err(sqlite::sqlite_failure)?
            .map(sqlite::row_value)
            .transpose()
    }

    fn all(&self, query: GraphRead, parameters: &[Value]) -> EngineResult<Vec<Value>> {
        self.query_values(statement(query), parameters)
    }

    fn visit(
        &self,
        query: GraphRead,
        parameters: &[Value],
        visit: &mut dyn FnMut(Value) -> EngineResult<()>,
    ) -> EngineResult<()> {
        let mut statement = self
            .connection()
            .prepare(statement(query))
            .map_err(sqlite::sqlite_failure)?;
        let parameters = sqlite::parameters(parameters, statement.parameter_count())?;
        let mut rows = statement
            .query(rusqlite::params_from_iter(parameters.iter()))
            .map_err(sqlite::sqlite_failure)?;
        while let Some(row) = rows.next().map_err(sqlite::sqlite_failure)? {
            visit(sqlite::row_value(row)?)?;
        }
        Ok(())
    }
}
