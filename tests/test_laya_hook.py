from __future__ import annotations

import http.client
import importlib.util
import io
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import threading
from types import SimpleNamespace
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), ROOT / 'scripts' / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


hook = load('laya-hook')
service = load('laya-service')


def prediction(task='editing', difficulty='routine', truncated=False):
    return {'answers': {'task_type': {'choice': task}, 'difficulty': {'choice': difficulty}},
            'usage': {'truncated': truncated}}


class LayaTests(unittest.TestCase):
    def setUp(self):
        self.agent = SimpleNamespace(predict=lambda *_: prediction())
        self.server = service.ClassifierServer(0, lambda prompt: service.recommend(self.agent, prompt))
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.addCleanup(self.server.server_close)
        self.addCleanup(self.server.shutdown)
        self.port = self.server.server_address[1]

    def request(self, method, path, body=None, headers=None):
        connection = http.client.HTTPConnection('127.0.0.1', self.port, timeout=2)
        try:
            connection.request(method, path, body, headers or {})
            response = connection.getresponse()
            return response.status, json.loads(response.read())
        finally:
            connection.close()

    def run_hook(self, event=None, raw=None, **env):
        if raw is None:
            raw = json.dumps(event or {'hook_event_name': 'UserPromptSubmit', 'prompt': 'Rename café.'}).encode()
        output = io.StringIO()
        with patch.dict(os.environ, {'LAYA_CODEX_PORT': str(self.port), 'LAYA_CODEX_DISABLED': '0', **env}), \
             patch.object(sys, 'stdin', SimpleNamespace(buffer=io.BytesIO(raw))), patch.object(sys, 'stdout', output):
            self.assertEqual(0, hook.main())
        return json.loads(output.getvalue())

    def test_live_client_contract_and_resident_requests(self):
        for _ in range(2):
            result = self.run_hook()
            context = result['hookSpecificOutput']['additionalContext']
            self.assertIn('gpt-6-luna and fork_turns=none', context)
            self.assertIn('review-only limits', context)
            self.assertIn('do not route again', context)
        status, health = self.request('GET', '/health')
        self.assertEqual(200, status)
        self.assertTrue(health['ready'])
        self.assertEqual(2, health['requests_completed'])
        self.assertEqual('127.0.0.1', self.server.server_address[0])

    def test_model_tiers_and_debugging_architecture_floor(self):
        for task, difficulty, model in [('editing', 'routine', 'gpt-6-luna'),
                                      ('debugging', 'routine', 'gpt-6.1-sol'),
                                      ('architecture', 'routine', 'gpt-6.1-sol'),
                                      ('implementation', 'standard', 'gpt-6.1-sol'),
                                      ('architecture', 'demanding', 'gpt-6-astra')]:
            with self.subTest(task=task, difficulty=difficulty):
                self.agent.predict = lambda *_, t=task, d=difficulty: prediction(t, d)
                self.assertEqual(model, service.recommend(self.agent, 'task')['model'])

    def test_abstains_on_truncated_and_missing_metadata(self):
        for value in (True, None, 0):
            self.agent.predict = lambda *_, v=value: prediction(truncated=v)
            self.assertEqual({}, self.run_hook())

    def test_invalid_events_and_disable_do_not_infer(self):
        for raw in (b'null', b'[]', b'{}', b'not JSON', b'\xff', b'x' * (hook.MAX_EVENT_BYTES + 1)):
            self.assertEqual({}, self.run_hook(raw=raw))
        for prompt in (' ', [], 'x' * 1201):
            self.assertEqual({}, self.run_hook({'hook_event_name': 'UserPromptSubmit', 'prompt': prompt}))
        self.assertEqual({}, self.run_hook(LAYA_CODEX_DISABLED='1'))
        for port in ('bad', '0', '65536'):
            self.assertEqual({}, self.run_hook(LAYA_CODEX_PORT=port))
        self.assertEqual(0, self.server.requests_completed)

    def test_bad_or_inconsistent_server_output(self):
        for result in (None, [], {}, {'status': 'abstain_truncated'},
                       {'status': 'experimental_suggestion', 'task_type': [], 'difficulty': 'routine', 'model': 'x'},
                       {'status': 'experimental_suggestion', 'task_type': 'debugging', 'difficulty': 'routine', 'model': 'gpt-6-luna'}):
            self.assertEqual({}, hook.context_from(result))

    def test_bounded_server_payload_and_inference_failure(self):
        for body, headers, expected in [(b'x' * 8193, {}, 413), (b'bad', {}, 400),
                                        (b'{"prompt":""}', {}, 400), (b'{}', {'Transfer-Encoding': 'chunked'}, 413)]:
            try:
                self.assertEqual(expected, self.request('POST', '/classify', body, headers)[0])
            except (ConnectionAbortedError, ConnectionResetError):
                # Windows may reset when rejected body bytes remain unread.
                # Either outcome must reject the body before inference.
                self.assertEqual(413, expected)
            self.assertEqual(0, self.server.requests_completed)
        self.assertEqual(404, self.request('POST', '/other', b'{}')[0])
        self.server.classifier = lambda _: (_ for _ in ()).throw(RuntimeError('private'))
        self.assertEqual({}, self.run_hook())

    def test_http_failure_timeout_response_limits_and_proxy(self):
        with patch.object(hook.http.client, 'HTTPConnection') as factory:
            connection = factory.return_value
            response = connection.getresponse.return_value
            for status, body in ((302, b'{}'), (503, b'{}'), (200, b'not JSON'),
                                 (200, b'x' * (hook.MAX_RESPONSE_BYTES + 1))):
                response.status, response.read.return_value = status, body
                self.assertEqual({}, self.run_hook(HTTP_PROXY='http://invalid', ALL_PROXY='http://invalid'))
            for error in (TimeoutError(), ConnectionRefusedError(), http.client.BadStatusLine('bad')):
                connection.getresponse.side_effect = error
                self.assertEqual({}, self.run_hook())
            self.assertEqual(7, connection.close.call_count)
            factory.assert_called_with('127.0.0.1', self.port, timeout=0.75)

    def test_template_launches_from_nested_path_with_spaces(self):
        template = json.loads((ROOT / 'hooks/laya-hooks.json').read_text(encoding='utf-8'))
        handler = template['hooks']['UserPromptSubmit'][0]['hooks'][0]
        scratch = ROOT / 'tmp'
        scratch.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='laya hook ', dir=scratch) as directory:
            for field in ('command', 'commandWindows'):
                command = shlex.split(handler[field])
                command[0] = sys.executable
                result = subprocess.run(command, input='{}', text=True, capture_output=True,
                                        cwd=directory, timeout=5,
                                        env=dict(os.environ, LAYA_CODEX_DISABLED='1'))
                self.assertEqual(0, result.returncode, result.stderr)
                self.assertEqual({}, json.loads(result.stdout))


if __name__ == '__main__':
    unittest.main()
