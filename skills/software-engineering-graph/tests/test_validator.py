import copy
import json
from pathlib import Path
from unittest.mock import patch

from graph_engine.config import load_policy
from graph_engine.contracts import ContractError, validate_result_manifest
from graph_engine.execution import reconstruct_execution_plan
from graph_engine.ids import canonical_bytes, sha256_bytes
from graph_engine.state import StateError
from graph_engine.validator import (
    compute_delivery_outcome, compute_design_outcome, validate_consolidation_manifest,
    verify_resume, verify_semantic_state, _delivery_outcome_without_delegated_echoes,
    _predates_phase_dispositions, _validate_execution_plan,
)

from tests.test_support import GraphCase


class ValidatorTests(GraphCase):
    def test_codex_and_cursor_legacy_writer_approval_survives_resume_and_claim(self):
        for index, host in enumerate(("codex", "cursor")):
            if index:
                self.tearDown()
                self.setUp()
            task = self.task(route="fast_path")
            legacy = reconstruct_execution_plan("RUN-1", task, {"host": host}, "small")
            with patch("graph_engine.cli.build_execution_plan", return_value=legacy):
                self.initialize_task(task, host=host, approve=False, size="small")
            database = self.store.db_path("albanian-live-translate", "RUN-1")
            self.graphctl("record", "plan-approval", "--run-id", "RUN-1", "--plan-digest",
                          legacy["plan_digest"], "--decision", "APPROVE", "--authority-ref",
                          "authority:test", "--op-id", "legacy-approval")
            with self.store.connect(database) as connection:
                before = dict(connection.execute("SELECT * FROM execution_plans").fetchone())
            self.graphctl("--ack-degraded-permissions", "--ack-degraded-durability",
                          "resume", "--run-id", "RUN-1")
            self.impact("fast_path")
            writer = self.claim()
            recorded = next(row for row in legacy["assignments"] if row["node_key"] == "senior_engineer")
            self.assertEqual(writer["node_key"], "senior_engineer")
            self.assertEqual(recorded["intelligence_class"], "economy")
            self.assertNotIn("simplification_review_version", writer)
            self.assertEqual((writer["model"], writer["reasoning_effort"]),
                             (recorded["model"], recorded["reasoning_effort"]))
            with self.store.connect(database) as connection:
                after = dict(connection.execute("SELECT * FROM execution_plans").fetchone())
            self.assertEqual(after, before)

    def test_new_codex_and_cursor_writer_assignment_survives_resume_and_claim(self):
        for index, host in enumerate(("codex", "cursor")):
            if index:
                self.tearDown()
                self.setUp()
            initialized = self.initialize(route="fast_path", host=host, size="small")
            self.graphctl("--ack-degraded-permissions", "--ack-degraded-durability",
                          "resume", "--run-id", "RUN-1")
            self.impact("fast_path")
            writer = self.claim()
            self.assertEqual(writer["node_key"], "senior_engineer")
            self.assertEqual(writer["reasoning_effort"], "medium")
            self.assertEqual(writer["simplification_review_version"], 1)
            self.assertIn(writer["model"], {"gpt-6.1-sol", "grok-4.7"})
            plan = self.graphctl("status", "--run-id", "RUN-1")["execution_plan"]
            self.assertEqual(plan["plan_digest"], initialized["execution_plan_digest"])
            self.assertEqual(plan["status"], "approved")

    def test_legacy_astra_pending_and_approved_plans_keep_original_digest(self):
        legacy = reconstruct_execution_plan("RUN-1", self.task(), {"host": "codex-astra"}, "small")
        with patch("graph_engine.cli.build_execution_plan", return_value=legacy):
            initialized = self.initialize(host="codex-astra", approve=False, size="small")
        database = self.store.db_path("albanian-live-translate", "RUN-1")
        with self.store.connect(database) as connection:
            run = connection.execute("SELECT * FROM runs WHERE run_id='RUN-1'").fetchone()
            self.assertEqual(_validate_execution_plan(connection, run, self.task()), legacy)
        self.assertEqual(initialized["execution_plan_digest"], legacy["plan_digest"])
        self.graphctl("record", "plan-approval", "--run-id", "RUN-1", "--plan-digest",
                      legacy["plan_digest"], "--decision", "APPROVE", "--authority-ref",
                      "authority:test", "--op-id", "legacy-approval")
        with self.store.connect(database) as connection:
            run = connection.execute("SELECT * FROM runs WHERE run_id='RUN-1'").fetchone()
            self.assertEqual(_validate_execution_plan(connection, run, self.task()), legacy)

    def test_astra_plan_tampering_rejects_recomputed_hash_and_retained_approval(self):
        initialized = self.initialize(host="codex-astra", size="small")
        original = initialized["execution_plan"]
        mutations = []
        for field, value in (("model", "gpt-5.6-luna"), ("reasoning_effort", "high"),
                             ("intelligence_class", "economy"), ("dispatch_model", "gpt-5.6-luna")):
            changed = copy.deepcopy(original)
            assignment = next(row for row in changed["assignments"] if row["node_key"] == "tech_lead")
            assignment[field] = value
            mutations.append((changed, "EXECUTION_PLAN_STATE_INVALID"))
        for marker in (None, True, 2.0, "2", original["catalog_revision"] + 1):
            changed = copy.deepcopy(original)
            changed["catalog_revision"] = marker
            mutations.append((changed, "EXECUTION_PLAN_STATE_INVALID"))
        downgraded = copy.deepcopy(original)
        downgraded.pop("catalog_revision")
        mutations.append((downgraded, "EXECUTION_PLAN_STATE_INVALID"))
        legacy = reconstruct_execution_plan("RUN-1", self.task(), {"host": "codex-astra"}, "small")
        mutations.append((legacy, "EXECUTION_PLAN_APPROVAL_INVALID"))
        database = self.store.db_path("albanian-live-translate", "RUN-1")
        with self.store.connect(database) as connection:
            run = connection.execute("SELECT * FROM runs WHERE run_id='RUN-1'").fetchone()
            for changed, error in mutations:
                changed.pop("plan_digest", None)
                changed["plan_digest"] = sha256_bytes(canonical_bytes(changed))
                connection.execute("UPDATE execution_plans SET plan_json=?,plan_digest=? WHERE run_id='RUN-1'",
                                   (json.dumps(changed), changed["plan_digest"]))
                with self.assertRaisesRegex(StateError, error):
                    _validate_execution_plan(connection, run, self.task())

    def test_persisted_v2_task_drives_reconstruction_independently_of_plan_schema(self):
        initialized = self.initialize_task(self.task_v2())
        self.assertEqual(initialized["execution_plan"]["schema_version"], 1)
        self.assertEqual(initialized["execution_plan"]["size"], "small")
        policy, policy_snapshot = load_policy(self.repo)
        database = self.store.db_path("albanian-live-translate", "RUN-1")
        skill_root = Path(__file__).resolve().parents[1]
        with self.store.connect(database) as connection:
            run = connection.execute("SELECT * FROM runs WHERE run_id='RUN-1'").fetchone()
            verify_semantic_state(
                connection, run, self.repo, policy_snapshot.digest, policy, skill_root,
                case_sensitive=True,
            )
            verify_resume(
                connection, run, self.repo, policy_snapshot.digest, policy, skill_root,
                case_sensitive=True,
            )

    def test_decision_precedence_is_order_independent(self):
        approve = {"status": "succeeded", "mandatory": 1, "result_json": '{"decision":"APPROVE","findings":[]}', "branch_id": "a"}
        revise = {"status": "succeeded", "mandatory": 1, "result_json": '{"decision":"REVISE","findings":[{"finding_id":"ARCH-001"}]}', "branch_id": "b"}
        self.assertEqual(compute_design_outcome([approve, revise]), compute_design_outcome([revise, approve]))
        self.assertEqual(compute_design_outcome([approve, revise])[0], "REVISE")

    def test_delivery_dominance(self):
        repair = {"status": "succeeded", "mandatory": 1, "result_json": '{"findings":[{"finding_id":"REV-001","disposition":"repair"}]}'}
        redesign = {"status": "succeeded", "mandatory": 1, "result_json": '{"findings":[{"finding_id":"TEST-001","disposition":"redesign"}]}'}
        self.assertEqual(compute_delivery_outcome([repair, redesign])[0], "REDESIGN")
        block = {"status": "succeeded", "mandatory": 1, "result_json": '{"decision":"BLOCK","findings":[{"finding_id":"SEC-001","disposition":"block"}]}' }
        self.assertEqual(compute_delivery_outcome([repair, redesign, block])[0], "BLOCK")

    def test_out_of_scope_findings_do_not_raise_delivery_outcome(self):
        observation = {"finding_id": "REL-001", "disposition": "approve", "scope": "out_of_scope"}
        members = [{
            "status": "succeeded", "mandatory": 1, "branch_id": "a",
            "result_json": json.dumps({"decision": "APPROVE", "findings": [observation]}),
        }]
        self.assertEqual(compute_delivery_outcome(members), ("ACCEPT", ["REL-001"]))
        unrelated_delegation = [{"parent_branch_id": "other", "issues": []}]
        self.assertEqual(
            _delivery_outcome_without_delegated_echoes(members, unrelated_delegation),
            ("ACCEPT", ["REL-001"]),
        )

    def _review_branch(self, stage, node_key, output_kind):
        return {
            "node_key": node_key, "stage": stage, "run_id": "RUN-1", "branch_id": "branch-1",
            "output_contract": {"artifact_kind": output_kind, "artifact_required": False},
        }

    def _review_result(self, output_kind, decision, findings):
        return {
            "schema_version": 1, "run_id": "RUN-1", "branch_id": "branch-1", "status": "succeeded",
            "output_kind": output_kind, "evidence": [], "attempt_id": "attempt-1",
            "claim_digest": "0" * 64, "decision": decision, "findings": findings,
        }

    def test_stored_cross_phase_dispositions_remain_valid_on_reverification(self):
        branch = self._review_branch("design", "architect", "design_review")
        result = self._review_result("design_review", "REVISE", [
            {"finding_id": "ARCH-001", "disposition": "revise"},
            {"finding_id": "ARCH-002", "disposition": "accept"},
        ])
        with self.assertRaisesRegex(ContractError, "PHASE_DISPOSITION_MISMATCH"):
            validate_result_manifest(result, branch)
        validate_result_manifest(result, branch, historical=True)
        consolidation_branch = {
            "node_key": "supervisor_design_consolidation", "run_id": "RUN-1",
            "branch_id": "branch-2", "generation": 0,
        }
        consolidation = {
            "schema_version": 1, "kind": "design_consolidation", "run_id": "RUN-1",
            "join_id": "join-1", "generation": 0, "source_branch_ids": ["branch-1"],
            "finding_dispositions": [{"finding_id": "ARCH-002", "disposition": "accept"}],
            "outcome": "REVISE", "attempt_id": "attempt-2", "claim_digest": "0" * 64,
        }
        validate_result_manifest(consolidation, consolidation_branch)
        consolidation["finding_dispositions"] = [{"finding_id": "ARCH-002", "disposition": "invented"}]
        with self.assertRaisesRegex(ContractError, "disposition:UNKNOWN_VALUE"):
            validate_result_manifest(consolidation, consolidation_branch)

    def test_only_runs_from_earlier_engines_keep_shared_dispositions_on_reverification(self):
        self.assertTrue(_predates_phase_dispositions({"engine_version": "2.7.0"}))
        self.assertFalse(_predates_phase_dispositions({"engine_version": "2.8.0"}))
        self.assertFalse(_predates_phase_dispositions({"engine_version": "2.9.0"}))

    def test_delivery_rejects_in_scope_approve_disposition(self):
        branch = self._review_branch("delivery", "code_reviewer", "delivery_review")
        result = self._review_result("delivery_review", "REVISE", [
            {"finding_id": "REV-001", "disposition": "repair"},
            {"finding_id": "REV-002", "disposition": "approve", "scope": "in_scope"},
        ])
        with self.assertRaisesRegex(ContractError, "PHASE_DISPOSITION_MISMATCH"):
            validate_result_manifest(result, branch)

    def test_unknown_finding_scope_is_rejected(self):
        branch = self._review_branch("design", "architect", "design_review")
        result = self._review_result("design_review", "APPROVE", [
            {"finding_id": "ARCH-001", "disposition": "approve", "scope": "OUT_OF_SCOPE"},
        ])
        with self.assertRaisesRegex(ContractError, "scope:UNKNOWN_VALUE"):
            validate_result_manifest(result, branch)

    def test_duplicate_and_conflicting_finding_ids_are_rejected(self):
        sources = [
            {"status": "succeeded", "mandatory": 1, "branch_id": "a", "result_json": '{"decision":"REVISE","findings":[{"finding_id":"REV-001","disposition":"repair"}]}'},
            {"status": "succeeded", "mandatory": 1, "branch_id": "b", "result_json": '{"decision":"REVISE","findings":[{"finding_id":"REV-001","disposition":"repair"}]}'},
        ]
        manifest = {"kind": "delivery_consolidation", "run_id": "RUN-1", "join_id": "join", "generation": 0, "source_branch_ids": ["a", "b"], "finding_dispositions": [{"finding_id": "REV-001", "disposition": "repair"}], "outcome": "REPAIR"}
        with self.assertRaisesRegex(ContractError, "DUPLICATE_FINDING_ID"):
            validate_consolidation_manifest(manifest, "delivery", "RUN-1", "join", 0, sources)
        sources[1]["result_json"] = '{"decision":"REVISE","findings":[{"finding_id":"REV-001","disposition":"redesign"}]}'
        with self.assertRaisesRegex(ContractError, "CONFLICTING_FINDING_DISPOSITION"):
            validate_consolidation_manifest(manifest, "delivery", "RUN-1", "join", 0, sources)

    def test_persisted_fanout_uses_forwarded_case_policy(self):
        tags = ["security_privacy"]
        self.initialize(tags=tags)
        self.impact("full_delivery", tags)
        tech_lead = self.claim()
        self.success(tech_lead)
        advanced = self.advance("design_inputs")
        fanout_id = advanced["created_fanout_ids"][0]
        status = self.graphctl("status", "--run-id", "RUN-1")
        member_ids = next(
            fanout["member_branch_ids"] for fanout in status["fanouts"]
            if fanout["fanout_id"] == fanout_id
        )
        evidence = self.repo_artifact("finding", "case-policy-evidence")
        manifest = {
            "schema_version": 1,
            "kind": "fanout_assessment",
            "run_id": "RUN-1",
            "fanout_id": fanout_id,
            "members": [
                {
                    "branch_id": branch_id,
                    "resources": {
                        "writable_paths": [{
                            "path": "src/Feature.py" if index == 0 else "src/feature.py",
                            "scope": "exact",
                        }],
                        "mutable_state_refs": [],
                        "exclusive_device_refs": [],
                        "services": [],
                    },
                }
                for index, branch_id in enumerate(member_ids)
            ],
            "dependencies": [],
            "evidence": [evidence],
        }
        manifest_path = self.inbox_manifest(manifest)
        with patch("graph_engine.cli.os.path.normcase", side_effect=lambda value: value):
            self.graphctl(
                "record", "fanout-assessment", "--run-id", "RUN-1",
                "--fanout-id", fanout_id, "--assessment-manifest", str(manifest_path),
                "--authority-ref", "authority:test", "--op-id", "case-policy-assessment",
            )

        policy, policy_snapshot = load_policy(self.repo)
        database = self.store.db_path("albanian-live-translate", "RUN-1")
        skill_root = Path(__file__).resolve().parents[1]
        with self.store.connect(database) as connection:
            run = connection.execute("SELECT * FROM runs WHERE run_id='RUN-1'").fetchone()
            verify_semantic_state(
                connection, run, self.repo, policy_snapshot.digest, policy, skill_root,
                case_sensitive=True,
            )
            verify_resume(
                connection, run, self.repo, policy_snapshot.digest, policy, skill_root,
                case_sensitive=True,
            )
            with self.assertRaisesRegex(StateError, "FANOUT_ASSESSMENT_INVALID"):
                verify_semantic_state(
                    connection, run, self.repo, policy_snapshot.digest, policy, skill_root,
                    case_sensitive=False,
                )
            with self.assertRaisesRegex(StateError, "FANOUT_ASSESSMENT_INVALID"):
                verify_resume(
                    connection, run, self.repo, policy_snapshot.digest, policy, skill_root,
                    case_sensitive=False,
                )
