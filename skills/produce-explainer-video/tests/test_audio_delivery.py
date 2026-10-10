"""Approved AAC preservation and budget contracts; synthetic playback is not user approval."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
import phone_copy


class AudioDeliveryContracts(unittest.TestCase):
    def test_packet_evidence_requires_payload_and_timestamps(self):
        for data in ({'packets': []}, {'packets': [{'size': '20', 'data_hash': 'SHA256:x'}]}):
            with patch.object(phone_copy, 'run', return_value=json.dumps(data).encode()):
                with self.assertRaisesRegex(ValueError, 'Incomplete audio'):
                    phone_copy.audio_signature(Path('fixture.mp4'))

    @unittest.skipUnless(shutil.which('ffmpeg') and shutil.which('ffprobe'), 'FFmpeg/ffprobe required')
    def test_video_only_compression_preserves_aac_and_fails_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            master, output = root/'master.mp4', root/'phone.mp4'
            subprocess.run(['ffmpeg', '-v', 'error', '-nostdin', '-f', 'lavfi', '-i',
                            'testsrc2=size=160x240:rate=24:duration=4', '-f', 'lavfi', '-i',
                            'anoisesrc=color=pink:amplitude=0.1:duration=4:sample_rate=48000',
                            '-map', '0:v:0', '-map', '1:a:0', '-c:v', 'libx264', '-crf', '12',
                            '-af', 'pan=stereo|c0=c0|c1=c0', '-c:a', 'aac', '-profile:a', 'aac_low',
                            '-b:a', '256k', '-map_metadata', '-1', '-movflags', '+faststart', str(master)],
                           check=True, capture_output=True, timeout=60)
            before = hashlib.sha256(master.read_bytes()).hexdigest()
            signature = phone_copy.audio_signature(master)
            base = ['phone_copy', str(master), str(output), '--copy-audio', '--width', '128']
            with patch.object(sys, 'argv', base + ['--max-bytes', '240000']):
                phone_copy.main()
            self.assertEqual(signature, phone_copy.audio_signature(output))
            self.assertEqual(before, hashlib.sha256(master.read_bytes()).hexdigest())
            qa = json.loads(output.with_suffix('.qa.json').read_text())
            self.assertTrue(qa['copied_audio_packets_and_timing_verified'])
            self.assertEqual('stream-copy', qa['audio_mode'])
            self.assertLessEqual(qa['bytes'], 240000)
            self.assertEqual(96, qa['frames'])
            self.assertEqual(0, qa['full_decode_errors'])
            with patch.object(sys, 'argv', base + ['--max-bytes', '240000']):
                with self.assertRaises(FileExistsError):
                    phone_copy.main()
            output.unlink()
            with patch.object(sys, 'argv', base + ['--max-bytes', '10000']):
                with self.assertRaisesRegex(ValueError, 'budget too small'):
                    phone_copy.main()
            self.assertFalse(output.exists())

            original = phone_copy.audio_signature
            calls = 0
            def changed_signature(path, ffprobe='ffprobe'):
                nonlocal calls
                calls += 1
                data = copy.deepcopy(original(path, ffprobe))
                if calls == 2:
                    data['packets'][0]['data_hash'] = 'SHA256:changed'
                return data
            with patch.object(sys, 'argv', base + ['--max-bytes', '240000']), \
                    patch.object(phone_copy, 'audio_signature', changed_signature):
                with self.assertRaisesRegex(ValueError, 'Approved AAC'):
                    phone_copy.main()
            self.assertFalse(output.exists())
            self.assertFalse(output.with_name(output.name+'.lock').exists())
            self.assertEqual(before, hashlib.sha256(master.read_bytes()).hexdigest())


if __name__ == '__main__':
    unittest.main()
