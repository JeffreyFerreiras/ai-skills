import io
import json
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
from urllib.error import URLError

import hook
import route_subagent as router


def result(route, probability=0.94, status="ok"):
    return {"status": status, "route": route, "truncated": status == "truncated",
            "probabilities": {name: probability if name == route else (1 - probability) / 3
                              for name in router.PROFILES}}


class SubagentRoutingTests(unittest.TestCase):
    def test_routes_return_actual_model_and_effort_fields(self):
        expected = {
            "file_finding": ("gpt-6-luna", "low", "explorer"),
            "bounded_reading": ("gpt-6-luna", "medium", "explorer"),
            "coding": ("gpt-6.1-sol", "high", "worker"),
            "complex": ("gpt-6-astra", "high", "default"),
        }
        for route, settings in expected.items():
            with self.subTest(route=route):
                selection = router.select_subagent(result(route))
                self.assertEqual("selected", selection["status"])
                spawn = selection["spawn"]
                self.assertEqual(settings, (spawn["model"], spawn["reasoning_effort"], spawn["agent_type"]))
                self.assertEqual("none", spawn["fork_turns"])

    def test_uncertain_truncated_and_conflicting_scores_do_not_delegate(self):
        conflicting = result("coding")
        conflicting["probabilities"]["complex"] = 0.99
        close = result("coding", 0.60)
        close["probabilities"]["complex"] = 0.50
        for response in (result("coding", 0.40), result("complex", status="truncated"), conflicting, close):
            with self.subTest(response=response):
                selection = router.select_subagent(response)
                self.assertEqual("fallback", selection["status"])
                self.assertNotIn("spawn", selection)

    def test_cold_start_then_route(self):
        with patch.object(hook, "request_route", side_effect=[
            URLError(ConnectionRefusedError()), result("file_finding"),
        ]) as request, patch.object(router.subprocess, "run") as start:
            self.assertEqual("file_finding", router.classify("Find files", {"powershell": "pwsh"})["route"])
            self.assertEqual(2, request.call_count)
            start.assert_called_once()
            self.assertEqual("Start", start.call_args.args[0][-1])

    def test_running_server_timeout_does_not_start_another_process(self):
        with patch.object(hook, "request_route", side_effect=TimeoutError), patch.object(router.subprocess, "run") as start:
            with self.assertRaises(TimeoutError):
                router.classify("Find files", {})
            start.assert_not_called()

    def test_windows_connection_timeout_checks_service_before_retry(self):
        with patch.object(hook, "request_route", side_effect=[
            URLError(TimeoutError()), result("file_finding"),
        ]), patch.object(router.subprocess, "run") as start:
            self.assertEqual("file_finding", router.classify("Find files", {"powershell": "pwsh"})["route"])
            start.assert_called_once()

    def run_cli(self, packet, response=None, enabled=True, error=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "config.json").write_text(json.dumps({"subagent_routing_enabled": enabled}))
            stdin = SimpleNamespace(buffer=io.BytesIO(packet))
            stdout = io.StringIO()
            with patch.object(hook, "ROOT", root), patch.object(router.sys, "stdin", stdin), \
                    patch.object(router, "classify", return_value=response, side_effect=error) as classify:
                with redirect_stdout(stdout):
                    router.main()
            return json.loads(stdout.getvalue()), (root / "routing.jsonl").read_text(), classify.call_count

    def test_logs_neither_task_nor_claimed_execution(self):
        output, log, _ = self.run_cli(b'{"task":"private-marker"}', result("file_finding"))
        self.assertEqual("selected", output["status"])
        self.assertNotIn("private-marker", log)
        self.assertFalse(json.loads(log)["executed"])
        self.assertEqual(output["decision_id"], json.loads(log)["decision_id"])

    def test_disabled_invalid_or_long_packet_does_not_call_classifier(self):
        for packet, enabled in ((b'{}', True), (b'x' * 16385, True), (b'{}', False)):
            with self.subTest(enabled=enabled, packet_bytes=len(packet)):
                output, _, calls = self.run_cli(packet, enabled=enabled)
                self.assertEqual("fallback", output["status"])
                self.assertEqual(0, calls)

    def test_failure_does_not_expose_exception_content(self):
        output, log, _ = self.run_cli(b'{"task":"find files"}', error=RuntimeError("private-token"))
        self.assertEqual("unavailable", output["reason"])
        self.assertNotIn("private-token", log)


if __name__ == "__main__":
    unittest.main()
