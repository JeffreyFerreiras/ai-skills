from pathlib import Path

from graph_engine.execution import (
    CLASS_ASSIGNMENTS, SIZE_ASSIGNMENTS, assignment_for, build_execution_plan,
    reconstruct_execution_plan, validate_model_assignment, validate_new_plan_assignment,
)
from graph_engine.hosts import (
    CURRENT_CATALOG_REVISIONS, DEFAULT_HOST, dispatch_weight_for, known_hosts,
    resolve_assignment, supported_dispatch_weights, selected_dispatch_model, MUSE_MODEL,
)
from graph_engine.ids import stable_id
from graph_engine.planner import validate_fanout_ordering

from tests.test_support import TaskCase


EXPECTED_SIZE_ASSIGNMENTS = {
    "small": {
        "impact_mapper": ("gpt-5.6-luna", "max"),
        "advisory_reviewer": ("gpt-5.6-luna", "max"),
        "tech_lead": ("gpt-5.6-sol", "medium"),
        "architect": ("gpt-5.6-sol", "medium"),
        "senior_engineer": ("gpt-5.6-luna", "max"),
        "code_reviewer": ("gpt-5.6-luna", "max"),
        "test_engineer": ("gpt-5.6-luna", "max"),
        "audio_realtime_specialist": ("gpt-5.6-luna", "max"),
        "ios_platform_specialist": ("gpt-5.6-luna", "max"),
        "release_operations_reviewer": ("gpt-5.6-sol", "medium"),
        "security_reviewer": ("gpt-5.6-sol", "high"),
        "supervisor": ("primary-thread", "inherited"),
    },
    "medium": {
        "impact_mapper": ("gpt-5.6-luna", "max"),
        "advisory_reviewer": ("gpt-5.6-sol", "medium"),
        "tech_lead": ("gpt-5.6-sol", "medium"),
        "architect": ("gpt-5.6-sol", "high"),
        "senior_engineer": ("gpt-5.6-sol", "medium"),
        "code_reviewer": ("gpt-5.6-sol", "high"),
        "test_engineer": ("gpt-5.6-luna", "max"),
        "audio_realtime_specialist": ("gpt-5.6-sol", "high"),
        "ios_platform_specialist": ("gpt-5.6-sol", "high"),
        "release_operations_reviewer": ("gpt-5.6-sol", "high"),
        "security_reviewer": ("gpt-5.6-sol", "high"),
        "supervisor": ("primary-thread", "inherited"),
    },
    "large": {
        "impact_mapper": ("gpt-5.6-luna", "max"),
        "advisory_reviewer": ("gpt-5.6-sol", "high"),
        "tech_lead": ("gpt-5.6-sol", "high"),
        "architect": ("gpt-5.6-sol", "xhigh"),
        "senior_engineer": ("gpt-5.6-sol", "high"),
        "code_reviewer": ("gpt-5.6-sol", "xhigh"),
        "test_engineer": ("gpt-5.6-sol", "high"),
        "audio_realtime_specialist": ("gpt-5.6-sol", "high"),
        "ios_platform_specialist": ("gpt-5.6-sol", "high"),
        "release_operations_reviewer": ("gpt-5.6-sol", "high"),
        "security_reviewer": ("gpt-5.6-sol", "xhigh"),
        "supervisor": ("primary-thread", "inherited"),
    },
}


class PlannerUnitTests(TaskCase):
    def test_bug_hunter_binds_host_skill_and_review_model(self):
        for host, required_skill in (("codex", "review-agent"), ("codex-astra", "review-agent"),
                                     ("cursor", "review-bugbot"), ("claude", "bug-hunter-review")):
            with self.subTest(host=host):
                plan = build_execution_plan("RUN-1", self.task_v2(), host=host)
                hunter = assignment_for(plan, "code_reviewer_bug_hunter")
                self.assertEqual(hunter["role"], "code_reviewer")
                self.assertEqual(hunter["code_review_assignment"], {"focus": "bug_hunting", "required_skill": required_skill})
                self.assertEqual((hunter["model"], hunter["reasoning_effort"]),
                                 (assignment_for(plan, "code_reviewer")["model"], "high"))
                self.assertEqual(reconstruct_execution_plan("RUN-1", self.task_v2(), plan), plan)

    def test_invalid_panel_version_fails_closed(self):
        for marker in (None, True, 0, 2, "1", []):
            with self.subTest(marker=marker), self.assertRaisesRegex(ValueError, "CODE_REVIEW_PANEL_VERSION_INVALID"):
                reconstruct_execution_plan("RUN-1", self.task(), {"code_review_panel_version": marker})

    def test_retired_models_cannot_enter_new_plans_or_legacy_selection_fallback(self):
        for host in known_hosts():
            for model in ("gpt-5.6-sol", "gpt-5.6-luna", "gpt-5.6-terra"):
                for revision in (None, CURRENT_CATALOG_REVISIONS[host]):
                    with self.subTest(host=host, model=model, revision=revision):
                        with self.assertRaisesRegex(ValueError, "MODEL_RETIRED"):
                            selected_dispatch_model(host, model, "max", revision)
                for key in ("tech_lead", "impact_mapper", "design_research_validation",
                            "supervisor_recommendation", "publication_assignment"):
                    task = self.task_v2()
                    task["model_overrides"] = {key: {"model": model, "reasoning_effort": "max"}}
                    with self.subTest(host=host, model=model, key=key):
                        with self.assertRaisesRegex(ValueError, "MODEL_RETIRED"):
                            build_execution_plan("RUN-1", task, host=host)

    def test_revision_seven_and_cursor_three_keep_recorded_digests(self):
        for host, revision, digest in (
            ("codex", 7, "72366eef1929dba0e3b467888c0d3d3a716e82725909ea46ab94afd02f745732"),
            ("codex-astra", 7, "7af80b3eb9bcf15fa5f8da734629f2e10210790272027657aafd9fbb986733f8"),
            ("cursor", 3, "6192d520850fc5d31af8ef58d7712ffc6417a213cfffb56c6da5b486313361c5"),
        ):
            with self.subTest(host=host):
                plan = reconstruct_execution_plan(
                    "RUN-1", self.task_v2(), {"host": host, "catalog_revision": revision},
                )
                self.assertEqual(plan["plan_digest"], digest)
                self.assertIn("gpt-5.6-sol", plan["model_options"])
                self.assertEqual(reconstruct_execution_plan("RUN-1", self.task_v2(), plan), plan)
                self.assertNotEqual(build_execution_plan("RUN-1", self.task_v2(), host=host)["plan_digest"], digest)

    def test_historical_retired_override_is_a_record_not_a_new_selection(self):
        for host, revision in (("codex", 7), ("codex-astra", 7), ("cursor", 3)):
            task = self.task_v2()
            task["model_overrides"] = {
                "tech_lead": {"model": "gpt-5.6-sol", "reasoning_effort": "medium"},
            }
            plan = reconstruct_execution_plan("RUN-1", task, {"host": host, "catalog_revision": revision})
            self.assertEqual(assignment_for(plan, "tech_lead")["model"], "gpt-5.6-sol")
            self.assertEqual(reconstruct_execution_plan("RUN-1", task, plan), plan)
            with self.assertRaisesRegex(ValueError, "MODEL_RETIRED"):
                build_execution_plan("RUN-1", task, host=host)

    def test_current_recommendations_and_available_alternatives(self):
        expected = {
            "codex-astra": ("gpt-6-astra", "gpt-6.1-sol"),
            "codex": ("gpt-6.1-sol", "gpt-6.1-sol"),
            "claude": ("claude-opus-5-5", "claude-sonnet-5-5"),
            "cursor": ("grok-4.7", "gemini-3.8-flash"),
        }
        for host, (core, scout) in expected.items():
            plan = build_execution_plan("RUN-1", self.task_v2(), host=host)
            self.assertEqual(plan["catalog_revision"], CURRENT_CATALOG_REVISIONS[host])
            for node in ("tech_lead", "senior_engineer", "test_engineer"):
                row = assignment_for(plan, node)
                self.assertEqual((row["model"], row["reasoning_effort"]), (core, "medium"))
            helper_effort = "low"
            self.assertEqual(plan["helper_recommendation"], {"model": scout, "reasoning_effort": helper_effort})
            for node in ("impact_mapper", "design_research_architecture", "design_research_validation"):
                row = assignment_for(plan, node)
                self.assertEqual((row["model"], row["reasoning_effort"]), (scout, helper_effort))
            self.assertEqual(plan["publication_assignment"]["reasoning_effort"], helper_effort)
            if host in {"codex", "codex-astra"}:
                self.assertEqual(plan["economy_fanout_option"], {
                    "model": "gpt-6.1-sol", "reasoning_effort": "low",
                })
                self.assertEqual(plan["external_economy_option"], {
                    "model": MUSE_MODEL, "reasoning_effort": "xhigh", "dispatch_runtime": "opencode-cli",
                })
                self.assertEqual(plan["model_options"][MUSE_MODEL],
                                 ["minimal", "low", "medium", "high", "xhigh"])
            else:
                self.assertNotIn("economy_fanout_option", plan)
                self.assertNotIn("external_economy_option", plan)
                self.assertNotIn(MUSE_MODEL, plan["model_options"])
            self.assertIn(core, plan["model_options"])
            self.assertEqual(reconstruct_execution_plan("RUN-1", self.task_v2(), plan), plan)
        for host in known_hosts():
            plan = build_execution_plan("RUN-1", self.task_v2(), host=host)
            self.assertFalse(any(model.startswith("gpt-5.6-") for model in plan["model_options"]))
            self.assertFalse(any(row["model"].startswith("gpt-5.6-") for row in plan["assignments"]))
        self.assertIn("gpt-6-sol", build_execution_plan("RUN-1", self.task_v2())["model_options"])
        self.assertIn("gpt-6.1-sol", build_execution_plan("RUN-1", self.task_v2())["model_options"])

    def test_muse_rejects_unavailable_effort_host_and_supervisor(self):
        for host, node, effort, error in (
            ("codex", "impact_mapper", "max", "MODEL_ASSIGNMENT_INVALID"),
            ("claude", "impact_mapper", "low", "MODEL_ASSIGNMENT_INVALID"),
            ("cursor", "impact_mapper", "low", "MODEL_ASSIGNMENT_INVALID"),
            ("codex-astra", "supervisor_recommendation", "low", "SUPERVISOR_MODEL_RUNTIME_UNSUPPORTED"),
        ):
            task = self.task_v2()
            task["model_overrides"] = {node: {"model": MUSE_MODEL, "reasoning_effort": effort}}
            with self.subTest(host=host, node=node), self.assertRaisesRegex(ValueError, error):
                build_execution_plan("RUN-1", task, host=host)

    def test_codex_revision_six_retains_approved_plan_digests(self):
        expected_digests = {
            "codex": "76dda252f4a359ebf2b1b826f2c8cd74055115bec6c9dcdf06926e1db5f98193",
            "codex-astra": "6f45a16d1e1e08f9f45941396d994367f4f7e4e54f882479e02b6bc5720807af",
        }
        for host, digest in expected_digests.items():
            plan = reconstruct_execution_plan(
                "RUN-1", self.task_v2(), {"host": host, "catalog_revision": 6},
            )
            self.assertEqual(plan["plan_digest"], digest)
            self.assertNotIn("gpt-6.1-sol", plan["model_options"])
            self.assertEqual(reconstruct_execution_plan("RUN-1", self.task_v2(), plan), plan)

    def test_sol_six_one_selections_reconstruct_and_reject_unsupported_efforts(self):
        for host in ("codex", "codex-astra"):
            for effort in ("low", "medium", "high", "xhigh", "max"):
                with self.subTest(host=host, effort=effort):
                    task = self.task_v2()
                    task["model_overrides"] = {
                        "tech_lead": {"model": "gpt-6.1-sol", "reasoning_effort": effort},
                    }
                    plan = build_execution_plan("RUN-1", task, host=host)
                    selected = assignment_for(plan, "tech_lead")
                    self.assertEqual((selected["model"], selected["reasoning_effort"], selected["dispatch_model"]),
                                     ("gpt-6.1-sol", effort, "gpt-6.1-sol"))
                    self.assertEqual(reconstruct_execution_plan("RUN-1", task, plan), plan)
                    for revision in (3, 4, 5, 6):
                        with self.assertRaisesRegex(ValueError, "MODEL_ASSIGNMENT_INVALID"):
                            reconstruct_execution_plan("RUN-1", task, {"host": host, "catalog_revision": revision})
            for effort in ("none", "minimal", "ultra", "inherited"):
                task = self.task_v2()
                task["model_overrides"] = {
                    "tech_lead": {"model": "gpt-6.1-sol", "reasoning_effort": effort},
                }
                error = "MODEL_OVERRIDES_INVALID" if effort == "inherited" else "MODEL_ASSIGNMENT_INVALID"
                with self.subTest(host=host, effort=effort), self.assertRaisesRegex(ValueError, error):
                    build_execution_plan("RUN-1", task, host=host)

    def test_codex_revision_three_reconstructs_original_recommendations_and_options(self):
        for host, core in (("codex", "gpt-5.6-sol"), ("codex-astra", "gpt-6-astra")):
            plan = reconstruct_execution_plan(
                "RUN-1", self.task_v2(), {"host": host, "catalog_revision": 3},
            )
            self.assertEqual(assignment_for(plan, "tech_lead")["model"], core)
            self.assertEqual(assignment_for(plan, "impact_mapper")["model"], "gpt-5.6-luna")
            self.assertEqual(plan["helper_recommendation"]["model"], "gpt-5.6-luna")
            self.assertNotIn("gpt-6-sol", plan["model_options"])
            self.assertNotIn("gpt-6-luna", plan["model_options"])
            self.assertEqual(reconstruct_execution_plan("RUN-1", self.task_v2(), plan), plan)

    def test_codex_revision_four_retains_approved_plan_digests(self):
        expected_digests = {
            "codex-astra": "4c7d29c083d2019ebda9f0c404f8053812b3198efe86169b866d5ecc53bffa89",
            "codex": "550860b5764d72e9d59d53bcec408e5db8ab6391859d3dc55c7fb71a2d1d32db",
        }
        for host, digest in expected_digests.items():
            plan = reconstruct_execution_plan(
                "RUN-1", self.task_v2(), {"host": host, "catalog_revision": 4},
            )
            self.assertEqual(plan["plan_digest"], digest)
            self.assertEqual(plan["helper_recommendation"]["reasoning_effort"], "low")
            self.assertEqual(assignment_for(plan, "impact_mapper")["reasoning_effort"], "low")

    def test_codex_revision_five_retains_approved_plan_digests(self):
        expected_digests = {
            "codex-astra": "6d71352cd2cdf4119b8e55881a7075f9b6770c1f3592980bd6447773902ee9ac",
            "codex": "bfffd45c8cb2d3b759aaa247feb1f20fdaa1b65881065d6a66858f8ac13c477a",
        }
        for host, digest in expected_digests.items():
            plan = reconstruct_execution_plan(
                "RUN-1", self.task_v2(), {"host": host, "catalog_revision": 5},
            )
            self.assertEqual(plan["plan_digest"], digest)
            self.assertEqual(plan["helper_recommendation"], {
                "model": "gpt-6-luna", "reasoning_effort": "max",
            })
            self.assertNotIn("economy_fanout_option", plan)

    def test_codex_economy_fanout_selection_is_exact_and_reconstructs(self):
        for host in ("codex-astra", "codex"):
            task = self.task_v2()
            pair = {"model": "gpt-6.1-sol", "reasoning_effort": "low"}
            task["model_overrides"] = {
                key: pair for key in (
                    "impact_mapper", "design_research_architecture", "design_research_validation",
                    "publication_assignment",
                )
            }
            plan = build_execution_plan("RUN-1", task, host=host)
            self.assertEqual(plan["economy_fanout_option"], pair)
            for key in ("impact_mapper", "design_research_architecture", "design_research_validation"):
                assignment = assignment_for(plan, key)
                self.assertEqual((assignment["model"], assignment["reasoning_effort"]),
                                 ("gpt-6.1-sol", "low"))
            self.assertEqual(plan["publication_assignment"]["model"], "gpt-6.1-sol")
            self.assertEqual(reconstruct_execution_plan("RUN-1", task, plan), plan)

    def test_alternative_models_are_not_rejected_as_nondefault(self):
        for host, model, effort in (("cursor", "gemini-3.8-flash", "medium"),
                                    ("claude", "claude-sonnet-5", "high"),
                                    ("codex-astra", "gpt-6-sol", "medium")):
            task = self.task_v2()
            task["model_overrides"] = {"tech_lead": {"model": model, "reasoning_effort": effort}}
            plan = build_execution_plan("RUN-1", task, host=host)
            selected = assignment_for(plan, "tech_lead")
            self.assertEqual((selected["model"], selected["reasoning_effort"]), (model, effort))
            self.assertEqual(reconstruct_execution_plan("RUN-1", task, plan), plan)

    def test_selected_plan_cannot_be_reinterpreted_as_historical(self):
        task = self.task_v2()
        task["model_overrides"] = {"tech_lead": {"model": "gpt-5.6-sol", "reasoning_effort": "medium"}}
        for revision in (None, 2):
            stored = {"host": "codex-astra"}
            if revision is not None:
                stored["catalog_revision"] = revision
            with self.assertRaisesRegex(ValueError, "MODEL_OVERRIDES_REQUIRE_CATALOG_3"):
                reconstruct_execution_plan("RUN-1", task, stored)

    def test_v1_plan_shape_and_digest_remain_frozen(self):
        plan = reconstruct_execution_plan("RUN-1", self.task(), {"host": "codex"})
        self.assertEqual(
            set(plan), {
                "approval_id", "approval_required", "assignments", "host",
                "mandatory_impact_tags", "minimum_route", "plan_digest",
                "publication_assignment", "run_id", "schema_version", "size",
                "size_recommendation", "size_recommendation_reason", "size_source",
                "supervisor_recommendation", "task_id",
            },
        )
        self.assertEqual(
            plan["plan_digest"],
            "4f1be289d36f4b025ab1a4e56d56cd1be6152246942e9a13370dd30f8be865f0",
        )
        self.assertEqual(
            build_execution_plan("RUN-1", self.task(), "small")["size"], "small",
        )

    def test_v2_full_delivery_uses_structured_size_policy(self):
        cases = (
            ({}, "small", ["bounded_low_risk_low_uncertainty"]),
            ({"risk": "medium"}, "medium", ["risk_medium"]),
            ({"scope_extent": "cross_file"}, "medium", ["scope_cross_file"]),
            ({"uncertainty": "medium"}, "medium", ["uncertainty_medium"]),
            ({"tags": ["release_operations"]}, "medium", ["mandatory_nonsecurity_impact_tag"]),
            ({"risk": "high"}, "large", ["risk_high_or_critical"]),
            ({"risk": "critical", "tags": ["security_privacy"]}, "large", ["risk_high_or_critical", "security_privacy_required"]),
            ({"tags": ["security_privacy"]}, "large", ["security_privacy_required"]),
            ({"uncertainty": "high"}, "large", ["uncertainty_high"]),
            ({"scope_extent": "broadly_cross_cutting"}, "large", ["scope_broadly_cross_cutting"]),
        )
        for arguments, expected_size, reasons in cases:
            with self.subTest(arguments=arguments):
                plan = build_execution_plan("RUN-1", self.task_v2(**arguments))
                self.assertEqual(plan["size"], expected_size)
                self.assertEqual(plan["size_policy_version"], 2)
                self.assertEqual(plan["size_recommendation_reason_codes"], reasons)
                self.assertEqual(
                    set(plan["size_recommendation_inputs"]),
                    {"risk_level", "mandatory_impact_tags", "model_sizing"},
                )
                self.assertEqual(plan["minimum_route"], "full_delivery")

    def test_v2_override_cannot_drop_below_safety_floor(self):
        with self.assertRaisesRegex(ValueError, "EXECUTION_SIZE_BELOW_SAFETY_FLOOR"):
            build_execution_plan("RUN-1", self.task_v2(risk="high"), "medium")
        with self.assertRaisesRegex(ValueError, "EXECUTION_SIZE_BELOW_SAFETY_FLOOR"):
            build_execution_plan("RUN-1", self.task_v2(scope_extent="cross_file"), "small")
        self.assertEqual(
            build_execution_plan("RUN-1", self.task_v2(), "large")["size"], "large",
        )

    def test_host_and_supervisor_mappings_are_unchanged_for_v2(self):
        codex = reconstruct_execution_plan("RUN-1", self.task_v2(), {"host": "codex", "catalog_revision": 2})
        cursor = reconstruct_execution_plan("RUN-1", self.task_v2(), {"host": "cursor", "catalog_revision": 2})
        self.assertEqual(
            (codex["supervisor_recommendation"]["model"], codex["supervisor_recommendation"]["reasoning_effort"]),
            ("gpt-5.6-sol", "xhigh"),
        )
        self.assertEqual(
            (cursor["supervisor_recommendation"]["model"], cursor["supervisor_recommendation"]["reasoning_effort"]),
            ("cursor-grok-4.6", "high"),
        )
        cursor_assignments = {
            item["node_key"]: (item["model"], item["reasoning_effort"])
            for item in cursor["assignments"]
        }
        self.assertEqual(cursor_assignments["senior_engineer"], ("cursor-grok-4.6", "medium"))
        self.assertEqual(cursor_assignments["tech_lead"], ("cursor-grok-4.6", "medium"))

    def test_size_assignment_matrix_is_exact(self):
        self.assertEqual(SIZE_ASSIGNMENTS, EXPECTED_SIZE_ASSIGNMENTS)

    def test_impact_mapper_always_uses_economy_class(self):
        assignments = {
            size: roles["impact_mapper"] for size, roles in CLASS_ASSIGNMENTS.items()
        }
        self.assertEqual(
            assignments,
            {
                "small": ("economy", "max"),
                "medium": ("economy", "max"),
                "large": ("economy", "max"),
            },
        )

    def test_luna_and_design_model_invariants_hold_at_every_size(self):
        for size, assignments in SIZE_ASSIGNMENTS.items():
            for role, (model, effort) in assignments.items():
                if model == "gpt-5.6-luna":
                    self.assertEqual(effort, "max", (size, role))
            self.assertEqual(assignments["tech_lead"][0], "gpt-5.6-sol")
            self.assertEqual(assignments["architect"][0], "gpt-5.6-sol")

    def test_astra_is_the_default_host(self):
        plan = build_execution_plan("RUN-1", self.task(), "medium")
        explicit = build_execution_plan("RUN-1", self.task(), "medium", host="codex-astra")
        self.assertEqual(plan, explicit)
        self.assertEqual(plan["host"], DEFAULT_HOST)
        by_key = {item["node_key"]: item for item in plan["assignments"]}
        self.assertEqual(plan["catalog_revision"], 9)
        self.assertEqual(by_key["tech_lead"]["model"], "gpt-6-astra")
        self.assertEqual(by_key["tech_lead"]["reasoning_effort"], "medium")
        self.assertEqual(by_key["tech_lead"]["dispatch_model"], "gpt-6-astra")
        self.assertEqual(by_key["impact_mapper"]["model"], "gpt-6.1-sol")
        self.assertEqual(by_key["impact_mapper"]["reasoning_effort"], "low")
        self.assertEqual(plan["supervisor_recommendation"]["model"], "gpt-6-astra")
        self.assertEqual(plan["publication_assignment"]["model"], "gpt-6.1-sol")
        self.assertEqual(plan["publication_assignment"]["reasoning_effort"], "low")

    def test_every_host_can_expand_the_class_matrix(self):
        for host in known_hosts():
            for size, roles in CLASS_ASSIGNMENTS.items():
                for role, (intelligence_class, effort) in roles.items():
                    model, resolved = resolve_assignment(host, intelligence_class, effort)
                    self.assertTrue(model, (host, size, role))
                    self.assertTrue(resolved, (host, size, role))

    def test_claude_catalog_recommends_exact_models_and_efforts(self):
        plan = build_execution_plan("RUN-1", self.task_v2(), host="claude")
        self.assertEqual(plan["catalog_revision"], 4)
        self.assertEqual(plan["supervisor_recommendation"], {
            "model": "claude-opus-5-5", "reasoning_effort": "high", "dispatch_model": "claude-opus-5-5",
        })
        self.assertEqual(plan["publication_assignment"], {
            "model": "claude-sonnet-5-5", "reasoning_effort": "low", "dispatch_model": "claude-sonnet-5-5",
        })
        assignments = {
            item["node_key"]: (item["model"], item["reasoning_effort"], item["dispatch_model"])
            for item in plan["assignments"]
        }
        self.assertEqual(assignments["impact_mapper"], ("claude-sonnet-5-5", "low", "claude-sonnet-5-5"))
        self.assertEqual(assignments["senior_engineer"], ("claude-opus-5-5", "medium", "claude-opus-5-5"))

    def test_claude_55_options_preserve_previous_catalog_digests(self):
        for host, revision, digest in (
            ("claude", 3, "2872b241695fa9ee062567d3bce2911b2cd0da989f751e2cbfc97750a30c40dc"),
            ("cursor", 4, "1e023755045a153b3d59db4c4e71b233167e9688fcf55de72c32f102a6afa963"),
        ):
            previous = reconstruct_execution_plan("RUN-1", self.task_v2(),
                                                  {"host": host, "catalog_revision": revision})
            self.assertEqual(previous["plan_digest"], digest)
            self.assertNotIn("claude-opus-5-5", previous["model_options"])
            for model in ("claude-sonnet-5-5", "claude-opus-5-5"):
                for effort in ("low", "medium", "high", "xhigh", "max"):
                    task = self.task_v2()
                    task["model_overrides"] = {"tech_lead": {"model": model, "reasoning_effort": effort}}
                    plan = build_execution_plan("RUN-1", task, host=host)
                    self.assertEqual(assignment_for(plan, "tech_lead")["model"], model)
                    self.assertEqual(reconstruct_execution_plan("RUN-1", task, plan), plan)
                    with self.assertRaisesRegex(ValueError, "MODEL_ASSIGNMENT_INVALID"):
                        reconstruct_execution_plan("RUN-1", task, previous)

    def test_historical_missing_host_keeps_codex_catalog(self):
        legacy = reconstruct_execution_plan("RUN-1", self.task(), {"host": "codex"}, "medium")
        self.assertEqual(reconstruct_execution_plan("RUN-1", self.task(), {}, "medium"), legacy)

    def test_astra_catalog_revision_two_core_assignments_for_both_task_versions(self):
        for task, size in ((task, size) for task in (self.task(), self.task_v2())
                           for size in ("small", "medium", "large")):
            default = reconstruct_execution_plan("RUN-1", task, {"host": "codex", "catalog_revision": 2}, size)
            astra = reconstruct_execution_plan("RUN-1", task, {"host": "codex-astra", "catalog_revision": 2}, size)
            self.assertEqual(astra["catalog_revision"], 2)
            self.assertNotEqual(astra["plan_digest"], default["plan_digest"])
            self.assertEqual(astra["minimum_route"], default["minimum_route"])
            self.assertEqual(astra["publication_assignment"], default["publication_assignment"])
            self.assertEqual(astra["supervisor_recommendation"], {
                "model": "gpt-6-astra", "reasoning_effort": "xhigh", "dispatch_model": "gpt-6-astra",
            })
            for original, selected in zip(default["assignments"], astra["assignments"]):
                expected = dict(original)
                if original["intelligence_class"] == "reasoning":
                    expected.update(model="gpt-6-astra", dispatch_model="gpt-6-astra")
                if original["node_key"] in {"tech_lead", "senior_engineer", "test_engineer"}:
                    expected.update(intelligence_class="reasoning", model="gpt-6-astra",
                                    dispatch_model="gpt-6-astra", reasoning_effort="low")
                if original["node_key"] in {"architect", "code_reviewer", "security_reviewer"}:
                    expected.update(intelligence_class="reasoning", model="gpt-6-astra",
                                    dispatch_model="gpt-6-astra", reasoning_effort="medium")
                self.assertEqual(selected, expected)

    def test_frozen_catalog_digests_remain_exact(self):
        task = {"schema_version": 1, "task_id": "CATALOG-COMPAT", "minimum_route": "full_delivery",
                "mandatory_impact_tags": [], "risk_level": "low"}
        digests = {
            "codex": ("17bfe0160948551e4221aa55eb72b86cc6618cb11dce867379788e6125dbf03e",
                      "8c9039c939b99b9bba57cd64732181daac5a32d7c8705c021e787d35804383f2",
                      "fe2f684917696ef47080eb9b40e17bfadcb0a075b204222d36fdc8f7b44f41e3"),
            "cursor": ("76daf3f9f53d336d30ca3fa65d694b2145fe953d66d50a7748c2eaa332ef46f0",
                       "6396dbcdbc2b33415b393e2441bf61ab187f918fb038418eb9b5e637a6bc388f",
                       "f256f89795d0731cd0de5ec49dfbbcbf1a8cb8983740e7e0399e9d0e5e00d6ad"),
            "codex-astra": ("7265a44577ffb1c7c4f67edfe5d849a5409453556479bb0382753e45793ac93c",
                            "54d8495428d739eeeca53aa151a1fb643f5b67b2e42afab2a73a03cd11cdea0e",
                            "a1f1a10b84d22a1d8a130fd40df9389bf87c9803f853a39f1fff059c2965274c"),
        }
        for host, expected in digests.items():
            for size, expected_digest in zip(("small", "medium", "large"), expected):
                plan = reconstruct_execution_plan("RUN-COMPAT", task, {"host": host}, size)
                self.assertNotIn("catalog_revision", plan)
                self.assertEqual(plan["plan_digest"], expected_digest, (host, size))
                self.assertNotEqual(build_execution_plan("RUN-COMPAT", task, size, host)["plan_digest"], plan["plan_digest"])

    def test_catalog_revision_markers_fail_closed(self):
        for host in known_hosts():
            revision = CURRENT_CATALOG_REVISIONS[host]
            invalid_markers = (None, True, False, float(revision), str(revision), 0,
                               revision + 1, [], {})
            for marker in invalid_markers:
                with self.subTest(host=host, marker=marker):
                    with self.assertRaisesRegex(ValueError, "CATALOG_REVISION_INVALID"):
                        reconstruct_execution_plan("RUN-1", self.task(),
                                                   {"host": host, "catalog_revision": marker})
            revised = reconstruct_execution_plan("RUN-1", self.task(),
                                                 {"host": host, "catalog_revision": revision,
                                                  "code_review_panel_version": 1})
            self.assertEqual(revised, build_execution_plan("RUN-1", self.task(), host=host))

    def test_historical_delegation_plan_reconstructs_without_catalog_upgrade(self):
        from tests.test_reviewer_delegation import policy_config

        for task in (self.task(), self.task_v2()):
            task["reviewer_delegation"] = policy_config()
            legacy = reconstruct_execution_plan("RUN-1", task, {"host": "codex-astra"}, "small")
            rebuilt = reconstruct_execution_plan("RUN-1", task, legacy, "small")
            self.assertEqual(rebuilt, legacy)
            self.assertEqual(rebuilt["schema_version"], 2)
            self.assertNotIn("catalog_revision", rebuilt)
            with self.assertRaisesRegex(ValueError, "CODE_REVIEW_PANEL_EXTRA_REVIEWER_FORBIDDEN"):
                build_execution_plan("RUN-1", task, "small", "codex-astra")
            task["reviewer_delegation"]["assignments"][0]["role"] = "security_reviewer"
            candidate = build_execution_plan("RUN-1", task, "small", "codex-astra")
            self.assertEqual(candidate["reviewer_delegation_limits"], legacy["reviewer_delegation_limits"])
            self.assertNotEqual(candidate["plan_digest"], legacy["plan_digest"])

    def test_historical_retired_review_selection_reconstructs_but_new_plan_rejects_it(self):
        from tests.test_reviewer_delegation import policy_config

        task = self.task_v2()
        task["reviewer_delegation"] = policy_config()
        task["reviewer_delegation"]["assignments"][0]["model"] = "gpt-5.6-sol"
        plan = reconstruct_execution_plan("RUN-1", task, {"host": "codex-astra", "catalog_revision": 7})
        self.assertEqual(reconstruct_execution_plan("RUN-1", task, plan), plan)
        with self.assertRaisesRegex(ValueError, "MODEL_RETIRED"):
            build_execution_plan("RUN-1", task)

    def test_astra_medium_delegation_weight_is_model_specific(self):
        weights = supported_dispatch_weights()
        self.assertEqual(dispatch_weight_for("gpt-6-astra", "medium"), 3)
        self.assertEqual(weights[("gpt-6-astra", "medium")], 3)
        for pair in (("gpt-6-astra", "low"), ("gpt-5.6-sol", "medium")):
            self.assertIsNone(dispatch_weight_for(*pair))
            self.assertNotIn(pair, weights)

    def test_astra_assignments_reject_unapproved_efforts_and_catalogs(self):
        for effort in ("none", "minimal", "ultra", "inherited"):
            with self.subTest(effort=effort):
                with self.assertRaisesRegex(ValueError, "MODEL_ASSIGNMENT_INVALID"):
                    validate_model_assignment("tech_lead", "gpt-6-astra", effort, host="codex-astra")
        with self.assertRaisesRegex(ValueError, "MODEL_ASSIGNMENT_INVALID"):
            validate_model_assignment("tech_lead", "gpt-6-astra", "high", host="codex")

    def test_new_writers_are_reasoning_and_legacy_writers_remain_loadable(self):
        for host in known_hosts():
            for task in (self.task(), self.task_v2()):
                for size in ("small", "medium", "large"):
                    with self.subTest(host=host, task=task["schema_version"], size=size):
                        new = reconstruct_execution_plan("RUN-1", task, {"host": host, "catalog_revision": 1 if host == "claude" else 2}, size)
                        writer = assignment_for(new, "senior_engineer")
                        self.assertEqual(writer["intelligence_class"], "reasoning")
                        legacy = reconstruct_execution_plan("RUN-1", task, {"host": host}, size)
                        self.assertEqual(reconstruct_execution_plan("RUN-1", task, legacy, size), legacy)
                        recorded = assignment_for(legacy, "senior_engineer")
                        expected_class = CLASS_ASSIGNMENTS[size]["senior_engineer"][0]
                        self.assertEqual(recorded["intelligence_class"], expected_class)
                        for node in new["assignments"]:
                            if node["node_key"] != "senior_engineer" and host != "codex-astra":
                                self.assertEqual(node, assignment_for(legacy, node["node_key"]))

    def test_new_writer_validation_rejects_economy_even_with_valid_effort(self):
        for host in known_hosts():
            model, effort = resolve_assignment(host, "economy", "max")
            with self.subTest(host=host):
                with self.assertRaisesRegex(ValueError, "IMPLEMENTATION_REASONING_MODEL_REQUIRED"):
                    validate_new_plan_assignment("senior_engineer", model, effort, host)
                validate_model_assignment("senior_engineer", model, effort, host)
                plan = reconstruct_execution_plan("RUN-1", self.task(), {"host": host, "catalog_revision": 1 if host == "claude" else 2}, "small")
                writer = next(row for row in plan["assignments"] if row["node_key"] == "senior_engineer")
                writer.update(model=model, reasoning_effort=effort, intelligence_class="economy")
                with self.assertRaisesRegex(ValueError, "IMPLEMENTATION_REASONING_MODEL_REQUIRED"):
                    assignment_for(plan, "senior_engineer")

    def test_execution_plan_prefers_node_assignment_then_role_fallback(self):
        for size in SIZE_ASSIGNMENTS:
            assignments = {
                item["node_key"]: (item["model"], item["reasoning_effort"])
                for item in reconstruct_execution_plan("RUN-1", self.task(), {"host": "codex"}, size)["assignments"]
            }
            self.assertEqual(assignments["advisory_reviewer"], SIZE_ASSIGNMENTS[size]["advisory_reviewer"])
            self.assertEqual(assignments["supervisor_design_consolidation"], SIZE_ASSIGNMENTS[size]["supervisor"])
            self.assertEqual(assignments["supervisor_delivery_consolidation"], SIZE_ASSIGNMENTS[size]["supervisor"])

    def test_profile_defaults_match_medium_runtime_assignments(self):
        profile_roles = {
            "impact_mapper": "impact_mapper",
            "tech_lead": "tech_lead",
            "software_architect": "architect",
            "senior_engineer": "senior_engineer",
            "code_reviewer": "code_reviewer",
            "test_engineer": "test_engineer",
            "security_reviewer": "security_reviewer",
        }
        profile_root = Path(__file__).resolve().parents[1] / "profile-agents"
        assignments = {
            item["node_key"]: item
            for item in build_execution_plan("RUN-1", self.task(), "medium")["assignments"]
        }
        for profile_name, assignment_role in profile_roles.items():
            lines = (profile_root / (profile_name + ".toml")).read_text(encoding="utf-8").splitlines()
            effort_line = next(line for line in lines if line.startswith("model_reasoning_effort = "))
            effort = effort_line.split('"', 2)[1]
            model_line = next(line for line in lines if line.startswith("model = "))
            self.assertEqual(model_line.split('"', 2)[1], assignments[assignment_role]["model"], profile_name)
            self.assertEqual(effort, assignments[assignment_role]["reasoning_effort"], profile_name)

    def test_delivery_only_plan_uses_route_aware_dispatch_metadata(self):
        assignments = {
            item["node_key"]: item
            for item in build_execution_plan("RUN-1", self.task_delivery_only())["assignments"]
        }
        for node_key in ("senior_engineer", "code_reviewer", "test_engineer"):
            self.assertEqual(
                assignments[node_key]["dispatch_when"],
                "delivery_only or full_delivery route",
            )

    def test_model_assignment_invariant_fails_closed(self):
        with self.assertRaisesRegex(ValueError, "ECONOMY_REASONING_EFFORT_REQUIRED"):
            validate_model_assignment("impact_mapper", "gpt-5.6-luna", "high")
        with self.assertRaisesRegex(ValueError, "DESIGN_MODEL_REQUIRED"):
            validate_model_assignment("tech_lead", "gpt-5.6-luna", "max")
        with self.assertRaisesRegex(ValueError, "HOST_UNSUPPORTED"):
            validate_model_assignment("tech_lead", "gpt-5.6-sol", "medium", host="unknown")
        self.assertEqual(dispatch_weight_for("gpt-5.6-luna", "max", historical=True), 3)
        self.assertEqual(dispatch_weight_for("gpt-5.6-sol", "high", historical=True), 3)
        self.assertEqual(dispatch_weight_for("gpt-5.6-sol", "xhigh", historical=True), 4)
        self.assertEqual(dispatch_weight_for("gpt-5.6-sol", "max", historical=True), 5)
        self.assertIsNone(dispatch_weight_for("gpt-5.6-luna", "high"))

    def test_ids_are_stable_across_ordering(self):
        first = stable_id("RUN-1", "a" * 64, "branch", "architect", 0)
        second = stable_id("RUN-1", "a" * 64, "branch", "architect", 0)
        changed = stable_id("RUN-1", "a" * 64, "branch", "architect", 1)
        self.assertEqual(first, second)
        self.assertNotEqual(first, changed)

    @staticmethod
    def _member(branch_id, paths=None, services=None):
        return {
            "branch_id": branch_id,
            "resources": {
                "writable_paths": paths or [], "mutable_state_refs": [],
                "exclusive_device_refs": [], "services": services or [],
            },
        }

    def test_fanout_conflicts_require_transitive_ordering(self):
        members = [
            self._member("A", [{"path": "src", "scope": "subtree"}]),
            self._member("B", [{"path": "src/app.py", "scope": "exact"}]),
            self._member("C"),
        ]
        with self.assertRaisesRegex(ValueError, "FANOUT_UNORDERED_CONFLICT"):
            validate_fanout_ordering(members, [], case_sensitive=True)
        dependencies = [
            {"before_branch_id": "A", "after_branch_id": "C", "reason": "first"},
            {"before_branch_id": "C", "after_branch_id": "B", "reason": "then"},
        ]
        self.assertEqual(
            len(validate_fanout_ordering(members, dependencies, case_sensitive=True)), 2,
        )

    def test_fanout_rejects_cycles_and_over_capacity_antichains(self):
        service = {"ref": "gpu", "units": 1, "capacity": 1}
        members = [self._member("A", services=[service]), self._member("B", services=[service])]
        with self.assertRaisesRegex(ValueError, "FANOUT_CAPACITY_EXCEEDED"):
            validate_fanout_ordering(members, [], case_sensitive=True)
        ordered = [{"before_branch_id": "A", "after_branch_id": "B", "reason": "capacity"}]
        validate_fanout_ordering(members, ordered, case_sensitive=True)
        with self.assertRaisesRegex(ValueError, "FANOUT_CYCLE"):
            validate_fanout_ordering(
                members,
                ordered + [{"before_branch_id": "B", "after_branch_id": "A", "reason": "cycle"}],
                case_sensitive=True,
            )

    def test_fanout_case_policy_is_explicit(self):
        members = [
            self._member("A", [{"path": "src/Feature.py", "scope": "exact"}]),
            self._member("B", [{"path": "src/feature.py", "scope": "exact"}]),
        ]
        with self.assertRaisesRegex(ValueError, "FANOUT_UNORDERED_CONFLICT"):
            validate_fanout_ordering(members, [], case_sensitive=False)
        self.assertEqual(
            validate_fanout_ordering(members, [], case_sensitive=True), [],
        )
