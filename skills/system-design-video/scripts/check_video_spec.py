#!/usr/bin/env python3
"""Check a system-design video plan and, optionally, measured narration clips."""

from __future__ import annotations

import argparse
import json
import math
import sys
import wave
from pathlib import Path
from typing import Any


ROUNDING_TOLERANCE = 1e-6
AUDIO_TOLERANCE = 0.1


class SpecChecker:
    def __init__(self, project: Path, ready: bool) -> None:
        self.project = project.resolve()
        self.ready = ready
        self.errors: list[str] = []

    def error(self, where: str, message: str) -> None:
        self.errors.append(f"{where}: {message}")

    def text(self, value: Any, where: str) -> str | None:
        if not isinstance(value, str) or not value.strip():
            self.error(where, "must be a nonempty string")
            return None
        return value

    def number(self, value: Any, where: str, positive: bool = False) -> float | None:
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            self.error(where, "must be a finite number")
            return None
        try:
            number = float(value)
        except OverflowError:
            self.error(where, "must be a finite number")
            return None
        if not math.isfinite(number) or number < 0 or (positive and number == 0):
            self.error(where, "must be finite and positive" if positive else "must be finite and nonnegative")
            return None
        return number

    def records(self, doc: dict[str, Any], key: str, required: bool = True) -> dict[str, dict[str, Any]]:
        values = doc.get(key)
        if not isinstance(values, list):
            self.error(key, "must be an array")
            return {}
        if required and not values:
            self.error(key, "must not be empty")
        records: dict[str, dict[str, Any]] = {}
        for index, item in enumerate(values):
            where = f"{key}[{index}]"
            if not isinstance(item, dict):
                self.error(where, "must be an object")
                continue
            identifier = self.text(item.get("id"), f"{where}.id")
            if identifier is None:
                continue
            if identifier in records:
                self.error(where, f"duplicate id {identifier!r}")
            else:
                records[identifier] = item
        return records

    def refs(self, value: Any, targets: dict[str, Any], where: str, required: bool = True) -> list[str]:
        if not isinstance(value, list):
            self.error(where, "must be an array of IDs")
            return []
        if required and not value:
            self.error(where, "must not be empty")
        valid: list[str] = []
        for identifier in value:
            if not isinstance(identifier, str) or identifier not in targets:
                self.error(where, f"unknown reference {identifier!r}")
            else:
                valid.append(identifier)
        if len(valid) != len(set(valid)):
            self.error(where, "contains duplicate references")
        return valid

    def audio(self, narration: dict[str, Any], where: str) -> float | None:
        name = self.text(narration.get("file"), f"{where}.file")
        duration = self.number(narration.get("duration_seconds"), f"{where}.duration_seconds", positive=True)
        if name is None:
            return
        try:
            path = (self.project / name).resolve()
        except (OSError, ValueError, RuntimeError) as error:
            self.error(f"{where}.file", f"invalid audio path: {error}")
            return None
        if Path(name).is_absolute() or not path.is_relative_to(self.project):
            self.error(f"{where}.file", "must stay inside the spec's project directory")
            return
        if not self.ready:
            return
        try:
            with wave.open(str(path), "rb") as clip:
                measured = clip.getnframes() / clip.getframerate()
        except (OSError, EOFError, wave.Error, ZeroDivisionError) as error:
            self.error(f"{where}.file", f"cannot measure PCM WAV: {error}")
            return
        if measured <= 0:
            self.error(f"{where}.file", "audio clip is empty")
        if duration is not None and abs(measured - duration) > AUDIO_TOLERANCE + ROUNDING_TOLERANCE:
            self.error(where, f"declares {duration:g}s but WAV measures {measured:g}s")
        return measured

    def narration(self, scene: dict[str, Any], where: str, length: float | None) -> None:
        narration = scene.get("narration")
        where = f"{where}.narration"
        if not isinstance(narration, dict):
            self.error(where, "narrated scenes require an object")
            return
        self.text(narration.get("text"), f"{where}.text")
        offset = self.number(narration.get("offset"), f"{where}.offset")
        basis = narration.get("timing_basis")
        if basis not in ("planned", "measured"):
            self.error(f"{where}.timing_basis", "must be planned or measured")
        if self.ready and basis != "measured":
            self.error(where, "animation inputs require measured narration")
        measured = None
        if basis == "measured":
            measured = self.audio(narration, where)
        else:
            for key in ("file", "duration_seconds"):
                if key not in narration:
                    self.error(where, f"missing {key}; use null until generated")
        duration = narration.get("duration_seconds")
        if duration is not None:
            duration = self.number(duration, f"{where}.duration_seconds", positive=True)
        if length is not None and offset is not None:
            if offset >= length:
                self.error(where, "offset must be inside the scene")
            if duration is not None and offset + duration > length + ROUNDING_TOLERANCE:
                self.error(where, "narration extends beyond the scene")
            if measured is not None and offset + measured > length + ROUNDING_TOLERANCE:
                self.error(where, "measured WAV extends beyond the scene")

    def check(self, doc: Any) -> list[str]:
        if not isinstance(doc, dict):
            self.error("spec", "must be an object")
            return self.errors
        if type(doc.get("schema_version")) is not int or doc["schema_version"] != 1:
            self.error("schema_version", "must be 1")
        self.text(doc.get("question"), "question")
        self.text(doc.get("audience"), "audience")
        profile = doc.get("profile")
        if not isinstance(profile, dict):
            self.error("profile", "must be an object")
            profile = {}
        if profile.get("renderer") not in ("remotion", "hyperframes", "manim"):
            self.error("profile.renderer", "must be remotion, hyperframes, or manim")
        for key in ("width", "height"):
            value = profile.get(key)
            if type(value) is not int or value <= 0 or value % 2:
                self.error(f"profile.{key}", "must be a positive even integer")
        self.number(profile.get("fps"), "profile.fps", positive=True)
        duration = self.number(profile.get("duration_seconds"), "profile.duration_seconds", positive=True)
        narrated = profile.get("narrated")
        if type(narrated) is not bool:
            self.error("profile.narrated", "must be a boolean")

        sources = self.records(doc, "sources", required=False)
        claims = self.records(doc, "claims")
        nodes = self.records(doc, "nodes")
        edges = self.records(doc, "edges", required=False)
        scenes = self.records(doc, "scenes")
        events = self.records(doc, "events")
        for identifier, source in sources.items():
            for key in ("locator", "supports"):
                self.text(source.get(key), f"sources.{identifier}.{key}")
        for identifier, claim in claims.items():
            where = f"claims.{identifier}"
            self.text(claim.get("text"), f"{where}.text")
            self.text(claim.get("rationale"), f"{where}.rationale")
            kind = claim.get("kind")
            if kind not in ("supported", "inference", "assumption", "illustrative"):
                self.error(f"{where}.kind", "unknown claim kind")
            self.refs(claim.get("source_ids"), sources, f"{where}.source_ids", required=kind in ("supported", "inference"))
        for identifier, node in nodes.items():
            where = f"nodes.{identifier}"
            for key in ("label", "responsibility"):
                self.text(node.get(key), f"{where}.{key}")
            self.refs(node.get("claim_ids"), claims, f"{where}.claim_ids")
        for identifier, edge in edges.items():
            where = f"edges.{identifier}"
            self.text(edge.get("label"), f"{where}.label")
            for key in ("from", "to"):
                self.refs([edge.get(key)], nodes, f"{where}.{key}")
            self.refs(edge.get("claim_ids"), claims, f"{where}.claim_ids")

        windows: dict[str, tuple[float, float]] = {}
        previous_end = 0.0
        for identifier, scene in scenes.items():
            where = f"scenes.{identifier}"
            start = self.number(scene.get("start"), f"{where}.start")
            end = self.number(scene.get("end"), f"{where}.end", positive=True)
            self.text(scene.get("caption"), f"{where}.caption")
            self.refs(scene.get("claim_ids"), claims, f"{where}.claim_ids")
            length = None
            if start is not None and end is not None:
                if end <= start:
                    self.error(where, "end must be after start")
                else:
                    windows[identifier] = (start, end)
                    length = end - start
                if abs(start - previous_end) > ROUNDING_TOLERANCE:
                    self.error(where, "scene windows must be contiguous and start at zero")
                previous_end = end
            if narrated is True:
                self.narration(scene, where, length)
        if duration is not None and abs(previous_end - duration) > ROUNDING_TOLERANCE:
            self.error("scenes", "last scene must end at profile.duration_seconds")

        prior_events: dict[str, dict[str, Any]] = {}
        previous_time = 0.0
        for identifier, event in events.items():
            where = f"events.{identifier}"
            at = self.number(event.get("at"), f"{where}.at")
            self.text(event.get("action"), f"{where}.action")
            self.refs([event.get("scene_id")], scenes, f"{where}.scene_id")
            participants = self.refs(event.get("node_ids"), nodes, f"{where}.node_ids")
            self.refs(event.get("claim_ids"), claims, f"{where}.claim_ids")
            self.refs(event.get("after"), prior_events, f"{where}.after", required=False)
            edge_id = event.get("edge_id")
            if edge_id is not None:
                matches = self.refs([edge_id], edges, f"{where}.edge_id")
                if matches:
                    edge = edges[matches[0]]
                    if any(endpoint not in participants for endpoint in (edge.get("from"), edge.get("to"))):
                        self.error(where, "packet participants must include both edge endpoints")
            scene_id = event.get("scene_id")
            window = windows.get(scene_id) if isinstance(scene_id, str) else None
            if at is not None:
                if at < previous_time:
                    self.error(where, "events must be chronological")
                if window and not window[0] <= at < window[1]:
                    self.error(where, "event must occur inside its scene window")
                previous_time = at
            prior_events[identifier] = event
        return self.errors


def validate_spec(doc: Any, project: Path, ready: bool = False) -> list[str]:
    return SpecChecker(project, ready).check(doc)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("spec", type=Path, help="Path to video-spec.json")
    parser.add_argument("--ready", action="store_true", help="Verify measured PCM WAV clips before animation")
    parser.add_argument("--json", action="store_true", help="Emit a structured read-only result")
    args = parser.parse_args()
    try:
        doc = json.loads(args.spec.read_text(encoding="utf-8"))
        errors = validate_spec(doc, args.spec.parent, args.ready)
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        errors = [f"spec: {error}"]
    result = {"valid": not errors, "stage": "animation-inputs" if args.ready else "plan", "errors": errors}
    if args.json:
        print(json.dumps(result, indent=2))
    elif errors:
        print("\n".join(errors), file=sys.stderr)
    else:
        print(f"Valid {result['stage']} contract; architecture and final-video review remain separate.")
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
