#!/usr/bin/env python3
"""Keep the committed Cursor discovery directory in sync with canonical skills."""

from __future__ import annotations

import argparse
import json
import shutil
from pathlib import Path


IGNORED_DIRS = {"__pycache__", ".pytest_cache", ".mypy_cache", "obj", "bin"}
IGNORED_SUFFIXES = {".pyc", ".pyo"}
GRAPH_RUNTIME_DIRS = {"policies", "state"}
MANIFEST_NAME = "sync-discovery-manifest.json"


def is_link(path: Path) -> bool:
    return path.is_symlink() or path.is_junction()


def files_under(root: Path) -> dict[Path, Path]:
    files: dict[Path, Path] = {}
    for path in root.rglob("*"):
        relative = path.relative_to(root)
        if any(part in IGNORED_DIRS for part in relative.parts):
            continue
        if len(relative.parts) > 1 and relative.parts[0] == "software-engineering-graph" and relative.parts[1] in GRAPH_RUNTIME_DIRS:
            continue
        if is_link(path):
            raise ValueError(f"Discovery trees must not contain links: {path}")
        if path.is_file() and path.suffix not in IGNORED_SUFFIXES:
            files[relative] = path
    return files


def managed_files(manifest: Path) -> set[Path] | None:
    if not manifest.exists():
        return None
    contents = json.loads(manifest.read_text(encoding="utf-8"))
    if not isinstance(contents, dict) or contents.get("version") != 1 or not isinstance(contents.get("files"), list):
        raise ValueError(f"Invalid discovery manifest: {manifest}")
    paths = set()
    for name in contents["files"]:
        if not isinstance(name, str) or not name or "\\" in name or ":" in name:
            raise ValueError(f"Invalid discovery manifest path: {name!r}")
        path = Path(name)
        if path.is_absolute() or any(part in {"", ".", ".."} for part in name.split("/")):
            raise ValueError(f"Invalid discovery manifest path: {name!r}")
        if (len(path.parts) > 1 and path.parts[0].casefold() == "software-engineering-graph"
                and path.parts[1].casefold() in GRAPH_RUNTIME_DIRS):
            raise ValueError(f"Managed discovery path points to graph runtime data: {name!r}")
        paths.add(path)
    if len(paths) != len(contents["files"]):
        raise ValueError(f"Duplicate discovery manifest paths: {manifest}")
    return paths


def remove_managed_file(destination: Path, relative: Path) -> None:
    root = destination.resolve(strict=True)
    target = destination / relative
    # A manifest must never turn deletion into a traversal outside the mirror.
    if not target.resolve(strict=False).is_relative_to(root):
        raise ValueError(f"Managed discovery path escapes mirror: {relative}")
    for component in (destination, *(destination / Path(*relative.parts[:index]) for index in range(1, len(relative.parts) + 1))):
        if is_link(component):
            raise ValueError(f"Managed discovery path contains a link: {relative}")
    if target.is_file():
        target.unlink()
    elif target.exists():
        raise ValueError(f"Managed discovery file became a directory: {relative}")


def prune_empty_directories(destination: Path, removed: set[Path]) -> None:
    parents = {destination / parent for relative in removed for parent in relative.parents if parent != Path(".")}
    for directory in sorted(parents, key=lambda path: len(path.parts), reverse=True):
        if directory.is_dir() and not is_link(directory):
            try:
                directory.rmdir()
            except OSError:
                pass


def sync_discovery(repository_root: Path, check_only: bool) -> list[str]:
    source = repository_root / "skills"
    destination = repository_root / ".cursor" / "skills"
    manifest = repository_root / ".cursor" / MANIFEST_NAME
    if not source.is_dir() or is_link(source):
        raise ValueError(f"Canonical skills must be a real directory: {source}")
    if is_link(repository_root / ".cursor") or is_link(destination) or (destination.exists() and not destination.is_dir()):
        raise ValueError(f"Cursor discovery must be a real directory: {destination}")
    if is_link(manifest):
        raise ValueError(f"Discovery manifest must be a real file: {manifest}")

    source_files = files_under(source)
    destination_files = files_under(destination) if destination.is_dir() else {}
    previous = managed_files(manifest)
    if previous is None:
        # Adopt only files that correspond to current canonical files. Unknown
        # files in an existing mirror might belong to the user.
        previous = source_files.keys() & destination_files.keys()
    if not check_only:
        destination.mkdir(parents=True, exist_ok=True)
        stale = previous - source_files.keys()
        for relative in sorted(stale):
            remove_managed_file(destination, relative)
        prune_empty_directories(destination, stale)
        for relative, path in source_files.items():
            target = destination / relative
            if target.is_dir():
                try:
                    target.rmdir()
                except OSError as error:
                    raise ValueError(f"Discovery destination contains an unrelated directory: {target}") from error
            target.parent.mkdir(parents=True, exist_ok=True)
            if not target.is_file() or target.read_bytes() != path.read_bytes():
                shutil.copy2(path, target)
        manifest.write_text(
            json.dumps({"version": 1, "files": sorted(path.as_posix() for path in source_files)}) + "\n",
            encoding="utf-8",
        )
        destination_files = files_under(destination)

    missing = source_files.keys() - destination_files.keys()
    extra = destination_files.keys() - source_files.keys()
    changed = {
        relative for relative in source_files.keys() & destination_files.keys()
        if source_files[relative].read_bytes() != destination_files[relative].read_bytes()
    }
    return [
        *(f"missing: {path.as_posix()}" for path in sorted(missing)),
        *(f"extra: {path.as_posix()}" for path in sorted(extra)),
        *(f"changed: {path.as_posix()}" for path in sorted(changed)),
    ]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Report drift without copying files")
    parser.add_argument("--repository-root", type=Path, default=Path(__file__).resolve().parents[1])
    arguments = parser.parse_args()
    try:
        issues = sync_discovery(arguments.repository_root, arguments.check)
    except (OSError, ValueError) as error:
        parser.exit(1, f"{error}\n")
    if issues:
        for issue in issues:
            print(issue)
        return 1
    print("Cursor discovery copy matches canonical skills.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
