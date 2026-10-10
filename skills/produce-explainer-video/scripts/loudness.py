"""Read-only final-encode loudness/true-peak QA and measured second-pass recipe."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess


def parse_measurement(stderr):
    for block in reversed(re.findall(r'\{[^{}]*\}', stderr)):
        data = json.loads(block)
        if 'input_i' in data:
            keys = ('input_i', 'input_tp', 'input_lra', 'input_thresh', 'target_offset')
            result = {key: float(data[key]) for key in keys}
            if not all(math.isfinite(value) for value in result.values()):
                raise ValueError('Non-finite loudness: silence/short material needs separate review')
            return result
    raise ValueError('FFmpeg did not return complete loudnorm measurements')


def measure(media, ffmpeg='ffmpeg', target=-18, peak=-3):
    """Measure decoded first audio stream; never change the input file."""
    before = hashlib.sha256(media.read_bytes()).hexdigest()
    process = subprocess.run([ffmpeg, '-hide_banner', '-nostdin', '-v', 'info', '-xerror',
                              '-i', str(media), '-map', '0:a:0', '-vn', '-af',
                              f'loudnorm=I={target}:TP={peak}:LRA=11:dual_mono=false:print_format=json',
                              '-f', 'null', os.devnull], capture_output=True, text=True, timeout=600)
    if process.returncode:
        raise RuntimeError(process.stderr[-2000:])
    result = parse_measurement(process.stderr)
    if hashlib.sha256(media.read_bytes()).hexdigest() != before:
        raise ValueError('Input changed during loudness measurement')
    return {'sha256': before, 'dual_mono': False, **result}


def second_pass(measured, target=-18, peak=-3):
    return (f'loudnorm=I={target}:TP={peak}:LRA=11:'
            f'measured_I={measured["input_i"]}:measured_TP={measured["input_tp"]}:'
            f'measured_LRA={measured["input_lra"]}:measured_thresh={measured["input_thresh"]}:'
            f'offset={measured["target_offset"]}:linear=true:dual_mono=false:print_format=json')


def findings(measured, minimum=-20, maximum=-16, ceiling=-2):
    if not all(math.isfinite(value) for value in (minimum, maximum, ceiling)) or minimum > maximum:
        raise ValueError('Expected finite ordered loudness bounds and peak ceiling')
    issues = []
    if not minimum <= measured['input_i'] <= maximum:
        issues.append('Integrated loudness outside selected speech delivery range')
    if measured['input_tp'] > ceiling:
        issues.append('Decoded true peak exceeds selected ceiling; correct and re-encode')
    return issues


def normalization_settings(minimum, maximum, ceiling, target=None, peak=None):
    findings({'input_i': minimum, 'input_tp': ceiling}, minimum, maximum, ceiling)
    target = (minimum + maximum) / 2 if target is None else target
    peak = max(-9, min(-3, ceiling - 1)) if peak is None else peak
    if not (math.isfinite(target) and -70 <= target <= -5 and minimum <= target <= maximum):
        raise ValueError('Normalization target must fit the delivery range and loudnorm -70..-5 LUFS')
    if not (math.isfinite(peak) and -9 <= peak <= 0 and peak <= ceiling):
        raise ValueError('Pre-encode peak must fit the ceiling and loudnorm -9..0 dBTP')
    return target, peak


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('media', type=Path)
    parser.add_argument('--ffmpeg', default='ffmpeg')
    parser.add_argument('--min-lufs', type=float, default=-20)
    parser.add_argument('--max-lufs', type=float, default=-16)
    parser.add_argument('--max-dbtp', type=float, default=-2)
    parser.add_argument('--target-lufs', type=float)
    parser.add_argument('--target-dbtp', type=float)
    args = parser.parse_args()
    target, peak = normalization_settings(args.min_lufs, args.max_lufs, args.max_dbtp,
                                          args.target_lufs, args.target_dbtp)
    measured = measure(args.media, args.ffmpeg, target, peak)
    issues = findings(measured, args.min_lufs, args.max_lufs, args.max_dbtp)
    print(json.dumps({'measurement': measured, 'issues': issues,
                      'second_pass_filter': second_pass(measured, target, peak),
                      'listening_review': 'pending'}, indent=2))
    raise SystemExit(1 if issues else 0)


if __name__ == '__main__':
    main()
