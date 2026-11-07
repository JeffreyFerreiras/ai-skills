"""Disposable continuation traces with synthetic approvals and role reports."""

import json
import subprocess
import sys
import unittest
from unittest.mock import patch

from graph_engine.state import StateError, StateStore
from graph_engine.ids import sha256_bytes
from tests.test_support import GraphCase


class PlanPreflightTests(GraphCase):
    def test_preview_exposes_scope_effects_checks_and_budgets_without_initializing(self):
        task = self.task_v2()
        task["scope"] = {"included": ["product fix"], "excluded": ["graph redesign"]}
        task["required_human_decisions"] = ["release-decision"]
        write = {"effect": "filesystem_write", "action": "edit", "target_ref": "repo:docs/result.txt"}
        command = {"effect": "command", "action": "run", "target_ref": "npm-run-check"}
        task["authority"]["capabilities"].extend([write, command])
        task_path = self.repo / "docs/task.json"
        task_path.write_text(json.dumps(task), encoding="utf-8")
        before = task_path.read_bytes()
        preview = self.graphctl("plan", "--run-id", "RUN-1", "--task-brief", str(task_path))
        preflight = preview["preflight"]
        self.assertEqual(task["scope"], preflight["task"]["scope"])
        self.assertEqual(task["acceptance_criteria"], preflight["task"]["acceptance_criteria"])
        self.assertEqual(sha256_bytes(before), preflight["task"]["sha256"])
        self.assertEqual(sha256_bytes(self.policy_bytes), preflight["policy_digest"])
        self.assertEqual(preview["execution_plan"]["plan_digest"], preflight["plan_digest"])
        self.assertEqual([write], [cap for cap in preflight["role_capabilities"]["senior_engineer"]
                                   if cap["effect"] == "filesystem_write"])
        self.assertNotIn(write, preflight["role_capabilities"]["code_reviewer"])
        self.assertEqual(["release-decision"], preflight["required_human_decisions"])
        self.assertEqual(task["inspection_budget"], preflight["inspection_budget"])
        self.assertEqual({"design_revisions": 2, "delivery_repairs": 3}, preflight["loop_limits"])
        check = preflight["required_checks"][0]
        self.assertEqual(("repo-check", "npm-run-check"), (check["check_id"], check["command_id"]))
        self.assertIn("test_engineer", check["capable_roles"])
        self.assertNotIn("impact_mapper", check["capable_roles"])
        self.assertFalse(self.store.db_path("albanian-live-translate", "RUN-1").exists())
        self.assertEqual(before, task_path.read_bytes())
        initialized = self.initialize_task(task, approve=False)
        self.assertEqual(preflight["plan_digest"], initialized["execution_plan_digest"])

    def test_unconfigured_required_check_is_rejected_before_plan_or_run(self):
        task = self.task()
        task["required_check_ids"].append("missing-check")
        task_path = self.repo / "docs/task.json"
        task_path.write_text(json.dumps(task), encoding="utf-8")
        with self.assertRaisesRegex(StateError, "UNKNOWN_CHECK"):
            self.graphctl("plan", "--run-id", "RUN-1", "--task-brief", str(task_path))
        with self.assertRaisesRegex(StateError, "UNKNOWN_CHECK"):
            self.initialize_task(task)
        self.assertFalse(self.store.db_path("albanian-live-translate", "RUN-1").exists())

    def test_required_check_without_command_is_rejected_before_initialization(self):
        policy_path = self.repo / ".codex/engineering-graph.json"
        policy = json.loads(policy_path.read_text(encoding="utf-8"))
        policy["required_checks"]["repo-check"].pop("argv")
        policy_path.write_text(json.dumps(policy), encoding="utf-8")
        self.policy_bytes = policy_path.read_bytes()
        with self.assertRaisesRegex(StateError, "CHECK_COMMAND_NOT_CONFIGURED"):
            self.initialize()
        self.assertFalse(self.store.db_path("albanian-live-translate", "RUN-1").exists())


class ContinuationTests(GraphCase):
    def setUp(self):
        super().setUp()
        self.source = self.repo / "docs/source.txt"
        self.source.write_text("pending", encoding="utf-8")
        policy_path = self.repo / ".codex/engineering-graph.json"
        policy = json.loads(policy_path.read_text(encoding="utf-8"))
        policy["required_checks"]["repo-check"]["argv"] = [sys.executable, "-c",
            "from pathlib import Path; assert Path('docs/source.txt').read_text() == 'delivered'"]
        policy_path.write_text(json.dumps(policy), encoding="utf-8")
        self.policy_bytes = policy_path.read_bytes()

    def _resume(self):
        self.store = StateStore(self.runtime_root)
        return self.graphctl("resume", "--run-id", "RUN-1",
                             "--ack-degraded-permissions", "--ack-degraded-durability")

    def _snapshot(self):
        with self.store.open_run("albanian-live-translate", "RUN-1") as connection:
            return tuple(connection.iterdump())

    def _enable_evidence(self):
        for arguments in [("init",), ("add", "."),
                          ("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                           "commit", "-m", "synthetic continuation fixture")]:
            subprocess.run(["git", *arguments], cwd=self.repo, check=True, capture_output=True)
        coverage = self.inbox_manifest({"schema_version": 1, "kind": "check_coverage", "checks": [
            {"check_id": "repo-check", "relevant_inputs": ["docs/source.txt"], "complete": True}]})
        self.graphctl("evidence", "enable", "--run-id", "RUN-1", "--contract-version", "2",
                      "--coverage-manifest", str(coverage), "--op-id", "enable-evidence")

    def _failure(self, branch):
        failure = self.repo_artifact("failure", "interrupted-" + branch["attempt_id"])
        self.record(branch, {"schema_version": 1, "run_id": "RUN-1", "branch_id": branch["branch_id"],
                            "status": "failed", "output_kind": branch["output_contract"]["artifact_kind"],
                            "failure_code": "TOOL_FAILURE", "evidence": [failure]})
        return failure

    def _check(self, executor, source, operation, previous=None):
        args = ["check", "run", "--run-id", "RUN-1", "--check-id", "repo-check", "--op-id", operation,
                "--executor-branch-id", executor["branch_id"], "--executor-attempt-id", executor["attempt_id"],
                "--executor-claim-token", executor["claim_token"], "--source-branch-id", source["branch_id"],
                "--source-attempt-id", source["attempt_id"], "--source-claim-digest", source["claim_digest"]]
        if previous:
            args += ["--replace-ref", previous]
        return self.graphctl(*args)

    def _interrupted_delivery(self, evidence_contract):
        task = self.task()
        task["authority"]["capabilities"] = [
            {"effect": "filesystem_read", "action": "read", "target_ref": "repo:docs/source.txt"},
            {"effect": "filesystem_write", "action": "edit", "target_ref": "repo:docs/source.txt"},
            {"effect": "command", "action": "run", "target_ref": "npm-run-check"},
        ]
        initialized = self.initialize_task(task)
        if evidence_contract:
            self._enable_evidence()
        self.impact("full_delivery")
        tech = self.claim()
        self.success(tech)
        self.advance("design_inputs")
        self.success(self.claim(), "APPROVE")
        self.advance("design_collection")
        self.consolidation("design", "APPROVE")
        self.advance("design_consolidation")
        writer = self.claim()
        self.graphctl("record", "budget-use", "--run-id", "RUN-1", "--budget-id", "file_reads",
                      "--amount", "3", "--source-branch-id", writer["branch_id"], "--op-id", "reads")
        failure = self._failure(writer)
        before = self._snapshot()
        resumed = self._resume()
        self.assertEqual(before, self._snapshot())
        self.assertEqual("retry", resumed["next_action"]["kind"])
        self.assertEqual(writer["branch_id"], resumed["next_action"]["branch_id"])
        self.assertEqual(9, resumed["continuation"]["remaining_budgets"]["file_reads"])
        self.assertEqual(initialized["execution_plan_digest"], resumed["execution_plan"]["plan_digest"])
        failure_ref = resumed["next_action"]["repair_packet"]["failure_result_ref"]
        with self.store.open_run("albanian-live-translate", "RUN-1") as connection:
            preserved_result = connection.execute("SELECT content_json FROM artifacts WHERE ref=?", (failure_ref,)).fetchone()[0]
        self.graphctl("record", "retry", "--run-id", "RUN-1", "--branch-id", writer["branch_id"],
                      "--reason-code", "CONTINUE", "--op-id", "retry-writer")
        replacement = self.claim()
        self.assertEqual(writer["branch_id"], replacement["branch_id"])
        self.assertNotEqual(writer["attempt_id"], replacement["attempt_id"])
        self.assertEqual(writer["effect_capabilities"], replacement["effect_capabilities"])
        self.assertEqual(1, replacement["retry_count"])
        self.source.write_text("delivered", encoding="utf-8")
        selected = None
        if evidence_contract:
            selected = self._check(replacement, replacement, "writer-check")["artifact_ref"]
        self.success(replacement, "IMPLEMENTED")
        self.advance("implementation")
        seen = set()
        for _ in range(5):
            reviewer = self.claim()
            seen.add(reviewer["node_key"])
            if evidence_contract and reviewer["node_key"] == "test_engineer":
                selected = self._check(reviewer, replacement, "tester-check", selected)["artifact_ref"]
            self.success(reviewer, "APPROVE")
        self.assertIn("test_engineer", seen)
        self.assertEqual(5, len(seen))
        self.advance("delivery_collection")
        self.consolidation("delivery", "ACCEPT")
        self.advance("delivery_consolidation")
        if not evidence_contract:
            checked = self.graphctl("check", "run", "--run-id", "RUN-1", "--check-id", "repo-check", "--op-id", "check")
            self.assertEqual("PASS", checked["outcome"])
        acceptance = self.repo_artifact("acceptance_evidence")
        self.graphctl("record", "acceptance-evidence", "--run-id", "RUN-1", "--criterion-id", "AC-001",
                      "--artifact-ref", acceptance["ref"], "--artifact-sha256", acceptance["sha256"], "--op-id", "acceptance")
        self.assertEqual("complete", self.graphctl("complete", "--run-id", "RUN-1", "--op-id", "complete")["status"])
        finished = self._resume()
        self.assertEqual("complete", finished["next_action"]["kind"])
        self.assertEqual(resumed["budgets"], finished["budgets"])
        self.assertEqual(resumed["execution_plan"], finished["execution_plan"])
        self.assertEqual(1, sum(branch["node_key"] == "tech_lead" for branch in finished["branches"]))
        with self.store.open_run("albanian-live-translate", "RUN-1") as connection:
            self.assertEqual(preserved_result, connection.execute("SELECT content_json FROM artifacts WHERE ref=?", (failure_ref,)).fetchone()[0])
        path = self.repo / failure["ref"].split("#", 1)[0].removeprefix("repo:")
        self.assertEqual(failure["sha256"], sha256_bytes(path.read_bytes()))
        if evidence_contract:
            self.assertTrue(all(item["eligibility"] == "eligible" for item in finished["evidence_lifecycle"]["checks"]))

    def test_legacy_interruption_completes_without_repeating_design_or_refunding_budget(self):
        self._interrupted_delivery(evidence_contract=False)

    def test_format7_interruption_completes_with_source_bound_checks(self):
        self._interrupted_delivery(evidence_contract=True)

    def test_resume_reconciles_running_review_before_recommending_ready_sibling(self):
        self.initialize(route="fast_path")
        self.impact("fast_path")
        self.success(self.claim(), "IMPLEMENTED")
        self.advance("implementation")
        reviewer = self.claim()
        status = self.graphctl("status", "--run-id", "RUN-1")
        self.assertEqual("claim", status["next_action"]["kind"])
        before = self._snapshot()
        resumed = self._resume()
        self.assertEqual("await_result", resumed["next_action"]["kind"])
        self.assertEqual([reviewer["branch_id"]], resumed["next_action"]["branch_ids"])
        with patch("graph_engine.cli._lease_expired", return_value=True):
            expired = self._resume()
        self.assertEqual({"kind": "timeout_expired", "branch_id": reviewer["branch_id"],
                          "attempt_id": reviewer["attempt_id"]}, expired["next_action"])
        self.assertEqual(before, self._snapshot())

    def test_resume_preserves_unknown_check_and_its_reservation(self):
        task = self.task(route="fast_path")
        task["authority"]["capabilities"].append({"effect": "command", "action": "run", "target_ref": "npm-run-check"})
        self.initialize_task(task)
        self._enable_evidence()
        self.impact("fast_path")
        writer = self.claim()
        with patch("graph_engine.cli.run_check", side_effect=RuntimeError("interrupted")):
            with self.assertRaisesRegex(RuntimeError, "interrupted"):
                self._check(writer, writer, "interrupted-check")
        before = self._snapshot()
        resumed = self._resume()
        self.assertEqual("check_execution_unknown", resumed["next_action"]["kind"])
        self.assertEqual(1, len(resumed["next_action"]["reservation_ids"]))
        self.assertEqual(before, self._snapshot())
        self._failure(writer)
        before = self._snapshot()
        self.assertEqual(resumed["next_action"], self._resume()["next_action"])
        self.assertEqual(before, self._snapshot())

    def test_resume_reports_stale_check_before_ready_reviews(self):
        task = self.task(route="fast_path")
        task["authority"]["capabilities"] = [
            {"effect": "filesystem_read", "action": "read", "target_ref": "repo:docs/source.txt"},
            {"effect": "command", "action": "run", "target_ref": "npm-run-check"},
        ]
        self.initialize_task(task)
        self._enable_evidence()
        self.impact("fast_path")
        writer = self.claim()
        self.source.write_text("delivered", encoding="utf-8")
        receipt = self._check(writer, writer, "writer-check")
        self.success(writer, "IMPLEMENTED")
        self.advance("implementation")
        fanout = self.graphctl("status", "--run-id", "RUN-1")["next_action"]["fanout_id"]
        self.assess_fanout(fanout)
        self.source.write_text("changed-after-review-boundary", encoding="utf-8")
        before = self._snapshot()
        resumed = self._resume()
        self.assertEqual("replace_check", resumed["next_action"]["kind"])
        self.assertEqual(receipt["artifact_ref"], resumed["next_action"]["replace_ref"])
        self.assertNotEqual("eligible", resumed["next_action"]["eligibility"])
        with self.assertRaisesRegex(StateError, "REVIEW_SOURCE_CHANGED"):
            self.graphctl("next", "--run-id", "RUN-1", "--claim", "--op-id", "stale-source-claim")
        self.assertEqual(before, self._snapshot())

    def test_resume_restores_failed_check_source_before_replacing_receipt(self):
        task = self.task(route="fast_path")
        task["authority"]["capabilities"] = [
            {"effect": "filesystem_read", "action": "read", "target_ref": "repo:docs/source.txt"},
            {"effect": "command", "action": "run", "target_ref": "npm-run-check"},
        ]
        self.initialize_task(task)
        self._enable_evidence()
        self.impact("fast_path")
        writer = self.claim()
        self.source.write_text("delivered", encoding="utf-8")
        receipt = self._check(writer, writer, "writer-check")
        with self.store.open_run("albanian-live-translate", "RUN-1") as connection:
            original = connection.execute("SELECT content_json FROM artifacts WHERE ref=?", (receipt["artifact_ref"],)).fetchone()[0]
        self._failure(writer)
        before = self._snapshot()
        resumed = self._resume()
        self.assertEqual("retry", resumed["next_action"]["kind"])
        self.assertEqual(writer["branch_id"], resumed["next_action"]["branch_id"])
        self.assertEqual(before, self._snapshot())
        self.assertNotEqual("eligible", resumed["evidence_lifecycle"]["checks"][0]["eligibility"])
        self.graphctl("record", "retry", "--run-id", "RUN-1", "--branch-id", writer["branch_id"],
                      "--reason-code", "RESTORE_CHECK_SOURCE", "--op-id", "retry-check-source")
        ready = self._resume()
        self.assertEqual({"kind": "claim", "branch_id": writer["branch_id"]}, ready["next_action"])
        replacement = self.claim()
        checked = self._check(replacement, replacement, "replacement-check", receipt["artifact_ref"])
        self.assertEqual("PASS", checked["outcome"])
        self.assertNotEqual(receipt["artifact_ref"], checked["artifact_ref"])
        self.assertEqual(1, replacement["retry_count"])
        self.assertEqual(resumed["budgets"], ready["budgets"])
        with self.store.open_run("albanian-live-translate", "RUN-1") as connection:
            self.assertEqual(original, connection.execute("SELECT content_json FROM artifacts WHERE ref=?", (receipt["artifact_ref"],)).fetchone()[0])
        self._failure(replacement)
        before = self._snapshot()
        exhausted = self._resume()
        self.assertEqual("supervisor_assessment", exhausted["next_action"]["kind"])
        self.assertEqual(0, exhausted["next_action"]["repair_packet"]["remaining_retries"])
        self.assertEqual(before, self._snapshot())

    def test_resume_reuses_human_decisions_and_retains_the_original_task_reference(self):
        task = self.task()
        task["required_human_decisions"] = ["release", "scope"]
        self.initialize_task(task)
        approval = self.repo_artifact("acceptance_evidence")
        self.graphctl("record", "approval", "--run-id", "RUN-1", "--approval-id", "scope",
                      "--scope-ref", approval["ref"], "--decision", "APPROVE", "--authority-ref", "authority:test",
                      "--artifact-sha256", approval["sha256"], "--op-id", "scope-approval")
        before = self._snapshot()
        resumed = self._resume()
        self.assertEqual(["release"], resumed["continuation"]["pending_human_decisions"])
        original_task = resumed["continuation"]["task"]
        self.assertEqual(sha256_bytes((self.repo / "docs/task.json").read_bytes()), original_task["sha256"])
        self.assertEqual("repo:docs/task.json#sha256=" + original_task["sha256"], original_task["ref"])
        self.assertEqual(before, self._snapshot())

    def test_resume_does_not_reopen_retry_exhaustion_or_refund_inspection(self):
        self.initialize(route="fast_path")
        self.impact("fast_path")
        writer = self.claim()
        self.graphctl("record", "budget-use", "--run-id", "RUN-1", "--budget-id", "file_reads",
                      "--amount", "12", "--source-branch-id", writer["branch_id"], "--op-id", "all-reads")
        for attempt in range(2):
            self._failure(writer)
            self.graphctl("record", "retry", "--run-id", "RUN-1", "--branch-id", writer["branch_id"],
                          "--reason-code", "RETRY", "--op-id", "retry-" + str(attempt))
            if attempt == 0:
                writer = self.claim()
        before = self._snapshot()
        resumed = self._resume()
        self.assertEqual(("blocked", "RETRY_LIMIT"), (resumed["status"], resumed["blocked_reason"]))
        self.assertEqual("blocked", resumed["next_action"]["kind"])
        self.assertEqual(0, resumed["continuation"]["remaining_budgets"]["file_reads"])
        self.assertEqual(before, self._snapshot())


if __name__ == "__main__":
    unittest.main()
