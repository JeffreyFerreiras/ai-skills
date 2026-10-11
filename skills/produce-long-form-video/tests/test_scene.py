"""Starter rendering contracts; these tests do not establish editorial quality."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch
from PIL import ImageFont

SKILL = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('landscape_starter', SKILL / 'assets/scene.py')
scene = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scene)


class SceneTests(unittest.TestCase):
    def test_caption_plan_reuses_measurements_and_invalidates_inputs(self):
        font_path = os.environ.get('VIDEO_TEST_FONT')
        if not font_path:
            self.skipTest('Set VIDEO_TEST_FONT')
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            shutil.copy2(font_path, root/'font.ttf')
            cues = [{'text': f'word{i}', 'start': i, 'end': i+.7, 'paragraph': i//12} for i in range(1200)]
            (root/'words.json').write_text(json.dumps(cues))
            config = {'words': 'words.json', 'font': 'font.ttf', 'caption_box': [100, 840, 1820, 1020], 'scenes': [{'start': 0, 'end': 1200}]}
            font = ImageFont.truetype(str(root/'font.ttf'), 46)
            plan = scene.caption_plan(config, root, 2, 2, font)
            with patch.object(scene.json, 'loads', side_effect=AssertionError('Transcript rebuilt')):
                self.assertIs(plan, scene.caption_plan(config, root, 2, 2, font))
            config['caption_breaks'] = [2]
            changed = scene.caption_plan(config, root, 2, 2, font)
            self.assertIsNot(plan, changed)
            self.assertEqual(2, changed[1][0]['end'])
            cues[0]['text'] = 'changed'
            (root/'words.json').write_text(json.dumps(cues))
            self.assertEqual('changed', scene.caption_plan(config, root, 2, 2, font)[1][0]['glyphs'][0][0]['text'])

    def test_timeline_and_comparison_reveal_only_current_values(self):
        font_path = os.environ.get('VIDEO_TEST_FONT')
        if not font_path:
            self.skipTest('Set VIDEO_TEST_FONT')
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            shutil.copy2(font_path, root/'font.ttf')
            (root/'words.json').write_text('[]')
            config = json.loads((SKILL/'assets/project.json').read_text())
            config.update(font='font.ttf', words='words.json')
            config['scenes'] = [{'start': 0, 'end': 4, 'title': 'Age matters', 'type': 'timeline', 'events': [
                {'time': 0, 'at': 0, 'label': 'Store', 'value': '$40'},
                {'time': 3, 'at': 2, 'label': 'Update', 'value': '$45'}]}]
            early = scene.frame(1, config, root)[1]
            late = scene.frame(3, config, root)[1]
            self.assertFalse(any(o.get('text') == '$45' for o in early))
            self.assertTrue(any(o.get('text') == '$45' for o in late))
            config['scenes'][0].update(type='comparison', comparisons=[
                {'id': 'cache', 'label': 'Copy', 'states': [{'at': 0, 'text': '$40'}]},
                {'id': 'db', 'label': 'Source', 'states': [{'at': 0, 'text': '$40'}, {'at': 2, 'text': '$45'}]}])
            labels = {o['id']: o.get('text') for o in scene.frame(3, config, root)[1]}
            self.assertEqual('$40', labels['cache-state'])
            self.assertEqual('$45', labels['db-state'])

    def test_states_follow_cues_and_seek_without_future_reveals(self):
        node = {'states': [{'at': 1, 'text': 'empty'}, {'at': 2, 'text': 'key:42 = v1'}]}
        for at, expected in [(0, None), (1, 'empty'), (1.99, 'empty'), (2, 'key:42 = v1'), (.5, None)]:
            self.assertEqual(expected, scene.state_text(node, at, 3))

    def test_invalid_state_cues_and_readouts_fail(self):
        for cues in ([1, 1], [2, 1], [-1], [3], [float('nan')], [True]):
            with self.subTest(cues=cues), self.assertRaises(ValueError):
                scene.state_text({'states': [{'at': at, 'text': 'value'} for at in cues]}, 0, 3)
        with self.assertRaises(ValueError):
            scene.state_text({'states': [{'at': 0, 'text': ' '}]}, 0, 3)

    def test_aspect_change_requires_recomposition(self):
        config = json.loads((SKILL / 'assets/project.json').read_text())
        config.update(width=1080, height=1920)
        with self.assertRaisesRegex(ValueError, 'Recompose'):
            scene.frame(0, config, SKILL)
        for dimension in (0, -1, float('nan'), True):
            config['design_width'] = dimension
            with self.subTest(dimension=dimension), self.assertRaisesRegex(ValueError, 'finite and positive'):
                scene.frame(0, config, SKILL)

    def test_landscape_render_shows_values_and_preserves_source(self):
        font = os.environ.get('VIDEO_TEST_FONT')
        if not font or not Path(font).is_file():
            self.skipTest('Set VIDEO_TEST_FONT to a licensed test font')
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            shutil.copy2(font, root / 'font.ttf')
            (root / 'words.json').write_text(json.dumps([
                {'text': 'Copy', 'start': 0, 'end': 1},
                {'text': 'retained.', 'start': 1, 'end': 3},
            ]))
            config = json.loads((SKILL / 'assets/project.json').read_text())
            config.update(font='font.ttf', words='words.json')
            config['scenes'] = [{
                'start': 0, 'end': 3, 'type': 'diagram', 'title': 'Copies can differ',
                'nodes': [
                    {'id': 'cache', 'label': 'Cache', 'box': [100, 180, 300, 110],
                     'states': [{'at': 0, 'text': 'empty'}, {'at': 1, 'text': 'key:42 = v1'}]},
                    {'id': 'db', 'label': 'Database', 'box': [560, 180, 300, 110],
                     'states': [{'at': 0, 'text': 'key:42 = v1'}, {'at': 2, 'text': 'key:42 = v2'}]},
                ],
            }]
            before = json.dumps(config)
            rendered = []
            for at, cache_value, db_value in [(.5, 'empty', 'key:42 = v1'),
                                             (1.5, 'key:42 = v1', 'key:42 = v1'),
                                             (2.5, 'key:42 = v1', 'key:42 = v2')]:
                image, objects = scene.frame(at, config, root)
                self.assertEqual((1920, 1080), image.size)
                labels = {item['id']: item['text'] for item in objects if item['kind'] == 'label'}
                self.assertEqual(cache_value, labels['cache-state'])
                self.assertEqual(db_value, labels['db-state'])
                self.assertTrue(any(item['kind'] == 'caption' for item in objects))
                for item in objects:
                    left, top, right, bottom = item['box']
                    self.assertTrue(0 <= left < right <= image.width)
                    self.assertTrue(0 <= top < bottom <= image.height)
                rendered.append(image.tobytes())
            self.assertNotEqual(rendered[0], rendered[1])
            self.assertNotEqual(rendered[1], rendered[2])
            self.assertEqual(rendered[1], scene.frame(1.5, config, root)[0].tobytes())
            self.assertEqual(before, json.dumps(config))
            config['scenes'][0]['nodes'][0]['states'] = [{'at': 1, 'text': 'key:42 = v1'}]
            early = {item['id']: item for item in scene.frame(.5, config, root)[1]}
            later = {item['id']: item for item in scene.frame(1.5, config, root)[1]}
            self.assertNotIn('cache-state', early)
            self.assertIn('cache-state', later)
            self.assertEqual(early['cache']['box'], later['cache']['box'])


if __name__ == '__main__':
    unittest.main()
