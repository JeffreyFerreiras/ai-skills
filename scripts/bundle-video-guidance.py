#!/usr/bin/env python3
"""Bundle shared video guidance into independently installable skills."""

from __future__ import annotations

import argparse
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
BUNDLES = {
    "system-design-video": ("scene-direction.md", "story-bridge-contract.md", "scene_bridge.py"),
    "notebooklm-cinematic-video": (
        "scene-direction.md", "notebooklm-workflow.md", "reference-history.md", "story-bridge-contract.md", "scene_bridge.py",
    ),
    "notebooklm-short-video": (
        "scene-direction.md", "notebooklm-workflow.md", "reference-history.md", "story-bridge-contract.md", "scene_bridge.py",
    ),
}
COVER_RESOURCES = ("cover-style.md", "cover_style.py", "cover-references/manifest.json",
                   "cover-references/safe-rollbacks-tiktok-cover.png", "cover-references/logical-clocks-tiktok-cover.png")
STYLE_RESOURCES = ("neon-chalk-systems.md", "cloudflare-cinematic-decomposition.md")
BUNDLES = {
    skill: (*resources, *COVER_RESOURCES, *STYLE_RESOURCES)
    for skill, resources in BUNDLES.items()
}


def bundle(root: Path, *, check: bool = False) -> list[str]:
    """Check without writes, or update only missing/stale declared resources."""
    names = sorted({name for resources in BUNDLES.values() for name in resources})
    sources = {name: (root / "shared/video-production" / name).read_bytes() for name in names}
    problems = []
    for skill, resources in BUNDLES.items():
        for name in resources:
            directory = "assets" if name.startswith("cover-references/") else (
                "scripts" if name.endswith(".py") else "references")
            target = root / "skills" / skill / directory / name
            actual = target.read_bytes() if target.exists() else None
            if actual == sources[name]:
                continue
            if check:
                state = "missing" if actual is None else "stale"
                problems.append(f"{state}: {target.relative_to(root).as_posix()}")
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(sources[name])
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Detect drift without writing")
    arguments = parser.parse_args()
    try:
        problems = bundle(REPOSITORY_ROOT, check=arguments.check)
    except OSError as error:
        parser.exit(1, f"{error}\n")
    if problems:
        parser.exit(1, "\n".join(problems) + "\n")
    print("Video guidance bundles are current." if arguments.check else "Video guidance bundled.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
