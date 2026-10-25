import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stdout

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('laya_install', ROOT / 'hooks/laya/install.py')
installer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(installer)


@unittest.skipUnless(os.name == 'nt', 'Windows profile installer')
class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix='laya client ')
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.home = self.root / 'codex home'
        self.runtime = self.home / 'laya-hook'
        self.service = self.root / 'resident service'

    def install(self, *extra):
        args = installer.parse_args(['--codex-home', str(self.home), '--service-root', str(self.service), *extra])
        with redirect_stdout(io.StringIO()):
            installer.install(args)

    def test_dry_run_has_no_side_effects(self):
        self.install('--dry-run')
        self.assertFalse(self.home.exists())
        self.assertFalse(self.service.exists())

    def test_migration_removes_only_owned_hooks_and_preserves_runtime_data(self):
        self.runtime.mkdir(parents=True)
        config = {'port': 18147, 'model_path': 'external weights', 'custom': 'keep'}
        (self.runtime / 'config.json').write_text(json.dumps(config))
        for name in ('token.txt', 'routing.jsonl', 'server.py', 'manage.ps1'):
            (self.runtime / name).write_text('retain original data')
        command = lambda path: subprocess.list2cmdline([sys.executable, str(path)])
        old = {'type': 'command', 'command': command(self.runtime / 'hook.py')}
        resident = {'type': 'command', 'command': command(self.service / 'laya-hook.py')}
        unrelated = {'type': 'command', 'command': 'python audit.py ' + str(self.runtime / 'hook.py')}
        startup = {'type': 'command', 'command': subprocess.list2cmdline([
            'pwsh', '-NoProfile', '-NonInteractive', '-File', str(self.runtime / 'start-hook.ps1')])}
        document = {'description': 'preserve metadata', 'hooks': {
            'UserPromptSubmit': [{'matcher': '*', 'hooks': [old, resident, unrelated]}],
            'SessionStart': [{'hooks': [startup, {'type': 'command', 'command': 'keep-startup'}]}],
            'Stop': []}}
        (self.home / 'hooks.json').write_text(json.dumps(document))
        (self.home / 'AGENTS.md').write_text('Existing guidance.\n')
        self.install('--integration', 'both')
        self.install('--integration', 'both')
        merged = json.loads((self.home / 'hooks.json').read_text())
        prompts = merged['hooks']['UserPromptSubmit']
        self.assertEqual(2, len(prompts))
        self.assertEqual([unrelated], prompts[0]['hooks'])
        self.assertEqual('*', prompts[0]['matcher'])
        self.assertEqual([{'type': 'command', 'command': 'keep-startup'}], merged['hooks']['SessionStart'][0]['hooks'])
        self.assertEqual([], merged['hooks']['Stop'])
        self.assertEqual('preserve metadata', merged['description'])
        for name in ('token.txt', 'routing.jsonl', 'server.py', 'manage.ps1'):
            self.assertEqual('retain original data', (self.runtime / name).read_text())
        installed = json.loads((self.runtime / 'config.json').read_text())
        self.assertEqual(8766, installed['service_port'])
        for key, value in config.items():
            self.assertEqual(value, installed[key])
        guidance = (self.home / 'AGENTS.md').read_text()
        self.assertTrue(guidance.startswith('Existing guidance.\n'))
        self.assertEqual(1, guidance.count(installer.START))
        self.assertNotIn('__LAYA_', guidance)
        self.assertFalse((self.runtime / '.venv').exists())

    def test_subagent_only_has_no_prompt_or_startup_hook(self):
        self.install()
        self.assertFalse((self.home / 'hooks.json').exists())
        self.assertTrue((self.runtime / 'route_subagent.py').is_file())
        result = subprocess.run([sys.executable, str(self.runtime / 'route_subagent.py')],
                                input='{"task":"find files"}', text=True, capture_output=True,
                                cwd=self.root, timeout=5, env=dict(os.environ, LAYA_CODEX_DISABLED='1'))
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual('disabled', json.loads(result.stdout)['reason'])

    def test_nonempty_override_is_the_active_instruction_file(self):
        self.home.mkdir()
        override = self.home / 'AGENTS.override.md'
        override.write_text('Override guidance.\n')
        self.install()
        self.assertTrue(override.read_text().startswith('Override guidance.\n'))
        self.assertFalse((self.home / 'AGENTS.md').exists())

    def test_invalid_markers_or_port_reject_before_client_writes(self):
        self.home.mkdir()
        instructions = self.home / 'AGENTS.md'
        instructions.write_text(installer.START)
        with self.assertRaisesRegex(ValueError, 'Malformed'):
            self.install()
        self.assertFalse(self.runtime.exists())
        with self.assertRaisesRegex(ValueError, 'Port'):
            self.install('--service-port', '65536')
        self.assertFalse(self.runtime.exists())

    def test_explicit_service_port_overrides_legacy_port(self):
        self.install('--service-port', '19001')
        config = json.loads((self.runtime / 'config.json').read_text())
        self.assertEqual(19001, config['service_port'])


if __name__ == '__main__':
    unittest.main()
