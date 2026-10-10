"""Size-budgeted separate phone encode, with final decode and timestamp checks."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import tempfile
from project import ownership
from render import media_qa, run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('master', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--max-bytes', type=int, required=True)
    parser.add_argument('--width', type=int, default=540)
    parser.add_argument('--ffmpeg', default='ffmpeg')
    parser.add_argument('--ffprobe', default='ffprobe')
    args = parser.parse_args()
    if args.output.exists() or args.output.resolve() == args.master.resolve():
        raise FileExistsError('Preserve master and existing output')
    master_hash = hashlib.sha256(args.master.read_bytes()).hexdigest()
    probe = json.loads(run([args.ffprobe, '-v', 'error', '-count_frames', '-show_streams', '-show_format', '-of', 'json', str(args.master)]))
    video = next(s for s in probe['streams'] if s['codec_type'] == 'video')
    master_audio = next(s for s in probe['streams'] if s['codec_type'] == 'audio')
    audio_seconds = float(master_audio['duration'])
    numerator, denominator = map(int, video['avg_frame_rate'].split('/'))
    if denominator != 1 or args.width < 32 or args.width % 2:
        raise ValueError('Expected integer FPS and positive even phone width')
    fps, frames = numerator, int(video['nb_read_frames'])
    duration = frames/fps
    if abs(audio_seconds - duration) > max(1/fps, .05):
        raise ValueError('Master audio does not cover the complete video timeline')
    height = round(args.width*video['height']/video['width']/2)*2
    bitrate = math.floor(args.max_bytes*8*.94/duration - 64000)
    if bitrate < 50000:
        raise ValueError('Size budget too small for useful phone video')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with ownership(args.output.with_name(args.output.name+'.lock')), tempfile.TemporaryDirectory(dir=args.output.parent) as temporary:
        if args.output.exists():
            raise FileExistsError('Preserve output created by another completed pipeline')
        temp = Path(temporary)
        partial = temp/'phone.mp4'
        common = [args.ffmpeg, '-v', 'error', '-y', '-i', str(args.master), '-map', '0:v:0',
                  '-vf', f'scale={args.width}:{height}:flags=lanczos', '-c:v', 'libx264', '-b:v', str(bitrate),
                  '-pix_fmt', 'yuv420p', '-passlogfile', str(temp/'pass')]
        run(common+['-pass', '1', '-an', '-f', 'null', os.devnull])
        run(common+['-pass', '2', '-map', '0:a:0', '-c:a', 'aac', '-b:a', '64k', '-movflags', '+faststart', str(partial)])
        qa = media_qa(partial, frames, fps, args.width, height, args.ffmpeg, args.ffprobe, expected_audio_seconds=audio_seconds)
        if hashlib.sha256(args.master.read_bytes()).hexdigest() != master_hash:
            raise ValueError('Master changed during phone encoding')
        if qa['bytes'] > args.max_bytes:
            raise ValueError('Actual phone copy exceeds budget; revise bitrate/size and review again')
        os.replace(partial, args.output)
    qa['phone_legibility_review'] = 'pending'
    qa['master_sha256'] = master_hash
    args.output.with_suffix('.qa.json').write_text(json.dumps(qa, indent=2), encoding='utf-8')
    print(json.dumps(qa, indent=2))


if __name__ == '__main__':
    main()
