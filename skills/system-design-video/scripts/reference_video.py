#!/usr/bin/env python3
"""Prepare or render the small synthetic integration fixture in a separate project."""

from __future__ import annotations

import argparse
import json
import math
import shutil
import subprocess
import sys
import wave
from pathlib import Path

from check_video_spec import validate_spec
from video_project import SKILL, atomic_json, check_media, file_hash, initialize, preview_frames, read_json


def subtitle_time(seconds):
    milliseconds = round(seconds * 1000)
    hours, remainder = divmod(milliseconds, 3600000)
    minutes, remainder = divmod(remainder, 60000)
    seconds, milliseconds = divmod(remainder, 1000)
    return f"{hours:02}:{minutes:02}:{seconds:02},{milliseconds:03}"


def prepare(project, voice_dir=None, synthesize_windows=False):
    initialize(project)
    spec = read_json(project / "video-spec.json")
    if synthesize_windows:
        if sys.platform != "win32":
            raise ValueError("Windows fixture speech is unavailable; supply --voice-dir instead")
        result = subprocess.run(["powershell.exe", "-NoProfile", "-NonInteractive", "-File",
                                 str(SKILL / "scripts/reference_voice.ps1"), "-Project", str(project)],
                                capture_output=True, text=True, timeout=60, check=True)
        atomic_json(project / "voice-provenance.json", json.loads(result.stdout))
    else:
        target = project / "public/audio"
        target.mkdir(parents=True)
        for scene in spec["scenes"]:
            shutil.copyfile(voice_dir / f"{scene['id']}.wav", target / f"{scene['id']}.wav")
        atomic_json(project / "voice-provenance.json", {"source": str(voice_dir), "scope": "supplied fixture audio"})
    audio_hashes = {}
    for scene in spec["scenes"]:
        path = project / "public/audio" / f"{scene['id']}.wav"
        with wave.open(str(path), "rb") as clip:
            duration = clip.getnframes() / clip.getframerate()
        scene["narration"].update(file=path.relative_to(project).as_posix(),
                                  duration_seconds=duration, timing_basis="measured")
        audio_hashes[path.name] = file_hash(path)
    before_errors = validate_spec(spec, project, ready=True)
    offset = 0
    for index, scene in enumerate(spec["scenes"]):
        voice = scene["narration"]
        # Extend visual windows only. Never alter the measured speech or scale word cues.
        length = math.ceil((voice["duration_seconds"] + 0.5) * spec["profile"]["fps"]) / spec["profile"]["fps"]
        scene.update(start=offset, end=offset + length)
        spec["events"][index].update(at=offset + 0.1, duration_seconds=min(1.0, length - 0.2))
        spec["captions"][index].update(start=offset, end=offset + voice["duration_seconds"])
        offset += length
    spec["profile"]["duration_seconds"] = offset
    errors = validate_spec(spec, project, ready=True)
    if errors:
        raise ValueError("; ".join(errors))
    atomic_json(project / "video-spec.json", spec)
    cues = spec["captions"]
    (project / "subtitles.srt").write_text("\n\n".join(
        f"{i + 1}\n{subtitle_time(cue['start'])} --> {subtitle_time(cue['end'])}\n{cue['text']}"
        for i, cue in enumerate(cues)) + "\n", encoding="utf-8")
    report = {"before_retiming_errors": before_errors, "after_retiming_errors": errors,
              "duration_seconds": offset, "audio_sha256": audio_hashes,
              "cue_precision": "scene-level measured clips; not word alignment",
              "scope": "Synthetic integration fixture, no production approval"}
    atomic_json(project / "retiming-report.json", report)
    return report


def render(project, ffmpeg, ffprobe, browser=None):
    spec = read_json(project / "video-spec.json")
    errors = validate_spec(spec, project, ready=True)
    if errors:
        raise ValueError("; ".join(errors))
    node = shutil.which("node")
    cli = project / "node_modules/@remotion/cli/remotion-cli.js"
    if not node or not cli.is_file():
        raise ValueError("Run npm ci in the fixture project before rendering")
    output = project / "outputs"
    output.mkdir(exist_ok=False)
    video = output / "reference.mp4"
    common = [node, str(cli)]
    browser_args = [f"--browser-executable={browser}"] if browser else []
    render_command = common + ["render", "src/index.jsx", "Explainer", str(video), "--concurrency=2", *browser_args]
    subprocess.run(render_command, cwd=project, check=True, timeout=600)
    atomic_json(output / "render-command.json", {"command": render_command})
    return inspect(project, ffmpeg, ffprobe)


def inspect(project, ffmpeg, ffprobe):
    spec = read_json(project / "video-spec.json")
    output = project / "outputs"
    video = output / "reference.mp4"
    media = check_media(video, spec, ffmpeg, ffprobe)
    frames = preview_frames(spec)
    # Keep the fixture render small while covering travel, completion and the cut.
    selected = sorted(set([frames[0], frames[-1], *[
        math.ceil((event["at"] + event["duration_seconds"] * fraction) * spec["profile"]["fps"])
        for event in spec["events"] for fraction in (0, 0.5, 1)]]))
    for frame in selected:
        destination = output / f"encoded-{media['sha256'][:16]}-{frame:04}.png"
        subprocess.run([ffmpeg, "-v", "error", "-nostdin", "-y", "-i", str(video),
                        "-vf", f"trim=start_frame={frame}:end_frame={frame + 1}", "-frames:v", "1",
                        "-fps_mode", "passthrough", "-c:v", "png", str(destination)],
                       check=True, timeout=120)
    media.update(sampled_frames=selected, sample_source="decoded final MP4")
    atomic_json(output / "media-report.json", media)
    if not media["valid"]:
        raise ValueError("; ".join(media["errors"]))
    return media


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prep = commands.add_parser("prepare")
    prep.add_argument("project", type=Path)
    voice = prep.add_mutually_exclusive_group(required=True)
    voice.add_argument("--voice-dir", type=Path)
    voice.add_argument("--synthesize-windows", action="store_true")
    for name in ("render", "inspect"):
        run = commands.add_parser(name)
        run.add_argument("project", type=Path)
        run.add_argument("--ffmpeg", default="ffmpeg")
        run.add_argument("--ffprobe", default="ffprobe")
        if name == "render":
            run.add_argument("--browser")
    args = parser.parse_args()
    try:
        if args.command == "prepare":
            result = prepare(args.project.resolve(), args.voice_dir, args.synthesize_windows)
        elif args.command == "render":
            result = render(args.project.resolve(), args.ffmpeg, args.ffprobe, args.browser)
        else:
            result = inspect(args.project.resolve(), args.ffmpeg, args.ffprobe)
        print(json.dumps(result, indent=2))
        return 0
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
