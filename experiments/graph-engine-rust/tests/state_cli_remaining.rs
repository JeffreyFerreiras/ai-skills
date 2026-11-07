#[allow(dead_code)]
#[path = "support/hardening_case.rs"]
mod hardening_case;
#[path = "support/state_faults.rs"]
mod state_faults;
#[path = "support/state_platform_overrides.rs"]
mod state_platform_overrides;

use graph_engine::{
    application::{
        artifacts::{self, ArtifactResolver},
        error::{EngineError, EngineResult},
        persistent_store::StateStore,
        policy::{self, PolicyFilePort},
    },
    compat::{PyString, Value},
    domain::{contracts::ContractError, evidence::canonical_ledger_artifact},
    infrastructure::{policy_files::NativePolicyFiles, sqlite::SqliteSession},
};
use hardening_case::HardeningCase;
use state_faults::{init_argv, init_with, no_fault, write_task};
use state_platform_overrides::StatePlatformOverrides;
use std::{collections::BTreeMap, fs};

const REPOSITORY: &str = "albanian-live-translate";
const RUN: &str = "RUN-1";
const STATUS: &[&str] = &["status", "--run-id", RUN];
const RESUME: &[&str] = &[
    "resume",
    "--run-id",
    RUN,
    "--ack-degraded-permissions",
    "--ack-degraded-durability",
];

fn state_failure<T>(result: EngineResult<T>, expected: &[&str]) {
    match result {
        Err(EngineError::State(error)) => {
            assert!(expected.contains(&error.code.as_str()), "{error:?}");
        }
        Err(error) => panic!("expected state error {expected:?}, got {error:?}"),
        Ok(_) => panic!("expected state error {expected:?}, got success"),
    }
}

fn contract_failure<T>(result: EngineResult<T>) -> ContractError {
    match result {
        Err(EngineError::Contract(error)) => error,
        Err(error) => panic!("expected contract error, got {error:?}"),
        Ok(_) => panic!("expected contract error, got success"),
    }
}

fn database(store: &StateStore<'_>) -> SqliteSession {
    SqliteSession::connect_existing(&store.db_path(REPOSITORY, RUN)).unwrap()
}

fn initialize(case: &HardeningCase, tags: &[&str]) -> Value {
    let task = case.fixture.graph.task("delivery", "full_delivery", tags);
    case.fixture.initialize(&task, true).unwrap()
}

#[test]
fn test_oversized_ledger_artifact_fails_resolution_status_and_resume() {
    let mut case = HardeningCase::stock();
    case.edit_policy(|policy| {
        let mut kinds = policy["artifact_kinds"].clone();
        let mut collection = kinds["collection"].clone();
        collection.set("max_bytes", 1024_i64.into());
        kinds.set("collection", collection);
        policy.set("artifact_kinds", kinds);
    });
    initialize(&case, &[]);
    let marker = "private-collection-content";
    let oversized = canonical_ledger_artifact(
        &"corrupt-collection".into(),
        &"collection".into(),
        &Value::object([("payload", format!("{marker}{}", "x".repeat(2048)).into())]),
    )
    .unwrap();
    assert!(oversized.size_bytes.as_i64().unwrap() > 1024);
    case.fixture
        .with_session(|session, context| {
            artifacts::persist_artifact(session, RUN, &oversized)?;
            let resolver = ArtifactResolver {
                files: context.files,
                snapshots: context.snapshots,
                repository: context.repository,
                skill: context.skill,
                installation: context.installation,
                runtime: context.runtime,
                policy: context.policy,
                ledger: Some(session),
            };
            let error = contract_failure(resolver.resolve_reference(
                &oversized.reference,
                &oversized.sha256,
                &oversized.kind,
            ));
            assert_eq!(error.code, "FILE_TOO_LARGE");
            assert!(!error.to_string().contains(marker));
            Ok(())
        })
        .unwrap();
    for argv in [STATUS, RESUME] {
        let error = contract_failure(case.run(argv));
        assert!(error.field.equals("artifact_ref"));
        assert_eq!(error.code, "FILE_TOO_LARGE");
        assert!(!error.to_string().contains(marker));
    }
}

#[test]
fn test_init_and_identical_operation_are_idempotent() {
    let case = HardeningCase::stock();
    let first = initialize(&case, &[]);
    let permission = if cfg!(windows) {
        "DEGRADED_PERMISSION_VERIFICATION"
    } else {
        "verified"
    };
    assert!(first["permission_verification"].is_text(permission));
    let second = init_with(
        &case,
        &case.fixture.graph.store,
        &init_argv(&case, "init-1"),
    )
    .unwrap();
    assert!(first["state_revision"].py_eq(&second["state_revision"]));
    assert!(second["code"].is_text("REPLAYED"));
}

#[test]
#[cfg_attr(not(windows), ignore = "Windows-specific degraded permission contract")]
fn test_windows_permission_degradation_requires_acknowledgment() {
    let case = HardeningCase::stock();
    write_task(&case);
    let mut argv = init_argv(&case, "init-no-permission-ack");
    argv.remove(2);
    state_failure(
        init_with(&case, &case.fixture.graph.store, &argv),
        &["DEGRADED_PERMISSION_ACK_REQUIRED"],
    );
}

#[test]
fn test_resume_consumes_host_bound_degraded_acknowledgments() {
    let case = HardeningCase::stock();
    initialize(&case, &[]);
    if cfg!(windows) {
        state_failure(
            case.run(&["resume", "--run-id", RUN]),
            &["DEGRADED_PERMISSION_ACK_REQUIRED"],
        );
    }
    let resumed = case.run(RESUME).unwrap();
    let ack = &resumed["acknowledgments"];
    assert!(
        ack["host_identity"].is_text(&case.fixture.graph.store.platform.host_identity().unwrap())
    );
    assert!(ack["degraded_permissions"].truthy());
}

#[test]
fn test_acknowledged_degraded_directory_sync_is_recorded() {
    let case = HardeningCase::stock();
    write_task(&case);
    let graph = &case.fixture.graph;
    let platform = StatePlatformOverrides {
        directory_sync: Some(|acknowledged| {
            assert!(acknowledged, "ack not supplied");
            "simulated_directory_sync_unavailable".into()
        }),
        ..Default::default()
    };
    let store = StateStore::at_root(
        graph.root.join("degraded-codex"),
        &platform,
        graph.store.database,
        &no_fault,
    )
    .unwrap();
    let result = init_with(&case, &store, &init_argv(&case, "degraded-op")).unwrap();
    assert!(result["durability"].is_text("degraded"));
    assert!(result["durability_detail"].is_text("simulated_directory_sync_unavailable"));
    let run = store
        .open_run(REPOSITORY, RUN)
        .unwrap()
        .first_run()
        .unwrap()
        .unwrap();
    assert_eq!(run["degraded_durability_ack"].as_i64(), Some(1));
    assert!(run["host_identity"].is_text(&store.platform.host_identity().unwrap()));
}

#[test]
fn test_changed_operation_id_payload_conflicts_without_mutation() {
    let case = HardeningCase::stock();
    initialize(&case, &[]);
    let first = case
        .run(&["next", "--run-id", RUN, "--claim", "--op-id", "same-op"])
        .unwrap();
    state_failure(
        case.run(&[
            "abort",
            "--run-id",
            RUN,
            "--reason-code",
            "STOP",
            "--authority-ref",
            "authority:test",
            "--op-id",
            "same-op",
        ]),
        &["OPERATION_CONFLICT"],
    );
    assert!(case.status().unwrap()["state_revision"].py_eq(&first["state_revision"]));
}

#[test]
fn test_non_local_state_root_and_changed_locality_fail_closed() {
    let case = HardeningCase::stock();
    write_task(&case);
    let graph = &case.fixture.graph;
    let remote = StatePlatformOverrides {
        locality: Some(|| Err(EngineError::state("NON_LOCAL_FILESYSTEM"))),
        ..Default::default()
    };
    let remote_store = StateStore::at_root(
        graph.root.join("remote-codex"),
        &remote,
        graph.store.database,
        &no_fault,
    )
    .unwrap();
    state_failure(
        init_with(&case, &remote_store, &init_argv(&case, "remote-op")),
        &["NON_LOCAL_FILESYSTEM"],
    );

    initialize(&case, &[]);
    let changed = StatePlatformOverrides {
        locality: Some(|| Ok("changed-filesystem".into())),
        ..Default::default()
    };
    let changed_store = StateStore::at_root(
        graph.store.state_root.clone(),
        &changed,
        graph.store.database,
        &no_fault,
    )
    .unwrap();
    let argv = [
        "--repo".into(),
        graph.repository.to_str().unwrap().into(),
        "status".into(),
        "--run-id".into(),
        RUN.into(),
    ];
    state_failure(
        case.fixture.execute_with_store(&argv, &changed_store),
        &["FILESYSTEM_IDENTITY_CHANGED"],
    );
}

#[test]
fn test_run_and_inbox_identity_are_reverified_on_open() {
    let case = HardeningCase::stock();
    initialize(&case, &[]);
    fs::create_dir(
        case.fixture
            .graph
            .store
            .inbox_root(REPOSITORY, RUN)
            .join("unexpected-directory"),
    )
    .unwrap();
    state_failure(case.status(), &["INBOX_ENTRY_INVALID"]);
}

fn with_environment(values: &[(&str, PyString)]) -> NativePolicyFiles {
    NativePolicyFiles {
        environment_override: Some(
            values
                .iter()
                .map(|(key, value)| (key.to_string(), value.clone()))
                .collect::<BTreeMap<_, _>>(),
        ),
    }
}

#[test]
fn test_state_root_precedence_and_invalid_supplied_values_fail_closed() {
    let case = HardeningCase::stock();
    let graph = &case.fixture.graph;
    let explicit = graph.root.join("explicit");
    let environment = graph.root.join("environment");
    let xdg = graph.root.join("xdg");
    let native = NativePolicyFiles::default();
    let environment_var = (
        "SOFTWARE_ENGINEERING_GRAPH_STATE_HOME",
        native.spelling(&environment),
    );
    let xdg_var = ("XDG_STATE_HOME", native.spelling(&xdg));
    let files = with_environment(&[environment_var.clone(), xdg_var.clone()]);
    assert_eq!(
        policy::resolve_state_root(&files, Some(&explicit), &graph.skill_root).unwrap(),
        explicit
    );
    assert_eq!(
        policy::resolve_state_root(&files, None, &graph.skill_root).unwrap(),
        environment
    );
    let files = with_environment(&[
        environment_var,
        xdg_var.clone(),
        (
            "SOFTWARE_ENGINEERING_GRAPH_HOME",
            native.spelling(&explicit),
        ),
    ]);
    assert_eq!(
        policy::resolve_state_root(&files, None, &graph.skill_root).unwrap(),
        explicit
    );
    let files = with_environment(&[xdg_var]);
    assert_eq!(
        policy::resolve_state_root(&files, None, &graph.skill_root).unwrap(),
        xdg.join("software-engineering-graph")
    );
    for key in [
        "SOFTWARE_ENGINEERING_GRAPH_HOME",
        "SOFTWARE_ENGINEERING_GRAPH_STATE_HOME",
        "XDG_STATE_HOME",
    ] {
        for value in ["", "relative"] {
            assert!(
                matches!(
                    policy::resolve_state_root(
                        &with_environment(&[(key, value.into())]),
                        None,
                        &graph.skill_root
                    ),
                    Err(EngineError::State(_))
                ),
                "{key}={value:?}"
            );
        }
    }
    state_failure(
        StateStore::new(
            Some(&explicit),
            Some(&environment),
            &graph.skill_root,
            &files,
            graph.store.platform,
            graph.store.database,
            graph.store.fault,
        ),
        &["STATE_ROOT_CONFLICT"],
    );
}

#[test]
fn test_explicit_legacy_root_is_opened_without_migration_or_discovery() {
    let case = HardeningCase::stock();
    initialize(&case, &[]);
    let graph = &case.fixture.graph;
    let new = graph.root.join("new");
    let files = with_environment(&[(
        "SOFTWARE_ENGINEERING_GRAPH_STATE_HOME",
        NativePolicyFiles::default().spelling(&new),
    )]);
    let selected = StateStore::new(
        None,
        Some(&graph.store.state_root),
        &graph.skill_root,
        &files,
        graph.store.platform,
        graph.store.database,
        graph.store.fault,
    )
    .unwrap();
    let run = selected
        .open_run(REPOSITORY, RUN)
        .unwrap()
        .first_run()
        .unwrap()
        .unwrap();
    assert!(run["status"].is_text("active"));
    assert!(!new.exists());
}

fn alter(case: &HardeningCase, statement: &str) {
    database(&case.fixture.graph.store)
        .execute_values(statement, &[])
        .unwrap();
}

#[test]
fn test_semantic_corruption_fails_closed() {
    for (statement, code) in [
        (
            "UPDATE budgets SET used=-1 WHERE budget_id='file_reads'",
            "BUDGET_STATE_INVALID",
        ),
        ("UPDATE runs SET policy_json='{}'", "POLICY_STATE_INVALID"),
        (
            "UPDATE nodes SET role='senior_engineer'",
            "NODE_BINDING_INVALID",
        ),
        (
            "UPDATE operations SET response_json='{}' WHERE resulting_revision=1",
            "OPERATION_LEDGER_INVALID",
        ),
    ] {
        let case = HardeningCase::stock();
        initialize(&case, &[]);
        alter(&case, statement);
        state_failure(case.status(), &[code]);
    }
}

#[test]
fn test_compatible_engine_upgrade_is_accepted_but_range_exit_is_rejected() {
    let case = HardeningCase::stock();
    initialize(&case, &[]);
    alter(&case, "UPDATE runs SET engine_version='2.5.0'");
    assert!(case.run(RESUME).unwrap()["code"].is_text("RESUMED"));
    alter(&case, "UPDATE runs SET engine_version='3.0.0'");
    state_failure(case.status(), &["ENGINE_VERSION_INCOMPATIBLE"]);
}

fn design_inputs(case: &mut HardeningCase) {
    let tech = case.claim();
    case.success(&tech);
    case.advance("design_inputs");
}

#[test]
fn test_route_tag_specialist_and_generation_corruption_fail_closed() {
    for (tags, statement, code) in [
        (
            &[][..],
            "UPDATE runs SET selected_route='fast_path'",
            "ROUTE_EVIDENCE_MISMATCH",
        ),
        (
            &["security_privacy"][..],
            "UPDATE runs SET selected_tags_json='[]'",
            "ROUTE_EVIDENCE_MISMATCH",
        ),
        (
            &[][..],
            "UPDATE runs SET design_generation=7",
            "GENERATION_STATE_INVALID",
        ),
    ] {
        let mut case = HardeningCase::stock();
        initialize(&case, tags);
        case.impact_with_tags("full_delivery", tags);
        alter(&case, statement);
        state_failure(case.status(), &[code]);
    }
    let mut case = HardeningCase::stock();
    let tags = &["security_privacy"];
    initialize(&case, tags);
    case.impact_with_tags("full_delivery", tags);
    design_inputs(&mut case);
    alter(&case, "UPDATE nodes SET specialist_tag='audio_realtime_translation' WHERE specialist_tag='security_privacy'");
    state_failure(
        case.status(),
        &[
            "NODE_BINDING_INVALID",
            "TOPOLOGY_STATE_INVALID",
            "STABLE_ID_INVALID",
        ],
    );
}

#[test]
fn test_artifact_envelope_and_join_corruption_fail_closed() {
    for (statement, code) in [
        ("DELETE FROM artifacts WHERE kind='task_brief'", "INPUT_ARTIFACT_INVALID"),
        ("UPDATE nodes SET envelope_json='{}' WHERE node_key='architect'", "ENVELOPE_INVALID"),
        ("DELETE FROM join_members WHERE join_id=(SELECT join_id FROM joins WHERE join_key='design_collection')", "JOIN_MEMBERSHIP_INVALID"),
    ] {
        let mut case = HardeningCase::stock();
        initialize(&case, &[]);
        case.impact("full_delivery");
        design_inputs(&mut case);
        alter(&case, statement);
        state_failure(case.status(), &[code]);
    }
}
