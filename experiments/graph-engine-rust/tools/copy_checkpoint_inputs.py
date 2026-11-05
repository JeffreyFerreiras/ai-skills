"""Archive each frozen input under a unique, verified checkpoint path."""

import argparse
import hashlib
import json
import os
import shutil
from pathlib import Path


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def verify_copies(records, root):
    root = Path(root).resolve(strict=True)
    names = set()
    for record in records:
        relative = Path(record["archive"])
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError("Archive path escapes its checkpoint")
        key = os.path.normcase(str(relative))
        if key in names:
            raise ValueError("Archive path collision")
        names.add(key)
        copied = root / relative
        if not copied.resolve(strict=True).is_relative_to(root):
            raise ValueError("Archive file escapes its checkpoint")
        if sha256(copied) != record["sha256"]:
            raise ValueError("Archived input hash mismatch")
    actual = {
        os.path.normcase(str(path.relative_to(root)))
        for path in root.rglob("*") if path.is_file()
    }
    if actual != names:
        raise ValueError("Archive inventory does not match its input records")


def copy_inputs(records, root):
    root = Path(root)
    root.mkdir(parents=True, exist_ok=False)
    archived = []
    for index, original in enumerate(records):
        source = Path(original["source"])
        expected = original["sha256"]
        if len(expected) != 64 or any(char not in "0123456789abcdef" for char in expected):
            raise ValueError("Invalid frozen input SHA256")
        if sha256(source) != expected:
            raise ValueError("Frozen input hash mismatch")
        relative = Path(f"{index:04d}-{expected[:16]}") / source.name
        destination = root / relative
        destination.parent.mkdir(exist_ok=False)
        # Exclusive creation also prevents overwrites if path construction regresses.
        with source.open("rb") as incoming, destination.open("xb") as outgoing:
            shutil.copyfileobj(incoming, outgoing)
        if sha256(source) != expected:
            raise ValueError("Frozen input changed while copying")
        record = dict(original)
        record["record_index"] = index
        record["archive"] = relative.as_posix()
        archived.append(record)
    # Recheck all copies after all writes, so a later collision cannot hide an overwrite.
    verify_copies(archived, root)
    return archived


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--mapping", type=Path, required=True)
    arguments = parser.parse_args()
    if arguments.mapping.exists():
        raise FileExistsError(arguments.mapping)
    manifest = json.loads(arguments.manifest.read_text(encoding="utf-8-sig"))
    records = copy_inputs(manifest["frozen_inputs"], arguments.output)
    with arguments.mapping.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump({"records": records, "record_count": len(records)}, stream, indent=2)
        stream.write("\n")
    print(json.dumps({"records": len(records), "copies": len(records), "mapping": str(arguments.mapping)}))


if __name__ == "__main__":
    main()
