"""Sign and verify the tracked skill inventory with OpenSSH."""

from __future__ import annotations

import hashlib
import json
import os
import stat
import subprocess
import tempfile
import zipfile
from pathlib import Path, PureWindowsPath
from typing import TypedDict


RECEIPT_FILE = ".skill-signature.json"
MANIFEST_FILE = ".skill-signatures.json"
SIGNING_ID = "ai-skills"
REPOSITORY = "https://github.com/JeffreyFerreiras/ai-skills.git"


class SignedFile(TypedDict):
    sha256: str
    executable: bool


def json_bytes(value: object) -> bytes:
    return (json.dumps(value, ensure_ascii=True, sort_keys=True, indent=2) + "\n").encode("ascii")


def file_digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def regular_tree(root: Path) -> tuple[dict[str, Path], set[str]]:
    """Reject links and special files before any staged copy of an installation."""
    files = {}
    directories = set()

    def visit(path: Path) -> None:
        info = path.lstat()
        if getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT:
            raise ValueError(f"Reparse point in skill tree: {path}")
        if stat.S_ISDIR(info.st_mode):
            if path != root:
                directories.add(path.relative_to(root).as_posix())
            for child in path.iterdir():
                visit(child)
        elif stat.S_ISREG(info.st_mode) and path != root:
            files[path.relative_to(root).as_posix()] = path
        else:
            raise ValueError(f"Expected a regular skill directory or file: {path}")

    visit(root)
    return files, directories


def archive_skills(master: Path, destination: Path) -> dict[str, dict[str, SignedFile]]:
    """Export HEAD:skills and hash the exact bytes that can be installed."""
    command = ["git", "-C", str(master), "-c", "core.autocrlf=false", "-c", "core.eol=lf"]
    tree = subprocess.check_output([*command, "rev-parse", "HEAD:skills"], timeout=120).strip().decode("ascii")
    entries = subprocess.check_output([*command, "ls-tree", "-r", "-z", tree], timeout=120)
    tracked = {}
    for entry in entries.split(b"\0")[:-1]:
        metadata, name = entry.split(b"\t", 1)
        mode, kind, object_id = metadata.decode("ascii").split()
        if kind != "blob" or mode not in {"100644", "100755"}:
            raise ValueError(f"Unsupported tracked skill entry: {name!r}")
        tracked[name.decode("utf-8")] = (mode, object_id)

    archive = destination.parent / "skills.zip"
    subprocess.run(
        [*command, "archive", "--format=zip", f"--output={archive}", tree],
        check=True,
        timeout=120,
    )
    seen: set[str] = set()
    executable: set[str] = set()
    destination.mkdir()
    with zipfile.ZipFile(archive) as contents:
        for entry in contents.infolist():
            name = entry.filename.removesuffix("/")
            parts = name.split("/")
            if any(
                not part or part in {".", ".."} or part != part.rstrip(" .")
                or any(character in '\\:<>"|?*' or ord(character) < 32 for character in part)
                or PureWindowsPath(part).is_reserved()
                for part in parts
            ):
                raise ValueError(f"Unsupported skill archive path: {name}")
            if name.casefold() in seen or stat.S_ISLNK(entry.external_attr >> 16):
                raise ValueError(f"Duplicate or linked skill archive path: {name}")
            seen.add(name.casefold())
            target = destination.joinpath(*parts)
            if entry.is_dir():
                target.mkdir()
                continue
            if name not in tracked:
                raise ValueError(f"Archive contains a file outside the committed tree: {name}")
            mode, expected_id = tracked.pop(name)
            algorithm = "sha1" if len(expected_id) == 40 else "sha256"
            blob = hashlib.new(algorithm, f"blob {entry.file_size}\0".encode("ascii"))
            with contents.open(entry) as incoming, target.open("xb") as outgoing:
                for chunk in iter(lambda: incoming.read(1024 * 1024), b""):
                    blob.update(chunk)
                    outgoing.write(chunk)
            if blob.hexdigest() != expected_id:
                raise ValueError(f"Git archive transformed tracked skill file: {name}")
            if mode == "100755":
                executable.add(name)
            if os.name != "nt":
                target.chmod(0o755 if mode == "100755" else 0o644)
    if tracked:
        raise ValueError(f"Git archive omitted tracked skill file: {next(iter(tracked))}")

    inventory = {}
    for skill in sorted(destination.iterdir()):
        if not skill.is_dir() or skill.name.startswith(".") or not (skill / "SKILL.md").is_file():
            continue
        files, _ = regular_tree(skill)
        if any(path.name.casefold() == RECEIPT_FILE for path in skill.rglob("*")):
            raise ValueError(f"{skill.name}: {RECEIPT_FILE} is reserved for installed provenance")
        inventory[skill.name] = {
            name: {"sha256": file_digest(path), "executable": f"{skill.name}/{name}" in executable}
            for name, path in files.items()
        }
    return inventory


def sign_skill(name: str, files: dict[str, SignedFile], signing_key: Path) -> dict:
    manifest = {"version": 1, "repository": REPOSITORY, "skill": name, "files": files}
    signature = subprocess.run(
        ["ssh-keygen", "-Y", "sign", "-f", str(signing_key), "-n", SIGNING_ID],
        input=json_bytes(manifest), stdout=subprocess.PIPE, check=True, timeout=120,
    )
    return {"manifest": manifest, "signature": signature.stdout.decode("ascii")}


def verified_files(receipt: object, name: str, allowed_signers: Path) -> dict[str, SignedFile]:
    try:
        if not isinstance(receipt, dict):
            raise ValueError("Expected a signed skill receipt")
        manifest = receipt["manifest"]
        signature = receipt["signature"].encode("ascii")
        inventory = manifest["files"]
        if (
            manifest["version"] != 1 or manifest["repository"] != REPOSITORY or manifest["skill"] != name
            or not isinstance(inventory, dict)
            or any(
                not isinstance(file, dict) or not isinstance(file.get("sha256"), str)
                or not isinstance(file.get("executable"), bool) for file in inventory.values()
            )
        ):
            raise ValueError("Invalid skill name, repository or signed file inventory")
    except (AttributeError, KeyError, TypeError, UnicodeError, ValueError) as error:
        raise ValueError("Invalid skill signature manifest") from error

    with tempfile.TemporaryDirectory(prefix="skill-signature-") as temporary:
        detached = Path(temporary) / "manifest.sig"
        detached.write_bytes(signature)
        verified = subprocess.run(
            ["ssh-keygen", "-Y", "verify", "-f", str(allowed_signers), "-I", SIGNING_ID,
             "-n", SIGNING_ID, "-s", str(detached)],
            input=json_bytes(manifest), capture_output=True, timeout=120,
        )
    if verified.returncode:
        raise ValueError("Skill signature did not verify against the allowed signers")
    return inventory
