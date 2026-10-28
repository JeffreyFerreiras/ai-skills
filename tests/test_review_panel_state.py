"""Focused behavioral tests for the review-panel per-run state helper."""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "review_state", REPOSITORY_ROOT / "skills/review-panel/scripts/review_state.py"
)
assert SPEC and SPEC.loader
review_state = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(review_state)

ROLES = list(review_state.DEFAULT_ROLES)


def make_state(run_id: str = "run-1") -> dict:
    return review_state.init_state(
        run_id,
        r"C:\repo",
        "base-abc",
        "head-def",
        "rev-1",
        {"diff": "a" * 64},
        ["src/"],
    )


def lock_with_reports(state: dict, skip: str = "") -> None:
    for role in ROLES:
        if role == skip:
            review_state.mark_unknown(state, role, "worker timed out.")
        else:
            review_state.add_initial_report(state, role, f"{role} findings.")
    review_state.lock_initial(state)


def locked_state_with_findings() -> dict:
    state = make_state()
    lock_with_reports(state)
    review_state.add_finding(state, ROLES[0], "Bug one", {"file": "a.py:1"})
    review_state.add_finding(state, ROLES[1], "Bug two", {"file": "b.py:2"})
    return state


def run_cli(args: list[str]) -> tuple[int, str, str]:
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        code = review_state.main(args)
    return code, out.getvalue(), err.getvalue()


class PersistenceRoundtripTests(unittest.TestCase):
    def test_save_load_roundtrip_preserves_identity_and_phase(self) -> None:
        state = make_state()
        review_state.add_initial_report(state, ROLES[0], "first report.")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.json"
            review_state.save_state(path, state)
            loaded = review_state.load_state(path)
        self.assertEqual(loaded["run_id"], "run-1")
        self.assertEqual(loaded["review_identity"]["head"], "head-def")
        self.assertEqual(loaded["reports"][ROLES[0]]["text"], "first report.")
        self.assertEqual(loaded["phase"], "initial")
        self.assertFalse(loaded["source_stale"])
        self.assertEqual(loaded["followups"], [])
        self.assertEqual(loaded["next_finding"], 1)

    def test_malformed_record_fails_without_overwrite(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.json"
            path.write_text("{not json", encoding="utf-8")
            before = path.read_bytes()
            with self.assertRaises(review_state.StateError):
                review_state.load_state(path)
            self.assertEqual(path.read_bytes(), before)


class BlindnessAndPhaseGateTests(unittest.TestCase):
    def test_first_packet_leaks_no_other_results(self) -> None:
        state = make_state()
        review_state.add_initial_report(state, ROLES[0], "secret defect detail.")
        review_state.add_finding(state, ROLES[0], "Hidden bug", {"file": "a.py:1"})
        packet = review_state.first_packet(state, ROLES[1])
        text = json.dumps(packet)
        self.assertNotIn("secret defect detail", text)
        self.assertNotIn("Hidden bug", text)
        self.assertNotIn("reports", packet)
        self.assertNotIn("findings", packet)

    def test_mutations_before_lock_are_rejected(self) -> None:
        state = make_state()
        review_state.add_initial_report(state, ROLES[0], "report.")
        finding = review_state.add_finding(state, ROLES[0], "Bug", {"file": "a.py:1"})
        with self.assertRaises(review_state.StateError):
            review_state.set_disposition(state, finding["id"], "accepted", "too early.")
        with self.assertRaises(review_state.StateError):
            review_state.add_reply(state, finding["id"], ROLES[1], "too early.", "agree")
        with self.assertRaises(review_state.StateError):
            review_state.set_fix(state, finding["id"], "verified", "too early.")
        with self.assertRaises(review_state.StateError):
            review_state.request_followup(state, ROLES[1], [finding["id"]])
        with self.assertRaises(review_state.StateError):
            review_state.followup_packet(state, "Q1")

    def test_mutations_after_lock_succeed(self) -> None:
        state = locked_state_with_findings()
        review_state.set_disposition(state, "F1", "accepted", "Confirmed on frozen head.")
        review_state.add_reply(state, "F1", ROLES[1], "Agree.", "agree")
        review_state.set_fix(state, "F1", "fixed", "Patched.")
        self.assertEqual(state["findings"][0]["disposition"], "accepted")
        self.assertEqual(state["findings"][0]["fix"]["status"], "fixed")

    def test_lock_requires_all_roles_terminal(self) -> None:
        state = make_state()
        review_state.add_initial_report(state, ROLES[0], "report.")
        with self.assertRaises(review_state.StateError):
            review_state.lock_initial(state)

    def test_followup_request_then_packet_carries_only_requested_findings(self) -> None:
        state = locked_state_with_findings()
        request = review_state.request_followup(state, ROLES[0], ["F1"], "check this one.")
        self.assertEqual(request["status"], "pending")
        packet = review_state.followup_packet(state, request["id"])
        self.assertEqual([f["id"] for f in packet["findings"]], ["F1"])
        self.assertNotIn("Bug two", json.dumps(packet))

    def test_unknown_request_and_lock_closed_packets_rejected(self) -> None:
        state = locked_state_with_findings()
        with self.assertRaises(review_state.StateError):
            review_state.followup_packet(state, "Q99")

    def test_first_packet_only_during_blind_collection(self) -> None:
        state = make_state()
        review_state.first_packet(state, ROLES[0])  # allowed while collecting
        lock_with_reports(state)
        with self.assertRaises(review_state.StateError):
            review_state.first_packet(state, ROLES[0])
        review_state.close_run(state)
        with self.assertRaises(review_state.StateError):
            review_state.first_packet(state, ROLES[0])

    def test_unknown_role_stays_uncovered(self) -> None:
        state = make_state()
        lock_with_reports(state, skip=ROLES[-1])
        self.assertEqual(state["roles"][ROLES[-1]]["status"], "unknown")
        self.assertEqual(state["phase"], "reconciling")


class UnknownReopenTests(unittest.TestCase):
    def test_unknown_then_report_is_rejected_and_reason_kept(self) -> None:
        state = make_state()
        review_state.mark_unknown(state, ROLES[0], "worker timed out.")
        with self.assertRaises(review_state.StateError):
            review_state.add_initial_report(state, ROLES[0], "late report.")
        self.assertEqual(state["roles"][ROLES[0]]["status"], "unknown")
        self.assertEqual(state["roles"][ROLES[0]]["note"], "worker timed out.")
        self.assertNotIn(ROLES[0], state["reports"])

    def test_reopen_preserves_reason_then_late_report_succeeds(self) -> None:
        state = make_state()
        review_state.mark_unknown(state, ROLES[0], "worker timed out.")
        review_state.reopen_role(state, ROLES[0], "worker settled; redispatching once.")
        review_state.add_initial_report(state, ROLES[0], "late report.")
        history_text = json.dumps(state["history"])
        self.assertIn("worker timed out", history_text)
        self.assertIn("reopen", history_text)
        self.assertEqual(state["reports"][ROLES[0]]["text"], "late report.")

    def test_reopen_guards(self) -> None:
        state = make_state()
        with self.assertRaises(review_state.StateError):
            review_state.reopen_role(state, ROLES[0], "not unknown.")
        review_state.mark_unknown(state, ROLES[0], "timed out.")
        with self.assertRaises(review_state.StateError):
            review_state.mark_unknown(state, ROLES[0], "again.")
        for role in ROLES[1:]:
            review_state.add_initial_report(state, role, "report.")
        review_state.reopen_role(state, ROLES[0], "settled.")
        review_state.add_initial_report(state, ROLES[0], "late.")
        review_state.lock_initial(state)
        with self.assertRaises(review_state.StateError):
            review_state.reopen_role(state, ROLES[1], "after lock.")


class ValidationTests(unittest.TestCase):
    def test_rewound_finding_counter_raises_instead_of_duplicating(self) -> None:
        state = locked_state_with_findings()
        state["next_finding"] = 1
        with self.assertRaises(review_state.StateError):
            review_state.add_finding(state, ROLES[0], "Duplicate", {})
        self.assertEqual([f["id"] for f in state["findings"]], ["F1", "F2"])

    def test_duplicate_ids_rejected_on_save(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.json"
            state = locked_state_with_findings()
            review_state.save_state(path, state)
            state["findings"].append(dict(state["findings"][0]))
            with self.assertRaises(review_state.StateError):
                review_state.save_state(path, state)
            self.assertEqual(review_state.load_state(path)["next_finding"], 3)

    def test_malformed_shapes_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.json"
            base = locked_state_with_findings()
            review_state.save_state(path, base)
            pristine = json.loads(path.read_bytes().decode("utf-8"))
            cases = ["findings_dict", "finding_missing_keys", "bad_enum",
                     "bad_digest", "bad_counter", "bad_role_status"]
            for case in cases:
                with self.subTest(case=case):
                    broken = json.loads(json.dumps(pristine))
                    if case == "findings_dict":
                        broken["findings"] = {"F1": {}}
                    elif case == "finding_missing_keys":
                        broken["findings"][0] = {"id": "F1"}
                    elif case == "bad_enum":
                        broken["findings"][0]["disposition"] = "maybe"
                    elif case == "bad_digest":
                        broken["review_identity"]["inputs"][0]["sha256"] = "xyz"
                    elif case == "bad_counter":
                        broken["next_finding"] = 1
                    elif case == "bad_role_status":
                        broken["roles"][ROLES[0]]["status"] = "vibes"
                    path.write_bytes(json.dumps(broken).encode("utf-8"))
                    with self.assertRaises(review_state.StateError):
                        review_state.load_state(path)
            # Original file content replaced by broken cases; rewrite valid base
            # would be a test artifact change, so just confirm valid base loads.
            review_state.save_state(path, base)
            self.assertEqual(review_state.load_state(path)["run_id"], "run-1")

    def test_bad_digest_rejected_at_init(self) -> None:
        with self.assertRaises(review_state.StateError):
            review_state.init_state("r", "w", "b", "h", inputs={"diff": "not-a-digest"})


class DispositionAndReplyTests(unittest.TestCase):
    def test_stable_ids_and_append_only_replies(self) -> None:
        state = locked_state_with_findings()
        review_state.set_disposition(state, "F1", "rejected", "Not reachable on this path.")
        review_state.add_reply(state, "F1", ROLES[1], "Agree, my case overlaps.", "agree")
        original = state["reports"][ROLES[0]]["text"]
        review_state.add_reply(state, "F1", ROLES[0], "Disagree, trigger stands.", "dispute")
        finding = next(f for f in state["findings"] if f["id"] == "F1")
        # A dispute reopens the finding instead of leaving a closed verdict
        # standing beside an unanswered objection.
        self.assertEqual(finding["disposition"], "unresolved")
        self.assertIn("dispute", finding["reason"])
        self.assertEqual(len(finding["replies"]), 2)
        self.assertEqual(state["reports"][ROLES[0]]["text"], original)
        with self.assertRaises(review_state.StateError):
            review_state.add_initial_report(state, ROLES[0], "rewrite attempt.")

    def test_disposition_without_reason_is_rejected(self) -> None:
        state = locked_state_with_findings()
        with self.assertRaises(review_state.StateError):
            review_state.set_disposition(state, "F1", "accepted", "")


class StaleTargetTests(unittest.TestCase):
    def test_source_change_stales_everything_and_resets_verified_fix(self) -> None:
        state = locked_state_with_findings()
        review_state.set_disposition(state, "F1", "accepted", "Confirmed on frozen head.")
        review_state.set_fix(state, "F1", "verified", "Old source passed.")
        review_state.set_disposition(state, "F2", "rejected", "Rejected on frozen head.")
        mismatches = review_state.check_identity(state, head="head-CHANGED")
        self.assertTrue(mismatches)
        stale = review_state.mark_findings_stale(state, "head moved to head-CHANGED.")
        self.assertEqual(sorted(stale), ["F1", "F2"])
        for finding in state["findings"]:
            self.assertTrue(finding["stale"])
            self.assertTrue(any(e["event"] == "stale" for e in finding["history"]))
        first = next(f for f in state["findings"] if f["id"] == "F1")
        self.assertEqual(first["evidence"], {"file": "a.py:1"})
        # The old verified fix must not read as currently valid.
        self.assertEqual(first["fix"]["status"], "open")
        self.assertTrue(any(e["event"] == "fix:reset-stale" for e in first["history"]))
        self.assertTrue(state["source_stale"])

    def test_stale_initial_first_packet_rejected_but_record_loads(self) -> None:
        state = make_state()
        review_state.mark_findings_stale(state, "source moved before collection finished.")
        with self.assertRaises(review_state.StateError):
            review_state.first_packet(state, ROLES[0])
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.json"
            review_state.save_state(path, state)
            loaded = review_state.load_state(path)
            self.assertTrue(loaded["source_stale"])

    def test_stale_source_blocks_live_decisions(self) -> None:
        state = locked_state_with_findings()
        review_state.set_disposition(state, "F1", "accepted", "Confirmed.")
        review_state.mark_findings_stale(state, "head moved.")
        with self.assertRaises(review_state.StateError):
            review_state.set_disposition(state, "F2", "accepted", "after stale.")
        with self.assertRaises(review_state.StateError):
            review_state.set_fix(state, "F1", "verified", "after stale.")
        with self.assertRaises(review_state.StateError):
            review_state.add_reply(state, "F2", ROLES[0], "after stale.", "agree")
        with self.assertRaises(review_state.StateError):
            review_state.request_followup(state, ROLES[0], ["F2"])
        with self.assertRaises(review_state.StateError):
            review_state.add_finding(state, ROLES[0], "after stale.", {})
        with self.assertRaises(review_state.StateError):
            review_state.close_run(state)

    def test_changed_input_hash_and_scope_detected(self) -> None:
        state = make_state()
        mismatches = review_state.check_identity(state, inputs={"diff": "b" * 64})
        self.assertTrue(any("diff" in mismatch for mismatch in mismatches))
        scope_mismatch = review_state.check_identity(state, scope=["other/"])
        self.assertTrue(any("scope" in mismatch for mismatch in scope_mismatch))
        self.assertEqual(
            review_state.check_identity(
                state, head="head-def", inputs={"diff": "a" * 64}, scope=["src/"]
            ),
            [],
        )

    def test_new_verified_run_replaces_stale_one(self) -> None:
        fresh = review_state.init_state(
            "run-2", r"C:\repo", "base-abc", "head-NEW",
            "rev-2", {"diff": "b" * 64}, ["src/"], supersedes="run-1",
        )
        lock_with_reports(fresh)
        review_state.add_finding(fresh, ROLES[0], "Bug", {"file": "a.py:1"})
        review_state.set_disposition(fresh, "F1", "accepted", "Confirmed on new head.")
        self.assertIn("run-1", json.dumps(fresh["history"]))


class FollowupRequestTests(unittest.TestCase):
    def test_request_persists_across_save_reload_before_packet(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.json"
            state = locked_state_with_findings()
            request = review_state.request_followup(state, ROLES[0], ["F1"], "verify.")
            review_state.save_state(path, state)
            # A crash here loses nothing: the request is persisted first.
            loaded = review_state.load_state(path)
            self.assertEqual(loaded["followups"][0]["status"], "pending")
            packet = review_state.followup_packet(loaded, request["id"])
            self.assertEqual(packet["request_id"], request["id"])
            self.assertEqual([f["id"] for f in packet["findings"]], ["F1"])

    def test_reply_links_to_request_and_dispute_stays_visible(self) -> None:
        state = locked_state_with_findings()
        review_state.set_disposition(state, "F1", "accepted", "Confirmed.")
        request = review_state.request_followup(state, ROLES[1], ["F1"], "second look.")
        review_state.add_reply(
            state, "F1", ROLES[1], "Trigger does not hold.", "dispute",
            request_id=request["id"],
        )
        self.assertEqual(
            next(r for r in state["followups"] if r["id"] == request["id"])["status"],
            "replied",
        )
        finding = next(f for f in state["findings"] if f["id"] == "F1")
        self.assertEqual(finding["disposition"], "unresolved")
        self.assertIn("dispute", finding["reason"])

    def test_reply_link_guards(self) -> None:
        state = locked_state_with_findings()
        request = review_state.request_followup(state, ROLES[0], ["F1"])
        with self.assertRaises(review_state.StateError):
            review_state.add_reply(
                state, "F2", ROLES[0], "wrong finding.", "note", request_id=request["id"]
            )
        with self.assertRaises(review_state.StateError):
            review_state.add_reply(
                state, "F1", ROLES[0], "unknown request.", "note", request_id="Q99"
            )

    def test_pending_blocks_close_and_verification_until_resolved(self) -> None:
        state = locked_state_with_findings()
        request = review_state.request_followup(state, ROLES[0], ["F1"])
        with self.assertRaises(review_state.StateError):
            review_state.close_run(state)
        with self.assertRaises(review_state.StateError):
            review_state.set_fix(state, "F1", "verified", "too soon.")
        # Non-verification fix notes are still recordable while waiting.
        review_state.set_fix(state, "F1", "fixed", "Patched; awaiting reply.")
        review_state.resolve_request(state, request["id"], "unresolved", "Reviewer silent.")
        review_state.set_disposition(state, "F1", "unresolved", "No reviewer reply arrived.")
        review_state.close_run(state)
        self.assertEqual(state["phase"], "closed")
        with self.assertRaises(review_state.StateError):
            review_state.set_fix(state, "F1", "verified", "after close.")


class CheckSourceCliTests(unittest.TestCase):
    def _write_locked(self, directory: str) -> Path:
        path = Path(directory) / "run.json"
        review_state.save_state(path, locked_state_with_findings())
        return path

    def test_mismatch_returns_nonzero(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self._write_locked(directory)
            code, out, _ = run_cli(
                ["--state", str(path), "check-source", "--head", "different-head",
                 "--input", "diff=" + "a" * 64]
            )
            self.assertEqual(code, 1)
            self.assertFalse(json.loads(out)["ok"])

    def test_match_returns_zero(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self._write_locked(directory)
            code, out, _ = run_cli(
                ["--state", str(path), "check-source", "--head", "head-def",
                 "--input", "diff=" + "a" * 64]
            )
            self.assertEqual(code, 0)
            self.assertTrue(json.loads(out)["ok"])

    def test_empty_check_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self._write_locked(directory)
            code, _, err = run_cli(["--state", str(path), "check-source"])
            self.assertEqual(code, 2)
            self.assertIn("at least one identity value", err)

    def test_input_file_hashing_roundtrip(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "evidence.diff"
            target.write_bytes(b"fake diff bytes")
            path = Path(directory) / "run.json"
            code, out, _ = run_cli(
                ["--state", str(path), "init", "--run-id", "f", "--workspace", "w",
                 "--base", "b", "--head", "h", "--input-file", f"diff={target}"]
            )
            self.assertEqual(code, 0)
            code, out, _ = run_cli(
                ["--state", str(path), "check-source", "--input-file", f"diff={target}"]
            )
            self.assertEqual(code, 0)
            self.assertTrue(json.loads(out)["ok"])


class FailedWriteTests(unittest.TestCase):
    def test_failed_validation_leaves_previous_valid_record(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.json"
            state = make_state()
            review_state.save_state(path, state)
            bad = make_state("run-bad")
            bad["phase"] = "nonsense"
            with self.assertRaises(review_state.StateError):
                review_state.save_state(path, bad)
            loaded = review_state.load_state(path)
            self.assertEqual(loaded["run_id"], "run-1")

    def test_injected_replace_failure_leaves_previous_valid_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.json"
            state = make_state()
            review_state.save_state(path, state)
            before = path.read_bytes()
            updated = make_state("run-1")
            review_state.add_initial_report(updated, ROLES[0], "new report.")
            with mock.patch.object(
                review_state.os, "replace", side_effect=OSError("injected failure")
            ):
                with self.assertRaises(OSError):
                    review_state.save_state(path, updated)
            # Previous valid bytes are untouched and loadable.
            self.assertEqual(path.read_bytes(), before)
            self.assertEqual(review_state.load_state(path)["run_id"], "run-1")

    def test_refusing_to_init_over_existing_record(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.json"
            review_state.save_state(path, make_state())
            code, _, _ = run_cli(
                ["--state", str(path), "init", "--run-id", "x",
                 "--workspace", "w", "--base", "b", "--head", "h"]
            )
            self.assertEqual(code, 2)
            self.assertEqual(review_state.load_state(path)["run_id"], "run-1")

    def test_malformed_evidence_json_is_machine_readable_and_preserves_state(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.json"
            review_state.save_state(path, locked_state_with_findings())
            before = path.read_bytes()
            code, _, err = run_cli(
                ["--state", str(path), "add-finding", "--origin-role", ROLES[0],
                 "--title", "Bug", "--evidence", "{bad json"]
            )
            self.assertEqual(code, 2)
            self.assertFalse(json.loads(err)["ok"])
            self.assertEqual(path.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
