import json
from pathlib import Path

from graph_engine.config import load_policy
from graph_engine.execution import (
    CODE_REVIEW_PANEL, assignment_for, build_execution_plan, reconstruct_execution_plan,
)
from graph_engine.hosts import DEFAULT_HOST, MUSE_MODEL
from graph_engine.planner import (
    NodeSpec, delivery_review_nodes, design_research_nodes, design_review_nodes,
    envelope, initial_route_nodes,
)

from tests.test_support import GraphCase


class PlannerTests(GraphCase):
    def test_new_panel_binds_four_independent_scopes_and_model_selections(self):
        from graph_engine.contracts import validate_task_brief
        from tests.test_contracts import _validate_json_schema

        policy, snapshot = load_policy(self.repo)
        task = self.task_v2(route="fast_path")
        task["model_overrides"] = {
            "code_reviewer_architecture": {"model": "gpt-6-astra", "reasoning_effort": "medium"},
        }
        normalized = validate_task_brief(task, snapshot.digest, policy)
        plan = build_execution_plan("RUN-1", normalized)
        self.assertEqual(plan["code_review_panel_version"], 1)
        self.assertEqual(reconstruct_execution_plan("RUN-1", normalized, plan), plan)
        specs = delivery_review_nodes(policy, [], 0, plan)
        reviewers = [spec for spec in specs if spec.role == "code_reviewer"]
        self.assertEqual({spec.key for spec in reviewers}, set(CODE_REVIEW_PANEL))
        self.assertTrue(all(spec.mandatory for spec in reviewers))
        schema = json.loads((Path(__file__).parents[1] / "references" / "branch-envelope.schema.json").read_text(encoding="utf-8"))
        envelopes = [envelope("RUN-1", snapshot.digest, policy, normalized, spec, "ready", [], execution_plan=plan) for spec in reviewers]
        self.assertEqual(len({item["branch_id"] for item in envelopes}), 4)
        for item in envelopes:
            _validate_json_schema(item, schema, schema)
            self.assertEqual(item["code_review_assignment"], CODE_REVIEW_PANEL[item["node_key"]])
        naming = next(item for item in envelopes if item["node_key"] == "code_reviewer_naming")
        self.assertEqual((naming["model"], naming["reasoning_effort"]), ("gpt-6.1-sol", "low"))
        self.assertEqual(assignment_for(plan, "code_reviewer_naming")["intelligence_class"], "economy")
        self.assertTrue(all(cap["effect"] in {"filesystem_read", "external_read"} for cap in naming["effect_capabilities"]))
        normalized["model_overrides"]["code_reviewer_naming"] = {"model": "gpt-6-astra", "reasoning_effort": "high"}
        override = assignment_for(build_execution_plan("RUN-1", normalized), "code_reviewer_naming")
        self.assertEqual((override["model"], override["reasoning_effort"]), ("gpt-6-astra", "high"))

    def test_historical_plan_keeps_one_reviewer_and_no_focus_envelope(self):
        policy, snapshot = load_policy(self.repo)
        task = self.task()
        legacy = reconstruct_execution_plan("RUN-1", task, {"host": DEFAULT_HOST})
        specs = delivery_review_nodes(policy, [], 0, legacy)
        reviewers = [spec for spec in specs if spec.role == "code_reviewer"]
        self.assertEqual([spec.key for spec in reviewers], ["code_reviewer"])
        packet = envelope("RUN-1", snapshot.digest, policy, task, reviewers[0], "ready", [], execution_plan=legacy)
        self.assertNotIn("code_review_assignment", packet)
        self.assertEqual(reconstruct_execution_plan("RUN-1", task, legacy), legacy)

    def test_muse_selection_uses_opencode_and_preserves_previous_catalog(self):
        for host in ("codex", "codex-astra"):
            task = self.task_v2()
            task["model_overrides"] = {
                "impact_mapper": {"model": MUSE_MODEL, "reasoning_effort": "low"},
                "design_research_architecture": {"model": MUSE_MODEL, "reasoning_effort": "low"},
                "tech_lead": {"model": MUSE_MODEL, "reasoning_effort": "medium"},
                "publication_assignment": {"model": MUSE_MODEL, "reasoning_effort": "low"},
            }
            plan = build_execution_plan("RUN-1", task, host=host)
            for node, effort in (("impact_mapper", "low"),
                                 ("design_research_architecture", "low"),
                                 ("tech_lead", "medium")):
                assignment = assignment_for(plan, node)
                self.assertEqual((assignment["model"], assignment["reasoning_effort"],
                                  assignment["dispatch_model"], assignment["dispatch_runtime"]),
                                 (MUSE_MODEL, effort, MUSE_MODEL, "opencode-cli"))
            self.assertEqual(reconstruct_execution_plan("RUN-1", task, plan), plan)
            self.assertEqual(plan["publication_assignment"]["dispatch_runtime"], "opencode-cli")
            policy, snapshot = load_policy(self.repo)
            research = design_research_nodes(policy, 0)[0]
            branch = envelope("RUN-1", snapshot.digest, policy, task, research, "pending", [],
                              execution_plan=plan)
            self.assertEqual(branch["dispatch_runtime"], "opencode-cli")
            previous = reconstruct_execution_plan(
                "RUN-1", self.task_v2(), {"host": host, "catalog_revision": 8},
            )
            self.assertEqual(previous["plan_digest"], {
                "codex": "d273f7915dbecda79e789826eb37919981a792caf1cbad0c72c481e56f701a6f",
                "codex-astra": "63fdff6e6791bee4acff9bc725d8d52367b06a561ac3b88f1654151cb8e6de6e",
            }[host])
            self.assertNotIn(MUSE_MODEL, previous["model_options"])
            self.assertNotIn("external_economy_option", previous)
            self.assertEqual(reconstruct_execution_plan("RUN-1", self.task_v2(), previous), previous)
            with self.assertRaisesRegex(ValueError, "MODEL_ASSIGNMENT_INVALID"):
                reconstruct_execution_plan("RUN-1", task, {"host": host, "catalog_revision": 8})

    def test_approved_muse_mapper_claim_retains_cli_dispatch_route(self):
        task = self.task_v2(route="fast_path")
        task["model_overrides"] = {
            "impact_mapper": {"model": MUSE_MODEL, "reasoning_effort": "xhigh"},
        }
        self.initialize_task(task)
        branch = self.claim_raw()
        self.assertEqual((branch["model"], branch["reasoning_effort"],
                          branch["dispatch_runtime"]),
                         (MUSE_MODEL, "xhigh", "opencode-cli"))
        self.graphctl("status", "--run-id", "RUN-1")

    def test_preview_can_be_adjusted_before_initialization_and_approved_selection_survives_resume(self):
        task = self.task_v2(route="fast_path")
        path = self.repo / "docs" / "task.json"
        path.write_text(json.dumps(task), encoding="utf-8")
        before = self.graphctl("plan", "--run-id", "RUN-1", "--task-brief", str(path))["execution_plan"]
        self.assertFalse(self.store.db_path("albanian-live-translate", "RUN-1").exists())
        task["model_overrides"] = {
            "senior_engineer": {"model": "gpt-6.1-sol", "reasoning_effort": "medium"},
            "supervisor_recommendation": {"model": "gpt-6.1-sol", "reasoning_effort": "high"},
        }
        path.write_text(json.dumps(task), encoding="utf-8")
        adjusted = self.graphctl("plan", "--run-id", "RUN-1", "--task-brief", str(path))["execution_plan"]
        self.assertNotEqual(before["plan_digest"], adjusted["plan_digest"])
        initialized = self.initialize_task(task)
        self.assertEqual(initialized["execution_plan_digest"], adjusted["plan_digest"])
        self.graphctl("--ack-degraded-permissions", "--ack-degraded-durability", "resume", "--run-id", "RUN-1")
        self.impact("fast_path")
        writer = self.claim()
        self.assertEqual((writer["model"], writer["reasoning_effort"]), ("gpt-6.1-sol", "medium"))

    def test_invalid_model_overrides_fail_without_creating_state(self):
        from graph_engine.contracts import ContractError
        for overrides in ([], {"unknown_role": {"model": "gpt-5.6-sol", "reasoning_effort": "medium"}},
                          {"tech_lead": {"model": "primary-thread", "reasoning_effort": "inherited"}},
                          {"tech_lead": {"model": "gpt-5.6-sol", "reasoning_effort": "ultra"}},
                          {"tech_lead": {"model": "grok-4.7", "reasoning_effort": "medium"}}):
            task = self.task_v2()
            task["model_overrides"] = overrides
            with self.subTest(overrides=overrides), self.assertRaises(ContractError):
                self.initialize_task(task)
            self.assertFalse(self.store.db_path("albanian-live-translate", "RUN-1").exists())

    def _normalized_topology(self):
        database = self.store.db_path("albanian-live-translate", "RUN-1")
        with self.store.connect(database) as connection:
            run = connection.execute("SELECT * FROM runs WHERE run_id='RUN-1'").fetchone()
            node_rows = connection.execute(
                "SELECT * FROM nodes WHERE run_id='RUN-1' ORDER BY branch_id"
            ).fetchall()
            node_identity = {
                row["branch_id"]: (
                    row["node_key"], row["role"], row["stage"], row["generation"],
                    row["specialist_tag"],
                )
                for row in node_rows
            }
            join_rows = connection.execute(
                "SELECT * FROM joins WHERE run_id='RUN-1' ORDER BY join_id"
            ).fetchall()
            join_identity = {
                row["join_id"]: (
                    row["join_key"], row["kind"], row["stage"], row["generation"],
                )
                for row in join_rows
            }
            fanout_rows = connection.execute(
                "SELECT * FROM fanouts WHERE run_id='RUN-1' ORDER BY fanout_id"
            ).fetchall()
            fanout_identity = {
                row["fanout_id"]: (row["stage"], row["generation"])
                for row in fanout_rows
            }
            plan_row = connection.execute(
                "SELECT * FROM execution_plans WHERE run_id='RUN-1'"
            ).fetchone()
            task = json.loads(run["task_json"])
            return {
                "run": {
                    "status": run["status"],
                    "request_mode": run["request_mode"],
                    "minimum_route": run["minimum_route"],
                    "selected_route": run["selected_route"],
                    "selected_tags": json.loads(run["selected_tags_json"] or "[]"),
                },
                "approval_barrier": {
                    "plan_schema_version": json.loads(plan_row["plan_json"])["schema_version"],
                    "approval_required": json.loads(plan_row["plan_json"])["approval_required"],
                    "status": plan_row["status"],
                    "approved": all(
                        plan_row[key] is not None
                        for key in ("authority_ref", "approved_at", "approved_by", "approval_digest")
                    ),
                },
                "closure_requirements": {
                    "acceptance_ids": task["acceptance_ids"],
                    "required_check_ids": task["required_check_ids"],
                    "required_human_decisions": task["required_human_decisions"],
                },
                "nodes": sorted((
                    node_identity[row["branch_id"]], bool(row["mandatory"]), row["status"],
                    row["retry_count"], row["max_retries"],
                    node_identity.get(row["parent_branch_id"]), row["depth"],
                ) for row in node_rows),
                "joins": sorted((
                    join_identity[row["join_id"]], row["status"], bool(row["degraded"]),
                ) for row in join_rows),
                "join_members": sorted((
                    join_identity[row["join_id"]], node_identity[row["branch_id"]],
                    bool(row["mandatory"]),
                ) for row in connection.execute(
                    "SELECT * FROM join_members ORDER BY join_id,branch_id"
                )),
                "fanouts": sorted((
                    fanout_identity[row["fanout_id"]], row["status"],
                    tuple(sorted(node_identity[item] for item in json.loads(row["member_branch_ids_json"]))),
                ) for row in fanout_rows),
                "fanout_dependencies": sorted((
                    fanout_identity[row["fanout_id"]], node_identity[row["before_branch_id"]],
                    node_identity[row["after_branch_id"]], row["reason"],
                ) for row in connection.execute(
                    "SELECT * FROM fanout_dependencies ORDER BY fanout_id,before_branch_id,after_branch_id"
                )),
            }

    def _full_delivery_topology_trace(self, size):
        initialized = self.initialize_task(self.task_v2(), size=size, approve=False, host="codex")
        assignments = {
            item["node_key"]: (
                item["role"], item["intelligence_class"], item["model"],
                item["reasoning_effort"], item["dispatch_when"],
            )
            for item in initialized["execution_plan"]["assignments"]
        }
        trace = [self._normalized_topology()]
        self.graphctl(
            "record", "plan-approval", "--run-id", "RUN-1",
            "--plan-digest", initialized["execution_plan_digest"], "--decision", "APPROVE",
            "--authority-ref", "authority:test", "--op-id", "plan-approval-1",
        )
        trace.append(self._normalized_topology())

        specialist_tags = ["release_operations"]
        self.impact("full_delivery", specialist_tags)
        trace.append(self._normalized_topology())
        self._assess_current_fanout_in_order()
        trace.append(self._normalized_topology())
        for _ in range(2):
            self.success(self.claim_raw())
        self.advance("research_collection")
        trace.append(self._normalized_topology())

        tech_lead = self.claim_raw()
        self.assertEqual(tech_lead["node_key"], "tech_lead")
        self.success(tech_lead)
        self.advance("design_inputs")
        trace.append(self._normalized_topology())
        self._assess_current_fanout_in_order()
        trace.append(self._normalized_topology())
        for _ in range(2):
            self.success(self.claim_raw(), "APPROVE")
        self.advance("design_collection")
        trace.append(self._normalized_topology())
        self.consolidation("design", "APPROVE")
        self.advance("design_consolidation")
        trace.append(self._normalized_topology())

        engineer = self.claim_raw()
        self.assertEqual(engineer["node_key"], "senior_engineer")
        self.success(engineer, "IMPLEMENTED")
        self.advance("implementation")
        trace.append(self._normalized_topology())
        self._assess_current_fanout_in_order()
        trace.append(self._normalized_topology())
        for _ in range(6):
            self.success(self.claim_raw(), "APPROVE")
        self.advance("delivery_collection")
        trace.append(self._normalized_topology())
        self.consolidation("delivery", "ACCEPT")
        self.advance("delivery_consolidation")
        trace.append(self._normalized_topology())
        return trace, assignments

    def _assess_current_fanout_in_order(self):
        status = self.graphctl("status", "--run-id", "RUN-1")
        action = status["next_action"]
        fanout = next(
            item for item in status["fanouts"] if item["fanout_id"] == action["fanout_id"]
        )
        dependencies = [
            {
                "before_branch_id": before,
                "after_branch_id": after,
                "reason": "topology-order",
            }
            for before, after in zip(
                fanout["member_branch_ids"], fanout["member_branch_ids"][1:]
            )
        ]
        self.assess_fanout(action["fanout_id"], dependencies)

    def test_v2_sizes_change_assignments_without_changing_full_delivery_gates(self):
        traces = {}
        assignments = {}
        for index, size in enumerate(("small", "medium", "large")):
            if index:
                self.tearDown()
                self.setUp()
            traces[size], assignments[size] = self._full_delivery_topology_trace(size)
        self.assertEqual(traces["small"], traces["medium"])
        self.assertEqual(traces["small"], traces["large"])
        self.assertEqual(
            traces["small"][0]["approval_barrier"],
            {
                "plan_schema_version": 1, "approval_required": True,
                "status": "pending", "approved": False,
            },
        )
        self.assertEqual(
            traces["small"][1]["approval_barrier"]["status"], "approved",
        )
        self.assertTrue(traces["small"][1]["approval_barrier"]["approved"])

        final = traces["small"][-1]
        self.assertEqual(
            {item[0][0] for item in final["nodes"]},
            {
                "impact_mapper", "design_research_architecture", "design_research_validation",
                "tech_lead", "architect", "release_operations_reviewer", "senior_engineer",
                "code_reviewer", "code_reviewer_architecture", "code_reviewer_naming", "code_reviewer_bug_hunter",
                "test_engineer", "supervisor_design_consolidation",
                "supervisor_delivery_consolidation",
            },
        )
        self.assertTrue(all(item[1] for item in final["nodes"]))
        self.assertTrue(all(item[2] for item in final["join_members"]))
        self.assertEqual(
            {
                item[0][2] for item in final["nodes"]
                if item[0][0] == "release_operations_reviewer"
            },
            {"design", "delivery"},
        )
        self.assertEqual(
            {item[0][0] for item in final["joins"]},
            {
                "research_collection", "design_inputs", "design_collection",
                "design_consolidation", "implementation", "delivery_collection",
                "delivery_consolidation", "closure",
            },
        )
        self.assertEqual(
            {item[0][0] for item in final["fanouts"]},
            {"research", "design", "delivery"},
        )
        self.assertEqual(len(final["fanout_dependencies"]), 7)
        closure = next(item for item in final["joins"] if item[0][0] == "closure")
        self.assertEqual(closure[1], "open")
        self.assertEqual(final["run"]["selected_tags"], ["release_operations"])
        self.assertEqual(
            final["closure_requirements"],
            {
                "acceptance_ids": ["AC-001"], "required_check_ids": ["repo-check"],
                "required_human_decisions": [],
            },
        )
        engineer_assignments = {
            size: plan["senior_engineer"] for size, plan in assignments.items()
        }
        self.assertEqual(engineer_assignments["small"], engineer_assignments["medium"])
        self.assertEqual(engineer_assignments["medium"], engineer_assignments["large"])

    def test_default_cli_plan_survives_approval_claim_and_resume(self):
        initialized = self.initialize(size="medium")
        self.impact("full_delivery")
        lead = self.claim()
        self.assertEqual((lead["model"], lead["reasoning_effort"]), ("gpt-6-astra", "medium"))
        self.graphctl("--ack-degraded-permissions", "--ack-degraded-durability",
                      "resume", "--run-id", "RUN-1")
        plan = self.graphctl("status", "--run-id", "RUN-1")["execution_plan"]
        self.assertEqual(plan["host"], "codex-astra")
        self.assertEqual(plan["plan_digest"], initialized["execution_plan_digest"])
        self.assertEqual(plan["status"], "approved")

    def test_astra_catalog_survives_approval_claim_and_resume(self):
        initialized = self.initialize(host="codex-astra", size="medium")
        self.impact("full_delivery")
        lead = self.claim()
        self.assertEqual(lead["node_key"], "tech_lead")
        self.assertEqual(lead["model"], "gpt-6-astra")
        self.assertEqual(lead["reasoning_effort"], "medium")
        self.graphctl("--ack-degraded-permissions", "--ack-degraded-durability",
                      "resume", "--run-id", "RUN-1")
        plan = self.graphctl("status", "--run-id", "RUN-1")["execution_plan"]
        self.assertEqual(plan["host"], "codex-astra")
        self.assertEqual(plan["plan_digest"], initialized["execution_plan_digest"])
        self.assertEqual(plan["status"], "approved")

    def test_every_route_has_exact_entry(self):
        policy, _ = load_policy(self.repo)
        expected = {
            "advisory": "advisory_reviewer",
            "delivery_only": "senior_engineer",
            "fast_path": "senior_engineer",
        }
        self.assertEqual({route: initial_route_nodes(policy, route)[0].key for route in expected}, expected)
        for route in ("design_only", "full_delivery"):
            self.assertEqual(
                [node.key for node in initial_route_nodes(policy, route)],
                ["design_research_architecture", "design_research_validation"],
            )

    def test_research_envelopes_split_budget_and_project_read_only_capabilities(self):
        policy, snapshot = load_policy(self.repo)
        task = self.task()
        task["authority"]["capabilities"] = [
            {"effect": "filesystem_read", "action": "read", "target_ref": "repo:docs/"},
            {"effect": "filesystem_write", "action": "edit", "target_ref": "repo:docs/"},
            {"effect": "external_read", "action": "inspect", "target_ref": "andromeda"},
            {"effect": "command", "action": "run", "target_ref": "npm-run-check"},
        ]
        nodes = design_research_nodes(policy, 3)
        envelopes = [
            envelope("RUN-1", snapshot.digest, policy, task, node, "pending", [])
            for node in nodes
        ]
        self.assertEqual([item["research_assignment"]["focus"] for item in envelopes], ["architecture", "validation"])
        totals = task["inspection_budget"]
        for key in totals:
            self.assertLessEqual(
                sum(item["research_assignment"]["inspection_budget"][key] for item in envelopes),
                totals[key],
            )
        for item in envelopes:
            self.assertEqual(
                {cap["effect"] for cap in item["effect_capabilities"]},
                {"filesystem_read", "external_read"},
            )

    def test_research_node_assignments_reuse_impact_mapper(self):
        policy, _ = load_policy(self.repo)
        execution = build_execution_plan("RUN-1", self.task(), "small")
        by_key = {item["node_key"]: item for item in execution["assignments"]}
        for node in design_research_nodes(policy, 0):
            self.assertEqual(
                (node.role, by_key[node.key]["model"], by_key[node.key]["reasoning_effort"]),
                ("impact_mapper", "gpt-6.1-sol", "low"),
            )

    def test_multiple_specialists_are_canonical_and_mandatory(self):
        policy, _ = load_policy(self.repo)
        nodes = design_review_nodes(policy, ["security_privacy", "audio_realtime_translation"], 0)
        self.assertEqual([node.specialist_tag for node in nodes[1:]], ["audio_realtime_translation", "security_privacy"])
        self.assertTrue(all(node.mandatory for node in nodes))

    def test_policy_and_role_bounds_cannot_add_task_authority(self):
        policy, snapshot = load_policy(self.repo)
        task = self.task()
        task["authority"]["capabilities"].append({"effect": "deploy", "action": "deploy", "target_ref": "production"})
        env = envelope("RUN-1", snapshot.digest, policy, task, NodeSpec("senior_engineer", "senior_engineer", "implementation", 0), "ready", [])
        self.assertNotIn("deploy", {cap["effect"] for cap in env["effect_capabilities"]})
