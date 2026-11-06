"""Check teardown on a fresh copy; never rerun the source-capture oracle."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import tempfile
from owned_fixture_cleanup import remove_owned_fixture


def manifest(root):
    records = {}
    for path in sorted(root.rglob("*")):
        info = path.lstat()
        if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
            raise ValueError("SEED_REPARSE_POINT")
        if not path.resolve(strict=True).is_relative_to(root):
            raise ValueError("SEED_CONTAINMENT")
        if path.is_file():
            records[path.relative_to(root).as_posix()] = dict(length=info.st_size, sha256=hashlib.sha256(path.read_bytes()).hexdigest())
        if len(records) > 4096:
            raise ValueError("SEED_FILE_LIMIT")
    return records


def main():
    artifact = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve(strict=True)
    fixtures = (artifact / "fixtures").resolve(strict=True)
    source_report = artifact / "parity/source-capture-01/report.json"
    expected = "ef790d3d83d5a60e18db4c2e90444f02abd08116e7b8be9480a7137a5b94f40d"
    if hashlib.sha256(source_report.read_bytes()).hexdigest() != expected:
        raise ValueError("FROZEN_REFERENCE_REPORT_CHANGED")
    seed = (source_report.parent / "seed-repository").resolve(strict=True)
    if not seed.is_relative_to(artifact):
        raise ValueError("SEED_OUTSIDE_ART")
    before = manifest(seed)
    root = Path(tempfile.mkdtemp(prefix="source-capture-cleanup-", dir=fixtures)).resolve(strict=True)
    repository = root / "repository"
    shutil.copytree(seed, repository, symlinks=True)
    if manifest(repository) != before:
        raise ValueError("FRESH_COPY_MISMATCH")
    readonly = [path.relative_to(root).as_posix() for path in repository.rglob("*")
                if path.is_file() and not path.stat().st_mode & stat.S_IWRITE]
    if os.name == "nt" and not readonly:
        raise ValueError("READONLY_GIT_OBJECT_CONDITION_MISSING")
    remove_owned_fixture(root, fixtures)
    if root.exists() or manifest(seed) != before:
        raise ValueError("TEARDOWN_OR_SEED_PRESERVATION_FAILED")
    try:
        remove_owned_fixture(seed, fixtures)
    except ValueError as error:
        if str(error) != "OWNED_FIXTURE_TEARDOWN_CONTAINMENT":
            raise
    else:
        raise AssertionError("immutable seed must not be accepted as a disposable fixture")
    if manifest(seed) != before:
        raise ValueError("SEED_CHANGED_AFTER_NEGATIVE_CHECK")
    output = artifact / "parity/source-capture-cleanup-01"
    output.mkdir(parents=True, exist_ok=False)
    report = dict(scope="fresh-copy teardown only; zero capture_source or Git calls", source_report_sha256=expected,
                  fixture=str(root), fixture_removed=True, seed=str(seed), seed_unchanged=True,
                  seed_files=before, readonly_files=readonly, rejected_immutable_seed=True,
                  harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  cleanup_sha256=hashlib.sha256(Path(__file__).with_name("owned_fixture_cleanup.py").read_bytes()).hexdigest())
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(dict(files=len(before), readonly_files=len(readonly), fixture_removed=True, seed_unchanged=True, source_capture_calls=0)))


if __name__ == "__main__":
    main()
