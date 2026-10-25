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
sys.path.insert(0, str(ROOT / 'scripts'))
import laya_routing as policy


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), ROOT / 'scripts' / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


hook = load('laya-hook')
service = load('laya-service')


def prediction(route='file_finding', probability=0.94, truncated=False):
    return {'answers': {'route': {'choice': route, 'probabilities': {
        name: probability if name == route else (1 - probability) / 3 for name in policy.PROFILES}}},
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

    def run_hook(self, event=None, raw=None, subagent=False, **env):
        if raw is None:
            raw = json.dumps(event or {'hook_event_name': 'UserPromptSubmit', 'prompt': 'Rename café.'}).encode()
        output = io.StringIO()
        with patch.dict(os.environ, {'LAYA_CODEX_PORT': str(self.port), 'LAYA_CODEX_DISABLED': '0', **env}), \
             patch.object(sys, 'stdin', SimpleNamespace(buffer=io.BytesIO(raw))), patch.object(sys, 'stdout', output):
            self.assertEqual(0, hook.main(subagent=subagent))
        return json.loads(output.getvalue())

    def test_live_client_contract_and_resident_requests(self):
        for _ in range(2):
            result = self.run_hook()
            context = result['hookSpecificOutput']['additionalContext']
            self.assertIn('gpt-6-luna at low effort', context)
            self.assertIn('route_subagent.py', context)
            self.assertIn('review-only limits', context)
            self.assertIn('do not route again', context)
        status, health = self.request('GET', '/health')
        self.assertEqual(200, status)
        self.assertTrue(health['ready'])
        self.assertEqual(2, health['requests_completed'])
        self.assertEqual('127.0.0.1', self.server.server_address[0])

    def test_each_profile_uses_the_shared_model_and_effort(self):
        expected = {
            'file_finding': ('gpt-6-luna', 'low', 'explorer'),
            'bounded_reading': ('gpt-6-luna', 'medium', 'explorer'),
            'coding': ('gpt-6.1-sol', 'high', 'worker'),
            'complex': ('gpt-6-astra', 'high', 'default'),
        }
        for route, settings in expected.items():
            with self.subTest(route=route):
                self.agent.predict = lambda *_, r=route: prediction(r)
                result = service.recommend(self.agent, 'task')
                self.assertEqual('selected', result['status'])
                self.assertEqual(policy.spawn_settings(route), result['spawn'])
                self.assertEqual(settings, tuple(result['spawn'][key] for key in ('model', 'reasoning_effort', 'agent_type')))
                self.assertEqual('none', result['spawn']['fork_turns'])

    def test_confidence_and_margin_boundaries(self):
        item = prediction(probability=0.60)
        scores = item['answers']['route']['probabilities']
        scores.update(complex=0.45)
        self.assertEqual('selected', policy.select_prediction(item)['status'])
        scores['complex'] = 0.451
        self.assertEqual('fallback', policy.select_prediction(item)['status'])
        item = prediction(probability=0.599)
        self.assertEqual('fallback', policy.select_prediction(item)['status'])

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
        for body, headers, expected in [(b'x' * (service.MAX_BODY_BYTES + 1), {}, 413), (b'bad', {}, 400),
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

    def test_uncertain_and_invalid_probabilities_fail_open(self):
        invalid = [prediction(probability=0.40), prediction(truncated=None),
                   {'answers': {}}, {'usage': {'truncated': False}, 'answers': []}]
        for score in (True, float('nan'), float('inf'), -1, 2):
            item = prediction()
            item['answers']['route']['probabilities']['file_finding'] = score
            invalid.append(item)
        conflict = prediction()
        conflict['answers']['route']['probabilities']['complex'] = 0.99
        invalid.append(conflict)
        for item in invalid:
            with self.subTest(item=item):
                self.agent.predict = lambda *_, p=item: p
                self.assertEqual({}, self.run_hook())

    def test_explicit_packet_reuses_the_same_resident_server(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'config.json').write_text(json.dumps({'subagent_routing_enabled': True}))
            with patch.object(hook, 'ROOT', root):
                for route in policy.PROFILES:
                    self.agent.predict = lambda *_, r=route: prediction(r)
                    result = self.run_hook(raw=json.dumps({'task': 'Context: inspect files. ' * 100}).encode(), subagent=True)
                    self.assertEqual('selected', result['status'])
                    self.assertEqual(policy.spawn_settings(route), result['spawn'])
                    self.assertFalse(result['executed'])
                    self.assertTrue(result['decision_id'])
                self.assertEqual(4, self.server.requests_completed)
                self.assertFalse(list(root.glob('*.jsonl')))

    def test_subagent_failures_do_not_expose_tasks_or_start_processes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = root / 'config.json'
            config.write_text('{"subagent_routing_enabled":true}')
            with patch.object(hook, 'ROOT', root), patch.object(hook, 'classify') as classify:
                for raw in (b'null', b'[]', b'{}', b'bad', b'x' * (hook.MAX_TASK_BYTES + 1),
                            json.dumps({'task': 'x' * 4001}).encode()):
                    self.assertEqual('fallback', self.run_hook(raw=raw, subagent=True)['status'])
                classify.assert_not_called()
                classify.side_effect = TimeoutError('private-packet')
                result = self.run_hook(raw=b'{"task":"private-packet"}', subagent=True)
                self.assertEqual('unavailable', result['reason'])
                self.assertNotIn('private-packet', json.dumps(result))
                config.write_text('{"subagent_routing_enabled":false}')
                result = self.run_hook(raw=b'{"task":"find files"}', subagent=True)
                self.assertEqual('disabled', result['reason'])

    def test_unicode_packet_reaches_resident_inference_within_transport_limit(self):
        task = 'Inspect this bounded reading packet: ' + '\U0001f680' * 3000
        raw = json.dumps({'task': task}, ensure_ascii=False).encode('utf-8')
        self.assertLessEqual(len(task), 4000)
        self.assertLessEqual(len(raw), hook.MAX_TASK_BYTES)
        escaped = json.dumps({'prompt': task, 'kind': 'subagent'}).encode('utf-8')
        self.assertGreater(len(escaped), service.MAX_BODY_BYTES)
        received = []
        self.agent.predict = lambda request, _: received.append(request['request']) or prediction()
        with tempfile.TemporaryDirectory(prefix='laya unicode ') as directory:
            root = Path(directory)
            (root / 'config.json').write_text('{"subagent_routing_enabled":true}', encoding='utf-8')
            with patch.object(hook, 'ROOT', root):
                result = self.run_hook(raw=raw, subagent=True)
        self.assertEqual('selected', result['status'])
        self.assertEqual([task], received)
        self.assertEqual(1, self.server.requests_completed)
        self.assertFalse(result['executed'])

    def test_untrusted_spawn_fields_are_rejected(self):
        for route in policy.PROFILES:
            result = {'status': 'selected', 'route': route, 'spawn': policy.spawn_settings(route)}
            result['spawn']['model'] = 'arbitrary-model'
            self.assertEqual({}, hook.context_from(result))
        for value in ([], {}, None):
            self.assertEqual({}, hook.context_from({'status': 'fallback', 'reason': value}))

    def test_packet_limit_is_distinct_from_prompt_limit(self):
        body = json.dumps({'prompt': 'x' * 1201}).encode()
        self.assertEqual(400, self.request('POST', '/classify', body)[0])
        body = json.dumps({'prompt': 'x' * 4001, 'kind': 'subagent'}).encode()
        self.assertEqual(400, self.request('POST', '/classify', body)[0])
        body = json.dumps({'prompt': 'x', 'kind': 'arbitrary_questions'}).encode()
        self.assertEqual(400, self.request('POST', '/classify', body)[0])
        self.assertEqual(0, self.server.requests_completed)

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
