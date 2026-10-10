"""Portable bounded frame renderer and encoded QA, adapted from production contracts."""
import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import sys
import uuid

sys.dont_write_bytecode = True
from audio import audit, check_cues
from project import capture, outside, ownership, validate


def run(command):
    result = subprocess.run(command, capture_output=True, check=True)
    return result.stdout


def media_qa(path, frames, fps, width, height, ffmpeg, ffprobe, require_audio=True, expected_audio_seconds=None):
    probe = json.loads(run([ffprobe, '-v', 'error', '-count_frames', '-show_streams',
                            '-show_format', '-of', 'json', str(path)]))
    video = next(s for s in probe['streams'] if s['codec_type'] == 'video')
    if int(video['nb_read_frames']) != frames or (video['width'], video['height']) != (width, height):
        raise ValueError('Encoded frame count/dimensions mismatch')
    if video['avg_frame_rate'] != f'{fps}/1' or video['r_frame_rate'] != f'{fps}/1':
        raise ValueError('Encoded FPS mismatch')
    timestamps = json.loads(run([ffprobe, '-v', 'error', '-select_streams', 'v:0',
                                 '-show_entries', 'frame=best_effort_timestamp_time',
                                 '-of', 'json', str(path)]))['frames']
    if len(timestamps) != frames:
        raise ValueError('Incomplete decoded timeline')
    error = max(abs(float(f['best_effort_timestamp_time']) - i / fps) for i, f in enumerate(timestamps))
    tick_numerator, tick_denominator = map(int, video['time_base'].split('/'))
    timestamp_tolerance = tick_numerator / tick_denominator / 2 + 0.000001
    if error > timestamp_tolerance:
        raise ValueError('Encoded timestamps mismatch')
    decode = subprocess.run([ffmpeg, '-v', 'error', '-xerror', '-i', str(path), '-f', 'null', '-'], capture_output=True, check=True)
    if decode.stderr.strip():
        raise ValueError('Full decode reported errors')
    audio = [s for s in probe['streams'] if s['codec_type'] == 'audio']
    if require_audio and not audio:
        raise ValueError('Missing encoded narration')
    if expected_audio_seconds is not None:
        if not audio or abs(float(audio[0]['duration']) - expected_audio_seconds) > .05:
            raise ValueError('Encoded audio duration differs from the preserved narration range')
    if abs(float(video.get('duration', frames / fps)) - frames / fps) > 0.002:
        raise ValueError('Encoded video duration mismatch')
    return {'frames': frames, 'fps': fps, 'width': width, 'height': height,
            'duration': float(probe['format']['duration']), 'bytes': path.stat().st_size,
            'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
            'max_timestamp_error': error, 'timestamp_tolerance': timestamp_tolerance, 'full_decode_errors': 0,
            'audio': audio, 'perceptual_playback': False}


def frame_hashes(path, ffmpeg):
    output = run([ffmpeg, '-v', 'error', '-i', str(path), '-map', '0:v:0', '-f', 'framemd5', '-']).decode()
    return [r.split(',')[-1].strip() for r in output.splitlines() if r and not r.startswith('#')]


def geometry_issues(objects, width, height, caption_box):
    """Conservative transformed envelopes. Intentional overlaps need visual triage."""
    issues = []
    def intersect(a, b):
        return a[0] < b[2] and a[2] > b[0] and a[1] < b[3] and a[3] > b[1]
    labels = [o for o in objects if o['kind'] == 'label' and o.get('alpha', 1) >= .25]
    for item in objects:
        box = item['box']
        if len(box) != 4 or not all(math.isfinite(v) for v in box):
            raise ValueError('Invalid transformed geometry')
        if box[0] < 0 or box[1] < 0 or box[2] > width or box[3] > height:
            issues.append(f"crop:{item['id']}")
        if item['kind'] != 'caption' and intersect(box, caption_box):
            issues.append(f"caption:{item['id']}")
        if item['kind'] in ('actor', 'pulse', 'glow', 'path'):
            for label in labels:
                if intersect(box, label['box']):
                    issues.append(f"{item['kind']}/label:{item['id']}/{label['id']}")
    for index, label in enumerate(labels):
        for other in labels[index + 1:]:
            if intersect(label['box'], other['box']):
                issues.append(f"label/label:{label['id']}/{other['id']}")
    return sorted(set(issues))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('--lock', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--mode', choices=['sample', 'full'], default='sample')
    parser.add_argument('--review', type=Path, help='Full-mode evidence receipt; see tools reference')
    parser.add_argument('--seconds', type=float, help='Sample excerpt only; full mode always uses the whole WAV')
    parser.add_argument('--start', type=float, default=0)
    parser.add_argument('--batch', type=int, default=72)
    parser.add_argument('--stop-file', type=Path)
    parser.add_argument('--ffmpeg', default='ffmpeg')
    parser.add_argument('--ffprobe', default='ffprobe')
    parser.add_argument('--resume', action='store_true')
    parser.add_argument('--preflight-only', action='store_true')
    args = parser.parse_args()
    if not 1 <= args.batch <= 240:
        parser.error('batch must be 1..240')
    for tool in (args.ffmpeg, args.ffprobe):
        if not shutil.which(tool):
            parser.error(f'Missing media tool: {tool}')
    root = args.root.resolve()
    output = outside(root, args.output)
    outside(root, args.lock)
    lock = json.loads(args.lock.read_text(encoding='utf-8'))
    validate(root, lock)
    config = json.loads((root / 'project.json').read_text(encoding='utf-8'))
    width, height, fps = (config[k] for k in ('width', 'height', 'fps'))
    if any(not isinstance(n, int) for n in (width, height, fps)) or width < 32 or height < 32 or width % 2 or height % 2 or not 1 <= fps <= 60:
        raise ValueError('Use positive even dimensions and integer FPS 1..60')
    def source(name):
        path = (root / name).resolve()
        if not path.is_relative_to(root):
            raise ValueError('Input outside source project')
        return path
    wav = source(config['wav'])
    audio = audit(wav)
    if audio['clipped_samples']:
        raise ValueError('Clipped PCM narration')
    words = json.loads(source(config['words']).read_text(encoding='utf-8'))
    check_cues(source(config['script']).read_text(encoding='utf-8'), words, audio['duration'])
    duration = audio['duration']
    if args.mode == 'full':
        if args.start or args.seconds:
            raise ValueError('Full production must cover the exact whole WAV')
        if not args.review:
            raise ValueError('Full production requires the sample and animatic review receipt')
        receipt = json.loads(args.review.read_text(encoding='utf-8'))
        if receipt['source_set_sha256'] != lock['source_set_sha256']:
            raise ValueError('Review receipt approves different sources')
        for gate in ('sample_approved', 'technical_preflight', 'editorial_animatic'):
            if not receipt.get(gate, {}).get('passed') or not receipt[gate].get('evidence'):
                raise ValueError(f'Missing review gate: {gate}')
    else:
        duration = min(duration - args.start, args.seconds if args.seconds is not None else 30)
    if args.start < 0 or duration <= 0:
        raise ValueError('Invalid sample range')
    frames = math.ceil(duration * fps)
    renderer_path = source(config.get('renderer', 'scene.py'))
    spec = importlib.util.spec_from_file_location('episode_scene', renderer_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    output.parent.mkdir(parents=True, exist_ok=True)
    work = output.with_name(output.stem + '-work')
    work.mkdir(exist_ok=True)
    signature = {'lock': lock['source_set_sha256'], 'frames': frames, 'fps': fps,
                 'width': width, 'height': height, 'start': args.start}
    def stopped():
        if args.stop_file and args.stop_file.exists():
            raise InterruptedError('Stop signal; no final output promoted')
    with ownership(root / '.render.lock'), ownership(output.with_name(output.name + '.lock')):
        if output.exists():
            raise FileExistsError('Preserve valid output; use a new revision name')
        stopped()
        issues = []
        from PIL import Image
        for index in range(frames):
            stopped()
            t = args.start + index / fps
            image, objects = module.frame(t, config, root)
            if image.size != (width, height):
                raise ValueError('Renderer dimensions mismatch')
            for issue in geometry_issues(objects, width, height, config['caption_box']):
                issues.append({'frame': index, 'time': t, 'issue': issue})
        validate(root, lock)
        report = {'frames': frames, 'source_set_sha256': lock['source_set_sha256'],
                  'issues': issues, 'scope': 'Every requested frame: conservative crop/caption/path-pulse/glow/actor/label envelopes. Alpha and intentional occlusion need visual triage.'}
        (work / 'geometry.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
        if issues:
            raise ValueError('Geometry issues; see work/geometry.json; repair or explicitly classify in renderer and visually recheck')
        if args.preflight_only:
            print(json.dumps(report))
            return
        parts, expected_hashes = [], []
        for begin in range(0, frames, args.batch):
            stopped()
            validate(root, lock)
            count = min(args.batch, frames - begin)
            part = work / f'part-{begin:08d}.mkv'
            receipt = part.with_suffix('.json')
            key = {**signature, 'begin': begin, 'count': count}
            if args.resume and part.exists() and receipt.exists():
                saved = json.loads(receipt.read_text())
                if saved['signature'] != key or saved['sha256'] != hashlib.sha256(part.read_bytes()).hexdigest():
                    raise ValueError('Stale or corrupt resume batch')
            else:
                partial = part.with_name(part.stem + '.partial.mkv')
                command = [args.ffmpeg, '-v', 'error', '-y', '-f', 'rawvideo', '-pixel_format', 'rgb24',
                           '-video_size', f'{width}x{height}', '-framerate', str(fps), '-i', 'pipe:0',
                           '-c:v', 'ffv1', '-pix_fmt', 'bgr0', str(partial)]
                with (work / f'encode-{begin}.log').open('wb') as log:
                    process = subprocess.Popen(command, stdin=subprocess.PIPE, stderr=log)
                    try:
                        for index in range(begin, begin + count):
                            stopped()
                            image, _ = module.frame(args.start + index / fps, config, root)
                            process.stdin.write(image.convert('RGB').tobytes())
                        process.stdin.close()
                        if process.wait() != 0:
                            raise ValueError('Batch encoder failed')
                    finally:
                        if process.poll() is None:
                            process.kill()
                            process.wait()
                validate(root, lock)
                media_qa(partial, count, fps, width, height, args.ffmpeg, args.ffprobe, False)
                os.replace(partial, part)
                receipt.write_text(json.dumps({'signature': key, 'sha256': hashlib.sha256(part.read_bytes()).hexdigest()}))
            hashes = frame_hashes(part, args.ffmpeg)
            if len(hashes) != count:
                raise ValueError('Batch decoded frame count mismatch')
            expected_hashes += hashes
            parts.append(part)
            print(json.dumps({'frames_complete': begin + count, 'frames_total': frames}), flush=True)
        concat = work / 'concat.txt'
        # Only safe generated basenames enter the concat format; arbitrary paths never do.
        concat.write_text('ffconcat version 1.0\n' + ''.join(f"file '{p.name}'\n" for p in parts), encoding='utf-8')
        joined = work / 'joined.mkv'
        run([args.ffmpeg, '-v', 'error', '-y', '-f', 'concat', '-safe', '1', '-i', str(concat), '-c', 'copy', str(joined)])
        if frame_hashes(joined, args.ffmpeg) != expected_hashes:
            raise ValueError('Joined decoded frames differ from the exact batch sequence')
        validate(root, lock)
        partial = output.with_name(output.stem + '.' + uuid.uuid4().hex + '.partial.mp4')
        run([args.ffmpeg, '-v', 'error', '-y', '-i', str(joined), '-ss', str(args.start), '-i', str(wav),
             '-t', str(frames / fps), '-map', '0:v:0', '-map', '1:a:0', '-c:v', 'libx264', '-crf', str(config.get('crf', 18)),
             '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-b:a', '96k', '-movflags', '+faststart', str(partial)])
        qa = media_qa(partial, frames, fps, width, height, args.ffmpeg, args.ffprobe, expected_audio_seconds=duration)
        stopped()
        validate(root, lock)
        if audit(wav)['sha256'] != audio['sha256']:
            raise ValueError('Exact source WAV changed')
        os.replace(partial, output)
        qa.update({'source_set_sha256': lock['source_set_sha256'], 'source_wav': audio,
                   'mode': args.mode, 'start': args.start, 'source_seconds': duration,
                   'visual_tail_seconds': frames / fps - duration, 'joined_frames_verified': True,
                   'editorial_verdict': 'pending independent encoded review'})
        output.with_suffix('.qa.json').write_text(json.dumps(qa, indent=2), encoding='utf-8')
        print(json.dumps(qa, indent=2))


if __name__ == '__main__':
    main()
