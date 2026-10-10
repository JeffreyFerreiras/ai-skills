"""Extract explicit decoded frame indices and report actual encoded timestamps."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def extract(video, output, times, ffmpeg='ffmpeg', ffprobe='ffprobe'):
    if output.exists():
        raise FileExistsError('Keep prior review evidence; use a new directory')
    source_hash = hashlib.sha256(video.read_bytes()).hexdigest()
    probe = json.loads(subprocess.check_output([ffprobe, '-v', 'error', '-select_streams', 'v:0',
                                                '-show_entries', 'frame=best_effort_timestamp_time', '-of', 'json', str(video)]))
    timestamps = [float(frame['best_effort_timestamp_time']) for frame in probe['frames']]
    if not timestamps:
        raise ValueError('No decoded frame timestamps')
    indices = set()
    requests = []
    for time in times:
        if not 0 <= time <= timestamps[-1]:
            raise ValueError('Requested time outside encoded frame timeline')
        index = min(range(len(timestamps)), key=lambda index: abs(timestamps[index]-time))
        indices.add(index)
        requests.append({'requested_seconds': time, 'frame_index': index, 'actual_seconds': timestamps[index]})
    indices = sorted(indices)
    if not indices:
        raise ValueError('No review times')
    output.mkdir(parents=True)
    expression = '+'.join(f'eq(n,{index})' for index in indices)
    subprocess.run([ffmpeg, '-v', 'error', '-xerror', '-i', str(video), '-vf', f"select='{expression}'",
                    '-fps_mode', 'vfr', str(output/'frame-%04d.png')], check=True)
    files = sorted(output.glob('frame-*.png'))
    if len(files) != len(indices):
        raise ValueError('Extracted review frame count mismatch')
    if hashlib.sha256(video.read_bytes()).hexdigest() != source_hash:
        raise ValueError('Video changed during review extraction')
    mapping = {index: files[position].name for position, index in enumerate(indices)}
    for request in requests:
        request['file'] = mapping[request['frame_index']]
    report = {'video_sha256': source_hash, 'requests': requests,
              'scope': 'Decoded frame samples only; not continuous playback or perceptual listening'}
    (output/'review-map.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('video', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--times', required=True, help='Comma-separated requested seconds')
    parser.add_argument('--ffmpeg', default='ffmpeg')
    parser.add_argument('--ffprobe', default='ffprobe')
    args = parser.parse_args()
    print(json.dumps(extract(args.video, args.output, [float(value) for value in args.times.split(',')], args.ffmpeg, args.ffprobe), indent=2))
