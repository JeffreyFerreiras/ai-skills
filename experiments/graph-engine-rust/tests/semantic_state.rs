#[path = "support/graph_case.rs"]
mod graph_case;
use graph_case::GraphFixture;
use graph_engine::application::{
    graph_plan_approval::record_plan_approval,
    graph_run_control::{abort_run, block_run},
    graph_writes::{GraphLedgerWritePort, GraphWrite},
    persistent_store::StateStore,
    state::GraphSession,
};
use graph_engine::{
    application::{
        artifacts::ArtifactResolver,
        check_receipts::CheckProvenancePort,
        error::{EngineError, EngineResult},
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        persistent_store::InitializeRequest,
        semantic_state::{verify_semantic_state, SemanticContext},
        snapshots::safe_json_snapshot_integer,
    },
    compat::{self, PyString, Value},
    domain::{
        evidence::VerifiedArtifact, execution, hosts, ids, planner, python_operations,
        task_contract,
    },
    infrastructure::{
        policy_files::NativePolicyFiles,
        sqlite::SqliteSession,
        state_platform::{self, NativeStatePlatform},
    },
};
use rusqlite::Connection;
use std::{fs, path::PathBuf};

fn json(value: &Value) -> String {
    String::from_utf8(ids::canonical_bytes(value).unwrap()).unwrap()
}
struct TestProvenance;
impl CheckProvenancePort for TestProvenance {
    fn actor(&self) -> EngineResult<PyString> {
        state_platform::current_actor()
    }
    fn host_identity(&self) -> EngineResult<String> {
        graph_engine::application::persistent_store::StatePlatform::host_identity(
            &NativeStatePlatform,
        )
    }
    fn repository_worktree_digest(&self) -> EngineResult<String> {
        panic!("bootstrap has no check receipts")
    }
}
fn artifact_value(artifact: &VerifiedArtifact) -> Value {
    Value::object([
        ("ref", artifact.reference.clone()),
        ("kind", artifact.kind.clone()),
        ("sha256", artifact.sha256.clone()),
        ("size_bytes", artifact.size_bytes.clone()),
        ("source_type", artifact.source_type.clone()),
        ("source_path", artifact.source_path.clone()),
        ("content_json", artifact.content_json.clone()),
        ("device", artifact.device.clone()),
        ("inode", artifact.inode.clone()),
    ])
}
struct Fixture {
    graph: GraphFixture,
    policy: Value,
    digest: String,
    task_path: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let graph = GraphFixture::new();
        let files = NativePolicyFiles::default();
        let policy = compat::loads(&graph.policy_bytes).unwrap();
        let digest = ids::sha256_bytes(&graph.policy_bytes);
        let raw = graph.task("delivery", "full_delivery", &[]);
        let task_path = graph.repository.join("docs/artifacts/task.json");
        fs::write(&task_path, ids::canonical_bytes(&raw).unwrap()).unwrap();
        let snapshot = safe_json_snapshot_integer(
            &files,
            &task_path,
            &[graph.repository.join("docs")],
            policy["artifact_kinds"]["task_brief"]["max_bytes"]
                .strict_integer()
                .unwrap(),
        )
        .unwrap();
        let full = task_contract::validate_task_brief(
            snapshot.parsed.as_ref().unwrap(),
            &digest.clone().into(),
            &policy,
            None,
            cfg!(windows),
        )
        .unwrap();
        let resolver = ArtifactResolver {
            files: &files,
            snapshots: &files,
            repository: &graph.repository,
            skill: &graph.skill_root,
            installation: &graph.skill_root,
            policy: &policy,
            runtime: Some(&graph.runtime_root),
            ledger: None,
        };
        let mut evidence = Vec::new();
        for reference in full["evidence_paths"].as_list().unwrap() {
            evidence.push(
                resolver
                    .resolve_unhashed_reference(reference, &"acceptance_evidence".into())
                    .unwrap(),
            );
        }
        let mut task = task_contract::authoritative_task_subset(&full)
            .unwrap()
            .string_keyed_value()
            .unwrap();
        let mut evidence_refs = evidence
            .iter()
            .map(|artifact| artifact.reference.clone())
            .collect::<Vec<_>>();
        python_operations::sort_comparable_values(&mut evidence_refs).unwrap();
        task.set("evidence_paths", Value::List(evidence_refs));
        let plan =
            execution::build_execution_plan("RUN", &task, None, hosts::DEFAULT_HOST).unwrap();
        let spec = planner::bootstrap(&policy).unwrap();
        let task_ref = Value::from(format!(
            "repo:docs/artifacts/task.json#sha256={}",
            snapshot.digest
        ));
        let input = Value::object([
            ("kind", "task_brief".into()),
            ("ref", task_ref.clone().into()),
            ("sha256", snapshot.digest.clone().into()),
            ("size_bytes", snapshot.size.into()),
        ]);
        let envelope = planner::envelope(
            "RUN",
            &digest,
            &policy,
            &task,
            &spec,
            "ready",
            &[input],
            0,
            Some(&plan),
            cfg!(windows),
        )
        .unwrap();
        let bootstrap = Value::object([
            ("branch_id", envelope["branch_id"].clone()),
            ("node_instance_id", envelope["node_instance_id"].clone()),
            ("node_key", spec.key.into()),
            ("role", spec.role.into()),
            ("stage", spec.stage.into()),
            ("generation", spec.generation.into()),
            ("mandatory", spec.mandatory.into()),
            ("status", "ready".into()),
            ("retry_count", 0_i64.into()),
            ("max_retries", envelope["max_retries"].clone()),
            ("envelope_json", json(&envelope).into()),
        ]);
        let mut evidence_digests = evidence
            .iter()
            .map(|artifact| Value::list([artifact.reference.clone(), artifact.sha256.clone()]))
            .collect::<Vec<_>>();
        python_operations::sort_comparable_values(&mut evidence_digests).unwrap();
        let request = Value::object([
            ("command", "init".into()),
            ("run_id", "RUN".into()),
            ("op_id", "init-1".into()),
            ("policy_digest", digest.clone().into()),
            ("task_digest", snapshot.digest.clone().into()),
            ("evidence_digests", Value::List(evidence_digests)),
            ("ack_permissions", true.into()),
            ("ack_durability", true.into()),
            ("execution_size", plan["size"].clone()),
            ("execution_plan_digest", plan["plan_digest"].clone()),
        ]);
        let request_digest = ids::sha256_bytes(&ids::canonical_bytes(&request).unwrap());
        let mut artifacts = vec![Value::object([
            ("ref", task_ref.clone().into()),
            ("kind", "task_brief".into()),
            ("sha256", snapshot.digest.clone().into()),
            ("size_bytes", snapshot.size.into()),
            ("source_type", "repo".into()),
            (
                "source_path",
                Value::String(graph_engine::application::policy::PolicyFilePort::spelling(
                    &files,
                    &snapshot.path,
                )),
            ),
            ("content_json", Value::Null),
            ("device", Value::Integer(snapshot.identity.device.into())),
            ("inode", Value::Integer(snapshot.identity.inode.into())),
        ])];
        artifacts.extend(evidence.iter().map(artifact_value));
        graph
            .store
            .initialize(&InitializeRequest {
                repository: &graph.repository,
                policy: &policy,
                policy_digest: &digest,
                task: &task,
                task_path: &snapshot.path,
                task_digest: &snapshot.digest,
                run_id: "RUN",
                operation_id: "init-1",
                request_digest: &request_digest,
                acknowledge_permissions: true,
                acknowledge_durability: true,
                bootstrap: &bootstrap,
                execution_plan: &plan,
                task_ref: &task_ref,
                initial_artifacts: &artifacts,
            })
            .unwrap();
        Self {
            graph,
            policy,
            digest,
            task_path,
        }
    }
    fn database(&self) -> PathBuf {
        self.graph
            .store
            .db_path(&self.policy["repository_id"].text().unwrap(), "RUN")
    }
    fn alter(&self, sql: &str) {
        Connection::open(self.database())
            .unwrap()
            .execute_batch(sql)
            .unwrap();
    }
    fn verify(&self) -> EngineResult<()> {
        self.with_session(|session, context| {
            let run = session.first(GraphRead::Run, &["RUN".into()])?.unwrap();
            verify_semantic_state(session, &run, context)
        })
    }
    fn with_session<T>(
        &self,
        action: impl FnOnce(&SqliteSession, &SemanticContext<'_>) -> EngineResult<T>,
    ) -> EngineResult<T> {
        let files = NativePolicyFiles::default();
        let session = SqliteSession::connect_existing(&self.database())?;
        action(
            &session,
            &SemanticContext {
                store: &self.graph.store,
                files: &files,
                snapshots: &files,
                provenance: &TestProvenance,
                repository: &self.graph.repository,
                skill: &self.graph.skill_root,
                installation: &self.graph.skill_root,
                runtime: Some(&self.graph.runtime_root),
                policy: &self.policy,
                current_policy_digest: &self.digest,
                case_sensitive: !cfg!(windows),
                evidence_enable: false,
            },
        )
    }
    fn plan_arguments(&self, operation: &str, decision: &str) -> Value {
        let digest: String = Connection::open(self.database())
            .unwrap()
            .query_row(
                "SELECT plan_digest FROM execution_plans WHERE run_id='RUN'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        Value::object([
            ("op_id", operation.into()),
            ("plan_digest", digest.into()),
            ("decision", decision.into()),
            ("authority_ref", "authority:test".into()),
            ("actor", Value::Null),
        ])
    }
}
fn expect(result: EngineResult<()>, code: &str) {
    match result {
        Err(EngineError::State(error)) => assert_eq!(error.code, code),
        other => panic!("expected {code}, got {other:?}"),
    }
}

#[test]
fn plan_approval_commits_after_full_before_and_after_semantic_validation() {
    let fixture = Fixture::new();
    let arguments = fixture.plan_arguments("approve-1", "APPROVE");
    let response = fixture
        .with_session(|session, context| record_plan_approval(session, "RUN", &arguments, context))
        .unwrap();
    assert!(response["code"].is_text("EXECUTION_PLAN_APPROVED"));
    assert!(response["status"].is_text("active"));
    assert!(response["state_revision"].py_eq(&2_i64.into()));
    fixture.verify().unwrap();
}

#[test]
fn plan_rejection_seals_the_blocked_timestamp_and_remains_semantically_valid() {
    let fixture = Fixture::new();
    let arguments = fixture.plan_arguments("reject-1", "REJECT");
    let response = fixture
        .with_session(|session, context| record_plan_approval(session, "RUN", &arguments, context))
        .unwrap();
    assert!(response["code"].is_text("EXECUTION_PLAN_REJECTED"));
    assert!(response["status"].is_text("blocked"));
    fixture.verify().unwrap();
}

#[test]
fn plan_digest_failure_rolls_back_without_a_revision_or_operation() {
    let fixture = Fixture::new();
    let mut arguments = fixture.plan_arguments("wrong-plan", "APPROVE");
    arguments.set("plan_digest", "0".repeat(64).into());
    let result = fixture
        .with_session(|session, context| record_plan_approval(session, "RUN", &arguments, context));
    assert!(
        matches!(result, Err(EngineError::State(error)) if error.code == "EXECUTION_PLAN_DIGEST_MISMATCH")
    );
    fixture
        .with_session(|session, _| {
            assert!(session.operation("RUN", "wrong-plan")?.is_none());
            assert!(session.run("RUN")?.unwrap().fields["state_revision"].py_eq(&1_i64.into()));
            Ok(())
        })
        .unwrap();
    fixture.verify().unwrap();
}

#[test]
fn approved_plan_cannot_be_approved_under_a_second_operation() {
    let fixture = Fixture::new();
    let first = fixture.plan_arguments("approve-1", "APPROVE");
    fixture
        .with_session(|session, context| record_plan_approval(session, "RUN", &first, context))
        .unwrap();
    let second = fixture.plan_arguments("approve-2", "APPROVE");
    let result = fixture
        .with_session(|session, context| record_plan_approval(session, "RUN", &second, context));
    assert!(
        matches!(result, Err(EngineError::State(error)) if error.code == "INVALID_RUN_TRANSITION")
    );
    fixture
        .with_session(|session, _| {
            assert!(session.operation("RUN", "approve-2")?.is_none());
            Ok(())
        })
        .unwrap();
    fixture.verify().unwrap();
}

#[test]
fn successful_plan_replay_precedes_new_semantic_failures_and_later_reads_are_fresh() {
    let fixture = Fixture::new();
    let arguments = fixture.plan_arguments("approve-1", "APPROVE");
    fixture
        .with_session(|session, context| record_plan_approval(session, "RUN", &arguments, context))
        .unwrap();
    fixture.alter("DROP TABLE budgets");
    let replay = fixture
        .with_session(|session, context| record_plan_approval(session, "RUN", &arguments, context))
        .unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(replay["state_revision"].py_eq(&2_i64.into()));
    expect(fixture.verify(), "DATABASE_STATE_INVALID");
}

#[test]
fn before_commit_hook_precedes_metadata_writes_and_after_validation_rolls_everything_back() {
    let fixture = Fixture::new();
    let arguments = fixture.plan_arguments("approve-1", "APPROVE");
    let observed = std::cell::Cell::new(false);
    let result = fixture.with_session(|session, context| {
        let fault = |point: &str| -> EngineResult<()> {
            if point == "before_commit" {
                observed.set(true);
                assert!(session.run("RUN")?.unwrap().fields["state_revision"].py_eq(&1_i64.into()));
                assert!(session.operation("RUN", "approve-1")?.is_none());
                session.write(
                    GraphWrite::PlanApprovalRunState,
                    &[
                        "initialized".into(),
                        Value::Null,
                        "initialized".into(),
                        "unused".into(),
                        "RUN".into(),
                    ],
                )?;
            }
            Ok(())
        };
        let store = StateStore {
            state_root: context.store.state_root.clone(),
            platform: context.store.platform,
            database: context.store.database,
            fault: &fault,
        };
        let changed = SemanticContext {
            store: &store,
            ..*context
        };
        record_plan_approval(session, "RUN", &arguments, &changed)
    });
    assert!(observed.get());
    assert!(
        matches!(result, Err(EngineError::State(error)) if error.code == "EXECUTION_PLAN_STATE_INVALID")
    );
    fixture
        .with_session(|session, _| {
            assert!(session.operation("RUN", "approve-1")?.is_none());
            assert!(session.run("RUN")?.unwrap().fields["state_revision"].py_eq(&1_i64.into()));
            assert!(session
                .first(GraphRead::ExecutionPlan, &["RUN".into()])?
                .unwrap()["status"]
                .is_text("pending"));
            Ok(())
        })
        .unwrap();
    fixture.verify().unwrap();
}

#[test]
fn actor_mismatch_is_rejected_before_beginning_the_mutation() {
    let fixture = Fixture::new();
    let mut arguments = fixture.plan_arguments("approve-1", "APPROVE");
    let mut actor = state_platform::current_actor().unwrap();
    actor.0.extend(PyString::from("-different").0);
    arguments.set("actor", Value::String(actor));
    let result = fixture.with_session(|session, context| {
        session.begin_immediate()?;
        let result = record_plan_approval(session, "RUN", &arguments, context);
        session.rollback()?;
        result
    });
    assert!(
        matches!(result, Err(EngineError::State(error)) if error.code == "APPROVAL_ACTOR_MISMATCH")
    );
    fixture.verify().unwrap();
}

#[test]
fn before_validation_rejects_corrupt_budget_state_without_plan_writes() {
    let fixture = Fixture::new();
    let arguments = fixture.plan_arguments("approve-1", "APPROVE");
    fixture.alter("UPDATE budgets SET limit_value=limit_value+1 WHERE budget_id='file_reads'");
    let result = fixture
        .with_session(|session, context| record_plan_approval(session, "RUN", &arguments, context));
    assert!(
        matches!(result, Err(EngineError::State(error)) if error.code == "BUDGET_STATE_INVALID")
    );
    fixture
        .with_session(|session, _| {
            assert!(session.operation("RUN", "approve-1")?.is_none());
            assert!(session
                .first(GraphRead::ExecutionPlan, &["RUN".into()])?
                .unwrap()["status"]
                .is_text("pending"));
            Ok(())
        })
        .unwrap();
}
fn abort_arguments(operation: &str) -> Value {
    Value::object([
        ("op_id", operation.into()),
        ("reason_code", "STOP".into()),
        ("authority_ref", "authority:test".into()),
    ])
}

fn block_arguments(fixture: &Fixture, operation: &str, evidence: Value) -> (Value, Value) {
    let manifest = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "block".into()),
        ("run_id", "RUN".into()),
        ("reason_code", "STOP".into()),
        ("evidence", evidence),
    ]);
    let path = fixture
        .graph
        .store
        .inbox_root(&fixture.policy["repository_id"].text().unwrap(), "RUN")
        .join(format!("block-{operation}.json"));
    fs::write(&path, ids::canonical_bytes(&manifest).unwrap()).unwrap();
    (
        Value::object([
            ("op_id", operation.into()),
            ("reason_code", "STOP".into()),
            ("evidence_manifest", path.to_str().unwrap().into()),
        ]),
        manifest,
    )
}

fn control_counts(fixture: &Fixture) -> (String, i64, i64, i64, i64) {
    Connection::open(fixture.database()).unwrap().query_row(
        "SELECT status,state_revision,(SELECT COUNT(*) FROM artifacts),(SELECT COUNT(*) FROM operations),(SELECT COUNT(*) FROM events) FROM runs WHERE run_id='RUN'",
        [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)),
    ).unwrap()
}

#[test]
fn abort_commits_with_full_semantic_before_and_after_validation() {
    let fixture = Fixture::new();
    let before = control_counts(&fixture);
    let response = fixture
        .with_session(|session, context| {
            abort_run(session, "RUN", &abort_arguments("abort-1"), context)
        })
        .unwrap();
    assert!(response["code"].is_text("ABORTED"));
    assert!(response["status"].is_text("aborted"));
    let after = control_counts(&fixture);
    assert_eq!(
        after,
        (
            "aborted".into(),
            before.1 + 1,
            before.2,
            before.3 + 1,
            before.4 + 1
        )
    );
    fixture
        .with_session(|session, _| {
            let run = session.run("RUN")?.unwrap().fields;
            assert!(run["blocked_reason"].is_text("STOP"));
            assert!(!run["finished_at"].is_null());
            Ok(())
        })
        .unwrap();
    fixture.verify().unwrap();
}

#[test]
fn abort_preflight_reason_and_authority_precede_operation_and_begin() {
    let fixture = Fixture::new();
    for (reason, authority, field) in [
        ("bad reason", "bad", "reason_code"),
        ("STOP", "bad", "authority_ref"),
    ] {
        let arguments = Value::object([
            ("op_id", "bad operation".into()),
            ("reason_code", reason.into()),
            ("authority_ref", authority.into()),
        ]);
        let result = fixture.with_session(|session, context| {
            session.begin_immediate()?;
            let result = abort_run(session, "RUN", &arguments, context);
            session.rollback()?;
            result
        });
        assert!(matches!(result, Err(EngineError::Contract(error)) if error.field.equals(field)));
    }
    fixture.verify().unwrap();
}

#[test]
fn abort_replay_precedes_new_semantic_failure_and_new_operation_checks_terminal_state() {
    let fixture = Fixture::new();
    let arguments = abort_arguments("abort-1");
    let first = fixture
        .with_session(|session, context| abort_run(session, "RUN", &arguments, context))
        .unwrap();
    fixture.alter("UPDATE budgets SET used=-1");
    let replay = fixture
        .with_session(|session, context| abort_run(session, "RUN", &arguments, context))
        .unwrap();
    let mut expected = first.clone();
    expected.set("code", "REPLAYED".into());
    assert!(expected.py_eq(&replay));
    let error = fixture.with_session(|session, context| {
        abort_run(session, "RUN", &abort_arguments("abort-2"), context)
    });
    assert!(matches!(error,Err(EngineError::State(error)) if error.code=="TERMINAL_RUN"));
    expect(fixture.verify(), "BUDGET_STATE_INVALID");
}

#[test]
fn abort_semantic_before_rejects_corrupt_budget_without_any_writes() {
    let fixture = Fixture::new();
    fixture.alter("UPDATE budgets SET used=-1");
    let before = control_counts(&fixture);
    let result = fixture.with_session(|session, context| {
        abort_run(session, "RUN", &abort_arguments("abort-1"), context)
    });
    assert!(matches!(result,Err(EngineError::State(error)) if error.code=="BUDGET_STATE_INVALID"));
    assert_eq!(control_counts(&fixture), before);
}

#[test]
fn block_commits_the_canonical_manifest_and_run_metadata() {
    let fixture = Fixture::new();
    let before = control_counts(&fixture);
    let (arguments, manifest) = block_arguments(&fixture, "block-1", Value::list([]));
    let response = fixture
        .with_session(|session, context| block_run(session, "RUN", &arguments, context))
        .unwrap();
    assert!(response["code"].is_text("GRAPH_BLOCKED"));
    assert!(response["status"].is_text("blocked"));
    let after = control_counts(&fixture);
    assert_eq!(
        after,
        (
            "blocked".into(),
            before.1 + 1,
            before.2 + 1,
            before.3 + 1,
            before.4 + 1
        )
    );
    let content: String = Connection::open(fixture.database())
        .unwrap()
        .query_row(
            "SELECT content_json FROM artifacts WHERE ref LIKE 'ledger:block-block-1#sha256=%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(content, json(&manifest));
    fixture.verify().unwrap();
}

#[test]
fn abort_from_blocked_preserves_the_existing_finished_timestamp() {
    let fixture = Fixture::new();
    let (arguments, _) = block_arguments(&fixture, "block-1", Value::list([]));
    fixture
        .with_session(|session, context| block_run(session, "RUN", &arguments, context))
        .unwrap();
    fixture.alter("UPDATE runs SET finished_at='2030-01-01T00:00:00Z'");
    fixture.verify().unwrap();
    fixture
        .with_session(|session, context| {
            abort_run(session, "RUN", &abort_arguments("abort-1"), context)
        })
        .unwrap();
    let finish: String = Connection::open(fixture.database())
        .unwrap()
        .query_row("SELECT finished_at FROM runs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(finish, "2030-01-01T00:00:00Z");
    fixture.verify().unwrap();
}

#[test]
fn block_fixed_inbox_check_precedes_reason_operation_and_begin() {
    let fixture = Fixture::new();
    let arguments = Value::object([
        ("op_id", "bad operation".into()),
        ("reason_code", "bad reason".into()),
        (
            "evidence_manifest",
            fixture.task_path.to_str().unwrap().into(),
        ),
    ]);
    let result = fixture.with_session(|session, context| {
        session.begin_immediate()?;
        let result = block_run(session, "RUN", &arguments, context);
        session.rollback()?;
        result
    });
    assert!(
        matches!(result,Err(EngineError::Contract(error)) if error.field.equals("path") && error.code=="OUTSIDE_FIXED_INBOX")
    );
    fixture.verify().unwrap();
}

#[test]
fn block_json_snapshot_error_precedes_semantic_before() {
    let fixture = Fixture::new();
    let (arguments, _) = block_arguments(&fixture, "block-1", Value::list([]));
    fs::write(arguments["evidence_manifest"].text().unwrap(), b"{").unwrap();
    fixture.alter("UPDATE budgets SET used=-1");
    let before = control_counts(&fixture);
    let result =
        fixture.with_session(|session, context| block_run(session, "RUN", &arguments, context));
    assert!(
        matches!(result,Err(EngineError::Contract(error)) if error.field.equals("path") && error.code=="INVALID_JSON")
    );
    assert_eq!(control_counts(&fixture), before);
}

#[test]
fn block_control_shape_validation_follows_semantic_before() {
    let fixture = Fixture::new();
    let (arguments, _) = block_arguments(&fixture, "block-1", Value::list([]));
    fs::write(arguments["evidence_manifest"].text().unwrap(), b"[]").unwrap();
    fixture.alter("UPDATE budgets SET used=-1");
    let before = control_counts(&fixture);
    let result =
        fixture.with_session(|session, context| block_run(session, "RUN", &arguments, context));
    assert!(matches!(result,Err(EngineError::State(error)) if error.code=="BUDGET_STATE_INVALID"));
    assert_eq!(control_counts(&fixture), before);
}

#[test]
fn block_invalid_control_shape_rolls_back_on_a_healthy_run() {
    let fixture = Fixture::new();
    let (arguments, _) = block_arguments(&fixture, "block-1", Value::list([]));
    fs::write(arguments["evidence_manifest"].text().unwrap(), b"[]").unwrap();
    let before = control_counts(&fixture);
    let result =
        fixture.with_session(|session, context| block_run(session, "RUN", &arguments, context));
    assert!(
        matches!(result,Err(EngineError::Contract(error)) if error.field.equals("evidence_manifest") && error.code=="INVALID_OBJECT")
    );
    assert_eq!(control_counts(&fixture), before);
    fixture.verify().unwrap();
}

#[test]
fn block_bad_later_evidence_rolls_back_an_earlier_artifact_insert() {
    let fixture = Fixture::new();
    let bytes = fs::read(&fixture.task_path).unwrap();
    fs::write(
        fixture
            .graph
            .repository
            .join("docs/artifacts/extra-task.json"),
        &bytes,
    )
    .unwrap();
    let digest = ids::sha256_bytes(&bytes);
    let evidence = Value::list([
        Value::object([
            ("kind", "task_brief".into()),
            (
                "ref",
                format!("repo:docs/artifacts/extra-task.json#sha256={digest}").into(),
            ),
            ("sha256", digest.into()),
        ]),
        "bad item".into(),
    ]);
    let (arguments, _) = block_arguments(&fixture, "block-1", evidence);
    let before = control_counts(&fixture);
    let result =
        fixture.with_session(|session, context| block_run(session, "RUN", &arguments, context));
    assert!(
        matches!(result,Err(EngineError::Contract(error)) if error.field.equals("evidence") && error.code=="INVALID_OBJECT")
    );
    assert_eq!(control_counts(&fixture), before);
    fixture.verify().unwrap();
}

#[test]
fn block_semantic_after_rolls_back_artifacts_run_revision_operation_and_event() {
    let fixture = Fixture::new();
    let (arguments, manifest) = block_arguments(&fixture, "block-1", Value::list([]));
    let reference = Value::from(format!(
        "ledger:block-block-1#sha256={}",
        ids::sha256_bytes(&ids::canonical_bytes(&manifest).unwrap())
    ));
    let before = control_counts(&fixture);
    let original = fs::read(&fixture.task_path).unwrap();
    let observed = std::cell::Cell::new(false);
    let result = fixture.with_session(|session, context| {
        let fault = |point: &str| -> EngineResult<()> {
            if point == "before_commit" {
                observed.set(true);
                let current = session.run("RUN")?.unwrap().fields;
                assert!(current["status"].is_text("blocked"));
                assert!(current["state_revision"].py_eq(&1_i64.into()));
                assert!(session.operation("RUN", "block-1")?.is_none());
                assert!(
                    graph_engine::application::artifacts::ArtifactLedger::artifact_by_reference(
                        session, &reference
                    )?
                    .is_some()
                );
                fs::write(&fixture.task_path, b"{}")?;
            }
            Ok(())
        };
        let store = StateStore {
            state_root: context.store.state_root.clone(),
            platform: context.store.platform,
            database: context.store.database,
            fault: &fault,
        };
        let changed = SemanticContext {
            store: &store,
            ..*context
        };
        block_run(session, "RUN", &arguments, &changed)
    });
    assert!(observed.get());
    assert!(matches!(result,Err(EngineError::State(error)) if error.code=="INPUT_DIGEST_MISMATCH"));
    assert_eq!(control_counts(&fixture), before);
    fs::write(&fixture.task_path, original).unwrap();
    fixture.verify().unwrap();
}

#[test]
fn block_replay_precedes_new_semantic_failure_and_later_abort_is_fresh() {
    let fixture = Fixture::new();
    let (arguments, _) = block_arguments(&fixture, "block-1", Value::list([]));
    let first = fixture
        .with_session(|session, context| block_run(session, "RUN", &arguments, context))
        .unwrap();
    fixture.alter("UPDATE budgets SET used=-1");
    let replay = fixture
        .with_session(|session, context| block_run(session, "RUN", &arguments, context))
        .unwrap();
    let mut expected = first.clone();
    expected.set("code", "REPLAYED".into());
    assert!(expected.py_eq(&replay));
    let result = fixture.with_session(|session, context| {
        abort_run(session, "RUN", &abort_arguments("abort-1"), context)
    });
    assert!(matches!(result,Err(EngineError::State(error)) if error.code=="BUDGET_STATE_INVALID"));
    expect(fixture.verify(), "BUDGET_STATE_INVALID");
}

#[test]
fn initialized_graph_passes_every_semantic_validator_with_real_adapters() {
    Fixture::new().verify().unwrap();
}
#[test]
fn schema_error_precedes_engine_and_policy_errors() {
    let f = Fixture::new();
    f.alter(
        "UPDATE runs SET state_schema_version=99,engine_version='invalid',policy_digest='changed'",
    );
    expect(f.verify(), "UNSUPPORTED_STATE_SCHEMA");
}
#[test]
fn lifecycle_json_shape_error_precedes_engine_validation() {
    let f = Fixture::new();
    f.alter("UPDATE runs SET state_schema_version=7,task_json='null',engine_version='invalid'");
    expect(f.verify(), "EVIDENCE_ACTIVATION_INVALID");
}
#[test]
fn lifecycle_json_decode_error_is_wrapped_before_engine_validation() {
    let f = Fixture::new();
    f.alter("UPDATE runs SET state_schema_version=7,task_json=x'ff',engine_version='invalid'");
    expect(f.verify(), "EVIDENCE_LIFECYCLE_INVALID");
}
#[test]
fn incompatible_engine_precedes_run_state_and_digest() {
    let f = Fixture::new();
    f.alter("UPDATE runs SET engine_version='invalid',status='unexpected',policy_digest='changed'");
    expect(f.verify(), "ENGINE_VERSION_INCOMPATIBLE");
}
#[test]
fn invalid_run_state_precedes_configuration_digest() {
    let f = Fixture::new();
    f.alter("UPDATE runs SET status='unexpected',policy_digest='changed'");
    expect(f.verify(), "RUN_STATE_INVALID");
}
#[test]
fn changed_configuration_precedes_host_identity() {
    let f = Fixture::new();
    f.alter("UPDATE runs SET policy_digest='changed',host_identity='other-host'");
    expect(f.verify(), "CONFIG_CHANGED");
}
#[test]
fn changed_host_precedes_repository_identity() {
    let f = Fixture::new();
    f.alter("UPDATE runs SET host_identity='other-host',repository_inode=-1");
    expect(f.verify(), "HOST_IDENTITY_MISMATCH");
}
#[test]
fn changed_repository_precedes_task_snapshot() {
    let f = Fixture::new();
    f.alter("UPDATE runs SET repository_inode=-1,task_path='missing-task'");
    expect(f.verify(), "REPOSITORY_IDENTITY_MISMATCH");
}
#[test]
fn changed_task_bytes_precede_stored_metadata() {
    let f = Fixture::new();
    fs::write(&f.task_path, b"{}").unwrap();
    f.alter("UPDATE runs SET task_json='null'");
    expect(f.verify(), "INPUT_DIGEST_MISMATCH");
}
#[test]
fn malformed_operation_rows_are_rejected_before_graph_reconstruction() {
    let f = Fixture::new();
    f.alter("UPDATE operations SET request_digest='short'");
    expect(f.verify(), "OPERATION_LEDGER_INVALID");
}
#[test]
fn malformed_ledger_schema_is_a_database_state_error() {
    let f = Fixture::new();
    f.alter("DROP TABLE budgets");
    expect(f.verify(), "DATABASE_STATE_INVALID");
}
#[test]
fn fresh_artifact_bytes_are_reverified_after_graph_validation() {
    let f = Fixture::new();
    fs::write(
        f.graph.repository.join("docs/engineering-graph.md"),
        b"changed",
    )
    .unwrap();
    assert!(
        matches!(f.verify(),Err(EngineError::Contract(error))if error.code=="INPUT_DIGEST_MISMATCH")
    );
}
