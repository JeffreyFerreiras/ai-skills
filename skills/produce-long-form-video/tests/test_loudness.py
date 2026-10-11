"""Loudness contracts on synthetic encodes; no perceptual listening claim."""
import array
import hashlib
import json
import math
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import wave

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from loudness import findings, measure, normalization_settings, parse_measurement, second_pass


class LoudnessContracts(unittest.TestCase):
    def test_silent_missing_and_incomplete_measurements_fail_closed(self):
        for value in ('-inf', 'nan', 'inf'):
            data = {'input_i': value, 'input_tp': '-2', 'input_lra': '0',
                    'input_thresh': '-28', 'target_offset': '0'}
            with self.assertRaises(ValueError):
                parse_measurement(json.dumps(data))
        with self.assertRaises(ValueError):
            parse_measurement('no audio statistics')
        with self.assertRaises(KeyError):
            parse_measurement('{"input_i": "-18"}')

    def test_quiet_program_and_peak_overshoot_are_separate_failures(self):
        self.assertEqual(1, len(findings({'input_i': -25.7, 'input_tp': -2.2})))
        self.assertEqual(1, len(findings({'input_i': -18.7, 'input_tp': -.1})))
        self.assertEqual([], findings({'input_i': -18.7, 'input_tp': -2.2}))
        with self.assertRaises(ValueError):
            findings({}, minimum=-16, maximum=-20)

    def test_custom_delivery_contract_sets_both_normalization_passes(self):
        self.assertEqual((-18, -3), normalization_settings(-20, -16, -2))
        self.assertEqual((-23, -7), normalization_settings(-24, -22, -6))
        for target, peak in ((-18, -7), (-23, -3)):
            with self.assertRaises(ValueError):
                normalization_settings(-24, -22, -6, target, peak)
        with self.assertRaises(ValueError):
            normalization_settings(-24, -22, -10)

    @unittest.skipUnless(shutil.which('ffmpeg'), 'FFmpeg required for encoded loudness test')
    def test_two_pass_encoded_remeasurement_preserves_video(self):
        def run(arguments):
            return subprocess.run(['ffmpeg', '-v', 'error', '-nostdin', *arguments],
                                  check=True, capture_output=True, timeout=60).stdout

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name, amplitude in (('quiet', .015), ('hot', .98)):
                pcm = array.array('h', [round(32767 * amplitude * math.sin(2*math.pi*1000*i/48000))
                                        for i in range(5*48000)])
                if sys.byteorder != 'little':
                    pcm.byteswap()
                with wave.open(str(root/f'{name}.wav'), 'wb') as wav:
                    wav.setparams((1, 2, 48000, 0, 'NONE', 'not compressed'))
                    wav.writeframes(pcm.tobytes())
                run(['-f', 'lavfi', '-i', 'color=size=64x96:rate=24:duration=5',
                     '-i', str(root/f'{name}.wav'), '-map', '0:v:0', '-map', '1:a:0',
                     '-c:v', 'libx264', '-c:a', 'aac', str(root/f'{name}.mp4')])
            quiet, hot = root/'quiet.mp4', root/'hot.mp4'
            before = hashlib.sha256(quiet.read_bytes()).hexdigest()
            original = measure(quiet)
            self.assertLess(original['input_i'], -25)
            self.assertTrue(findings(original))
            self.assertGreater(measure(hot)['input_tp'], -2)
            self.assertEqual(before, hashlib.sha256(quiet.read_bytes()).hexdigest())

            corrected = root/'corrected.mp4'
            run(['-i', str(quiet), '-map', '0:v:0', '-map', '0:a:0', '-c:v', 'copy',
                 '-af', second_pass(original), '-ar', '48000', '-c:a', 'aac', str(corrected)])
            encoded = measure(corrected)
            self.assertLess(abs(encoded['input_i'] + 18), .5)
            self.assertEqual([], findings(encoded))
            def video_hash(path):
                return run(['-i', str(path), '-map', '0:v:0', '-c:v', 'copy', '-f', 'hash', '-'])
            self.assertEqual(video_hash(quiet), video_hash(corrected))
            if shutil.which('ffprobe'):
                def timeline(path):
                    return subprocess.run(['ffprobe', '-v', 'error', '-select_streams', 'v:0',
                                           '-show_entries', 'packet=pts_time,dts_time,duration_time',
                                           '-of', 'json', str(path)], check=True,
                                          capture_output=True, timeout=60).stdout
                self.assertEqual(timeline(quiet), timeline(corrected))
            run(['-xerror', '-i', str(corrected), '-f', 'null', '-'])


if __name__ == '__main__':
    unittest.main()
