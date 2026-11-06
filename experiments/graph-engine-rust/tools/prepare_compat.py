"""Verify frozen compatibility bytes without generating or approving inputs."""

import argparse
import hashlib
import json
import os
import stat
import struct
from pathlib import Path

MANIFEST_SHA256 = "e64534513fb200261b1853452e8bf995dc9500660ef1064ff01c5d1b097fb6f8"
INPUT_PATH = "compat/unicode/cpython-3.12.10/unicode-tables.bin"


def regular(path):
    for component in (path, *path.parents):
        info = component.lstat()
        if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
            raise ValueError("COMPAT_LINK_FORBIDDEN")
    before = path.stat()
    if not stat.S_ISREG(before.st_mode):
        raise ValueError("COMPAT_REGULAR_FILE_REQUIRED")
    data = path.read_bytes()
    after = path.stat()
    if (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) != (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns):
        raise ValueError("COMPAT_INPUT_CHANGED")
    return data


def verify(package, artifacts):
    if not artifacts.is_absolute():
        raise ValueError("COMPAT_ARTIFACT_ROOT_ABSOLUTE_REQUIRED")
    raw = regular(package / "compat/unicode-input-manifest.json")
    if hashlib.sha256(raw).hexdigest() != MANIFEST_SHA256:
        raise ValueError("COMPAT_MANIFEST_DIGEST_MISMATCH")
    manifest = json.loads(raw)
    generator = regular(package / "tools/generate_unicode.py")
    if hashlib.sha256(generator).hexdigest() != manifest["generator"]["sha256"]:
        raise ValueError("COMPAT_GENERATOR_DIGEST_MISMATCH")
    repo = package.parent.parent
    for name, expected in manifest["oracle"]["modules"].items():
        if hashlib.sha256(regular(repo / "skills/software-engineering-graph/graph_engine" / name)).hexdigest() != expected:
            raise ValueError("COMPAT_ORACLE_SOURCE_MISMATCH")
    data = regular(artifacts / INPUT_PATH)
    if len(data) != manifest["input"]["size_bytes"]:
        raise ValueError("COMPAT_INPUT_LENGTH_MISMATCH")
    if hashlib.sha256(data).hexdigest() != manifest["input"]["sha256"]:
        raise ValueError("COMPAT_INPUT_DIGEST_MISMATCH")
    if data[:8] != b"GUNICOD1":
        raise ValueError("COMPAT_INPUT_FORMAT_INVALID")
    header_size, = struct.unpack_from("<I", data, 8)
    header = json.loads(data[12:12 + header_size])
    if header != {"codepoints": 0x110000, "format": "graph-unicode", "version": 1, "oracle": manifest["oracle"]}:
        raise ValueError("COMPAT_INPUT_PROVENANCE_MISMATCH")
    offset = 12 + header_size + 0x110000 * 2
    count, = struct.unpack_from("<I", data, offset)
    offset += 4
    previous = -1
    for _ in range(count):
        point, lower, folded = struct.unpack_from("<IBB", data, offset)
        offset += 6
        if not previous < point <= 0x10ffff or not lower or not folded:
            raise ValueError("COMPAT_MAPPING_INVALID")
        previous = point
        points = struct.unpack_from("<" + "I" * (lower + folded), data, offset)
        if any(point > 0x10ffff for point in points):
            raise ValueError("COMPAT_MAPPING_INVALID")
        offset += 4 * (lower + folded)
    if offset != len(data):
        raise ValueError("COMPAT_INPUT_TRAILING_BYTES")
    return {"manifest_sha256": MANIFEST_SHA256, "input_sha256": manifest["input"]["sha256"], "size_bytes": len(data)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["verify"])
    args = parser.parse_args()
    del args
    root = os.environ.get("GRAPH_RUST_RUN_ARTIFACT_ROOT")
    if not root:
        parser.error("GRAPH_RUST_RUN_ARTIFACT_ROOT is required")
    print(json.dumps(verify(Path(__file__).resolve().parent.parent, Path(root)), sort_keys=True))


if __name__ == "__main__":
    main()
