use super::{assignment, build, overrides, pair, rebuild, stored, TaskFixture};
use graph_engine::{
    compat::{PyString, Value},
    domain::{execution, hosts},
};

pub fn revision_nine_baselines() {
    let fixture = TaskFixture::new();
    for (host, digests) in [
        (
            "codex",
            [
                "01861410ea766747abe4ed3f6482ba236280ab39b4ce298c1c4d2dc250394134",
                "c9be2561436c234e219f117b0d4ec4857520dbcf456ccc8d38b3fdad11074f5a",
                "c394fb86108039004fe3ac68cbd4fa077dc3d9a47cb4d054814a3edf7227f336",
            ],
        ),
        (
            "codex-astra",
            [
                "ccd4742a29a7d0bc8c96da28d33efb0205335347ec3b5483412222b65c62571d",
                "4190f3ce6a69cdfaaed7a2ecd5bc3da6af343054d366144cabb285e8637ec8ab",
                "7b2752700b4d398604a8610bef10b0e37bd390e2f6f01b9c37afa676ce4813a4",
            ],
        ),
    ] {
        for (index, digest) in digests.iter().enumerate() {
            let mut task = if index == 0 {
                fixture.default_task()
            } else {
                fixture.default_v2()
            };
            if index == 2 {
                overrides(
                    &mut task,
                    &[
                        "tech_lead",
                        "publication_assignment",
                        "simplification_assignment",
                    ],
                    hosts::MUSE_MODEL,
                    "low",
                );
            }
            let mut record = stored(host, Some(9));
            for marker in [
                "code_review_panel_version",
                "simplification_review_version",
                "capability_projection_version",
            ] {
                record.set(marker, 1_i64.into());
            }
            let old = execution::reconstruct_execution_plan("RUN-1", &task, &record, None).unwrap();
            assert!(old["plan_digest"].is_text(digest));
            assert!(old["model_options"].get("gpt-6.1-terra").is_none());
            assert!(
                execution::reconstruct_execution_plan("RUN-1", &task, &old, None)
                    .unwrap()
                    .py_eq(&old)
            );
            let current = build(&task, host, None);
            for (key, value) in old.as_object().unwrap() {
                if ["catalog_revision", "model_options", "plan_digest"]
                    .iter()
                    .any(|name| key == &PyString::from(*name))
                {
                    continue;
                }
                let selected = &current
                    .as_object()
                    .unwrap()
                    .iter()
                    .find(|(name, _)| name == key)
                    .unwrap()
                    .1;
                assert!(selected.py_eq(value), "changed historical field {key:?}");
            }
            if index == 2 {
                for plan in [&old, &current] {
                    for selection in [
                        assignment(plan, "tech_lead"),
                        &plan["publication_assignment"],
                        &plan["simplification_assignment"],
                    ] {
                        assert!(selection["dispatch_runtime"].is_text("opencode-cli"));
                    }
                }
            }
        }
    }
}

pub fn revision_six() {
    let fixture = TaskFixture::new();
    for (host, digest) in [
        (
            "codex",
            "76dda252f4a359ebf2b1b826f2c8cd74055115bec6c9dcdf06926e1db5f98193",
        ),
        (
            "codex-astra",
            "6f45a16d1e1e08f9f45941396d994367f4f7e4e54f882479e02b6bc5720807af",
        ),
    ] {
        let plan = rebuild(&fixture.default_v2(), host, Some(6), None);
        assert!(plan["plan_digest"].is_text(digest));
        assert!(plan["model_options"].get("gpt-6.1-sol").is_none());
        assert!(
            execution::reconstruct_execution_plan("RUN-1", &fixture.default_v2(), &plan, None)
                .unwrap()
                .py_eq(&plan)
        );
    }
}

pub fn sol_six_one() {
    let fixture = TaskFixture::new();
    for host in ["codex", "codex-astra"] {
        for effort in ["low", "medium", "high", "xhigh", "max"] {
            let mut task = fixture.default_v2();
            overrides(&mut task, &["tech_lead"], "gpt-6.1-sol", effort);
            let plan = build(&task, host, None);
            let selected = assignment(&plan, "tech_lead");
            assert!(selected["model"].is_text("gpt-6.1-sol"));
            assert!(selected["reasoning_effort"].is_text(effort));
            assert!(selected["dispatch_model"].is_text("gpt-6.1-sol"));
            assert!(
                execution::reconstruct_execution_plan("RUN-1", &task, &plan, None)
                    .unwrap()
                    .py_eq(&plan)
            );
            for revision in [3, 4, 5, 6] {
                assert_eq!(
                    execution::reconstruct_execution_plan(
                        "RUN-1",
                        &task,
                        &stored(host, Some(revision)),
                        None
                    )
                    .unwrap_err(),
                    "MODEL_ASSIGNMENT_INVALID"
                );
            }
        }
        for effort in ["none", "minimal", "ultra", "inherited"] {
            let mut task = fixture.default_v2();
            overrides(&mut task, &["tech_lead"], "gpt-6.1-sol", effort);
            assert_eq!(
                execution::build_execution_plan("RUN-1", &task, None, host).unwrap_err(),
                if effort == "inherited" {
                    "MODEL_OVERRIDES_INVALID"
                } else {
                    "MODEL_ASSIGNMENT_INVALID"
                }
            );
        }
    }
}

pub fn revision_three() {
    let fixture = TaskFixture::new();
    for (host, core) in [("codex", "gpt-5.6-sol"), ("codex-astra", "gpt-6-astra")] {
        let plan = rebuild(&fixture.default_v2(), host, Some(3), None);
        assert!(assignment(&plan, "tech_lead")["model"].is_text(core));
        assert!(assignment(&plan, "impact_mapper")["model"].is_text("gpt-5.6-luna"));
        assert!(plan["helper_recommendation"]["model"].is_text("gpt-5.6-luna"));
        for model in ["gpt-6-sol", "gpt-6-luna"] {
            assert!(plan["model_options"].get(model).is_none());
        }
        assert!(
            execution::reconstruct_execution_plan("RUN-1", &fixture.default_v2(), &plan, None)
                .unwrap()
                .py_eq(&plan)
        );
    }
}

pub fn revision_four() {
    let fixture = TaskFixture::new();
    for (host, digest) in [
        (
            "codex-astra",
            "4c7d29c083d2019ebda9f0c404f8053812b3198efe86169b866d5ecc53bffa89",
        ),
        (
            "codex",
            "550860b5764d72e9d59d53bcec408e5db8ab6391859d3dc55c7fb71a2d1d32db",
        ),
    ] {
        let plan = rebuild(&fixture.default_v2(), host, Some(4), None);
        assert!(plan["plan_digest"].is_text(digest));
        assert!(plan["helper_recommendation"]["reasoning_effort"].is_text("low"));
        assert!(assignment(&plan, "impact_mapper")["reasoning_effort"].is_text("low"));
    }
}

pub fn revision_five() {
    let fixture = TaskFixture::new();
    for (host, digest) in [
        (
            "codex-astra",
            "6d71352cd2cdf4119b8e55881a7075f9b6770c1f3592980bd6447773902ee9ac",
        ),
        (
            "codex",
            "bfffd45c8cb2d3b759aaa247feb1f20fdaa1b65881065d6a66858f8ac13c477a",
        ),
    ] {
        let plan = rebuild(&fixture.default_v2(), host, Some(5), None);
        assert!(plan["plan_digest"].is_text(digest));
        assert!(plan["helper_recommendation"].py_eq(&pair("gpt-6-luna", "max")));
        assert!(plan.get("economy_fanout_option").is_none());
    }
}

pub fn economy_fanout() {
    let fixture = TaskFixture::new();
    for host in ["codex-astra", "codex"] {
        let mut task = fixture.default_v2();
        overrides(
            &mut task,
            &[
                "impact_mapper",
                "design_research_architecture",
                "design_research_validation",
                "publication_assignment",
            ],
            "gpt-6.1-sol",
            "low",
        );
        let plan = build(&task, host, None);
        assert!(plan["economy_fanout_option"].py_eq(&pair("gpt-6.1-sol", "low")));
        for key in [
            "impact_mapper",
            "design_research_architecture",
            "design_research_validation",
        ] {
            assert!(assignment(&plan, key)["model"].is_text("gpt-6.1-sol"));
            assert!(assignment(&plan, key)["reasoning_effort"].is_text("low"));
        }
        assert!(plan["publication_assignment"]["model"].is_text("gpt-6.1-sol"));
        assert!(
            execution::reconstruct_execution_plan("RUN-1", &task, &plan, None)
                .unwrap()
                .py_eq(&plan)
        );
    }
}

pub fn alternative_models() {
    let fixture = TaskFixture::new();
    for (host, model, effort) in [
        ("cursor", "gemini-3.8-flash", "medium"),
        ("claude", "claude-sonnet-5", "high"),
        ("codex-astra", "gpt-6-sol", "medium"),
    ] {
        let mut task = fixture.default_v2();
        overrides(&mut task, &["tech_lead"], model, effort);
        let plan = build(&task, host, None);
        assert!(assignment(&plan, "tech_lead")["model"].is_text(model));
        assert!(assignment(&plan, "tech_lead")["reasoning_effort"].is_text(effort));
        assert!(
            execution::reconstruct_execution_plan("RUN-1", &task, &plan, None)
                .unwrap()
                .py_eq(&plan)
        );
    }
}

pub fn selected_history_rejected() {
    let fixture = TaskFixture::new();
    let mut task = fixture.default_v2();
    overrides(&mut task, &["tech_lead"], "gpt-5.6-sol", "medium");
    for revision in [None, Some(2)] {
        assert_eq!(
            execution::reconstruct_execution_plan(
                "RUN-1",
                &task,
                &stored("codex-astra", revision),
                None
            )
            .unwrap_err(),
            "MODEL_OVERRIDES_REQUIRE_CATALOG_3"
        );
    }
}

pub fn version_one_shape() {
    let fixture = TaskFixture::new();
    let plan = rebuild(&fixture.default_task(), "codex", None, None);
    let expected = [
        "approval_id",
        "approval_required",
        "assignments",
        "host",
        "mandatory_impact_tags",
        "minimum_route",
        "plan_digest",
        "publication_assignment",
        "run_id",
        "schema_version",
        "size",
        "size_recommendation",
        "size_recommendation_reason",
        "size_source",
        "supervisor_recommendation",
        "task_id",
    ];
    let mut names: Vec<_> = plan
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    names.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        names,
        expected.into_iter().map(PyString::from).collect::<Vec<_>>()
    );
    assert!(plan["plan_digest"]
        .is_text("4f1be289d36f4b025ab1a4e56d56cd1be6152246942e9a13370dd30f8be865f0"));
    assert!(
        build(&fixture.default_task(), hosts::DEFAULT_HOST, Some("small"))["size"].is_text("small")
    );
}

pub fn legacy_host_mappings() {
    let fixture = TaskFixture::new();
    let codex = rebuild(&fixture.default_v2(), "codex", Some(2), None);
    let cursor = rebuild(&fixture.default_v2(), "cursor", Some(2), None);
    assert!(codex["supervisor_recommendation"]["model"].is_text("gpt-5.6-sol"));
    assert!(codex["supervisor_recommendation"]["reasoning_effort"].is_text("xhigh"));
    assert!(cursor["supervisor_recommendation"]["model"].is_text("cursor-grok-4.6"));
    assert!(cursor["supervisor_recommendation"]["reasoning_effort"].is_text("high"));
    for key in ["senior_engineer", "tech_lead"] {
        assert!(assignment(&cursor, key)["model"].is_text("cursor-grok-4.6"));
        assert!(assignment(&cursor, key)["reasoning_effort"].is_text("medium"));
    }
}

pub fn claude_current() {
    let fixture = TaskFixture::new();
    let plan = build(&fixture.default_v2(), "claude", None);
    assert_eq!(plan["catalog_revision"].as_i64(), Some(4));
    for (key, model, effort) in [
        ("supervisor_recommendation", "claude-opus-5-5", "high"),
        ("publication_assignment", "claude-sonnet-5-5", "low"),
    ] {
        let mut expected = pair(model, effort);
        expected.set("dispatch_model", model.into());
        assert!(plan[key].py_eq(&expected));
    }
    for (key, model, effort) in [
        ("impact_mapper", "claude-sonnet-5-5", "low"),
        ("senior_engineer", "claude-opus-5-5", "medium"),
    ] {
        let selected = assignment(&plan, key);
        assert!(selected["model"].is_text(model));
        assert!(selected["reasoning_effort"].is_text(effort));
        assert!(selected["dispatch_model"].is_text(model));
    }
}

pub fn claude_previous_options() {
    let fixture = TaskFixture::new();
    for (host, revision, digest) in [
        (
            "claude",
            3,
            "2872b241695fa9ee062567d3bce2911b2cd0da989f751e2cbfc97750a30c40dc",
        ),
        (
            "cursor",
            4,
            "1e023755045a153b3d59db4c4e71b233167e9688fcf55de72c32f102a6afa963",
        ),
    ] {
        let previous = rebuild(&fixture.default_v2(), host, Some(revision), None);
        assert!(previous["plan_digest"].is_text(digest));
        assert!(previous["model_options"].get("claude-opus-5-5").is_none());
        for model in ["claude-sonnet-5-5", "claude-opus-5-5"] {
            for effort in ["low", "medium", "high", "xhigh", "max"] {
                let mut task = fixture.default_v2();
                overrides(&mut task, &["tech_lead"], model, effort);
                let plan = build(&task, host, None);
                assert!(assignment(&plan, "tech_lead")["model"].is_text(model));
                assert!(
                    execution::reconstruct_execution_plan("RUN-1", &task, &plan, None)
                        .unwrap()
                        .py_eq(&plan)
                );
                assert_eq!(
                    execution::reconstruct_execution_plan("RUN-1", &task, &previous, None)
                        .unwrap_err(),
                    "MODEL_ASSIGNMENT_INVALID"
                );
            }
        }
    }
}

pub fn missing_host() {
    let fixture = TaskFixture::new();
    let legacy = rebuild(&fixture.default_task(), "codex", None, Some("medium"));
    assert!(execution::reconstruct_execution_plan(
        "RUN-1",
        &fixture.default_task(),
        &Value::Object(vec![]),
        Some("medium")
    )
    .unwrap()
    .py_eq(&legacy));
}

pub fn astra_revision_two() {
    let fixture = TaskFixture::new();
    for task in [fixture.default_task(), fixture.default_v2()] {
        for size in ["small", "medium", "large"] {
            let default = rebuild(&task, "codex", Some(2), Some(size));
            let astra = rebuild(&task, "codex-astra", Some(2), Some(size));
            assert_eq!(astra["catalog_revision"].as_i64(), Some(2));
            assert!(!astra["plan_digest"].py_eq(&default["plan_digest"]));
            assert!(astra["minimum_route"].py_eq(&default["minimum_route"]));
            assert!(astra["publication_assignment"].py_eq(&default["publication_assignment"]));
            let mut supervisor = pair("gpt-6-astra", "xhigh");
            supervisor.set("dispatch_model", "gpt-6-astra".into());
            assert!(astra["supervisor_recommendation"].py_eq(&supervisor));
            for (original, selected) in default["assignments"]
                .as_list()
                .unwrap()
                .iter()
                .zip(astra["assignments"].as_list().unwrap())
            {
                let mut expected = original.clone();
                if original["intelligence_class"].is_text("reasoning") {
                    expected.set("model", "gpt-6-astra".into());
                    expected.set("dispatch_model", "gpt-6-astra".into());
                }
                for (keys, effort) in [
                    (
                        &["tech_lead", "senior_engineer", "test_engineer"][..],
                        "low",
                    ),
                    (
                        &["architect", "code_reviewer", "security_reviewer"][..],
                        "medium",
                    ),
                ] {
                    if keys.iter().any(|key| original["node_key"].is_text(key)) {
                        expected.set("intelligence_class", "reasoning".into());
                        expected.set("model", "gpt-6-astra".into());
                        expected.set("dispatch_model", "gpt-6-astra".into());
                        expected.set("reasoning_effort", effort.into());
                    }
                }
                assert!(selected.py_eq(&expected));
            }
        }
    }
}

pub fn frozen_catalog_digests() {
    let task = Value::object([
        ("schema_version", 1_i64.into()),
        ("task_id", "CATALOG-COMPAT".into()),
        ("minimum_route", "full_delivery".into()),
        ("mandatory_impact_tags", Value::List(vec![])),
        ("risk_level", "low".into()),
    ]);
    for (host, digests) in [
        (
            "codex",
            [
                "17bfe0160948551e4221aa55eb72b86cc6618cb11dce867379788e6125dbf03e",
                "8c9039c939b99b9bba57cd64732181daac5a32d7c8705c021e787d35804383f2",
                "fe2f684917696ef47080eb9b40e17bfadcb0a075b204222d36fdc8f7b44f41e3",
            ],
        ),
        (
            "cursor",
            [
                "76daf3f9f53d336d30ca3fa65d694b2145fe953d66d50a7748c2eaa332ef46f0",
                "6396dbcdbc2b33415b393e2441bf61ab187f918fb038418eb9b5e637a6bc388f",
                "f256f89795d0731cd0de5ec49dfbbcbf1a8cb8983740e7e0399e9d0e5e00d6ad",
            ],
        ),
        (
            "codex-astra",
            [
                "7265a44577ffb1c7c4f67edfe5d849a5409453556479bb0382753e45793ac93c",
                "54d8495428d739eeeca53aa151a1fb643f5b67b2e42afab2a73a03cd11cdea0e",
                "a1f1a10b84d22a1d8a130fd40df9389bf87c9803f853a39f1fff059c2965274c",
            ],
        ),
    ] {
        for (size, digest) in ["small", "medium", "large"].into_iter().zip(digests) {
            let plan = execution::reconstruct_execution_plan(
                "RUN-COMPAT",
                &task,
                &stored(host, None),
                Some(size),
            )
            .unwrap();
            assert!(plan.get("catalog_revision").is_none());
            assert!(plan["plan_digest"].is_text(digest));
            let mut current_task = task.clone();
            current_task.set("request_mode", "delivery".into());
            assert!(!execution::build_execution_plan(
                "RUN-COMPAT",
                &current_task,
                Some(size),
                host
            )
            .unwrap()["plan_digest"]
                .py_eq(&plan["plan_digest"]));
        }
    }
}

pub fn catalog_markers() {
    let fixture = TaskFixture::new();
    for host in hosts::HOSTS {
        let revision = hosts::current_revision(host).unwrap();
        for marker in [
            Value::Null,
            true.into(),
            false.into(),
            Value::Float(revision as f64),
            revision.to_string().into(),
            0_i64.into(),
            (revision + 1).into(),
            Value::List(vec![]),
            Value::Object(vec![]),
        ] {
            let mut record = stored(host, None);
            record.set("catalog_revision", marker);
            assert_eq!(
                execution::reconstruct_execution_plan(
                    "RUN-1",
                    &fixture.default_task(),
                    &record,
                    None
                )
                .unwrap_err(),
                "CATALOG_REVISION_INVALID"
            );
        }
        let mut record = stored(host, Some(revision));
        for marker in [
            "code_review_panel_version",
            "simplification_review_version",
            "capability_projection_version",
        ] {
            record.set(marker, 1_i64.into());
        }
        let revised =
            execution::reconstruct_execution_plan("RUN-1", &fixture.default_task(), &record, None)
                .unwrap();
        assert!(revised.py_eq(&build(&fixture.default_task(), host, None)));
    }
}
