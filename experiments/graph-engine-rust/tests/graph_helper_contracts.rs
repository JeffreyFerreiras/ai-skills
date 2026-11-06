#[allow(dead_code)]
#[path = "support/hardening_case.rs"]
mod hardening_case;

use graph_engine::{
    application::{
        error::EngineError,
        persistent_store::StateStore,
        policy::{installed_codex_home, PolicyFilePort},
    },
    compat::{self, Value},
    domain::{
        errors::ValidationFailure,
        execution::{build_execution_plan, NODE_ROLES},
        hosts::{recommended_assignment, DEFAULT_HOST},
        ids, policy,
        task_contract::validate_task_brief,
    },
    infrastructure::policy_files::NativePolicyFiles,
};
use hardening_case::HardeningCase;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

const PROFILE_NAMES: [&str; 9] = [
    "impact_mapper",
    "tech_lead",
    "software_architect",
    "senior_engineer",
    "code_reviewer",
    "test_engineer",
    "security_reviewer",
    "evidence_scout",
    "validation_executor",
];
const HELPERS: [&str; 2] = ["evidence_scout", "validation_executor"];

fn source_skill() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/software-engineering-graph")
}

fn allowance(target: &str) -> Value {
    compat::loads(format!(r#"{{
        "schema_version":1,"allowance_id":"helpers-1","run_id":"RUN-1",
        "assignments":[{{
            "assignment_id":"tech-evidence","parent_role":"tech_lead","helper_role":"evidence_scout",
            "contract_revision":1,"model":"gpt-6.1-sol","reasoning_effort":"low",
            "parent_capabilities":[{{"effect":"filesystem_read","action":"read","target_ref":"{target}"}}],
            "scope_refs":["repo:docs/"],"commands":[],"checkpoint_policy":"observed_repository_state",
            "resource_keys":["worktree"],"required_host_capabilities":[
                "fresh_model_effort_selection","filesystem_confinement","tool_confinement"
            ],
            "limits":{{"children":1,"concurrency":1,"commands":0,"time_seconds":60,"output_tokens":1000,"file_reads":3}}
        }}],
        "shared_limits":{{"children":1,"concurrency":1,"commands":0,"time_seconds":60,"output_tokens":1000,"file_reads":3}},
        "resources":[{{"key":"worktree","capacity":1}}]
    }}"#).as_bytes()).unwrap()
}

fn helper_task(case: &HardeningCase, bytes: &[u8]) -> Value {
    fs::write(
        case.fixture
            .graph
            .repository
            .join("docs/helper-allowance.json"),
        bytes,
    )
    .unwrap();
    let mut task = case.fixture.graph.task("delivery", "full_delivery", &[]);
    task.set("schema_version", 3_i64.into());
    task.set("risk_level", "low".into());
    task.set(
        "model_sizing",
        Value::object([
            ("scope_extent", "bounded".into()),
            ("uncertainty", "low".into()),
        ]),
    );
    task.set(
        "helper_allowance",
        Value::object([
            ("ref", "repo:docs/helper-allowance.json".into()),
            ("sha256", ids::sha256_bytes(bytes).into()),
        ]),
    );
    task
}

#[test]
fn test_helper_enabled_task_initializes_and_persists_plan_v3() {
    let case = HardeningCase::golden();
    let bytes = ids::canonical_bytes(&allowance("repo:docs/")).unwrap();
    let task = helper_task(&case, &bytes);
    let initialized = case.fixture.initialize(&task, true).unwrap();
    assert!(initialized["execution_plan"]["schema_version"].py_eq(&3_i64.into()));
    assert!(initialized["execution_plan"]["helper_allowance"].py_eq(&task["helper_allowance"]));
    assert!(case.status().unwrap()["execution_plan"]["schema_version"].py_eq(&3_i64.into()));
}

#[test]
fn test_helper_allowance_cannot_exceed_effective_parent_authority() {
    let case = HardeningCase::golden();
    let bytes = ids::canonical_bytes(&allowance("repo:src/")).unwrap();
    let task = helper_task(&case, &bytes);
    let error = case.fixture.initialize(&task, true).unwrap_err();
    assert!(
        matches!(&error, EngineError::Contract(error) if error.code == "PARENT_AUTHORITY_EXCEEDED"),
        "{error:?}"
    );
}

#[test]
fn test_v3_task_requires_and_binds_one_helper_allowance() {
    let case = HardeningCase::stock();
    let task = helper_task(&case, b"{}");
    case.fixture
        .with_context(|context| {
            let digest = context.current_policy_digest.into();
            let validate = |task: &Value| {
                validate_task_brief(task, &digest, context.policy, None, context.files.windows())
            };
            let validated = validate(&task).unwrap();
            assert!(validated["helper_allowance"].py_eq(&task["helper_allowance"]));
            let plan = build_execution_plan("RUN-1", &validated, None, DEFAULT_HOST).unwrap();
            assert!(plan["schema_version"].py_eq(&3_i64.into()));
            assert!(plan["helper_allowance"].py_eq(&task["helper_allowance"]));
            let without_allowance = Value::Object(
                task.as_object()
                    .unwrap()
                    .iter()
                    .filter(|(key, _)| !key.equals("helper_allowance"))
                    .cloned()
                    .collect(),
            );
            let change = |key: &str, value: &str| {
                let mut candidate = task.clone();
                let mut allowance = task["helper_allowance"].clone();
                allowance.set(key, value.into());
                candidate.set("helper_allowance", allowance);
                candidate
            };
            for (candidate, expected) in [
                (without_allowance, "MISSING_FIELD"),
                (change("extra", "no"), "UNKNOWN_FIELD"),
                (change("ref", "authority:test"), "REPOSITORY_REF_REQUIRED"),
            ] {
                let error = validate(&candidate).unwrap_err();
                assert!(
                    matches!(&error, ValidationFailure::Contract(error) if error.code == expected),
                    "{error:?}"
                );
            }
            Ok(())
        })
        .unwrap();
}

fn profile_text(name: &str) -> String {
    fs::read_to_string(
        source_skill()
            .join("profile-agents")
            .join(format!("{name}.toml")),
    )
    .unwrap()
}

#[test]
fn test_supported_profiles_and_parent_contract_wiring() {
    let root = source_skill();
    let standalone = fs::read_to_string(root.join("tests/test_standalone_acceptance.py")).unwrap();
    let entries = standalone
        .split_once("PROFILE_AGENTS = {")
        .unwrap()
        .1
        .split_once('}')
        .unwrap()
        .0;
    let standalone_names = entries
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            line.strip_prefix('"')
                .unwrap()
                .strip_suffix("\",")
                .unwrap()
                .to_owned()
        })
        .collect::<BTreeSet<_>>();
    let expected_names = PROFILE_NAMES.map(|name| format!("{name}.toml"));
    assert_eq!(standalone_names, BTreeSet::from(expected_names));
    let files = fs::read_dir(root.join("profile-agents"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(standalone_names, files);
    let scout = || BTreeSet::from(["evidence_scout"]);
    let both = || BTreeSet::from(HELPERS);
    let expected = BTreeMap::from([
        ("tech_lead", scout()),
        ("software_architect", scout()),
        ("senior_engineer", both()),
        ("code_reviewer", scout()),
        ("test_engineer", both()),
        ("security_reviewer", scout()),
    ]);
    let contract = fs::read_to_string(root.join("references/economy-helpers.md")).unwrap();
    let matrix = contract
        .lines()
        .filter_map(|line| {
            let parts = line
                .strip_prefix("| ")?
                .strip_suffix(" |")?
                .split(" | ")
                .collect::<Vec<_>>();
            let [parent, scout, executor] = parts.as_slice() else {
                return None;
            };
            if !parent.chars().all(|c| c.is_alphanumeric() || c == '_')
                || ![scout, executor]
                    .into_iter()
                    .all(|flag| ["yes", "no"].contains(flag))
            {
                return None;
            }
            let allowed = HELPERS
                .into_iter()
                .zip([scout, executor])
                .filter_map(|(helper, flag)| (*flag == "yes").then_some(helper))
                .collect();
            Some((*parent, allowed))
        })
        .collect::<BTreeMap<_, BTreeSet<_>>>();
    assert_eq!(matrix, expected);
    for name in PROFILE_NAMES {
        let text = profile_text(name);
        let allowed = text
            .lines()
            .find_map(|line| {
                let types = line
                    .strip_prefix("Approved helper types: ")?
                    .strip_suffix('.')?;
                types
                    .chars()
                    .all(|c| c.is_alphanumeric() || ['_', ',', ' '].contains(&c))
                    .then(|| types.split(", ").collect::<BTreeSet<_>>())
            })
            .unwrap_or_default();
        assert_eq!(
            allowed,
            expected.get(name).cloned().unwrap_or_default(),
            "{name}"
        );
        if !allowed.is_empty() {
            assert!(text.contains("references/economy-helpers.md"));
        }
    }
    let capabilities = policy::default_profile_policy("fixture", &[])["role_capabilities"].clone();
    for helper in HELPERS {
        assert!(NODE_ROLES
            .iter()
            .all(|(node, role)| *node != helper && *role != helper));
        assert!(!policy::ROLES.contains(&helper));
        assert!(capabilities.get(helper).is_none());
    }
}

#[test]
fn test_helper_contract_sources_and_host_defaults() {
    let assigned = recommended_assignment("codex-astra", "helper", None).unwrap();
    for (name, sandbox) in [
        ("evidence_scout", "read-only"),
        ("validation_executor", "workspace-write"),
    ] {
        let text = profile_text(name);
        let field = |key: &str| {
            text.lines()
                .find_map(|line| {
                    line.strip_prefix(key)?
                        .strip_prefix(" = \"")?
                        .strip_suffix('"')
                        .map(str::to_owned)
                })
                .unwrap()
        };
        assert_eq!((field("model"), field("model_reasoning_effort")), assigned);
        assert!(text.contains(&format!("sandbox_mode = \"{sandbox}\"")));
        assert_eq!(text.matches("\"\"\"").count(), 2);
        assert!(text.contains("references/economy-helpers.md"));
    }
    let scenarios =
        fs::read_to_string(source_skill().join("references/behavioral-evaluations.md")).unwrap();
    let found = scenarios
        .lines()
        .filter_map(|line| {
            let (id, _) = line.strip_prefix("### H")?.split_once(':')?;
            (!id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())).then(|| id.to_owned())
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(found, (1..=15).map(|index| format!("{index:02}")).collect());
}

#[test]
fn test_source_checkout_uses_sibling_local_default_and_keeps_strict_legacy_helper() {
    let case = HardeningCase::stock();
    let store = &case.fixture.graph.store;
    let portable_home = case.fixture.graph.root.join("portable-home");
    let files = NativePolicyFiles {
        environment_override: Some(BTreeMap::from([(
            "HOME".to_owned(),
            portable_home.to_str().unwrap().into(),
        )])),
    };
    let skill = files.resolve(&source_skill(), true).unwrap();
    let source = skill.join("graph_engine/state.py");
    assert!(matches!(installed_codex_home(&files, &source),
        Err(EngineError::State(error)) if error.code == "CODEX_PROFILE_ROOT_NOT_FOUND"));
    let default = StateStore::new(
        None,
        None,
        &skill,
        &files,
        store.platform,
        store.database,
        store.fault,
    )
    .unwrap();
    let checkout = skill.parent().unwrap().parent().unwrap();
    let expected = checkout
        .with_file_name(format!(
            "{}.local",
            checkout.file_name().unwrap().to_str().unwrap()
        ))
        .join("software-engineering-graph");
    assert_eq!(default.state_root, expected);
    let explicit_root = case.fixture.graph.root.join("explicit-codex-home");
    let explicit = StateStore::new(
        Some(&explicit_root),
        None,
        &skill,
        &files,
        store.platform,
        store.database,
        store.fault,
    )
    .unwrap();
    assert_eq!(explicit.state_root, files.absolute(&explicit_root).unwrap());
}
