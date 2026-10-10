"""Isolated contract tests. Synthetic sound is not narration/editorial evidence."""
import array
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import wave

SKILL = Path(__file__).resolve().parents[1]
sys.dont_write_bytecode = True
sys.path.insert(0, str(SKILL / 'scripts'))
from audio import audit, check_cues
from project import capture, checkpoint, ownership, validate
from render import geometry_issues
from mix_events import mix
import phone_copy
import audio
from export_srt import export
from review_frames import extract


class Contracts(unittest.TestCase):
    def test_source_changes_and_checkpoint_reproduction(self):
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary)
            root = work / 'source'
            root.mkdir()
            (root / 'narration.wav').write_bytes(b'preserved exact test bytes')
            (root / 'scene.py').write_text('a=1')
            lock = capture(root)
            output = work / 'source.zip'
            checkpoint(root, output, lock)
            restored = work / 'restored'
            shutil.unpack_archive(output, restored)
            self.assertEqual(lock, capture(restored))
            for action in ('add', 'remove', 'modify'):
                with self.subTest(action=action):
                    path = root / 'scene.py'
                    if action == 'add':
                        path = root / 'extra.png'
                        path.write_bytes(b'added')
                    elif action == 'remove':
                        path.unlink()
                    else:
                        path.write_text('a=2')
                    with self.assertRaises(ValueError):
                        validate(root, lock)
                    if action == 'add':
                        path.unlink()
                    else:
                        path.write_text('a=1')
            with self.assertRaises(ValueError):
                checkpoint(root, root / 'bad.zip', lock)

    def test_concurrent_ownership_and_existing_lock(self):
        with tempfile.TemporaryDirectory() as temporary:
            lock = Path(temporary) / 'lock'
            with ownership(lock):
                with self.assertRaises(FileExistsError):
                    with ownership(lock):
                        self.fail('Concurrent ownership accepted')
                self.assertTrue(lock.exists())
            self.assertFalse(lock.exists())

    def test_cues_and_geometry_detect_actual_faults(self):
        check_cues('Hello world.', [{'text': 'Hello', 'start': 0, 'end': .5}, {'text': 'world.', 'start': .5, 'end': 1}], 1)
        for cues in ([{'text': 'Wrong', 'start': 0, 'end': 1}], [{'text': 'Hello world.', 'start': 0, 'end': float('nan')}]):
            with self.assertRaises(ValueError):
                check_cues('Hello world.', cues, 1)
        with self.assertRaises(ValueError):
            check_cues('a nice view', [{'text': 'an', 'start': 0, 'end': .2}, {'text': 'ice', 'start': .2, 'end': .4}, {'text': 'view', 'start': .4, 'end': .6}], 1)
        objects = [{'id': 'word', 'kind': 'label', 'box': [10, 10, 40, 30]},
                   {'id': 'pulse', 'kind': 'glow', 'box': [25, 5, 45, 25]},
                   {'id': 'actor', 'kind': 'actor', 'box': [-1, 90, 20, 110]}]
        issues = geometry_issues(objects, 100, 120, [0, 100, 100, 120])
        self.assertIn('glow/label:pulse/word', issues)
        self.assertIn('crop:actor', issues)
        self.assertIn('caption:actor', issues)
        srt = export([{'text':'First.','start':0,'end':.5}, {'text':'Second','start':1,'end':1.5}, {'text':'scene.','start':2,'end':2.5}], breaks=[2])
        self.assertEqual(3, srt.count('-->'))

    def test_phone_race_and_incomplete_audio_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary)
            master, output = work/'master.mp4', work/'phone.mp4'
            master.write_bytes(b'test master fixture')
            probe = {'streams': [{'codec_type': 'video', 'avg_frame_rate': '24/1', 'nb_read_frames': '24', 'height': 960, 'width': 540}, {'codec_type': 'audio', 'duration': '1.0'}]}
            def race_probe(command):
                output.write_bytes(b'existing valid final')
                return json.dumps(probe).encode()
            with patch.object(sys, 'argv', ['phone_copy', str(master), str(output), '--max-bytes', '100000']), patch.object(phone_copy, 'run', race_probe):
                with self.assertRaises(FileExistsError):
                    phone_copy.main()
            self.assertEqual(b'existing valid final', output.read_bytes())
            output.unlink()
            probe['streams'][1]['duration'] = '.2'
            with patch.object(sys, 'argv', ['phone_copy', str(master), str(output), '--max-bytes', '100000']), patch.object(phone_copy, 'run', return_value=json.dumps(probe).encode()):
                with self.assertRaisesRegex(ValueError, 'complete video'):
                    phone_copy.main()
            self.assertFalse(output.exists())

    def test_failed_synthesis_leaves_no_completed_audio_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def fail(script, output, voice, speed):
                output.mkdir()
                (output/'partial.wav').write_bytes(b'incomplete')
                raise RuntimeError('synthesis unavailable')
            with patch.object(audio, '_generate', fail):
                with self.assertRaises(RuntimeError):
                    audio.generate(root/'narration.txt', root/'audio')
            self.assertFalse((root/'audio').exists())
            self.assertFalse((root/'audio.lock').exists())

    def test_starter_geometry_font_and_caption_gap(self):
        font = os.environ.get('VIDEO_TEST_FONT')
        if not font or not Path(font).is_file():
            self.skipTest('Set VIDEO_TEST_FONT')
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            shutil.copy2(font, root/'font.ttf')
            (root/'words.json').write_text(json.dumps([{'text':'Hello','start':0,'end':.5}, {'text':'world.','start':1,'end':1.5}]))
            spec=importlib.util.spec_from_file_location('starter', SKILL/'assets/scene.py')
            scene=importlib.util.module_from_spec(spec)
            spec.loader.exec_module(scene)
            config=json.loads((SKILL/'assets/project.json').read_text())
            config.update(font='font.ttf', words='words.json')
            config['scenes']=[{'start':0,'end':2,'type':'diagram','title':'Test','nodes':[{'id':'cropped','label':'Visible','box':[-100,300,300,100]}], 'paths':[{'id':'outside','points':[[-40,450],[-20,470]],'start':1,'end':2}]}]
            _, objects=scene.frame(.75, config, root)
            issues=geometry_issues(objects,540,960,config['caption_box'])
            self.assertIn('crop:cropped-body',issues)
            self.assertTrue(any('crop:outside' in issue for issue in issues))
            self.assertTrue(any(item['kind']=='caption' for item in objects))
            config['font']=font
            with self.assertRaisesRegex(ValueError,'inside the source'):
                scene.frame(.75,config,root)

    def test_audio_mix_retains_original_and_duration(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, output = root/'native.wav', root/'mix.wav'
            with wave.open(str(source), 'wb') as wav:
                wav.setparams((1, 2, 24000, 0, 'NONE', 'not compressed'))
                wav.writeframes(array.array('h', [100]*24000).tobytes())
            before = source.read_bytes()
            report = mix(source, [{'time': .2}], output)
            self.assertEqual(before, source.read_bytes())
            self.assertEqual(report['original']['duration'], report['mix']['duration'])
            self.assertEqual(0, report['mix']['clipped_samples'])
            with self.assertRaises(FileExistsError):
                mix(source, [], source)

    @unittest.skipUnless(shutil.which('ffmpeg') and shutil.which('ffprobe') and importlib.util.find_spec('PIL'), 'FFmpeg/ffprobe/Pillow unavailable')
    def test_windows_encode_resume_stop_and_stale_sources(self):
        font = os.environ.get('VIDEO_TEST_FONT')
        if not font or not Path(font).is_file():
            self.skipTest('Set VIDEO_TEST_FONT to a licensed test font')
        with tempfile.TemporaryDirectory(prefix='video spaced ') as temporary:
            work, root = Path(temporary), Path(temporary)/'source'
            root.mkdir()
            shutil.copy2(SKILL/'assets/scene.py', root/'scene.py')
            shutil.copy2(font, root/'font.ttf')
            with wave.open(str(root/'voice.wav'), 'wb') as wav:
                wav.setparams((1, 2, 24000, 0, 'NONE', 'not compressed'))
                wav.writeframes(array.array('h', [100]*60240).tobytes())
            text = 'Learn why snapshots help. A saved copy restores state.'
            (root/'narration.txt').write_text(text)
            tokens = text.split()
            words = [{'text': text, 'start': i*.25, 'end': min((i+1)*.25, 2.51)} for i, text in enumerate(tokens)]
            (root/'words.json').write_text(json.dumps(words))
            config = json.loads((SKILL/'assets/project.json').read_text())
            config.update(wav='voice.wav', words='words.json', font='font.ttf', width=540, height=960)
            config['scenes'] = [{'start': 0, 'end': 1.2, 'type': 'context', 'title': 'Why save state?',
                                 'nodes': [{'id': 'app', 'label': 'Saved state', 'box': [160, 320, 200, 80], 'activate': .6}]},
                                {'start': 1.2, 'end': 2.51, 'type': 'diagram', 'title': 'Restore a copy',
                                 'nodes': [{'id': 'disk', 'label': 'Saved', 'box': [60, 270, 140, 70]}, {'id': 'app', 'label': 'Restored', 'box': [300, 480, 170, 70]}],
                                 'paths': [{'id': 'copy', 'points': [[150, 345], [160, 430], [350, 430], [350, 465]], 'start': .1, 'end': 1.1}]}]
            (root/'project.json').write_text(json.dumps(config))
            lock = capture(root)
            lock_path = work/'lock.json'
            lock_path.write_text(json.dumps(lock))
            base = [sys.executable, str(SKILL/'scripts/render.py'), str(root), '--lock', str(lock_path), '--batch', '20']
            def invoke(*args):
                return subprocess.run(base+list(args), capture_output=True, text=True)
            stopped = work/'stop'
            stopped.touch()
            self.assertNotEqual(0, invoke('--output', str(work/'stopped.mp4'), '--stop-file', str(stopped)).returncode)
            self.assertFalse((work/'stopped.mp4').exists())
            self.assertNotEqual(0, invoke('--output', str(work/'full.mp4'), '--mode', 'full').returncode)
            sample = work/'sample.mp4'
            result = invoke('--output', str(sample), '--seconds', '2.51')
            self.assertEqual(0, result.returncode, result.stderr)
            qa = json.loads(sample.with_suffix('.qa.json').read_text())
            self.assertEqual(61, qa['frames'])
            self.assertTrue(qa['joined_frames_verified'])
            self.assertEqual(0, qa['full_decode_errors'])
            samples = extract(sample, work/'review', [.01, .74, 1.24])
            self.assertEqual(3, len(samples['requests']))
            self.assertTrue(all(abs(row['requested_seconds']-row['actual_seconds']) <= 1/48 for row in samples['requests']))
            saved = sample.read_bytes()
            self.assertNotEqual(0, invoke('--output', str(sample)).returncode)
            self.assertEqual(saved, sample.read_bytes())
            sample.unlink()
            result = invoke('--output', str(sample), '--seconds', '2.51', '--resume')
            self.assertEqual(0, result.returncode, result.stderr)
            (root/'added.png').write_bytes(b'changed asset set')
            self.assertNotEqual(0, invoke('--output', str(work/'mixed.mp4'), '--resume').returncode)
            self.assertFalse((work/'mixed.mp4').exists())
            (root/'added.png').unlink()
            with (root/'scene.py').open('a') as renderer:
                renderer.write("\n_original_frame = frame\n_calls = 0\ndef frame(t, config, root):\n    global _calls\n    _calls += 1\n    if _calls == 65:\n        (root / 'mid-render-asset.png').write_bytes(b'changed during encode')\n    return _original_frame(t, config, root)\n")
            lock_path.write_text(json.dumps(capture(root)))
            result = invoke('--output', str(work/'mid-render.mp4'), '--seconds', '2.51')
            self.assertNotEqual(0, result.returncode)
            self.assertIn('Source set changed', result.stderr)
            self.assertFalse((work/'mid-render.mp4').exists())


if __name__ == '__main__':
    unittest.main()
