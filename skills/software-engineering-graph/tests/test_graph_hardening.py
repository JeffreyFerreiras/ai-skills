import json
import subprocess
import sys
import unittest
from pathlib import Path

from graph_engine.config import load_policy
from graph_engine.contracts import ContractError, validate_task_brief
from graph_engine.execution import build_execution_plan
from graph_engine.helper_register import HelperRegister, HelperRegisterError, validate_allowance
from graph_engine.ids import canonical_bytes, sha256_bytes
from graph_engine.state import StateError
from graph_engine.validator import compute_timing

from tests.test_support import GraphCase


class GraphHardeningTests(GraphCase):
    def setUp(self):
        super().setUp()
        policy_path = self.repo / ".codex" / "engineering-graph.json"
        policy = json.loads(policy_path.read_text(encoding="utf-8"))
        policy["limits"]["branch_lease_seconds"] = 30
        policy["required_checks"]["repo-check"]["argv"] = [sys.executable, "-c", "import sys; sys.exit(0)"]
        policy_path.write_text(json.dumps(policy), encoding="utf-8")
        self.policy_bytes = policy_path.read_bytes()

    def _record_mapper(self, branch, route="full_delivery", tags=None):
        evidence = self.repo_artifact("finding", "impact-evidence")
        manifest = {
            "schema_version": 1,
            "task_id": "TASK-1",
            "route_label": route,
            "impact_tags": sorted(tags or []),
            "evidence_refs": [evidence["ref"]],
            "attempt_id": branch["attempt_id"],
            "claim_digest": sha256_bytes(branch["claim_token"].encode("utf-8")),
        }
        self.counter += 1
        path = self.inbox_manifest(manifest)
        return self.graphctl(
            "record", "branch-result", "--run-id", "RUN-1", "--branch-id", branch["branch_id"],
            "--attempt-id", branch["attempt_id"], "--claim-token", branch["claim_token"],
            "--result-manifest", str(path), "--op-id", f"mapper-result-{self.counter}",
        )

    def _awaiting_design_fanout(self, tags=None):
        selected = tags or ["security_privacy"]
        self.initialize(tags=selected)
        self._record_mapper(self.claim(), tags=selected)
        self.success(self.claim())
        self.advance("design_inputs")
        return next(
            fanout for fanout in self.graphctl("status", "--run-id", "RUN-1")["fanouts"]
            if fanout["stage"] == "design"
        )

    def _sealed_research(self):
        self.initialize()
        mapper = self.claim_raw()
        self._record_mapper(mapper)
        research = next(
            fanout for fanout in self.graphctl("status", "--run-id", "RUN-1")["fanouts"]
            if fanout["stage"] == "research"
        )
        self.assess_fanout(research["fanout_id"])
        self.success(self.claim_raw())
        self.success(self.claim_raw())
        sealed = self.advance("research_collection")
        return sealed, next(
            branch for branch in self.graphctl("status", "--run-id", "RUN-1")["branches"]
            if branch["node_key"] == "tech_lead"
        )

    def _resource_assessment(self, fanout, dependencies=None, claims=None):
        evidence = self.repo_artifact("finding", "fanout-resource-proof-" + str(self.counter))
        members = []
        for index, branch_id in enumerate(reversed(fanout["member_branch_ids"])):
            default = {
                "writable_paths": [
                    {"path": "src/member-{}/file.py".format(index), "scope": "exact"},
                    {"path": "docs/member-{}".format(index), "scope": "subtree"},
                ],
                "mutable_state_refs": ["state:member-{}".format(index)],
                "exclusive_device_refs": ["device:member-{}".format(index)],
                "services": [{"ref": "service:review", "units": 1, "capacity": len(fanout["member_branch_ids"])}],
            }
            members.append({
                "branch_id": branch_id,
                "resources": (claims or {}).get(branch_id, default),
            })
        return {
            "schema_version": 1, "kind": "fanout_assessment", "run_id": "RUN-1",
            "fanout_id": fanout["fanout_id"], "members": members,
            "dependencies": dependencies or [], "evidence": [evidence],
        }

    def _record_assessment_manifest(self, fanout, manifest, op_id):
        path = self.inbox_manifest(manifest)
        return self.graphctl(
            "record", "fanout-assessment", "--run-id", "RUN-1", "--fanout-id", fanout["fanout_id"],
            "--assessment-manifest", str(path), "--authority-ref", "authority:test", "--op-id", op_id,
        )

    def _assert_failed_assessment_atomic(self, fanout, manifest, code, op_id):
        database = self.store.db_path("albanian-live-translate", "RUN-1")
        with self.store.connect(database) as connection:
            before_revision = connection.execute("SELECT state_revision FROM runs").fetchone()[0]
            before_envelopes = connection.execute(
                "SELECT branch_id,envelope_json,status FROM nodes ORDER BY branch_id"
            ).fetchall()
            before_artifacts = connection.execute(
                "SELECT COUNT(*) FROM artifacts WHERE ref LIKE ?", ("ledger:" + fanout["fanout_id"] + "#%",)
            ).fetchone()[0]
        with self.assertRaisesRegex((StateError, ContractError), code):
            self._record_assessment_manifest(fanout, manifest, op_id)
        with self.store.connect(database) as connection:
            row = connection.execute("SELECT * FROM fanouts WHERE fanout_id=?", (fanout["fanout_id"],)).fetchone()
            self.assertEqual(row["status"], "awaiting")
            self.assertEqual(connection.execute(
                "SELECT COUNT(*) FROM fanout_dependencies WHERE fanout_id=?", (fanout["fanout_id"],)
            ).fetchone()[0], 0)
            self.assertEqual(connection.execute(
                "SELECT COUNT(*) FROM artifacts WHERE ref LIKE ?", ("ledger:" + fanout["fanout_id"] + "#%",)
            ).fetchone()[0], before_artifacts)
            self.assertEqual(connection.execute("SELECT state_revision FROM runs").fetchone()[0], before_revision)
            after_envelopes = connection.execute(
                "SELECT branch_id,envelope_json,status FROM nodes ORDER BY branch_id"
            ).fetchall()
            self.assertEqual([tuple(row) for row in after_envelopes], [tuple(row) for row in before_envelopes])
            self.assertIsNone(connection.execute(
                "SELECT 1 FROM operations WHERE operation_id=?", (op_id,)
            ).fetchone())

    def _assert_corrupt_mutation_rejected(self, code, op_id):
        database = self.store.db_path("albanian-live-translate", "RUN-1")
        with self.store.connect(database) as connection:
            revision = connection.execute("SELECT state_revision FROM runs").fetchone()[0]
        with self.assertRaisesRegex(StateError, code):
            self.graphctl("next", "--run-id", "RUN-1", "--claim", "--op-id", op_id)
        with self.store.connect(database) as connection:
            self.assertEqual(connection.execute("SELECT state_revision FROM runs").fetchone()[0], revision)
            self.assertIsNone(connection.execute(
                "SELECT 1 FROM operations WHERE operation_id=?", (op_id,)
            ).fetchone())

    def test_late_result_from_previous_attempt_is_fenced(self):
        self.initialize()
        first = self.claim()
        timeout_manifest = self.control_manifest("timeout", "WORKER_TIMEOUT", {"branch_id": first["branch_id"]})
        timeout_value = json.loads(timeout_manifest.read_text(encoding="utf-8"))
        timeout_value.update({
            "attempt_id": first["attempt_id"],
            "claim_digest": sha256_bytes(first["claim_token"].encode("utf-8")),
        })
        timeout_manifest.write_text(json.dumps(timeout_value), encoding="utf-8")
        self.graphctl(
            "record", "timeout", "--run-id", "RUN-1", "--branch-id", first["branch_id"],
            "--attempt-id", first["attempt_id"], "--claim-token", first["claim_token"],
            "--reason-code", "WORKER_TIMEOUT", "--evidence-manifest", str(timeout_manifest),
            "--op-id", "timeout-1",
        )
        self.graphctl("record", "retry", "--run-id", "RUN-1", "--branch-id", first["branch_id"], "--reason-code", "RETRY", "--op-id", "retry-1")
        second = self.claim()
        stale = {
            "schema_version": 1, "task_id": "TASK-1", "route_label": "full_delivery",
            "impact_tags": [], "evidence_refs": [], "attempt_id": first["attempt_id"],
            "claim_digest": sha256_bytes(first["claim_token"].encode("utf-8")),
        }
        path = self.inbox_manifest(stale)
        with self.assertRaisesRegex(StateError, "ATTEMPT_FENCE_MISMATCH"):
            self.graphctl(
                "record", "branch-result", "--run-id", "RUN-1", "--branch-id", second["branch_id"],
                "--attempt-id", first["attempt_id"], "--claim-token", first["claim_token"],
                "--result-manifest", str(path), "--op-id", "stale-result-1",
            )

    def test_heartbeat_and_expired_lease_are_visible(self):
        self.initialize()
        branch = self.claim()
        heartbeat = self.graphctl(
            "record", "heartbeat", "--run-id", "RUN-1", "--branch-id", branch["branch_id"],
            "--attempt-id", branch["attempt_id"], "--claim-token", branch["claim_token"], "--op-id", "heartbeat-1",
        )
        self.assertEqual(heartbeat["code"], "HEARTBEAT_RECORDED")
        database = self.store.db_path("albanian-live-translate", "RUN-1")
        with self.store.connect(database) as connection:
            envelope = json.loads(connection.execute("SELECT envelope_json FROM nodes WHERE branch_id=?", (branch["branch_id"],)).fetchone()[0])
            envelope["lease_expires_at"] = "2000-01-01T00:00:00Z"
            connection.execute("UPDATE nodes SET envelope_json=? WHERE branch_id=?", (json.dumps(envelope), branch["branch_id"]))
            connection.commit()
        status = self.graphctl("status", "--run-id", "RUN-1")
        self.assertEqual(status["next_action"]["kind"], "timeout_expired")

    def test_only_local_check_runner_receipts_can_satisfy_checks(self):
        self.initialize()
        self._record_mapper(self.claim())
        receipt = self.graphctl("check", "run", "--run-id", "RUN-1", "--check-id", "repo-check", "--op-id", "check-1")
        self.assertEqual(receipt["code"], "CHECK_RECORDED")
        database = self.store.db_path("albanian-live-translate", "RUN-1")
        with self.store.connect(database) as connection:
            row = connection.execute("SELECT * FROM check_evidence WHERE check_id='repo-check'").fetchone()
            artifact = connection.execute("SELECT * FROM artifacts WHERE ref=?", (row["artifact_ref"],)).fetchone()
            self.assertEqual(artifact["source_type"], "ledger")
            self.assertEqual(json.loads(artifact["content_json"])["kind"], "check_receipt")
        forged = self.repo_artifact("check_evidence", "forged-check")
        with self.assertRaisesRegex(StateError, "CHECK_RECEIPT_REQUIRED"):
            self.graphctl(
                "record", "check-evidence", "--run-id", "RUN-1", "--check-id", "repo-check",
                "--outcome", "PASS", "--artifact-ref", forged["ref"], "--artifact-sha256", forged["sha256"],
                "--op-id", "forged-check-1",
            )

    def test_successful_non_mapper_result_keeps_attempt_provenance(self):
        self.initialize()
        self._record_mapper(self.claim())
        branch = self.claim()
        artifact = self.repo_artifact(branch["output_contract"]["artifact_kind"], "technical-design")
        manifest = {
            "schema_version": 1, "run_id": "RUN-1", "branch_id": branch["branch_id"],
            "status": "succeeded", "output_kind": branch["output_contract"]["artifact_kind"],
            "artifact_ref": artifact, "evidence": [], "findings": [],
            "attempt_id": branch["attempt_id"],
            "claim_digest": sha256_bytes(branch["claim_token"].encode("utf-8")),
        }
        self.counter += 1
        path = self.inbox_manifest(manifest)
        result = self.graphctl(
            "record", "branch-result", "--run-id", "RUN-1", "--branch-id", branch["branch_id"],
            "--attempt-id", branch["attempt_id"], "--claim-token", branch["claim_token"],
            "--result-manifest", str(path), "--op-id", f"design-result-{self.counter}",
        )
        self.assertEqual(result["branch_status"], "succeeded")
        self.assertEqual(self.graphctl("status", "--run-id", "RUN-1")["status"], "active")

    def test_execution_plan_is_visible_and_blocks_claim_until_human_approval(self):
        initialized = self.initialize(size="small", approve=False)
        plan = initialized["execution_plan"]
        self.assertEqual(plan["size"], "small")
        self.assertTrue(plan["approval_required"])
        senior = next(item for item in plan["assignments"] if item["node_key"] == "senior_engineer")
        self.assertEqual((senior["model"], senior["reasoning_effort"]), ("gpt-6-astra", "low"))
        ready = self.graphctl("next", "--run-id", "RUN-1")
        self.assertEqual(ready["code"], "EXECUTION_PLAN_APPROVAL_REQUIRED")
        with self.assertRaisesRegex(StateError, "EXECUTION_PLAN_APPROVAL_REQUIRED"):
            self.claim()
        self.graphctl(
            "record", "plan-approval", "--run-id", "RUN-1",
            "--plan-digest", initialized["execution_plan_digest"], "--decision", "APPROVE",
            "--authority-ref", "authority:test", "--op-id", "plan-approval-1",
        )
        branch = self.claim()
        self.assertEqual((branch["model"], branch["reasoning_effort"]), ("gpt-5.6-luna", "max"))

    def test_rejected_execution_plan_blocks_the_run(self):
        initialized = self.initialize(approve=False)
        result = self.graphctl(
            "record", "plan-approval", "--run-id", "RUN-1",
            "--plan-digest", initialized["execution_plan_digest"], "--decision", "REJECT",
            "--authority-ref", "authority:test", "--op-id", "plan-rejection-1",
        )
        self.assertEqual(result["status"], "blocked")
        status = self.graphctl("status", "--run-id", "RUN-1")
        self.assertEqual(status["execution_plan"]["status"], "rejected")
        with self.assertRaisesRegex(StateError, "GRAPH_BLOCKED"):
            self.claim()

    def test_critical_delivery_is_forced_through_security_full_delivery(self):
        policy, snapshot = load_policy(self.repo)
        task = self.task(route="fast_path")
        task["risk_level"] = "critical"
        with self.assertRaisesRegex(ContractError, "CRITICAL_REQUIRES_FULL_DELIVERY"):
            validate_task_brief(task, snapshot.digest, policy)
        task["minimum_route"] = "full_delivery"
        with self.assertRaisesRegex(ContractError, "CRITICAL_REQUIRES_SECURITY_REVIEW"):
            validate_task_brief(task, snapshot.digest, policy)
        task["mandatory_impact_tags"] = ["security_privacy"]
        validate_task_brief(task, snapshot.digest, policy)

    def test_approval_records_local_actor_attestation(self):
        task = self.task()
        task["required_human_decisions"] = ["human-1"]
        self.initialize_task(task)
        self._record_mapper(self.claim())
        scope = self.repo_artifact("acceptance_evidence", "approval-scope")
        self.graphctl(
            "record", "acceptance-evidence", "--run-id", "RUN-1", "--criterion-id", "AC-001",
            "--artifact-ref", scope["ref"], "--artifact-sha256", scope["sha256"], "--op-id", "acceptance-1",
        )
        self.graphctl(
            "record", "approval", "--run-id", "RUN-1", "--approval-id", "human-1",
            "--scope-ref", scope["ref"], "--decision", "APPROVE", "--authority-ref", "authority:test",
            "--artifact-sha256", scope["sha256"], "--op-id", "approval-1",
        )
        database = self.store.db_path("albanian-live-translate", "RUN-1")
        with self.store.connect(database) as connection:
            self.assertIsNotNone(connection.execute("SELECT 1 FROM approval_attestations WHERE approval_id='human-1'").fetchone())
            connection.execute("DELETE FROM approval_attestations WHERE approval_id='human-1'")
            connection.commit()
        with self.assertRaisesRegex(StateError, "APPROVAL_ATTESTATION_INVALID"):
            self.graphctl("status", "--run-id", "RUN-1")

    def test_whole_join_deletion_is_detected_from_operation_history(self):
        self.initialize(tags=["security_privacy"])
        self._record_mapper(self.claim(), tags=["security_privacy"])
        self.success(self.claim())
        join = self.open_join("design_inputs")
        database = self.store.db_path("albanian-live-translate", "RUN-1")
        with self.store.connect(database) as connection:
            connection.execute("DELETE FROM join_members WHERE join_id=?", (join["join_id"],))
            connection.execute("DELETE FROM joins WHERE join_id=?", (join["join_id"],))
            connection.commit()
        self._assert_corrupt_mutation_rejected("TOPOLOGY_JOIN_INVALID", "deleted-join-claim")

    def test_whole_successor_deletion_is_detected_from_operation_history(self):
        self.initialize()
        self._record_mapper(self.claim())
        branches = self.graphctl("status", "--run-id", "RUN-1")["branches"]
        successor_id = next(
            item["branch_id"] for item in branches
            if item["node_key"] == "design_research_architecture"
        )
        with self.store.connect(self.store.db_path("albanian-live-translate", "RUN-1")) as connection:
            # Research branches are persisted as collection members immediately,
            # so disable SQLite FK enforcement only for this corruption probe.
            connection.execute("PRAGMA foreign_keys=OFF")
            connection.execute("DELETE FROM nodes WHERE branch_id=?", (successor_id,))
            connection.commit()
        self._assert_corrupt_mutation_rejected(
            "TOPOLOGY_NODE_INVALID|JOIN_MEMBERSHIP_INVALID", "deleted-successor-claim"
        )

    def test_research_collection_witness_requires_exact_tech_lead_successor(self):
        sealed, tech_lead = self._sealed_research()
        database = self.store.db_path("albanian-live-translate", "RUN-1")
        with self.store.connect(database) as connection:
            rows = connection.execute("SELECT operation_id,response_json FROM operations").fetchall()
            witness = next(
                row for row in rows
                if json.loads(row["response_json"]).get("join_id") == sealed["join_id"]
            )
            response = json.loads(witness["response_json"])
            response["successor_branch_ids"] = []
            connection.execute(
                "UPDATE operations SET response_json=? WHERE operation_id=?",
                (json.dumps(response, sort_keys=True), witness["operation_id"]),
            )
            connection.commit()
        self._assert_corrupt_mutation_rejected("TOPOLOGY_HISTORY_INVALID", "missing-research-successor")

    def test_research_tech_lead_successor_cannot_appear_on_unrelated_operation(self):
        sealed, tech_lead = self._sealed_research()
        database = self.store.db_path("albanian-live-translate", "RUN-1")
        with self.store.connect(database) as connection:
            rows = connection.execute("SELECT operation_id,response_json FROM operations").fetchall()
            unrelated = next(
                row for row in rows
                if json.loads(row["response_json"]).get("code") == "BRANCH_RESULT_RECORDED"
                and json.loads(row["response_json"]).get("branch_id")
            )
            response = json.loads(unrelated["response_json"])
            response["successor_branch_ids"].append(tech_lead["branch_id"])
            connection.execute(
                "UPDATE operations SET response_json=? WHERE operation_id=?",
                (json.dumps(response, sort_keys=True), unrelated["operation_id"]),
            )
            connection.commit()
        self._assert_corrupt_mutation_rejected("TOPOLOGY_HISTORY_INVALID", "unrelated-research-successor")

    def test_extra_node_and_join_fail_before_mutation(self):
        self.initialize()
        database = self.store.db_path("albanian-live-translate", "RUN-1")
        with self.store.connect(database) as connection:
            source = connection.execute("SELECT * FROM nodes WHERE node_key='impact_mapper'").fetchone()
            envelope = json.loads(source["envelope_json"])
            envelope.update({"branch_id": "extra-branch", "node_instance_id": "extra-node", "generation": 1})
            connection.execute(
                """INSERT INTO nodes(branch_id,run_id,node_instance_id,node_key,role,stage,generation,
                mandatory,specialist_tag,status,retry_count,max_retries,envelope_json)
                VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)""",
                ("extra-branch", "RUN-1", "extra-node", source["node_key"], source["role"],
                 source["stage"], 1, source["mandatory"], None, "ready", 0,
                 source["max_retries"], json.dumps(envelope)),
            )
            connection.commit()
        self._assert_corrupt_mutation_rejected("STABLE_ID_INVALID|TOPOLOGY_NODE_INVALID", "extra-node-claim")

        self.tearDown(); self.setUp()
        self.initialize(); self._record_mapper(self.claim()); self.success(self.claim())
        with self.store.connect(self.store.db_path("albanian-live-translate", "RUN-1")) as connection:
            tech = connection.execute("SELECT * FROM nodes WHERE node_key='tech_lead'").fetchone()
            connection.execute(
                "INSERT INTO joins(join_id,run_id,join_key,kind,stage,generation,status,degraded) VALUES(?,?,?,?,?,?,?,0)",
                ("extra-join", "RUN-1", "closure", "closure", "closure", 99, "open"),
            )
            connection.execute("INSERT INTO join_members VALUES(?,?,1)", ("extra-join", tech["branch_id"]))
            connection.commit()
        self._assert_corrupt_mutation_rejected("STABLE_ID_INVALID|JOIN_MEMBERSHIP_INVALID", "extra-join-claim")

    def test_removed_and_added_join_member_edges_fail_closed(self):
        fanout = self._awaiting_design_fanout(["audio_realtime_translation", "security_privacy"])
        database = self.store.db_path("albanian-live-translate", "RUN-1")
        with self.store.connect(database) as connection:
            collection = connection.execute("SELECT * FROM joins WHERE join_key='design_collection'").fetchone()
            connection.execute(
                "DELETE FROM join_members WHERE join_id=? AND branch_id=?",
                (collection["join_id"], fanout["member_branch_ids"][0]),
            )
            connection.commit()
        self._assert_corrupt_mutation_rejected("JOIN_MEMBERSHIP_INVALID", "removed-member-claim")

        self.tearDown(); self.setUp()
        self._awaiting_design_fanout(["audio_realtime_translation", "security_privacy"])
        with self.store.connect(self.store.db_path("albanian-live-translate", "RUN-1")) as connection:
            collection = connection.execute("SELECT * FROM joins WHERE join_key='design_collection'").fetchone()
            producer = connection.execute("SELECT * FROM nodes WHERE node_key='tech_lead'").fetchone()
            connection.execute(
                "INSERT INTO join_members VALUES(?,?,1)", (collection["join_id"], producer["branch_id"])
            )
            connection.commit()
        self._assert_corrupt_mutation_rejected("JOIN_MEMBERSHIP_INVALID", "added-member-claim")

    def test_assessment_envelope_and_dependency_mismatches_fail_closed(self):
        fanout = self._awaiting_design_fanout()
        self._record_assessment_manifest(fanout, self._resource_assessment(fanout), "envelope-assessment")
        database = self.store.db_path("albanian-live-translate", "RUN-1")
        with self.store.connect(database) as connection:
            assessed = connection.execute("SELECT * FROM fanouts WHERE fanout_id=?", (fanout["fanout_id"],)).fetchone()
            branch_id = fanout["member_branch_ids"][0]
            envelope = json.loads(connection.execute(
                "SELECT envelope_json FROM nodes WHERE branch_id=?", (branch_id,)
            ).fetchone()[0])
            envelope["inputs"] = [item for item in envelope["inputs"] if item["ref"] != assessed["assessment_ref"]]
            connection.execute("UPDATE nodes SET envelope_json=? WHERE branch_id=?", (json.dumps(envelope), branch_id))
            connection.commit()
        self._assert_corrupt_mutation_rejected("FANOUT_ASSESSMENT_INVALID", "envelope-mismatch-claim")

        self.tearDown(); self.setUp()
        fanout = self._awaiting_design_fanout()
        self._record_assessment_manifest(fanout, self._resource_assessment(fanout), "provenance-assessment")
        with self.store.connect(self.store.db_path("albanian-live-translate", "RUN-1")) as connection:
            connection.execute("DROP TRIGGER fanout_transition_guard")
            connection.execute(
                "UPDATE fanouts SET actor='tampered-actor' WHERE fanout_id=?", (fanout["fanout_id"],)
            )
            connection.commit()
        self._assert_corrupt_mutation_rejected(
            "FANOUT_ASSESSMENT_INVALID", "provenance-mismatch-claim"
        )

        self.tearDown(); self.setUp()
        fanout = self._awaiting_design_fanout()
        before, after = fanout["member_branch_ids"]
        dependencies = [{"before_branch_id": before, "after_branch_id": after, "reason": "ordered"}]
        self._record_assessment_manifest(
            fanout, self._resource_assessment(fanout, dependencies=dependencies), "dependency-assessment"
        )
        with self.store.connect(self.store.db_path("albanian-live-translate", "RUN-1")) as connection:
            connection.execute("DROP TRIGGER fanout_dependency_delete_guard")
            connection.execute(
                "DELETE FROM fanout_dependencies WHERE fanout_id=?", (fanout["fanout_id"],)
            )
            connection.commit()
        self._assert_corrupt_mutation_rejected(
            "FANOUT_DEPENDENCY_STATE_INVALID", "dependency-mismatch-claim"
        )

    def test_fanout_assessment_gates_claim_and_orders_promotion(self):
        self.initialize(tags=["security_privacy"])
        self._record_mapper(self.claim(), tags=["security_privacy"])
        self.success(self.claim())
        self.advance("design_inputs")
        required = self.graphctl("next", "--run-id", "RUN-1")
        self.assertEqual(required["code"], "FANOUT_ASSESSMENT_REQUIRED")
        fanout = next(
            item for item in self.graphctl("status", "--run-id", "RUN-1")["fanouts"]
            if item["stage"] == "design"
        )
        before, after = fanout["member_branch_ids"]
        result = self.assess_fanout(fanout["fanout_id"], [{
            "before_branch_id": before, "after_branch_id": after, "reason": "shared review state",
        }])
        self.assertEqual(result["ready_branch_ids"], [before])
        by_id = {item["branch_id"]: item for item in self.graphctl("status", "--run-id", "RUN-1")["branches"]}
        self.assertEqual((by_id[before]["status"], by_id[after]["status"]), ("ready", "pending"))
        first = self.claim()
        timeout_path = self.control_manifest("timeout", "WORKER_TIMEOUT", first)
        timeout = json.loads(timeout_path.read_text(encoding="utf-8"))
        timeout.update({
            "attempt_id": first["attempt_id"],
            "claim_digest": sha256_bytes(first["claim_token"].encode("utf-8")),
        })
        timeout_path.write_text(json.dumps(timeout), encoding="utf-8")
        self.graphctl(
            "record", "timeout", "--run-id", "RUN-1", "--branch-id", before,
            "--attempt-id", first["attempt_id"], "--claim-token", first["claim_token"],
            "--reason-code", "WORKER_TIMEOUT", "--evidence-manifest", str(timeout_path),
            "--op-id", "ordered-timeout",
        )
        by_id = {item["branch_id"]: item for item in self.graphctl("status", "--run-id", "RUN-1")["branches"]}
        self.assertEqual(by_id[after]["status"], "pending")
        self.graphctl(
            "record", "retry", "--run-id", "RUN-1", "--branch-id", before,
            "--reason-code", "RETRY", "--op-id", "ordered-retry",
        )
        self.success(self.claim())
        by_id = {item["branch_id"]: item for item in self.graphctl("status", "--run-id", "RUN-1")["branches"]}
        self.assertEqual(by_id[after]["status"], "ready")

    def test_fanout_assessment_is_atomic_replayable_and_single_assignment(self):
        self.initialize(tags=["security_privacy"])
        self._record_mapper(self.claim(), tags=["security_privacy"])
        self.success(self.claim())
        self.advance("design_inputs")
        fanout = next(
            item for item in self.graphctl("status", "--run-id", "RUN-1")["fanouts"]
            if item["stage"] == "design"
        )
        evidence = self.repo_artifact("finding", "assessment-proof")

        def manifest(member_ids):
            return {
                "schema_version": 1, "kind": "fanout_assessment", "run_id": "RUN-1",
                "fanout_id": fanout["fanout_id"],
                "members": [{
                    "branch_id": branch_id,
                    "resources": {
                        "writable_paths": [], "mutable_state_refs": [],
                        "exclusive_device_refs": [], "services": [],
                    },
                } for branch_id in member_ids],
                "dependencies": [], "evidence": [evidence],
            }

        invalid_path = self.inbox_manifest(manifest(fanout["member_branch_ids"][:-1]))
        with self.assertRaisesRegex(ContractError, "FANOUT_MEMBER_INVALID"):
            self.graphctl(
                "record", "fanout-assessment", "--run-id", "RUN-1",
                "--fanout-id", fanout["fanout_id"], "--assessment-manifest", str(invalid_path),
                "--authority-ref", "authority:test", "--op-id", "assessment-invalid",
            )
        self.assertEqual(
            next(item for item in self.graphctl("status", "--run-id", "RUN-1")["fanouts"]
                 if item["fanout_id"] == fanout["fanout_id"])["status"],
            "awaiting",
        )

        valid_path = self.inbox_manifest(manifest(fanout["member_branch_ids"]))
        args = (
            "record", "fanout-assessment", "--run-id", "RUN-1", "--fanout-id", fanout["fanout_id"],
            "--assessment-manifest", str(valid_path), "--authority-ref", "authority:test",
            "--op-id", "assessment-valid",
        )
        recorded = self.graphctl(*args)
        replay = self.graphctl(*args)
        self.assertEqual((recorded["code"], replay["code"]), ("FANOUT_ASSESSMENT_RECORDED", "REPLAYED"))
        with self.assertRaisesRegex(StateError, "FANOUT_ASSESSMENT_EXISTS"):
            self.graphctl(
                "record", "fanout-assessment", "--run-id", "RUN-1",
                "--fanout-id", fanout["fanout_id"], "--assessment-manifest", str(valid_path),
                "--authority-ref", "authority:test", "--op-id", "assessment-replacement",
            )

    def test_retained_schemas_fail_closed_without_mutation(self):
        self.initialize()
        database = self.store.db_path("albanian-live-translate", "RUN-1")
        for schema_version in (2, 3, 4):
            with self.subTest(schema_version=schema_version):
                with self.store.connect(database) as connection:
                    connection.execute("UPDATE runs SET state_schema_version=?", (schema_version,))
                    connection.commit()
                    revision = connection.execute("SELECT state_revision FROM runs").fetchone()[0]
                    operation_count = connection.execute("SELECT COUNT(*) FROM operations").fetchone()[0]
                with self.assertRaisesRegex(StateError, "UNSUPPORTED_STATE_SCHEMA"):
                    self.graphctl(
                        "next", "--run-id", "RUN-1", "--claim",
                        "--op-id", "unsupported-schema-{}".format(schema_version),
                    )
                with self.store.connect(database) as connection:
                    current = connection.execute(
                        "SELECT state_schema_version,state_revision FROM runs"
                    ).fetchone()
                    self.assertEqual(tuple(current), (schema_version, revision))
                    self.assertEqual(connection.execute("SELECT COUNT(*) FROM operations").fetchone()[0], operation_count)

    def test_real_resource_assessment_is_canonical_and_promotes_three_roots(self):
        fanout = self._awaiting_design_fanout(["audio_realtime_translation", "security_privacy"])
        self.assertEqual(len(fanout["member_branch_ids"]), 3)
        result = self._record_assessment_manifest(
            fanout, self._resource_assessment(fanout), "real-resource-assessment"
        )
        self.assertEqual(result["ready_branch_ids"], fanout["member_branch_ids"])
        database = self.store.db_path("albanian-live-translate", "RUN-1")
        with self.store.connect(database) as connection:
            persisted = connection.execute(
                "SELECT * FROM fanouts WHERE fanout_id=?", (fanout["fanout_id"],)
            ).fetchone()
            artifact = connection.execute(
                "SELECT * FROM artifacts WHERE ref=?", (persisted["assessment_ref"],)
            ).fetchone()
            content = json.loads(artifact["content_json"])
            self.assertEqual(
                [member["branch_id"] for member in content["members"]],
                sorted(fanout["member_branch_ids"]),
            )
            self.assertTrue(all(
                member["resources"][category]
                for member in content["members"]
                for category in ("writable_paths", "mutable_state_refs", "exclusive_device_refs", "services")
            ))
            self.assertEqual(connection.execute(
                "SELECT COUNT(*) FROM nodes WHERE status='ready' AND branch_id IN ({})".format(
                    ",".join("?" for _ in fanout["member_branch_ids"])
                ), fanout["member_branch_ids"],
            ).fetchone()[0], 3)
            assessment_ref = persisted["assessment_ref"]
            for branch_id in fanout["member_branch_ids"]:
                envelope = json.loads(connection.execute(
                    "SELECT envelope_json FROM nodes WHERE branch_id=?", (branch_id,)
                ).fetchone()[0])
                self.assertEqual(sum(item["ref"] == assessment_ref for item in envelope["inputs"]), 1)

    def test_invalid_resource_assessments_are_atomic(self):
        fanout = self._awaiting_design_fanout(["audio_realtime_translation", "security_privacy"])
        member_ids = fanout["member_branch_ids"]

        def unique_claims():
            return {
                branch_id: {
                    "writable_paths": [{"path": "src/{}.py".format(index), "scope": "exact"}],
                    "mutable_state_refs": ["state:{}".format(index)],
                    "exclusive_device_refs": ["device:{}".format(index)],
                    "services": [{"ref": "service:review", "units": 1, "capacity": 3}],
                }
                for index, branch_id in enumerate(member_ids)
            }

        for category in ("file", "state", "device"):
            claims = unique_claims()
            if category == "file":
                claims[member_ids[0]]["writable_paths"] = [{"path": "src/shared", "scope": "subtree"}]
                claims[member_ids[1]]["writable_paths"] = [{"path": "src/shared/file.py", "scope": "exact"}]
            elif category == "state":
                claims[member_ids[0]]["mutable_state_refs"] = ["state:shared"]
                claims[member_ids[1]]["mutable_state_refs"] = ["state:shared"]
            else:
                claims[member_ids[0]]["exclusive_device_refs"] = ["device:shared"]
                claims[member_ids[1]]["exclusive_device_refs"] = ["device:shared"]
            self._assert_failed_assessment_atomic(
                fanout, self._resource_assessment(fanout, claims=claims),
                "FANOUT_UNORDERED_CONFLICT", "unordered-" + category,
            )

        inconsistent = unique_claims()
        inconsistent[member_ids[0]]["services"][0]["capacity"] = 2
        self._assert_failed_assessment_atomic(
            fanout, self._resource_assessment(fanout, claims=inconsistent),
            "FANOUT_CAPACITY_INVALID", "inconsistent-capacity",
        )
        over_capacity = unique_claims()
        for resources in over_capacity.values():
            resources["services"][0]["capacity"] = 2
        self._assert_failed_assessment_atomic(
            fanout, self._resource_assessment(fanout, claims=over_capacity),
            "FANOUT_CAPACITY_EXCEEDED", "over-capacity",
        )
        cycle = [
            {"before_branch_id": member_ids[0], "after_branch_id": member_ids[1], "reason": "first"},
            {"before_branch_id": member_ids[1], "after_branch_id": member_ids[0], "reason": "cycle"},
        ]
        self._assert_failed_assessment_atomic(
            fanout, self._resource_assessment(fanout, dependencies=cycle),
            "FANOUT_CYCLE", "cycle-assessment",
        )

    def test_mandatory_ready_branch_cannot_be_skipped(self):
        fanout = self._awaiting_design_fanout()
        self._record_assessment_manifest(fanout, self._resource_assessment(fanout), "skip-assessment")
        branch_id = fanout["member_branch_ids"][0]
        status = self.graphctl("status", "--run-id", "RUN-1")
        branch = next(item for item in status["branches"] if item["branch_id"] == branch_id)
        self.assertEqual(branch["status"], "ready")
        manifest = self.control_manifest("skip", "NOT_APPLICABLE", branch)
        revision = status["state_revision"]
        with self.assertRaisesRegex(StateError, "INVALID_BRANCH_TRANSITION"):
            self.graphctl(
                "record", "skip", "--run-id", "RUN-1", "--branch-id", branch_id,
                "--reason-code", "NOT_APPLICABLE", "--evidence-manifest", str(manifest),
                "--op-id", "mandatory-skip",
            )
        self.assertEqual(self.graphctl("status", "--run-id", "RUN-1")["state_revision"], revision)

    def test_status_returns_incomplete_and_completed_attempt_timing(self):
        self.initialize()
        mapper = self.claim()
        running = self.graphctl("status", "--run-id", "RUN-1")
        mapper_status = next(item for item in running["branches"] if item["branch_id"] == mapper["branch_id"])
        self.assertEqual(mapper_status["attempt_count"], 1)
        self.assertFalse(mapper_status["timing_complete"])
        self.assertIsNone(mapper_status["wall_time_ms"])
        self.assertIsNone(mapper_status["active_duration_ms"])
        self.assertFalse(running["timing"]["overall"]["timing_complete"])
        self.assertIsNone(running["timing"]["overall"]["critical_path"])
        self._record_mapper(mapper)
        completed = self.graphctl("status", "--run-id", "RUN-1")
        mapper_status = next(item for item in completed["branches"] if item["branch_id"] == mapper["branch_id"])
        self.assertTrue(mapper_status["timing_complete"])
        self.assertEqual(mapper_status["attempt_count"], 1)
        self.assertIsNotNone(mapper_status["active_duration_ms"])
        bootstrap = next(item for item in completed["timing"]["stages"] if item["stage"] == "bootstrap")
        self.assertTrue(bootstrap["timing_complete"])

    def test_reversed_attempt_timestamp_fails_semantic_status_without_mutation(self):
        self.initialize()
        mapper = self.claim()
        self._record_mapper(mapper)
        database = self.store.db_path("albanian-live-translate", "RUN-1")
        with self.store.connect(database) as connection:
            connection.execute("DROP TRIGGER branch_attempt_identity_guard")
            connection.execute(
                "UPDATE branch_attempts SET finished_at='2000-01-01T00:00:00Z' WHERE branch_id=?",
                (mapper["branch_id"],),
            )
            connection.commit()
            revision = connection.execute("SELECT state_revision FROM runs").fetchone()[0]
        with self.assertRaisesRegex(StateError, "ATTEMPT_TIMESTAMP_INVALID"):
            self.graphctl("status", "--run-id", "RUN-1")
        with self.store.connect(database) as connection:
            self.assertEqual(connection.execute("SELECT state_revision FROM runs").fetchone()[0], revision)


class HelperRegisterTests(GraphCase):
    def _assignment(
        self, assignment_id, parent_role, helper_role, commands,
        model="gpt-5.6-luna", reasoning_effort="max",
    ):
        required = [
            "fresh_model_effort_selection", "filesystem_confinement", "tool_confinement",
        ]
        if helper_role == "validation_executor":
            required.append("command_confinement")
        parent_capabilities = [
            {"effect": "filesystem_read", "action": "read", "target_ref": "repo:docs/"},
        ]
        parent_capabilities.extend(
            {"effect": "command", "action": "run", "target_ref": item["command_id"]}
            for item in commands
        )
        return {
            "assignment_id": assignment_id, "parent_role": parent_role,
            "helper_role": helper_role, "contract_revision": 1,
            "model": model, "reasoning_effort": reasoning_effort,
            "parent_capabilities": parent_capabilities,
            "scope_refs": ["repo:docs/"], "commands": commands,
            "checkpoint_policy": "observed_repository_state",
            "resource_keys": ["worktree"],
            "required_host_capabilities": required,
            "limits": {
                "children": 2, "concurrency": 1, "commands": 2,
                "time_seconds": 240, "output_tokens": 4000, "file_reads": 12,
            },
        }

    def _materials(
        self, unsupported=False, model="gpt-5.6-luna", reasoning_effort="max",
        observed_model=None, observed_effort=None,
    ):
        validation_command = {
            "command_id": "focused-tests",
            "argv": [sys.executable, "-m", "unittest", "tests.test_contracts"],
            "timeout_seconds": 120,
        }
        allowance = {
            "schema_version": 1, "allowance_id": "helpers-1", "run_id": "RUN-HELPERS",
            "assignments": [
                self._assignment(
                    "evidence", "tech_lead", "evidence_scout", [], model, reasoning_effort,
                ),
                self._assignment(
                    "validation", "senior_engineer", "validation_executor", [validation_command],
                    model, reasoning_effort,
                ),
            ],
            "shared_limits": {
                "children": 2, "concurrency": 1, "commands": 2,
                "time_seconds": 240, "output_tokens": 4000, "file_reads": 12,
            },
            "resources": [{"key": "worktree", "capacity": 1}],
        }
        allowance_path = self.repo / "docs" / "helper-allowance.json"
        allowance_path.write_bytes(canonical_bytes(allowance))
        task = self.task_v2()
        task["schema_version"] = 3
        task["task_id"] = "TASK-HELPERS"
        task["helper_allowance"] = {
            "ref": "repo:docs/helper-allowance.json",
            "sha256": sha256_bytes(allowance_path.read_bytes()),
        }
        plan = build_execution_plan("RUN-HELPERS", task, host="codex-astra")
        plan_path = self.repo / "docs" / "helper-plan.json"
        plan_path.write_bytes(canonical_bytes(plan))
        capabilities = {}
        for name in (
            "fresh_model_effort_selection", "filesystem_confinement", "tool_confinement",
            "command_confinement",
        ):
            unavailable = unsupported and name == "filesystem_confinement"
            capabilities[name] = {
                "status": "unavailable" if unavailable else "verified",
                "source": "host_api",
                "uncertainty": "not exposed" if unavailable else "none observed",
            }
        host_path = self.repo / "docs" / "host-observation.json"
        host_path.write_bytes(canonical_bytes({
            "schema_version": 1, "host_id": "local-host",
            "observed_at": "2026-09-12T00:00:00Z", "source": "host capability probe",
            "uncertainty": "trusted caller evidence only", "capabilities": capabilities,
            "supported_assignments": [{
                "model": observed_model or model,
                "reasoning_effort": observed_effort or reasoning_effort, "status": "verified",
                "source": "host_api", "uncertainty": "none observed",
            }],
        }))
        registry = HelperRegister()
        initialized = registry.initialize(
            self.root / "host-state", self.repo, "RUN-HELPERS", plan_path,
            allowance_path, host_path,
        )
        return registry, initialized, validation_command, plan, allowance

    def _request(self, request_id="request-1", assignment="evidence", command=None):
        helper_role = "validation_executor" if assignment == "validation" else "evidence_scout"
        parent_role = "senior_engineer" if assignment == "validation" else "tech_lead"
        checkpoint = self.repo / "docs" / ("checkpoint-" + request_id + ".json")
        checkpoint.write_text(json.dumps({"request": request_id, "dirty": True}), encoding="utf-8")
        checkpoint_ref = (
            "repo:docs/" + checkpoint.name + "#sha256=" + sha256_bytes(checkpoint.read_bytes())
        )
        return {
            "schema_version": 1, "request_id": request_id, "assignment_id": assignment,
            "parent_role": parent_role, "helper_role": helper_role, "contract_revision": 1,
            "model": "gpt-5.6-luna", "reasoning_effort": "max",
            "scope_refs": ["repo:docs/"], "commands": [] if command is None else [command],
            "checkpoint_ref": checkpoint_ref, "resource_keys": ["worktree"],
            "budgets": {"time_seconds": 60, "output_tokens": 1000, "file_reads": 3},
        }

    @staticmethod
    def _settlement(request, state="failed"):
        return {
            "schema_version": 1, "request_id": request["request_id"],
            "request_digest": sha256_bytes(canonical_bytes(request)),
            "terminal_state": state, "evidence_refs": [],
            "uncertainty": "host execution evidence only",
            "session_identity_sha256": "a" * 64, "usage_ref": None,
        }

    def test_hash_order_binds_allowance_then_plan_then_register(self):
        registry, initialized, _command, plan, allowance = self._materials()
        self.assertEqual(plan["schema_version"], 3)
        self.assertNotIn("plan_digest", allowance)
        self.assertEqual(
            plan["helper_allowance"]["sha256"], initialized["context"]["allowance_digest"],
        )
        self.assertEqual(plan["plan_digest"], initialized["context"]["plan_digest"])
        self.assertEqual(
            initialized["context"]["allowance_payload_digest"],
            sha256_bytes(canonical_bytes(validate_allowance(allowance, "RUN-HELPERS", "codex-astra"))),
        )
        self.assertEqual(
            Path(initialized["context"]["register_path"]), Path(initialized["register_path"]),
        )
        replay = registry.initialize(
            self.root / "host-state", self.repo, "RUN-HELPERS",
            self.repo / "docs" / "helper-plan.json",
            self.repo / "docs" / "helper-allowance.json",
            self.repo / "docs" / "host-observation.json",
        )
        self.assertEqual(replay["code"], "REPLAYED")

    def test_reopened_register_rejects_mutated_immutable_payloads_and_identities(self):
        registry, initialized, _command, _plan, _allowance = self._materials()
        path, context = Path(initialized["register_path"]), initialized["context"]
        original = path.read_bytes()
        mutations = (
            ("allowance", lambda record: record["allowance"]["shared_limits"].update(children=3), "status"),
            ("host", lambda record: record["host_observation"].update(uncertainty="changed"), "preflight"),
            ("reference", lambda record: record.update(allowance_ref="repo:docs/other.json"), "reserve"),
            ("repository", lambda record: record["repository_identity"].update(inode=123), "reopen"),
        )
        for name, mutate, operation in mutations:
            with self.subTest(name=name, operation=operation):
                record = json.loads(original)
                mutate(record)
                path.write_bytes(canonical_bytes(record))
                reopened = HelperRegister()
                with self.assertRaisesRegex(HelperRegisterError, "REGISTER_INVALID"):
                    if operation == "preflight":
                        reopened.preflight(path, context, self._request("mutated-host"))
                    elif operation == "reserve":
                        reopened.reserve(path, context, self._request("mutated-reference"))
                    else:
                        reopened.status(path, context)
                path.write_bytes(original)

    def test_initialization_replay_compares_all_immutable_inputs(self):
        registry, initialized, _command, _plan, _allowance = self._materials()
        host_path = self.repo / "docs" / "host-observation.json"
        host = json.loads(host_path.read_text(encoding="utf-8"))
        host["uncertainty"] = "new observation bytes"
        host_path.write_bytes(canonical_bytes(host))
        with self.assertRaisesRegex(HelperRegisterError, "INITIALIZATION_CONFLICT"):
            registry.initialize(
                self.root / "host-state", self.repo, "RUN-HELPERS",
                self.repo / "docs" / "helper-plan.json",
                self.repo / "docs" / "helper-allowance.json", host_path,
            )
        self.assertEqual(
            registry.status(Path(initialized["register_path"]), initialized["context"])["code"],
            "STATUS",
        )

    def test_reservation_replay_precedes_checkpoint_and_authority_revalidation(self):
        registry, initialized, _command, _plan, _allowance = self._materials()
        path, context = Path(initialized["register_path"]), initialized["context"]
        request = self._request("durable-replay")
        self.assertTrue(registry.reserve(path, context, request)["new_dispatch_authorized"])
        checkpoint_body = request["checkpoint_ref"][5:].split("#sha256=", 1)[0]
        checkpoint = self.repo / checkpoint_body
        checkpoint.write_text("changed", encoding="utf-8")
        replay = registry.reserve(path, context, request)
        self.assertEqual((replay["code"], replay["new_dispatch_authorized"]), ("REPLAYED", False))
        checkpoint.unlink()
        self.assertEqual(registry.reserve(path, context, request)["code"], "REPLAYED")
        for field, value in (
            ("scope_refs", ["repo:docs/artifacts/"]),
            ("assignment_id", "validation"),
        ):
            changed = json.loads(json.dumps(request))
            changed[field] = value
            with self.subTest(field=field), self.assertRaisesRegex(
                HelperRegisterError, "RESERVATION_CONFLICT",
            ):
                registry.reserve(path, context, changed)

    def test_state_root_and_register_location_are_bound_to_context(self):
        registry, initialized, _command, _plan, _allowance = self._materials()
        original_path = Path(initialized["register_path"])
        alternate = registry.initialize(
            self.root / "alternate-state", self.repo, "RUN-HELPERS",
            self.repo / "docs" / "helper-plan.json",
            self.repo / "docs" / "helper-allowance.json",
            self.repo / "docs" / "host-observation.json",
        )
        self.assertNotEqual(initialized["context"]["register_id"], alternate["context"]["register_id"])
        with self.assertRaisesRegex(HelperRegisterError, "REGISTER_BINDING_MISMATCH"):
            registry.status(Path(alternate["register_path"]), initialized["context"])
        copied_directory = self.root / "copied" / initialized["context"]["register_id"]
        copied_directory.mkdir(parents=True)
        copied = copied_directory / "register.json"
        copied.write_bytes(original_path.read_bytes())
        with self.assertRaisesRegex(HelperRegisterError, "REGISTER_BINDING_MISMATCH"):
            registry.status(copied, initialized["context"])

    def test_mutations_reject_external_register_paths_before_touching_locks(self):
        registry, initialized, _command, _plan, _allowance = self._materials()
        request = self._request("external-path")
        settlement = self._settlement(request)
        for operation, value in (("reserve", request), ("settle", settlement)):
            for lock_exists in (False, True):
                with self.subTest(operation=operation, lock_exists=lock_exists):
                    directory = self.root / "external" / operation / str(lock_exists)
                    directory.mkdir(parents=True)
                    register_path = directory / "register.json"
                    register_path.write_bytes(b"external register sentinel")
                    lock_path = directory / "register.lock"
                    if lock_exists:
                        lock_path.write_bytes(b"external lock sentinel")
                    before = {
                        path.name: path.read_bytes()
                        for path in directory.iterdir()
                    }
                    with self.assertRaisesRegex(
                        HelperRegisterError, "REGISTER_BINDING_MISMATCH",
                    ):
                        getattr(registry, operation)(
                            register_path, initialized["context"], value,
                        )
                    after = {
                        path.name: path.read_bytes()
                        for path in directory.iterdir()
                    }
                    self.assertEqual(after, before)

    def test_missing_observed_host_restriction_blocks_without_reservation(self):
        registry, initialized, _command, _plan, _allowance = self._materials(unsupported=True)
        request = self._request()
        result = registry.preflight(
            Path(initialized["register_path"]), initialized["context"], request,
        )
        self.assertEqual((result["ok"], result["code"]), (False, "BLOCKED_UNSUPPORTED"))
        self.assertEqual(result["missing_host_capabilities"], ["filesystem_confinement"])
        status = registry.status(Path(initialized["register_path"]), initialized["context"])
        self.assertEqual(status["reservations"], {})

    def test_approved_host_supported_assignment_override_is_preserved_exactly(self):
        registry, initialized, _command, _plan, _allowance = self._materials(
            model="gpt-6-astra", reasoning_effort="medium",
        )
        request = self._request("astra-override")
        request["model"] = "gpt-6-astra"
        request["reasoning_effort"] = "medium"
        result = registry.preflight(
            Path(initialized["register_path"]), initialized["context"], request,
        )
        self.assertEqual(result["code"], "PREFLIGHT_READY")

    def test_observed_assignment_mismatch_blocks_without_substitution(self):
        registry, initialized, _command, _plan, _allowance = self._materials(
            observed_model="gpt-5.6-sol", observed_effort="high",
        )
        result = registry.preflight(
            Path(initialized["register_path"]), initialized["context"], self._request("host-mismatch"),
        )
        self.assertEqual((result["ok"], result["code"]), (False, "BLOCKED_UNSUPPORTED"))
        self.assertEqual(result["missing_host_capabilities"], ["model_effort_assignment"])

    def test_assignment_scope_command_and_mandatory_resource_keys_fail_closed(self):
        registry, initialized, command, _plan, _allowance = self._materials()
        path = Path(initialized["register_path"])
        wrong_model = self._request("wrong-model")
        wrong_model["model"] = "gpt-5.6-sol"
        wrong_scope = self._request("wrong-scope")
        wrong_scope["scope_refs"] = ["repo:src/"]
        missing_resource = self._request("missing-resource")
        missing_resource["resource_keys"] = []
        wrong_command = self._request("wrong-command", "validation", command)
        wrong_command["commands"][0] = {**command, "argv": [sys.executable, "-V"]}
        for request in (wrong_model, wrong_scope, missing_resource, wrong_command):
            with self.subTest(request=request["request_id"]), self.assertRaises(ContractError):
                registry.preflight(path, initialized["context"], request)

    def test_allowance_may_not_exceed_approved_parent_capabilities(self):
        _registry, _initialized, _command, _plan, allowance = self._materials()
        exceeded_scope = json.loads(json.dumps(allowance))
        exceeded_scope["assignments"][0]["scope_refs"] = ["repo:src/"]
        exceeded_command = json.loads(json.dumps(allowance))
        exceeded_command["assignments"][1]["parent_capabilities"] = [
            {"effect": "filesystem_read", "action": "read", "target_ref": "repo:docs/"},
        ]
        for candidate in (exceeded_scope, exceeded_command):
            with self.subTest(candidate=candidate["assignments"]), self.assertRaisesRegex(
                ContractError, "PARENT_AUTHORITY_EXCEEDED",
            ):
                validate_allowance(candidate, "RUN-HELPERS", "codex-astra")

    def test_settlement_releases_only_active_resources_and_never_refunds_budgets(self):
        registry, initialized, command, _plan, _allowance = self._materials()
        path, context = Path(initialized["register_path"]), initialized["context"]
        first = self._request("validation-1", "validation", command)
        reserved = registry.reserve(path, context, first)
        self.assertEqual(registry.reserve(path, context, first)["code"], "REPLAYED")
        settlement = self._settlement(first)
        self.assertEqual(registry.settle(path, context, settlement)["code"], "SETTLED")
        self.assertEqual(registry.settle(path, context, settlement)["code"], "REPLAYED")
        changed = dict(first)
        changed["budgets"] = {**first["budgets"], "output_tokens": 999}
        with self.assertRaisesRegex(HelperRegisterError, "RESERVATION_CONFLICT"):
            registry.reserve(path, context, changed)
        second = self._request("validation-2", "validation", command)
        registry.reserve(path, context, second)
        registry.settle(path, context, self._settlement(second, "replaced"))
        status = registry.status(path, context)
        self.assertEqual(status["usage"]["children"], 2)
        self.assertEqual(status["usage"]["commands"], 2)
        self.assertEqual(status["active_concurrency"], 0)
        self.assertEqual(status["active_resources"]["worktree"], 0)
        with self.assertRaisesRegex(HelperRegisterError, "SHARED_LIMIT_EXCEEDED"):
            registry.reserve(path, context, self._request("validation-3", "validation", command))
        self.assertEqual(reserved["request_digest"], settlement["request_digest"])

    def test_cross_process_lock_allows_only_one_atomic_active_reservation(self):
        registry, initialized, _command, _plan, _allowance = self._materials()
        context_path = self.root / "context.json"
        context_path.write_bytes(canonical_bytes(initialized["context"]))
        request_paths = []
        for request_id in ("parallel-1", "parallel-2"):
            path = self.root / (request_id + ".json")
            path.write_bytes(canonical_bytes(self._request(request_id)))
            request_paths.append(path)
        script = Path(__file__).parents[1] / "scripts" / "helper-register.py"
        processes = [subprocess.Popen(
            [sys.executable, str(script), "reserve", "--register", initialized["register_path"],
             "--context", str(context_path), "--request", str(request_path)],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        ) for request_path in request_paths]
        results = []
        for process in processes:
            stdout, stderr = process.communicate(timeout=15)
            self.assertEqual(stderr, "")
            results.append(json.loads(stdout))
        self.assertEqual(
            sorted(result["code"] for result in results),
            ["RESERVED", "SHARED_LIMIT_EXCEEDED"],
        )
        status = registry.status(Path(initialized["register_path"]), initialized["context"])
        self.assertEqual((status["usage"]["children"], status["active_concurrency"]), (1, 1))

    def test_interrupted_atomic_replace_preserves_prior_register(self):
        _registry, initialized, _command, _plan, _allowance = self._materials()
        path, context = Path(initialized["register_path"]), initialized["context"]
        failing = HelperRegister(
            lambda _point: (_ for _ in ()).throw(RuntimeError("interrupted")),
        )
        with self.assertRaisesRegex(RuntimeError, "interrupted"):
            failing.reserve(path, context, self._request("interrupted"))
        clean = HelperRegister()
        self.assertEqual(clean.status(path, context)["reservations"], {})
        self.assertTrue(list(path.parent.glob(".register.*.tmp")))
        self.assertEqual(
            clean.reserve(path, context, self._request("interrupted"))["code"], "RESERVED",
        )


class TimingMetricTests(unittest.TestCase):
    def setUp(self):
        import sqlite3
        self.connection = sqlite3.connect(":memory:")
        self.connection.row_factory = sqlite3.Row
        self.connection.executescript("""
        CREATE TABLE nodes(branch_id TEXT PRIMARY KEY,run_id TEXT,node_key TEXT,role TEXT,stage TEXT,
          generation INTEGER,status TEXT,retry_count INTEGER,max_retries INTEGER,envelope_json TEXT,
          started_at TEXT,finished_at TEXT);
        CREATE TABLE branch_attempts(run_id TEXT,branch_id TEXT,attempt_number INTEGER,attempt_id TEXT,
          claim_digest TEXT,started_at TEXT,finished_at TEXT,outcome TEXT);
        CREATE TABLE operations(run_id TEXT,resulting_revision INTEGER,response_json TEXT);
        CREATE TABLE joins(join_id TEXT PRIMARY KEY,run_id TEXT);
        CREATE TABLE join_members(join_id TEXT,branch_id TEXT,mandatory INTEGER);
        CREATE TABLE fanouts(fanout_id TEXT PRIMARY KEY,run_id TEXT);
        CREATE TABLE fanout_dependencies(fanout_id TEXT,before_branch_id TEXT,after_branch_id TEXT,reason TEXT);
        """)

    def tearDown(self):
        self.connection.close()

    def _node(self, branch_id, started, finished):
        self.connection.execute(
            "INSERT INTO nodes VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
            (branch_id, "RUN", branch_id, "role", "delivery", 0, "succeeded", 0, 1, "{}", started, finished),
        )

    def _attempt(self, branch_id, number, started, finished):
        self.connection.execute(
            "INSERT INTO branch_attempts VALUES(?,?,?,?,?,?,?,?)",
            ("RUN", branch_id, number, branch_id + str(number), "a" * 64, started, finished, "succeeded"),
        )

    def test_exact_overlap_slowest_and_critical_path_metrics(self):
        for branch_id, start, finish in (("A", 1, 5), ("B", 2, 7), ("C", 7, 9)):
            self._node(branch_id, "2026-01-01T00:00:0{}Z".format(start), "2026-01-01T00:00:0{}Z".format(finish))
            self._attempt(branch_id, 1, "2026-01-01T00:00:0{}Z".format(start), "2026-01-01T00:00:0{}Z".format(finish))
        self.connection.execute("INSERT INTO fanouts VALUES('F','RUN')")
        self.connection.execute("INSERT INTO fanout_dependencies VALUES('F','B','C','ordered')")
        timing = compute_timing(self.connection, {
            "run_id": "RUN", "status": "complete", "started_at": "2026-01-01T00:00:00Z",
            "finished_at": "2026-01-01T00:00:10Z",
        })
        stage = timing["stages"][0]
        self.assertEqual(timing["run"]["wall_time_ms"], 10000)
        self.assertEqual((stage["wall_time_ms"], stage["overlap_time_ms"]), (8000, 3000))
        self.assertEqual(stage["slowest_branch"], {"branch_id": "B", "active_duration_ms": 5000})
        self.assertEqual(stage["critical_path"], {"branch_ids": ["B", "C"], "active_duration_ms": 7000})

    def test_retry_lifecycle_and_microsecond_flooring(self):
        self._node("R", "2026-01-01T00:00:01Z", "2026-01-01T00:00:06Z")
        self._attempt("R", 1, "2026-01-01T00:00:01Z", "2026-01-01T00:00:02Z")
        self._attempt("R", 2, "2026-01-01T00:00:04Z", "2026-01-01T00:00:06Z")
        timing = compute_timing(self.connection, {
            "run_id": "RUN", "status": "complete", "started_at": "2026-01-01T00:00:00Z",
            "finished_at": "2026-01-01T00:00:01.001500Z",
        })
        self.assertEqual(timing["branches"]["R"]["wall_time_ms"], 5000)
        self.assertEqual(timing["branches"]["R"]["active_duration_ms"], 3000)
        self.assertEqual(timing["run"]["wall_time_ms"], 1001)

    def test_incomplete_metrics_are_null_and_equal_ties_are_lexical(self):
        for branch_id in ("B", "A"):
            self._node(branch_id, "2026-01-01T00:00:01Z", "2026-01-01T00:00:03Z")
            self._attempt(branch_id, 1, "2026-01-01T00:00:01Z", "2026-01-01T00:00:03Z")
        tied = compute_timing(self.connection, {
            "run_id": "RUN", "status": "complete", "started_at": "2026-01-01T00:00:00Z",
            "finished_at": "2026-01-01T00:00:04Z",
        })["stages"][0]
        self.assertEqual(tied["slowest_branch"]["branch_id"], "A")
        self.assertEqual(tied["critical_path"]["branch_ids"], ["A"])

        self.connection.execute("DELETE FROM branch_attempts")
        self.connection.execute("DELETE FROM nodes")
        self.connection.execute(
            "INSERT INTO nodes VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
            ("RUNNING", "RUN", "running", "role", "delivery", 0, "running", 0, 1, "{}",
             "2026-01-01T00:00:01Z", None),
        )
        self.connection.execute(
            "INSERT INTO branch_attempts VALUES(?,?,?,?,?,?,?,?)",
            ("RUN", "RUNNING", 1, "attempt", "a" * 64, "2026-01-01T00:00:01Z", None, None),
        )
        incomplete = compute_timing(self.connection, {
            "run_id": "RUN", "status": "active", "started_at": "2026-01-01T00:00:00Z",
            "finished_at": None,
        })
        branch = incomplete["branches"]["RUNNING"]
        self.assertFalse(branch["timing_complete"])
        self.assertIsNone(branch["wall_time_ms"])
        self.assertIsNone(branch["active_duration_ms"])
        self.assertIsNone(incomplete["overall"]["overlap_time_ms"])
        self.assertIsNone(incomplete["overall"]["slowest_branch"])
        self.assertIsNone(incomplete["overall"]["critical_path"])


if __name__ == "__main__":
    unittest.main()
