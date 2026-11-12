from __future__ import annotations

import copy
import hashlib
import importlib.util
import itertools
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("production_context", ROOT / "shared/video-production/production_context.py")
assert SPEC and SPEC.loader
context = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(context)


class ProductionContextTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.serial = 0
        self.previous_budget = {"schema_version": 1, "production_id": "p1", "stage_id": "s1",
            "lineage_id": "work1", "sequence": 0, "spent_rounds": 0, "reservations": [],
            "policy_limit": 3, "allowance_limit": 3, "allowance_refs": [], "attempts": []}
        checkpoint = self.save(self.previous_budget)
        self.record = {"schema_version": 1, "production_id": "p1", "stage_id": "s1", "generation": 0,
            "nodes": [self.node("script", [], ["lesson"]), self.node("voice", ["script"], ["speech"]),
                      self.node("encode", ["voice"], ["movie"]), self.node("cover", [], ["cover"])],
            "gates": [{"id": "g1", "prerequisite_nodes": ["script"], "budget_checkpoint": checkpoint,
                       "required": [{"role": "technical", "reviewer_id": "r1", "scope_ids": ["speech"], "criterion_ids": ["C1"]},
                                    {"role": "visual", "reviewer_id": "r2", "scope_ids": ["movie"], "criterion_ids": ["C2"]}],
                       "check_ids": ["structure"], "consistency_coverage": ["captions", "encoded", "delivery"]}]}
        self.current = context.validate_context(self.record, self.root)
        self.gate, self.budget = self.runtime(self.current)

    def save(self, value):
        self.serial += 1
        name = f"record-{self.serial}.json"
        data = context.canonical(value)
        (self.root / name).write_bytes(data)
        return {"path": name, "sha256": hashlib.sha256(data).hexdigest()}

    def source(self, name, data):
        (self.root / name).parent.mkdir(parents=True, exist_ok=True)
        (self.root / name).write_text(data, encoding="utf-8")
        return {"path": name, "sha256": hashlib.sha256(data.encode("utf-8")).hexdigest()}

    def node(self, identifier, parents, scopes):
        return {"id": identifier, "kind": "source", "scope_ids": scopes, "depends_on": parents,
                "artifacts": [self.source(identifier + ".txt", "synthetic " + identifier)]}

    def runtime(self, validated, candidate="a" * 64, reservation="round1"):
        gate = {"id": "g1", "candidateDigest": candidate, "reservation_id": reservation, "prior_findings": []}
        binding = context.binding(validated, gate)
        gate["validation_evidence"] = [self.save({"schema_version": 1, **binding, "check_id": "structure", "status": "passed"})]
        gate["prerequisites"] = [self.save({"schema_version": 1, **binding, "node_id": "script",
            "fingerprint": validated["fingerprints"]["script"], "decision": "APPROVE", "status": "reviewed"})]
        gate["final_consistency"] = self.save({"schema_version": 1, **binding,
            "decision": "APPROVE", "status": "reviewed", "fresh": True, "live_candidateDigest": candidate,
            "coverage": ["captions", "encoded", "delivery"], "evidence": []})
        budget = {**copy.deepcopy(self.previous_budget), "sequence": 1, "spent_rounds": 1,
                  "reservations": [{"id": reservation, "round": 1, "candidateDigest": candidate,
                                    "contextDigest": validated["contextDigest"]}]}
        return gate, budget

    def reviews(self, validated=None, gate=None):
        validated, gate = validated or self.current, gate or self.gate
        packets, results = [], []
        for requirement in validated["record"]["gates"][0]["required"]:
            packet = context.build_role_packet(validated, gate, requirement["role"])
            reference = self.save(packet)
            packets.append(reference)
            results.append({"schema_version": 1, **context.binding(validated, gate), **requirement,
                "closure": context.dependency_closure(validated, requirement["scope_ids"]),
                "packet": reference, "decision": "APPROVE", "status": "reviewed", "findings": [],
                "unresolved_findings": [], "dispositions": {}, "evidence": []})
        return packets, results

    def error(self, code, operation, *args):
        with self.assertRaises(context.ContextError) as caught:
            operation(*args)
        self.assertEqual(code, caught.exception.error["code"])

    def changed(self, identifier, *, remove_dependency=False, scopes=None):
        record = copy.deepcopy(self.record)
        record["generation"] += 1
        node = next(node for node in record["nodes"] if node["id"] == identifier)
        if remove_dependency:
            node["depends_on"] = []
        elif scopes:
            node["scope_ids"] = scopes
        else:
            node["artifacts"] = [self.source(identifier + ".txt", "updated synthetic " + identifier)]
        return context.validate_context(record, self.root)

    def test_utf8_and_dependency_closure_are_deterministic(self):
        self.assertEqual(["script", "voice"], list(context.dependency_closure(self.current, ["speech"])))
        record = copy.deepcopy(self.record)
        record["nodes"].reverse()
        self.assertEqual(self.current["contextDigest"], context.validate_context(record, self.root)["contextDigest"])
        value = {"spoken": "café, 日本語, акцент"}
        self.assertEqual(value, context.load_json(self.root / self.save(value)["path"]))
        self.assertIn("日本語".encode(), context.canonical(value))

    def test_changed_ancestor_invalidates_identical_descendant_bytes(self):
        updated = self.changed("script")
        self.assertEqual(["encode", "script", "voice"], context.invalidated_nodes(self.current, updated))
        self.error("STALE_ARTIFACT", context.refresh, self.current)

    def test_removed_dependency_and_scope_change_invalidate_descendants(self):
        updated = self.changed("voice", remove_dependency=True)
        self.assertEqual(["encode", "voice"], context.invalidated_nodes(self.current, updated))
        updated = self.changed("voice", scopes=["speech", "expanded"])
        self.assertEqual(["encode", "voice"], context.invalidated_nodes(self.current, updated))

    def test_shared_style_change_invalidates_all_consumers(self):
        record = copy.deepcopy(self.record)
        style = self.node("style", [], ["style"])
        record["nodes"].append(style)
        for node in record["nodes"][:4]:
            node["depends_on"].append("style")
        old = context.validate_context(record, self.root)
        style["artifacts"] = [self.source("style.txt", "new style")]
        new = context.validate_context(record, self.root)
        self.assertEqual(["cover", "encode", "script", "style", "voice"], context.invalidated_nodes(old, new))

    def test_rejects_graph_errors_unknown_versions_and_duplicate_json_keys(self):
        for mutation, code in ((lambda r: r["nodes"][0]["depends_on"].append("encode"), "CYCLE"),
                               (lambda r: r["nodes"][0]["depends_on"].append("absent"), "UNKNOWN_NODE"),
                               (lambda r: r["nodes"].append(copy.deepcopy(r["nodes"][0])), "DUPLICATE"),
                               (lambda r: r["nodes"][1]["depends_on"].append("script"), "DUPLICATE"),
                               (lambda r: r.update(generation=True), "LIMIT"),
                               (lambda r: r.update(schema_version=2), "SCHEMA")):
            record = copy.deepcopy(self.record)
            mutation(record)
            self.error(code, context.validate_context, record, self.root)
        path = self.root / "duplicate.json"
        path.write_text('{"id":1,"id":2}', encoding="utf-8")
        self.error("DUPLICATE_KEY", context.load_json, path)

    def test_paths_missing_hash_and_link_artifacts_fail_closed(self):
        for path in ("../outside", "/absolute", "C:\\outside", "..\\outside", ".git/config"):
            self.error("PATH", context.project_path, self.root, path)
        self.error("MISSING_ARTIFACT", context.artifact, self.root, {"path": "absent", "sha256": "a" * 64})
        self.error("STALE_ARTIFACT", context.artifact, self.root, {"path": "script.txt", "sha256": "a" * 64})
        link = self.root / "link.txt"
        try:
            link.symlink_to(self.root / "script.txt")
        except OSError:
            self.skipTest("Host does not allow synthetic symlink creation")
        self.error("LINK", context.project_path, self.root, "link.txt")

    def test_exact_manifest_node_and_packet_limits(self):
        record = copy.deepcopy(self.record)
        for index in range(context.MAX_NODES - len(record["nodes"])):
            record["nodes"].append({"id": f"n{index}", "kind": "declared", "scope_ids": ["lesson"], "artifacts": [], "depends_on": []})
        context.validate_context(record, self.root)
        record["nodes"].append({"id": "overflow"})
        self.error("NODE_LIMIT", context.validate_context, record, self.root)
        path = self.root / "bounded.json"
        path.write_bytes(b"{}" + b" " * 8)
        self.assertEqual({}, context.load_json(path, 10))
        self.error("MANIFEST_LIMIT", context.load_json, path, 9)
        packet = context.build_role_packet(self.current, self.gate, "technical")
        size = len(context.canonical(packet))
        record = copy.deepcopy(self.record)
        record["gates"][0]["bounds"] = {"packet_bytes": size - 1}
        updated = context.validate_context(record, self.root)
        gate, _ = self.runtime(updated)
        self.error("PACKET_LIMIT", context.build_role_packet, updated, gate, "technical")

    def test_prerequisites_and_validation_must_precede_packet(self):
        gate = copy.deepcopy(self.gate)
        gate["prerequisites"] = []
        self.error("PREREQUISITE", context.build_role_packet, self.current, gate, "technical")
        gate = copy.deepcopy(self.gate)
        gate["validation_evidence"] = []
        self.error("CHECK", context.build_role_packet, self.current, gate, "technical")

    def logical(self):
        record = copy.deepcopy(self.record)
        record["gates"][0]["required"].append({"role": "logical", "reviewer_id": "r3", "scope_ids": ["movie"], "criterion_ids": ["C3"]})
        current = context.validate_context(record, self.root)
        gate, _ = self.runtime(current)
        gate["audience"] = {"media": [self.source("audience/1.bin", "synthetic audience evidence")], "intervals": [[0, 1]]}
        return current, gate

    def blind_receipts(self, current, gate):
        packet = self.save(context.build_role_packet(current, gate, "logical", "audience"))
        reading = {"schema_version": 1, **context.binding(current, gate), "reviewer_id": "r3",
                   "packet": packet, "input_scope": "audience_evidence_only", "observations": "Visible synthetic change"}
        gate["first_reading"] = self.save(reading)
        capture = {"schema_version": 1, **context.binding(current, gate), "reviewer_id": "r3", "packet": packet,
                   "available_inputs": [packet], "first_reading_sha256": gate["first_reading"]["sha256"],
                   "fresh_session_id": "session3", "first_reading_sequence": 1, "intent_release_sequence": 2}
        gate["host_capture"] = self.save(capture)
        return capture

    def test_blind_allowlist_and_first_reading_order(self):
        current, gate = self.logical()
        packet = context.build_role_packet(current, gate, "logical", "audience")
        for key in ("nodes", "scope_ids", "criterion_ids", "prior_findings", "validation_evidence"):
            self.assertNotIn(key, packet)
        with self.assertRaises((context.ContextError, TypeError)):
            context.build_role_packet(current, gate, "logical")
        capture = self.blind_receipts(current, gate)
        self.assertEqual("technical", context.build_role_packet(current, gate, "logical")["phase"])
        capture["intent_release_sequence"] = 0
        gate["host_capture"] = self.save(capture)
        self.error("ISOLATION_GAP", context.build_role_packet, current, gate, "logical")
        gate["audience"]["media"][0]["intent"] = "desired meaning"
        self.error("PACKET_CONTAMINATION", context.build_role_packet, current, gate, "logical", "audience")

    def test_blind_packet_cross_version_and_missing_capture_rejected(self):
        current, gate = self.logical()
        capture = self.blind_receipts(current, gate)
        capture["generation"] += 1
        gate["host_capture"] = self.save(capture)
        self.error("CROSS_VERSION", context.build_role_packet, current, gate, "logical")
        del gate["host_capture"]
        self.error("TYPE", context.build_role_packet, current, gate, "logical")

    def test_fan_in_is_deterministic_and_role_namespaced(self):
        packets, results = self.reviews()
        for result in results:
            result["decision"] = "REVISE"
            result["findings"] = [{"id": "F1", "criterion_id": result["criterion_ids"][0],
                "scope_id": result["scope_ids"][0], "locator": "frame 1", "issue": "ambiguous event",
                "effect": "unclear causal step", "repair": "clarify anchor"}]
        outputs = [context.canonical(context.aggregate_reviews(self.current, self.gate, list(p), list(r), self.budget, self.previous_budget))
                   for p in itertools.permutations(packets) for r in itertools.permutations(results)]
        self.assertEqual(1, len(set(outputs)))
        self.assertEqual(["technical:F1", "visual:F1"], json.loads(outputs[0])["unresolved_findings"])

    def test_negative_or_pending_consistency_preserves_deterministic_repair(self):
        packets, results = self.reviews()
        results[0]["decision"] = "REVISE"
        results[0]["findings"] = [{"id": "F1", "criterion_id": "C1", "scope_id": "speech",
            "locator": "caption 1", "issue": "contradiction", "effect": "wrong outcome", "repair": "correct caption"}]
        final = context.load_json(self.root / self.gate["final_consistency"]["path"])
        for status, verdict, expected in (("reviewed", "REVISE", "REVISE"),
                                           ("reviewed", "BLOCK", "BLOCK"), ("pending", None, "REVISE")):
            with self.subTest(status=status, verdict=verdict):
                gate = {**self.gate, "final_consistency": self.save({**final, "status": status, "decision": verdict})}
                outputs = [context.canonical(context.aggregate_reviews(self.current, gate, list(p), list(r), self.budget, self.previous_budget))
                           for p in itertools.permutations(packets) for r in itertools.permutations(results)]
                self.assertEqual(1, len(set(outputs)))
                repair = json.loads(outputs[0])
                self.assertEqual(expected, repair["decision"])
                self.assertEqual(["technical:F1"], repair["unresolved_findings"])
                self.assertEqual("correct caption", repair["findings"][0]["repair"])
        gate = {key: value for key, value in self.gate.items() if key != "final_consistency"}
        repair = context.aggregate_reviews(self.current, gate, packets, results, self.budget, self.previous_budget)
        self.assertEqual("REVISE", repair["decision"])
        self.assertEqual(["technical:F1"], repair["unresolved_findings"])
        results[0]["decision"] = "BLOCK"
        self.assertEqual("BLOCK", context.aggregate_reviews(self.current, gate, packets, results, self.budget, self.previous_budget)["decision"])

    def test_approval_cannot_advance_with_negative_or_pending_consistency(self):
        packets, results = self.reviews()
        final = context.load_json(self.root / self.gate["final_consistency"]["path"])
        for receipt in (None, {**final, "decision": "REVISE"}, {**final, "decision": "BLOCK"},
                        {**final, "status": "pending", "decision": None}):
            gate = {**self.gate, "final_consistency": self.save(receipt) if receipt else None}
            self.error("FINAL_CONSISTENCY", context.aggregate_reviews, self.current, gate,
                       packets, results, self.budget, self.previous_budget)

    def test_role_namespace_separator_is_rejected(self):
        for roles in (("technical", "technical:audio"), ("technical:audio", "technical")):
            record = copy.deepcopy(self.record)
            for requirement, role in zip(record["gates"][0]["required"], roles):
                requirement["role"] = role
            self.error("ROLE_NAMESPACE", context.validate_context, record, self.root)

    def test_only_exact_owner_can_resolve_and_omission_is_permutation_stable(self):
        self.gate["prior_findings"] = ["visual:F1"]
        packets, results = self.reviews()
        outputs = [context.canonical(context.aggregate_reviews(self.current, self.gate, list(p), list(r), self.budget, self.previous_budget))
                   for p in itertools.permutations(packets) for r in itertools.permutations(results)]
        self.assertEqual(1, len(set(outputs)))
        self.assertEqual(["visual:F1"], json.loads(outputs[0])["unresolved_findings"])
        results[0]["dispositions"] = {"visual:F1": "resolved"}
        for permutation in itertools.permutations(results):
            self.error("DISPOSITION", context.aggregate_reviews, self.current, self.gate, packets,
                       list(permutation), self.budget, self.previous_budget)
        results[0]["dispositions"] = {}
        results[1]["dispositions"] = {"visual:F1": "resolved"}
        for permutation in itertools.permutations(results):
            self.assertEqual("APPROVE", context.aggregate_reviews(self.current, self.gate, packets,
                             list(permutation), self.budget, self.previous_budget)["decision"])

    def test_missing_duplicate_stale_coverage_and_identity_results_rejected(self):
        packets, results = self.reviews()
        self.error("MISSING_ROLE", context.aggregate_reviews, self.current, self.gate, packets, results[:1], self.budget, self.previous_budget)
        self.error("DUPLICATE_RESULT", context.aggregate_reviews, self.current, self.gate, packets, results + results[:1], self.budget, self.previous_budget)
        for key, value, code in (("generation", 2, "CROSS_VERSION"), ("candidateDigest", "b" * 64, "CROSS_VERSION"),
                                 ("reservation_id", "other", "CROSS_VERSION"), ("reviewer_id", "r2", "REVIEW_BINDING"),
                                 ("scope_ids", ["cover"], "REVIEW_BINDING"), ("closure", {}, "REVIEW_BINDING")):
            invalid = copy.deepcopy(results)
            invalid[0][key] = value
            self.error(code, context.aggregate_reviews, self.current, self.gate, packets, invalid, self.budget, self.previous_budget)
        self.error("DUPLICATE_EVIDENCE", context.aggregate_reviews, self.current, self.gate, packets + packets[:1], results, self.budget, self.previous_budget)

    def test_old_findings_remain_open_when_omitted(self):
        self.gate["prior_findings"] = ["technical:old"]
        packets, results = self.reviews()
        result = context.aggregate_reviews(self.current, self.gate, packets, results, self.budget, self.previous_budget)
        self.assertEqual("REVISE", result["decision"])
        self.assertEqual(["technical:old"], result["unresolved_findings"])
        results[0]["dispositions"] = {"technical:old": "resolved"}
        self.assertEqual("APPROVE", context.aggregate_reviews(self.current, self.gate, packets, results, self.budget, self.previous_budget)["decision"])

    def carry(self, old, new, gate, origin):
        return {"schema_version": 1, **context.binding(new, gate), "role": "technical", "reviewer_id": "r1",
                "scope_ids": ["speech"], "criterion_ids": ["C1"], "status": "carried_forward", "decision": "APPROVE",
                "prior_receipt": self.save(origin), "old_candidateDigest": origin["candidateDigest"],
                "old_contextDigest": old["contextDigest"], "old_closure": origin["closure"],
                "new_closure": context.dependency_closure(new, ["speech"]), "unresolved_findings": []}

    def test_cover_only_change_allows_explicit_scoped_carry(self):
        _, old_results = self.reviews()
        new = self.changed("cover")
        gate, _ = self.runtime(new, "b" * 64)
        receipt = self.carry(self.current, new, gate, old_results[0])
        self.assertTrue(context.validate_carry_forward(self.current, new, receipt))
        del receipt["old_closure"]
        self.error("CARRY_PROOF", context.validate_carry_forward, self.current, new, receipt)

    def test_carry_rejects_correctly_hashed_origin_packet_with_wrong_reviewer(self):
        _, old_results = self.reviews()
        origin = old_results[0]
        packet = context.load_json(self.root / origin["packet"]["path"])
        packet["reviewer_id"] = "wrong-reviewer"
        origin["packet"] = self.save(packet)
        new = self.changed("cover")
        gate, _ = self.runtime(new, "b" * 64)
        receipt = self.carry(self.current, new, gate, origin)
        self.error("CARRY_ORIGIN", context.validate_carry_forward, self.current, new, receipt)
        packet["reviewer_id"] = "r1"
        origin["packet"] = self.save(packet)
        receipt = self.carry(self.current, new, gate, origin)
        self.assertTrue(context.validate_carry_forward(self.current, new, receipt))

    def test_scoped_carry_still_needs_current_packets_checks_and_final_consistency(self):
        _, old_results = self.reviews()
        new = self.changed("cover")
        gate, budget = self.runtime(new, "b" * 64)
        packets, results = self.reviews(new, gate)
        carry = self.carry(self.current, new, gate, old_results[0])
        results[0].update(carry)
        gate["previous_context"] = self.current
        self.assertEqual("APPROVE", context.aggregate_reviews(new, gate, packets, results, budget, self.previous_budget)["decision"])
        gate["final_consistency"] = self.gate["final_consistency"]
        self.error("CROSS_VERSION", context.aggregate_reviews, new, gate, packets, results, budget, self.previous_budget)

    def test_changed_narration_dependency_or_contract_cannot_carry(self):
        _, old_results = self.reviews()
        new = self.changed("script")
        gate, _ = self.runtime(new, "b" * 64)
        receipt = self.carry(self.current, new, gate, old_results[0])
        self.error("CARRY_PROOF", context.validate_carry_forward, self.current, new, receipt)
        self.assertIn("encode", context.invalidated_nodes(self.current, new))
        receipt["role"] = "delivery"
        self.error("CARRY_FINAL", context.validate_carry_forward, self.current, new, receipt)

    def test_gate_criterion_change_and_stale_origin_cannot_carry(self):
        _, old_results = self.reviews()
        record = copy.deepcopy(self.record)
        record["generation"] += 1
        record["gates"][0]["required"][0]["criterion_ids"] = ["C1", "C4"]
        new = context.validate_context(record, self.root)
        gate, _ = self.runtime(new, "b" * 64)
        receipt = self.carry(self.current, new, gate, old_results[0])
        self.error("CARRY_CONTRACT", context.validate_carry_forward, self.current, new, receipt)
        new = self.changed("cover")
        gate, _ = self.runtime(new, "b" * 64)
        old_results[0]["candidateDigest"] = "c" * 64
        receipt = self.carry(self.current, new, gate, old_results[0])
        receipt["old_candidateDigest"] = "a" * 64
        self.error("CARRY_ORIGIN", context.validate_carry_forward, self.current, new, receipt)

    def test_fresh_final_consistency_required_for_changed_output_or_destination(self):
        packets, results = self.reviews()
        final = context.load_json(self.root / self.gate["final_consistency"]["path"])
        for key, value, code in (("fresh", False, "FINAL_CONSISTENCY"), ("candidateDigest", "b" * 64, "CROSS_VERSION"),
                                 ("live_candidateDigest", "b" * 64, "FINAL_CONSISTENCY"), ("coverage", ["encoded"], "FINAL_CONSISTENCY")):
            changed = copy.deepcopy(final)
            changed[key] = value
            gate = {**self.gate, "final_consistency": self.save(changed)}
            self.error(code, context.aggregate_reviews, self.current, gate, packets, results, self.budget, self.previous_budget)

    def test_cumulative_budget_rejects_reset_missing_reservation_unknown_and_round_four(self):
        for key, value, code in (("spent_rounds", 4, "BUDGET_EXHAUSTED"), ("reservations", [], "BUDGET_LINEAGE"),
                                 ("attempts", [{"status": "unknown"}], "UNRESOLVED_ATTEMPT"),
                                 ("stage_id", "renamed", "BUDGET_IDENTITY"), ("sequence", 0, "BUDGET_LINEAGE")):
            budget = copy.deepcopy(self.budget)
            budget[key] = value
            self.error(code, context.check_budget, self.current, self.gate, budget, self.previous_budget)
        second = copy.deepcopy(self.budget)
        second.update(sequence=2, spent_rounds=2)
        second["reservations"].append({**second["reservations"][0], "id": "round2", "round": 2})
        gate = {**self.gate, "reservation_id": "round2"}
        context.check_budget(self.current, gate, second, self.budget)
        self.error("BUDGET_LINEAGE", context.check_budget, self.current, self.gate, self.budget, second)
        second["reservations"][0]["candidateDigest"] = "b" * 64
        self.error("BUDGET_LINEAGE", context.check_budget, self.current, gate, second, self.budget)

    def test_no_round_four_after_unresolved_third_round(self):
        prior = copy.deepcopy(self.previous_budget)
        prior.update(sequence=2, spent_rounds=2, reservations=[
            {"id": f"round{i}", "round": i, "candidateDigest": "b" * 64, "contextDigest": "c" * 64} for i in (1, 2)])
        record = copy.deepcopy(self.record)
        record["gates"][0]["budget_checkpoint"] = self.save(prior)
        current = context.validate_context(record, self.root)
        gate, budget = self.runtime(current, reservation="round3")
        budget.update(sequence=3, spent_rounds=3, reservations=prior["reservations"] + [
            {"id": "round3", "round": 3, "candidateDigest": gate["candidateDigest"], "contextDigest": current["contextDigest"]}])
        gate.pop("final_consistency")
        gate["prior_findings"] = ["technical:old"]
        packets, results = self.reviews(current, gate)
        results[0]["decision"] = "REVISE"
        repair = context.aggregate_reviews(current, gate, packets, results, budget, prior)
        self.assertEqual("BLOCK", repair["decision"])
        self.assertEqual(["technical:old"], repair["unresolved_findings"])

    def run_aggregate_cli(self, current, gate, packets, results, budget, previous):
        manifest = self.root / self.save(current["record"])["path"]
        index = {"schema_version": 1, "gate": gate, "packets": packets,
                 "results": [self.save(result) for result in results], "budget": self.save(budget),
                 "previous_budget": self.save(previous)}
        evidence = self.root / self.save(index)["path"]
        output = f"aggregate-{self.serial}.json"
        process = subprocess.run([sys.executable, "-B", str(ROOT / "shared/video-production/production_context.py"),
            "aggregate", str(manifest), "--project", str(self.root), "--gate", "g1", "--evidence", str(evidence),
            "--output", output], capture_output=True, text=True, encoding="utf-8")
        self.assertEqual("", process.stderr)
        return process.returncode, json.loads(process.stdout), self.root / output

    def test_aggregate_cli_reports_located_errors_for_malformed_nested_objects(self):
        for target, locator in (("attempts", "attempts[0]"), ("reservations", "reservations[0]"),
                                ("previous_attempts", "previous_budget.attempts[0]"),
                                ("previous_reservations", "previous_budget.reservations[0]"),
                                ("findings", "technical.findings[0]")):
            with self.subTest(target=target):
                prior = copy.deepcopy(self.previous_budget)
                record = copy.deepcopy(self.record)
                if target.startswith("previous_"):
                    prior[target.removeprefix("previous_")] = [None]
                    record["gates"][0]["budget_checkpoint"] = self.save(prior)
                current = context.validate_context(record, self.root)
                gate, budget = self.runtime(current)
                packets, results = self.reviews(current, gate)
                if target in ("attempts", "reservations"):
                    budget[target] = [None]
                elif target == "findings":
                    results[0]["findings"] = [None]
                code, result, output = self.run_aggregate_cli(current, gate, packets, results, budget, prior)
                self.assertEqual(1, code)
                self.assertFalse(result["valid"])
                self.assertEqual({"code": "TYPE", "locator": locator, "message": "Object required"}, result["errors"][0])
                self.assertFalse(output.exists())

    def test_aggregate_cli_writes_repair_with_pending_consistency(self):
        self.gate.pop("final_consistency")
        self.gate["prior_findings"] = ["technical:old"]
        packets, results = self.reviews()
        code, repair, output = self.run_aggregate_cli(self.current, self.gate, packets, results, self.budget, self.previous_budget)
        self.assertEqual(1, code)
        self.assertEqual("REVISE", repair["decision"])
        self.assertEqual(["technical:old"], repair["unresolved_findings"])
        self.assertEqual(repair, context.load_json(output))

    def test_exclusive_outputs_preserve_existing_failed_evidence(self):
        context.write_new(self.root, "failed.json", {"decision": "BLOCK"})
        before = (self.root / "failed.json").read_bytes()
        with self.assertRaises(FileExistsError):
            context.write_new(self.root, "failed.json", {"decision": "APPROVE"})
        self.assertEqual(before, (self.root / "failed.json").read_bytes())

    def test_unknown_attempt_cannot_disappear_from_budget_lineage(self):
        previous = copy.deepcopy(self.previous_budget)
        previous["attempts"] = [{"id": "uncertain", "reservation_id": "round1", "status": "unknown"}]
        self.error("BUDGET_LINEAGE", context.check_budget, self.current, self.gate, self.budget, previous)
        budget = copy.deepcopy(self.budget)
        budget["attempts"] = [{"id": "uncertain", "reservation_id": "round1", "status": "settled",
                               "reconciliation": self.save({"status": "settled"})}]
        context.check_budget(self.current, self.gate, budget, previous)

    def test_dependency_artifact_and_evidence_limits(self):
        values = [self.source(f"evidence-{index}.txt", str(index)) for index in range(context.MAX_EVIDENCE + 1)]
        context.references(self.root, values[:-1])
        self.error("EVIDENCE_LIMIT", context.references, self.root, values)
        record = copy.deepcopy(self.record)
        record["nodes"][0]["artifacts"] = values[:context.MAX_ARTIFACTS]
        context.validate_context(record, self.root)
        record["nodes"][0]["artifacts"].append(values[context.MAX_ARTIFACTS])
        self.error("EVIDENCE_LIMIT", context.validate_context, record, self.root)
        record = copy.deepcopy(self.record)
        parents = [self.node(f"parent{i}", [], ["lesson"]) for i in range(context.MAX_DEPENDENCIES + 1)]
        record["nodes"].extend(parents)
        record["nodes"][0]["depends_on"] = [node["id"] for node in parents[:-1]]
        context.validate_context(record, self.root)
        record["nodes"][0]["depends_on"].append(parents[-1]["id"])
        self.error("DEPENDENCY_LIMIT", context.validate_context, record, self.root)

    def test_encoded_audio_requires_exact_aac_diagnostics_and_actual_listening(self):
        record = copy.deepcopy(self.record)
        record["nodes"][2]["kind"] = "encoded_master"
        record["gates"][0]["check_ids"] = ["structure", "encoded-audio"]
        current = context.validate_context(record, self.root)
        gate, _ = self.runtime(current)
        encoded = record["nodes"][2]["artifacts"][0]
        wav = record["nodes"][1]["artifacts"][0]
        settings = {"codec": "aac", "source": "approved"}
        metadata = {"schema_version": 1, "encoded_sha256": encoded["sha256"], "audio_codec": "aac"}
        diagnostics = {"schema_version": 1, "encoded_sha256": encoded["sha256"], "input_wav_sha256": wav["sha256"],
                       "settingsDigest": context.digest(settings), "peak_dbfs": -1.5, "true_peak_dbfs": -1.1, "clipped_samples": 0}
        listening = {"schema_version": 1, "encoded_sha256": encoded["sha256"], "reviewer_id": "listener1",
                     "decision": "APPROVE", "coverage": "full_encoded_audio"}
        audio = {"encoded": encoded, "approved_wav": wav, "settings": settings,
                 "metadata": self.save(metadata), "diagnostics": self.save(diagnostics), "listening": self.save(listening)}
        check = {"schema_version": 1, **context.binding(current, gate), "check_id": "encoded-audio", "status": "passed", "encoded_audio": audio}
        gate["validation_evidence"].append(self.save(check))
        context.build_role_packet(current, gate, "technical")
        diagnostics["encoded_sha256"] = "d" * 64
        invalid = copy.deepcopy(check)
        invalid["encoded_audio"]["diagnostics"] = self.save(diagnostics)
        self.error("STALE_ENCODED_QA", context.encoded_audio_evidence, current, invalid)
        invalid = copy.deepcopy(check)
        invalid["encoded_audio"]["encoded"] = wav
        self.error("ENCODED_AUDIO", context.encoded_audio_evidence, current, invalid)
        invalid = copy.deepcopy(check)
        del invalid["encoded_audio"]["listening"]
        self.error("TYPE", context.encoded_audio_evidence, current, invalid)


if __name__ == "__main__":
    unittest.main()
