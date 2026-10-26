from __future__ import annotations

import copy
import importlib.util
import math
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("caption_timing", ROOT / "shared/video-production/caption_timing.py")
assert SPEC and SPEC.loader
captions = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(captions)


def words(count):
    return [{"text": "word", "start": index, "end": index + 0.75,
             "source_phrase_id": index // 4} for index in range(count)]


def group(records, **options):
    return captions.group_words(records, measure=len, max_line_width=1000, **options)


class CaptionTimingTests(unittest.TestCase):
    def test_fragments_punctuation_and_zero_width_points_are_conserved(self):
        records = [
            {"text": " pre", "start": 1, "end": 1.2, "source_phrase_id": "a"},
            {"text": "-existing", "start": 1.2, "end": 1.4, "source_phrase_id": "b"},
            {"text": " state", "start": 1.4, "end": 1.4, "source_phrase_id": "b"},
            {"text": ".", "start": 1.4, "end": 1.4, "source_phrase_id": "b"},
        ]
        original = copy.deepcopy(records)
        lexical = captions.merge_lexical_units(records)
        self.assertEqual(["pre-existing", "state."], [word["text"] for word in lexical])
        result = group(lexical)
        self.assertEqual((1, 1.4, "pre-existing state."),
                         (result[0]["start"], result[0]["end"], result[0]["text"]))
        self.assertEqual(["a", "b"], result[0]["source_phrases"])
        self.assertEqual(original, records)

    def test_sentence_and_pause_boundaries_keep_measured_bounds(self):
        records = [
            {"text": "First.", "start": 0, "end": 1},
            {"text": '"Why?"', "start": 1, "end": 2},
            {"text": "Pause", "start": 2, "end": 3},
            {"text": "after", "start": 3.9, "end": 4.5},
        ]
        result = group(records)
        self.assertEqual(["First.", '"Why?"', "Pause", "after"], [cue["text"] for cue in result])
        self.assertEqual([(0, 1), (1, 2), (2, 3), (3.9, 4.5)],
                         [(cue["start"], cue["end"]) for cue in result])

    def test_gap_at_threshold_does_not_force_a_boundary(self):
        records = [{"text": "a", "start": 0, "end": 1},
                   {"text": "b", "start": 1.5, "end": 2}]
        self.assertEqual(["a b"], [cue["text"] for cue in group(records, gap_seconds=0.5)])

    def test_balanced_groups_and_configured_maximum(self):
        result = group(words(16))
        self.assertEqual([8, 8], [len(cue["text"].split()) for cue in result])
        self.assertEqual([(0, 7.75), (8, 15.75)], [(cue["start"], cue["end"]) for cue in result])
        result = group(words(11), target_words=4, min_words=2, max_words=4)
        self.assertEqual(11, sum(len(cue["text"].split()) for cue in result))
        self.assertTrue(all(len(cue["text"].split()) <= 4 for cue in result))

    def test_real_measurement_contract_and_line_limit(self):
        def measure(text):
            return sum(9 if char == "W" else 1 for char in text)

        result = captions.group_words(
            [{"text": "WW", "start": i, "end": i + 0.5} for i in range(6)],
            measure=measure, max_line_width=18, max_lines=2)
        self.assertEqual([2, 2, 2], [len(cue["text"].split()) for cue in result])
        for cue in result:
            rows = captions.wrap_lines(cue["text"], measure=measure, max_line_width=18, max_lines=2)
            self.assertLessEqual(len(rows), 2)
            self.assertTrue(all(measure(row) <= 18 for row in rows))
        with self.assertRaisesRegex(ValueError, "exceeds max_line_width"):
            captions.wrap_lines("WWW", measure=measure, max_line_width=18)
        with self.assertRaisesRegex(ValueError, "exceeds max_lines"):
            captions.wrap_lines("WW WW WW", measure=measure, max_line_width=18)
        with self.assertRaisesRegex(ValueError, "WWW"):
            captions.group_words([{"text": "WWW", "start": 0, "end": 1}],
                                 measure=measure, max_line_width=18)

    def test_short_tail_and_zero_width_tail_are_not_discarded(self):
        records = words(3)
        records[-1]["end"] = records[-1]["start"]
        result = group(records)
        self.assertEqual("word word word", " ".join(cue["text"] for cue in result))
        self.assertEqual(2, result[-1]["end"])
        self.assertTrue(all(cue["end"] > cue["start"] for cue in result))

    def test_zero_width_only_sentence_fails_without_inventing_time(self):
        with self.assertRaisesRegex(ValueError, "positive measured spans"):
            group([{"text": "point.", "start": 1, "end": 1}])
        with self.assertRaisesRegex(ValueError, "point"):
            group([{"text": "point.", "start": 1, "end": 1},
                   {"text": "later", "start": 1, "end": 2}])
        with self.assertRaisesRegex(ValueError, "zero-width points"):
            group([{"text": "one", "start": 1, "end": 1},
                   {"text": "two", "start": 1.5, "end": 1.5}])

    def test_overlaps_are_preserved_across_sentence_boundaries(self):
        records = [{"text": "End.", "start": 0, "end": 1.04},
                   {"text": "Next", "start": 1, "end": 2}]
        result = group(records)
        self.assertEqual([(0, 1.04), (1, 2)], [(cue["start"], cue["end"]) for cue in result])
        self.assertLess(result[1]["start"], result[0]["end"])

    def test_invalid_clocks_and_order_fail(self):
        for start, end in [(math.nan, 1), (0, math.inf), (2, 1), (-1, 0), (False, 1), ("0", 1)]:
            with self.subTest(start=start, end=end), self.assertRaises(ValueError):
                group([{"text": "bad", "start": start, "end": end}])
        for records in [
            [{"text": "a", "start": 1, "end": 2}, {"text": "b", "start": 0, "end": 3}],
            [{"text": "a", "start": 0, "end": 3}, {"text": "b", "start": 1, "end": 2}],
        ]:
            with self.assertRaisesRegex(ValueError, "ordered"):
                group(records)

    def test_deterministic_and_no_mutation(self):
        records = words(19)
        original = copy.deepcopy(records)
        self.assertEqual(group(records), group(records))
        self.assertEqual(original, records)
        self.assertEqual([], group([]))

    def test_configuration_and_bad_measurement_fail(self):
        for options in [{"max_words": 0}, {"min_words": 9}, {"max_lines": 0},
                        {"gap_seconds": math.nan}, {"target_words": True}]:
            with self.subTest(options=options), self.assertRaises(ValueError):
                group(words(1), **options)
        for width in [math.nan, -1, math.inf]:
            with self.subTest(width=width), self.assertRaises(ValueError):
                captions.wrap_lines("a", measure=lambda text: width, max_line_width=10)

    def test_only_system_design_bundles_caption_helper(self):
        spec = importlib.util.spec_from_file_location("bundle_video", ROOT / "scripts/bundle-video-guidance.py")
        bundler = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(bundler)
        for skill, resources in bundler.BUNDLES.items():
            self.assertEqual(skill == "system-design-video", "caption_timing.py" in resources)


if __name__ == "__main__":
    unittest.main()
