"""Master the encoded audio to -14 LUFS integrated and at most -2 dBTP."""
import json
import re
import subprocess
from pathlib import Path
from tempfile import TemporaryDirectory


def measure(path):
    log = subprocess.run(
        ['ffmpeg', '-hide_banner', '-nostdin', '-i', str(path), '-map', '0:a',
         '-af', 'ebur128=peak=true', '-f', 'null', '-'],
        capture_output=True, text=True, check=True,
    ).stderr
    summary = log[log.rindex('Summary:'):]
    integrated = float(re.search(r'I:\s+(-?[\d.]+) LUFS', summary).group(1))
    true_peak = float(re.search(r'Peak:\s+(-?[\d.]+) dBFS', summary).group(1))
    loudness_range = float(re.search(r'LRA:\s+(-?[\d.]+) LU', summary).group(1))
    return integrated, true_peak, loudness_range


def main():
    project = Path(__file__).resolve().parent
    output = project / 'transactional-outbox-final-v1-master.mp4'
    video = project / 'video-only.mp4'
    premaster = project / 'audio' / 'premaster.wav'
    evidence = project / 'evidence'

    integrated, _, _ = measure(premaster)
    gain = -14.0 - integrated
    iterations = []
    passed = False
    with TemporaryDirectory(dir=project) as scratch:
        candidate = Path(scratch) / 'candidate.mp4'
        for attempt in range(10):
            audio_filter = (
                f'volume={gain:.3f}dB,aresample=192000,'
                'alimiter=limit=0.70:attack=1:release=60:level=disabled,aresample=48000'
            )
            subprocess.run(
                ['ffmpeg', '-hide_banner', '-nostdin', '-loglevel', 'error', '-y',
                 '-i', str(video), '-i', str(premaster), '-map', '0:v', '-map', '1:a',
                 '-c:v', 'copy', '-af', audio_filter, '-c:a', 'aac', '-b:a', '192k',
                 '-ar', '48000', '-shortest', '-movflags', '+faststart', str(candidate)],
                check=True,
            )
            integrated, true_peak, loudness_range = measure(candidate)
            iterations.append({
                'gain_db': round(gain, 3),
                'I': integrated,
                'TP': true_peak,
                'LRA': loudness_range,
            })
            print(iterations[-1], flush=True)
            if abs(integrated + 14.0) <= 0.05 and true_peak <= -2.0:
                passed = True
                break
            gain += (-14.0 - integrated) * (1.0 if attempt < 3 else 1.5)

        evidence.mkdir(exist_ok=True)
        report = {'file': output.name if passed else None, 'passed': passed, 'iterations': iterations}
        (evidence / 'loudness.json').write_text(json.dumps(report, indent=1), encoding='utf-8')
        if not passed:
            raise SystemExit(
                f'Mastering missed its targets after {len(iterations)} attempts. '
                f'Last measurement: {integrated} LUFS, {true_peak} dBTP.'
            )
        candidate.replace(output)


if __name__ == '__main__':
    main()
