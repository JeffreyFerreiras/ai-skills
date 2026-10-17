#!/usr/bin/env python3
"""Local video project helpers. No installation, synthesis, upload, or review approval."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import shutil
import subprocess
import sys
import tempfile
from fractions import Fraction
from pathlib import Path

from check_video_spec import validate_spec


SKILL = Path(__file__).resolve().parents[1]


def read_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                     ensure_ascii=False, allow_nan=False).encode()).hexdigest()


def file_hash(path):
    hasher = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def atomic_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=path.parent,
                                         delete=False) as stream:
            temporary = Path(stream.name)
            json.dump(value, stream, indent=2, ensure_ascii=False, allow_nan=False)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def local_path(project, name):
    relative = Path(name)
    if relative.is_absolute() or ".." in relative.parts:
        raise ValueError(f"Expected a project-relative path: {name}")
    path = project / relative
    for part in (path, *path.parents):
        if part == project:
            break
        if part.is_symlink() or (hasattr(part, "is_junction") and part.is_junction()):
            raise ValueError(f"Linked paths are unsupported: {name}")
    if not path.resolve().is_relative_to(project):
        raise ValueError(f"Path escapes the project: {name}")
    return path


def initialize(project):
    if project.exists():
        raise ValueError("Choose a new project directory; init never merges or overwrites work")
    shutil.copytree(SKILL / "assets/remotion-starter", project)
    shutil.copyfile(SKILL / "assets/reference-video.json", project / "video-spec.json")
    return {"project": str(project), "installed_dependencies": False}


def preflight(project, spec, capabilities, ffmpeg="ffmpeg", ffprobe="ffprobe"):
    tools = {}
    for name, executable, flag in (("node", "node", "--version"),
                                    ("ffmpeg", ffmpeg, "-version"),
                                    ("ffprobe", ffprobe, "-version")):
        path = shutil.which(executable)
        item = {"path": path, "status": "unavailable"}
        if path:
            try:
                result = subprocess.run([path, flag], capture_output=True, text=True, timeout=15)
                lines = (result.stdout + result.stderr).splitlines()
                item.update(status="available" if result.returncode == 0 else "unavailable",
                            version=lines[0] if lines else "", exit_code=result.returncode)
            except (OSError, subprocess.TimeoutExpired) as error:
                item["error"] = str(error)
        tools[name] = item
    renderer = spec["profile"]["renderer"]
    if renderer == "remotion":
        runtime = project / "node_modules/@remotion/renderer/package.json"
        tools["renderer"] = {"status": "available" if runtime.is_file() else "unavailable",
                             "evidence": str(runtime), "scope": "package presence, not a render test"}
    else:
        tools["renderer"] = {"status": "available" if shutil.which(renderer) else "unavailable",
                             "scope": "executable discovery, not a render test"}
    required = ["independent_review", "playback", "hardware"]
    if spec["profile"]["narrated"]:
        required += ["voice", "alignment", "listening"]
    declared = {}
    for name in required:
        item = capabilities.get(name, {})
        if item.get("status") not in ("available", "unavailable", "unknown"):
            item = {"status": "unknown"}
        if item.get("status") == "available" and not item.get("evidence"):
            item = {"status": "unknown", "reason": "No capability evidence supplied"}
        declared[name] = item
    unresolved = [key for key, item in {**tools, **declared}.items() if item["status"] != "available"]
    return {"ready_to_attempt": not unresolved, "tools": tools, "declared": declared,
            "unresolved": unresolved, "free_bytes": shutil.disk_usage(project).free,
            "scope": "Inventory and operator-supplied evidence; no quality approval or installation"}


def snapshot(project, paths, plan, baseline=None):
    entries = {}
    for name in paths:
        root = local_path(project, name)
        if not root.exists():
            if baseline is None:
                raise ValueError(f"Missing scope path: {name}")
            continue
        candidates = [root, *root.rglob("*")] if root.is_dir() else [root]
        for path in candidates:
            relative = path.relative_to(project).as_posix()
            local_path(project, relative)
            if path.name == ".git":
                raise ValueError("Do not snapshot Git metadata or embedded repositories")
            if path.is_file():
                entries[relative] = {"path": relative, "sha256": file_hash(path),
                                     "mode": path.stat().st_mode, "kind": "file"}
            elif path.is_dir():
                entries[relative] = {"path": relative, "kind": "directory"}
            else:
                raise ValueError(f"Unsupported file kind: {relative}")
    if baseline is not None:
        if digest({k: v for k, v in baseline.items() if k != "digest"}) != baseline.get("digest"):
            raise ValueError("Baseline manifest integrity mismatch")
        if baseline["project"] != str(project) or baseline["scope"] != sorted(set(paths)):
            raise ValueError("Baseline workspace/scope mismatch")
        for entry in baseline["entries"]:
            if entry["path"] not in entries:
                entries[entry["path"]] = (entry if entry["kind"] == "deleted" else
                                          {"path": entry["path"], "kind": "deleted", "base": entry})
    value = {"schema_version": 1, "project": str(project), "scope": sorted(set(paths)),
             "plan": plan, "entries": sorted(entries.values(), key=lambda item: item["path"])}
    return {**value, "digest": digest(value)}


def verify_manifest(manifest):
    content = {key: value for key, value in manifest.items() if key != "digest"}
    if digest(content) != manifest.get("digest"):
        raise ValueError("Manifest integrity mismatch")
    current = snapshot(Path(manifest["project"]), manifest["scope"], manifest["plan"], manifest)
    if current["digest"] != manifest["digest"]:
        raise ValueError("Artifact changed since snapshot; previous evidence is stale")


def record(state_path, manifest, entry):
    if not isinstance(entry, dict) or entry.get("kind") not in ("dispatch", "validation", "review", "checkpoint"):
        raise ValueError("Entry kind must be dispatch, validation, review, or checkpoint")
    if not entry.get("stage") or not entry.get("identity"):
        raise ValueError("Entry requires stage and identity")
    state_path = Path(state_path).resolve()
    project = Path(manifest["project"])
    for scope in manifest["scope"]:
        path = project / scope
        if state_path == path or state_path.is_relative_to(path):
            raise ValueError("Recovery state must be outside candidate scope")
    verify_manifest(manifest)
    state_path.parent.mkdir(parents=True, exist_ok=True)
    lock = state_path.with_suffix(state_path.suffix + ".lock")
    with lock.open("x", encoding="utf-8"):
        pass
    try:
        previous = read_json(state_path) if state_path.exists() else None
        if previous and digest({k: v for k, v in previous.items() if k != "integrity"}) != previous.get("integrity"):
            raise ValueError("Checkpoint integrity mismatch; recover the last good record")
        if previous and previous["project"] != str(project):
            raise ValueError("Checkpoint belongs to another project")
        receipts = previous["receipts"] if previous else []
        value = {"schema_version": 1, "sequence": len(receipts) + 1, "project": str(project),
                 "previous_integrity": previous["integrity"] if previous else None,
                 "receipts": receipts + [{**entry, "artifact_digest": manifest["digest"]}]}
        value["integrity"] = digest(value)
        verify_manifest(manifest)
        if previous:
            atomic_json(state_path.with_suffix(state_path.suffix + ".previous"), previous)
        atomic_json(state_path, value)
        return value
    finally:
        lock.unlink(missing_ok=True)


def preview_frames(spec):
    fps = spec["profile"]["fps"]
    last = math.ceil(spec["profile"]["duration_seconds"] * fps) - 1
    frames = {0, last}
    times = [scene[key] for scene in spec["scenes"] for key in ("start", "end")]
    for event in spec["events"]:
        duration = event.get("duration_seconds", 0)
        times += [event["at"], event["at"] + duration / 2, event["at"] + duration]
    for time in times:
        boundary = math.ceil(time * fps - 1e-8)
        frames.update(frame for frame in (boundary - 1, boundary, boundary + 1) if 0 <= frame <= last)
    return sorted(frames)


def parse_loudness(stderr):
    # FFmpeg can print its final muxing summary after the loudnorm JSON object.
    metrics, _ = json.JSONDecoder().raw_decode(stderr[stderr.rindex("{"):])
    values = [float(metrics[key]) for key in ("input_i", "input_tp")]
    if not all(math.isfinite(value) for value in values):
        raise ValueError("Nonfinite encoded loudness")
    return metrics


def check_media(path, spec, ffmpeg="ffmpeg", ffprobe="ffprobe"):
    probe_command = [ffprobe, "-v", "error", "-show_streams", "-show_format", "-of", "json", str(path)]
    result = subprocess.run(probe_command, capture_output=True, text=True, timeout=60, check=True)
    probe = json.loads(result.stdout)
    video = [s for s in probe["streams"] if s["codec_type"] == "video"]
    audio = [s for s in probe["streams"] if s["codec_type"] == "audio"]
    errors = []
    profile = spec["profile"]
    if len(video) != 1:
        errors.append("Expected exactly one video stream")
    else:
        stream = video[0]
        if (stream["width"], stream["height"]) != (profile["width"], profile["height"]):
            errors.append("Encoded dimensions differ from the spec")
        if abs(float(Fraction(stream["avg_frame_rate"])) - profile["fps"]) > 1e-6:
            errors.append("Encoded frame rate differs from the spec")
    if profile["narrated"] and not audio:
        errors.append("Narrated video has no audio stream")
    for stream in video + audio:
        start = float(stream.get("start_time", 0))
        end = start + float(stream.get("duration", probe["format"]["duration"]))
        if abs(start) > 0.1 or abs(end - profile["duration_seconds"]) > 0.1:
            errors.append(f"Stream {stream['index']} start/end differs by more than 0.1 seconds")
    decode_command = [ffmpeg, "-v", "error", "-xerror", "-nostdin", "-i", str(path),
                      "-map", "0:v", "-map", "0:a?", "-c:v", "rawvideo", "-c:a", "pcm_s16le", "-f", "null", "-"]
    decoded = subprocess.run(decode_command, capture_output=True, text=True, timeout=600)
    if decoded.returncode:
        errors.append("Full decode failed: " + decoded.stderr[-2000:])
    loudness = []
    for index, stream in enumerate(audio):
        command = [ffmpeg, "-hide_banner", "-nostdin", "-i", str(path), "-map", f"0:a:{index}",
                   "-af", "loudnorm=I=-18:TP=-3:LRA=11:dual_mono=false:print_format=json",
                   "-c:a", "pcm_s16le", "-f", "null", "-"]
        measured = subprocess.run(command, capture_output=True, text=True, timeout=600)
        try:
            metrics = parse_loudness(measured.stderr)
            values = [float(metrics[key]) for key in ("input_i", "input_tp")]
            if measured.returncode or not all(math.isfinite(v) for v in values):
                raise ValueError("Nonfinite loudness or failed measurement")
            if values[1] > -2:
                errors.append(f"Audio {index} exceeds the default -2 dBTP ceiling")
            loudness.append({"stream": stream["index"], "input": metrics, "command": command})
        except (ValueError, KeyError):
            errors.append(f"Could not measure finite encoded loudness for audio {index}: {measured.stderr[-1000:]}")
    return {"valid": not errors, "errors": errors, "sha256": file_hash(path), "probe": probe,
            "probe_command": probe_command, "decode_command": decode_command,
            "decode_exit": decoded.returncode, "loudness": loudness,
            "pending": ["visual inspection", "perceptual listening", "content and cue alignment"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    init = sub.add_parser("init")
    init.add_argument("project", type=Path)
    for name in ("preflight", "previews", "media-check"):
        command = sub.add_parser(name)
        command.add_argument("spec", type=Path)
        if name == "preflight":
            command.add_argument("--capabilities", type=Path)
        if name == "media-check":
            command.add_argument("video", type=Path)
        if name != "previews":
            command.add_argument("--ffmpeg", default="ffmpeg")
            command.add_argument("--ffprobe", default="ffprobe")
    snap = sub.add_parser("snapshot")
    snap.add_argument("project", type=Path)
    snap.add_argument("--path", action="append", required=True)
    snap.add_argument("--plan", type=Path, required=True)
    snap.add_argument("--baseline", type=Path)
    rec = sub.add_parser("record")
    rec.add_argument("--state", type=Path, required=True)
    rec.add_argument("--manifest", type=Path, required=True)
    rec.add_argument("--entry", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "init":
            result = initialize(args.project.resolve())
        elif args.command == "snapshot":
            result = snapshot(args.project.resolve(), args.path, read_json(args.plan),
                              read_json(args.baseline) if args.baseline else None)
        elif args.command == "record":
            result = record(args.state, read_json(args.manifest), read_json(args.entry))
        else:
            spec = read_json(args.spec)
            errors = validate_spec(spec, args.spec.parent)
            if errors:
                raise ValueError("; ".join(errors))
            if args.command == "preflight":
                result = preflight(args.spec.parent.resolve(), spec,
                                   read_json(args.capabilities) if args.capabilities else {}, args.ffmpeg, args.ffprobe)
            elif args.command == "previews":
                result = {"frames": preview_frames(spec), "scope": "sample plan, not playback evidence"}
            else:
                result = check_media(args.video, spec, args.ffmpeg, args.ffprobe)
        print(json.dumps(result, indent=2, ensure_ascii=False))
        return 1 if result.get("valid") is False or result.get("ready_to_attempt") is False else 0
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(json.dumps({"error": str(error)}), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
