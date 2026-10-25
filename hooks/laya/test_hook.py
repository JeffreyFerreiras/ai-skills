import io
import json
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import hook


class HookTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / 'config.json').write_text('{"enabled":true,"port":18147,"mode":"advisory"}')
        self.event = {
            'hook_event_name': 'UserPromptSubmit',
            'session_id': 'private-session-id',
            'prompt': 'private-prompt-marker',
        }

    def run_hook(self, result=None, error=None):
        captured = io.StringIO()
        stdin = SimpleNamespace(buffer=io.BytesIO(json.dumps(self.event).encode()))
        with patch.object(hook, 'ROOT', self.root), patch.object(hook.sys, 'stdin', stdin):
            with patch.object(hook, 'request_route', return_value=result, side_effect=error) as request:
                with redirect_stdout(captured):
                    hook.main()
        self.output = json.loads(captured.getvalue()) if captured.getvalue() else None
        log = self.root / 'routing.jsonl'
        record = json.loads(log.read_text()) if log.exists() else None
        return record, request

    def test_success_logs_no_prompt_or_session_text(self):
        record, _ = self.run_hook(result={'status': 'ok', 'route': 'direct', 'truncated': False})
        self.assertEqual(record['route'], 'direct')
        self.assertNotIn('private-prompt-marker', json.dumps(record))
        self.assertNotIn('private-session-id', json.dumps(record))
        self.assertTrue(record['context_returned'])
        context = self.output['hookSpecificOutput']
        self.assertEqual(context['hookEventName'], 'UserPromptSubmit')
        self.assertIn('Laya routing suggestion: direct.', context['additionalContext'])
        self.assertNotIn('private-prompt-marker', json.dumps(self.output))
        self.assertEqual(set(self.output), {'hookSpecificOutput'})
        self.assertEqual(set(context), {'hookEventName', 'additionalContext'})

    def test_timeout_continues_silently_and_does_not_log_error_message(self):
        record, _ = self.run_hook(error=TimeoutError('private-prompt-marker'))
        self.assertEqual(record['status'], 'unavailable')
        self.assertEqual(record['error_type'], 'TimeoutError')
        self.assertNotIn('private-prompt-marker', json.dumps(record))
        self.assertIsNone(self.output)

    def test_disabled_hook_makes_no_request(self):
        (self.root / 'config.json').write_text('{"enabled":false}')
        record, request = self.run_hook()
        self.assertIsNone(record)
        self.assertIsNone(self.output)
        request.assert_not_called()

    def test_unrelated_events_are_ignored(self):
        self.event['hook_event_name'] = 'PreToolUse'
        record, request = self.run_hook()
        self.assertIsNone(record)
        self.assertIsNone(self.output)
        request.assert_not_called()

    def test_long_prompts_are_skipped(self):
        self.event['prompt'] = 'x' * 12001
        record, request = self.run_hook()
        self.assertEqual(record['status'], 'prompt_too_long')
        self.assertIsNone(self.output)
        request.assert_not_called()

    def test_truncation_is_preserved(self):
        record, _ = self.run_hook(result={'status': 'truncated', 'route': 'direct', 'truncated': True})
        self.assertTrue(record['truncated'])
        self.assertEqual(record['status'], 'truncated')
        self.assertFalse(record['context_returned'])
        self.assertIsNone(self.output)

    def test_observation_mode_still_logs_without_guidance(self):
        (self.root / 'config.json').write_text('{"enabled":true,"port":18147,"mode":"observe"}')
        record, _ = self.run_hook(result={'status': 'ok', 'route': 'direct', 'truncated': False})
        self.assertEqual(record['mode'], 'observe')
        self.assertFalse(record['context_returned'])
        self.assertIsNone(self.output)

    def test_each_route_returns_its_own_bounded_guidance(self):
        for route in hook.ROUTES:
            with self.subTest(route=route):
                log = self.root / 'routing.jsonl'
                if log.exists():
                    log.unlink()
                self.run_hook(result={'status': 'ok', 'route': route, 'truncated': False})
                context = self.output['hookSpecificOutput']['additionalContext']
                self.assertIn(hook.GUIDANCE[route], context)
                self.assertLess(len(context), 900)

    def test_invalid_model_labels_are_rejected(self):
        (self.root / 'token.txt').write_text('test-token')
        response = unittest.mock.MagicMock()
        response.__enter__.return_value.read.return_value = json.dumps({
            'answers': {'route': {'choice': 'unexpected', 'probabilities': {'unexpected': 1}}}
        }).encode()
        opener = unittest.mock.MagicMock()
        opener.open.return_value = response
        with patch.object(hook, 'ROOT', self.root), patch.object(hook, 'build_opener', return_value=opener):
            with self.assertRaises(ValueError):
                hook.request_route('sample', {'port': 18147})


if __name__ == '__main__':
    unittest.main()
