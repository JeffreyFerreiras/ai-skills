#[path = "support/daily_review.rs"]
mod daily_review;
#[allow(dead_code)]
#[path = "support/hardening_case.rs"]
mod hardening_case;
#[allow(dead_code)]
#[path = "support/recovery_case.rs"]
mod recovery_case;

use graph_engine::{
    application::error::EngineError,
    compat::{self, SerializationProfile, Value},
    domain::{
        errors::ValidationFailure, ids, merge_readiness::assess_merge_readiness,
        task_contract::validate_task_brief,
    },
};
use hardening_case::{write_json, HardeningCase};
use recovery_case::state_error;
use std::{fs, path::PathBuf};

const SOURCES: [&str; 2] = ["src/slots.py", "src/test_slots.py"];
const AUTHORITY: &str = "authority:synthetic-fixture";
const FIXED_SOURCE: &str = concat!(
    "from datetime import date, timedelta\n",
    "def slot_date(service_date, opening, slot):\n",
    "    return date.fromisoformat(service_date) + timedelta(days=slot < opening)\n",
);
const FIXED_TESTS: &str = concat!(
    "import unittest\nfrom slots import slot_date\n",
    "class SlotsTests(unittest.TestCase):\n",
    "    def test_late_slot_moves_to_next_day(self):\n",
    "        self.assertEqual('2026-05-09', str(slot_date('2026-05-08', '19:00', '00:30')))\n",
    "    def test_evening_slot_stays_on_service_day(self):\n",
    "        self.assertEqual('2026-05-08', str(slot_date('2026-05-08', '19:00', '20:00')))\n",
);

fn capability(effect: &str, action: &str, target: &str) -> Value {
    Value::object([
        ("effect", effect.into()),
        ("action", action.into()),
        ("target_ref", target.into()),
    ])
}

fn writes(branch: &Value) -> Vec<Value> {
    branch["effect_capabilities"]
        .as_list()
        .unwrap()
        .iter()
        .filter(|item| item["effect"].is_text("filesystem_write"))
        .cloned()
        .collect()
}

fn coverage(paths: &[&str]) -> Value {
    Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "check_coverage".into()),
        (
            "checks",
            Value::list([Value::object([
                ("check_id", "repo-check".into()),
                (
                    "relevant_inputs",
                    Value::list(paths.iter().copied().map(Value::from)),
                ),
                ("complete", true.into()),
            ])]),
        ),
    ])
}

#[test]
fn test_scope_text_does_not_grant_two_file_or_broad_write_authority() {
    let case = HardeningCase::stock();
    case.fixture
        .with_context(|context| {
            let mut policy = context.policy.clone();
            let roles = policy["role_capabilities"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(role, items)| {
                    let capabilities = items
                        .as_list()
                        .unwrap()
                        .iter()
                        .filter(|item| {
                            !item["effect"].is_text("filesystem_write")
                                || item["target_ref"].is_text("repo:src/")
                        })
                        .cloned();
                    (role.clone(), Value::list(capabilities))
                })
                .collect();
            policy.set("role_capabilities", Value::Object(roles));
            let mut task = case.fixture.graph.task("delivery", "full_delivery", &[]);
            let mut scope = task["scope"].clone();
            scope.set(
                "included",
                Value::list([
                    "repo:src/feature.py".into(),
                    "repo:scripts/rollout.py".into(),
                ]),
            );
            task.set("scope", scope);
            let digest = context.current_policy_digest.into();
            let validate = |task: &Value| {
                validate_task_brief(task, &digest, &policy, None, context.files.windows())
            };
            let approved = validate(&task).unwrap();
            assert!(approved["authority"].py_eq(&task["authority"]));
            for target in ["repo:scripts/rollout.py", "repo:scripts/", "repo:"] {
                let mut successor = task.clone();
                let mut authority = task["authority"].clone();
                let mut grants = authority["capabilities"].as_list().unwrap().to_vec();
                grants.push(capability("filesystem_write", "edit", target));
                authority.set("capabilities", Value::List(grants));
                successor.set("authority", authority);
                let error = validate(&successor).unwrap_err();
                assert!(
                    matches!(&error, ValidationFailure::Contract(error)
                if ["POLICY_AUTHORITY_EXCEEDED", "INVALID_STRING"].contains(&error.code.as_str())),
                    "{error:?}"
                );
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn test_narrow_retry_cannot_restore_broad_policy_directory() {
    let mut case = HardeningCase::stock();
    let mut task = case.fixture.graph.task("delivery", "fast_path", &[]);
    let narrow = capability("filesystem_write", "edit", "repo:src/slots.py");
    let mut authority = task["authority"].clone();
    let mut grants = authority["capabilities"].as_list().unwrap().to_vec();
    grants.push(narrow.clone());
    authority.set("capabilities", Value::List(grants));
    task.set("authority", authority);
    case.fixture.initialize(&task, true).unwrap();
    let path = case.inbox(&coverage(&["docs/engineering-graph.md"]));
    case.run(&[
        "evidence",
        "enable",
        "--run-id",
        "RUN-1",
        "--contract-version",
        "2",
        "--coverage-manifest",
        path.to_str().unwrap(),
        "--op-id",
        "enable",
    ])
    .unwrap();
    case.impact("fast_path");
    let writer = case.claim();
    let evidence = case.repo_artifact("failure", &format!("artifact-{}", case.counter() + 1));
    case.record_as(
        &writer,
        Value::object([
            ("schema_version", 1_i64.into()),
            ("run_id", "RUN-1".into()),
            ("branch_id", writer["branch_id"].clone()),
            ("status", "failed".into()),
            ("output_kind", "implementation_handoff".into()),
            ("failure_code", "TOOL_FAILURE".into()),
            ("evidence", Value::list([evidence])),
        ]),
        "result",
    )
    .unwrap();
    let mut judgment = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "repair_judgment".into()),
        ("cause", "implementation".into()),
        ("failed_criterion_ids", Value::list(["AC-001".into()])),
        (
            "authorized_scope",
            Value::list([capability("filesystem_write", "edit", "repo:src/")]),
        ),
    ]);
    let id = writer["branch_id"].text().unwrap();
    let retry = |case: &mut HardeningCase, judgment: &Value, op: &str| {
        let path = case.inbox(judgment);
        case.run(&[
            "record",
            "retry",
            "--run-id",
            "RUN-1",
            "--branch-id",
            &id,
            "--reason-code",
            "RETRY",
            "--repair-manifest",
            path.to_str().unwrap(),
            "--op-id",
            op,
        ])
    };
    let error = retry(&mut case, &judgment, "broad-retry").unwrap_err();
    assert!(
        matches!(&error, EngineError::Contract(error) if error.code == "AUTHORITY_EXCEEDED"),
        "{error:?}"
    );
    judgment.set("authorized_scope", Value::list([narrow.clone()]));
    retry(&mut case, &judgment, "narrow-retry").unwrap();
    let retried = case.claim();
    assert!(Value::List(writes(&retried)).py_eq(&Value::list([narrow])));
    assert!(retried["retry_count"].py_eq(&1_i64.into()));
}

fn write_source(case: &HardeningCase, relative: &str, content: &str) {
    let text = if cfg!(windows) {
        content.replace('\n', "\r\n")
    } else {
        content.to_owned()
    };
    fs::write(case.fixture.graph.repository.join(relative), text).unwrap();
}

fn source_hashes(case: &HardeningCase) -> Value {
    Value::object(SOURCES.map(|path| {
        let bytes = fs::read(case.fixture.graph.repository.join(path)).unwrap();
        (path, ids::sha256_bytes(&bytes).into())
    }))
}

fn unhashed_ref(artifact: &Value) -> String {
    artifact["ref"]
        .text()
        .unwrap()
        .split('#')
        .next()
        .unwrap()
        .to_owned()
}

fn artifact_path(case: &HardeningCase, artifact: &Value) -> PathBuf {
    case.fixture
        .graph
        .repository
        .join(unhashed_ref(artifact).strip_prefix("repo:").unwrap())
}

fn call(case: &HardeningCase, run_id: &str, args: &[&str]) -> Value {
    let mut argv = args.to_vec();
    argv.extend(["--run-id", run_id]);
    case.run(&argv).unwrap()
}

struct SuccessorTrace<'a> {
    case: &'a HardeningCase,
    counter: &'a mut usize,
    run_id: &'a str,
    task: &'a Value,
    reject_design: bool,
    writer: Option<Value>,
    check_ref: Option<String>,
    rejection: Option<Value>,
    seen: Vec<(String, String)>,
}

impl<'a> SuccessorTrace<'a> {
    fn new(
        case: &'a HardeningCase,
        counter: &'a mut usize,
        run_id: &'a str,
        task: &'a Value,
        reject_design: bool,
    ) -> Self {
        Self {
            case,
            counter,
            run_id,
            task,
            reject_design,
            writer: None,
            check_ref: None,
            rejection: None,
            seen: Vec::new(),
        }
    }

    fn call(&self, args: &[&str]) -> Value {
        call(self.case, self.run_id, args)
    }

    fn inbox(&mut self, manifest: &Value) -> PathBuf {
        *self.counter += 1;
        let graph = &self.case.fixture.graph;
        let path = graph
            .store
            .inbox_root("albanian-live-translate", self.run_id)
            .join(format!("trace-{}.json", self.counter));
        write_json(&path, manifest);
        graph.store.platform.set_mode(&path, 0o600).unwrap();
        path
    }

    fn artifact(&mut self, kind: &str) -> Value {
        *self.counter += 1;
        let name = format!("artifact-{}", self.counter);
        let (reference, digest) = self.case.evidence(
            &name,
            &Value::object([
                ("schema_version", 1_i64.into()),
                ("kind", kind.into()),
                ("label", format!("{name}.json").into()),
            ]),
        );
        Value::object([
            ("kind", kind.into()),
            ("ref", reference.into()),
            ("sha256", digest.into()),
        ])
    }

    fn initialize(&mut self) {
        let path = self
            .case
            .fixture
            .graph
            .repository
            .join(format!("docs/task-{}.json", self.run_id));
        write_json(&path, self.task);
        let initialized = self
            .case
            .run(&[
                "--ack-degraded-permissions",
                "--ack-degraded-durability",
                "init",
                "--run-id",
                self.run_id,
                "--task-brief",
                path.to_str().unwrap(),
                "--op-id",
                "synthetic-init",
            ])
            .unwrap();
        self.call(&[
            "record",
            "plan-approval",
            "--plan-digest",
            &initialized["execution_plan_digest"].text().unwrap(),
            "--decision",
            "APPROVE",
            "--authority-ref",
            AUTHORITY,
            "--op-id",
            "synthetic-plan-approval",
        ]);
        let path = self.inbox(&coverage(&SOURCES));
        self.call(&[
            "evidence",
            "enable",
            "--contract-version",
            "2",
            "--coverage-manifest",
            path.to_str().unwrap(),
            "--op-id",
            "enable-evidence",
        ]);
    }

    fn check(&mut self, executor: &Value) {
        let writer = self.writer.as_ref().unwrap();
        let text = |value: &Value, key: &str| value[key].text().unwrap();
        let executor_id = text(executor, "branch_id");
        let attempt = text(executor, "attempt_id");
        let token = text(executor, "claim_token");
        let source = text(writer, "branch_id");
        let source_attempt = text(writer, "attempt_id");
        let source_digest = text(writer, "claim_digest");
        let op = format!("check-{}", text(executor, "node_key"));
        let mut args = vec![
            "check",
            "run",
            "--check-id",
            "repo-check",
            "--executor-branch-id",
            &executor_id,
            "--executor-attempt-id",
            &attempt,
            "--executor-claim-token",
            &token,
            "--source-branch-id",
            &source,
            "--source-attempt-id",
            &source_attempt,
            "--source-claim-digest",
            &source_digest,
            "--op-id",
            &op,
        ];
        if let Some(previous) = &self.check_ref {
            args.extend(["--replace-ref", previous]);
        }
        let receipt = self.call(&args);
        assert!(receipt["outcome"].is_text("PASS"));
        self.check_ref = receipt["artifact_ref"].text();
    }

    fn record(&mut self, branch: &Value, stage: &str) {
        let key = branch["node_key"].text().unwrap();
        self.seen.push((key.clone(), stage.to_owned()));
        let mut granted = writes(branch)
            .iter()
            .map(|item| item["target_ref"].text().unwrap())
            .collect::<Vec<_>>();
        granted.sort();
        let expected = if branch["role"].is_text("senior_engineer") {
            SOURCES.iter().map(|path| format!("repo:{path}")).collect()
        } else {
            Vec::new()
        };
        assert_eq!(granted, expected);
        let kind = branch["output_contract"]["artifact_kind"].text().unwrap();
        let mut artifact = self.artifact(&kind);
        let mut manifest = if kind == "impact_map" {
            Value::object([
                ("schema_version", 1_i64.into()),
                ("task_id", self.task["task_id"].clone()),
                ("route_label", "full_delivery".into()),
                ("impact_tags", Value::list(["security_privacy".into()])),
                ("evidence_refs", Value::list([artifact["ref"].clone()])),
            ])
        } else if ["design_consolidation", "delivery_consolidation"].contains(&kind.as_str()) {
            let collection = &branch["inputs"]
                .as_list()
                .unwrap()
                .iter()
                .find(|item| item["kind"].is_text("collection"))
                .unwrap()["content"];
            let blocked = self.reject_design && kind == "design_consolidation";
            let mut sources = collection["members"]
                .as_list()
                .unwrap()
                .iter()
                .map(|item| item["branch_id"].text().unwrap())
                .collect::<Vec<_>>();
            sources.sort();
            Value::object([
                ("schema_version", 1_i64.into()),
                ("kind", kind.as_str().into()),
                ("run_id", self.run_id.into()),
                ("join_id", collection["join_id"].clone()),
                ("generation", branch["generation"].clone()),
                (
                    "source_branch_ids",
                    Value::list(sources.into_iter().map(Value::from)),
                ),
                (
                    "finding_dispositions",
                    if blocked {
                        blocked_finding()
                    } else {
                        Value::List(vec![])
                    },
                ),
                (
                    "outcome",
                    if blocked {
                        "BLOCK"
                    } else if kind == "design_consolidation" {
                        "APPROVE"
                    } else {
                        "ACCEPT"
                    }
                    .into(),
                ),
            ])
        } else {
            let blocked = self.reject_design && key == "architect";
            if blocked {
                let path = artifact_path(self.case, &artifact);
                let report = Value::object([
                    ("kind", kind.as_str().into()),
                    ("finding_id", "ARCH-001".into()),
                    ("fixture", "synthetic rejected design".into()),
                    (
                        "rationale",
                        "A 00:30 slot after a 19:00 opening must use the next calendar day.".into(),
                    ),
                    (
                        "approved_source_files",
                        Value::list(SOURCES.map(Value::from)),
                    ),
                    ("source_sha256", source_hashes(self.case)),
                ]);
                let digest = ids::sha256_bytes(&write_json(&path, &report));
                artifact.set(
                    "ref",
                    format!("{}#sha256={digest}", unhashed_ref(&artifact)).into(),
                );
                artifact.set("sha256", digest.into());
                self.rejection = Some(artifact.clone());
            }
            let mut manifest = Value::object([
                ("schema_version", 1_i64.into()),
                ("run_id", self.run_id.into()),
                ("branch_id", branch["branch_id"].clone()),
                ("status", "succeeded".into()),
                ("output_kind", kind.into()),
                ("artifact_ref", artifact),
                (
                    "evidence",
                    if stage == "research" {
                        Value::list([self.artifact("finding")])
                    } else {
                        Value::List(vec![])
                    },
                ),
                (
                    "findings",
                    if blocked {
                        blocked_finding()
                    } else {
                        Value::List(vec![])
                    },
                ),
            ]);
            if branch["output_contract"]["decision_values"].truthy() {
                let decision = if blocked {
                    "BLOCK"
                } else if branch["role"].is_text("senior_engineer") {
                    "IMPLEMENTED"
                } else {
                    "APPROVE"
                };
                manifest.set("decision", decision.into());
            }
            manifest
        };
        manifest.set("attempt_id", branch["attempt_id"].clone());
        manifest.set("claim_digest", branch["claim_digest"].clone());
        let path = self.inbox(&manifest);
        let id = branch["branch_id"].text().unwrap();
        self.call(&[
            "record",
            "branch-result",
            "--branch-id",
            &id,
            "--attempt-id",
            &branch["attempt_id"].text().unwrap(),
            "--claim-token",
            &branch["claim_token"].text().unwrap(),
            "--result-manifest",
            path.to_str().unwrap(),
            "--op-id",
            &format!("result-{id}"),
        ]);
    }

    fn assess(&mut self, status: &Value, id: &Value, step: usize) {
        let fanout = status["fanouts"]
            .as_list()
            .unwrap()
            .iter()
            .find(|item| item["fanout_id"].py_eq(id))
            .unwrap();
        let resources = Value::object([
            ("writable_paths", Value::List(vec![])),
            ("mutable_state_refs", Value::List(vec![])),
            ("exclusive_device_refs", Value::List(vec![])),
            ("services", Value::List(vec![])),
        ]);
        let members = fanout["member_branch_ids"]
            .as_list()
            .unwrap()
            .iter()
            .map(|branch| {
                Value::object([
                    ("branch_id", branch.clone()),
                    ("resources", resources.clone()),
                ])
            });
        let evidence = self.artifact("finding");
        let manifest = Value::object([
            ("schema_version", 1_i64.into()),
            ("kind", "fanout_assessment".into()),
            ("run_id", self.run_id.into()),
            ("fanout_id", id.clone()),
            ("members", Value::list(members)),
            ("dependencies", Value::List(vec![])),
            ("evidence", Value::list([evidence])),
        ]);
        let path = self.inbox(&manifest);
        self.call(&[
            "record",
            "fanout-assessment",
            "--fanout-id",
            &id.text().unwrap(),
            "--assessment-manifest",
            path.to_str().unwrap(),
            "--authority-ref",
            AUTHORITY,
            "--op-id",
            &format!("assessment-{step}"),
        ]);
    }

    fn drive(&mut self) -> Value {
        for step in 0..80 {
            let status = self.call(&["status"]);
            if status["status"].is_text("blocked") {
                return status;
            }
            let action = &status["next_action"];
            match action["kind"].text().unwrap().as_str() {
                "record_fanout_assessment" => self.assess(&status, &action["fanout_id"], step),
                "claim" => {
                    let branch =
                        self.call(&["next", "--claim", "--op-id", &format!("claim-{step}")])
                            ["branch"]
                            .clone();
                    if branch["role"].is_text("senior_engineer") {
                        self.writer = Some(branch.clone());
                        write_source(self.case, SOURCES[0], FIXED_SOURCE);
                        write_source(self.case, SOURCES[1], FIXED_TESTS);
                        daily_review::git(self.case, &["add", "--", SOURCES[0], SOURCES[1]]);
                        daily_review::git(
                            self.case,
                            &[
                                "-c",
                                "user.name=Fixture",
                                "-c",
                                "user.email=fixture@example.invalid",
                                "commit",
                                "-m",
                                "synthetic approved successor",
                            ],
                        );
                        self.check(&branch);
                    } else if branch["node_key"].is_text("test_engineer") {
                        self.check(&branch);
                    }
                    let stage = status["branches"]
                        .as_list()
                        .unwrap()
                        .iter()
                        .find(|item| item["branch_id"].py_eq(&branch["branch_id"]))
                        .unwrap()["stage"]
                        .text()
                        .unwrap();
                    self.record(&branch, &stage);
                }
                "advance_join" => {
                    let id = &action["join_id"];
                    let join = status["joins"]
                        .as_list()
                        .unwrap()
                        .iter()
                        .find(|item| item["join_id"].py_eq(id))
                        .unwrap();
                    if join["join_key"].is_text("closure") {
                        let acceptance = self.artifact("acceptance_evidence");
                        self.call(&[
                            "record",
                            "acceptance-evidence",
                            "--criterion-id",
                            "AC-001",
                            "--artifact-ref",
                            &acceptance["ref"].text().unwrap(),
                            "--artifact-sha256",
                            &acceptance["sha256"].text().unwrap(),
                            "--op-id",
                            "acceptance",
                        ]);
                        self.call(&["complete", "--op-id", "complete"]);
                        return self.call(&["status"]);
                    }
                    self.call(&[
                        "join",
                        "advance",
                        "--join-id",
                        &id.text().unwrap(),
                        "--op-id",
                        &format!("join-{step}"),
                    ]);
                }
                _ => panic!("unexpected trace action: {action:?}"),
            }
        }
        panic!("successor trace exceeded its bounded step budget")
    }
}

fn blocked_finding() -> Value {
    Value::list([Value::object([
        ("finding_id", "ARCH-001".into()),
        ("disposition", "block".into()),
    ])])
}

fn successor_task(case: &HardeningCase) -> Value {
    let mut task = case
        .fixture
        .graph
        .task("delivery", "full_delivery", &["security_privacy"]);
    task.set(
        "objective",
        "Synthetic fixture: correct overnight slot calendar dates".into(),
    );
    task.set(
        "scope",
        Value::object([
            (
                "included",
                Value::list(SOURCES.map(|path| Value::from(format!("repo:{path}")))),
            ),
            ("excluded", Value::List(vec![])),
        ]),
    );
    let mut approval = task["policy_approval"].clone();
    approval.set("authority_ref", AUTHORITY.into());
    task.set("policy_approval", approval);
    let grants = [("filesystem_read", "read"), ("filesystem_write", "edit")]
        .into_iter()
        .flat_map(|(effect, action)| {
            SOURCES.map(|path| capability(effect, action, &format!("repo:{path}")))
        })
        .chain([capability("command", "run", "npm-run-check")]);
    task.set(
        "authority",
        Value::object([("capabilities", Value::list(grants))]),
    );
    task
}

#[test]
fn test_terminal_rejection_approved_narrow_successor_completes() {
    let mut case = HardeningCase::stock();
    let python = std::env::var("GRAPH_RUST_ORACLE_PYTHON").unwrap();
    case.edit_policy(|policy| {
        let mut checks = policy["required_checks"].clone();
        let mut check = checks["repo-check"].clone();
        check.set(
            "argv",
            Value::list([
                python.into(),
                "-m".into(),
                "unittest".into(),
                "discover".into(),
                "-s".into(),
                "src".into(),
                "-p".into(),
                "test_slots.py".into(),
            ]),
        );
        checks.set("repo-check", check);
        policy.set("required_checks", checks);
    });
    let graph = &case.fixture.graph;
    let policy_path = graph.repository.join(".codex/engineering-graph.json");
    let policy_digest = ids::sha256_bytes(&graph.policy_bytes);
    fs::create_dir(graph.repository.join("src")).unwrap();
    write_source(&case, SOURCES[0], "from datetime import date\ndef slot_date(service_date, opening, slot):\n    return date.fromisoformat(service_date)\n");
    write_source(&case, SOURCES[1], "import unittest\n");
    daily_review::git(&case, &["init"]);
    daily_review::git(&case, &["add", "--", SOURCES[0], SOURCES[1]]);
    daily_review::git(
        &case,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "seed",
        ],
    );
    let task = successor_task(&case);
    let mut counter = 0;
    let mut original = SuccessorTrace::new(&case, &mut counter, "RUN-1", &task, true);
    original.initialize();
    let rejected = original.drive();
    assert!(rejected["status"].is_text("blocked"));
    assert!(rejected["blocked_reason"].is_text("CONSOLIDATION_BLOCK"));
    assert!(!original
        .seen
        .iter()
        .any(|(node, stage)| node == "senior_engineer" && stage == "implementation"));
    let rejection = original.rejection.take().unwrap();
    drop(original);
    let rejection_path = artifact_path(&case, &rejection);
    let original_evidence = fs::read(&rejection_path).unwrap();
    state_error(
        case.run(&[
            "next",
            "--claim",
            "--op-id",
            "forbidden-reopen",
            "--run-id",
            "RUN-1",
        ]),
        "GRAPH_BLOCKED",
    );
    let mut task_next = task.clone();
    task_next.set("task_id", "TASK-SUCCESSOR".into());
    task_next.set(
        "objective",
        "Synthetic approved successor: late slots use next-day dates".into(),
    );
    let mut evidence = task["evidence_paths"].as_list().unwrap().to_vec();
    evidence.push(unhashed_ref(&rejection).into());
    task_next.set("evidence_paths", Value::List(evidence));
    let mut constraints = task["constraints"].as_list().unwrap().to_vec();
    constraints.push(
        "Preserve RUN-1 rejection; resolve ARCH-001 in this separately approved successor.".into(),
    );
    task_next.set("constraints", Value::List(constraints));
    let mut successor = SuccessorTrace::new(&case, &mut counter, "RUN-2", &task_next, false);
    successor.initialize();
    let completed = successor.drive();
    assert!(completed["status"].is_text("complete"));
    assert!(completed["state_schema_version"].py_eq(&7_i64.into()));
    assert!(completed["execution_plan"]["status"].is_text("approved"));
    assert!(completed["policy_digest"].is_text(&policy_digest));
    assert!(!completed["execution_plan"]["plan_digest"]
        .py_eq(&rejected["execution_plan"]["plan_digest"]));
    assert_eq!(graph.policy_bytes, fs::read(policy_path).unwrap());
    assert!(task["authority"].py_eq(&task_next["authority"]));
    let expected_gates = [
        ("architect", "design"),
        ("security_reviewer", "design"),
        ("code_reviewer", "delivery"),
        ("code_reviewer_architecture", "delivery"),
        ("code_reviewer_naming", "delivery"),
        ("code_reviewer_bug_hunter", "delivery"),
        ("test_engineer", "delivery"),
        ("security_reviewer", "delivery"),
    ];
    assert!(expected_gates
        .iter()
        .all(|(node, stage)| successor.seen.iter().any(|(n, s)| n == node && s == stage)));
    assert!(completed["branches"]
        .as_list()
        .unwrap()
        .iter()
        .all(|branch| branch["status"].is_text("succeeded")
            && branch["generation"].py_eq(&0_i64.into())));
    let acceptance = completed["acceptance_evidence"].as_list().unwrap();
    assert_eq!(acceptance.len(), 1);
    let checks = completed["check_evidence"].as_list().unwrap();
    assert!(checks.iter().all(|item| item["outcome"].is_text("PASS")));
    assert!(completed["evidence_lifecycle"]["checks"]
        .as_list()
        .unwrap()
        .iter()
        .all(|item| item["eligibility"].is_text("eligible")));
    let preserved = call(&case, "RUN-1", &["status"]);
    assert!(preserved["status"].is_text("blocked"));
    assert!(preserved["state_revision"].py_eq(&rejected["state_revision"]));
    assert_eq!(original_evidence, fs::read(rejection_path).unwrap());
    for budget in rejected["budgets"].as_list().unwrap() {
        if ["design_revisions", "delivery_repairs"]
            .iter()
            .any(|id| budget["budget_id"].is_text(id))
        {
            assert!(budget["used"].py_eq(&0_i64.into()));
            let selected = completed["budgets"]
                .as_list()
                .unwrap()
                .iter()
                .find(|item| item["budget_id"].py_eq(&budget["budget_id"]))
                .unwrap();
            assert!(budget.py_eq(selected));
        }
    }
    let (_, stored_task) = daily_review::verify(&case, "RUN-2").unwrap();
    let mut grants = task["authority"]["capabilities"]
        .as_list()
        .unwrap()
        .to_vec();
    grants.sort_by_key(|grant| {
        ["effect", "action", "target_ref"].map(|key| grant[key].text().unwrap())
    });
    assert!(Value::List(grants).py_eq(&stored_task["authority"]["capabilities"]));
    let head = daily_review::git(&case, &["rev-parse", "HEAD"]);
    let snapshot =
        daily_review::merge_snapshot(&completed, &head, &Value::list(["repo-check".into()]));
    let readiness = assess_merge_readiness(&snapshot);
    assert!(readiness["decision"].is_text("DRY_RUN_READY"));
    assert!(!readiness["merge_authorized"].truthy());
    assert!(!readiness["deploy_authorized"].truthy());
    let mut gates = expected_gates;
    gates.sort();
    let trace = Value::object([
        ("synthetic_approvals_and_reviews", true.into()),
        (
            "original",
            Value::object([
                ("status", rejected["status"].clone()),
                ("reason", rejected["blocked_reason"].clone()),
                ("preserved_revision", rejected["state_revision"].clone()),
                ("rejection_sha256", rejection["sha256"].clone()),
            ]),
        ),
        (
            "successor",
            Value::object([
                ("run_id", "RUN-2".into()),
                ("approval", completed["execution_plan"]["status"].clone()),
                (
                    "plan_digest",
                    completed["execution_plan"]["plan_digest"].clone(),
                ),
                ("policy_sha256", policy_digest.into()),
                ("file_grants", task["authority"]["capabilities"].clone()),
                (
                    "fresh_gates",
                    Value::list(
                        gates.map(|(node, stage)| Value::list([node.into(), stage.into()])),
                    ),
                ),
                ("generation", 0_i64.into()),
                ("status", completed["status"].clone()),
                (
                    "local_checks",
                    Value::list(checks.iter().map(|item| {
                        Value::object([
                            ("id", item["check_id"].clone()),
                            ("outcome", item["outcome"].clone()),
                        ])
                    })),
                ),
                ("acceptance_count", acceptance.len().into()),
            ]),
        ),
        ("source_sha256", source_hashes(&case)),
        ("merge_readiness", readiness),
    ]);
    println!(
        "DAILY_SUCCESSOR_TRACE {}",
        compat::dumps_text(&trace, SerializationProfile::CLI)
            .unwrap()
            .utf8()
            .unwrap()
    );
}
