#!/usr/bin/env python3
"""Bundle canonical design graph resources into standalone skills."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
SKILLS = ("clean-code", "clean-code-review")
RESOURCES = (
    Path("scripts/search_review_graph.py"),
    Path("references/review-graph.manifest.json"),
)


def bundle(root: Path, *, check: bool = False) -> list[str]:
    """Return drift in check mode; otherwise replace only missing or stale files."""
    # Read every source before writing, so a missing source cannot partly update bundles.
    sources = {
        resource: (root / "shared/design-graph" / resource).read_bytes()
        for resource in RESOURCES
    }
    problems = []
    for skill in SKILLS:
        for resource, expected in sources.items():
            target = root / "skills" / skill / resource
            actual = target.read_bytes() if target.exists() else None
            if actual == expected:
                continue
            if check:
                state = "missing" if actual is None else "stale"
                problems.append(f"{state}: {target.relative_to(root).as_posix()}")
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(expected)
    return problems


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Detect drift without writing")
    args = parser.parse_args(argv)
    try:
        problems = bundle(REPOSITORY_ROOT, check=args.check)
    except OSError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    if problems:
        print("\n".join(problems), file=sys.stderr)
        return 1
    print("Design graph bundles are current." if args.check else "Design graph bundles generated.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
