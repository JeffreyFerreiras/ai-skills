"""Speech-recognition check on the FINAL MP4 audio (music + SFX in): per paragraph with 0.5 s padding and per sentence with 0.15 s padding;
faster-whisper small.en and base.en, beam 5 and greedy (4 decodes).

Every expected sentence is checked against the decoded duration. A sentence that starts at or after the end of the audio is missing. A sentence that starts inside the audio and ends after it is truncated. Those IDs are written into the report, and the process exits nonzero. Complete sentences in a partly covered paragraph are still decoded. A paragraph decode runs only when every sentence in that paragraph ends inside the audio.
"""
import json, sys
from pathlib import Path


def sentence_coverage(paragraphs, duration):
    """Split expected sentences into complete, and missing or truncated.

    A paragraph is complete only when every one of its sentences ends at or before duration.
    Earlier complete sentences are still returned when a later sentence in the same paragraph is not.
    """
    complete_paragraphs = []
    complete_sentences = []
    gaps = []
    for paragraph in paragraphs:
        sentences = paragraph['sentences']
        ready = []
        paragraph_gaps = []
        for sentence in sentences:
            start, end = sentence['start'], sentence['end']
            if start >= duration or end > duration:
                reason = 'missing' if start >= duration else 'truncated'
                paragraph_gaps.append({'sid': sentence['sid'], 'reason': reason, 'start': start, 'end': end})
            else:
                ready.append(sentence)
        gaps.extend(paragraph_gaps)
        complete_sentences.extend(ready)
        if sentences and not paragraph_gaps:
            complete_paragraphs.append(paragraph)
    return complete_paragraphs, complete_sentences, gaps


def main(argv=None):
    import subprocess
    import numpy as np
    from faster_whisper import WhisperModel
    argv = sys.argv if argv is None else argv
    media = argv[1] if len(argv) > 1 else 'transactional-outbox-final-v1-master.mp4'
    performance = argv[2] if len(argv) > 2 else Path(__file__).resolve().parent / 'narration' / 'fenrir-performance.json'
    sample_rate = 16000
    samples = np.frombuffer(subprocess.run(
        ['ffmpeg', '-loglevel', 'error', '-i', media, '-f', 'f32le', '-ac', '1', '-ar', str(sample_rate), '-'],
        capture_output=True, check=True).stdout, dtype='<f4').copy()
    duration = len(samples) / sample_rate
    paragraphs = json.load(open(performance))['paragraphs']
    complete_paragraphs, complete_sentences, gaps = sentence_coverage(paragraphs, duration)
    out = {'duration_s': round(duration, 3), 'paragraphs': [], 'sentences': [], 'missing': gaps}
    if complete_paragraphs or complete_sentences:
        models = {name: WhisperModel(name, device='cpu', compute_type='int8') for name in ('small.en', 'base.en')}

        def decode(start, end, pad):
            segment = samples[max(0, int((start - pad) * sample_rate)):int(min(duration, end + pad) * sample_rate)]
            heard = []
            for model in models.values():
                for beam in (5, 1):
                    heard.append(' '.join(piece.text.strip() for piece in model.transcribe(
                        segment, language='en', beam_size=beam, condition_on_previous_text=False)[0]))
            return heard

        for paragraph in complete_paragraphs:
            sentences = paragraph['sentences']
            out['paragraphs'].append({'para': paragraph['para'], 'heard': decode(sentences[0]['start'], sentences[-1]['end'], 0.5)})
        for sentence in complete_sentences:
            out['sentences'].append({'sid': sentence['sid'], 'text': sentence['text'], 'heard': decode(sentence['start'], sentence['end'], 0.15)})
    json.dump(out, open('evidence/asr-final-mp4.json', 'w'), indent=1)
    for key in ('paragraphs', 'sentences'):
        for row in out[key]:
            print(row.get('para', row.get('sid')), '|', ' || '.join(row['heard']))
    if gaps:
        print('missing or truncated sentences:', ', '.join(gap['sid'] for gap in gaps), file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
