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


def audio_signature(path, ffprobe='ffprobe'):
    """AAC packet payload, timing, priming and codec identity for exact stream-copy QA."""
    data = json.loads(run([ffprobe, '-v', 'error', '-select_streams', 'a:0',
                           '-show_packets', '-show_streams', '-show_data_hash', 'sha256',
                           '-show_entries', 'packet=pts_time,dts_time,duration_time,size,data_hash:'
                           'packet_side_data=side_data_type,skip_samples,discard_padding:'
                           'stream=codec_name,profile,sample_rate,channels,channel_layout,extradata_hash',
                           '-of', 'json', str(path)]))
    packets = data.get('packets', [])
    if not packets or any(not all(key in packet for key in
                                  ('pts_time', 'dts_time', 'duration_time', 'size', 'data_hash'))
                          for packet in packets):
        raise ValueError('Incomplete audio packet/timestamp evidence')
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('master', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--max-bytes', type=int, required=True)
    parser.add_argument('--width', type=int, default=540)
    parser.add_argument('--copy-audio', action='store_true',
                        help='Preserve approved AAC packets/timestamps; compress only video')
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
    signature = None
    audio_bitrate = 64000
    if args.copy_audio:
        if master_audio['codec_name'] != 'aac':
            raise ValueError('Copy-audio phone route requires approved AAC input')
        signature = audio_signature(args.master, args.ffprobe)
        audio_bitrate = sum(int(packet['size']) for packet in signature['packets'])*8/duration
    bitrate = math.floor(args.max_bytes*8*.94/duration - audio_bitrate)
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
        audio_options = ['-c:a', 'copy'] if args.copy_audio else ['-c:a', 'aac', '-b:a', '64k']
        run(common+['-pass', '2', '-map', '0:a:0', *audio_options, '-movflags', '+faststart', str(partial)])
        qa = media_qa(partial, frames, fps, args.width, height, args.ffmpeg, args.ffprobe, expected_audio_seconds=audio_seconds)
        if signature is not None:
            if audio_signature(partial, args.ffprobe) != signature:
                raise ValueError('Approved AAC packets, timing or codec identity changed')
            qa['copied_audio_signature_sha256'] = hashlib.sha256(
                json.dumps(signature, sort_keys=True).encode()).hexdigest()
            qa['copied_audio_packets_and_timing_verified'] = True
        if hashlib.sha256(args.master.read_bytes()).hexdigest() != master_hash:
            raise ValueError('Master changed during phone encoding')
        if qa['bytes'] > args.max_bytes:
            raise ValueError('Actual phone copy exceeds budget; revise bitrate/size and review again')
        os.replace(partial, args.output)
    qa['phone_legibility_review'] = 'pending'
    qa['master_sha256'] = master_hash
    qa['audio_mode'] = 'stream-copy' if args.copy_audio else 're-encoded; renewed listening required'
    args.output.with_suffix('.qa.json').write_text(json.dumps(qa, indent=2), encoding='utf-8')
    print(json.dumps(qa, indent=2))


if __name__ == '__main__':
    main()
