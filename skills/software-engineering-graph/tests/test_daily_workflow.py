"""Non-publishing daily workflow regression and disposable-repository dry run.

All worker reports are synthetic contract fixtures. No live model, remote CI,
publication, deployment, or schedule is exercised or claimed by this entry point.
"""

import json
import os
import subprocess
import sys
import unittest

from graph_engine.checks import verify_review_source
from graph_engine.config import load_policy
from graph_engine.contracts import ContractError, validate_task_brief
from graph_engine.merge_readiness import assess_merge_readiness
from graph_engine.ids import sha256_bytes
from graph_engine.state import StateError
from tests import test_cli, test_graph_hardening
from tests.test_support import GraphCase


class SuccessorTrace:
    """Real CLI driver; approvals and worker reports are explicitly synthetic."""

    source_paths = ("src/slots.py", "src/test_slots.py")

    def __init__(self, case, run_id, task, reject_design=False):
        self.case, self.run_id, self.task = case, run_id, task
        self.reject_design = reject_design
        self.writer = None
        self.check_ref = None
        self.rejection = None
        self.seen = []

    def call(self, *args):
        return self.case.graphctl(*args, "--run-id", self.run_id)

    def inbox(self, value):
        self.case.counter += 1
        path = self.case.store.inbox_root("albanian-live-translate", self.run_id) / ("trace-%s.json" % self.case.counter)
        path.write_text(json.dumps(value), encoding="utf-8")
        if os.name != "nt":
            os.chmod(path, 0o600)
        return path

    def initialize(self):
        task_path = self.case.repo / ("docs/task-%s.json" % self.run_id)
        task_path.write_text(json.dumps(self.task), encoding="utf-8")
        initialized = self.case.graphctl(
            "--ack-degraded-permissions", "--ack-degraded-durability", "init",
            "--run-id", self.run_id, "--task-brief", str(task_path), "--op-id", "synthetic-init")
        self.call("record", "plan-approval", "--plan-digest", initialized["execution_plan_digest"],
                  "--decision", "APPROVE", "--authority-ref", "authority:synthetic-fixture",
                  "--op-id", "synthetic-plan-approval")
        coverage = self.inbox({"schema_version": 1, "kind": "check_coverage", "checks": [
            {"check_id": "repo-check", "relevant_inputs": list(self.source_paths), "complete": True}]})
        self.call("evidence", "enable", "--contract-version", "2", "--coverage-manifest", str(coverage),
                  "--op-id", "enable-evidence")
        return initialized

    def check(self, executor):
        source = self.writer
        args = ["check", "run", "--check-id", "repo-check",
                "--executor-branch-id", executor["branch_id"], "--executor-attempt-id", executor["attempt_id"],
                "--executor-claim-token", executor["claim_token"], "--source-branch-id", source["branch_id"],
                "--source-attempt-id", source["attempt_id"], "--source-claim-digest", source["claim_digest"],
                "--op-id", "check-" + executor["node_key"]]
        if self.check_ref:
            args += ["--replace-ref", self.check_ref]
        receipt = self.call(*args)
        self.case.assertEqual("PASS", receipt["outcome"])
        self.check_ref = receipt["artifact_ref"]

    def record(self, branch, stage):
        self.seen.append((branch["node_key"], stage))
        writes = [item["target_ref"] for item in branch["effect_capabilities"]
                  if item["effect"] == "filesystem_write"]
        expected = ["repo:" + path for path in self.source_paths] if branch["role"] == "senior_engineer" else []
        self.case.assertEqual(sorted(expected), sorted(writes))
        kind = branch["output_contract"]["artifact_kind"]
        artifact = self.case.repo_artifact(kind)
        if kind == "impact_map":
            manifest = {"schema_version": 1, "task_id": self.task["task_id"], "route_label": "full_delivery",
                        "impact_tags": ["security_privacy"], "evidence_refs": [artifact["ref"]]}
        elif kind in {"design_consolidation", "delivery_consolidation"}:
            collection = next(item["content"] for item in branch["inputs"] if item["kind"] == "collection")
            blocked = self.reject_design and kind == "design_consolidation"
            manifest = {"schema_version": 1, "kind": kind, "run_id": self.run_id,
                        "join_id": collection["join_id"], "generation": branch["generation"],
                        "source_branch_ids": sorted(item["branch_id"] for item in collection["members"]),
                        "finding_dispositions": [{"finding_id": "ARCH-001", "disposition": "block"}] if blocked else [],
                        "outcome": "BLOCK" if blocked else "APPROVE" if kind == "design_consolidation" else "ACCEPT"}
        else:
            blocked = self.reject_design and branch["node_key"] == "architect"
            if blocked:
                report_path = self.case.repo / artifact["ref"].split("#", 1)[0][5:]
                report_path.write_text(json.dumps({
                    "kind": kind, "finding_id": "ARCH-001", "fixture": "synthetic rejected design",
                    "rationale": "A 00:30 slot after a 19:00 opening must use the next calendar day.",
                    "approved_source_files": list(self.source_paths),
                    "source_sha256": {path: sha256_bytes((self.case.repo / path).read_bytes())
                                      for path in self.source_paths},
                }), encoding="utf-8")
                artifact["sha256"] = sha256_bytes(report_path.read_bytes())
                artifact["ref"] = artifact["ref"].split("#", 1)[0] + "#sha256=" + artifact["sha256"]
            manifest = {"schema_version": 1, "run_id": self.run_id, "branch_id": branch["branch_id"],
                        "status": "succeeded", "output_kind": kind, "artifact_ref": artifact,
                        "evidence": [self.case.repo_artifact("finding")] if stage == "research" else [],
                        "findings": [{"finding_id": "ARCH-001", "disposition": "block"}] if blocked else []}
            if branch["output_contract"].get("decision_values"):
                manifest["decision"] = "BLOCK" if blocked else "IMPLEMENTED" if branch["role"] == "senior_engineer" else "APPROVE"
            if blocked:
                self.rejection = artifact
        manifest.update(attempt_id=branch["attempt_id"], claim_digest=branch["claim_digest"])
        path = self.inbox(manifest)
        self.call("record", "branch-result", "--branch-id", branch["branch_id"],
                  "--attempt-id", branch["attempt_id"], "--claim-token", branch["claim_token"],
                  "--result-manifest", str(path), "--op-id", "result-" + branch["branch_id"])

    def drive(self):
        for step in range(80):
            status = self.call("status")
            action = status["next_action"]
            if status["status"] == "blocked":
                return status
            if action["kind"] == "record_fanout_assessment":
                fanout = next(item for item in status["fanouts"] if item["fanout_id"] == action["fanout_id"])
                assessment = self.inbox({"schema_version": 1, "kind": "fanout_assessment", "run_id": self.run_id,
                    "fanout_id": fanout["fanout_id"], "members": [{"branch_id": branch_id, "resources": {
                        "writable_paths": [], "mutable_state_refs": [], "exclusive_device_refs": [], "services": []}}
                        for branch_id in fanout["member_branch_ids"]], "dependencies": [],
                    "evidence": [self.case.repo_artifact("finding")]})
                self.call("record", "fanout-assessment", "--fanout-id", fanout["fanout_id"],
                          "--assessment-manifest", str(assessment), "--authority-ref", "authority:synthetic-fixture",
                          "--op-id", "assessment-%s" % step)
            elif action["kind"] == "claim":
                branch = self.call("next", "--claim", "--op-id", "claim-%s" % step)["branch"]
                if branch["role"] == "senior_engineer":
                    self.writer = branch
                    (self.case.repo / self.source_paths[0]).write_text(
                        "from datetime import date, timedelta\n"
                        "def slot_date(service_date, opening, slot):\n"
                        "    return date.fromisoformat(service_date) + timedelta(days=slot < opening)\n", encoding="utf-8")
                    (self.case.repo / self.source_paths[1]).write_text(
                        "import unittest\nfrom slots import slot_date\n"
                        "class SlotsTests(unittest.TestCase):\n"
                        "    def test_late_slot_moves_to_next_day(self):\n"
                        "        self.assertEqual('2026-05-09', str(slot_date('2026-05-08', '19:00', '00:30')))\n"
                        "    def test_evening_slot_stays_on_service_day(self):\n"
                        "        self.assertEqual('2026-05-08', str(slot_date('2026-05-08', '19:00', '20:00')))\n", encoding="utf-8")
                    subprocess.run(["git", "add", "--", *self.source_paths], cwd=self.case.repo, check=True, capture_output=True)
                    subprocess.run(["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                                    "commit", "-m", "synthetic approved successor"], cwd=self.case.repo, check=True, capture_output=True)
                    self.check(branch)
                elif branch["node_key"] == "test_engineer":
                    self.check(branch)
                stage = next(item["stage"] for item in status["branches"] if item["branch_id"] == branch["branch_id"])
                self.record(branch, stage)
            elif action["kind"] == "advance_join":
                join = next(item for item in status["joins"] if item["join_id"] == action["join_id"])
                if join["join_key"] == "closure":
                    acceptance = self.case.repo_artifact("acceptance_evidence")
                    self.call("record", "acceptance-evidence", "--criterion-id", "AC-001",
                              "--artifact-ref", acceptance["ref"], "--artifact-sha256", acceptance["sha256"], "--op-id", "acceptance")
                    self.call("complete", "--op-id", "complete")
                    return self.call("status")
                self.call("join", "advance", "--join-id", join["join_id"], "--op-id", "join-%s" % step)
            else:
                self.case.fail("Unexpected trace action: %s" % action)
        self.case.fail("Successor trace exceeded its bounded step budget")


class DailyWorkflowTests(GraphCase):
    # Reuse workflow drivers without inheriting their unrelated test inventories.
    _design_to_reviews = test_cli.CliGoldenTraceTests._design_to_reviews
    _record_design_review = test_cli.CliGoldenTraceTests._record_design_review
    _assert_repair_packet_schema = test_graph_hardening.GraphHardeningTests._assert_repair_packet_schema

    def test_terminal_rejection_approved_narrow_successor_completes(self):
        policy_path = self.repo / ".codex/engineering-graph.json"
        policy = json.loads(policy_path.read_text(encoding="utf-8"))
        policy["required_checks"]["repo-check"]["argv"] = [
            sys.executable, "-m", "unittest", "discover", "-s", "src", "-p", "test_slots.py"]
        policy_path.write_text(json.dumps(policy), encoding="utf-8")
        self.policy_bytes = policy_path.read_bytes()
        policy_digest = sha256_bytes(self.policy_bytes)
        (self.repo / "src").mkdir()
        (self.repo / "src/slots.py").write_text(
            "from datetime import date\ndef slot_date(service_date, opening, slot):\n"
            "    return date.fromisoformat(service_date)\n", encoding="utf-8")
        (self.repo / "src/test_slots.py").write_text("import unittest\n", encoding="utf-8")
        for arguments in (("init",), ("add", "--", *SuccessorTrace.source_paths),
                          ("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-m", "seed")):
            subprocess.run(["git", *arguments], cwd=self.repo, check=True, capture_output=True)
        task = self.task(tags=["security_privacy"])
        task["objective"] = "Synthetic fixture: correct overnight slot calendar dates"
        task["scope"] = {"included": ["repo:" + path for path in SuccessorTrace.source_paths], "excluded": []}
        task["policy_approval"]["authority_ref"] = "authority:synthetic-fixture"
        task["authority"]["capabilities"] = [
            {"effect": effect, "action": action, "target_ref": "repo:" + path}
            for effect, action in (("filesystem_read", "read"), ("filesystem_write", "edit"))
            for path in SuccessorTrace.source_paths
        ] + [{"effect": "command", "action": "run", "target_ref": "npm-run-check"}]
        original = SuccessorTrace(self, "RUN-1", task, reject_design=True)
        original.initialize()
        rejected = original.drive()
        self.assertEqual(("blocked", "CONSOLIDATION_BLOCK"), (rejected["status"], rejected["blocked_reason"]))
        self.assertNotIn(("senior_engineer", "implementation"), original.seen)
        self.assertTrue(original.rejection)
        rejected_revision = rejected["state_revision"]
        original_evidence = (self.repo / original.rejection["ref"].split("#", 1)[0][5:]).read_bytes()
        with self.assertRaisesRegex(StateError, "GRAPH_BLOCKED"):
            original.call("next", "--claim", "--op-id", "forbidden-reopen")

        successor_task = json.loads(json.dumps(task))
        successor_task["task_id"] = "TASK-SUCCESSOR"
        successor_task["objective"] = "Synthetic approved successor: late slots use next-day dates"
        successor_task["evidence_paths"].append(original.rejection["ref"].split("#", 1)[0])
        successor_task["constraints"].append("Preserve RUN-1 rejection; resolve ARCH-001 in this separately approved successor.")
        successor = SuccessorTrace(self, "RUN-2", successor_task)
        successor.initialize()
        completed = successor.drive()
        self.assertEqual(("complete", 7, "approved"),
                         (completed["status"], completed["state_schema_version"], completed["execution_plan"]["status"]))
        self.assertEqual(policy_digest, completed["policy_digest"])
        self.assertNotEqual(rejected["execution_plan"]["plan_digest"], completed["execution_plan"]["plan_digest"])
        self.assertEqual(self.policy_bytes, policy_path.read_bytes())
        self.assertEqual(task["authority"], successor_task["authority"])
        expected_gates = {("architect", "design"), ("security_reviewer", "design"),
                          ("code_reviewer", "delivery"), ("code_reviewer_architecture", "delivery"),
                          ("code_reviewer_naming", "delivery"), ("code_reviewer_bug_hunter", "delivery"),
                          ("test_engineer", "delivery"), ("security_reviewer", "delivery")}
        self.assertTrue(expected_gates.issubset(set(successor.seen)))
        self.assertTrue(all(item["status"] == "succeeded" and item["generation"] == 0
                            for item in completed["branches"]))
        self.assertEqual(1, len(completed["acceptance_evidence"]))
        self.assertTrue(all(item["outcome"] == "PASS" for item in completed["check_evidence"]))
        self.assertTrue(all(item["eligibility"] == "eligible"
                            for item in completed["evidence_lifecycle"]["checks"]))
        preserved = original.call("status")
        self.assertEqual(("blocked", rejected_revision), (preserved["status"], preserved["state_revision"]))
        self.assertEqual(original_evidence, (self.repo / original.rejection["ref"].split("#", 1)[0][5:]).read_bytes())
        for budget in rejected["budgets"]:
            if budget["budget_id"] in {"design_revisions", "delivery_repairs"}:
                self.assertEqual(0, budget["used"])
                self.assertEqual(budget, next(item for item in completed["budgets"] if item["budget_id"] == budget["budget_id"]))
        with self.store.open_run("albanian-live-translate", "RUN-2") as connection:
            run = connection.execute("SELECT * FROM runs WHERE run_id='RUN-2'").fetchone()
            verify_review_source(connection, run, policy)
            self.assertEqual(sorted(task["authority"]["capabilities"], key=lambda item: (
                item["effect"], item["action"], item["target_ref"])),
                json.loads(run["task_json"])["authority"]["capabilities"])
        head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=self.repo, check=True,
                              capture_output=True, text=True).stdout.strip()
        readiness = assess_merge_readiness(
            head_sha=head, reviewed_sha=head, ci_head_sha=head, required_check_ids=["repo-check"],
            checks={"repo-check": "success"}, review_decision="APPROVED", unresolved_review_count=0,
            graph_status=completed["status"], dry_run=True)
        self.assertEqual("DRY_RUN_READY", readiness["decision"])
        self.assertFalse(readiness["merge_authorized"])
        self.assertFalse(readiness["deploy_authorized"])
        print("DAILY_SUCCESSOR_TRACE " + json.dumps({
            "synthetic_approvals_and_reviews": True, "original": {"status": rejected["status"],
                "reason": rejected["blocked_reason"], "preserved_revision": rejected_revision,
                "rejection_sha256": original.rejection["sha256"]},
            "successor": {"run_id": "RUN-2", "approval": completed["execution_plan"]["status"],
                "plan_digest": completed["execution_plan"]["plan_digest"],
                "policy_sha256": policy_digest, "file_grants": task["authority"]["capabilities"],
                "fresh_gates": sorted(expected_gates), "generation": 0,
                "local_checks": [{"id": item["check_id"], "outcome": item["outcome"]}
                                 for item in completed["check_evidence"]],
                "acceptance_count": len(completed["acceptance_evidence"]), "status": completed["status"]},
            "source_sha256": {path: sha256_bytes((self.repo / path).read_bytes()) for path in SuccessorTrace.source_paths},
            "merge_readiness": readiness}, sort_keys=True))

    def test_terminal_design_rejection_reports_original_gate_without_reopening(self):
        self._design_to_reviews()
        self._record_design_review("BLOCK", "SEC-001")
        self.consolidation("design", "BLOCK", dispositions=[
            {"finding_id": "SEC-001", "disposition": "block"}])
        self.advance("design_consolidation")
        before = self.graphctl("status", "--run-id", "RUN-1")
        recovery = self.graphctl("recovery", "show", "--run-id", "RUN-1")
        guidance = recovery["blocked_recovery"]
        self.assertEqual([], recovery["packets"])
        self.assertEqual("new_plan_required", guidance["next_step"])
        self.assertEqual("CONSOLIDATION_BLOCK", guidance["blocked_reason"])
        architect = next(item for item in guidance["origins"] if item["gate"] == "architect")
        self.assertEqual([{"finding_id": "SEC-001", "disposition": "block"}], architect["findings"])
        self.assertTrue(architect["result_ref"]["ref"].startswith("ledger:"))
        self.assertEqual(guidance, before["next_action"]["recovery"])
        with self.assertRaisesRegex(StateError, "GRAPH_BLOCKED"):
            self.graphctl("next", "--run-id", "RUN-1", "--claim", "--op-id", "unsafe-retry")
        after = self.graphctl("status", "--run-id", "RUN-1")
        self.assertEqual((before["state_revision"], "blocked"), (after["state_revision"], after["status"]))
        self.assertFalse(any(item["role"] == "senior_engineer" for item in after["branches"]))

    def test_scope_text_does_not_grant_two_file_or_broad_write_authority(self):
        policy, policy_snapshot = load_policy(self.repo)
        # Reproduce the reported src-only writer policy in memory, without changing authority.
        for role, capabilities in policy["role_capabilities"].items():
            policy["role_capabilities"][role] = [item for item in capabilities
                if item["effect"] != "filesystem_write" or item["target_ref"] == "repo:src/"]
        task = self.task()
        task["scope"]["included"] = ["repo:src/feature.py", "repo:scripts/rollout.py"]
        approved = validate_task_brief(task, policy_snapshot.digest, policy)
        self.assertEqual(task["authority"], approved["authority"])
        for target in ("repo:scripts/rollout.py", "repo:scripts/", "repo:"):
            with self.subTest(target=target):
                successor = json.loads(json.dumps(task))
                successor["authority"]["capabilities"].append({
                    "effect": "filesystem_write", "action": "edit", "target_ref": target})
                with self.assertRaises(ContractError) as denied:
                    validate_task_brief(successor, policy_snapshot.digest, policy)
                self.assertIn(denied.exception.code, {"POLICY_AUTHORITY_EXCEEDED", "INVALID_STRING"})

    def test_design_revision_returns_through_fresh_design_gates(self):
        self._design_to_reviews()
        self._record_design_review("REVISE", "ARCH-001")
        self.consolidation("design", "REVISE", dispositions=[
            {"finding_id": "ARCH-001", "disposition": "revise"}])
        self.advance("design_consolidation")
        lead = self.claim()
        self.assertEqual(("tech_lead", 1), (lead["node_key"], lead["generation"]))
        self.success(lead)
        self.advance("design_inputs", 1)
        architect = self.claim()
        self.assertEqual(("architect", 1), (architect["node_key"], architect["generation"]))
        self.success(architect, "APPROVE")
        self.advance("design_collection", 1)
        self.consolidation("design", "APPROVE", 1)
        self.advance("design_consolidation", 1)
        writer = self.claim()
        self.assertEqual("senior_engineer", writer["node_key"])
        status = self.graphctl("status", "--run-id", "RUN-1")
        self.assertEqual(1, next(item["used"] for item in status["budgets"]
                                 if item["budget_id"] == "design_revisions"))
        self.assertIsNone(self.graphctl("recovery", "show", "--run-id", "RUN-1")["blocked_recovery"])

    def test_scope_change_during_retry_stays_denied(self):
        test_graph_hardening.GraphHardeningTests.test_format7_retry_packet_retains_failure_and_rejects_scope_expansion(self)

    def test_narrow_retry_cannot_restore_broad_policy_directory(self):
        task = self.task(route="fast_path")
        narrow = {"effect": "filesystem_write", "action": "edit", "target_ref": "repo:src/slots.py"}
        task["authority"]["capabilities"].append(narrow)
        self.initialize_task(task)
        coverage = self.inbox_manifest({"schema_version": 1, "kind": "check_coverage", "checks": [
            {"check_id": "repo-check", "relevant_inputs": ["docs/engineering-graph.md"], "complete": True}]})
        self.graphctl("evidence", "enable", "--run-id", "RUN-1", "--contract-version", "2",
                      "--coverage-manifest", str(coverage), "--op-id", "enable")
        self.impact("fast_path")
        writer = self.claim()
        self.record(writer, {"schema_version": 1, "run_id": "RUN-1", "branch_id": writer["branch_id"],
            "status": "failed", "output_kind": "implementation_handoff", "failure_code": "TOOL_FAILURE",
            "evidence": [self.repo_artifact("failure")]})
        judgment = {"schema_version": 1, "kind": "repair_judgment", "cause": "implementation",
                    "failed_criterion_ids": ["AC-001"], "authorized_scope": [{**narrow, "target_ref": "repo:src/"}]}
        with self.assertRaisesRegex(ContractError, "AUTHORITY_EXCEEDED"):
            self.graphctl("record", "retry", "--run-id", "RUN-1", "--branch-id", writer["branch_id"],
                          "--reason-code", "RETRY", "--repair-manifest", str(self.inbox_manifest(judgment)), "--op-id", "broad-retry")
        judgment["authorized_scope"] = [narrow]
        self.graphctl("record", "retry", "--run-id", "RUN-1", "--branch-id", writer["branch_id"],
                      "--reason-code", "RETRY", "--repair-manifest", str(self.inbox_manifest(judgment)), "--op-id", "narrow-retry")
        retried = self.claim()
        self.assertEqual([narrow], [item for item in retried["effect_capabilities"] if item["effect"] == "filesystem_write"])
        self.assertEqual(1, retried["retry_count"])

    def test_blocked_failure_keeps_no_retry_contract(self):
        test_cli.CliGoldenTraceTests.test_recovery_does_not_advertise_retry_for_blocked_run(self)

    def test_equivalent_review_recovery_preserves_architecture_gate(self):
        test_cli.CliGoldenTraceTests.test_audited_recovery_reopens_equivalent_bug_review_and_preserves_architecture_gate(self)

    def test_disposable_daily_repair_closes_with_checks_and_exact_head_review(self):
        policy_path = self.repo / ".codex/engineering-graph.json"
        policy = json.loads(policy_path.read_text(encoding="utf-8"))
        policy["required_checks"]["repo-check"]["argv"] = [sys.executable, "-c", "pass"]
        policy_path.write_text(json.dumps(policy), encoding="utf-8")
        test_graph_hardening.GraphHardeningTests._exercise_repair_checks(self, stale_reviews=True)
        status = self.graphctl("status", "--run-id", "RUN-1")
        self.assertEqual(("complete", 7), (status["status"], status["state_schema_version"]))
        self.assertEqual({"repo-check", "unit", "integration"},
                         {item["check_id"] for item in status["check_evidence"]})
        self.assertTrue(all(item["eligibility"] == "eligible"
                            for item in status["evidence_lifecycle"]["checks"]))
        policy, _ = load_policy(self.repo)
        with self.store.open_run("albanian-live-translate", "RUN-1") as connection:
            run = connection.execute("SELECT * FROM runs WHERE run_id='RUN-1'").fetchone()
            binding, _ = verify_review_source(connection, run, policy)
            self.assertIsNotNone(binding)
            reviewed_head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=self.repo,
                                           check=True, capture_output=True, text=True).stdout.strip()
            merge_snapshot = {
                "head_sha": reviewed_head, "reviewed_sha": reviewed_head, "ci_head_sha": reviewed_head,
                "required_check_ids": json.loads(run["task_json"])["required_check_ids"],
                "checks": {item["check_id"]: "success" if item["outcome"] == "PASS" else "failure"
                           for item in status["check_evidence"]},
                "review_decision": "APPROVED", "unresolved_review_count": 0,
                "graph_status": status["status"], "dry_run": True,
            }
            readiness = assess_merge_readiness(**merge_snapshot)
            self.assertEqual("DRY_RUN_READY", readiness["decision"])
            self.assertFalse(readiness["merge_authorized"])
            self.assertFalse(readiness["deploy_authorized"])
            # An unchanged tree at a different commit still needs exact-head evidence.
            subprocess.run(["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                            "commit", "--allow-empty", "-m", "simulate successor PR head"],
                           cwd=self.repo, check=True, capture_output=True)
            with self.assertRaisesRegex(StateError, "REVIEW_SOURCE_CHANGED"):
                verify_review_source(connection, run, policy)
            merge_snapshot["head_sha"] = subprocess.run(["git", "rev-parse", "HEAD"], cwd=self.repo,
                check=True, capture_output=True, text=True).stdout.strip()
            self.assertEqual(["REVIEW_HEAD_CHANGED", "CI_HEAD_CHANGED"],
                             assess_merge_readiness(**merge_snapshot)["blockers"])


class MergeReadinessTests(unittest.TestCase):
    def evidence(self):
        return {
            "head_sha": "a" * 40, "reviewed_sha": "a" * 40, "ci_head_sha": "a" * 40,
            "required_check_ids": ["unit", "integration"],
            "checks": {"unit": "success", "integration": "success"},
            "review_decision": "APPROVED", "unresolved_review_count": 0,
            "graph_status": "complete", "dry_run": True,
        }

    def test_matching_snapshot_is_dry_run_only_and_does_not_mutate_evidence(self):
        evidence = self.evidence()
        before = json.loads(json.dumps(evidence))
        result = assess_merge_readiness(**evidence)
        self.assertEqual("DRY_RUN_READY", result["decision"])
        self.assertEqual([], result["blockers"])
        self.assertFalse(result["merge_authorized"])
        self.assertFalse(result["deploy_authorized"])
        self.assertEqual(before, evidence)

    def test_head_change_invalidates_review_and_ci_even_with_identical_tree(self):
        evidence = self.evidence()
        evidence["head_sha"] = "b" * 40
        self.assertEqual(["REVIEW_HEAD_CHANGED", "CI_HEAD_CHANGED"],
                         assess_merge_readiness(**evidence)["blockers"])
        evidence["ci_head_sha"] = evidence["head_sha"]
        self.assertEqual(["REVIEW_HEAD_CHANGED"], assess_merge_readiness(**evidence)["blockers"])
        evidence["reviewed_sha"] = evidence["head_sha"]
        self.assertEqual("DRY_RUN_READY", assess_merge_readiness(**evidence)["decision"])

    def test_required_ci_must_exist_and_pass(self):
        for outcome in (None, "pending", "failure", "cancelled", "skipped", "neutral", "unknown"):
            with self.subTest(outcome=outcome):
                evidence = self.evidence()
                if outcome is None:
                    del evidence["checks"]["integration"]
                else:
                    evidence["checks"]["integration"] = outcome
                result = assess_merge_readiness(**evidence)
                self.assertEqual("BLOCKED", result["decision"])
                self.assertTrue(any(item.endswith(":integration") for item in result["blockers"]))
        evidence = self.evidence()
        evidence["ci_head_sha"] = "c" * 40
        self.assertEqual(["CI_HEAD_CHANGED"], assess_merge_readiness(**evidence)["blockers"])

    def test_review_blocks_and_incomplete_graph_prevent_readiness(self):
        for changes, blocker in (
            ({"review_decision": "CHANGES_REQUESTED"}, "REVIEW_NOT_APPROVED"),
            ({"review_decision": "PENDING"}, "REVIEW_NOT_APPROVED"),
            ({"unresolved_review_count": 1}, "UNRESOLVED_REVIEWS"),
            ({"graph_status": "blocked"}, "GRAPH_INCOMPLETE"),
            ({"required_check_ids": []}, "REQUIRED_CHECKS_UNSPECIFIED"),
            ({"head_sha": "main"}, "INVALID_HEAD_EVIDENCE"),
            ({"dry_run": False}, "LIVE_EXECUTION_UNSUPPORTED"),
        ):
            with self.subTest(changes=changes):
                result = assess_merge_readiness(**{**self.evidence(), **changes})
                self.assertEqual("BLOCKED", result["decision"])
                self.assertIn(blocker, result["blockers"])
                self.assertFalse(result["merge_authorized"])
                self.assertFalse(result["deploy_authorized"])


if __name__ == "__main__":
    unittest.main()
