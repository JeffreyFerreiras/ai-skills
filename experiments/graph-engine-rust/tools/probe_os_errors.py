"""Freeze actual selected-Python WinAPI and CRT failure metadata on ART fixtures."""

import argparse
import hashlib
import json
import os
import sys
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    regular = args.output / "regular.txt"
    regular.write_bytes(b"fixture")
    descriptor = os.open(regular, os.O_RDONLY)
    os.close(descriptor)
    records = []
    def observe(name, action):
        try:
            value = action()
            if name.startswith("open"):
                os.close(value)
            records.append(dict(name=name, ok=True))
        except Exception as error:
            records.append(dict(name=name, kind=type(error).__name__, message=str(error), errno=getattr(error, "errno", None), winerror=getattr(error, "winerror", None), filename=getattr(error, "filename", None), filename2=getattr(error, "filename2", None)))
    for index, path in enumerate((args.output / "missing.txt", regular / "child", args.output / "apostrophe's-é.txt", args.output / 'double"quote.txt')):
        observe("lstat-" + str(index), lambda path=path: os.lstat(path))
        observe("open-" + str(index), lambda path=path: os.open(path, os.O_RDONLY))
    observe("open-directory", lambda: os.open(args.output, os.O_RDONLY))
    observe("read-closed", lambda: os.read(descriptor, 1))
    observe("fsync-closed", lambda: os.fsync(descriptor))
    observe("fstat-closed", lambda: os.fstat(descriptor))
    observe("replace-missing", lambda: os.replace(args.output / "missing-source", args.output / "missing-target"))
    report = dict(scope="isolated selected-Python OS error metadata, generated ART only", executable_sha256=hashlib.sha256(Path(sys.executable).read_bytes()).hexdigest(), python=sys.version, records=records)
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="ascii")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
