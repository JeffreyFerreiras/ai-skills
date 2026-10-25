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
from unittest.mock import patch

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

    def test_hooks_transition_removes_only_active_managed_instructions(self):
        for integration in ('subagents', 'both'):
            for filename in ('AGENTS.md', 'AGENTS.override.md'):
                with self.subTest(integration=integration, filename=filename):
                    self.home = self.root / (integration + filename)
                    self.runtime = self.home / 'laya-hook'
                    self.home.mkdir()
                    instructions = self.home / filename
                    before = 'Keep existing guidance.\n<!-- other:start -->Other guidance.<!-- other:end -->\n'
                    after = '\nKeep later guidance.\n'
                    instructions.write_text(before, encoding='utf-8')
                    inactive = self.home / 'AGENTS.md'
                    if filename == 'AGENTS.override.md':
                        inactive.write_text('Inactive general guidance.\n', encoding='utf-8')
                    self.install('--integration', integration)
                    managed = instructions.read_text(encoding='utf-8')
                    self.assertIn(installer.START, managed)
                    instructions.write_text(managed + after, encoding='utf-8')
                    expected = (managed[:managed.index(installer.START)]
                                + managed[managed.index(installer.END) + len(installer.END):] + after)

                    self.install('--integration', 'hooks')
                    self.assertEqual(expected, instructions.read_text(encoding='utf-8'))
                    config = json.loads((self.runtime / 'config.json').read_text())
                    self.assertTrue(config['enabled'])
                    self.assertFalse(config['subagent_routing_enabled'])
                    if filename == 'AGENTS.override.md':
                        self.assertEqual('Inactive general guidance.\n', inactive.read_text(encoding='utf-8'))
                    first = {path.relative_to(self.home): path.read_bytes()
                             for path in self.home.rglob('*') if path.is_file()}
                    self.install('--integration', 'hooks')
                    self.assertEqual(first, {path.relative_to(self.home): path.read_bytes()
                                            for path in self.home.rglob('*') if path.is_file()})

    def test_hooks_only_does_not_create_or_rewrite_unmanaged_instructions(self):
        self.install('--integration', 'hooks')
        self.assertFalse((self.home / 'AGENTS.md').exists())
        self.assertFalse((self.home / 'AGENTS.override.md').exists())
        instructions = self.home / 'AGENTS.override.md'
        original = b'\xef\xbb\xbfKeep override guidance.\r\n'
        instructions.write_bytes(original)
        self.install('--integration', 'hooks')
        self.assertEqual(original, instructions.read_bytes())
        self.assertFalse((self.home / 'AGENTS.md').exists())

    def test_hooks_only_preserves_surrounding_instruction_bytes(self):
        self.home.mkdir()
        instructions = self.home / 'AGENTS.md'
        before = b'\xef\xbb\xbfKeep earlier guidance.\r\n\r\n'
        after = b'\r\nKeep later guidance.\r\n'
        managed = (installer.START + '\r\nOwned guidance.\r\n' + installer.END).encode('utf-8')
        instructions.write_bytes(before + managed + after)
        self.install('--integration', 'hooks')
        self.assertEqual(before + after, instructions.read_bytes())

    def test_hooks_only_cleans_newly_active_fallback_in_one_pass(self):
        for index, (bom, newline) in enumerate(((b'', b'\n'), (b'\xef\xbb\xbf', b'\r\n'))):
            with self.subTest(bom=bom, newline=newline):
                self.home = self.root / str(index)
                self.runtime = self.home / 'laya-hook'
                self.home.mkdir()
                managed = (installer.START.encode() + newline + b'Owned routing guidance.'
                           + newline + installer.END.encode())
                before = bom + b'Keep user guidance.' + newline
                after = newline + b'Keep later guidance.' + newline
                fallback = self.home / 'AGENTS.md'
                override = self.home / 'AGENTS.override.md'
                fallback.write_bytes(before + managed + after)
                override.write_bytes(bom + newline + managed + newline)

                self.install('--integration', 'hooks')
                self.assertEqual(before + after, fallback.read_bytes())
                self.assertEqual(bom + newline * 2, override.read_bytes())
                self.assertFalse(override.read_text(encoding='utf-8-sig').strip())
                first = {path.relative_to(self.home): path.read_bytes()
                         for path in self.home.rglob('*') if path.is_file()}
                self.install('--integration', 'hooks')
                self.assertEqual(first, {path.relative_to(self.home): path.read_bytes()
                                        for path in self.home.rglob('*') if path.is_file()})

    def test_hooks_only_retained_override_leaves_inactive_managed_file_untouched(self):
        self.home.mkdir()
        managed = (installer.START + '\r\nOwned routing guidance.\r\n' + installer.END).encode()
        fallback = self.home / 'AGENTS.md'
        override = self.home / 'AGENTS.override.md'
        original_fallback = b'\xef\xbb\xbfKeep inactive guidance.\r\n' + managed + b'\r\n'
        before, after = b'\xef\xbb\xbfKeep override guidance.\r\n', b'\r\nKeep later guidance.\r\n'
        fallback.write_bytes(original_fallback)
        override.write_bytes(before + managed + after)
        self.install('--integration', 'hooks')
        self.assertEqual(before + after, override.read_bytes())
        self.assertEqual(original_fallback, fallback.read_bytes())
        self.install('--integration', 'hooks')
        self.assertEqual(before + after, override.read_bytes())
        self.assertEqual(original_fallback, fallback.read_bytes())

    def test_hooks_only_rejects_malformed_newly_active_fallback_before_writes(self):
        malformed = (installer.START, installer.END, installer.END + installer.START,
                     installer.START * 2 + installer.END, installer.START + installer.END * 2)
        for index, content in enumerate(malformed):
            with self.subTest(content=content):
                self.home = self.root / str(index)
                self.runtime = self.home / 'laya-hook'
                self.home.mkdir()
                override = '\ufeff\r\n' + installer.START + '\r\nOwned.\r\n' + installer.END + '\r\n'
                (self.home / 'AGENTS.override.md').write_bytes(override.encode())
                (self.home / 'AGENTS.md').write_text('Keep user guidance.\n' + content, encoding='utf-8')
                (self.home / 'hooks.json').write_text('{"hooks":{},"custom":"keep"}', encoding='utf-8')
                before = {path.relative_to(self.home): path.read_bytes()
                          for path in self.home.rglob('*') if path.is_file()}
                with self.assertRaisesRegex(ValueError, 'Malformed'):
                    self.install('--integration', 'hooks')
                self.assertEqual(before, {path.relative_to(self.home): path.read_bytes()
                                         for path in self.home.rglob('*') if path.is_file()})
                self.assertFalse(self.runtime.exists())

    def test_hooks_only_checks_affected_instruction_snapshots_before_runtime_writes(self):
        for filename in ('AGENTS.override.md', 'AGENTS.md'):
            with self.subTest(filename=filename):
                self.home = self.root / filename
                self.runtime = self.home / 'laya-hook'
                self.home.mkdir()
                managed = installer.START + '\nOwned routing guidance.\n' + installer.END
                (self.home / 'AGENTS.override.md').write_text(managed, encoding='utf-8')
                (self.home / 'AGENTS.md').write_text('Keep user guidance.\n' + managed, encoding='utf-8')
                changed = self.home / filename
                merge_hooks = installer.merge_hooks

                def edit_during_planning(*args):
                    changed.write_text('Concurrent user edit.\n', encoding='utf-8')
                    return merge_hooks(*args)

                with patch.object(installer, 'merge_hooks', side_effect=edit_during_planning):
                    with self.assertRaisesRegex(ValueError, 'Profile changed'):
                        self.install('--integration', 'hooks')
                self.assertEqual('Concurrent user edit.\n', changed.read_text(encoding='utf-8'))
                self.assertFalse(self.runtime.exists())
                self.assertFalse((self.home / 'hooks.json').exists())

    def test_profile_edits_during_client_copies_stop_profile_writes(self):
        for filename in ('hooks.json', 'AGENTS.md', 'laya-hook/config.json'):
            with self.subTest(filename=filename):
                self.home = self.root / filename.replace('/', '-')
                self.runtime = self.home / 'laya-hook'
                self.runtime.mkdir(parents=True)
                managed = installer.START + '\nOwned routing guidance.\n' + installer.END
                (self.home / 'AGENTS.override.md').write_text(managed, encoding='utf-8')
                (self.home / 'AGENTS.md').write_text('Keep user guidance.\n' + managed, encoding='utf-8')
                (self.home / 'hooks.json').write_text('{"hooks":{},"custom":"keep"}', encoding='utf-8')
                (self.runtime / 'config.json').write_text('{"custom":"keep"}', encoding='utf-8')
                profiles = (self.home / 'AGENTS.override.md', self.home / 'AGENTS.md',
                            self.home / 'hooks.json', self.runtime / 'config.json')
                expected = {path: path.read_bytes() for path in profiles}
                changed = self.home / filename
                expected[changed] = b'Concurrent user edit.\r\n'
                copy_client = installer.shutil.copy2

                def edit_during_copy(*args):
                    result = copy_client(*args)
                    changed.write_bytes(expected[changed])
                    return result

                with patch.object(installer.shutil, 'copy2', side_effect=edit_during_copy):
                    with self.assertRaisesRegex(ValueError, 'Profile changed'):
                        self.install('--integration', 'hooks')
                self.assertEqual(expected, {path: path.read_bytes() for path in profiles})

    def test_hooks_only_rejects_malformed_markers_before_any_writes(self):
        malformed = (installer.START, installer.END, installer.END + installer.START,
                     installer.START * 2 + installer.END, installer.START + installer.END * 2)
        for index, content in enumerate(malformed):
            with self.subTest(content=content):
                self.home = self.root / str(index)
                self.runtime = self.home / 'laya-hook'
                self.home.mkdir()
                (self.home / 'AGENTS.override.md').write_text(content, encoding='utf-8')
                (self.home / 'hooks.json').write_text('{"hooks":{},"custom":"keep"}', encoding='utf-8')
                before = {path.relative_to(self.home): path.read_bytes()
                          for path in self.home.rglob('*') if path.is_file()}
                with self.assertRaisesRegex(ValueError, 'Malformed'):
                    self.install('--integration', 'hooks')
                self.assertEqual(before, {path.relative_to(self.home): path.read_bytes()
                                          for path in self.home.rglob('*') if path.is_file()})
                self.assertFalse(self.runtime.exists())

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
