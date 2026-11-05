use super::hardening_case::HardeningCase;
use graph_engine::{
    application::{
        error::EngineResult, evidence_lifecycle::verify_review_source,
        evidence_verification::EvidenceVerification, state::GraphSession,
    },
    compat::{self, Value},
    infrastructure::{
        check_runtime::NativeCheckRuntime, process::NativeProcess, source_files::NativeSourceFiles,
        sqlite::SqliteSession,
    },
};
use std::{fs, process::Command};

pub(super) fn git(case: &HardeningCase, args: &[&str]) -> String {
    let graph = &case.fixture.graph;
    let config = graph.root.join("git-config-empty");
    let template = graph.root.join("git-template-empty");
    if !config.exists() {
        fs::write(&config, b"").unwrap();
        fs::create_dir(&template).unwrap();
    }
    let output = Command::new("git")
        .args(args)
        .current_dir(&graph.repository)
        .env("GIT_CONFIG_GLOBAL", config)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TEMPLATE_DIR", template)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

pub(super) fn verify(case: &HardeningCase, run_id: &str) -> EngineResult<(Value, Value)> {
    case.fixture.with_context(|context| {
        let database = context.store.db_path("albanian-live-translate", run_id);
        let session = SqliteSession::connect_existing(&database)?;
        let run = session.run(run_id)?.unwrap().fields;
        let files = NativeSourceFiles::new(context.repository, context.policy, context.files);
        let clock = NativeCheckRuntime {
            repository: context.repository,
        };
        let verification = EvidenceVerification {
            session: &session,
            run: &run,
            context: *context,
            clock: &clock,
            source_files: &files,
            process: &NativeProcess,
        };
        let (binding, _) = verify_review_source(&session, &clock, &verification, &run, None, true)?;
        Ok((binding, compat::loads_value(&run["task_json"])?))
    })
}

pub(super) fn merge_snapshot(status: &Value, head: &str, required: &Value) -> Value {
    let mut checks = Value::object([]);
    for item in status["check_evidence"].as_list().unwrap() {
        checks.set(
            &item["check_id"].text().unwrap(),
            if item["outcome"].is_text("PASS") {
                "success".into()
            } else {
                "failure".into()
            },
        );
    }
    Value::object([
        ("head_sha", head.into()),
        ("reviewed_sha", head.into()),
        ("ci_head_sha", head.into()),
        ("required_check_ids", required.clone()),
        ("checks", checks),
        ("review_decision", "APPROVED".into()),
        ("unresolved_review_count", 0_i64.into()),
        ("graph_status", status["status"].clone()),
        ("dry_run", true.into()),
    ])
}
