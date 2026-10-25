#!/usr/bin/env python3
"""Check story-bridge implementation and encoded evidence, not aesthetic quality."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import math
import re
import stat
import subprocess
from fractions import Fraction
from pathlib import Path


PHASES = ("before", "entry", "action", "exit", "after")
STORY_FORMS = {"contextual_story", "visual_metaphor", "illustrative_cutaway"}
STAGES = ("plan", "implemented", "encoded")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def text(value, name):
    require(isinstance(value, str) and bool(value.strip()), f"{name}: nonempty text required")
    return value


def integer(value, name, minimum=0):
    require(type(value) is int and value >= minimum, f"{name}: integer >= {minimum} required")
    return value


def file_hash(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def candidate_digest(record):
    candidate = {key: value for key, value in record.items() if key != "reviews"}
    return hashlib.sha256(json.dumps(candidate, sort_keys=True, separators=(",", ":"),
                                     ensure_ascii=False, allow_nan=False).encode()).hexdigest()


def project_path(project, name):
    text(name, "path")
    relative = Path(name)
    require(not relative.is_absolute() and not relative.drive and ".." not in relative.parts
            and ".git" not in relative.parts, f"Expected project-relative artifact: {name}")
    path = project / relative
    for part in (path, *path.parents):
        if part == project:
            break
        try:
            info = part.lstat()
        except FileNotFoundError:
            continue
        require(not stat.S_ISLNK(info.st_mode) and not getattr(info, "st_file_attributes", 0) & 0x400,
                f"Linked artifacts are unsupported: {name}")
    require(path.resolve().is_relative_to(project), f"Artifact escapes project: {name}")
    return path


def artifact(project, reference):
    require(isinstance(reference, dict), "Artifact needs path and sha256")
    expected = reference.get("sha256")
    require(isinstance(expected, str) and re.fullmatch(r"[0-9a-f]{64}", expected),
            "Artifact needs a lowercase SHA-256")
    path = project_path(project, reference.get("path"))
    require(path.is_file(), f"Missing artifact: {reference.get('path')}")
    require(file_hash(path) == expected, f"Stale artifact: {reference['path']}")
    return path


def probe_video(path, ffprobe):
    command = [ffprobe, "-v", "error", "-select_streams", "v:0", "-show_entries",
               "stream=width,height,avg_frame_rate,nb_frames", "-of", "json", str(path)]
    result = subprocess.run(command, capture_output=True, timeout=120, check=True)
    streams = json.loads(result.stdout).get("streams", [])
    require(len(streams) == 1, "Encoded evidence needs one selected video stream")
    stream = streams[0]
    return {"fps": float(Fraction(stream["avg_frame_rate"])),
            "frame_count": int(stream["nb_frames"]), "width": int(stream["width"]),
            "height": int(stream["height"])}


def encoded_frame(path, frame, ffmpeg):
    command = [ffmpeg, "-v", "error", "-nostdin", "-i", str(path), "-map", "0:v:0",
               "-vf", f"select=eq(n\\,{frame})", "-fps_mode", "vfr", "-frames:v", "1",
               "-c:v", "png", "-threads", "1", "-f", "image2pipe", "-"]
    result = subprocess.run(command, capture_output=True, timeout=120, check=True)
    require(result.stdout.startswith(b"\x89PNG\r\n\x1a\n"), f"No encoded PNG at frame {frame}")
    return result.stdout


def check_plan(record):
    require(isinstance(record, dict) and type(record.get("schema_version")) is int and record["schema_version"] == 1,
            "Expected story-bridge record schema_version 1")
    text(record.get("episode_id"), "episode_id")
    creator = text(record.get("creator_id"), "creator_id")
    director = text(record.get("director_id"), "director_id")
    require(creator != director, "Scene director must be a non-author")
    fps = record.get("fps")
    require(type(fps) in (int, float) and math.isfinite(fps) and fps > 0, "fps must be positive and finite")
    count = integer(record.get("frame_count"), "frame_count", 1)
    bridges = record.get("bridges")
    require(isinstance(bridges, list) and bool(bridges), "At least one storytelling bridge is required")
    identifiers = set()
    references = record.get("references")
    require(isinstance(references, dict), "Separate style and storytelling references are required")
    for kind in ("style", "storytelling"):
        reference = references.get(kind)
        require(isinstance(reference, dict), f"{kind} reference record required")
        require(reference.get("status") in ("inspected", "unavailable"), f"{kind} reference inspection status required")
        text(reference.get("locator"), f"{kind}.locator")
        text(reference.get("observations"), f"{kind}.observations or access limit")
        if reference["status"] == "inspected":
            text(reference.get("timecodes"), f"{kind}.timecodes")
    for bridge in bridges:
        require(isinstance(bridge, dict), "Bridge must be an object")
        identifier = text(bridge.get("id"), "bridge.id")
        require(re.fullmatch(r"[a-z0-9_-]+", identifier) and identifier not in identifiers,
                "Bridge IDs must be unique lowercase filename-safe names")
        identifiers.add(identifier)
        require(bridge.get("visual_form") in STORY_FORMS,
                f"{identifier}: need a storytelling scene, not a diagram transform or transition effect")
        for name in ("context", "subject", "story_action", "incoming_idea", "next_idea",
                     "semantic_link", "continuity_anchor", "return_to_explanation"):
            text(bridge.get(name), f"{identifier}.{name}")
        scene_ids = [text(bridge.get(key), f"{identifier}.{key}")
                     for key in ("incoming_scene_id", "scene_id", "next_scene_id")]
        require(len(set(scene_ids)) == 3, f"{identifier}: bridge must have its own scene between two ideas")
        start = integer(bridge.get("start_frame"), f"{identifier}.start_frame")
        end = integer(bridge.get("end_frame"), f"{identifier}.end_frame", 1)
        samples = bridge.get("sample_frames")
        require(isinstance(samples, dict) and set(samples) == set(PHASES),
                f"{identifier}: before, entry, action, exit and after samples required")
        values = [integer(samples[phase], f"{identifier}.{phase}") for phase in PHASES]
        before, entry, action, exit_frame, after = values
        require(0 <= before < start <= entry < action < exit_frame < end <= after < count,
                f"{identifier}: samples must span the bridge and both neighboring scenes")
    return bridges


def check_implementation(bridges, project):
    for bridge in bridges:
        implementation = bridge.get("implementation")
        require(isinstance(implementation, dict), f"{bridge['id']}: implemented render mapping required")
        artifact(project, implementation.get("source"))
        text(implementation.get("locator"), f"{bridge['id']}.implementation.locator")
        assets = implementation.get("assets", [])
        require(isinstance(assets, list), "implementation.assets must be an array")
        for reference in assets:
            artifact(project, reference)


def check_reviews(record, bridges):
    reviews = record.get("reviews")
    require(isinstance(reviews, list) and len(reviews) == 2,
            "Current independent visual and logical bridge reviews are required")
    roles, identities = set(), {record["creator_id"], record["director_id"]}
    expected_frames = {f"{bridge['id']}:{phase}" for bridge in bridges for phase in PHASES}
    expected_digest = candidate_digest(record)
    for review in reviews:
        require(isinstance(review, dict), "Review must be an object")
        role = review.get("role")
        require(role in ("visual", "logical") and role not in roles, "Distinct visual and logical roles required")
        roles.add(role)
        identity = text(review.get("reviewer_id"), "reviewer_id")
        require(identity not in identities, "Bridge reviewers must be distinct from creator, director and each other")
        identities.add(identity)
        require(review.get("candidate_digest") == expected_digest, "Stale bridge review candidate digest")
        require(review.get("decision") == "APPROVE", f"{role}: bridge review has not approved")
        inspected = review.get("inspected_frames")
        require(isinstance(inspected, list) and all(isinstance(item, str) for item in inspected)
                and set(inspected) == expected_frames, f"{role}: every bridge phase must be inspected")
        require(review.get("coverage") in ("sampled_frames", "continuous_playback"),
                "State actual sampled-frame versus playback coverage")
        observations = review.get("observations")
        require(isinstance(observations, dict) and set(observations) == {b["id"] for b in bridges},
                f"{role}: observations required for every bridge")
        for identifier, observation in observations.items():
            require(isinstance(observation, dict), "Bridge observation must be an object")
            for key in ("story_scene_present", "introduces_next_idea", "continuity_clear", "caption_safe"):
                require(observation.get(key) is True, f"{role}:{identifier}: {key} was not accepted")
            for key in ("observed_story_action", "observed_connection"):
                text(observation.get(key), f"{role}:{identifier}.{key}")
            if role == "logical":
                text(observation.get("audience_first_reading"), f"logical:{identifier}.audience_first_reading")


def check_blind_reading(record, project):
    logical = next(review for review in record["reviews"] if review["role"] == "logical")
    path = artifact(project, logical.get("first_reading"))
    first = json.loads(path.read_text(encoding="utf-8"))
    require(isinstance(first, dict) and first.get("input_scope") == "audience_evidence_only",
            "Blind first reading must use audience evidence before creator intent")
    require(first.get("reviewer_id") == logical["reviewer_id"], "Blind reading reviewer differs from logical reviewer")
    require(first.get("video_sha256") == record["encoded"]["video"]["sha256"], "Blind reading belongs to another encode")
    expected = {f"{identifier}:{phase}": frame["sha256"]
                for identifier, evidence in record["encoded"]["frames"].items() for phase, frame in evidence.items()}
    require(first.get("frame_hashes") == expected, "Blind reading must bind all current encoded samples")
    observations = first.get("observations")
    require(isinstance(observations, dict) and set(observations) == {bridge["id"] for bridge in record["bridges"]},
            "Blind observations required for every story bridge")
    for identifier, observation in observations.items():
        require(isinstance(observation, dict), "Blind observation must be an object")
        for key in ("situation", "visible_event", "new_understanding", "return_to_mechanism"):
            text(observation.get(key), f"blind:{identifier}.{key}")


def check_record(record, project, stage, ffmpeg="ffmpeg", ffprobe="ffprobe", expected_video=None):
    """Verify declared structure/provenance; semantic verdicts remain independent review evidence."""
    try:
        require(stage in STAGES, "Unknown bridge gate stage")
        project = Path(project).resolve()
        bridges = check_plan(record)
        if stage != "plan":
            check_implementation(bridges, project)
        if stage == "encoded":
            encoded = record.get("encoded")
            require(isinstance(encoded, dict), "Actual encoded video and frame evidence required")
            video = artifact(project, encoded.get("video"))
            if expected_video is not None:
                require(video.resolve() == Path(expected_video).resolve(), "Bridge evidence belongs to a different delivery video")
            metadata = probe_video(video, ffprobe)
            require(abs(metadata["fps"] - record["fps"]) < 1e-6
                    and metadata["frame_count"] == record["frame_count"], "Retiming required for actual encoded frame rate/count")
            frames = encoded.get("frames")
            require(isinstance(frames, dict) and set(frames) == {b["id"] for b in bridges},
                    "Encoded samples required for every bridge")
            decoded = {}
            for bridge in bridges:
                evidence = frames[bridge["id"]]
                require(isinstance(evidence, dict) and set(evidence) == set(PHASES), "Every encoded bridge phase is required")
                for phase in PHASES:
                    reference = evidence[phase]
                    image = artifact(project, reference)
                    index = bridge["sample_frames"][phase]
                    require(type(reference.get("frame")) is int and reference["frame"] == index,
                            "Encoded sample frame index differs from plan")
                    if index not in decoded:
                        decoded[index] = hashlib.sha256(encoded_frame(video, index, ffmpeg)).hexdigest()
                    require(file_hash(image) == decoded[index], "Sample does not match the actual encoded video frame")
            check_reviews(record, bridges)
            check_blind_reading(record, project)
        errors = []
    except (OSError, ValueError, KeyError, TypeError, OverflowError, subprocess.SubprocessError) as error:
        errors = [str(error)]
    return {"valid": not errors, "stage": stage, "errors": errors,
            "scope": "Bridge structure, artifact/frame provenance and recorded independent verdicts; not automated story quality or full video QA"}


def check_spec_mapping(record, spec):
    """Connect the local renderer's timeline to the separately verified story record."""
    try:
        fps = spec["profile"]["fps"]
        require(abs(record["fps"] - fps) < 1e-6, "Bridge and local spec frame rates differ")
        frame_at = lambda seconds: math.ceil(seconds * fps - 1e-8)
        require(record["frame_count"] == frame_at(spec["profile"]["duration_seconds"]),
                "Bridge and local spec frame counts differ")
        scenes = spec["scenes"]
        for bridge in record["bridges"]:
            matches = [index for index, scene in enumerate(scenes) if scene["id"] == bridge["scene_id"]]
            require(len(matches) == 1 and 0 < matches[0] < len(scenes) - 1,
                    "Story bridge must be an implemented interior local scene")
            index = matches[0]
            require(scenes[index - 1]["id"] == bridge["incoming_scene_id"]
                    and scenes[index + 1]["id"] == bridge["next_scene_id"], "Bridge neighboring scene IDs differ from local timeline")
            require(frame_at(scenes[index]["start"]) == bridge["start_frame"]
                    and frame_at(scenes[index]["end"]) == bridge["end_frame"], "Bridge window differs from local render timeline")
            require(bridge["sample_frames"]["before"] >= frame_at(scenes[index - 1]["start"])
                    and bridge["sample_frames"]["after"] < frame_at(scenes[index + 1]["end"]),
                    "Bridge samples do not belong to the named neighboring scenes")
        return []
    except (KeyError, ValueError, TypeError, IndexError) as error:
        return [str(error)]


def capture(record, project, video_name, frames_name, output_name, ffmpeg, ffprobe):
    project = Path(project).resolve()
    result = check_record(record, project, "implemented")
    require(result["valid"], "; ".join(result["errors"]))
    video = project_path(project, video_name)
    metadata = probe_video(video, ffprobe)
    require(metadata["frame_count"] == record["frame_count"]
            and abs(metadata["fps"] - record["fps"]) < 1e-6, "Retiming required before capturing encoded bridge evidence")
    destination = project_path(project, frames_name)
    output = project_path(project, output_name)
    require(not destination.exists() and not output.exists(), "Choose new evidence paths; capture never overwrites")
    candidate = copy.deepcopy(record)
    candidate["encoded"] = {"video": {"path": video_name, "sha256": file_hash(video)}, "frames": {}}
    candidate["reviews"] = []
    destination.mkdir(parents=True)
    for bridge in candidate["bridges"]:
        frames = {}
        for phase in PHASES:
            index = bridge["sample_frames"][phase]
            path = destination / f"{bridge['id']}-{phase}.png"
            with path.open("xb") as stream:
                stream.write(encoded_frame(video, index, ffmpeg))
            frames[phase] = {"path": path.relative_to(project).as_posix(), "sha256": file_hash(path), "frame": index}
        candidate["encoded"]["frames"][bridge["id"]] = frames
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("x", encoding="utf-8") as stream:
        json.dump(candidate, stream, indent=2, ensure_ascii=False, allow_nan=False)
        stream.write("\n")
    return {"captured": True, "record": str(output), "candidate_digest": candidate_digest(candidate),
            "pending": ["independent visual bridge review", "fresh-context logical bridge review"],
            "scope": "Encoded samples captured; no story approval or video QA"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    check = sub.add_parser("check")
    check.add_argument("record", type=Path)
    check.add_argument("--stage", choices=STAGES, required=True)
    grab = sub.add_parser("capture")
    grab.add_argument("record", type=Path)
    grab.add_argument("video", help="Project-relative encoded video")
    grab.add_argument("frames", help="New project-relative evidence directory")
    grab.add_argument("--output", required=True, help="New project-relative encoded record")
    for command in (check, grab):
        command.add_argument("--project", type=Path)
        command.add_argument("--ffmpeg", default="ffmpeg")
        command.add_argument("--ffprobe", default="ffprobe")
    args = parser.parse_args()
    try:
        record = json.loads(args.record.read_text(encoding="utf-8"))
        project = args.project or args.record.parent
        if args.command == "check":
            result = check_record(record, project, args.stage, args.ffmpeg, args.ffprobe)
        else:
            result = capture(record, project, args.video, args.frames, args.output, args.ffmpeg, args.ffprobe)
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        result = {"valid": False, "errors": [str(error)]}
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 1 if result.get("valid") is False else 0


if __name__ == "__main__":
    raise SystemExit(main())
