"""Observe lstat modes on the explicitly authorized generated file corpus."""

import argparse
import hashlib
import json
import os
import pathlib
import shutil
import stat
import sys
import tempfile


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact-root", type=pathlib.Path, required=True)
    args = parser.parse_args()
    artifact = args.artifact_root.resolve(strict=True)
    fixtures = (artifact / "fixtures").resolve(strict=True)
    root = pathlib.Path(tempfile.mkdtemp(prefix="source-stat-", dir=fixtures)).resolve(strict=True)
    if not root.is_relative_to(fixtures) or root == fixtures:
        raise RuntimeError("owned source-stat fixture containment failed")
    names = ["ordinary.txt", "noextension", "probe.exe", "probe.EXE", "probe.bat", "probe.cmd",
             "probe.com", "probe.ps1", "probe.sh", "readonly.txt", "readonly.exe", "directory", "missing"]
    if len(names) > 16:
        raise RuntimeError("source-stat observation bound exceeded before calls")
    data = b"source-stat\x00\xff\r\n"
    readonly = [root / "readonly.txt", root / "readonly.exe"]
    records = []
    try:
        for name in names:
            path = root / name
            if name == "directory":
                path.mkdir()
            elif name != "missing":
                path.write_bytes(data)
        for path in readonly:
            path.chmod(stat.S_IREAD)
        for name in names:
            path = root / name
            record = {"name": name, "path": str(path), "readonly": name.startswith("readonly"),
                      "input_hex": None if name in {"directory", "missing"} else data.hex(),
                      "input_sha256": None if name in {"directory", "missing"} else hashlib.sha256(data).hexdigest()}
            try:
                metadata = os.lstat(path)
                record["expected"] = {"kind": "stat", "st_mode": metadata.st_mode,
                    "mode": stat.S_IMODE(metadata.st_mode), "regular": stat.S_ISREG(metadata.st_mode),
                    "symlink": stat.S_ISLNK(metadata.st_mode), "size": metadata.st_size}
            except OSError as error:
                record["expected"] = {"kind": "exception", "class": type(error).__name__,
                    "message": str(error), "errno": error.errno, "winerror": getattr(error, "winerror", None)}
            records.append(record)
        output = artifact / "parity" / "source-stat-01"
        output.mkdir(parents=True, exist_ok=False)
        report = {"observations": len(records), "limit": 16, "root": str(root), "python_version": sys.version,
            "bindings": {name: {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
                for name, path in {"python": pathlib.Path(sys.executable), "generator": pathlib.Path(__file__)}.items()},
            "records": records}
        encoded = (json.dumps(report, ensure_ascii=True, indent=2) + "\n").encode("ascii")
        (output / "report.json").write_bytes(encoded)
        print(json.dumps({"observations": len(records), "sha256": hashlib.sha256(encoded).hexdigest(), "report": str(output / "report.json")}))
    finally:
        for path in readonly:
            if path.exists():
                path.chmod(stat.S_IREAD | stat.S_IWRITE)
        if root.parent != fixtures or not root.is_relative_to(fixtures):
            raise RuntimeError("owned source-stat teardown containment failed")
        shutil.rmtree(root)


if __name__ == "__main__":
    main()
