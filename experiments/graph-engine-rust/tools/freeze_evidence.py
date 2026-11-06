"""Freeze byte-identical authored source, binary and Unicode input evidence."""

import argparse
import datetime
import hashlib
import json
import os
import re
import shutil
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verified_copy(source, destination):
    before = digest(source)
    shutil.copyfile(source, destination)
    if digest(source) != before or digest(destination) != before:
        raise ValueError("ARTIFACT_CHANGED_DURING_FREEZE")
    return before


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--name",required=True)
    parser.add_argument("--binary",type=Path,required=True)
    parser.add_argument("--report",type=Path,action="append",default=[])
    parser.add_argument("--auxiliary-binary",type=Path,action="append",default=[])
    args=parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,63}",args.name):
        raise ValueError("INVALID_CHECKPOINT_NAME")
    root=Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve(strict=True)
    package=Path(__file__).resolve().parent.parent
    binary=args.binary.resolve(strict=True)
    if not binary.is_relative_to(root):
        raise ValueError("BINARY_OUTSIDE_ART")
    destination=root/"checkpoints"/args.name
    destination.mkdir(parents=True,exist_ok=False)
    records={}
    for path in sorted(package.rglob("*")):
        if not path.is_file() or "__pycache__" in path.parts:
            continue
        if path.suffix not in {".rs",".toml",".py",".json",".sql",".md",".manifest"} and path.name!="Cargo.lock":
            continue
        if path.is_symlink() or not path.resolve(strict=True).is_relative_to(package):
            raise ValueError("SOURCE_LINK_FORBIDDEN")
        relative=path.relative_to(package)
        target=destination/"source"/Path(str(relative)+".txt")
        target.parent.mkdir(parents=True,exist_ok=True)
        before=digest(path)
        shutil.copyfile(path,target)
        if digest(target)!=before or digest(path)!=before:
            raise ValueError("SOURCE_CHANGED_DURING_FREEZE")
        records[relative.as_posix()]=dict(sha256=before,length=target.stat().st_size,snapshot=target.relative_to(destination).as_posix())
    target=destination/"binary"/binary.name
    target.parent.mkdir()
    binary_digest = verified_copy(binary, target)
    auxiliary = []
    for auxiliary_path in args.auxiliary_binary:
        auxiliary_path = auxiliary_path.resolve(strict=True)
        if not auxiliary_path.is_relative_to(root):
            raise ValueError("AUXILIARY_BINARY_OUTSIDE_ART")
        auxiliary_target = target.parent / auxiliary_path.name
        if auxiliary_target.exists():
            raise ValueError("DUPLICATE_BINARY_NAME")
        auxiliary.append(dict(original=str(auxiliary_path), snapshot=auxiliary_target.relative_to(destination).as_posix(), sha256=verified_copy(auxiliary_path, auxiliary_target)))
    unicode_input=root/"compat/unicode/cpython-3.12.10/unicode-tables.bin"
    frozen=destination/"input/unicode-tables.bin"
    frozen.parent.mkdir()
    input_digest = verified_copy(unicode_input, frozen)
    reports=[]
    for report in args.report:
        report=report.resolve(strict=True)
        if not report.is_relative_to(root):
            raise ValueError("REPORT_OUTSIDE_ART")
        reports.append(dict(path=report.relative_to(root).as_posix(),sha256=digest(report)))
    manifest=dict(timestamp_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  scope="Immutable component evidence snapshot; report bindings determine tested source, not this archive alone",
                  source=records,binary=dict(original=str(binary),sha256=binary_digest),auxiliary_binaries=auxiliary,
                  unicode_input=dict(sha256=input_digest,length=frozen.stat().st_size),reports=reports)
    if digest(binary) != binary_digest or digest(unicode_input) != input_digest or any(digest(Path(item["original"])) != item["sha256"] for item in auxiliary):
        raise ValueError("INPUT_OR_BINARY_CHANGED_BEFORE_MANIFEST")
    (destination/"manifest.json").write_text(json.dumps(manifest,indent=2)+"\n",encoding="ascii")
    print(json.dumps(dict(checkpoint=str(destination),source_files=len(records),binary_sha256=manifest["binary"]["sha256"],cargo_lock_sha256=records["Cargo.lock"]["sha256"])))


if __name__=="__main__":main()
