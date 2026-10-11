"""Native Kokoro narration, model cues and exact PCM audit. No bundled weights."""
import argparse
import array
import hashlib
import importlib.metadata
import json
import math
import os
from pathlib import Path
import re
import sys
import tempfile
import wave


def audit(path):
    with wave.open(str(path), 'rb') as wav:
        if wav.getsampwidth() != 2 or wav.getcomptype() != 'NONE':
            raise ValueError('Expected uncompressed PCM16 WAV')
        data = array.array('h', wav.readframes(wav.getnframes()))
        if sys.byteorder != 'little':
            data.byteswap()
        if not data:
            raise ValueError('Empty narration')
        return {'duration': wav.getnframes() / wav.getframerate(),
                'sample_rate': wav.getframerate(), 'channels': wav.getnchannels(),
                'peak': max(abs(n) for n in data) / 32768,
                'clipped_samples': sum(n in (-32768, 32767) for n in data),
                'finite_samples': True, 'sha256': hashlib.sha256(Path(path).read_bytes()).hexdigest(),
                'perceptually_listened': False}


def check_cues(text, cues, duration):
    norm = lambda s: ' '.join(s.split())
    if norm(' '.join(c['text'] for c in cues)) != norm(text):
        raise ValueError('Cue text does not exactly cover narration')
    previous = 0.0
    for cue in cues:
        start, end = cue['start'], cue['end']
        if not math.isfinite(start + end) or not 0 <= start <= end <= duration or start < previous:
            raise ValueError('Invalid or overlapping word cues')
        previous = end


def generate(script, output, voice='af_heart', speed=1.0):
    from project import ownership
    output.parent.mkdir(parents=True, exist_ok=True)
    with ownership(output.with_name(output.name + '.lock')):
        if output.exists():
            raise FileExistsError('Preserve existing audio; use a new revision')
        with tempfile.TemporaryDirectory(prefix='voice-partial-', dir=output.parent) as temporary:
            staging = Path(temporary) / 'audio'
            report = _generate(script, staging, voice, speed)
            os.replace(staging, output)
            return report


def _generate(script, output, voice, speed):
    import numpy as np
    import soundfile as sf
    from kokoro import KPipeline
    if output.exists():
        raise FileExistsError('Preserve the existing exact WAV; use a new audio revision')
    output.mkdir(parents=True)
    text = script.read_text(encoding='utf-8').strip()
    if not text:
        raise ValueError('Empty narration script')
    pipeline = KPipeline(lang_code='a', repo_id='hexgrad/Kokoro-82M', device='cpu')
    parts, cues, paragraphs, offset = [], [], [], 0.0
    for index, paragraph in enumerate(re.split(r'\n\s*\n', text)):
        begin = offset
        results = list(pipeline(paragraph, voice=voice, speed=speed, split_pattern=None))
        if ' '.join(' '.join(r.graphemes for r in results).split()) != ' '.join(paragraph.split()):
            raise ValueError('Kokoro did not preserve the narration')
        for result in results:
            samples = result.audio.numpy()
            if not np.isfinite(samples).all() or np.max(np.abs(samples)) >= 1:
                raise ValueError('Non-finite or clipped synthesis')
            duration = len(samples) / 24000
            parts.append(samples)
            for token in result.tokens or []:
                start, end = getattr(token, 'start_ts', None), getattr(token, 'end_ts', None)
                if start is not None and end is not None:
                    cue = {'text': token.text, 'start': offset + max(0, min(duration, start)),
                           'end': offset + max(0, min(duration, end)), 'paragraph': index}
                    if re.search(r'\w', token.text):
                        cues.append(cue)
                    elif cues:
                        cues[-1]['text'] += token.text
            offset += duration
        paragraphs.append({'start': begin, 'end': offset, 'text': paragraph})
    for a, b in zip(cues, cues[1:]):
        a['end'] = min(a['end'], b['start'])
    check_cues(text, cues, offset)
    sf.write(output / 'narration.wav', np.concatenate(parts), 24000, subtype='PCM_16')
    report = {**audit(output / 'narration.wav'), 'voice': voice, 'speed': speed,
              'post_stretch': False, 'manual_gaps': False, 'manual_trim': False,
              'script_sha256': hashlib.sha256(script.read_bytes()).hexdigest(),
              'timing_basis': 'Kokoro predictions; not independent forced alignment',
              'versions': {n: importlib.metadata.version(n) for n in ('kokoro', 'numpy', 'soundfile')}}
    for name, value in [('words.json', cues), ('paragraphs.json', paragraphs), ('audio-qa.json', report)]:
        (output / name).write_text(json.dumps(value, indent=2), encoding='utf-8')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['generate', 'audit', 'cues'])
    parser.add_argument('path', type=Path)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--script', type=Path)
    parser.add_argument('--wav', type=Path)
    parser.add_argument('--voice', default='af_heart')
    parser.add_argument('--speed', type=float, default=1.0)
    args = parser.parse_args()
    if args.action == 'generate':
        if not args.output:
            parser.error('generate requires --output')
        result = generate(args.path, args.output, args.voice, args.speed)
    elif args.action == 'audit':
        result = audit(args.path)
    else:
        if not args.script or not args.wav:
            parser.error('cues requires --script and --wav')
        check_cues(args.script.read_text(encoding='utf-8'), json.loads(args.path.read_text()), audit(args.wav)['duration'])
        result = {'exact_text_coverage': True, 'timing_basis': 'Model cues; not forced alignment'}
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
