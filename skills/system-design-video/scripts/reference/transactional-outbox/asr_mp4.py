"""Check every paragraph and sentence of the final mixed MP4 with four decodes."""
import json
import subprocess
import sys
from pathlib import Path

import numpy as np

SAMPLE_RATE = 16000


def decode(audio, models, start, end, padding):
    first = max(0, int((start - padding) * SAMPLE_RATE))
    last = int((end + padding) * SAMPLE_RATE)
    heard = []
    for model in models:
        for beam_size in (5, 1):
            segments, _ = model.transcribe(
                audio[first:last], language='en', beam_size=beam_size,
                condition_on_previous_text=False,
            )
            heard.append(' '.join(segment.text.strip() for segment in segments))
    return heard


def main():
    video = sys.argv[1] if len(sys.argv) > 1 else 'transactional-outbox-final-v1-master.mp4'
    performance = (
        Path(sys.argv[2]) if len(sys.argv) > 2
        else Path(__file__).resolve().parent / 'narration' / 'fenrir-performance.json'
    )
    paragraphs = json.loads(performance.read_text(encoding='utf-8'))['paragraphs']
    pcm = subprocess.run(
        ['ffmpeg', '-loglevel', 'error', '-i', video, '-f', 'f32le',
         '-ac', '1', '-ar', str(SAMPLE_RATE), '-'],
        capture_output=True, check=True,
    ).stdout
    audio = np.frombuffer(pcm, dtype='<f4').copy()
    duration = len(audio) / SAMPLE_RATE

    gaps = []
    for paragraph in paragraphs:
        for sentence in paragraph['sentences']:
            if sentence['end'] > duration:
                status = 'missing' if sentence['start'] >= duration else 'truncated'
                gaps.append({'sid': sentence['sid'], 'status': status})

    report = {'audio_duration_seconds': duration, 'coverage_gaps': gaps, 'paragraphs': [], 'sentences': []}
    report_path = Path('evidence/asr-final-mp4.json')
    report_path.parent.mkdir(exist_ok=True)
    if gaps:
        report_path.write_text(json.dumps(report, indent=1), encoding='utf-8')
        details = ', '.join(f"{gap['sid']} ({gap['status']})" for gap in gaps)
        raise SystemExit(f'Speech check could not cover every sentence: {details}')

    from faster_whisper import WhisperModel

    models = [WhisperModel(name, device='cpu', compute_type='int8') for name in ('small.en', 'base.en')]
    for paragraph in paragraphs:
        sentences = paragraph['sentences']
        report['paragraphs'].append({
            'para': paragraph['para'],
            'heard': decode(audio, models, sentences[0]['start'], sentences[-1]['end'], 0.5),
        })
        for sentence in sentences:
            report['sentences'].append({
                'sid': sentence['sid'],
                'text': sentence['text'],
                'heard': decode(audio, models, sentence['start'], sentence['end'], 0.15),
            })
    report_path.write_text(json.dumps(report, indent=1), encoding='utf-8')
    for entry in report['paragraphs'] + report['sentences']:
        print(entry.get('para', entry.get('sid')), '|', ' || '.join(entry['heard']))


if __name__ == '__main__':
    main()
