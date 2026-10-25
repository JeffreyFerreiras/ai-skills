#!/usr/bin/env python3
"""Bind cover-style reviews to bundled references and the actual thumbnail."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from pathlib import Path

from scene_bridge import artifact, candidate_digest, file_hash, project_path, require, text


CHECKS = ("rough_chalk_lettering", "one_dominant_illustration", "short_hook",
          "minimal_labels", "thumbnail_readable", "accessible_contrast", "not_lesson_slide")
THUMBNAIL_WIDTH = 240


def reference_assets(directory):
    manifest = json.loads((directory / "manifest.json").read_text(encoding="utf-8"))
    require(manifest.get("schema_version") == 1, "Unsupported cover reference manifest")
    references = manifest.get("references")
    require(isinstance(references, list) and len(references) == 2, "Two original cover references required")
    result = {}
    for reference in references:
        identifier = text(reference.get("id"), "reference.id")
        require(identifier not in result, "Duplicate cover reference")
        path = artifact(directory.resolve(), reference)
        require(path.stat().st_size == reference.get("size_bytes"), "Reference byte size changed")
        result[identifier] = reference
    return result


def thumbnail_bytes(cover, ffmpeg):
    result = subprocess.run([ffmpeg, "-v", "error", "-nostdin", "-i", str(cover),
        "-vf", f"scale={THUMBNAIL_WIDTH}:-1", "-frames:v", "1", "-c:v", "png",
        "-threads", "1", "-f", "image2pipe", "-"], capture_output=True, check=True, timeout=120)
    require(result.stdout.startswith(b"\x89PNG\r\n\x1a\n"), "Thumbnail decoder produced no PNG")
    return result.stdout


def check(record, project, references, ffmpeg="ffmpeg"):
    require(isinstance(record, dict), "Cover record must be an object")
    require(type(record.get("schema_version")) is int and record["schema_version"] == 1,
            "Expected cover-style record schema_version 1")
    text(record.get("episode_id"), "episode_id")
    creator = text(record.get("creator_id"), "creator_id")
    cover = artifact(project, record.get("cover"))
    thumbnail = artifact(project, record.get("thumbnail"))
    require(thumbnail.read_bytes() == thumbnail_bytes(cover, ffmpeg),
            "Thumbnail is not the current cover at the required width")
    assets = reference_assets(references)
    inspections = record.get("reference_inspections")
    require(isinstance(inspections, list) and len(inspections) == len(assets),
            "Producer must inspect both original reference images")
    inspected = set()
    for inspection in inspections:
        require(isinstance(inspection, dict), "Reference inspection must be an object")
        identifier = inspection.get("reference_id")
        require(identifier in assets and identifier not in inspected, "Unknown or repeated reference inspection")
        inspected.add(identifier)
        require(inspection.get("sha256") == assets[identifier]["sha256"], "Reference inspection is stale")
        require(inspection.get("viewer_id") == creator and inspection.get("pixels_viewed") is True,
                "Producer reference-pixel inspection required")
        text(inspection.get("observations"), "reference observations")
    reviews = record.get("reviews")
    require(isinstance(reviews, list) and len(reviews) == 1, "One current independent cover visual verdict required")
    review = reviews[0]
    require(isinstance(review, dict), "Cover review must be an object")
    require(review.get("role") == "cover_visual" and review.get("decision") == "APPROVE",
            "Cover visual review has not approved")
    require(text(review.get("reviewer_id"), "reviewer_id") != creator, "Creator cannot approve own cover")
    require(review.get("candidate_digest") == candidate_digest(record), "Cover review is stale")
    expected = {"cover": record["cover"]["sha256"], "thumbnail": record["thumbnail"]["sha256"]}
    require(review.get("inspected_images") == expected, "Review must inspect actual full cover and thumbnail")
    require(review.get("reference_hashes") == {key: value["sha256"] for key, value in assets.items()},
            "Review must compare both bundled references")
    observations = review.get("observations")
    require(isinstance(observations, dict), "Concrete cover observations required")
    for name in CHECKS:
        require(observations.get(name) is True, f"Cover fails or lacks {name}")
    text(observations.get("observed_hook"), "observed_hook")
    text(observations.get("observed_illustration"), "observed_illustration")


def capture(cover_name, thumbnail_name, output_name, project, episode, creator, ffmpeg="ffmpeg"):
    cover = project_path(project, cover_name)
    require(cover.is_file(), "Cover must exist")
    thumbnail = project_path(project, thumbnail_name)
    output = project_path(project, output_name)
    require(thumbnail != output and not thumbnail.exists() and not output.exists(),
            "Use new thumbnail and record paths; capture never overwrites")
    pixels = thumbnail_bytes(cover, ffmpeg)
    record = {"schema_version": 1, "episode_id": text(episode, "episode_id"),
              "creator_id": text(creator, "creator_id"),
              "cover": {"path": cover_name, "sha256": file_hash(cover)},
              "thumbnail": {"path": thumbnail_name, "sha256": hashlib.sha256(pixels).hexdigest()},
              "reference_inspections": [], "reviews": []}
    thumbnail.parent.mkdir(parents=True, exist_ok=True)
    with thumbnail.open("xb") as stream:
        stream.write(pixels)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("x", encoding="utf-8") as stream:
        json.dump(record, stream, indent=2, ensure_ascii=False)
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    capture_parser = commands.add_parser("capture", help="Create thumbnail and unapproved evidence record")
    capture_parser.add_argument("cover")
    capture_parser.add_argument("--thumbnail", required=True)
    capture_parser.add_argument("--output", required=True)
    capture_parser.add_argument("--episode", required=True)
    capture_parser.add_argument("--creator", required=True)
    check_parser = commands.add_parser("check", help="Verify current evidence and recorded independent verdict")
    check_parser.add_argument("record")
    for command in (capture_parser, check_parser):
        command.add_argument("--project", type=Path, default=Path.cwd())
        command.add_argument("--ffmpeg", default="ffmpeg")
    arguments = parser.parse_args()
    project = arguments.project.resolve()
    try:
        if arguments.command == "capture":
            capture(arguments.cover, arguments.thumbnail, arguments.output, project,
                    arguments.episode, arguments.creator, arguments.ffmpeg)
            print("Cover evidence captured; producer inspections and independent review remain pending.")
        else:
            record = json.loads(project_path(project, arguments.record).read_text(encoding="utf-8"))
            check(record, project, Path(__file__).resolve().parents[1] / "assets/cover-references", arguments.ffmpeg)
            print("Current cover evidence and recorded independent verdict verified; semantic judgment remains human review.")
    except (ValueError, OSError, KeyError, TypeError, subprocess.SubprocessError) as error:
        parser.exit(1, f"{error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
