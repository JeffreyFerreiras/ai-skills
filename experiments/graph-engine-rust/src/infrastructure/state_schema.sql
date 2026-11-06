
CREATE TABLE runs (
  run_id TEXT PRIMARY KEY, repository_id TEXT NOT NULL, repository_digest TEXT NOT NULL,
  repository_path TEXT NOT NULL, repository_device INTEGER NOT NULL, repository_inode INTEGER NOT NULL,
  status TEXT NOT NULL, state_revision INTEGER NOT NULL, engine_version TEXT NOT NULL,
  state_schema_version INTEGER NOT NULL, policy_digest TEXT NOT NULL, policy_json TEXT NOT NULL,
  task_digest TEXT NOT NULL, task_ref TEXT NOT NULL, task_path TEXT NOT NULL, task_json TEXT NOT NULL,
  request_mode TEXT NOT NULL, minimum_route TEXT NOT NULL, selected_route TEXT,
  selected_tags_json TEXT, design_generation INTEGER NOT NULL DEFAULT 0,
  implementation_generation INTEGER NOT NULL DEFAULT 0, durability TEXT NOT NULL,
  durability_detail TEXT NOT NULL, permission_verification TEXT NOT NULL,
  degraded_permissions_ack INTEGER NOT NULL, degraded_durability_ack INTEGER NOT NULL,
  journal_mode TEXT NOT NULL, synchronous TEXT NOT NULL, sqlite_version TEXT NOT NULL,
  local_filesystem TEXT NOT NULL, host_identity TEXT NOT NULL, database_device INTEGER,
  database_inode INTEGER, blocked_reason TEXT, authoritative INTEGER NOT NULL
  ,started_at TEXT NOT NULL, finished_at TEXT
);
CREATE TABLE execution_plans (
  run_id TEXT PRIMARY KEY REFERENCES runs(run_id), size TEXT NOT NULL,
  plan_json TEXT NOT NULL, plan_digest TEXT NOT NULL, status TEXT NOT NULL,
  authority_ref TEXT, approved_at TEXT, approved_by TEXT, approval_digest TEXT
);
CREATE TABLE nodes (
  branch_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(run_id), node_instance_id TEXT NOT NULL UNIQUE,
  node_key TEXT NOT NULL, role TEXT NOT NULL, stage TEXT NOT NULL, generation INTEGER NOT NULL,
  mandatory INTEGER NOT NULL, specialist_tag TEXT, status TEXT NOT NULL, retry_count INTEGER NOT NULL,
  max_retries INTEGER NOT NULL, envelope_json TEXT NOT NULL, result_json TEXT, result_digest TEXT,
  failure_code TEXT, reason_code TEXT, started_at TEXT, finished_at TEXT,
  parent_branch_id TEXT REFERENCES nodes(branch_id), request_slot_id TEXT,
  assignment_id TEXT, ordinal INTEGER, depth INTEGER NOT NULL DEFAULT 0,
  UNIQUE(run_id,node_key,stage,generation,specialist_tag)
);
CREATE TABLE joins (
  join_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(run_id), join_key TEXT NOT NULL,
  kind TEXT NOT NULL, stage TEXT NOT NULL, generation INTEGER NOT NULL, status TEXT NOT NULL,
  degraded INTEGER NOT NULL DEFAULT 0, result_json TEXT,
  UNIQUE(run_id,join_key,generation)
);
CREATE TABLE join_members (
  join_id TEXT NOT NULL REFERENCES joins(join_id), branch_id TEXT NOT NULL REFERENCES nodes(branch_id),
  mandatory INTEGER NOT NULL, PRIMARY KEY(join_id,branch_id)
);
CREATE TABLE budgets (
  run_id TEXT NOT NULL REFERENCES runs(run_id), budget_id TEXT NOT NULL, limit_value INTEGER NOT NULL,
  used INTEGER NOT NULL, PRIMARY KEY(run_id,budget_id)
);
CREATE TABLE budget_consumptions (
  run_id TEXT NOT NULL REFERENCES runs(run_id), budget_id TEXT NOT NULL,
  source_branch_id TEXT NOT NULL REFERENCES nodes(branch_id), amount INTEGER NOT NULL,
  PRIMARY KEY(run_id,budget_id,source_branch_id)
);
CREATE TABLE operations (
  run_id TEXT NOT NULL REFERENCES runs(run_id), operation_id TEXT NOT NULL, request_digest TEXT NOT NULL,
  response_json TEXT NOT NULL, resulting_revision INTEGER NOT NULL,
  PRIMARY KEY(run_id,operation_id)
);
CREATE TABLE approvals (
  run_id TEXT NOT NULL REFERENCES runs(run_id), approval_id TEXT NOT NULL, scope_ref TEXT NOT NULL,
  decision TEXT NOT NULL, authority_ref TEXT NOT NULL, artifact_sha256 TEXT NOT NULL,
  PRIMARY KEY(run_id,approval_id)
);
CREATE TABLE approval_attestations (
  run_id TEXT NOT NULL REFERENCES runs(run_id), approval_id TEXT NOT NULL,
  actor TEXT NOT NULL, host_identity TEXT NOT NULL, approved_at TEXT NOT NULL,
  approval_digest TEXT NOT NULL, PRIMARY KEY(run_id,approval_id),
  FOREIGN KEY(run_id,approval_id) REFERENCES approvals(run_id,approval_id)
);
CREATE TABLE acceptance_evidence (
  run_id TEXT NOT NULL REFERENCES runs(run_id), criterion_id TEXT NOT NULL, artifact_ref TEXT NOT NULL,
  artifact_sha256 TEXT NOT NULL, PRIMARY KEY(run_id,criterion_id)
);
CREATE TABLE check_evidence (
  run_id TEXT NOT NULL REFERENCES runs(run_id), check_id TEXT NOT NULL, outcome TEXT NOT NULL,
  artifact_ref TEXT NOT NULL, artifact_sha256 TEXT NOT NULL, PRIMARY KEY(run_id,check_id)
);
CREATE TABLE events (
  event_id INTEGER PRIMARY KEY AUTOINCREMENT, run_id TEXT NOT NULL REFERENCES runs(run_id),
  revision INTEGER NOT NULL, event_type TEXT NOT NULL, source_id TEXT, detail_json TEXT NOT NULL
);
CREATE TABLE artifacts (
  ref TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(run_id), kind TEXT NOT NULL,
  sha256 TEXT NOT NULL, size_bytes INTEGER NOT NULL, source_type TEXT NOT NULL,
  source_path TEXT, content_json TEXT, device INTEGER, inode INTEGER, immutable INTEGER NOT NULL,
  UNIQUE(run_id,kind,sha256,ref)
);
CREATE TABLE fanouts (
  fanout_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
  stage TEXT NOT NULL, generation INTEGER NOT NULL CHECK(generation>=0),
  status TEXT NOT NULL CHECK(status IN ('awaiting','assessed')),
  member_branch_ids_json TEXT NOT NULL,
  assessment_ref TEXT REFERENCES artifacts(ref) ON DELETE RESTRICT,
  assessment_digest TEXT, authority_ref TEXT, actor TEXT, host_identity TEXT, assessed_at TEXT,
  UNIQUE(run_id,stage,generation),
  CHECK(
    (status='awaiting' AND assessment_ref IS NULL AND assessment_digest IS NULL AND authority_ref IS NULL
      AND actor IS NULL AND host_identity IS NULL AND assessed_at IS NULL)
    OR
    (status='assessed' AND assessment_ref IS NOT NULL AND assessment_digest IS NOT NULL
      AND authority_ref IS NOT NULL AND actor IS NOT NULL AND host_identity IS NOT NULL AND assessed_at IS NOT NULL)
  )
);
CREATE TABLE fanout_dependencies (
  fanout_id TEXT NOT NULL REFERENCES fanouts(fanout_id) ON DELETE RESTRICT,
  before_branch_id TEXT NOT NULL REFERENCES nodes(branch_id) ON DELETE RESTRICT,
  after_branch_id TEXT NOT NULL REFERENCES nodes(branch_id) ON DELETE RESTRICT,
  reason TEXT NOT NULL,
  PRIMARY KEY(fanout_id,before_branch_id,after_branch_id),
  CHECK(before_branch_id<>after_branch_id)
);
CREATE TABLE branch_attempts (
  run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
  branch_id TEXT NOT NULL REFERENCES nodes(branch_id) ON DELETE RESTRICT,
  attempt_number INTEGER NOT NULL CHECK(attempt_number>0), attempt_id TEXT NOT NULL,
  claim_digest TEXT NOT NULL CHECK(length(claim_digest)=64), started_at TEXT NOT NULL,
  finished_at TEXT, outcome TEXT,
  PRIMARY KEY(branch_id,attempt_number), UNIQUE(run_id,attempt_id),
  CHECK((finished_at IS NULL AND outcome IS NULL) OR (finished_at IS NOT NULL AND outcome IS NOT NULL))
);
CREATE TABLE review_delegation_requests (
  request_slot_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
  parent_branch_id TEXT NOT NULL REFERENCES nodes(branch_id) ON DELETE RESTRICT,
  parent_attempt_id TEXT NOT NULL, parent_claim_digest TEXT NOT NULL,
  generation INTEGER NOT NULL, round_number INTEGER NOT NULL,
  request_digest TEXT NOT NULL, preliminary_ref TEXT NOT NULL REFERENCES artifacts(ref),
  preliminary_digest TEXT NOT NULL, request_ref TEXT NOT NULL REFERENCES artifacts(ref),
  authority_ref TEXT NOT NULL, status TEXT NOT NULL,
  child_member_tuples_json TEXT NOT NULL, dispatch_cost INTEGER NOT NULL,
  created_at TEXT NOT NULL, sealed_at TEXT,
  UNIQUE(parent_branch_id,generation,round_number)
);
CREATE TABLE review_delegation_members (
  request_slot_id TEXT NOT NULL REFERENCES review_delegation_requests(request_slot_id) ON DELETE RESTRICT,
  assignment_id TEXT NOT NULL, ordinal INTEGER NOT NULL,
  child_branch_id TEXT NOT NULL UNIQUE REFERENCES nodes(branch_id) ON DELETE RESTRICT,
  PRIMARY KEY(request_slot_id,assignment_id,ordinal)
);
CREATE TABLE review_delegation_fanouts (
  request_slot_id TEXT PRIMARY KEY REFERENCES review_delegation_requests(request_slot_id) ON DELETE RESTRICT,
  status TEXT NOT NULL, assessment_ref TEXT REFERENCES artifacts(ref), assessment_digest TEXT,
  authority_ref TEXT, actor TEXT, host_identity TEXT, assessed_at TEXT
);
CREATE TABLE review_delegation_dependencies (
  request_slot_id TEXT NOT NULL REFERENCES review_delegation_requests(request_slot_id) ON DELETE RESTRICT,
  before_branch_id TEXT NOT NULL REFERENCES nodes(branch_id) ON DELETE RESTRICT,
  after_branch_id TEXT NOT NULL REFERENCES nodes(branch_id) ON DELETE RESTRICT,
  reason TEXT NOT NULL, PRIMARY KEY(request_slot_id,before_branch_id,after_branch_id)
);
CREATE TABLE review_delegation_collections (
  request_slot_id TEXT PRIMARY KEY REFERENCES review_delegation_requests(request_slot_id) ON DELETE RESTRICT,
  status TEXT NOT NULL, collection_ref TEXT REFERENCES artifacts(ref), collection_digest TEXT,
  content_json TEXT, sealed_at TEXT
);
CREATE TRIGGER fanout_transition_guard BEFORE UPDATE ON fanouts
BEGIN
  SELECT CASE
    WHEN OLD.status<>'awaiting' OR NEW.status<>'assessed'
      OR OLD.fanout_id<>NEW.fanout_id OR OLD.run_id<>NEW.run_id OR OLD.stage<>NEW.stage
      OR OLD.generation<>NEW.generation OR OLD.member_branch_ids_json<>NEW.member_branch_ids_json
    THEN RAISE(ABORT,'FANOUT_IMMUTABLE')
  END;
END;
CREATE TRIGGER fanout_insert_guard BEFORE INSERT ON fanouts WHEN NEW.status<>'awaiting'
BEGIN SELECT RAISE(ABORT,'FANOUT_IMMUTABLE'); END;
CREATE TRIGGER fanout_delete_guard BEFORE DELETE ON fanouts
BEGIN SELECT RAISE(ABORT,'FANOUT_IMMUTABLE'); END;
CREATE TRIGGER fanout_dependency_update_guard BEFORE UPDATE ON fanout_dependencies
BEGIN SELECT RAISE(ABORT,'FANOUT_DEPENDENCY_IMMUTABLE'); END;
CREATE TRIGGER fanout_dependency_insert_guard BEFORE INSERT ON fanout_dependencies
WHEN (SELECT status FROM fanouts WHERE fanout_id=NEW.fanout_id)<>'awaiting'
BEGIN SELECT RAISE(ABORT,'FANOUT_DEPENDENCY_IMMUTABLE'); END;
CREATE TRIGGER fanout_dependency_delete_guard BEFORE DELETE ON fanout_dependencies
BEGIN SELECT RAISE(ABORT,'FANOUT_DEPENDENCY_IMMUTABLE'); END;
CREATE TRIGGER branch_attempt_identity_guard BEFORE UPDATE ON branch_attempts
BEGIN
  SELECT CASE
    WHEN OLD.run_id<>NEW.run_id OR OLD.branch_id<>NEW.branch_id
      OR OLD.attempt_number<>NEW.attempt_number OR OLD.attempt_id<>NEW.attempt_id
      OR OLD.claim_digest<>NEW.claim_digest OR OLD.started_at<>NEW.started_at
      OR OLD.finished_at IS NOT NULL OR (NEW.finished_at IS NULL)<>(NEW.outcome IS NULL)
    THEN RAISE(ABORT,'ATTEMPT_IMMUTABLE')
  END;
END;
CREATE TRIGGER branch_attempt_delete_guard BEFORE DELETE ON branch_attempts
BEGIN SELECT RAISE(ABORT,'ATTEMPT_IMMUTABLE'); END;
CREATE TRIGGER review_request_update_guard BEFORE UPDATE ON review_delegation_requests
BEGIN
  SELECT CASE WHEN
    NOT ((OLD.status='awaiting_assessment' AND NEW.status='running' AND NEW.sealed_at IS NULL)
      OR (OLD.status='running' AND NEW.status='sealed' AND NEW.sealed_at IS NOT NULL))
    OR OLD.request_slot_id<>NEW.request_slot_id OR OLD.run_id<>NEW.run_id
    OR OLD.parent_branch_id<>NEW.parent_branch_id OR OLD.parent_attempt_id<>NEW.parent_attempt_id
    OR OLD.parent_claim_digest<>NEW.parent_claim_digest OR OLD.generation<>NEW.generation
    OR OLD.round_number<>NEW.round_number OR OLD.request_digest<>NEW.request_digest
    OR OLD.preliminary_ref<>NEW.preliminary_ref OR OLD.preliminary_digest<>NEW.preliminary_digest
    OR OLD.request_ref<>NEW.request_ref OR OLD.authority_ref<>NEW.authority_ref
    OR OLD.child_member_tuples_json<>NEW.child_member_tuples_json
    OR OLD.dispatch_cost<>NEW.dispatch_cost OR OLD.created_at<>NEW.created_at
    THEN RAISE(ABORT,'REVIEW_REQUEST_IMMUTABLE') END;
END;
CREATE TRIGGER review_request_delete_guard BEFORE DELETE ON review_delegation_requests
BEGIN SELECT RAISE(ABORT,'REVIEW_REQUEST_IMMUTABLE'); END;
CREATE TRIGGER review_member_update_guard BEFORE UPDATE ON review_delegation_members
BEGIN SELECT RAISE(ABORT,'REVIEW_MEMBER_IMMUTABLE'); END;
CREATE TRIGGER review_member_delete_guard BEFORE DELETE ON review_delegation_members
BEGIN SELECT RAISE(ABORT,'REVIEW_MEMBER_IMMUTABLE'); END;
CREATE TRIGGER review_dependency_update_guard BEFORE UPDATE ON review_delegation_dependencies
BEGIN SELECT RAISE(ABORT,'REVIEW_DEPENDENCY_IMMUTABLE'); END;
CREATE TRIGGER review_dependency_delete_guard BEFORE DELETE ON review_delegation_dependencies
BEGIN SELECT RAISE(ABORT,'REVIEW_DEPENDENCY_IMMUTABLE'); END;
CREATE TRIGGER review_collection_update_guard BEFORE UPDATE ON review_delegation_collections
BEGIN
  SELECT CASE WHEN OLD.status<>'open' OR NEW.status<>'sealed'
    OR OLD.request_slot_id<>NEW.request_slot_id OR NEW.collection_ref IS NULL
    OR NEW.collection_digest IS NULL OR NEW.content_json IS NULL OR NEW.sealed_at IS NULL
    THEN RAISE(ABORT,'REVIEW_COLLECTION_IMMUTABLE') END;
END;
CREATE TRIGGER review_collection_delete_guard BEFORE DELETE ON review_delegation_collections
BEGIN SELECT RAISE(ABORT,'REVIEW_COLLECTION_IMMUTABLE'); END;
CREATE TRIGGER review_fanout_update_guard BEFORE UPDATE ON review_delegation_fanouts
BEGIN
  SELECT CASE WHEN OLD.status<>'awaiting' OR NEW.status<>'assessed'
    OR OLD.request_slot_id<>NEW.request_slot_id OR NEW.assessment_ref IS NULL
    OR NEW.assessment_digest IS NULL OR NEW.authority_ref IS NULL OR NEW.actor IS NULL
    OR NEW.host_identity IS NULL OR NEW.assessed_at IS NULL
    THEN RAISE(ABORT,'REVIEW_FANOUT_IMMUTABLE') END;
END;
CREATE TRIGGER review_fanout_delete_guard BEFORE DELETE ON review_delegation_fanouts
BEGIN SELECT RAISE(ABORT,'REVIEW_FANOUT_IMMUTABLE'); END;
