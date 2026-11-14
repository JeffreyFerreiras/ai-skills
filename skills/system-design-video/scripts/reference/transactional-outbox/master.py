"""Master premaster.wav to -14 LUFS integrated, true peak <= -2 dBTP, and mux with video-only.mp4 into the ONE clean output.
Measures the AAC inside each attempt (ebur128 true peak) and iterates the gain.
The final master filename is written only after both targets pass. Ten misses keep the measurement report, leave the candidate off that filename, and exit nonzero.
"""
import json, re, subprocess, sys
from pathlib import Path

P = Path(__file__).parent
OUT = P / 'transactional-outbox-final-v1-master.mp4'
CANDIDATE = P / 'transactional-outbox-loudness-candidate.mp4'
VID = P / 'video-only.mp4'
PRE = P / 'audio/premaster.wav'
TARGET_I = -14.0
TOLERANCE = 0.05
TARGET_TP = -2.0


def meets_loudness(integrated, true_peak, target_i=TARGET_I, tolerance=TOLERANCE, target_tp=TARGET_TP):
    return abs(integrated - target_i) <= tolerance and true_peak <= target_tp


def commit_master(passed, candidate, final):
    """Move candidate onto final only when both loudness checks passed.

    Returns the final filename, or None when this attempt is not the master.
    A failed attempt leaves any existing final file untouched.
    """
    if not passed:
        return None
    candidate.replace(final)
    return final.name


def measure(path):
    result = subprocess.run(
        ['ffmpeg', '-hide_banner', '-nostdin', '-i', str(path), '-map', '0:a', '-af', 'ebur128=peak=true', '-f', 'null', '-'],
        capture_output=True, text=True).stderr
    summary = result[result.rindex('Summary:'):]
    integrated = float(re.search(r'I:\s+(-?[\d.]+) LUFS', summary).group(1))
    true_peak = float(re.search(r'Peak:\s+(-?[\d.]+) dBFS', summary).group(1))
    loudness_range = float(re.search(r'LRA:\s+(-?[\d.]+) LU', summary).group(1))
    return integrated, true_peak, loudness_range


def main():
    integrated, _, _ = measure(PRE)
    gain = -14.0 - integrated
    history = []
    passed = False
    for attempt in range(10):
        audio_filter = f'volume={gain:.3f}dB,aresample=192000,alimiter=limit=0.70:attack=1:release=60:level=disabled,aresample=48000'
        subprocess.run(
            ['ffmpeg', '-hide_banner', '-nostdin', '-loglevel', 'error', '-y', '-i', str(VID), '-i', str(PRE),
             '-map', '0:v', '-map', '1:a', '-c:v', 'copy', '-af', audio_filter, '-c:a', 'aac', '-b:a', '192k',
             '-ar', '48000', '-shortest', '-movflags', '+faststart', str(CANDIDATE)],
            check=True)
        integrated, true_peak, loudness_range = measure(CANDIDATE)
        history.append({'gain_db': round(gain, 3), 'I': integrated, 'TP': true_peak, 'LRA': loudness_range})
        print(history[-1], flush=True)
        if meets_loudness(integrated, true_peak):
            passed = True
            break
        gain += (-14.0 - integrated) * (1.0 if attempt < 3 else 1.5)
        if true_peak > TARGET_TP:
            print('true peak over; limiter will hold at 0.70 so retry')
    report = {
        'passed': passed,
        'file': commit_master(passed, CANDIDATE, OUT),
        'candidate': None if passed else CANDIDATE.name,
        'iterations': history,
    }
    (P / 'evidence' / 'loudness.json').write_text(json.dumps(report, indent=1), encoding='utf-8')
    if not passed:
        print('loudness targets not met after 10 attempts', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
