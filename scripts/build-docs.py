#!/usr/bin/env python3
"""Generate the README catalog; check generated content and local documentation links."""

from __future__ import annotations

import argparse
import re
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit

import yaml


START = "<!-- skill-catalog:start -->"
END = "<!-- skill-catalog:end -->"


def table_text(value: str) -> str:
    return re.sub(r"([\\|\[\]`*_<>])", r"\\\1", " ".join(value.split()))


def catalog(root: Path) -> str:
    rows = ["| Skill | Purpose |", "| --- | --- |"]
    for directory in sorted((root / "skills").iterdir()):
        if not directory.is_dir() or directory.name.startswith("."):
            continue
        try:
            content = (directory / "SKILL.md").read_text(encoding="utf-8")
            frontmatter = re.match(r"\A---\n(.*?)\n---(?:\n|$)", content, re.DOTALL)
            if frontmatter is None:
                raise ValueError("SKILL.md must start with a closed frontmatter block")
            metadata = yaml.safe_load(frontmatter.group(1))
            interface = yaml.safe_load((directory / "agents/openai.yaml").read_text(encoding="utf-8"))["interface"]
            name = metadata["name"]
            summary = interface["short_description"]
            if name != directory.name or not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", name) or not isinstance(summary, str) or not summary.strip():
                raise ValueError("name must match directory and UI summary must be non-empty")
        except (OSError, KeyError, TypeError, yaml.YAMLError, ValueError) as error:
            raise ValueError(f"Cannot catalog {directory.name}: {error}") from error
        rows.append(f"| [{table_text(name)}](skills/{name}/SKILL.md) | {table_text(summary)} |")
    return "\n".join(rows)


def replace_catalog(content: str, generated: str) -> str:
    if content.count(START) != 1 or content.count(END) != 1 or content.index(START) > content.index(END):
        raise ValueError("README must contain one ordered pair of skill-catalog markers")
    before = content.split(START, 1)[0]
    after = content.split(END, 1)[1]
    return f"{before}{START}\n{generated}\n{END}{after}"


def prose(content: str) -> str:
    return re.sub(r"^(`{3,}|~{3,})[^\n]*\n.*?^\1\s*$", "", content, flags=re.MULTILINE | re.DOTALL)


class MediaLinks(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.targets: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        for name, value in attrs:
            if value and name in {"href", "src"}:
                self.targets.append(value)
            elif value and name == "srcset":
                self.targets.extend(candidate.strip().split()[0] for candidate in value.split(",") if candidate.strip())


def anchors(content: str) -> set[str]:
    identifiers: set[str] = set(re.findall(r'(?:id|name)="([^"]+)"', content))
    for heading in re.findall(r"^#{1,6}\s+(.+?)\s*#*\s*$", prose(content), re.MULTILINE):
        heading = re.sub(r"\[([^\]]+)\]\([^)]+\)", r"\1", heading)
        base = re.sub(r"[^\w\- ]", "", heading.lower()).replace(" ", "-")
        identifier = base
        suffix = 0
        while identifier in identifiers:
            suffix += 1
            identifier = f"{base}-{suffix}"
        identifiers.add(identifier)
    return identifiers


def check_links(root: Path, paths: list[Path]) -> list[str]:
    errors = []
    for document in paths:
        content = prose(document.read_text(encoding="utf-8"))
        targets = re.findall(r"\]\(<?([^\s)>]+)>?(?:\s+\"[^\"]*\")?\)", content)
        targets.extend(re.findall(r"^\s*\[[^\]]+\]:\s*<?([^\s>]+)", content, re.MULTILINE))
        media = MediaLinks()
        media.feed(content)
        for target in targets + media.targets:
            parts = urlsplit(target)
            if parts.scheme or parts.netloc:
                continue
            destination = (document.parent / unquote(parts.path)).resolve() if parts.path else document
            if not destination.exists():
                errors.append(f"{document.relative_to(root)}: missing {target}")
            elif parts.fragment and destination.suffix == ".md":
                if unquote(parts.fragment) not in anchors(destination.read_text(encoding="utf-8")):
                    errors.append(f"{document.relative_to(root)}: missing anchor {target}")
    return errors


def documents(root: Path) -> list[Path]:
    return sorted({
        *root.glob("*.md"), *root.glob("docs/**/*.md"),
        *root.glob("skills/**/*.md"), *root.glob("tests/**/*.md"),
    })


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Check without writing files")
    parser.add_argument("--repository-root", type=Path, default=Path(__file__).resolve().parents[1])
    arguments = parser.parse_args()
    root = arguments.repository_root.resolve()
    try:
        readme = root / "README.md"
        current = readme.read_text(encoding="utf-8")
        updated = replace_catalog(current, catalog(root))
        if arguments.check and current != updated:
            print("README catalog is stale. Run: python scripts/build-docs.py")
            return 1
        if not arguments.check and current != updated:
            readme.write_text(updated, encoding="utf-8", newline="\n")
        errors = check_links(root, documents(root))
    except (OSError, ValueError) as error:
        parser.exit(1, f"{error}\n")
    if errors:
        print("\n".join(errors))
        return 1
    print("README catalog is current; local documentation links and image paths resolve.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
