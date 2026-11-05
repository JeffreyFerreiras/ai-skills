use super::helper_fixture::{self as fixture, Fixture, Settings, FILES, NO_FAULT, TRANSACTIONS};
use fixture::{
    assert_code, json, read, register_path, remove, set_nested, settlement, sign_plan, write,
};
use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        helper_ledger::HelperReservationLedger,
        helper_repository::{
            HelperFileTransactions, HelperRegisterPersistence, RegisterTransaction,
        },
        helper_validation,
        policy::PolicyFilePort,
        state::FaultHook,
    },
    compat::Value,
    domain::{helper_register as helper, ids},
};
use std::path::{Path, PathBuf};

fn materials(settings: Settings) -> (Fixture, Value) {
    let fixture = Fixture::new(settings, None);
    let initialized = fixture.initialize().unwrap();
    (fixture, initialized)
}
fn standard() -> (Fixture, Value) {
    materials(Settings::default())
}
fn normalized_allowance(
    value: &Value,
    host: Option<&Value>,
    historical: bool,
) -> EngineResult<Value> {
    Ok(helper::validate_allowance(
        value,
        &"RUN-HELPERS".into(),
        host,
        None,
        historical,
        &|text| FILES.from_string(text).is_ok_and(|path| path.is_absolute()),
    )?)
}
fn change_assignments(allowance: &mut Value, mut change: impl FnMut(&mut Value)) {
    let mut assignments = allowance["assignments"].as_list().unwrap().to_vec();
    for assignment in &mut assignments {
        change(assignment);
    }
    allowance.set("assignments", Value::List(assignments));
}
fn replace_inputs(fixture: &Fixture, allowance: &Value, plan: &mut Value) {
    write(
        &fixture.repository.join("docs/helper-allowance.json"),
        allowance,
    );
    set_nested(
        plan,
        &["helper_allowance", "sha256"],
        ids::sha256_bytes(&ids::canonical_bytes(allowance).unwrap()).into(),
    );
    sign_plan(plan);
    write(&fixture.repository.join("docs/helper-plan.json"), plan);
}

pub fn test_retired_allowances_are_auditable_but_new_allowances_reject_them() {
    let (fixture, _) = standard();
    for model in ["gpt-5.6-luna", "gpt-5.6-sol", "gpt-5.6-terra"] {
        let mut allowance = fixture.allowance.clone();
        change_assignments(&mut allowance, |row| {
            row.set("model", model.into());
            row.set("reasoning_effort", "max".into());
        });
        for host in [
            None,
            Some("codex"),
            Some("codex-astra"),
            Some("cursor"),
            Some("claude"),
        ] {
            let host = host.map(Value::from);
            assert_code(
                normalized_allowance(&allowance, host.as_ref(), false),
                "MODEL_RETIRED",
            );
        }
        let normalized =
            normalized_allowance(&allowance, Some(&"codex-astra".into()), true).unwrap();
        assert!(normalized["assignments"]
            .as_list()
            .unwrap()
            .iter()
            .all(|row| row["model"].is_text(model)));
    }
}

pub fn test_historical_register_reads_and_settles_but_rejects_new_retired_dispatch() {
    let (fixture, initialized) = standard();
    let path = register_path(&initialized);
    let mut allowance = fixture.allowance.clone();
    change_assignments(&mut allowance, |row| {
        row.set("model", "gpt-5.6-luna".into());
        row.set("reasoning_effort", "max".into());
    });
    let mut plan = fixture.plan.clone();
    replace_inputs(&fixture, &allowance, &mut plan);
    let normalized = normalized_allowance(&allowance, Some(&"codex-astra".into()), true).unwrap();
    let mut observation = fixture.observation.clone();
    let mut supported = observation["supported_assignments"]
        .as_list()
        .unwrap()
        .to_vec();
    supported[0].set("model", "gpt-5.6-luna".into());
    supported[0].set("reasoning_effort", "max".into());
    observation.set("supported_assignments", Value::List(supported));
    write(
        &fixture.repository.join("docs/host-observation.json"),
        &observation,
    );
    let normalized_observation = helper::validate_host_observation(&observation).unwrap();
    let mut context = initialized["context"].clone();
    context.set(
        "allowance_digest",
        ids::sha256_bytes(&ids::canonical_bytes(&allowance).unwrap()).into(),
    );
    context.set(
        "allowance_payload_digest",
        ids::sha256_bytes(&ids::canonical_bytes(&normalized).unwrap()).into(),
    );
    context.set(
        "host_observation_digest",
        ids::sha256_bytes(&ids::canonical_bytes(&observation).unwrap()).into(),
    );
    context.set(
        "host_observation_payload_digest",
        ids::sha256_bytes(&ids::canonical_bytes(&normalized_observation).unwrap()).into(),
    );
    context.set("plan_digest", plan["plan_digest"].clone());
    let mut request = fixture.request("historical-reservation", "evidence", None);
    request.set("model", "gpt-5.6-luna".into());
    request.set("reasoning_effort", "max".into());
    let amounts = helper_validation::required_amounts(&request).unwrap();
    let mut usage = amounts.clone();
    usage.set("concurrency", 0_i64.into());
    // A durable former-catalog record exercises audit and settlement without an admission bypass.
    let mut record = read(&path);
    record.set("context", context.clone());
    record.set("allowance", normalized);
    record.set("host_observation", normalized_observation);
    record.set("usage", usage);
    record.set("active_concurrency", 1_i64.into());
    record.set(
        "active_resources",
        Value::object([("worktree", 1_i64.into())]),
    );
    set_nested(
        &mut record,
        &["assignment_usage", "evidence"],
        amounts.clone(),
    );
    record.set(
        "reservations",
        Value::object([(
            "historical-reservation",
            Value::object([
                (
                    "request_digest",
                    ids::sha256_bytes(&ids::canonical_bytes(&request).unwrap()).into(),
                ),
                ("assignment_id", "evidence".into()),
                ("status", "active".into()),
                ("reservation", amounts),
                ("resource_keys", Value::list(["worktree".into()])),
                ("settlement", Value::Null),
            ]),
        )]),
    );
    write(&path, &record);
    let registry = fixture.registry(&NO_FAULT);
    assert!(
        registry.status(&path, &context).unwrap()["reservations"]["historical-reservation"]
            ["status"]
            .is_text("active")
    );
    let mut new_request = fixture.request("new-retired-launch", "evidence", None);
    new_request.set("model", "gpt-5.6-luna".into());
    new_request.set("reasoning_effort", "max".into());
    assert_code(
        registry.preflight(&path, &context, &new_request),
        "MODEL_RETIRED",
    );
    assert_code(
        registry.reserve(&path, &context, &new_request),
        "MODEL_RETIRED",
    );
    assert!(registry
        .settle(&path, &context, &settlement(&request, "failed"))
        .unwrap()["code"]
        .is_text("SETTLED"));
}

pub fn test_plan_binds_allowance_and_register_context_is_immutable() {
    let (fixture, initialized) = standard();
    let context = &initialized["context"];
    assert_eq!(fixture.plan["schema_version"].as_i64(), Some(3));
    assert!(fixture.allowance.get("plan_digest").is_none());
    assert!(fixture.plan["helper_allowance"]["sha256"].py_eq(&context["allowance_digest"]));
    assert!(fixture.plan["plan_digest"].py_eq(&context["plan_digest"]));
    let normalized =
        normalized_allowance(&fixture.allowance, Some(&"codex-astra".into()), false).unwrap();
    assert!(
        context["allowance_payload_digest"].is_text(&ids::sha256_bytes(
            &ids::canonical_bytes(&normalized).unwrap()
        ))
    );
    assert_eq!(
        PathBuf::from(context["register_path"].text().unwrap()),
        register_path(&initialized)
    );
    assert!(fixture.initialize().unwrap()["code"].is_text("REPLAYED"));
}

pub fn test_runtime_allowance_and_checkpoint_are_bound_to_runtime_home() {
    let (fixture, _) = standard();
    let runtime = fixture.root.join("runtime-helper-home");
    let artifacts = runtime.join("artifacts/RUN-HELPERS");
    std::fs::create_dir_all(&artifacts).unwrap();
    let scope = "runtime:artifacts/RUN-HELPERS/";
    let mut allowance = fixture.allowance.clone();
    change_assignments(&mut allowance, |row| {
        row.set("scope_refs", Value::list([scope.into()]));
        let mut capabilities = row["parent_capabilities"].as_list().unwrap().to_vec();
        capabilities[0].set("target_ref", scope.into());
        row.set("parent_capabilities", Value::List(capabilities));
    });
    let allowance_path = artifacts.join("helper-allowance.json");
    write(&allowance_path, &allowance);
    let mut plan = fixture.plan.clone();
    plan.set(
        "helper_allowance",
        Value::object([
            ("ref", format!("{scope}helper-allowance.json").into()),
            (
                "sha256",
                ids::sha256_bytes(&std::fs::read(&allowance_path).unwrap()).into(),
            ),
        ]),
    );
    sign_plan(&mut plan);
    let plan_path = fixture.repository.join("docs/runtime-helper-plan.json");
    write(&plan_path, &plan);
    let registry = fixture.registry(&NO_FAULT);
    let initialized = registry
        .initialize(
            &runtime,
            &fixture.repository,
            &"RUN-HELPERS".into(),
            &plan_path,
            &allowance_path,
            &fixture.repository.join("docs/host-observation.json"),
        )
        .unwrap();
    let path = register_path(&initialized);
    let context = &initialized["context"];
    assert!(context["allowance_ref"].is_text(&format!("{scope}helper-allowance.json")));
    assert!(registry.status(&path, context).unwrap()["code"].is_text("STATUS"));
    assert_code(
        registry.initialize(
            &runtime,
            &fixture.repository,
            &"RUN-HELPERS".into(),
            &plan_path,
            &fixture.repository.join("docs/helper-allowance.json"),
            &fixture.repository.join("docs/host-observation.json"),
        ),
        "ALLOWANCE_REF_MISMATCH",
    );
    let checkpoint = artifacts.join("checkpoint.json");
    std::fs::write(&checkpoint, b"{\"state\":\"observed\"}").unwrap();
    let mut request = fixture.request("runtime-checkpoint", "evidence", None);
    request.set("scope_refs", Value::list([scope.into()]));
    request.set(
        "checkpoint_ref",
        format!(
            "{scope}checkpoint.json#sha256={}",
            ids::sha256_bytes(&std::fs::read(&checkpoint).unwrap())
        )
        .into(),
    );
    assert!(
        registry.preflight(&path, context, &request).unwrap()["code"].is_text("PREFLIGHT_READY")
    );
    assert!(registry.reserve(&path, context, &request).unwrap()["code"].is_text("RESERVED"));
    let mut outside = request.clone();
    outside.set("request_id", "outside-scope".into());
    outside.set(
        "scope_refs",
        Value::list(["runtime:artifacts/other/".into()]),
    );
    assert_code(
        registry.preflight(&path, context, &outside),
        "SCOPE_EXCEEDED",
    );
    let mut wrong_root = request;
    wrong_root.set("request_id", "wrong-root".into());
    wrong_root.set(
        "checkpoint_ref",
        format!(
            "repo:docs/checkpoint.json#sha256={}",
            ids::sha256_bytes(&std::fs::read(&checkpoint).unwrap())
        )
        .into(),
    );
    assert_code(
        registry.preflight(&path, context, &wrong_root),
        "SCOPE_EXCEEDED",
    );
    allowance.set("allowance_id", "changed".into());
    write(&allowance_path, &allowance);
    assert_code(registry.status(&path, context), "REGISTER_BINDING_MISMATCH");
}

pub fn test_runtime_allowance_ref_rejects_traversal() {
    let (fixture, _) = standard();
    let mut plan = fixture.plan.clone();
    set_nested(
        &mut plan,
        &["helper_allowance", "ref"],
        "runtime:artifacts/../outside.json".into(),
    );
    sign_plan(&mut plan);
    let plan_path = fixture.repository.join("docs/unsafe-helper-plan.json");
    write(&plan_path, &plan);
    assert_code(
        fixture.registry(&NO_FAULT).initialize(
            &fixture.root.join("unsafe-helper-home"),
            &fixture.repository,
            &"RUN-HELPERS".into(),
            &plan_path,
            &fixture.repository.join("docs/helper-allowance.json"),
            &fixture.repository.join("docs/host-observation.json"),
        ),
        "PATH_TRAVERSAL",
    );
}

pub fn test_reopened_register_rejects_mutated_immutable_payloads_and_identities() {
    let (fixture, initialized) = standard();
    let path = register_path(&initialized);
    let context = &initialized["context"];
    let bytes = std::fs::read(&path).unwrap();
    for (keys, value, operation) in [
        (
            vec!["allowance", "shared_limits", "children"],
            3_i64.into(),
            "status",
        ),
        (
            vec!["host_observation", "uncertainty"],
            "changed".into(),
            "preflight",
        ),
        (
            vec!["allowance_ref"],
            "repo:docs/other.json".into(),
            "reserve",
        ),
        (
            vec!["repository_identity", "inode"],
            123_i64.into(),
            "reopen",
        ),
    ] {
        let mut record = graph_engine::compat::loads(&bytes).unwrap();
        set_nested(&mut record, &keys, value);
        write(&path, &record);
        let reopened = fixture.registry(&NO_FAULT);
        let result = match operation {
            "preflight" => reopened.preflight(
                &path,
                context,
                &fixture.request("mutated-host", "evidence", None),
            ),
            "reserve" => reopened.reserve(
                &path,
                context,
                &fixture.request("mutated-reference", "evidence", None),
            ),
            _ => reopened.status(&path, context),
        };
        assert_code(result, "REGISTER_INVALID");
        std::fs::write(&path, &bytes).unwrap();
    }
}

pub fn test_initialization_replay_compares_all_immutable_inputs() {
    let (fixture, initialized) = standard();
    let mut observation = fixture.observation.clone();
    observation.set("uncertainty", "new observation bytes".into());
    write(
        &fixture.repository.join("docs/host-observation.json"),
        &observation,
    );
    assert_code(fixture.initialize(), "INITIALIZATION_CONFLICT");
    assert!(fixture
        .registry(&NO_FAULT)
        .status(&register_path(&initialized), &initialized["context"])
        .unwrap()["code"]
        .is_text("STATUS"));
}

pub fn test_same_run_cannot_reset_budgets_with_a_replacement_plan() {
    let (fixture, initialized) = standard();
    let path = register_path(&initialized);
    let context = &initialized["context"];
    let registry = fixture.registry(&NO_FAULT);
    registry
        .reserve(
            &path,
            context,
            &fixture.request("used-budget", "evidence", None),
        )
        .unwrap();
    let mut allowance = fixture.allowance.clone();
    allowance.set("allowance_id", "helpers-replacement".into());
    let mut plan = fixture.plan.clone();
    replace_inputs(&fixture, &allowance, &mut plan);
    assert_code(fixture.initialize(), "INITIALIZATION_CONFLICT");
    let directories = std::fs::read_dir(fixture.root.join("host-state/helper-registers"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("register.json").is_file())
        .count();
    assert_eq!(directories, 1);
    write(
        &fixture.repository.join("docs/helper-allowance.json"),
        &fixture.allowance,
    );
    write(
        &fixture.repository.join("docs/helper-plan.json"),
        &fixture.plan,
    );
    assert_eq!(
        registry.status(&path, context).unwrap()["usage"]["children"].as_i64(),
        Some(1)
    );
}

pub fn test_reservation_replay_precedes_checkpoint_and_authority_revalidation() {
    let (fixture, initialized) = standard();
    let registry = fixture.registry(&NO_FAULT);
    let path = register_path(&initialized);
    let context = &initialized["context"];
    let request = fixture.request("durable-replay", "evidence", None);
    assert!(
        registry.reserve(&path, context, &request).unwrap()["new_dispatch_authorized"]
            .py_eq(&true.into())
    );
    let checkpoint = fixture
        .repository
        .join("docs/checkpoint-durable-replay.json");
    std::fs::write(&checkpoint, b"changed").unwrap();
    let replay = registry.reserve(&path, context, &request).unwrap();
    assert!(replay["code"].is_text("REPLAYED"));
    assert!(replay["new_dispatch_authorized"].py_eq(&false.into()));
    std::fs::remove_file(checkpoint).unwrap();
    assert!(registry.reserve(&path, context, &request).unwrap()["code"].is_text("REPLAYED"));
    for (field, value) in [
        ("scope_refs", Value::list(["repo:docs/artifacts/".into()])),
        ("assignment_id", "validation".into()),
    ] {
        let mut changed = request.clone();
        changed.set(field, value);
        assert_code(
            registry.reserve(&path, context, &changed),
            "RESERVATION_CONFLICT",
        );
    }
}

pub fn test_state_root_and_register_location_are_bound_to_context() {
    let (fixture, initialized) = standard();
    let registry = fixture.registry(&NO_FAULT);
    let alternate = registry
        .initialize(
            &fixture.root.join("alternate-state"),
            &fixture.repository,
            &"RUN-HELPERS".into(),
            &fixture.repository.join("docs/helper-plan.json"),
            &fixture.repository.join("docs/helper-allowance.json"),
            &fixture.repository.join("docs/host-observation.json"),
        )
        .unwrap();
    assert!(!initialized["context"]["register_id"].py_eq(&alternate["context"]["register_id"]));
    assert_code(
        registry.status(&register_path(&alternate), &initialized["context"]),
        "REGISTER_BINDING_MISMATCH",
    );
    let copied_directory = fixture
        .root
        .join("copied")
        .join(initialized["context"]["register_id"].text().unwrap());
    std::fs::create_dir_all(&copied_directory).unwrap();
    let copied = copied_directory.join("register.json");
    std::fs::write(&copied, std::fs::read(register_path(&initialized)).unwrap()).unwrap();
    assert_code(
        registry.status(&copied, &initialized["context"]),
        "REGISTER_BINDING_MISMATCH",
    );
}

pub fn test_mutations_reject_external_register_paths_before_touching_locks() {
    let (fixture, initialized) = standard();
    let registry = fixture.registry(&NO_FAULT);
    let request = fixture.request("external-path", "evidence", None);
    let terminal = settlement(&request, "failed");
    for operation in ["reserve", "settle"] {
        for lock_exists in [false, true] {
            let directory = fixture
                .root
                .join("external")
                .join(operation)
                .join(if lock_exists { "True" } else { "False" });
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join("register.json");
            std::fs::write(&path, b"external register sentinel").unwrap();
            let lock = directory.join("register.lock");
            if lock_exists {
                std::fs::write(&lock, b"external lock sentinel").unwrap();
            }
            let contents = || {
                let mut entries = std::fs::read_dir(&directory)
                    .unwrap()
                    .map(|entry| {
                        let entry = entry.unwrap();
                        (entry.file_name(), std::fs::read(entry.path()).unwrap())
                    })
                    .collect::<Vec<_>>();
                entries.sort();
                entries
            };
            let before = contents();
            let result = if operation == "reserve" {
                registry.reserve(&path, &initialized["context"], &request)
            } else {
                registry.settle(&path, &initialized["context"], &terminal)
            };
            assert_code(result, "REGISTER_BINDING_MISMATCH");
            assert_eq!(contents(), before);
        }
    }
}

pub fn test_missing_observed_host_restriction_blocks_without_reservation() {
    let (fixture, initialized) = materials(Settings {
        unsupported: true,
        ..Settings::default()
    });
    let registry = fixture.registry(&NO_FAULT);
    let path = register_path(&initialized);
    let context = &initialized["context"];
    let result = registry
        .preflight(
            &path,
            context,
            &fixture.request("request-1", "evidence", None),
        )
        .unwrap();
    assert!(result["ok"].py_eq(&false.into()));
    assert!(result["code"].is_text("BLOCKED_UNSUPPORTED"));
    assert!(result["missing_host_capabilities"]
        .container_eq(&Value::list(["filesystem_confinement".into()])));
    assert!(
        registry.status(&path, context).unwrap()["reservations"].container_eq(&Value::object([]))
    );
}

pub fn test_test_mode_reserves_and_labels_unenforced_isolation_without_faking_observation() {
    let (fixture, initialized) = materials(Settings {
        unsupported: true,
        test_mode: true,
        ..Settings::default()
    });
    let registry = fixture.registry(&NO_FAULT);
    let path = register_path(&initialized);
    let context = &initialized["context"];
    let request = fixture.request(
        "cooperative-validation",
        "validation",
        Some(fixture.command.clone()),
    );
    let terminal = settlement(&request, "succeeded");
    let results = [
        initialized.clone(),
        registry.preflight(&path, context, &request).unwrap(),
        registry.reserve(&path, context, &request).unwrap(),
        registry.reserve(&path, context, &request).unwrap(),
        registry.settle(&path, context, &terminal).unwrap(),
        registry.settle(&path, context, &terminal).unwrap(),
        registry.status(&path, context).unwrap(),
    ];
    for result in &results {
        assert!(result["ok"].py_eq(&true.into()));
        assert!(result["execution_mode"].is_text("cooperative_test"));
        assert!(result["production_evidence"].py_eq(&false.into()));
        assert!(
            result["unenforced_capabilities"].container_eq(&Value::list([
                "command_confinement".into(),
                "filesystem_confinement".into(),
                "tool_confinement".into()
            ]))
        );
    }
    assert!(results[3]["new_dispatch_authorized"].py_eq(&false.into()));
    assert!(
        read(&path)["host_observation"]["capabilities"]["filesystem_confinement"]["status"]
            .is_text("unavailable")
    );
    assert_eq!(results[6]["usage"]["children"].as_i64(), Some(1));
}

pub fn test_test_mode_still_blocks_unverified_model_and_fresh_assignment_support() {
    let (fixture, initialized) = materials(Settings {
        unsupported: true,
        test_mode: true,
        observed_model: Some("gpt-5.6-sol"),
        ..Settings::default()
    });
    let registry = fixture.registry(&NO_FAULT);
    let path = register_path(&initialized);
    let result = registry
        .reserve(
            &path,
            &initialized["context"],
            &fixture.request("request-1", "evidence", None),
        )
        .unwrap();
    assert!(result["code"].is_text("BLOCKED_UNSUPPORTED"));
    assert!(result["missing_host_capabilities"]
        .container_eq(&Value::list(["model_effort_assignment".into()])));
    assert!(result["production_evidence"].py_eq(&false.into()));
    assert!(
        registry.status(&path, &initialized["context"]).unwrap()["reservations"]
            .container_eq(&Value::object([]))
    );
}

pub fn test_test_mode_does_not_relax_scope_commands_or_shared_budgets() {
    let (fixture, initialized) = materials(Settings {
        unsupported: true,
        test_mode: true,
        ..Settings::default()
    });
    let registry = fixture.registry(&NO_FAULT);
    let path = register_path(&initialized);
    let context = &initialized["context"];
    let mut wrong_scope = fixture.request("outside", "evidence", None);
    wrong_scope.set("scope_refs", Value::list(["repo:src/".into()]));
    assert_code(
        registry.reserve(&path, context, &wrong_scope),
        "SCOPE_EXCEEDED",
    );
    let mut command = fixture.command.clone();
    command.set(
        "argv",
        Value::list([
            fixture.command["argv"].as_list().unwrap()[0].clone(),
            "-V".into(),
        ]),
    );
    assert_code(
        registry.reserve(
            &path,
            context,
            &fixture.request("wrong-command", "validation", Some(command)),
        ),
        "COMMAND_NOT_APPROVED",
    );
    for identifier in ["one", "two"] {
        let request = fixture.request(identifier, "evidence", None);
        registry.reserve(&path, context, &request).unwrap();
        registry
            .settle(&path, context, &settlement(&request, "failed"))
            .unwrap();
    }
    assert_code(
        registry.reserve(&path, context, &fixture.request("three", "evidence", None)),
        "SHARED_LIMIT_EXCEEDED",
    );
}

pub fn test_test_mode_requires_explicit_versioned_acknowledgment() {
    let (fixture, _) = materials(Settings {
        test_mode: true,
        ..Settings::default()
    });
    for (field, replacement) in [
        ("acknowledge_unenforced_isolation", false.into()),
        ("acknowledge_unenforced_isolation", 1_i64.into()),
        ("external_effects", "live".into()),
        ("disposable_repository", "relative/repo".into()),
    ] {
        let mut allowance = fixture.allowance.clone();
        set_nested(&mut allowance, &["test_mode", field], replacement);
        assert!(normalized_allowance(&allowance, Some(&"codex-astra".into()), false).is_err());
    }
    let mut version_one = fixture.allowance.clone();
    version_one.set("schema_version", 1_i64.into());
    assert!(normalized_allowance(&version_one, Some(&"codex-astra".into()), false).is_err());
    let mut missing = fixture.allowance.clone();
    remove(&mut missing, "test_mode");
    assert!(normalized_allowance(&missing, Some(&"codex-astra".into()), false).is_err());
}

pub fn test_test_mode_requires_verified_fresh_model_selection() {
    let (fixture, initialized) = materials(Settings {
        unsupported: true,
        test_mode: true,
        fresh_selection: false,
        ..Settings::default()
    });
    let registry = fixture.registry(&NO_FAULT);
    let path = register_path(&initialized);
    let result = registry
        .preflight(
            &path,
            &initialized["context"],
            &fixture.request("request-1", "evidence", None),
        )
        .unwrap();
    assert!(result["code"].is_text("BLOCKED_UNSUPPORTED"));
    assert!(result["missing_host_capabilities"]
        .container_eq(&Value::list(["fresh_model_effort_selection".into()])));
    assert!(
        registry.status(&path, &initialized["context"]).unwrap()["reservations"]
            .container_eq(&Value::object([]))
    );
}

pub fn test_test_mode_rejects_wrong_repository_before_register_initialization() {
    let (fixture, _) = materials(Settings {
        test_mode: true,
        ..Settings::default()
    });
    let mut allowance = fixture.allowance.clone();
    set_nested(
        &mut allowance,
        &["test_mode", "disposable_repository"],
        Value::String(FILES.spelling(&fixture.root.join("different-repo"))),
    );
    let mut plan = fixture.plan.clone();
    replace_inputs(&fixture, &allowance, &mut plan);
    let state = fixture.root.join("different-state");
    assert_code(
        fixture.registry(&NO_FAULT).initialize(
            &state,
            &fixture.repository,
            &"RUN-HELPERS".into(),
            &fixture.repository.join("docs/helper-plan.json"),
            &fixture.repository.join("docs/helper-allowance.json"),
            &fixture.repository.join("docs/host-observation.json"),
        ),
        "TEST_REPOSITORY_REQUIRED",
    );
    assert!(!state.join("helper-registers").exists());
}

pub fn test_test_mode_cannot_change_after_approval() {
    let (fixture, initialized) = materials(Settings {
        test_mode: true,
        ..Settings::default()
    });
    let mut allowance = fixture.allowance.clone();
    allowance.set("schema_version", 1_i64.into());
    remove(&mut allowance, "test_mode");
    write(
        &fixture.repository.join("docs/helper-allowance.json"),
        &allowance,
    );
    assert_code(
        fixture
            .registry(&NO_FAULT)
            .status(&register_path(&initialized), &initialized["context"]),
        "REGISTER_BINDING_MISMATCH",
    );
}

struct DifferentTemporaryRoot<'a> {
    inner: &'a dyn HelperFileTransactions,
    root: PathBuf,
}
impl HelperFileTransactions for DifferentTemporaryRoot<'_> {
    fn temporary_root(&self) -> EngineResult<PathBuf> {
        Ok(self.root.clone())
    }
    fn atomic_write(&self, path: &Path, payload: &[u8], fault: &dyn FaultHook) -> EngineResult<()> {
        self.inner.atomic_write(path, payload, fault)
    }
    fn exclusive_lock(
        &self,
        path: &Path,
        action: &mut dyn FnMut() -> EngineResult<Value>,
    ) -> EngineResult<Value> {
        self.inner.exclusive_lock(path, action)
    }
}
pub fn test_test_mode_rechecks_disposable_repository_binding() {
    let (fixture, initialized) = materials(Settings {
        test_mode: true,
        ..Settings::default()
    });
    let transactions = DifferentTemporaryRoot {
        inner: &TRANSACTIONS,
        root: fixture.root.join("other"),
    };
    let mut registry = fixture.registry(&NO_FAULT);
    registry.repository.transactions = &transactions;
    assert_code(
        registry.status(&register_path(&initialized), &initialized["context"]),
        "TEST_REPOSITORY_REQUIRED",
    );
}

pub fn test_approved_host_supported_assignment_override_is_preserved_exactly() {
    let (fixture, initialized) = materials(Settings {
        model: "gpt-6-astra",
        effort: "medium",
        ..Settings::default()
    });
    let mut request = fixture.request("astra-override", "evidence", None);
    request.set("model", "gpt-6-astra".into());
    request.set("reasoning_effort", "medium".into());
    assert!(fixture
        .registry(&NO_FAULT)
        .preflight(
            &register_path(&initialized),
            &initialized["context"],
            &request
        )
        .unwrap()["code"]
        .is_text("PREFLIGHT_READY"));
}

pub fn test_helper_allowances_accept_selected_models_from_each_harness() {
    let (fixture, _) = standard();
    let mut selections = vec![
        ("codex-astra", "gpt-6-sol", "medium"),
        ("codex-astra", "gpt-6.1-sol", "low"),
        ("claude", "claude-opus-5", "medium"),
        ("claude", "claude-sonnet-5", "low"),
        ("claude", "claude-opus-5-5", "high"),
        ("claude", "claude-sonnet-5-5", "low"),
        ("cursor", "grok-4.7", "medium"),
        ("cursor", "gemini-3.8-flash", "low"),
    ];
    for host in ["codex", "codex-astra"] {
        for effort in ["low", "medium", "high", "xhigh", "max", "ultra"] {
            selections.push((host, "gpt-6.1-terra", effort));
        }
    }
    for (host, model, effort) in selections {
        let mut allowance = fixture.allowance.clone();
        change_assignments(&mut allowance, |row| {
            row.set("model", model.into());
            row.set("reasoning_effort", effort.into());
        });
        let normalized = normalized_allowance(&allowance, Some(&host.into()), false).unwrap();
        for row in normalized["assignments"].as_list().unwrap() {
            assert!(row["model"].is_text(model));
            assert!(row["reasoning_effort"].is_text(effort));
        }
        assert!(normalized_allowance(&allowance, Some(&host.into()), true)
            .unwrap()
            .container_eq(&normalized));
    }
}

pub fn test_terra_helper_preflight_retains_capability_and_assignment_checks() {
    let (fixture, initialized) = materials(Settings {
        model: "gpt-6.1-terra",
        effort: "ultra",
        ..Settings::default()
    });
    let registry = fixture.registry(&NO_FAULT);
    let path = register_path(&initialized);
    let context = &initialized["context"];
    let mut request = fixture.request("terra-helper", "evidence", None);
    request.set("model", "gpt-6.1-terra".into());
    request.set("reasoning_effort", "ultra".into());
    assert!(
        registry.preflight(&path, context, &request).unwrap()["code"].is_text("PREFLIGHT_READY")
    );
    request.set("reasoning_effort", "high".into());
    assert_code(
        registry.preflight(&path, context, &request),
        "HELPER_ASSIGNMENT_MISMATCH",
    );
    assert!(
        registry.status(&path, context).unwrap()["reservations"].container_eq(&Value::object([]))
    );
}

pub fn test_revision_nine_helper_allowance_cannot_acquire_terra_authority() {
    let fixture = Fixture::new(Settings::default(), Some(9));
    let initialized = fixture.initialize().unwrap();
    let registry = fixture.registry(&NO_FAULT);
    let path = register_path(&initialized);
    let context = &initialized["context"];
    assert_eq!(fixture.plan["catalog_revision"].as_i64(), Some(9));
    let mut request = fixture.request("new-terra-authority", "evidence", None);
    request.set("model", "gpt-6.1-terra".into());
    request.set("reasoning_effort", "ultra".into());
    assert_code(
        registry.preflight(&path, context, &request),
        "HELPER_ASSIGNMENT_MISMATCH",
    );
    assert!(
        registry.status(&path, context).unwrap()["reservations"].container_eq(&Value::object([]))
    );
    let mut allowance = fixture.allowance.clone();
    change_assignments(&mut allowance, |row| {
        row.set("model", "gpt-6.1-terra".into());
        row.set("reasoning_effort", "ultra".into());
    });
    write(
        &fixture.repository.join("docs/helper-allowance.json"),
        &allowance,
    );
    assert_code(
        registry.preflight(&path, context, &request),
        "REGISTER_BINDING_MISMATCH",
    );
}

pub fn test_muse_graph_option_does_not_create_helper_runtime_authority() {
    let (fixture, _) = standard();
    let mut allowance = fixture.allowance.clone();
    change_assignments(&mut allowance, |row| {
        row.set("model", "opencode-go/muse-spark-1.3-contributor".into());
        row.set("reasoning_effort", "low".into());
    });
    assert_code(
        normalized_allowance(&allowance, Some(&"codex-astra".into()), false),
        "HELPER_RUNTIME_UNSUPPORTED",
    );
}

pub fn test_observed_assignment_mismatch_blocks_without_substitution() {
    let (fixture, initialized) = materials(Settings {
        observed_model: Some("gpt-5.6-sol"),
        observed_effort: Some("high"),
        ..Settings::default()
    });
    let result = fixture
        .registry(&NO_FAULT)
        .preflight(
            &register_path(&initialized),
            &initialized["context"],
            &fixture.request("host-mismatch", "evidence", None),
        )
        .unwrap();
    assert!(result["ok"].py_eq(&false.into()));
    assert!(result["code"].is_text("BLOCKED_UNSUPPORTED"));
    assert!(result["missing_host_capabilities"]
        .container_eq(&Value::list(["model_effort_assignment".into()])));
}

pub fn test_assignment_scope_command_and_mandatory_resource_keys_fail_closed() {
    let (fixture, initialized) = standard();
    let registry = fixture.registry(&NO_FAULT);
    let path = register_path(&initialized);
    let context = &initialized["context"];
    let mut wrong_model = fixture.request("wrong-model", "evidence", None);
    wrong_model.set("model", "gpt-5.6-sol".into());
    let mut wrong_scope = fixture.request("wrong-scope", "evidence", None);
    wrong_scope.set("scope_refs", Value::list(["repo:src/".into()]));
    let mut missing_resource = fixture.request("missing-resource", "evidence", None);
    missing_resource.set("resource_keys", Value::list([]));
    let mut command = fixture.command.clone();
    command.set(
        "argv",
        Value::list([
            fixture.command["argv"].as_list().unwrap()[0].clone(),
            "-V".into(),
        ]),
    );
    let wrong_command = fixture.request("wrong-command", "validation", Some(command));
    for request in [wrong_model, wrong_scope, missing_resource, wrong_command] {
        assert!(matches!(
            registry.preflight(&path, context, &request),
            Err(EngineError::Contract(_))
        ));
    }
}

pub fn test_allowance_requires_declared_resources_and_assignment_keys() {
    let (fixture, _) = standard();
    let mut no_resources = fixture.allowance.clone();
    no_resources.set("resources", Value::list([]));
    let mut no_assignment_resources = fixture.allowance.clone();
    let mut assignments = no_assignment_resources["assignments"]
        .as_list()
        .unwrap()
        .to_vec();
    assignments[0].set("resource_keys", Value::list([]));
    no_assignment_resources.set("assignments", Value::List(assignments));
    for allowance in [no_resources, no_assignment_resources] {
        assert_code(
            normalized_allowance(&allowance, Some(&"codex-astra".into()), false),
            "INVALID_LIST",
        );
    }
}

pub fn test_allowance_may_not_exceed_approved_parent_capabilities() {
    let (fixture, _) = standard();
    let mut exceeded_scope = fixture.allowance.clone();
    let mut assignments = exceeded_scope["assignments"].as_list().unwrap().to_vec();
    assignments[0].set("scope_refs", Value::list(["repo:src/".into()]));
    exceeded_scope.set("assignments", Value::List(assignments));
    let mut exceeded_command = fixture.allowance.clone();
    let mut assignments = exceeded_command["assignments"].as_list().unwrap().to_vec();
    assignments[1].set(
        "parent_capabilities",
        json(r#"[{"effect":"filesystem_read","action":"read","target_ref":"repo:docs/"}]"#),
    );
    exceeded_command.set("assignments", Value::List(assignments));
    for allowance in [exceeded_scope, exceeded_command] {
        assert_code(
            normalized_allowance(&allowance, Some(&"codex-astra".into()), false),
            "PARENT_AUTHORITY_EXCEEDED",
        );
    }
}

pub fn test_settlement_releases_only_active_resources_and_never_refunds_budgets() {
    let (fixture, initialized) = standard();
    let registry = fixture.registry(&NO_FAULT);
    let path = register_path(&initialized);
    let context = &initialized["context"];
    let first = fixture.request("validation-1", "validation", Some(fixture.command.clone()));
    let reserved = registry.reserve(&path, context, &first).unwrap();
    assert!(registry.reserve(&path, context, &first).unwrap()["code"].is_text("REPLAYED"));
    let terminal = settlement(&first, "failed");
    assert!(registry.settle(&path, context, &terminal).unwrap()["code"].is_text("SETTLED"));
    assert!(registry.settle(&path, context, &terminal).unwrap()["code"].is_text("REPLAYED"));
    let mut changed = first.clone();
    set_nested(&mut changed, &["budgets", "output_tokens"], 999_i64.into());
    assert_code(
        registry.reserve(&path, context, &changed),
        "RESERVATION_CONFLICT",
    );
    let second = fixture.request("validation-2", "validation", Some(fixture.command.clone()));
    registry.reserve(&path, context, &second).unwrap();
    registry
        .settle(&path, context, &settlement(&second, "replaced"))
        .unwrap();
    let status = registry.status(&path, context).unwrap();
    assert_eq!(status["usage"]["children"].as_i64(), Some(2));
    assert_eq!(status["usage"]["commands"].as_i64(), Some(2));
    assert_eq!(status["active_concurrency"].as_i64(), Some(0));
    assert_eq!(status["active_resources"]["worktree"].as_i64(), Some(0));
    assert_code(
        registry.reserve(
            &path,
            context,
            &fixture.request("validation-3", "validation", Some(fixture.command.clone())),
        ),
        "SHARED_LIMIT_EXCEEDED",
    );
    assert!(reserved["request_digest"].py_eq(&terminal["request_digest"]));
}

struct Interrupted;
impl FaultHook for Interrupted {
    fn observe(&self, point: &str) -> EngineResult<()> {
        assert_eq!(point, "before_replace");
        Err(EngineError::python("RuntimeError", "interrupted"))
    }
}
pub fn test_interrupted_atomic_replace_preserves_prior_register() {
    let (fixture, initialized) = standard();
    let path = register_path(&initialized);
    let context = &initialized["context"];
    let request = fixture.request("interrupted", "evidence", None);
    let bytes = std::fs::read(&path).unwrap();
    let error = fixture
        .registry(&Interrupted)
        .reserve(&path, context, &request)
        .unwrap_err();
    match error {
        EngineError::Python { kind, message } => {
            assert_eq!(kind, "RuntimeError");
            assert!(message.equals("interrupted"));
        }
        other => panic!("{other:?}"),
    };
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let clean = fixture.registry(&NO_FAULT);
    assert!(clean.status(&path, context).unwrap()["reservations"].container_eq(&Value::object([])));
    assert!(std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .any(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            name.starts_with(".register.") && name.ends_with(".tmp")
        }));
    assert!(clean.reserve(&path, context, &request).unwrap()["code"].is_text("RESERVED"));
}

struct PublicOnlyPort<'a> {
    repository: &'a dyn HelperRegisterPersistence,
}
impl HelperRegisterPersistence for PublicOnlyPort<'_> {
    fn read_bound(&self, path: &Path, context: &Value) -> EngineResult<(Value, Value)> {
        self.repository.read_bound(path, context)
    }
    fn transaction(
        &self,
        path: &Path,
        context: &Value,
        action: &mut dyn FnMut(&mut dyn RegisterTransaction) -> EngineResult<Value>,
    ) -> EngineResult<Value> {
        self.repository.transaction(path, context, action)
    }
}
pub fn test_ledger_operates_through_public_persistence_port() {
    let (fixture, initialized) = standard();
    let registry = fixture.registry(&NO_FAULT);
    let port = PublicOnlyPort {
        repository: &registry.repository,
    };
    let ledger = HelperReservationLedger {
        repository: &port,
        files: &*FILES,
    };
    let path = register_path(&initialized);
    let context = &initialized["context"];
    let request = fixture.request("public-port", "evidence", None);
    assert!(ledger.preflight(&path, context, &request).unwrap()["code"].is_text("PREFLIGHT_READY"));
    assert!(ledger.reserve(&path, context, &request).unwrap()["code"].is_text("RESERVED"));
    assert!(ledger
        .settle(&path, context, &settlement(&request, "failed"))
        .unwrap()["code"]
        .is_text("SETTLED"));
    assert_eq!(
        ledger.status(&path, context).unwrap()["usage"]["children"].as_i64(),
        Some(1)
    );
}

fn helper_binary() -> PathBuf {
    if let Some(path) = std::env::var_os("GRAPH_RUST_HELPER_BINARY") {
        return path.into();
    }
    let executable = std::env::current_exe().unwrap();
    let directory = executable.parent().unwrap();
    let directory = if directory.file_name().is_some_and(|name| name == "deps") {
        directory.parent().unwrap()
    } else {
        directory
    };
    directory.join(if cfg!(windows) {
        "helper-register.exe"
    } else {
        "helper-register"
    })
}

pub fn test_malformed_nested_register_returns_structured_invalid_error() {
    let (fixture, initialized) = standard();
    let path = register_path(&initialized);
    let context = &initialized["context"];
    let mut record = read(&path);
    record.set("usage", Value::Null);
    write(&path, &record);
    assert_code(
        fixture.registry(&NO_FAULT).status(&path, context),
        "REGISTER_INVALID",
    );
    let context_path = fixture.root.join("malformed-context.json");
    write(&context_path, context);
    let completed = std::process::Command::new(helper_binary())
        .arg("status")
        .arg("--register")
        .arg(&path)
        .arg("--context")
        .arg(context_path)
        .output()
        .unwrap();
    assert_eq!(completed.status.code(), Some(4));
    assert!(completed.stderr.is_empty(), "{:?}", completed.stderr);
    assert!(graph_engine::compat::loads(&completed.stdout)
        .unwrap()
        .container_eq(&json(r#"{"ok":false,"code":"REGISTER_INVALID"}"#)));
    assert_eq!(
        completed.stdout,
        if cfg!(windows) {
            b"{\"code\":\"REGISTER_INVALID\",\"ok\":false}\r\n".as_slice()
        } else {
            b"{\"code\":\"REGISTER_INVALID\",\"ok\":false}\n".as_slice()
        }
    );
}

pub fn test_cross_process_lock_allows_only_one_atomic_active_reservation() {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let (fixture, initialized) = standard();
    let context_path = fixture.root.join("context.json");
    write(&context_path, &initialized["context"]);
    let request_paths = ["parallel-1", "parallel-2"].map(|identifier| {
        let path = fixture.root.join(format!("{identifier}.json"));
        write(&path, &fixture.request(identifier, "evidence", None));
        path
    });
    let mut processes = request_paths
        .iter()
        .map(|request| {
            Command::new(helper_binary())
                .arg("reserve")
                .arg("--register")
                .arg(register_path(&initialized))
                .arg("--context")
                .arg(&context_path)
                .arg("--request")
                .arg(request)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect::<Vec<_>>();
    let mut results = Vec::new();
    for mut process in processes.drain(..) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if process.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                process.kill().unwrap();
                let output = process.wait_with_output().unwrap();
                panic!("helper process exceeded15seconds: {output:?}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let completed = process.wait_with_output().unwrap();
        assert!(completed.stderr.is_empty(), "{:?}", completed.stderr);
        let response = graph_engine::compat::loads(&completed.stdout).unwrap();
        assert_eq!(
            completed.status.code(),
            Some(if response["code"].is_text("RESERVED") {
                0
            } else {
                4
            })
        );
        results.push(response["code"].text().unwrap());
    }
    results.sort();
    assert_eq!(results, ["RESERVED", "SHARED_LIMIT_EXCEEDED"]);
    let status = fixture
        .registry(&NO_FAULT)
        .status(&register_path(&initialized), &initialized["context"])
        .unwrap();
    assert_eq!(status["usage"]["children"].as_i64(), Some(1));
    assert_eq!(status["active_concurrency"].as_i64(), Some(1));
}

pub const CASES: &[(&str, fn())] = &[
    (
        "test_retired_allowances_are_auditable_but_new_allowances_reject_them",
        test_retired_allowances_are_auditable_but_new_allowances_reject_them,
    ),
    (
        "test_historical_register_reads_and_settles_but_rejects_new_retired_dispatch",
        test_historical_register_reads_and_settles_but_rejects_new_retired_dispatch,
    ),
    (
        "test_plan_binds_allowance_and_register_context_is_immutable",
        test_plan_binds_allowance_and_register_context_is_immutable,
    ),
    (
        "test_runtime_allowance_and_checkpoint_are_bound_to_runtime_home",
        test_runtime_allowance_and_checkpoint_are_bound_to_runtime_home,
    ),
    (
        "test_runtime_allowance_ref_rejects_traversal",
        test_runtime_allowance_ref_rejects_traversal,
    ),
    (
        "test_reopened_register_rejects_mutated_immutable_payloads_and_identities",
        test_reopened_register_rejects_mutated_immutable_payloads_and_identities,
    ),
    (
        "test_initialization_replay_compares_all_immutable_inputs",
        test_initialization_replay_compares_all_immutable_inputs,
    ),
    (
        "test_same_run_cannot_reset_budgets_with_a_replacement_plan",
        test_same_run_cannot_reset_budgets_with_a_replacement_plan,
    ),
    (
        "test_malformed_nested_register_returns_structured_invalid_error",
        test_malformed_nested_register_returns_structured_invalid_error,
    ),
    (
        "test_reservation_replay_precedes_checkpoint_and_authority_revalidation",
        test_reservation_replay_precedes_checkpoint_and_authority_revalidation,
    ),
    (
        "test_state_root_and_register_location_are_bound_to_context",
        test_state_root_and_register_location_are_bound_to_context,
    ),
    (
        "test_mutations_reject_external_register_paths_before_touching_locks",
        test_mutations_reject_external_register_paths_before_touching_locks,
    ),
    (
        "test_missing_observed_host_restriction_blocks_without_reservation",
        test_missing_observed_host_restriction_blocks_without_reservation,
    ),
    (
        "test_test_mode_reserves_and_labels_unenforced_isolation_without_faking_observation",
        test_test_mode_reserves_and_labels_unenforced_isolation_without_faking_observation,
    ),
    (
        "test_test_mode_still_blocks_unverified_model_and_fresh_assignment_support",
        test_test_mode_still_blocks_unverified_model_and_fresh_assignment_support,
    ),
    (
        "test_test_mode_does_not_relax_scope_commands_or_shared_budgets",
        test_test_mode_does_not_relax_scope_commands_or_shared_budgets,
    ),
    (
        "test_test_mode_requires_explicit_versioned_acknowledgment",
        test_test_mode_requires_explicit_versioned_acknowledgment,
    ),
    (
        "test_test_mode_requires_verified_fresh_model_selection",
        test_test_mode_requires_verified_fresh_model_selection,
    ),
    (
        "test_test_mode_rejects_wrong_repository_before_register_initialization",
        test_test_mode_rejects_wrong_repository_before_register_initialization,
    ),
    (
        "test_test_mode_cannot_change_after_approval",
        test_test_mode_cannot_change_after_approval,
    ),
    (
        "test_test_mode_rechecks_disposable_repository_binding",
        test_test_mode_rechecks_disposable_repository_binding,
    ),
    (
        "test_approved_host_supported_assignment_override_is_preserved_exactly",
        test_approved_host_supported_assignment_override_is_preserved_exactly,
    ),
    (
        "test_helper_allowances_accept_selected_models_from_each_harness",
        test_helper_allowances_accept_selected_models_from_each_harness,
    ),
    (
        "test_terra_helper_preflight_retains_capability_and_assignment_checks",
        test_terra_helper_preflight_retains_capability_and_assignment_checks,
    ),
    (
        "test_revision_nine_helper_allowance_cannot_acquire_terra_authority",
        test_revision_nine_helper_allowance_cannot_acquire_terra_authority,
    ),
    (
        "test_muse_graph_option_does_not_create_helper_runtime_authority",
        test_muse_graph_option_does_not_create_helper_runtime_authority,
    ),
    (
        "test_observed_assignment_mismatch_blocks_without_substitution",
        test_observed_assignment_mismatch_blocks_without_substitution,
    ),
    (
        "test_assignment_scope_command_and_mandatory_resource_keys_fail_closed",
        test_assignment_scope_command_and_mandatory_resource_keys_fail_closed,
    ),
    (
        "test_allowance_requires_declared_resources_and_assignment_keys",
        test_allowance_requires_declared_resources_and_assignment_keys,
    ),
    (
        "test_allowance_may_not_exceed_approved_parent_capabilities",
        test_allowance_may_not_exceed_approved_parent_capabilities,
    ),
    (
        "test_settlement_releases_only_active_resources_and_never_refunds_budgets",
        test_settlement_releases_only_active_resources_and_never_refunds_budgets,
    ),
    (
        "test_cross_process_lock_allows_only_one_atomic_active_reservation",
        test_cross_process_lock_allows_only_one_atomic_active_reservation,
    ),
    (
        "test_interrupted_atomic_replace_preserves_prior_register",
        test_interrupted_atomic_replace_preserves_prior_register,
    ),
    (
        "test_ledger_operates_through_public_persistence_port",
        test_ledger_operates_through_public_persistence_port,
    ),
];
